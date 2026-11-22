//! CUDA C++ prototype generation for word-level simulation designs.
//!
//! The generated kernel keeps state in a structure-of-arrays layout and runs
//! all requested cycles on the device.  It is intentionally self contained so
//! ordinary Rust builds and tests do not require CUDA.  A caller can compile
//! the returned source with [`compile_cuda`] when `nvcc` is available.

mod emit;
mod family;
mod kernel;
use crate::SimDesign;
pub use kernel::{generate_cuda, try_generate_cuda};
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

/// Host-side launch parameters for the self-contained CUDA runner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RunnerOptions {
    pub batch: usize,
    /// One-based CUDA cycle numbers span 1 through u32::MAX.
    pub cycles: u64,
    pub block_size: usize,
}

impl Default for RunnerOptions {
    fn default() -> Self {
        Self {
            batch: 1,
            cycles: 1_000,
            block_size: 256,
        }
    }
}

/// JSON summary emitted by the self-contained runner.
#[derive(Clone, Debug, PartialEq)]
pub struct RunnerResult {
    pub batch: usize,
    pub cycles: u64,
    pub elapsed_ms: f64,
    pub h2d_ms: f64,
    pub d2h_ms: f64,
    pub peak_vram_mib: usize,
    pub fail_batch: Option<usize>,
    pub fail_cycle: Option<u64>,
    pub fail_assert: Option<usize>,
    pub output: String,
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
    compile(source, output.as_ref(), true)
}

fn compile(source: &str, output: &Path, object: bool) -> Result<PathBuf, String> {
    let source_path = output.with_extension("cu");
    std::fs::write(&source_path, source)
        .map_err(|e| format!("write {}: {e}", source_path.display()))?;
    let mut compiler = Command::new("nvcc");
    compiler.args(["-O3", "-arch=sm_75"]);
    if object {
        compiler.arg("-c");
    }
    let result = compiler
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

/// Compile a generated CUDA translation unit as a runnable executable.
pub fn compile_cuda_executable(source: &str, output: impl AsRef<Path>) -> Result<PathBuf, String> {
    compile(source, output.as_ref(), false)
}

/// Compile and run a generated CUDA program. The translation unit includes a
/// host `main` that owns all allocations, CUDA event timing and JSON output.
pub fn run_cuda(
    design: &SimDesign,
    codegen: CodegenOptions,
    options: RunnerOptions,
    executable: impl AsRef<Path>,
) -> Result<RunnerResult, String> {
    if !(1..=i32::MAX as usize).contains(&options.batch) {
        return Err("batch must be in 1..=2147483647".into());
    }
    if !(1..=u64::from(u32::MAX)).contains(&options.cycles) {
        return Err("cycles must be in 1..=4294967295".into());
    }
    if !(1..=1024).contains(&options.block_size) {
        return Err("block size must be in 1..=1024".into());
    }
    if cuda_availability() == CudaAvailability::Unavailable {
        return Err("nvcc is unavailable".into());
    }
    let generated = try_generate_cuda(design, codegen)?.source;
    let executable = compile_cuda_executable(&generated, executable.as_ref())?;
    let executable =
        std::path::absolute(executable).map_err(|e| format!("resolve CUDA executable: {e}"))?;
    let output = Command::new(&executable)
        .args([
            options.batch.to_string(),
            options.cycles.to_string(),
            options.block_size.to_string(),
        ])
        .output()
        .map_err(|e| format!("run {}: {e}", executable.display()))?;
    if !output.status.success() {
        return Err(format!(
            "CUDA runner failed with {}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    parse_runner_result(&text)
}

fn parse_runner_result(text: &str) -> Result<RunnerResult, String> {
    let json = text
        .lines()
        .find(|line| line.trim_start().starts_with('{'))
        .ok_or_else(|| format!("runner produced no JSON: {text}"))?;
    fn number<T: std::str::FromStr>(json: &str, key: &str) -> Result<T, String> {
        let marker = format!("\"{key}\":");
        let value = json
            .split_once(&marker)
            .and_then(|(_, tail)| tail.split([',', '}']).next())
            .ok_or_else(|| format!("missing {key}"))?
            .trim();
        value.parse().map_err(|_| format!("invalid {key}: {value}"))
    }
    fn optional_number<T: std::str::FromStr>(json: &str, key: &str) -> Result<Option<T>, String> {
        let marker = format!("\"{key}\":");
        let value = json
            .split_once(&marker)
            .and_then(|(_, tail)| tail.split([',', '}']).next())
            .ok_or_else(|| format!("missing {key}"))?
            .trim();
        if value == "null" {
            Ok(None)
        } else {
            value
                .parse()
                .map(Some)
                .map_err(|_| format!("invalid {key}: {value}"))
        }
    }
    Ok(RunnerResult {
        batch: number(json, "batch")?,
        cycles: number(json, "cycles")?,
        elapsed_ms: number(json, "elapsed_ms")?,
        h2d_ms: number(json, "h2d_ms")?,
        d2h_ms: number(json, "d2h_ms")?,
        peak_vram_mib: number(json, "peak_vram_mib")?,
        fail_batch: optional_number(json, "fail_batch")?,
        fail_cycle: optional_number(json, "fail_cycle")?,
        fail_assert: optional_number(json, "fail_assert")?,
        output: json.to_owned(),
    })
}
