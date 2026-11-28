use syl_sim::{
    FastBatchCpuSimulator, FastProgram, HwDirection, JitProgram, SimDesign, Simulator, StateMode,
    Value, batch_stimulus_word,
};

// Compare every prefix, including the initialized output, against the same T3 input stream.
pub(super) fn check_trace(
    case: &str,
    d: &SimDesign,
    cycles: u64,
    family: Option<&syl_sim::FastFamilyProgram>,
) {
    let fast = FastProgram::new(d).unwrap();
    let jit = JitProgram::new(d).unwrap();
    let mut sim = FastBatchCpuSimulator::new(&fast, 1, 1);
    sim.for_each_tile(1, |_, tile| {
        let mut reference = Simulator::with_mode(d, StateMode::TwoState);
        for cycle in 0..=cycles {
            let mut base = 0;
            for p in d.top_ports() {
                let w = d.signal(p.signal).width;
                if p.direction == HwDirection::In && p.name != "clk" {
                    let mut parts = Vec::new();
                    for j in (0..w.div_ceil(64)).rev() {
                        let bits = (w - j * 64).min(64);
                        let v = if cycle == 0 {
                            0
                        } else if p.name == "rst" {
                            u64::from(cycle == 1)
                        } else {
                            batch_stimulus_word(0, 0, cycle, base, j) & (u64::MAX >> (64 - bits))
                        };
                        parts.push(Value::from_u64(bits, v));
                        tile.input_word(&fast, p.signal, j as usize)[0] = v;
                    }
                    reference
                        .drive(p.signal, Value::concat(&parts).unwrap())
                        .unwrap();
                }
                base += u64::from(w.div_ceil(64));
            }
            if cycle == 0 {
                reference.eval_comb().unwrap();
            } else {
                reference.tick("clk").unwrap();
                tile.tick(&fast).unwrap();
            }
            let mut expected = Vec::new();
            for p in d
                .top_ports()
                .iter()
                .filter(|p| p.direction == HwDirection::Out)
            {
                let v = reference.read(p.signal);
                for j in 0..v.width().div_ceil(64) {
                    let word = v
                        .slice(j * 64, (v.width() - j * 64).min(64))
                        .as_u64()
                        .unwrap();
                    assert_eq!(
                        tile.word(&fast, p.signal, j as usize),
                        [word],
                        "{case} {} {j} cycle {cycle}",
                        p.name
                    );
                    expected.push(word);
                }
            }
            let oracle = syl_sim::SingleTraceResult {
                outputs: expected,
                failure: None,
            };
            assert_eq!(
                jit.run_trajectory(0, 0, cycle),
                oracle,
                "{case} JIT cycle {cycle}"
            );
            if let Some(f) = family {
                assert_eq!(
                    f.run_trajectory(0, cycle),
                    oracle,
                    "{case} family cycle {cycle}"
                );
            }
        }
    });
}
