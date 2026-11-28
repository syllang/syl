use std::{collections::BTreeMap, fmt::Write, fs, path::Path};
use syl_elab::ElaborationOutput;
use syl_sim::{HwDirection, ParamValue, SimDesign};

pub(super) fn emit(
    d: &SimDesign,
    ir: &ElaborationOutput,
    top: &str,
    params: &BTreeMap<String, ParamValue>,
    dir: &Path,
) {
    fs::write(
        dir.join("design.sv"),
        syl_emit::SystemVerilogBackend::new()
            .emit(ir.hwir().unwrap())
            .unwrap(),
    )
    .unwrap();
    let mut drive = String::new();
    let mut digest = String::new();
    let mut base = 0;
    let mut index = 0;
    for p in d.top_ports() {
        let width = d.signal(p.signal).width;
        let name = &p.name;
        for limb in 0..width.div_ceil(64) {
            let bits = (width - 64 * limb).min(64);
            let lo = 2 * limb;
            let hi = lo + 1;
            if p.direction == HwDirection::In && name != "clk" {
                let v = if name == "rst" {
                    "(cycle == 1)".into()
                } else {
                    format!(
                        "(random_word(cycle, {}) & {}ULL)",
                        base + limb,
                        u64::MAX >> (64 - bits)
                    )
                };
                if width <= 64 {
                    writeln!(drive, "d.{name} = {v};").unwrap();
                } else {
                    writeln!(
                        drive,
                        "{{ const uint64_t v = {v}; d.{name}[{lo}] = uint32_t(v);"
                    )
                    .unwrap();
                    if bits > 32 {
                        writeln!(drive, "d.{name}[{hi}] = uint32_t(v >> 32);").unwrap();
                    }
                    drive.push_str("}\n");
                }
            } else if p.direction == HwDirection::Out {
                let v = if width <= 64 {
                    format!("uint64_t(d.{name})")
                } else if bits > 32 {
                    format!("(uint64_t(d.{name}[{lo}]) | uint64_t(d.{name}[{hi}]) << 32)")
                } else {
                    format!("uint64_t(d.{name}[{lo}])")
                };
                writeln!(digest, "sum += {v} * {}ULL;", 2 * index + 1).unwrap();
                index += 1;
            }
        }
        base += width.div_ceil(64);
    }
    fs::write(
        dir.join("harness.cpp"),
        include_str!("harness.cpp")
            .replace("@DRIVE@", &drive)
            .replace("@DIGEST@", &digest),
    )
    .unwrap();
    print!("{top}");
    for (k, v) in params {
        let ParamValue::Nat(n) = v else { panic!() };
        print!(" -G{k}={n}");
    }
    println!();
}
