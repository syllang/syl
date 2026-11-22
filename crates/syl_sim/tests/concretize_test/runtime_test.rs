use super::*;
use std::collections::VecDeque;
#[path = "runtime_test/fifo_test.rs"]
mod fifo_test;
#[path = "runtime_test/ram_test.rs"]
mod ram_test;
#[path = "runtime_test/semantics_test.rs"]
mod semantics_test;

fn design(top: &str, src: &str) -> SimDesign {
    SimDesign::lower(
        &compile(&[("std.stream", STREAM), ("fixture", src)]),
        top,
        &BTreeMap::new(),
    )
    .unwrap()
}
fn drive(sim: &mut Simulator, name: &str, n: u64) {
    sim.drive_named(name, Value::from_u64(64, n)).unwrap();
}
fn expect(sim: &Simulator, name: &str, n: u64) {
    assert_eq!(
        sim.read_named(name).unwrap().as_u64(),
        Some(n),
        "cycle {} {name}",
        sim.cycle()
    )
}
fn tick(sim: &mut Simulator) {
    sim.tick("clk").unwrap()
}
fn reset(sim: &mut Simulator) {
    drive(sim, "rst", 1);
    tick(sim);
    tick(sim);
    drive(sim, "rst", 0);
    sim.eval_comb().unwrap();
}
// std::mt19937, matching the C++ reference benches' seeded streams.
struct Random {
    state: [u32; 624],
    index: usize,
}
impl Random {
    fn new(seed: u32) -> Self {
        let mut state = [0u32; 624];
        state[0] = seed;
        for i in 1..624 {
            let p = state[i - 1];
            state[i] = 1812433253u32
                .wrapping_mul(p ^ (p >> 30))
                .wrapping_add(i as u32);
        }
        Self { state, index: 624 }
    }
    fn next(&mut self) -> u32 {
        if self.index == 624 {
            for i in 0..624 {
                let x = (self.state[i] & 0x80000000) | (self.state[(i + 1) % 624] & 0x7fffffff);
                self.state[i] = self.state[(i + 397) % 624]
                    ^ (x >> 1)
                    ^ if x & 1 != 0 { 0x9908b0df } else { 0 };
            }
            self.index = 0;
        }
        let mut y = self.state[self.index];
        self.index += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c5680;
        y ^= (y << 15) & 0xefc60000;
        y ^ (y >> 18)
    }
}
#[test]
fn pulse_and_edge_trains() {
    let src = include_str!("../../../../examples/std/pulse.syl");
    let d = design("EdgeDetector", src);
    let mut s = Simulator::new(&d);
    drive(&mut s, "level", 0);
    reset(&mut s);
    let mut prev = 0;
    for level in [0, 1, 1, 0, 0, 1, 0] {
        drive(&mut s, "level", level);
        s.eval_comb().unwrap();
        expect(&s, "rising", (level > prev).into());
        expect(&s, "falling", (level < prev).into());
        expect(&s, "any_edge", (level != prev).into());
        tick(&mut s);
        expect(&s, "any_edge", 0);
        prev = level;
    }
    let d = design("PulseStretcher", src);
    let mut s = Simulator::new(&d);
    drive(&mut s, "trigger", 0);
    reset(&mut s);
    expect(&s, "pulse", 0);
    for i in 0..16 {
        drive(&mut s, "trigger", matches!(i, 0 | 7 | 9).into());
        tick(&mut s);
        expect(&s, "pulse", (i < 4 || (7..13).contains(&i)).into());
    }
}
#[test]
fn arbiter_grants_and_rotation() {
    let src = include_str!("../../../../examples/std/arbiter.syl");
    let d = design("FixedPriorityArbiter4", src);
    let mut s = Simulator::new(&d);
    for req in 0..16 {
        drive(&mut s, "req", req);
        s.eval_comb().unwrap();
        expect(&s, "grant", req & req.wrapping_neg());
    }
    let d = design("RoundRobinArbiter4", src);
    let mut s = Simulator::new(&d);
    reset(&mut s);
    let mut ptr = 0;
    for req in [15; 16]
        .into_iter()
        .chain([10, 0, 10].into_iter().cycle().take(24))
    {
        drive(&mut s, "req", req);
        s.eval_comb().unwrap();
        let index = (0..4).map(|n| (ptr + n) % 4).find(|i| req & (1 << i) != 0);
        expect(&s, "grant_valid", index.is_some().into());
        expect(&s, "grant", index.map_or(0, |i| 1 << i));
        if let Some(i) = index {
            expect(&s, "grant_idx", i);
            ptr = (i + 1) % 4;
        }
        tick(&mut s);
    }
}
#[test]
fn skid_stalls_and_drains() {
    let d = design(
        "WordSkidBuffer",
        include_str!("../../../../examples/std/skid.syl"),
    );
    let mut s = Simulator::new(&d);
    drive(&mut s, "up_valid", 0);
    drive(&mut s, "down_ready", 0);
    reset(&mut s);
    let mut q = VecDeque::new();
    let mut data = 0;
    let mut step = |valid: bool, ready: bool| {
        drive(&mut s, "up_valid", valid.into());
        drive(&mut s, "down_ready", ready.into());
        drive(&mut s, "up_payload", data);
        s.eval_comb().unwrap();
        let capacity = q.len() < 2;
        expect(&s, "up_ready", capacity.into());
        expect(&s, "down_valid", (!q.is_empty()).into());
        if let Some(&head) = q.front() {
            expect(&s, "down_payload", head);
            if ready {
                q.pop_front();
            }
        }
        if valid && capacity {
            q.push_back(data);
            data = (data + 1) % 256;
        }
        tick(&mut s);
        valid && capacity
    };
    for _ in 0..16 {
        step(true, true);
    }
    for _ in 0..5 {
        step(true, false);
    }
    step(true, true);
    step(true, true);
    step(false, true);
    step(false, true);
    let mut random = Random::new(307);
    let mut stalled = false;
    for _ in 0..200 {
        let valid = stalled || random.next() & 1 != 0;
        stalled = !step(valid, random.next() & 1 != 0) && valid;
    }
    while stalled {
        stalled = !step(true, true);
    }
    for _ in 0..3 {
        step(false, true);
    }
    assert!(q.is_empty());
}
#[test]
fn synchronizers_reset_between_edges() {
    for (top, r, active) in [
        ("Sync2FF", "dst_rst", 1),
        ("Sync2FFAsyncLow", "dst_rst_n", 0),
    ] {
        let d = design(top, CDC);
        let mut s = Simulator::new(&d);
        drive(&mut s, "async_in", 1);
        drive(&mut s, r, active);
        s.tick("dst_clk").unwrap();
        expect(&s, "sync_out", 0);
        drive(&mut s, r, 1 - active);
        let transition = |s: &mut Simulator, level: u64| {
            let old = s.read_named("sync_out").unwrap().as_u64().unwrap();
            drive(s, "async_in", level);
            s.eval_comb().unwrap();
            expect(s, "sync_out", old);
            for result in [old, level, level] {
                s.tick("dst_clk").unwrap();
                expect(s, "sync_out", result);
            }
        };
        for level in [1, 0, 1] {
            transition(&mut s, level);
        }
        drive(&mut s, r, active);
        s.eval_comb().unwrap();
        expect(&s, "sync_out", active);
        s.tick("dst_clk").unwrap();
        expect(&s, "sync_out", 0);
        drive(&mut s, r, 1 - active);
        s.eval_comb().unwrap();
        expect(&s, "sync_out", 0);
        transition(&mut s, 1);
    }
}
#[test]
fn pulse_synchronizer_coincident_edges() {
    let d = design("PulseSynchronizer", CDC);
    for (src, dst) in [(5u32, 7u32), (11, 3)] {
        let mut s = Simulator::new(&d);
        let sc = s.clock("src_clk").unwrap();
        let dc = s.clock("dst_clk").unwrap();
        let reset_until = 8 * src.max(dst);
        let spacing = 6 * dst.div_ceil(src) + 4;
        let (mut toggle, mut stage1, mut stage2, mut prev, mut last) =
            (false, false, false, false, false);
        let (mut sent, mut received, mut src_edges) = (0, 0, 0);
        for t in 1..=reset_until + 2 * src * spacing * 10 {
            let reset = t <= reset_until;
            drive(&mut s, "src_rst", reset.into());
            drive(&mut s, "dst_rst", reset.into());
            let old = toggle;
            let mut events = Vec::new();
            if t % src == 0 {
                if (t / src) % 2 == 1 {
                    let pulse = !reset && sent < 8 && src_edges % spacing == 0;
                    drive(&mut s, "src_pulse", pulse.into());
                    sent += u32::from(pulse);
                    src_edges += u32::from(!reset);
                    toggle = !reset && (toggle ^ pulse);
                    events.push(ClockEvent::Rising(sc));
                } else {
                    events.push(ClockEvent::Falling(sc));
                }
            }
            let dst_edge = t % dst == 0 && (t / dst) % 2 == 1;
            if t % dst == 0 {
                if dst_edge {
                    prev = !reset && stage2;
                    stage2 = !reset && stage1;
                    stage1 = !reset && old;
                    events.push(ClockEvent::Rising(dc));
                } else {
                    events.push(ClockEvent::Falling(dc));
                }
            }
            s.step_clocks(&events).unwrap();
            if !reset {
                let pulse = stage2 ^ prev;
                expect(&s, "dst_pulse", pulse.into());
                if dst_edge {
                    assert!(!(last && pulse));
                    received += u32::from(pulse);
                    last = pulse;
                }
            }
        }
        assert_eq!((sent, received), (8, 8));
    }
}

#[path = "runtime_test/edge_contracts_test.rs"]
mod edge_contracts_test;
