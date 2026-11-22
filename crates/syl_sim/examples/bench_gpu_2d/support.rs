use serde_json::{Value as Json, json};
use std::collections::BTreeMap;
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use syl_elab::{ElaborationOutput, HardwareCompiler};
use syl_sema::{SemanticSession, SemanticSourceFile};
use syl_sim::gpu_cuda::{CodegenOptions, KernelMode, compile_cuda_executable, try_generate_cuda};
use syl_sim::{HwDirection, HwResetPolarity, RuntimeError, Simulator, StateMode, Value, VcdWriter};
use syl_sim::{ParamValue, SimDesign};
use syl_span::SourceId;
use syl_syntax::SourceParser;

pub fn compile(files: &[(&str, &str)]) -> ElaborationOutput {
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

pub fn sources() -> ElaborationOutput {
    compile(&[
        (
            "std.stream",
            include_str!("../../../../examples/std/stream.syl"),
        ),
        (
            "lanes",
            include_str!("../../../../examples/mvp/lane_array.syl"),
        ),
        (
            "std.fifo",
            include_str!("../../../../examples/std/fifo.syl"),
        ),
        ("std.mem", include_str!("../../../../examples/std/mem.syl")),
        (
            "std.arbiter",
            include_str!("../../../../examples/std/arbiter.syl"),
        ),
        ("fault", include_str!("fault.syl")),
    ])
}
pub fn lower(ir: &ElaborationOutput, top: &str, size: u64) -> SimDesign {
    let mut p = BTreeMap::new();
    match top {
        "LaneArray" => {
            p.insert("N".into(), ParamValue::Nat(size));
        }
        "SyncFifo" | "SimpleDualPortRam" => {
            p.insert("DEPTH".into(), ParamValue::Nat(size));
            p.insert("ADDR_W".into(), ParamValue::Nat(size.ilog2().into()));
            if top == "SyncFifo" {
                p.insert("CNT_W".into(), ParamValue::Nat((size.ilog2() + 1).into()));
            }
        }
        _ => {}
    }
    SimDesign::lower(ir, top, &p).unwrap()
}
pub fn options(d: &SimDesign, mode: KernelMode) -> CodegenOptions {
    let family_index = d.instance_families().iter().position(|f| {
        f.members
            .first()
            .is_some_and(|id| d.instances[id.0].parent == Some(syl_sim::InstanceId(0)))
    });
    CodegenOptions { mode, family_index }
}
pub struct Temp(pub PathBuf);
impl Temp {
    pub fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "syl-gpu-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap();
        Self(dir)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
pub struct Binary {
    pub path: PathBuf,
    pub compile_ms: f64,
    pub gen_ms: f64,
}
pub fn build(dir: &Path, d: &SimDesign, mode: KernelMode) -> Binary {
    let timer = Instant::now();
    let source = try_generate_cuda(d, options(d, mode)).unwrap().source;
    let gen_ms = ms(timer.elapsed());
    let path = dir.join("simulate");
    let timer = Instant::now();
    compile_cuda_executable(&source, &path).unwrap();
    Binary {
        path,
        compile_ms: ms(timer.elapsed()),
        gen_ms,
    }
}
pub fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.
}
pub fn run(bin: &Binary, b: u32, k: u32, mode: &str) -> Json {
    let start = Instant::now();
    let out = Command::new(&bin.path)
        .args([&b.to_string(), &k.to_string(), "128", mode])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{:?}: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let wall = ms(start.elapsed());
    let mut r: Json = serde_json::from_slice(&out.stdout).unwrap();
    r["hot_wall_ms"] = json!(wall);
    r
}
fn stimulus(batch: u32, cycle: u32, slot: u64) -> u64 {
    let mut z = ((u64::from(batch) << 32) | u64::from(cycle))
        .wrapping_add(0x9e3779b97f4a7c15u64.wrapping_mul(slot + 1));
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}
pub fn drive(sim: &mut Simulator<'_>, d: &SimDesign, b: u32, c: u32) {
    let mut bases = Vec::new();
    let mut base = 0u64;
    for s in &d.signals {
        bases.push(base);
        base += u64::from(s.width.div_ceil(64));
    }
    for p in d
        .top_ports()
        .iter()
        .filter(|p| p.direction == HwDirection::In)
    {
        let width = d.signal(p.signal).width;
        let value = if d.clocks.iter().any(|clk| clk.signal == p.signal) {
            Value::zero(1)
        } else if let Some(r) = d.resets.iter().find(|r| r.signal == p.signal) {
            Value::from_u64(
                1,
                u64::from((c == 1) ^ (r.polarity == HwResetPolarity::ActiveLow)),
            )
        } else {
            let parts = (0..width.div_ceil(64))
                .rev()
                .map(|i| {
                    Value::from_u64(
                        (width - 64 * i).min(64),
                        stimulus(b, c, bases[p.signal.0] + u64::from(i)),
                    )
                })
                .collect::<Vec<_>>();
            Value::concat(&parts).unwrap()
        };
        sim.drive(p.signal, value).unwrap();
    }
}
pub fn fault(ir: &ElaborationOutput, dir: &Path) -> Json {
    let cold = Instant::now();
    let d = lower(ir, "FaultProbe", 1);
    let bin = build(dir, &d, KernelMode::BatchPerThread);
    let gpu = run(&bin, 4096, 1000, "summary");
    let detect = ms(cold.elapsed());
    let b = gpu["fail_batch"].as_u64().unwrap() as u32;
    let c = gpu["fail_cycle"].as_u64().unwrap() as u32;
    let id = gpu["fail_assert"].as_u64().unwrap() as usize;
    assert_eq!((b, c, id), (173, 38, 0));
    let replay = Instant::now();
    let file = std::fs::File::create(dir.join("failure.vcd")).unwrap();
    let mut wave = VcdWriter::new(std::io::BufWriter::new(file), &d).unwrap();
    let mut sim = Simulator::with_mode(&d, StateMode::TwoState);
    for cycle in 1..=c {
        drive(&mut sim, &d, b, cycle);
        sim.eval_comb().unwrap();
        wave.sample(2 * u64::from(cycle) - 1, &sim).unwrap();
        let edge = sim.step_clocks(&[syl_sim::ClockEvent::Rising(syl_sim::DomainId(0))]);
        wave.sample(2 * u64::from(cycle), &sim).unwrap();
        if cycle < c {
            edge.unwrap();
            sim.step_clocks(&[syl_sim::ClockEvent::Falling(syl_sim::DomainId(0))])
                .unwrap();
        } else {
            let Err(RuntimeError::Assertion(f)) = edge else {
                panic!("missing replay failure")
            };
            assert_eq!(f.cycle, u64::from(c));
            assert_eq!(f.path, d.assertions[id].source.path);
        }
    }
    wave.finish()
        .unwrap()
        .into_inner()
        .unwrap()
        .sync_all()
        .unwrap();
    let replay_ms = ms(replay.elapsed());
    json!({"case":"FaultProbe","compile_ms":bin.compile_ms,"gen_ms":bin.gen_ms,"cold_to_detection_ms":detect,"replay_vcd_ms":replay_ms,"end_to_end_ms":ms(cold.elapsed()),"assert_path":d.assertions[id].source.path,"vcd_bytes":std::fs::metadata(dir.join("failure.vcd")).unwrap().len(),"run":gpu})
}
