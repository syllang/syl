//! CUDA C++ prototype generation for word-level simulation designs.
//!
//! The generated kernel keeps state in a structure-of-arrays layout and runs
//! all requested cycles on the device.  It is intentionally self contained so
//! ordinary Rust builds and tests do not require CUDA.  A caller can compile
//! the returned source with [`compile_cuda`] when `nvcc` is available.

use crate::{HwBinaryOp, HwUnaryOp, Selector, SimDesign, SimExpr, SimTarget};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Selects the parallel mapping used by the generated kernel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KernelMode {
    /// One CUDA thread executes one independent test instance.
    BatchPerThread,
    /// Threads in a block execute members of an instance family.
    InstanceFamilyParallel,
}

/// Options controlling CUDA source generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CodegenOptions {
    pub mode: KernelMode,
    pub family_index: Option<usize>,
}

impl Default for CodegenOptions {
    fn default() -> Self {
        Self {
            mode: KernelMode::BatchPerThread,
            family_index: None,
        }
    }
}

/// Generated source and the resource counts needed by a launcher.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedCuda {
    pub source: String,
    pub signal_slots: usize,
    pub memory_words: usize,
    pub family_size: usize,
    pub mode: KernelMode,
}

/// Result of probing the local CUDA compiler.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CudaAvailability {
    Available,
    Unavailable,
}

/// Return whether `nvcc` is present and can report its version.
pub fn cuda_availability() -> CudaAvailability {
    match Command::new("nvcc").arg("--version").output() {
        Ok(output) if output.status.success() => CudaAvailability::Available,
        _ => CudaAvailability::Unavailable,
    }
}

/// Compile generated CUDA source into an object file with the RTX 2080 Ti
/// architecture selected.  The source is written to a sibling `.cu` file.
pub fn compile_cuda(source: &str, output: impl AsRef<Path>) -> Result<PathBuf, String> {
    let output = output.as_ref();
    let source_path = output.with_extension("cu");
    std::fs::write(&source_path, source)
        .map_err(|e| format!("write {}: {e}", source_path.display()))?;
    let result = Command::new("nvcc")
        .args(["-O3", "-arch=sm_75", "-c"])
        .arg(&source_path)
        .arg("-o")
        .arg(output)
        .output()
        .map_err(|e| format!("run nvcc: {e}"))?;
    if !result.status.success() {
        return Err(format!(
            "nvcc failed with {}\n{}",
            result.status,
            String::from_utf8_lossy(&result.stderr)
        ));
    }
    Ok(output.to_path_buf())
}

/// Generate a compilable persistent CUDA kernel for a concrete simulation
/// design.  All signal and memory accesses use SoA indexing.
pub fn generate_cuda(design: &SimDesign, options: CodegenOptions) -> GeneratedCuda {
    let family_size = options
        .family_index
        .and_then(|i| design.families.get(i))
        .map_or(1, |family| family.members.len().max(1));
    let memory_words = design
        .memories
        .iter()
        .map(|memory| memory.depth as usize)
        .sum();
    let mut out = String::new();
    out.push_str("#include <cuda_runtime.h>\n#include <stdint.h>\n#include <stddef.h>\n\n");
    out.push_str("struct SylGpuSummary { unsigned long long cycles; unsigned int fail_batch; unsigned int fail_cycle; unsigned int fail_assert; };\n");
    out.push_str("__device__ __forceinline__ unsigned long long syl_mask(unsigned int width) { return width >= 64 ? ~0ull : (1ull << width) - 1ull; }\n");
    let mode_name = match options.mode {
        KernelMode::BatchPerThread => "batch-per-thread",
        KernelMode::InstanceFamilyParallel => "instance-family-parallel",
    };
    writeln!(
        out,
        "// Syl CUDA prototype, mode={mode_name}, family_size={family_size}"
    )
    .unwrap();
    writeln!(out, "// state layout: state[slot_id * total_threads + tid]").unwrap();
    writeln!(
        out,
        "// memory layout: memory[word_id * total_threads + tid]"
    )
    .unwrap();
    out.push_str("extern \"C\" __global__ void syl_sim_kernel(unsigned long long* state, unsigned long long* memory, SylGpuSummary* summary, unsigned long long cycles, unsigned int total_threads) {\n");
    out.push_str("  const unsigned int tid = blockIdx.x * blockDim.x + threadIdx.x;\n  if (tid >= total_threads) return;\n  unsigned long long rng = 0x9e3779b97f4a7c15ull ^ (unsigned long long)tid;\n");
    out.push_str("  if (tid == 0) { summary->cycles = 0; summary->fail_batch = 0xffffffffu; summary->fail_cycle = 0xffffffffu; summary->fail_assert = 0xffffffffu; }\n  __syncthreads();\n");
    if options.mode == KernelMode::InstanceFamilyParallel {
        writeln!(
            out,
            "  const unsigned int family_lane = tid % {family_size}u;"
        )
        .unwrap();
        out.push_str("  (void)family_lane;\n");
    }
    for (id, signal) in design.signals.iter().enumerate() {
        writeln!(out, "  state[{}u * total_threads + tid] = 0ull;", id).unwrap();
        let _ = signal;
    }
    for memory in &design.memories {
        let _ = memory;
    }
    out.push_str("  for (unsigned long long cycle = 0; cycle < cycles; ++cycle) {\n");
    out.push_str("    rng += 0x9e3779b97f4a7c15ull;\n    rng = (rng ^ (rng >> 30)) * 0xbf58476d1ce4e5b9ull;\n    rng = (rng ^ (rng >> 27)) * 0x94d049bb133111ebull;\n    rng ^= rng >> 31;\n");
    let memory_bases = memory_bases(design);
    for port in design
        .top_ports()
        .iter()
        .filter(|port| port.direction != crate::HwDirection::Out)
    {
        writeln!(
            out,
            "    state[{}u * total_threads + tid] = rng & {};",
            port.signal.0,
            mask(design.signal(port.signal).width)
        )
        .unwrap();
    }
    for comb_id in &design.comb_eval_order {
        let assign = &design.comb[comb_id.0];
        let expression = expression(design, &assign.value, &memory_bases);
        let target = target_assignment(design, &assign.target, &expression);
        writeln!(out, "    {target}").unwrap();
    }
    for (index, register) in design.regs.iter().enumerate() {
        let expression = expression(design, &register.next, &memory_bases);
        writeln!(
            out,
            "    unsigned long long next_reg_{index} = ({expression}) & {};",
            mask(register.width)
        )
        .unwrap();
    }
    for (memory_index, memory) in design.memories.iter().enumerate() {
        for read in &memory.sync_reads {
            let enable = expression(design, &read.enable, &memory_bases);
            let address = expression(design, &read.address, &memory_bases);
            let target = target_assignment(
                design,
                &read.target,
                &format!(
                    "memory[({}u + ({address})) * total_threads + tid]",
                    memory_bases[memory_index]
                ),
            );
            writeln!(out, "    if ({enable}) {target}").unwrap();
        }
        for write in &memory.writes {
            let enable = expression(design, &write.enable, &memory_bases);
            let address = expression(design, &write.address, &memory_bases);
            let data = expression(design, &write.data, &memory_bases);
            writeln!(
                out,
                "    if ({enable}) memory[({}u + ({address})) * total_threads + tid] = ({data});",
                memory_bases[memory_index]
            )
            .unwrap();
        }
    }
    for (index, register) in design.regs.iter().enumerate() {
        writeln!(
            out,
            "    state[{}u * total_threads + tid] = next_reg_{index};",
            register.signal.0
        )
        .unwrap();
    }
    for (index, assertion) in design.assertions.iter().enumerate() {
        let condition = expression(design, &assertion.condition, &memory_bases);
        writeln!(out, "    if (!({condition})) atomicCAS(&summary->fail_assert, 0xffffffffu, {index}u), atomicCAS(&summary->fail_batch, 0xffffffffu, tid), atomicCAS(&summary->fail_cycle, 0xffffffffu, (unsigned int)cycle);").unwrap();
    }
    if options.mode == KernelMode::InstanceFamilyParallel {
        out.push_str("    __syncthreads();\n");
    }
    out.push_str("  }\n  if (tid == 0) summary->cycles = cycles;\n}\n");
    GeneratedCuda {
        source: out,
        signal_slots: design.signals.len(),
        memory_words,
        family_size,
        mode: options.mode,
    }
}

fn memory_bases(design: &SimDesign) -> Vec<usize> {
    let mut base = 0;
    design
        .memories
        .iter()
        .map(|memory| {
            let old = base;
            base += memory.depth as usize;
            old
        })
        .collect()
}

fn mask(width: u32) -> String {
    if width >= 64 {
        "~0ull".into()
    } else {
        format!("0x{:x}ull", (1u64 << width.min(63)) - 1)
    }
}

fn target_assignment(design: &SimDesign, target: &SimTarget, value: &str) -> String {
    let slot = format!("state[{}u * total_threads + tid]", target.signal.0);
    if target.selectors.is_empty() {
        return format!(
            "{slot} = ({value}) & {} ;",
            mask(design.signal(target.signal).width)
        );
    }
    let mut low = 0u32;
    let mut width = design.signal(target.signal).width;
    for selector in &target.selectors {
        match selector {
            Selector::Slice { low: part_low, .. } => {
                low = low.saturating_add(*part_low);
                width = selector.width();
            }
            Selector::Index {
                width: part_width, ..
            } => {
                width = *part_width;
            }
        }
    }
    let field_mask = mask(width);
    format!(
        "{slot} = ({slot} & ~({field_mask}ull << {low})) | ((({value}) & {field_mask}) << {low});"
    )
}

fn expression(design: &SimDesign, expr: &SimExpr, memory_bases: &[usize]) -> String {
    match expr {
        SimExpr::Read(target) => read_target(design, target),
        SimExpr::Int(value) => format!("{}ull", value),
        SimExpr::Bool(value) => u8::from(*value).to_string(),
        SimExpr::HighZ | SimExpr::Zero => "0ull".into(),
        SimExpr::Unary { op, expr } => {
            let value = expression(design, expr, memory_bases);
            match op {
                HwUnaryOp::Neg => format!("(0ull - ({value}))"),
                HwUnaryOp::Not => format!("(!({value}))"),
                _ => value,
            }
        }
        SimExpr::Binary { op, left, right } => {
            let left = expression(design, left, memory_bases);
            let right = expression(design, right, memory_bases);
            let operator = match op {
                HwBinaryOp::AndAnd => "&&",
                HwBinaryOp::OrOr => "||",
                HwBinaryOp::Eq => "==",
                HwBinaryOp::NotEq => "!=",
                HwBinaryOp::Lt => "<",
                HwBinaryOp::LtEq => "<=",
                HwBinaryOp::Gt => ">",
                HwBinaryOp::GtEq => ">=",
                HwBinaryOp::Add => "+",
                HwBinaryOp::Sub => "-",
                HwBinaryOp::Mul => "*",
                HwBinaryOp::Div => "/",
                HwBinaryOp::Rem => "%",
                HwBinaryOp::Shl => "<<",
                HwBinaryOp::BitAnd => "&",
                HwBinaryOp::BitOr => "|",
                HwBinaryOp::BitXor => "^",
                _ => "&",
            };
            format!("(({left}) {operator} ({right}))")
        }
        SimExpr::Mux {
            cond,
            then_value,
            else_value,
        } => format!(
            "(({cond} ) ? ({then_value}) : ({else_value}))",
            cond = expression(design, cond, memory_bases),
            then_value = expression(design, then_value, memory_bases),
            else_value = expression(design, else_value, memory_bases)
        ),
        SimExpr::Select { arms, default, .. } => {
            let mut value = expression(design, default, memory_bases);
            for (guard, arm) in arms.iter().rev() {
                value = format!(
                    "(({guard}) ? ({arm}) : ({value}))",
                    guard = expression(design, guard, memory_bases),
                    arm = expression(design, arm, memory_bases)
                );
            }
            value
        }
        SimExpr::Concat(parts) => {
            let mut value = "0ull".to_string();
            for part in parts {
                value = format!(
                    "(({value} << 1) | ({part}))",
                    value = value,
                    part = expression(design, part, memory_bases)
                );
            }
            value
        }
        SimExpr::Project { value, selector } => {
            let value = expression(design, value, memory_bases);
            let width = selector.width();
            match selector {
                Selector::Slice { low, .. } => format!("(({value}) >> {low}) & {}", mask(width)),
                Selector::Index { .. } => format!("({value}) & {}", mask(width)),
            }
        }
        SimExpr::MemoryRead { memory, port } => {
            let mem = &design.memories[memory.0];
            let address = mem
                .comb_reads
                .get(*port)
                .map(|read| expression(design, &read.address, memory_bases))
                .unwrap_or_else(|| "0ull".into());
            format!(
                "memory[({}u + ({address})) * total_threads + tid]",
                memory_bases[memory.0]
            )
        }
    }
}

fn read_target(design: &SimDesign, target: &SimTarget) -> String {
    let slot = format!("state[{}u * total_threads + tid]", target.signal.0);
    let mut value = slot;
    for selector in &target.selectors {
        match selector {
            Selector::Slice { low, .. } => {
                value = format!("(({value}) >> {low}) & {}", mask(selector.width()))
            }
            Selector::Index { .. } => value = format!("({value}) & {}", mask(selector.width())),
        }
    }
    let _ = design;
    value
}
