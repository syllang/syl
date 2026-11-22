use std::collections::BTreeMap;
use std::time::Instant;
use syl_elab::{ElaborationOutput, HardwareCompiler};
use syl_sema::{SemanticSession, SemanticSourceFile};
use syl_sim::gpu_cuda::{CodegenOptions, KernelMode, RunnerOptions, cuda_availability, run_cuda};
use syl_sim::{ParamValue, SimDesign};
use syl_span::SourceId;
use syl_syntax::SourceParser;

fn compile(files: &[(&str, &str)]) -> ElaborationOutput {
    let ast = files
        .iter()
        .enumerate()
        .map(|(index, (_, source))| {
            SourceParser::new_in(source, SourceId::new(index))
                .parse_file()
                .unwrap()
        })
        .collect::<Vec<_>>();
    let sources = files
        .iter()
        .zip(&ast)
        .map(|((path, _), ast)| {
            SemanticSourceFile::new(path.split('.').map(str::to_owned).collect(), ast)
        })
        .collect();
    let hir = SemanticSession::new_sources(sources).resolve_hir().unwrap();
    let tir = hir.check_tir().unwrap();
    HardwareCompiler::new().output_for_tir(&tir)
}

fn main() {
    if cuda_availability() != syl_sim::gpu_cuda::CudaAvailability::Available {
        eprintln!("nvcc unavailable, benchmark skipped");
        return;
    }
    let output = compile(&[
        (
            "std.stream",
            include_str!("../../../examples/std/stream.syl"),
        ),
        (
            "lane_array",
            include_str!("../../../examples/mvp/lane_array.syl"),
        ),
    ]);
    println!("{{\"gpu\":\"RTX 2080 Ti\",\"cycles\":1000,\"results\":[");
    let mut first = true;
    for n in [1u64, 4, 16, 64] {
        let design = SimDesign::lower(
            &output,
            "LaneArray",
            &BTreeMap::from([
                ("N".into(), ParamValue::Nat(n)),
                ("W".into(), ParamValue::Nat(32)),
            ]),
        )
        .unwrap();
        let family_index = design
            .families
            .iter()
            .position(|family| family.module.last().is_some_and(|name| name == "Lane"));
        for batch in [1usize, 16, 64, 256, 1024, 4096, 16384] {
            for mode in [
                KernelMode::BatchPerThread,
                KernelMode::InstanceFamilyParallel,
            ] {
                let path = std::env::temp_dir().join(format!("syl_bench_{n}_{batch}_{mode:?}"));
                let started = Instant::now();
                let result = run_cuda(
                    &design,
                    CodegenOptions { mode, family_index },
                    RunnerOptions {
                        batch,
                        cycles: 1_000,
                        block_size: 256,
                    },
                    &path,
                )
                .unwrap();
                if !first {
                    println!(",");
                }
                first = false;
                print!(
                    "{{\"n\":{n},\"batch\":{batch},\"mode\":\"{mode:?}\",\"elapsed_ms\":{:.6},\"h2d_ms\":{:.6},\"d2h_ms\":{:.6},\"peak_vram_mib\":{},\"wall_ms\":{:.3}}}",
                    result.elapsed_ms,
                    result.h2d_ms,
                    result.d2h_ms,
                    result.peak_vram_mib,
                    started.elapsed().as_secs_f64() * 1000.0,
                );
                let _ = std::fs::remove_file(path.with_extension("cu"));
                let _ = std::fs::remove_file(path);
            }
        }
    }
    println!("]}}\n");
}
