use super::*;
struct Fifo<'a> {
    sim: Simulator<'a>,
    stream: bool,
    q: VecDeque<u64>,
    writes: usize,
    reads: usize,
    both: usize,
    overflow: usize,
    underflow: usize,
}
impl Fifo<'_> {
    fn inputs(&mut self, push: bool, pop: bool, data: u64) {
        let (p, r, d) = if self.stream {
            ("up_valid", "down_ready", "up_payload")
        } else {
            ("push", "pop", "wdata")
        };
        drive(&mut self.sim, p, push.into());
        drive(&mut self.sim, r, pop.into());
        drive(&mut self.sim, d, data);
    }
    fn check(&mut self) {
        self.sim.eval_comb().unwrap();
        let reset = self.sim.read_named("rst").unwrap().as_u64() == Some(1);
        if self.stream {
            expect(&self.sim, "up_ready", (self.q.len() < 8 && !reset).into());
            expect(
                &self.sim,
                "down_valid",
                (!self.q.is_empty() && !reset).into(),
            );
        } else {
            expect(&self.sim, "empty", self.q.is_empty().into());
            expect(&self.sim, "full", (self.q.len() == 8).into());
            expect(&self.sim, "count", self.q.len() as u64);
        }
        expect(
            &self.sim,
            if self.stream { "down_payload" } else { "rdata" },
            self.q.front().copied().unwrap_or(0),
        );
    }
    fn step(&mut self, push: bool, pop: bool, data: u64) -> bool {
        self.inputs(push, pop, data);
        self.check();
        let write = push && self.q.len() < 8;
        let read = pop && !self.q.is_empty();
        self.overflow += usize::from(push && !write);
        self.underflow += usize::from(pop && !read);
        self.both += usize::from(write && read);
        self.reads += usize::from(read);
        self.writes += usize::from(write);
        if read {
            self.q.pop_front();
        }
        if write {
            self.q.push_back(data & 255);
        }
        tick(&mut self.sim);
        self.check();
        write
    }
    fn drain(&mut self) {
        while !self.q.is_empty() {
            self.step(false, true, 0);
        }
    }
}
#[test]
fn fifo_and_stream_queue_boundary_reference() {
    for top in ["SyncFifo", "StreamSyncFifo"] {
        let d = design(top, include_str!("../../../../../examples/std/fifo.syl"));
        let mut f = Fifo {
            sim: Simulator::new(&d),
            stream: top == "StreamSyncFifo",
            q: VecDeque::new(),
            writes: 0,
            reads: 0,
            both: 0,
            overflow: 0,
            underflow: 0,
        };
        f.inputs(false, false, 0);
        reset(&mut f.sim);
        f.check();
        for _ in 0..3 {
            f.step(false, true, 0);
        }
        for i in 0..8 {
            f.step(true, false, i * 17 + 5);
        }
        for _ in 0..3 {
            f.step(true, false, 0xee);
        }
        f.step(true, true, 0xa5);
        f.drain();
        f.step(true, true, 0x63);
        f.step(false, true, 0);
        for i in 0..4 {
            f.step(true, false, i + 0x20);
        }
        for i in 0..64 {
            f.step(true, true, i + 0x40);
        }
        f.drain();
        let mut r = Random::new(0xf1f03b);
        let mut stalled = false;
        let mut data = 0;
        for i in 0..512 {
            let phase = (i / 64) % 3;
            let push = stalled || r.next() % 4 < if phase == 0 { 3 } else { 2 };
            let pop = r.next() % 4 < if phase == 1 { 3 } else { 2 };
            if !stalled {
                data = u64::from(r.next() as u8);
            }
            stalled = !f.step(push, pop, data) && push;
        }
        while stalled {
            stalled = !f.step(true, true, data);
        }
        f.drain();
        f.step(false, true, 0);
        assert!(
            f.writes > 64
                && f.reads == f.writes
                && f.both >= 64
                && f.overflow >= 4
                && f.underflow >= 4
        );
        for i in 0..4 {
            f.step(true, false, 0x70 + i);
        }
        f.inputs(true, true, 255);
        drive(&mut f.sim, "rst", 1);
        tick(&mut f.sim);
        f.q.clear();
        f.check();
        f.inputs(false, false, 0);
        drive(&mut f.sim, "rst", 0);
        f.check();
        for i in 0..8 {
            f.step(true, false, 0x80 + i);
        }
        f.drain();
    }
}
