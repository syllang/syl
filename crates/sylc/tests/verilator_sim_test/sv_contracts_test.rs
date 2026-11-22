use super::*;
use std::{collections::BTreeMap, fmt::Write};
use syl_session::{AnalysisHost, ProjectConfig};
use syl_sim::{ExternModel, SimDesign, Simulator, Value};

/// Use the same Syl source for native elaboration and SV emission.
fn design(work: &SimulationDir, source: &str) -> SimDesign {
    let path = work.0.join("top.syl");
    fs::write(&path, source).unwrap();
    run(
        Command::new(env!("CARGO_BIN_EXE_sylc"))
            .current_dir(&work.0)
            .args(["emit", "top.syl", "--out", "top.sv"]),
        "sylc emit",
    );
    let mut host = AnalysisHost::with_config(
        ProjectConfig::builder()
            .workspace_roots(vec![work.0.clone()])
            .build(),
    );
    let snapshot = host.load(&[path]).unwrap();
    SimDesign::from_hwir(snapshot.hwir().unwrap(), "Top", &Default::default()).unwrap()
}

fn samples(output: Output) -> Vec<String> {
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .filter_map(|line| line.strip_prefix("sample ").map(str::to_owned))
        .collect()
}

fn compare_icarus(work: &SimulationDir, bench: &str, native: &[String]) {
    if tool_available("iverilog") {
        fs::write(work.0.join("tb.sv"), bench).unwrap();
        run(
            Command::new("iverilog")
                .current_dir(&work.0)
                .args(["-g2012", "-s", "tb", "-o", "rtl", "top.sv", "tb.sv"]),
            "Icarus compile",
        );
        let output = run(
            Command::new("vvp").current_dir(&work.0).arg("rtl"),
            "Icarus simulate",
        );
        assert_eq!(samples(output), native);
    }
}

struct Pad;
impl ExternModel for Pad {
    fn eval(
        &mut self,
        inputs: &BTreeMap<String, Value>,
    ) -> Result<BTreeMap<String, Value>, String> {
        Ok(BTreeMap::from([
            ("p".into(), "1".repeat(65).parse().unwrap()),
            ("echo".into(), inputs["p"].clone()),
        ]))
    }
}

#[test]
fn inout_width_changes_resolve_actual_formal_and_downstream_reader() {
    for (ty, connection, instance, expected) in [
        (
            "Bit",
            "let e=place Pad(p:p,echo:echo)",
            "e_inst",
            "1".into(),
        ),
        (
            "UInt<65>",
            "let w=place Wrap(p:p,echo:echo)",
            "w_inst.e_inst",
            format!("{}1", "z".repeat(64)),
        ),
        (
            "[2] UInt<64>",
            "let e=place Pad(p:p[1],echo:echo)",
            "e_inst",
            format!("{}{}", "1".repeat(64), "z".repeat(64)),
        ),
    ] {
        let work = SimulationDir::new("inout");
        let d = design(
            &work,
            &format!(
                "extern cell Pad(p:inout UInt<65>,echo:out UInt<65>)
            cell Wrap(p:inout Bit,echo:out UInt<65>) {{let e=place Pad(p:p,echo:echo)}}
            cell Top(p:inout {ty}, y:out {ty},echo:out UInt<65>) {{ y:=p {connection} }}"
            ),
        );
        let id = d.externs[0].instance;
        let mut s = Simulator::new(&d);
        s.register_extern(id, Pad).unwrap();
        let width = d.signal(s.port("p").unwrap()).width;
        let formal_id = d.instances[id.0].ports[0].signal;
        let mut native = Vec::new();
        for drive in [Value::z(width), Value::zero(width), Value::z(width)] {
            s.drive_named("p", drive.clone()).unwrap();
            s.eval_comb().unwrap();
            let p = s.read_named("p").unwrap();
            let y = s.read_named("y").unwrap();
            let formal = s.read(formal_id);
            let echo = s.read_named("echo").unwrap();
            assert_eq!(y, p);
            assert_eq!(echo, formal);
            if drive == Value::z(width) {
                assert_eq!(p.to_binary(), expected);
                assert_eq!(formal.to_binary(), "1".repeat(65));
            }
            native.push(format!("{p} {y} {formal} {echo}"));
        }
        let path = work.0.join("top.sv");
        let sv = fs::read_to_string(&path).unwrap();
        let (header, rest) = sv.split_once("endmodule").unwrap();
        assert!(header.contains("module Pad ("));
        fs::write(
            path,
            format!("{header}  assign p = '1; assign echo = p;\nendmodule{rest}"),
        )
        .unwrap();
        let bench = format!(
            "module tb; wire [{msb}:0] p,y; reg [{msb}:0] driver; assign p=driver; wire [64:0] echo;
             Top dut(.*); task sample; #1; $display(\"sample %b %b %b %b\",p,y,dut.{instance}.p,echo); endtask
             initial begin driver='z;sample;driver='0;sample;driver='z;sample;end endmodule",
            msb = width-1
        );
        compare_icarus(&work, &bench, &native);
    }
}

#[test]
fn asynchronous_reset_sensitivity_and_branch_match_four_state_sv() {
    for (polarity, active, inactive) in [("ActiveHigh", "1", "0"), ("ActiveLow", "0", "1")] {
        let work = SimulationDir::new(polarity);
        let d = design(
            &work,
            &format!(
                "cell Top(clk:in Clock,rst:in Reset<Async,{polarity}>,i:in Bit)->y:Bit {{
            reg q:Bit reset(rst,0) next q:=i y:=q }}"
            ),
        );
        let mut s = Simulator::new(&d);
        s.drive_named("clk", Value::zero(1)).unwrap();
        let mut native = Vec::new();
        let mut bench =
            String::from("module tb; reg clk,rst,i; wire y; Top dut(.*); initial begin clk=0;\n");
        for (rst, i, y) in [
            (inactive, 1, "x"),
            (active, 1, "0"),
            (inactive, 1, "0"),
            ("x", 1, "1"),
            ("z", 0, "1"),
            (active, 1, "0"),
            (active, 1, "0"),
            (inactive, 1, "0"),
            ("z", 1, "1"),
            ("x", 0, "1"),
            (inactive, 0, "1"),
            ("z", 0, "0"),
        ] {
            s.drive_named("rst", rst.parse().unwrap()).unwrap();
            s.drive_named("i", Value::from_u64(1, i)).unwrap();
            s.eval_comb().unwrap();
            let value = s.read_named("y").unwrap().to_binary();
            assert_eq!(value, y, "{polarity} rst={rst}");
            native.push(value);
            writeln!(bench, "i={i};rst=1'b{rst};#1;$display(\"sample %b\",y);").unwrap();
        }
        bench.push_str("end endmodule");
        compare_icarus(&work, &bench, &native);
    }
}

#[test]
fn decimal_arithmetic_matches_icarus_and_verilator_at_four_widths() {
    let expressions = [
        "(-3)/2",
        "(-7)%3",
        "(-3)/u",
        "((-3)/2)+u",
        "(-7)%u",
        "7/(-3)",
        "7%(-3)",
        "((-3)/2)<<u",
        "2147483648/2",
        "4294967295/2",
        "((-3)/(2+u))",
        "select priority {i=>(-3)/2,default=>0,}",
    ];
    let work = SimulationDir::new("decimal");
    let mut ports = String::from("cell Top(i:in Bit,u:in UInt<8>");
    let mut assignments = String::new();
    let mut bench = String::from("module tb; reg i; reg [7:0] u;\n");
    let mut checks = String::new();
    let mut names = Vec::new();
    for (index, expr) in expressions.iter().enumerate() {
        for width in [8, 32, 65, 130] {
            let name = format!("q{index}_{width}");
            write!(ports, ",{name}:out UInt<{width}>").unwrap();
            writeln!(assignments, "{name}:={expr}").unwrap();
            writeln!(bench, "wire [{}:0] {name};", width - 1).unwrap();
            writeln!(checks, "$display(\"sample {name}=%b\",{name});").unwrap();
            names.push(name);
        }
    }
    let d = design(&work, &format!("{ports}){{\n{assignments}}}"));
    let mut s = Simulator::new(&d);
    s.drive_named("i", Value::from_u64(1, 1)).unwrap();
    s.drive_named("u", Value::from_u64(8, 2)).unwrap();
    s.eval_comb().unwrap();
    assert_eq!(s.read_named("q8_32").unwrap().as_u64(), Some(0x4000_0000));
    assert_eq!(s.read_named("q9_8").unwrap().as_u64(), Some(0xff));
    let native: Vec<_> = names
        .iter()
        .map(|n| format!("{n}={}", s.read_named(n).unwrap()))
        .collect();
    write!(
        bench,
        "Top dut(.*);initial begin i=1;u=2;#1;\n{checks}$finish;end endmodule"
    )
    .unwrap();
    compare_icarus(&work, &bench, &native);
    if tool_available("verilator") {
        fs::write(work.0.join("tb.sv"), bench).unwrap();
        run(
            Command::new("verilator").current_dir(&work.0).args([
                "--binary",
                "--timing",
                "-Wno-fatal",
                "--top-module",
                "tb",
                "top.sv",
                "tb.sv",
            ]),
            "Verilator compile",
        );
        let output = run(
            Command::new(work.0.join("obj_dir/Vtb")).current_dir(&work.0),
            "Verilator simulate",
        );
        assert_eq!(samples(output), native);
    }
}
