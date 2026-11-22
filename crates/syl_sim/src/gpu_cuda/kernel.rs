use super::{CodegenOptions, GeneratedCuda, KernelMode, emit::Emit, family};
use crate::{HwDirection, HwResetMode, HwResetPolarity, SimDesign};
use std::fmt::Write;

/// Compile a single-clock two-state design, returning a diagnostic for unsupported IR.
pub fn try_generate_cuda(d: &SimDesign, options: CodegenOptions) -> Result<GeneratedCuda, String> {
    if d.instances.is_empty()
        || d.clocks.len() > 1
        || !d.externs.is_empty()
        || d.signals.iter().any(|s| s.alias.is_some())
        || d.resets.iter().any(|r| r.mode != HwResetMode::Sync)
    {
        return Err("CUDA requires a concrete two-state, single-clock design with synchronous reset and no externs".into());
    }
    let mut signal_slots = 0;
    let bases = d
        .signals
        .iter()
        .map(|s| {
            let b = signal_slots;
            signal_slots += s.width.div_ceil(64) as usize;
            b
        })
        .collect::<Vec<_>>();
    let memory_words = d.memories.iter().map(|m| m.depth as usize).sum();
    let selected = options
        .family_index
        .map(|i| d.instance_families().get(i).ok_or("unknown family index"))
        .transpose()?;
    let family = selected.filter(|f| {
        options.mode == KernelMode::InstanceFamilyParallel
            && !f.members.contains(&crate::InstanceId(0))
    });
    let family_size = family.map_or(1, |f| f.members.len());
    let parallel = options.mode == KernelMode::InstanceFamilyParallel;
    let mut source = format!(
        "#define FAMILY {family_size}u\n#define PARALLEL {}\n#define ASSERT_COUNT {}ull\n#define SIGNAL_SLOTS {signal_slots}ull\n#define MEM_WORDS {memory_words}ull\n",
        u8::from(parallel),
        d.assertions.len().max(1)
    );
    source.push_str(include_str!("device.cuh"));
    let mut drive = String::new();
    let mut outputs = Vec::new();
    for p in d.top_ports() {
        let base = bases[p.signal.0];
        let width = d.signal(p.signal).width;
        match p.direction {
            HwDirection::Out => outputs.extend(base..base + width.div_ceil(64) as usize),
            HwDirection::In => {
                if d.clocks.iter().any(|c| c.signal == p.signal) {
                    continue;
                }
                if let Some(r) = d.resets.iter().find(|r| r.signal == p.signal) {
                    writeln!(
                        drive,
                        "f.word({base}) = (cycle==1) ^ {};",
                        u8::from(r.polarity == HwResetPolarity::ActiveLow)
                    )
                    .unwrap();
                } else {
                    writeln!(drive,"for(unsigned i=0;i<{};++i) f.word({base}+i)=stimulus(f.b,cycle,{base}+i) & mask(min(64u,{width}u-i*64));",width.div_ceil(64)).unwrap();
                }
            }
            _ => return Err("inout pins are unsupported".into()),
        }
    }
    if outputs.is_empty() {
        return Err("runner needs at least one output".into());
    }
    let clock_slot = d.clocks.first().map(|c| bases[c.signal.0]);
    if let Some(c) = d.clocks.first()
        && !d.top_ports().iter().any(|p| p.signal == c.signal)
    {
        return Err("clock must be a top port".into());
    }
    if d.memories.iter().any(|m| m.width > 64) {
        return Err("memory word exceeds 64 bits".into());
    }
    writeln!(
        source,
        "#define OUTPUT_WORDS {}u\n__device__ const unsigned output_slots[] = {{{}}};",
        outputs.len(),
        outputs
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",")
    )
    .unwrap();
    writeln!(
        source,
        "__device__ void drive(Frame f,unsigned cycle) {{\n{drive}}}"
    )
    .unwrap();
    writeln!(
        source,
        "__device__ void clock_level(Frame f,unsigned level) {{ {} }}",
        clock_slot.map_or(String::new(), |s| format!("f.word({s})=level;"))
    )
    .unwrap();
    if let Some(fam) = family {
        source.push_str(&family::functions(d, &bases, fam)?);
    } else {
        let e = Emit {
            design: d,
            bases: &bases,
            local: None,
            lane: 0,
            lanes: 1,
        };
        source.push_str("__device__ void eval_comb(Frame f,unsigned) {\n");
        for id in &d.comb_eval_order {
            let a = &d.comb[id.0];
            source.push_str(&e.assign(&a.target, &a.value)?);
        }
        writeln!(source,"}}\n__device__ void step_clock(Frame f,unsigned,unsigned cycle) {{\n{} }}\n__device__ void gather(Frame) {{}}",e.edge()?).unwrap();
    }
    source.push_str(include_str!("runner.cuh"));
    Ok(GeneratedCuda {
        source,
        signal_slots,
        memory_words,
        family_size,
        mode: options.mode,
    })
}
/// Generate CUDA source for supported designs. Use `try_generate_cuda` with arbitrary inputs.
pub fn generate_cuda(design: &SimDesign, options: CodegenOptions) -> GeneratedCuda {
    try_generate_cuda(design, options).expect("unsupported CUDA design")
}
