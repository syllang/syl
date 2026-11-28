pub(super) const SHAPES: [(&str, &str); 4] = [
    (
        "WideAluArray",
        include_str!("../../../../examples/benchmarks/wide_alu_array.syl"),
    ),
    (
        "DeepFsmChain",
        include_str!("../../../../examples/benchmarks/deep_fsm_chain.syl"),
    ),
    (
        "SharedRamArbiter",
        include_str!("../../../../examples/benchmarks/shared_ram_arbiter.syl"),
    ),
    (
        "FaultHuntPipeline",
        include_str!("../../../../examples/benchmarks/fault_hunt_pipeline.syl"),
    ),
];
