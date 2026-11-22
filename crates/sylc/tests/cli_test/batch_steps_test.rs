use super::*;

const FALSE_PASS: &str = "cell T(clk:in Clock,i:in Bit)->q:Bit{assert(not i) q:=i}";
const DERIVED: &str = "cell T(en:in Bit,pin:in Bit,i:in Bit)->q:Bit{signal clk:Clock:=pin and en reg v:Bit next v:=i assert(not (i eq i)) q:=v}";

fn sim(p: &TestProject, args: &[&str]) -> Output {
    let mut command = vec![
        "sim",
        "top.syl",
        "--top",
        "T",
        "--diagnostics-format",
        "json",
    ];
    command.extend_from_slice(args);
    p.run("", &command)
}

fn diagnostic(out: &Output) -> String {
    failure(out);
    let diag = &report(out)["diagnostics"][0];
    assert_eq!(diag["code"], "E_SIM");
    diag["message"].as_str().unwrap().to_owned()
}

#[test]
fn each_explicit_batch_option_selects_deterministic_inputs_on_its_own() {
    let p = TestProject::new();
    p.write("top.syl", FALSE_PASS);
    success(&sim(&p, &["--cycles", "5"]));
    for flag in [
        &["--backend", "auto"][..],
        &["--backend", "cpu-ref"],
        &["--batch", "1"],
        &["--seed", "0"],
        &["--threads", "1"],
        &["--gpu-mode", "auto"],
        &["--gpu-cache-dir", "cache"],
        &["--no-gpu-cache"],
        &["--replay-vcd", "out.vcd"],
    ] {
        let out = sim(&p, &[&["--cycles", "5"], flag].concat());
        assert_eq!(out.status.code(), Some(1), "{flag:?}");
        assert!(
            diagnostic(&out).contains("batch 0, cycle 1, T.assert[0]"),
            "{flag:?}"
        );
    }
}

#[test]
fn seeded_single_lane_keeps_debug_checks_and_verified_replay() {
    let p = TestProject::new();
    p.write("top.syl", FALSE_PASS);
    for flag in ["--strict-unique", "--fail-on-x"] {
        success(&sim(&p, &["--cycles", "5", flag]));
        let err = diagnostic(&sim(
            &p,
            &[
                "--backend",
                "cpu-ref",
                "--batch",
                "1",
                "--cycles",
                "5",
                "--seed",
                "0",
                "--replay-vcd",
                "out.vcd",
                flag,
            ],
        ));
        assert!(err.contains("batch 0, cycle 1, T.assert[0]"), "{err}");
        let wave = fs::read_to_string(p.0.join("out.vcd")).unwrap();
        assert!(wave.contains("#2\n1s0"));
        fs::remove_file(p.0.join("out.vcd")).unwrap();
    }
    p.write(
        "top.syl",
        "cell T(i:in Bit)->q:Bit{q:=select unique {i=>1,i=>0,default=>0,}}",
    );
    let err = diagnostic(&sim(
        &p,
        &["--seed", "0", "--cycles", "12", "--strict-unique"],
    ));
    assert!(err.contains("overlapping guards"), "{err}");
}

#[test]
fn replay_rejects_direct_trace_flags_before_writing() {
    let p = TestProject::new();
    p.write("top.syl", FALSE_PASS);
    p.write("s.json", "[]");
    for (flag, file) in [("--stimulus", "s.json"), ("--vcd", "direct.vcd")] {
        let err = diagnostic(&sim(&p, &[flag, file, "--replay-vcd", "out.vcd"]));
        assert!(err.contains("--replay-vcd") && err.contains(flag), "{err}");
        assert!(!p.0.join("out.vcd").exists() && !p.0.join("direct.vcd").exists());
    }
}

#[test]
fn replay_baseline_settles_combinational_outputs_before_checking_stimulus() {
    let p = TestProject::new();
    p.write(
        "top.syl",
        "cell T(clk:in Clock,i:in Bit)->q:Bit{
        assert(not i) q:=select unique { not i=>1,not i=>1,default=>0,} }",
    );
    let err = diagnostic(&sim(
        &p,
        &[
            "--cycles",
            "1",
            "--strict-unique",
            "--replay-vcd",
            "out.vcd",
        ],
    ));
    assert!(err.contains("batch 0, cycle 1, T.assert[0]"), "{err}");
    let wave = fs::read_to_string(p.0.join("out.vcd")).unwrap();
    let q = wave
        .lines()
        .find(|line| line.ends_with(" q $end"))
        .unwrap()
        .split_whitespace()
        .nth(3)
        .unwrap();
    let frames = wave.split_once("#0\n").unwrap().1;
    let (baseline, rest) = frames.split_once("#1\n").unwrap();
    let (driven, _) = rest.split_once("#2\n").unwrap();
    assert!(baseline.lines().any(|s| s == format!("1{q}")), "{baseline}");
    assert!(driven.lines().any(|s| s == format!("0{q}")), "{driven}");
}

#[test]
fn global_step_orders_derived_edges_and_records_the_failing_settle() {
    let p = TestProject::new();
    p.write("top.syl", DERIVED);
    let mut first = None;
    for (cycles, batch, expected) in [
        ("1", "4", "batch 3, cycle 1"),
        ("8", "4", "batch 3, cycle 1"),
        ("8", "1", "batch 0, cycle 5"),
    ] {
        let err = diagnostic(&sim(
            &p,
            &[
                "--cycles",
                cycles,
                "--batch",
                batch,
                "--seed",
                "6",
                "--replay-vcd",
                "out.vcd",
            ],
        ));
        assert!(
            err.contains(expected) && err.contains("T.assert[0]"),
            "{err}"
        );
        let wave = fs::read_to_string(p.0.join("out.vcd")).unwrap();
        let clk = wave
            .lines()
            .find(|s| s.ends_with(" clk $end"))
            .unwrap()
            .split_whitespace()
            .nth(3)
            .unwrap();
        let last = wave.rsplit_once('#').unwrap().1;
        assert!(last.lines().any(|line| line == format!("1{clk}")), "{last}");
        if batch == "4" {
            let (baseline, failed) = wave
                .split_once("#0\n")
                .unwrap()
                .1
                .split_once("#1\n")
                .unwrap();
            assert!(
                baseline.lines().any(|line| line == format!("0{clk}")),
                "{baseline}"
            );
            assert!(
                failed.lines().any(|line| line == format!("1{clk}")),
                "{failed}"
            );
            assert_eq!(last, format!("1\n{failed}"));
            assert_eq!(first.get_or_insert_with(|| wave.clone()), &wave);
        }
    }
}

#[test]
fn zero_step_settle_checks_unknown_outputs_before_digest() {
    let p = TestProject::new();
    p.write(
        "top.syl",
        "cell T(clk:in Clock,i:in Bit)->q:Bit{reg v:Bit next v:=i q:=v}",
    );
    for lanes in ["1", "4"] {
        let err = diagnostic(&sim(
            &p,
            &["--batch", lanes, "--cycles", "0", "--fail-on-x"],
        ));
        assert!(
            err.contains("batch 0, cycle 0: unknown output q=x"),
            "{err}"
        );
    }
}
