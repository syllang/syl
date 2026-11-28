use super::*;

#[test]
fn compile_time_format_errors_point_into_source() {
    let p = TestProject::new();
    for (statement, message, location) in [
        (r#"log("bad\n{")"#, "unclosed", "{"),
        (r#"log("bad }")"#, "unmatched", "}"),
        (r#"log("{i:foo}")"#, "unknown format specifier", ":foo"),
        (r#"log("{i + missing}")"#, "missing", "missing"),
        (r#"log("{i i}")"#, "unexpected token", "i"),
        (
            r#"log("{log(1) == 2}")"#,
            "standalone hardware statement",
            "log",
        ),
        (r#"log("{clk}")"#, "cannot format", "clk"),
        (
            r#"signal x: Bit := log("bad")"#,
            "standalone hardware statement",
            "log",
        ),
    ] {
        let source = format!("cell T(clk:in Clock,i:in Bit) {{ {statement}; }}");
        p.write("t.syl", &source);
        let output = p.run("", &["check", "t.syl", "--diagnostics-format", "json"]);
        failure(&output);
        let report = report(&output);
        let d = report["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["message"].as_str().unwrap().contains(message))
            .unwrap_or_else(|| panic!("{report}"));
        let loc = &d["location"];
        assert_eq!(
            &source[loc["byte_start"].as_u64().unwrap() as usize
                ..loc["byte_end"].as_u64().unwrap() as usize],
            location
        );
    }
    p.write("t.syl", r#"fn f() -> nat { log("bad"); return 1 }"#);
    let out = p.run("", &["check", "t.syl"]);
    failure(&out);
    assert!(String::from_utf8_lossy(&out.stderr).contains("standalone hardware statement"));
}

#[test]
fn cdc_checks_capture_positional_guard_and_explicit_reset() {
    let p = TestProject::new();
    for stmt in [
        r#"let p = place Producer(clk:a,rst:ra,x:i) log(clk:b,"{p}")"#,
        r#"log(clk:b,"{q}")"#,
        r#"log(clk:b,when:q,"bad")"#,
        r#"assert(clk:b,i,"{}",q)"#,
        r#"assert(clk:b,rst:ra,i)"#,
        r#"assert(clk:b,i,"{q}")"#,
    ] {
        p.write("t.syl",&format!("cell Producer(clk:in Clock,rst:in Reset,x:in Bit)->y:Bit {{reg q:Bit reset(rst,0) next q:=x y:=q}} cell T<A:Domain,B:Domain>(a:in Clock<A>,b:in Clock<B>,ra:in Reset<A>,i:in Bit) {{reg q:Bit reset(ra,0) next q:=i {stmt};}}"));
        let out = p.run("", &["check", "t.syl", "--diagnostics-format", "json"]);
        failure(&out);
        assert!(
            report(&out)["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .any(|d| d["code"] == "E_MIDDLE_CDC_VIOLATION"),
            "{stmt}"
        );
    }
}

#[test]
fn trace_logs_and_formatted_faults_reach_human_and_json_streams() {
    let p = TestProject::new();
    p.write("t.syl", r#"cell T(clk:in Clock,i:in Bit,data:in UInt<8>) {log("word={data:#X}") assert(i,"fault {data:#x}")}"#);
    p.write("s.json",r#"[{"drive":{"i":1,"data":42},"clocks":["clk"]},{"drive":{"i":0,"data":171},"clocks":["clk"]}]"#);
    for format in ["human", "json"] {
        let out = p.run(
            "",
            &[
                "sim",
                "t.syl",
                "--top",
                "T",
                "--stimulus",
                "s.json",
                "--diagnostics-format",
                format,
            ],
        );
        assert_eq!(out.status.code(), Some(1));
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            "[cycle 1] T: word=0X2A\n[cycle 2] T: word=0XAB\n"
        );
        let diagnostic = if format == "json" {
            report(&out)["diagnostics"][0]["message"]
                .as_str()
                .unwrap()
                .to_owned()
        } else {
            String::from_utf8(out.stderr).unwrap()
        };
        assert!(diagnostic.contains("cycle 2, T.assert[0]: fault 0xab"));
    }
}

#[test]
fn fast_backends_print_only_the_replayed_lane_with_or_without_vcd() {
    let p = TestProject::new();
    p.write(
        "t.syl",
        r#"cell T(clk:in Clock, rst:in Reset, data:in UInt<8>) -> q:UInt<8> {
        reg seen:Bit reset(rst,0) next seen:=1
        log("word={data:#x}") assert(not seen,"bad {data:#x}") q:=data }"#,
    );
    for backend in ["cpu-simd", "gpu-cuda"] {
        if backend == "gpu-cuda" && syl_sim::gpu_cuda::probe_cuda().is_err() {
            continue;
        }
        for vcd in [false, true] {
            let mut args = vec![
                "sim",
                "t.syl",
                "--top",
                "T",
                "--cycles",
                "8",
                "--batch",
                "7",
                "--backend",
                backend,
                "--diagnostics-format",
                "json",
            ];
            if vcd {
                args.extend(["--replay-vcd", "fault.vcd"]);
            }
            let out = p.run("", &args);
            assert_eq!(out.status.code(), Some(1));
            let word = |cycle| syl_sim::batch_stimulus_word(0, 0, cycle, 2, 0) & 255;
            assert_eq!(
                String::from_utf8_lossy(&out.stdout),
                format!(
                    "[cycle 2] T: word=0x{:02x}\n[cycle 3] T: word=0x{:02x}\n",
                    word(2),
                    word(3)
                ),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert!(
                report(&out)["diagnostics"][0]["message"]
                    .as_str()
                    .unwrap()
                    .contains(&format!(
                        "batch 0, cycle 3, T.assert[0]: bad 0x{:02x}",
                        word(3)
                    ))
            );
            if vcd {
                assert!(p.0.join("fault.vcd").is_file());
            }
        }
    }
}

#[test]
fn independent_resets_require_an_explicit_choice() {
    let p = TestProject::new();
    let source = |stmt: &str| {
        format!(
            "cell T<D:Domain>(clk:in Clock<D>,a:in Reset<D>,b:in Reset<D>,ok:in Bit) {{signal aa:Reset<D> := a {stmt};}}"
        )
    };
    for stmt in [r#"log("sample")"#, "assert(ok)"] {
        p.write("t.syl", &source(stmt));
        let output = p.run("", &["check", "t.syl"]);
        failure(&output);
        let text = String::from_utf8(output.stderr).unwrap();
        assert!(
            text.contains("ambiguous reset") && text.contains("specify rst:"),
            "{text}"
        );
    }
    p.write(
        "t.syl",
        &source(r#"assert(rst:b,ok) log(rst:aa,"A") log(rst:b,"B")"#),
    );
    p.write("s.json", r#"[{"drive":{"a":1,"b":0,"ok":1},"clocks":["clk"]},{"drive":{"a":0,"b":1,"ok":0},"clocks":["clk"]}]"#);
    let output = p.run("", &["sim", "t.syl", "--top", "T", "--stimulus", "s.json"]);
    success(&output);
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .starts_with("[cycle 1] T: B\n[cycle 2] T: A\n")
    );
}

#[test]
fn split_emission_preserves_the_sibling_fault_reporter() {
    let project = TestProject::new();
    project.write(
        "t.syl",
        r#"
        cell Logger(clk:in Clock) {log("sample")}
        cell Guard(clk:in Clock,ok:in Bit) {assert(ok,"fault {ok}")}
        cell Top(clk:in Clock,ok:in Bit) {
            let logger = place Logger(clk:clk)
            let guard = place Guard(clk:clk,ok:ok)
        }"#,
    );
    success(&project.run("", &["emit", "t.syl", "--out", "all.sv"]));
    success(&project.run("", &["emit", "t.syl", "--out-dir", "sv"]));
    let read = |path: &str| fs::read_to_string(project.0.join(path)).unwrap();
    let split = read("sv/Guard.sv");
    let (_, guard) = split.split_once("module Guard").unwrap();
    assert!(read("all.sv").contains(&format!("module Guard{guard}")));
    assert!(guard.contains("always @(syl$report0)"));
    assert!(guard.contains("syl$report0 <= "));
}
