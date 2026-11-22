use super::*;
use syl_sim::gpu_cuda::{CudaCache, KernelMode};

pub(super) fn run(design: &SimDesign, args: &SimArgs, protected: &[&Path]) -> Result<bool> {
    if args.threads == Some(0) {
        return Err(error("--threads must be at least 1"));
    }
    if args.no_gpu_cache && args.gpu_cache_dir.is_some() {
        return Err(error("--gpu-cache-dir conflicts with --no-gpu-cache"));
    }
    let trace = args.stimulus.is_some() || args.vcd.is_some();
    if trace && args.replay_vcd.is_some() {
        return Err(error("--replay-vcd conflicts with --stimulus and --vcd"));
    }
    if trace && args.seed.is_some() {
        return Err(error(
            "--seed selects deterministic inputs and conflicts with --stimulus and --vcd",
        ));
    }
    if let Some(path) = &args.replay_vcd {
        match same_file::Handle::from_path(path) {
            Ok(dest) => ensure_safe_destination(path, &dest, protected)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(error(e)),
        }
    }

    let batch = args.batch.unwrap_or(1);
    let accelerated = matches!(args.backend.as_deref(), Some("cpu-simd" | "gpu-cuda"));
    let deterministic =
        args.batch.is_some() || args.seed.is_some() || args.replay_vcd.is_some() || accelerated;
    let reference_trace = trace || !deterministic;
    let state_mode = if reference_trace || args.fail_on_x || args.strict_unique {
        StateMode::FourState
    } else {
        StateMode::TwoState
    };
    let cache = if args.no_gpu_cache {
        CudaCache::Disabled
    } else {
        args.gpu_cache_dir
            .as_deref()
            .map_or(CudaCache::Default, CudaCache::Directory)
    };
    let choice = match args.backend.as_deref() {
        Some("cpu-ref") => SimBackendChoice::CpuRef,
        Some("cpu-simd") => SimBackendChoice::CpuSimd,
        Some("gpu-cuda") => SimBackendChoice::GpuCuda,
        _ => SimBackendChoice::Auto,
    };
    let gpu_mode = match args.gpu_mode.as_deref() {
        Some("batch") => GpuKernelMode::Batch,
        Some("family") => GpuKernelMode::Family,
        _ => GpuKernelMode::Auto,
    };
    let request = BackendRequest {
        choice,
        gpu_mode,
        batch,
        cycles: args.cycles,
        state_mode,
        stimulus: args.stimulus.is_some(),
        direct_vcd: args.vcd.is_some(),
        strict_unique: args.strict_unique,
        fail_on_x: args.fail_on_x,
        ..Default::default()
    };
    let plan = select_local_backend(design, &request, cache).map_err(error)?;
    if reference_trace {
        return Ok(false);
    }
    let seed = args.seed.unwrap_or(0);
    let threads = args
        .threads
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, usize::from));
    let result = plan
        .run_with_replay(seed, threads, || {
            args.replay_vcd
                .as_deref()
                .map(|path| wave_file(path, protected).map(BufWriter::new))
                .transpose()
                .map_err(|e| RuntimeError::Invalid(e.message))
        })
        .map_err(runtime)?;
    if let Some((coordinate, failure)) = result.failure {
        return Err(located(
            failure.span,
            format!(
                "batch {}, {failure} (assert {})",
                coordinate.batch, coordinate.assert_id.0
            ),
        ));
    }
    let backend = match plan.selection.backend {
        ResolvedBackend::CpuRef => "cpu-ref",
        ResolvedBackend::CpuSimd => "cpu-simd",
        ResolvedBackend::GpuCuda(KernelMode::BatchPerThread) => "gpu-cuda/batch",
        ResolvedBackend::GpuCuda(KernelMode::InstanceFamilyParallel) => "gpu-cuda/family",
    };
    let cache = match result.cache_hit {
        Some(true) => " cache=hit",
        Some(false) => " cache=miss",
        None => "",
    };
    println!(
        "{} backend={backend} batch={batch} cycles={} seed={seed} digest={}{cache}",
        args.top, args.cycles, result.digest
    );
    Ok(true)
}
