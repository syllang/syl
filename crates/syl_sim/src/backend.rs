//! T3 crossover thresholds and semantic eligibility. Selection with an
//! injected request never inspects the host environment.
use crate::gpu_cuda::{
    CodegenOptions, CudaCache, GeneratedCuda, KernelMode, RunnerOptions, cache::Toolchain,
    try_generate_cuda,
};
use crate::{FastProgram, InstanceId, SimDesign, StateMode};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SimBackendChoice {
    #[default]
    Auto,
    CpuRef,
    CpuSimd,
    GpuCuda,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GpuKernelMode {
    #[default]
    Auto,
    Batch,
    Family,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResolvedBackend {
    CpuRef,
    CpuSimd,
    GpuCuda(KernelMode),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendSelection {
    pub backend: ResolvedBackend,
    pub reason: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct BackendRequest {
    pub choice: SimBackendChoice,
    pub gpu_mode: GpuKernelMode,
    pub batch: usize,
    pub cycles: u64,
    pub state_mode: StateMode,
    pub stimulus: bool,
    pub direct_vcd: bool,
    pub strict_unique: bool,
    pub fail_on_x: bool,
    pub gpu_available: bool,
    pub gpu_cache_hit: bool,
}

impl Default for BackendRequest {
    fn default() -> Self {
        Self {
            choice: Default::default(),
            gpu_mode: Default::default(),
            batch: 1,
            cycles: 1,
            state_mode: StateMode::TwoState,
            stimulus: false,
            direct_vcd: false,
            strict_unique: false,
            fail_on_x: false,
            gpu_available: false,
            gpu_cache_hit: false,
        }
    }
}

fn family_candidates(d: &SimDesign) -> impl Iterator<Item = (usize, usize)> {
    let mut candidates: Vec<_> = d
        .instance_families()
        .iter()
        .enumerate()
        .filter(|(_, f)| {
            (1..=1024).contains(&f.members.len())
                && f.members
                    .iter()
                    .all(|id| d.instances[id.0].parent == Some(InstanceId(0)))
        })
        .map(|(i, f)| (i, f.members.len()))
        .collect();
    candidates.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    candidates.into_iter()
}

pub(crate) fn gpu_program(
    d: &SimDesign,
    mode: GpuKernelMode,
    batch: usize,
    check_family_work: bool,
) -> Result<(GeneratedCuda, usize), String> {
    let mut size = 0;
    if mode == GpuKernelMode::Family
        || (mode == GpuKernelMode::Auto && batch <= 256)
        || check_family_work
    {
        for (index, n) in family_candidates(d) {
            if n < 4 && mode != GpuKernelMode::Family {
                break;
            }
            if let Ok(program) = try_generate_cuda(
                d,
                CodegenOptions {
                    mode: KernelMode::InstanceFamilyParallel,
                    family_index: Some(index),
                },
            ) {
                if mode == GpuKernelMode::Family || (mode == GpuKernelMode::Auto && batch <= 256) {
                    return Ok((program, n));
                }
                size = n;
                break;
            }
        }
    }
    // A forced family kernel can always map the complete top to a singleton
    // block when its child subtrees are dependent or there are no children.
    let kernel = if mode == GpuKernelMode::Family {
        KernelMode::InstanceFamilyParallel
    } else {
        KernelMode::BatchPerThread
    };
    Ok((
        try_generate_cuda(
            d,
            CodegenOptions {
                mode: kernel,
                family_index: None,
            },
        )?,
        size,
    ))
}

pub(crate) enum Program {
    Reference,
    Simd(FastProgram),
    Cuda(GeneratedCuda),
}

impl Program {
    fn selection(&self, reason: &'static str) -> BackendSelection {
        let backend = match self {
            Self::Reference => ResolvedBackend::CpuRef,
            Self::Simd(_) => ResolvedBackend::CpuSimd,
            Self::Cuda(p) => ResolvedBackend::GpuCuda(p.mode),
        };
        BackendSelection { backend, reason }
    }
}

/// Route using the availability and cache observations supplied by the caller.
pub fn select_backend(d: &SimDesign, r: &BackendRequest) -> Result<BackendSelection, String> {
    let (p, reason) = select(d, r, |_, _| {
        if r.gpu_available {
            Ok(r.gpu_cache_hit)
        } else {
            Err("CUDA compiler or device unavailable".into())
        }
    })?;
    Ok(p.selection(reason))
}

/// A host-validated selection retains the SIMD program or generated CUDA
/// runner. Launch parameters can vary between deterministic runs of this plan.
pub struct LocalBackend<'d> {
    pub selection: BackendSelection,
    pub(crate) design: &'d SimDesign,
    pub(crate) request: BackendRequest,
    pub(crate) program: Program,
    pub(crate) toolchain: Option<Toolchain>,
}

/// Route on this host. Reference-only and small CPU workloads skip CUDA probing.
pub fn select_local_backend<'d>(
    d: &'d SimDesign,
    r: &BackendRequest,
    cache: CudaCache<'_>,
) -> Result<LocalBackend<'d>, String> {
    let mut toolchain = None;
    let (program, reason) = select(d, r, |source, requires_device| {
        if !requires_device && !cache.has_entries() {
            return Ok(false);
        }
        let t = Toolchain::discover(cache)?;
        let hot = t.contains(source);
        if hot || requires_device {
            t.probe()?;
            toolchain = Some(t);
        }
        Ok(hot)
    })?;
    Ok(LocalBackend {
        selection: program.selection(reason),
        design: d,
        request: *r,
        program,
        toolchain,
    })
}

fn select(
    d: &SimDesign,
    r: &BackendRequest,
    gpu: impl FnOnce(&str, bool) -> Result<bool, String>,
) -> Result<(Program, &'static str), String> {
    use SimBackendChoice as C;
    let cpu = |reason| Ok((Program::Reference, reason));
    if r.batch == 0 {
        return Err("--batch must be at least 1".into());
    }
    if r.batch > 1 && (r.stimulus || r.direct_vcd) {
        return Err("--stimulus and --vcd require --batch 1".into());
    }
    if r.choice == C::CpuRef {
        return cpu("explicit cpu-ref");
    }
    let needs_ref = r.state_mode == StateMode::FourState
        || r.stimulus
        || r.direct_vcd
        || r.strict_unique
        || r.fail_on_x;
    if needs_ref {
        if r.choice == C::Auto {
            return cpu("reference simulation semantics requested");
        }
        return Err("accelerated backends require two-state simulation without --stimulus, --vcd, --strict-unique or --fail-on-x".into());
    }
    if r.choice == C::Auto && r.batch == 1 && r.cycles < 50_000 {
        return cpu("short single trajectory");
    }
    let simd = if r.choice != C::GpuCuda {
        match FastProgram::new(d) {
            Ok(program) if r.choice == C::CpuSimd => {
                return Ok((Program::Simd(program), "explicit cpu-simd"));
            }
            Ok(program) => Some(program),
            Err(e) if r.choice == C::CpuSimd => return Err(format!("cpu-simd: {e}")),
            Err(_) => return cpu("design outside CPU SIMD subset"),
        }
    } else {
        None
    };
    let work = (r.batch as u64).saturating_mul(r.cycles);
    let family_work = r.cycles >= 1000 && family_candidates(d).any(|(_, n)| n >= 16);
    if r.choice == C::GpuCuda || work >= 256_000 || family_work {
        let candidate = || {
            RunnerOptions {
                batch: r.batch,
                cycles: r.cycles,
                ..Default::default()
            }
            .validate()?;
            let (p, n) = gpu_program(d, r.gpu_mode, r.batch, family_work)?;
            let hot = gpu(
                &p.source,
                r.choice == C::GpuCuda || work >= 4_000_000 || (n >= 16 && r.cycles >= 1000),
            )?;
            let reason = if r.choice == C::GpuCuda {
                Some("explicit gpu-cuda")
            } else if work >= 4_000_000 {
                Some("large batch workload")
            } else if work >= 256_000 && hot {
                Some("cached CUDA workload")
            } else if n >= 16 && r.cycles >= 1000 {
                Some("parallel instance family")
            } else {
                None
            };
            Ok(reason.map(|reason| (Program::Cuda(p), reason)))
        };
        match candidate() {
            Ok(Some(selection)) => return Ok(selection),
            Err(e) if r.choice == C::GpuCuda => return Err(e),
            _ => {}
        }
    }
    Ok((
        Program::Simd(simd.expect("auto CPU candidate")),
        "CPU SIMD crossover",
    ))
}
