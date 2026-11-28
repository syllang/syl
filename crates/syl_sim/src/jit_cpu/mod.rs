//! Native deterministic trajectory loop. Loop parameters carry register state.
//! The flat plane holds indexed RAM and the final observable results.
mod liveness;
mod lower;
use crate::{
    AssertId, FastProgram, HwResetPolarity, RuntimeError, SimDesign, SingleTraceResult,
    batch_run::Ports,
};
use cranelift_codegen::{
    ir::{AbiParam, InstBuilder, MemFlags, Value, condcodes::IntCC, types::I64},
    settings::{self, Configurable},
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Module, default_libcall_names};
use lower::Frame;

type Run = unsafe extern "C" fn(*mut u64, u64, u64);
fn offset(slot: usize) -> i32 {
    (8 * slot) as i32
}
fn invalid(e: impl std::fmt::Display) -> RuntimeError {
    RuntimeError::Invalid(format!("cpu-jit: {e}"))
}

/// Native executable for one immutable design. Each invocation starts from zero state.
pub struct JitProgram {
    module: Option<JITModule>,
    run: Run,
    slots: usize,
    outputs: Vec<(usize, u64)>,
}

// SAFETY: construction uses only Cranelift's system memory provider and
// default libcalls. After finalization the module is private and accessed
// exclusively in Drop. Its executable mappings may run on any host thread,
// and each invocation uses a fresh, disjoint plane.
unsafe impl Send for JitProgram {}
unsafe impl Sync for JitProgram {}

impl Drop for JitProgram {
    fn drop(&mut self) {
        // SAFETY: run is private and cannot outlive the module.
        unsafe { self.module.take().unwrap().free_memory() }
    }
}

impl JitProgram {
    pub fn new(design: &SimDesign) -> Result<Self, RuntimeError> {
        Self::compile(design, &FastProgram::new(design)?)
    }
    pub(crate) fn compile(d: &SimDesign, p: &FastProgram) -> Result<Self, RuntimeError> {
        if p.slots > i32::MAX as usize / 8 - 2 {
            return Err(invalid("state exceeds native address displacement"));
        }
        for &op in p.comb.iter().chain(&p.edge) {
            use crate::fast_cpu::kernel::FastOp::{Gather, Scatter};
            if op
                .operands()
                .into_iter()
                .flatten()
                .chain(op.dst())
                .any(|s| s >= p.slots)
                || matches!(op, Gather {base,depth,..} | Scatter {base,depth,..}
                    if base.checked_add(depth).is_none_or(|end| end > p.slots))
            {
                return Err(invalid("invalid word-plane address"));
            }
        }
        let mut flags = settings::builder();
        flags.set("opt_level", "speed").map_err(invalid)?;
        let isa = cranelift_native::builder()
            .map_err(invalid)?
            .finish(settings::Flags::new(flags))
            .map_err(invalid)?;
        let mut module = JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()));
        let ptr = module.target_config().pointer_type();
        if ptr != I64 {
            return Err(invalid("64-bit host required"));
        }
        let mut ctx = module.make_context();
        ctx.func.signature = module.make_signature();
        ctx.func
            .signature
            .params
            .extend([ptr, I64, I64].map(AbiParam::new));
        let func = module
            .declare_anonymous_function(&ctx.func.signature)
            .map_err(invalid)?;
        let ports = Ports::new(d);
        let outputs = ports
            .outputs
            .iter()
            .map(|&(id, limb)| {
                let (base, w) = p.signals[id.0];
                (
                    base + limb as usize,
                    u64::MAX >> (64 - (w - 64 * limb).min(64)),
                )
            })
            .collect::<Vec<_>>();
        let slots_of = |id: crate::SignalId| {
            let (base, w) = p.signals[id.0];
            base..base + w.div_ceil(64) as usize
        };
        let mut state = d
            .regs
            .iter()
            .map(|r| r.signal)
            .chain(
                d.memories
                    .iter()
                    .flat_map(|m| m.sync_reads.iter().map(|r| r.target.signal)),
            )
            .flat_map(slots_of)
            .collect::<Vec<_>>();
        state.sort_unstable();
        state.dedup();
        let mut live = vec![0; p.slots];
        for &(slot, mask) in &outputs {
            live[slot] |= mask;
        }
        let final_ops = liveness::trace(p.comb.iter(), &mut live);
        let mut observed = state
            .iter()
            .copied()
            .chain(ports.inputs.iter().flat_map(|i| slots_of(i.id)))
            .filter(|&s| live[s] != 0)
            .collect::<Vec<_>>();
        observed.sort_unstable();
        observed.dedup();
        live.fill(0);
        for &s in state.iter().chain(&observed) {
            live[s] = u64::MAX;
        }
        let cycle_ops = liveness::trace(p.comb.iter().chain(&p.edge), &mut live);
        let mut context = FunctionBuilderContext::new();
        let mut b = FunctionBuilder::new(&mut ctx.func, &mut context);
        let entry = b.create_block();
        let cycle_block = b.create_block();
        let publish = b.create_block();
        let finish = b.create_block();
        // Bound simultaneous loop arguments on register-poor hosts. Keep
        // remaining storage in its assigned plane words across iterations.
        let (carried, spilled) = state.split_at(state.len().min(8));
        b.append_block_params_for_function_params(entry);
        for _ in 0..3 + carried.len() {
            b.append_block_param(cycle_block, I64);
        }
        b.switch_to_block(entry);
        let [plane, seed, cycles] = b.block_params(entry) else {
            unreachable!()
        };
        let (plane, seed, cycles) = (*plane, *seed, *cycles);
        let zero = b.ins().iconst(I64, 0);
        let one = b.ins().iconst(I64, 1);
        let none = b.ins().iconst(I64, -1);
        let mut initial = vec![one.into(), zero.into(), none.into()];
        initial.resize(3 + carried.len(), zero.into());
        b.ins().brif(cycles, cycle_block, &initial, finish, &[]);
        b.switch_to_block(cycle_block);
        let args = b.block_params(cycle_block);
        let (cycle, prev_cycle, prev_id) = (args[0], args[1], args[2]);
        let mut values = vec![zero; p.slots];
        for (&s, &v) in carried.iter().zip(&args[3..]) {
            values[s] = v;
        }
        for &s in spilled {
            values[s] = b.ins().load(I64, MemFlags::trusted(), plane, offset(s));
        }
        let mut frame = Frame {
            b: &mut b,
            plane,
            values,
            failed: none,
        };
        if let Some(clk) = p.clock {
            frame.values[clk] = one;
        }
        drive(&mut frame, p, &ports, seed, cycle);
        frame.execute(&cycle_ops);
        let failed = frame.b.ins().icmp_imm(IntCC::NotEqual, frame.failed, -1);
        let unset = frame.b.ins().icmp_imm(IntCC::Equal, prev_cycle, 0);
        let first = frame.b.ins().band(failed, unset);
        let id = frame.b.ins().select(first, frame.failed, prev_id);
        let fail_cycle = frame.b.ins().select(first, cycle, prev_cycle);
        let last = frame.b.ins().icmp(IntCC::Equal, cycle, cycles);
        let next = frame.b.ins().iadd_imm(cycle, 1);
        let mut again = vec![next.into(), fail_cycle.into(), id.into()];
        again.extend(
            carried
                .iter()
                .map(|&s| cranelift_codegen::ir::BlockArg::from(frame.read(s))),
        );
        for &s in spilled {
            frame.store(s, frame.read(s));
        }
        frame.b.ins().brif(last, publish, &[], cycle_block, &again);
        frame.b.switch_to_block(publish);
        for &s in &observed {
            if spilled.binary_search(&s).is_err() {
                frame.store(s, frame.read(s));
            }
        }
        frame.store(p.slots, fail_cycle);
        frame.store(p.slots + 1, id);
        frame.b.ins().jump(finish, &[]);
        b.switch_to_block(finish);
        let mut values = vec![zero; p.slots];
        for &s in &observed {
            values[s] = b.ins().load(I64, MemFlags::trusted(), plane, offset(s));
        }
        let mut frame = Frame {
            b: &mut b,
            plane,
            values,
            failed: none,
        };
        frame.execute(&final_ops);
        for &(s, _) in &outputs {
            let v = frame.read(s);
            frame.store(s, v);
        }
        b.ins().return_(&[]);
        b.seal_all_blocks();
        b.finalize();
        let linked = match module.define_function(func, &mut ctx) {
            Ok(()) => module.finalize_definitions(),
            err => err,
        };
        if let Err(e) = linked {
            // SAFETY: no entry point has been published.
            unsafe {
                module.free_memory();
            }
            return Err(invalid(e));
        }
        // SAFETY: emitted signature uses the host C ABI with (pointer, u64, u64).
        let run =
            unsafe { std::mem::transmute::<*const u8, Run>(module.get_finalized_function(func)) };
        Ok(Self {
            module: Some(module),
            run,
            slots: p.slots,
            outputs,
        })
    }

    pub fn run_trajectory(&self, seed: u64, lane: usize, cycles: u64) -> SingleTraceResult {
        self.simulator().run_trajectory(seed, lane, cycles)
    }

    pub fn simulator(&self) -> JitCpuSimulator<'_> {
        JitCpuSimulator {
            program: self,
            plane: vec![0; self.slots + 2],
        }
    }
}

/// Reusable workspace for independent invocations of a native trajectory.
pub struct JitCpuSimulator<'a> {
    program: &'a JitProgram,
    plane: Vec<u64>,
}
impl JitCpuSimulator<'_> {
    pub fn run_trajectory(&mut self, seed: u64, lane: usize, cycles: u64) -> SingleTraceResult {
        self.plane.fill(0);
        let p = self.program;
        // SAFETY: static offsets were checked at compilation, dynamic RAM
        // indices are guarded, and this invocation exclusively owns the plane.
        unsafe {
            (p.run)(
                self.plane.as_mut_ptr(),
                seed.wrapping_add((lane as u64).rotate_left(32)),
                cycles,
            );
        }
        SingleTraceResult {
            outputs: p
                .outputs
                .iter()
                .map(|&(s, mask)| self.plane[s] & mask)
                .collect(),
            failure: (self.plane[p.slots] != 0).then(|| {
                (
                    self.plane[p.slots],
                    AssertId(self.plane[p.slots + 1] as usize),
                )
            }),
        }
    }
}

fn drive(frame: &mut Frame, p: &FastProgram, ports: &Ports, seed: Value, cycle: Value) {
    let key = frame.b.ins().iadd(seed, cycle);
    for input in &ports.inputs {
        for limb in 0..input.width.div_ceil(64) {
            let v = if let Some(polarity) = input.reset {
                let cmp = if polarity == HwResetPolarity::ActiveHigh {
                    IntCC::Equal
                } else {
                    IntCC::NotEqual
                };
                let v = frame.b.ins().icmp_imm(cmp, cycle, 1);
                frame.b.ins().uextend(I64, v)
            } else {
                let salt = 0x9e3779b97f4a7c15u64.wrapping_mul(input.base + u64::from(limb) + 1);
                let mut z = frame.b.ins().iadd_imm(key, salt as i64);
                for (shift, k) in [(30, 0xbf58476d1ce4e5b9u64), (27, 0x94d049bb133111ebu64)] {
                    let hi = frame.b.ins().ushr_imm(z, shift);
                    z = frame.b.ins().bxor(z, hi);
                    z = frame.b.ins().imul_imm(z, k as i64);
                }
                let hi = frame.b.ins().ushr_imm(z, 31);
                z = frame.b.ins().bxor(z, hi);
                let mask = u64::MAX >> (64 - (input.width - 64 * limb).min(64));
                frame.b.ins().band_imm(z, mask as i64)
            };
            frame.values[p.signals[input.id.0].0 + limb as usize] = v;
        }
    }
}
