use super::*;
use syl_sim::gpu_cuda::{CudaCache, KernelMode};

pub(super) fn run(design: &SimDesign, args: &SimArgs, protected: &[&Path]) -> Result<bool> {
    if args.threads == Some(0) {
        return Err(error("--threads must be at least 1"));
    }
    if args.no_gpu_cache && args.gpu_cache_dir.is_some() {
        return Err(error("--gpu-cache-dir conflicts with --no-gpu-cache"));
    }
    if let Some(path) = &args.replay_vcd {
        match same_file::Handle::from_path(path) {
            Ok(dest) => ensure_safe_destination(path, &dest, protected)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(error(e)),
        }
    }

    let batch = args.batch.unwrap_or(1);
    let legacy = args.batch.is_none()
        && args.seed.is_none()
        && args.backend.is_none()
        && args.threads.is_none()
        && args.gpu_mode.is_none()
        && args.replay_vcd.is_none()
        && args.gpu_cache_dir.is_none()
        && !args.no_gpu_cache;
    let reference_trace = legacy
        || args.stimulus.is_some()
        || args.vcd.is_some()
        || (batch == 1 && (args.strict_unique || args.fail_on_x));
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
    let options = BatchOptions {
        batch,
        cycles: args.cycles,
        seed: args.seed.unwrap_or(0),
        threads: args
            .threads
            .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, usize::from)),
        cache,
        state_mode,
        strict_unique: args.strict_unique,
        fail_on_x: args.fail_on_x,
    };
    let result = plan.run(options.seed, options.threads).map_err(runtime)?;
    if let Some((coordinate, failure)) = result.failure {
        if let Some(path) = &args.replay_vcd {
            let file = wave_file(path, protected)?;
            replay_failure(design, options, coordinate, Some(&mut BufWriter::new(file)))
                .map_err(runtime)?;
        }
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
        "{} backend={backend} batch={batch} cycles={} seed={} digest={}{cache}",
        args.top, args.cycles, options.seed, result.digest
    );
    Ok(true)
}
