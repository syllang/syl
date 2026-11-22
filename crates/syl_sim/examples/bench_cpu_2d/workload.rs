use std::collections::BTreeMap;
use syl_elab::{ElaborationOutput, HardwareCompiler};
use syl_sema::{SemanticSession, SemanticSourceFile};
use syl_sim::{HwDirection, ParamValue, SignalId, SimDesign, Value};
use syl_span::SourceId;
use syl_syntax::SourceParser;

pub const FAULT: &str = "cell Fault<D: Domain>(clk: in Clock<D>, rst: in Reset<D>, inject: in Bit, i: in Bit) -> y: Bit { reg q: Bit reset(rst,0) next q := i assert(not inject) y := q }";
pub fn fixture(
    case: &str,
) -> (
    ElaborationOutput,
    &'static str,
    BTreeMap<String, ParamValue>,
) {
    let (top, src) = match case.split('-').next().unwrap() {
        "LaneArray" => (
            "LaneArray",
            include_str!("../../../../examples/mvp/lane_array.syl"),
        ),
        "SyncFifo" => (
            "SyncFifo",
            include_str!("../../../../examples/std/fifo.syl"),
        ),
        "SimpleDualPortRam" => (
            "SimpleDualPortRam",
            include_str!("../../../../examples/std/mem.syl"),
        ),
        "RoundRobinArbiter4" => (
            "RoundRobinArbiter4",
            include_str!("../../../../examples/std/arbiter.syl"),
        ),
        "Fault" => ("Fault", FAULT),
        _ => panic!("unknown fixture {case}"),
    };
    let mut params = BTreeMap::new();
    if let Some((_, n)) = case.split_once("-N") {
        params.insert("N".into(), ParamValue::Nat(n.parse().unwrap()));
    }
    if case == "SyncFifo-D64" {
        for (k, v) in [("DEPTH", 64), ("ADDR_W", 6), ("CNT_W", 7)] {
            params.insert(k.into(), ParamValue::Nat(v));
        }
    }
    let sources = [
        (
            "std.stream",
            include_str!("../../../../examples/std/stream.syl"),
        ),
        ("fixture", src),
    ];
    let ast: Vec<_> = sources
        .iter()
        .enumerate()
        .map(|(i, (_, s))| {
            SourceParser::new_in(s, SourceId::new(i))
                .parse_file()
                .unwrap()
        })
        .collect();
    let files = sources
        .iter()
        .zip(&ast)
        .map(|((p, _), a)| SemanticSourceFile::new(p.split('.').map(str::to_owned).collect(), a))
        .collect();
    let hir = SemanticSession::new_sources(files).resolve_hir().unwrap();
    let tir = hir.check_tir().unwrap();
    (HardwareCompiler::new().output_for_tir(&tir), top, params)
}

#[derive(Clone, Copy)]
pub struct Port {
    pub id: SignalId,
    pub width: u32,
    pub key: u64,
    pub reset: bool,
    pub inject: bool,
}
pub fn ports(d: &SimDesign, dir: HwDirection) -> Vec<Port> {
    d.top_ports()
        .iter()
        .enumerate()
        .filter(|(_, p)| p.direction == dir && p.name != "clk")
        .map(|(i, p)| Port {
            id: p.signal,
            width: d.signal(p.signal).width,
            key: i as u64,
            reset: p.name == "rst",
            inject: p.name == "inject",
        })
        .collect()
}
pub fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e3779b97f4a7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}
pub fn stimulus(p: Port, word: u32, lane: usize, cycle: u64) -> u64 {
    if p.reset {
        return u64::from(cycle == 1);
    }
    if p.inject {
        return u64::from(lane == 3073 && cycle == 500);
    }
    let key = (lane as u64).wrapping_mul(0x517cc1b727220a95)
        ^ cycle.wrapping_mul(0x5851f42d4c957f2d)
        ^ (p.key << 32)
        ^ u64::from(word);
    mix(key) & (u64::MAX >> (64 - (p.width - word * 64).min(64)))
}
pub fn scalar_value(p: Port, lane: usize, cycle: u64) -> Value {
    if p.width <= 64 {
        return Value::from_u64(p.width, stimulus(p, 0, lane, cycle));
    }
    let parts: Vec<_> = (0..p.width.div_ceil(64))
        .rev()
        .map(|w| Value::from_u64((p.width - w * 64).min(64), stimulus(p, w, lane, cycle)))
        .collect();
    Value::concat(&parts).unwrap()
}
