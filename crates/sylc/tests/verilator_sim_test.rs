use std::{
    env, fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn verilator_alu_matches_wrapping_arithmetic_and_bundle_layout() {
    simulate(
        "comb_alu.syl",
        "CombAlu32",
        &[],
        r#"
#include "VCombAlu32.h"
#include "verilated.h"
#include <cstdint>
#include <iostream>

int main() {
    VCombAlu32 dut;
    unsigned checks = 0;
    auto check = [&](uint32_t op, uint32_t a, uint32_t b) {
        // AluReq is {op: 2 bits, a: 32 bits, b: 32 bits}, MSB first.
        dut.req[0] = b;
        dut.req[1] = a;
        dut.req[2] = op;
        dut.eval();
        const uint32_t expected = op == 0 ? a + b : op == 1 ? a ^ b : a;
        // AluRsp is {data: 32 bits, zero: 1 bit}.
        const uint32_t data = static_cast<uint32_t>(dut.rsp >> 1);
        const bool zero = (dut.rsp & 1) != 0;
        ++checks;
        if (data != expected || zero != (expected == 0)) {
            std::cerr << "ALU check " << checks << ": op=" << op
                      << " a=" << a << " b=" << b << " expected=" << expected
                      << " data=" << data << " zero=" << zero << '\n';
            return false;
        }
        return true;
    };
    const uint32_t edges[] = {0, 1, 2, 0x7fffffff, 0x80000000, 0xfffffffe, 0xffffffff};
    for (uint32_t op = 0; op < 3; ++op)
        for (uint32_t a : edges)
            for (uint32_t b : edges)
                if (!check(op, a, b)) return 1;
    uint32_t seed = 0x12345678;
    for (unsigned i = 0; i < 512; ++i) {
        seed = seed * 1664525u + 1013904223u;
        const uint32_t a = seed;
        seed = seed * 1664525u + 1013904223u;
        for (uint32_t op = 0; op < 3; ++op)
            if (!check(op, a, seed)) return 1;
    }
    dut.final();
    std::cout << "PASS: ALU " << checks << " input/output checks\n";
}
"#,
    );
}

#[test]
fn verilator_counter_matches_synchronous_reset_enable_and_clear() {
    simulate(
        "counter.syl",
        "Counter",
        &["-GW=4"],
        r#"
#include "VCounter.h"
#include "verilated.h"
#include <cstdint>
#include <iostream>

int main() {
    VCounter dut;
    uint32_t expected = 0;
    unsigned cycles = 0;
    bool initialized = false;
    auto check = [&](const char* phase) {
        if (dut.value == expected) return true;
        std::cerr << "counter cycle " << cycles << ' ' << phase
                  << ": rst=" << unsigned(dut.rst)
                  << " clear=" << unsigned(dut.clear)
                  << " enable=" << unsigned(dut.enable)
                  << " expected=" << expected << " actual=" << unsigned(dut.value) << '\n';
        return false;
    };
    auto cycle = [&](bool reset, bool enable, bool clear) {
        ++cycles;
        dut.clk = 0;
        dut.rst = reset;
        dut.enable = enable;
        dut.clear = clear;
        dut.eval();
        // Changing inputs (including reset) must not change state before posedge.
        if (initialized && !check("before rising edge")) return false;
        expected = reset || clear ? 0 : enable ? (expected + 1) & 15u : expected;
        dut.clk = 1;
        dut.eval();
        initialized = true;
        if (!check("rising edge")) return false;
        // Multiple evaluations at the same clock level cannot count twice.
        dut.eval();
        if (!check("held high")) return false;
        dut.clk = 0;
        dut.eval();
        return check("falling edge");
    };
    if (!cycle(true, false, false)) return 1;
    for (unsigned i = 0; i < 35; ++i)
        if (!cycle(false, true, false)) return 1; // Two four-bit wraparounds.
    if (!cycle(false, false, false)) return 1; // Hold a nonzero value.
    if (!cycle(true, true, false)) return 1;   // Synchronous reset beats enable.
    if (!cycle(false, true, false)) return 1;
    if (!cycle(false, true, true)) return 1;  // Clear beats enable.
    if (!cycle(false, false, true)) return 1; // Clear without enable.
    for (unsigned i = 0; i < 128; ++i)
        if (!cycle(i % 41 == 40, i % 3 != 0, i % 17 == 16)) return 1;
    dut.final();
    std::cout << "PASS: counter " << cycles << " clock cycles\n";
}
"#,
    );
}

fn simulate(example: &str, top: &str, parameters: &[&str], testbench: &str) {
    if !tool_available("verilator") {
        eprintln!("SKIP {top}: `verilator` is not installed or is not on PATH");
        return;
    }
    let Some(cxx) = ["c++", "g++", "clang++"]
        .into_iter()
        .find(|tool| tool_available(tool))
    else {
        eprintln!("SKIP {top}: no C++ compiler found (tried c++, g++, clang++)");
        return;
    };

    let work = SimulationDir::new(top);
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/mvp")
        .join(example)
        .canonicalize()
        .expect("simulation example must exist");
    let sv = work.0.join("design.sv");
    let cpp = work.0.join("testbench.cpp");
    let obj = work.0.join("obj_dir");
    fs::write(&cpp, testbench).expect("testbench must be writable");

    run(
        Command::new(env!("CARGO_BIN_EXE_sylc"))
            .current_dir(&work.0)
            .arg("--out")
            .arg(&sv)
            .arg(source),
        "sylc compilation",
    );
    run(
        Command::new("verilator")
            .current_dir(&work.0)
            .args(["--cc", "--exe", "--build", "--sv", "--top-module", top])
            .args(parameters)
            .arg("--Mdir")
            .arg(&obj)
            .args(["-o", "simulation", "-MAKEFLAGS"])
            .arg(format!("CXX={cxx}"))
            .arg(&sv)
            .arg(&cpp),
        "Verilator C++ build",
    );
    let output = run(
        Command::new(obj.join("simulation")).current_dir(&work.0),
        "functional simulation",
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.starts_with("PASS:"),
        "simulation did not complete: {stdout}"
    );
    eprintln!("{top}: {}", stdout.trim());
}

fn tool_available(tool: &str) -> bool {
    let version = if tool == "iverilog" {
        "-V"
    } else {
        "--version"
    };
    match Command::new(tool).arg(version).output() {
        Ok(output) => {
            assert!(
                output.status.success(),
                "{tool} {version} failed: {output:?}"
            );
            true
        }
        Err(error) if error.kind() == ErrorKind::NotFound => false,
        Err(error) => panic!("could not probe {tool}: {error}"),
    }
}

fn run(command: &mut Command, stage: &str) -> Output {
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("{stage}: could not launch {command:?}: {error}"));
    assert!(
        output.status.success(),
        "{stage} failed ({command:?}, {})\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

struct SimulationDir(PathBuf);

impl SimulationDir {
    fn new(top: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must follow Unix epoch")
            .as_nanos();
        let path = env::temp_dir().join(format!("syl-sim-{top}-{}-{nonce}", std::process::id()));
        fs::create_dir(&path).expect("simulation scratch directory must be creatable");
        Self(path)
    }
}

impl Drop for SimulationDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[path = "verilator_sim_test/native_diff_test.rs"]
mod native_diff_test;

#[path = "verilator_sim_test/sv_contracts_test.rs"]
mod sv_contracts_test;
