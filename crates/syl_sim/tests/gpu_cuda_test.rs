use std::collections::BTreeMap;
use std::path::PathBuf;
use syl_elab::{ElaborationOutput, HardwareCompiler};
use syl_sema::{SemanticSession, SemanticSourceFile};
use syl_sim::SimDesign;
use syl_sim::gpu_cuda::{
    CodegenOptions, KernelMode, compile_cuda, cuda_availability, generate_cuda,
};
use syl_span::SourceId;
use syl_syntax::SourceParser;

fn compile(source: &str) -> ElaborationOutput {
    let ast = SourceParser::new_in(source, SourceId::new(0))
        .parse_file()
        .unwrap();
    let file = SemanticSourceFile::new(vec!["gpu_test".into()], &ast);
    let hir = SemanticSession::new_sources(vec![file])
        .resolve_hir()
        .unwrap();
    let tir = hir.check_tir().unwrap();
    HardwareCompiler::new().output_for_tir(&tir)
}

#[test]
fn cuda_codegen_emits_both_parallel_mappings() {
    let source = "cell Top(i: in UInt<8>) -> o: UInt<8> { o := i + 1 }";
    let output = compile(source);
    let design = SimDesign::lower(&output, "Top", &BTreeMap::new()).unwrap();
    let batch = generate_cuda(&design, CodegenOptions::default());
    let family = generate_cuda(
        &design,
        CodegenOptions {
            mode: KernelMode::InstanceFamilyParallel,
            family_index: Some(0),
        },
    );
    assert!(
        batch
            .source
            .contains("state[slot_id * total_threads + tid]")
    );
    assert!(batch.source.contains("syl_sim_kernel"));
    assert!(family.source.contains("family_lane"));
    assert!(family.source.contains("__syncthreads()"));
    assert_eq!(batch.signal_slots, design.signals.len());
}

#[test]
fn cuda_source_compiles_when_nvcc_is_available() {
    if cuda_availability() != syl_sim::gpu_cuda::CudaAvailability::Available {
        return;
    }
    let output = compile("cell Top(i: in Bit) -> o: Bit { o := not i }");
    let design = SimDesign::lower(&output, "Top", &BTreeMap::new()).unwrap();
    let generated = generate_cuda(&design, CodegenOptions::default());
    let root: PathBuf = std::env::temp_dir().join(format!("syl_cuda_{}", std::process::id()));
    let object = root.with_extension("o");
    let result = compile_cuda(&generated.source, &object);
    assert!(result.is_ok(), "{result:?}");
    let _ = std::fs::remove_file(object.with_extension("cu"));
    let _ = std::fs::remove_file(object);
}
