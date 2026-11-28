mod order;
use super::{FastOp as Op, FastProgram, mask};
use crate::runtime::signed;
use crate::{
    HwBinaryOp as B, HwDirection, HwResetMode, HwResetPolarity, HwUnaryOp, RuntimeError, Selector,
    SimDesign, SimExpr as E, SimTarget,
};
use std::collections::BTreeMap;

type Result<T> = std::result::Result<T, RuntimeError>;
fn invalid(s: &str) -> RuntimeError {
    RuntimeError::Invalid(s.into())
}

// A view can start midway through a limb. Fixed slices are free until materialized.
#[derive(Clone, Copy)]
struct View {
    slot: usize,
    low: u32,
    width: u32,
}
impl View {
    fn new(slot: usize, width: u32) -> Self {
        Self {
            slot,
            low: 0,
            width,
        }
    }
    fn slice(self, low: u32, width: u32) -> Self {
        let bit = self.low + low;
        Self {
            slot: self.slot + (bit / 64) as usize,
            low: bit % 64,
            width,
        }
    }
    fn limb(self, low: u32, width: u32, zero: View) -> View {
        if low < self.width {
            self.slice(low, (self.width - low).min(width))
        } else {
            zero
        }
    }
}

struct Builder<'a> {
    d: &'a SimDesign,
    signals: Vec<View>,
    mem: Vec<usize>,
    next: usize,
    peak: usize,
    ops: Vec<Op>,
    zero: View,
}
impl Builder<'_> {
    fn alloc(&mut self, w: u32) -> View {
        let v = View::new(self.next, w);
        self.next += w.div_ceil(64) as usize;
        self.peak = self.peak.max(self.next);
        v
    }
    fn const_word(&mut self, x: u64, w: u32) -> View {
        if x == 0 {
            return View::new(self.zero.slot, w.min(64));
        }
        let v = self.alloc(w);
        self.ops.push(Op::Const(v.slot, x));
        if w > 64 {
            // A u64 decimal literal with bit 63 set has a 65th, positive sign bit.
            self.ops.push(Op::Const(v.slot + 1, 0));
        }
        v
    }
    fn target(&self, t: &SimTarget) -> Result<View> {
        let mut v = self.signals[t.signal.0];
        if v.width == 0 {
            return Err(invalid("cross-family signal dependency"));
        }
        for s in &t.selectors {
            v = self.select(v, s)?;
        }
        Ok(v)
    }
    fn select(&self, v: View, s: &Selector) -> Result<View> {
        match s {
            Selector::Slice { low, high } if *high < v.width => Ok(v.slice(*low, high - low + 1)),
            Selector::Index { index, width } => match index.as_ref() {
                E::Int(i) if *i < u64::from(v.width / width) => {
                    Ok(v.slice(*i as u32 * width, *width))
                }
                _ => Err(invalid(
                    "dynamic packed indexing is outside the fast subset",
                )),
            },
            _ => Err(invalid("invalid fixed part selection")),
        }
    }
    // Split each copy at both source and destination limb boundaries.
    fn copy(&mut self, dst: View, src: View) {
        let mut bit = 0;
        while bit < dst.width {
            let d = dst.slice(bit, dst.width - bit);
            let s = if bit < src.width {
                src.slice(bit, src.width - bit)
            } else {
                self.zero
            };
            let w = d.width.min(64 - d.low).min(s.width).min(64 - s.low);
            if w == 64 {
                if d.slot != s.slot {
                    self.ops.push(Op::Copy(d.slot, s.slot));
                }
            } else {
                let bits = mask(w) << d.low;
                self.ops.push(Op::Bits {
                    dst: d.slot,
                    source: s.slot,
                    shift: d.low as i32 - s.low as i32,
                    mask: bits,
                    merge: !(bit == 0 && dst.low == 0 && self.is_temp(dst)),
                });
            }
            bit += w;
        }
    }
    fn is_temp(&self, v: View) -> bool {
        v.slot > self.zero.slot
    }
    fn word(&mut self, v: View) -> Result<usize> {
        if v.width > 64 {
            return Err(invalid("wide arithmetic operand"));
        }
        if v.low == 0 && v.width == 64 {
            return Ok(v.slot);
        }
        let out = self.alloc(v.width);
        self.copy(out, v);
        Ok(out.slot)
    }
    fn binary(&mut self, op: B, a: View, b: View, w: u32) -> Result<View> {
        if a.width.max(b.width).max(w) > 64 {
            return self.wide_binary(op, a, b, w);
        }
        let a = self.word(a)?;
        let b = self.word(b)?;
        let dst = self.alloc(w);
        self.ops.push(Op::Binary {
            dst: dst.slot,
            a,
            b,
            op,
            mask: mask(w),
        });
        Ok(dst)
    }
    // Lower wide operations to the existing word-plane instructions. Carry and
    // equality reductions stay lane-local, including a partially occupied high limb.
    fn wide_binary(&mut self, op: B, a: View, b: View, width: u32) -> Result<View> {
        let bits = a.width.max(b.width).max(width);
        if matches!(op, B::Lt | B::LtEq | B::Gt | B::GtEq) {
            return self.wide_order(op, (a, b), bits, false);
        }
        let out = self.alloc(width);
        let mut carry = self.zero;
        let mut equal = self.const_word(1, 1);
        for low in (0..bits).step_by(64) {
            let w = (bits - low).min(64);
            let (x, y) = (a.limb(low, w, self.zero), b.limb(low, w, self.zero));
            let chunk = match op {
                B::BitAnd | B::BitXor | B::BitOr => self.binary(op, x, y, w)?,
                B::Add => {
                    let sum = self.binary(B::Add, x, y, w)?;
                    let value = self.binary(B::Add, sum, carry, w)?;
                    let c1 = self.binary(B::Lt, sum, x, 1)?;
                    let c2 = self.binary(B::Lt, value, sum, 1)?;
                    carry = self.binary(B::BitOr, c1, c2, 1)?;
                    value
                }
                B::Sub => {
                    let diff = self.binary(B::Sub, x, y, w)?;
                    let value = self.binary(B::Sub, diff, carry, w)?;
                    if bits - low > 64 {
                        let borrowed = self.binary(B::Lt, x, y, 1)?;
                        let propagated = self.binary(B::Lt, diff, carry, 1)?;
                        carry = self.binary(B::BitOr, borrowed, propagated, 1)?;
                    }
                    value
                }
                B::Eq | B::NotEq => {
                    let same = self.binary(B::Eq, x, y, 1)?;
                    equal = self.binary(B::BitAnd, equal, same, 1)?;
                    continue;
                }
                _ => return Err(invalid("unsupported wide arithmetic operation")),
            };
            self.copy(out.slice(low, w), chunk);
        }
        match op {
            B::Eq => Ok(equal),
            B::NotEq => self.binary(B::Eq, equal, self.zero, 1),
            _ => Ok(out),
        }
    }
    fn mux(&mut self, cond: View, a: View, b: View, w: u32) -> Result<View> {
        let cond = self.word(cond)?;
        let out = self.alloc(w);
        for i in (0..w).step_by(64) {
            let n = (w - i).min(64);
            let mut chunk = |v: View| {
                if i >= v.width {
                    Ok(self.zero.slot)
                } else {
                    self.word(v.slice(i, (v.width - i).min(n)))
                }
            };
            let a = chunk(a)?;
            let b = chunk(b)?;
            self.ops.push(Op::Mux {
                dst: out.slot + (i / 64) as usize,
                cond,
                a,
                b,
            });
        }
        Ok(out)
    }
    fn assigned_expr(&mut self, e: &E, width: u32) -> Result<View> {
        let start = self.ops.len();
        let next = self.next;
        let result = self.expr(e, width)?;
        if !matches!(e, E::Select{arms,..} if arms.len()>=3) || width > 4 || result.low != 0 {
            return Ok(result);
        }
        let mut reads = Vec::new();
        crate::schedule::collect(e, self.d, &mut reads);
        let mut ids = reads.into_iter().map(|r| r.signal.0).collect::<Vec<_>>();
        ids.sort_unstable();
        ids.dedup();
        let [a, b] = ids.as_slice() else {
            return Ok(result);
        };
        let (a, b) = (self.signals[*a], self.signals[*b]);
        if let Some(table) = super::table::compile(
            &self.ops[start..],
            [(a.slot, a.width), (b.slot, b.width)],
            result.slot,
            width,
        ) {
            self.ops.truncate(start);
            self.next = next;
            let dst = self.alloc(width);
            self.ops.push(Op::Lookup {
                dst: dst.slot,
                a: a.slot,
                b: b.slot,
                table,
            });
            return Ok(dst);
        }
        Ok(result)
    }
    fn expr(&mut self, e: &E, context: u32) -> Result<View> {
        self.expr_typed(e, context, signed(e))
    }
    fn expr_typed(&mut self, e: &E, context: u32, signed_context: bool) -> Result<View> {
        match e {
            E::Read(t) => self.target(t),
            E::Int(x) => {
                let w = (65 - x.leading_zeros()).max(32);
                Ok(self.const_word(*x, w))
            }
            E::Bool(x) => Ok(self.const_word(u64::from(*x), 1)),
            E::HighZ | E::Zero => Ok(View::new(self.zero.slot, 1)),
            E::Project { value, selector } => {
                let v = self.expr(value, 0)?;
                self.select(v, selector)
            }
            E::Concat(parts) => {
                let parts = parts
                    .iter()
                    .rev()
                    .map(|p| self.expr(p, 0))
                    .collect::<Result<Vec<_>>>()?;
                let out = self.alloc(parts.iter().map(|p| p.width).sum());
                // Fully initialize all words, including unused bits in the top limb.
                for i in 0..out.width.div_ceil(64) {
                    self.ops.push(Op::Const(out.slot + i as usize, 0));
                }
                let mut bit = 0;
                for p in parts {
                    self.copy(out.slice(bit, p.width), p);
                    bit += p.width;
                }
                Ok(out)
            }
            E::MemoryRead { memory, port } => {
                let m = &self.d.memories[memory.0];
                let a = self.expr(&m.comb_reads[*port].address, 0)?;
                self.gather(memory.0, a)
            }
            E::Unary { op, expr } => {
                let v = if *op == HwUnaryOp::Not {
                    self.expr(expr, 0)?
                } else {
                    self.expr_typed(expr, context, signed_context)?
                };
                match op {
                    HwUnaryOp::Not => self.binary(B::Eq, v, self.zero, 1),
                    HwUnaryOp::Neg => self.binary(B::Sub, self.zero, v, v.width.max(context)),
                    _ => Err(invalid("unsupported unary operation")),
                }
            }
            E::Binary { op, left, right } => {
                let pred = matches!(
                    op,
                    B::Lt | B::LtEq | B::Gt | B::GtEq | B::NotEq | B::Eq | B::AndAnd | B::OrOr
                );
                let natural =
                    self.width(left)?
                        .max(if *op == B::Shl { 0 } else { self.width(right)? });
                let ctx = if matches!(op, B::AndAnd | B::OrOr) {
                    0
                } else if pred {
                    natural
                } else {
                    context.max(natural)
                };
                if ctx > 64
                    && matches!(op, B::Lt | B::LtEq | B::Gt | B::GtEq)
                    && let (Some(x), Some(y)) = (
                        self.sign_flipped_operand(left, ctx),
                        self.sign_flipped_operand(right, ctx),
                    )
                {
                    let a = self.expr(x, 0)?;
                    let b = self.expr(y, 0)?;
                    return self.wide_order(*op, (a, b), ctx, true);
                }
                let signed_operands = if pred {
                    signed(left) && signed(right)
                } else {
                    signed_context
                };
                let logical = matches!(op, B::AndAnd | B::OrOr);
                let a = if logical {
                    self.expr(left, 0)?
                } else {
                    self.expr_typed(left, ctx, signed_operands)?
                };
                let b = if logical || *op == B::Shl {
                    self.expr(right, 0)?
                } else {
                    self.expr_typed(right, ctx, signed_operands)?
                };
                if !matches!(
                    op,
                    B::BitAnd
                        | B::BitOr
                        | B::BitXor
                        | B::Add
                        | B::Sub
                        | B::Mul
                        | B::Div
                        | B::Rem
                        | B::Eq
                        | B::NotEq
                        | B::Lt
                        | B::LtEq
                        | B::Gt
                        | B::GtEq
                        | B::AndAnd
                        | B::OrOr
                        | B::Shl
                ) {
                    return Err(invalid("unsupported binary operation"));
                }
                if signed_operands
                    && matches!(op, B::Lt | B::LtEq | B::Gt | B::GtEq | B::Div | B::Rem)
                {
                    if ctx > 64 {
                        if pred {
                            return self.wide_order(*op, (a, b), ctx, true);
                        }
                        return Err(invalid("wide signed arithmetic operand"));
                    }
                    return self.signed_word_binary(*op, a, b, ctx);
                }
                let w = if pred {
                    1
                } else if *op == B::Shl {
                    a.width.max(ctx)
                } else {
                    a.width.max(b.width).max(ctx)
                };
                self.binary(*op, a, b, w)
            }
            E::Mux {
                cond,
                then_value,
                else_value,
            } => {
                let context = context
                    .max(self.width(then_value)?)
                    .max(self.width(else_value)?);
                let c = self.expr(cond, 0)?;
                let a = self.expr_typed(then_value, context, signed_context)?;
                let b = self.expr_typed(else_value, context, signed_context)?;
                self.mux(c, a, b, a.width.max(b.width).max(context))
            }
            E::Select { arms, default, .. } => {
                let context = context.max(self.width(e)?);
                let mut acc = self.expr_typed(default, context, signed_context)?;
                for (g, v) in arms.iter().rev() {
                    let c = self.expr(g, 0)?;
                    let a = self.expr_typed(v, context, signed_context)?;
                    acc = self.mux(c, a, acc, a.width.max(acc.width).max(context))?;
                }
                Ok(acc)
            }
        }
    }
    fn width(&self, expr: &E) -> Result<u32> {
        Ok(match expr {
            E::Read(t) => self.target(t)?.width,
            E::Int(n) => (65 - n.leading_zeros()).max(32),
            E::Zero
            | E::Bool(_)
            | E::HighZ
            | E::Unary {
                op: HwUnaryOp::Not, ..
            } => 1,
            E::Unary { expr, .. } => self.width(expr)?,
            E::Binary {
                op: B::Eq | B::NotEq | B::AndAnd | B::OrOr | B::Gt | B::GtEq | B::Lt | B::LtEq,
                ..
            } => 1,
            E::Binary {
                op: B::Shl, left, ..
            } => self.width(left)?,
            E::Binary { left, right, .. } => self.width(left)?.max(self.width(right)?),
            E::Mux {
                then_value,
                else_value,
                ..
            } => self.width(then_value)?.max(self.width(else_value)?),
            E::Select { arms, default, .. } => {
                let mut w = self.width(default)?;
                for (_, v) in arms {
                    w = w.max(self.width(v)?);
                }
                w
            }
            E::Concat(parts) => parts.iter().try_fold(0u32, |w, p| {
                w.checked_add(self.width(p)?)
                    .ok_or_else(|| invalid("concatenation too wide"))
            })?,
            E::Project { selector, .. } => selector.width(),
            E::MemoryRead { memory, .. } => self.d.memories[memory.0].width,
        })
    }
    fn gather(&mut self, m: usize, address: View) -> Result<View> {
        let address = self.word(address)?;
        let mem = &self.d.memories[m];
        if self.mem[m] == usize::MAX {
            return Err(invalid("cross-family RAM dependency"));
        }
        let dst = self.alloc(mem.width);
        self.ops.push(Op::Gather {
            dst: dst.slot,
            base: self.mem[m],
            depth: mem.depth as usize,
            address,
        });
        Ok(dst)
    }
    fn snapshot(&mut self, v: View, w: u32) -> View {
        let out = self.alloc(w);
        self.copy(out, v);
        out
    }
    fn reset(&mut self, id: usize) -> Result<View> {
        let r = &self.d.resets[id];
        let v = self.signals[r.signal.0];
        if v.width == 0 {
            return Err(invalid("cross-family reset dependency"));
        }
        if r.polarity == HwResetPolarity::ActiveLow {
            self.binary(B::Eq, v, self.zero, 1)
        } else {
            Ok(v)
        }
    }
}
pub(super) fn compile(d: &SimDesign) -> Result<FastProgram> {
    if d.clocks.len() > 1
        || !d.externs.is_empty()
        || d.resets.iter().any(|r| r.mode == HwResetMode::Async)
        || d.signals.iter().any(|s| s.alias.is_some())
        || d.instances
            .iter()
            .flat_map(|i| &i.ports)
            .any(|p| p.direction == HwDirection::InOut)
    {
        return Err(invalid(
            "requires a single synchronous clock, two-state nets and native cells",
        ));
    }
    compile_flat(d, &d.comb_eval_order, |_| true)
}

pub(super) fn compile_flat(
    d: &SimDesign,
    order: &[crate::CombId],
    owns: impl Fn(crate::InstanceId) -> bool,
) -> Result<FastProgram> {
    let mut members = BTreeMap::new();
    for (i, s) in d.signals.iter().enumerate() {
        members
            .entry(s.source.instance)
            .or_insert_with(Vec::new)
            .push(i);
    }
    let signals = d
        .families
        .iter()
        .flat_map(|f| &f.members)
        .flat_map(|id| members.get(id).into_iter().flatten().copied());
    compile_scope(d, order, signals, owns, true)
}

/// Subtrees use identical local slot numbers even when their concrete arena
/// IDs differ. Unmapped signal and RAM references reject dependent members.
pub(super) fn compile_scope(
    d: &SimDesign,
    order: &[crate::CombId],
    signals: impl Iterator<Item = usize>,
    owns: impl Fn(crate::InstanceId) -> bool,
    whole: bool,
) -> Result<FastProgram> {
    let mut b = Builder {
        d,
        signals: vec![View::new(0, 0); d.signals.len()],
        mem: vec![],
        next: 0,
        peak: 0,
        ops: vec![],
        zero: View::new(0, 64),
    };
    for i in signals {
        let v = b.alloc(d.signals[i].width);
        b.signals[i] = v;
    }
    if whole && b.signals.iter().any(|v| v.width == 0) {
        return Err(invalid("missing instance family or zero-width signal"));
    }
    for m in &d.memories {
        if !owns(m.source.instance) {
            // Keep the scalar parent RAM offsets identical to the flat layout.
            if whole {
                b.next += m.depth as usize;
            }
            b.mem.push(usize::MAX);
            continue;
        }
        if m.width > 64 {
            return Err(invalid("requires <=64-bit memory words"));
        }
        b.mem.push(b.next);
        b.next += m.depth as usize;
    }
    b.zero = b.alloc(64);
    let fixed = b.next;
    let mut comb_ends = Vec::with_capacity(order.len());
    for id in order {
        let a = &d.comb[id.0];
        let dst = b.target(&a.target)?;
        let src = b.assigned_expr(&a.value, dst.width)?;
        // Stage overlapping slices before modifying their common backing signal.
        let src = if a.target.selectors.is_empty() || src.slot >= fixed {
            src
        } else {
            b.snapshot(src, src.width)
        };
        b.copy(dst, src);
        b.next = fixed;
        comb_ends.push(b.ops.len());
    }
    let comb = std::mem::take(&mut b.ops);
    let mut commit = Vec::new();
    for reg in d.regs.iter().filter(|r| owns(r.source.instance)) {
        let next = b.assigned_expr(&reg.next, reg.width)?;
        let next = if let Some((id, e)) = &reg.reset {
            let cond = b.reset(id.0)?;
            let value = b.expr(e, reg.width)?;
            b.mux(cond, value, next, reg.width)?
        } else {
            next
        };
        let saved = b.snapshot(next, reg.width);
        commit.push((b.signals[reg.signal.0], saved));
    }
    let mut writes = Vec::new();
    for (i, m) in d
        .memories
        .iter()
        .enumerate()
        .filter(|(_, m)| owns(m.source.instance))
    {
        for p in &m.sync_reads {
            let dst = b.target(&p.target)?;
            let en = b.expr(&p.enable, 0)?;
            let addr = b.expr(&p.address, 0)?;
            let loaded = b.gather(i, addr)?;
            let saved = b.mux(en, loaded, dst, dst.width)?;
            commit.push((dst, saved));
        }
        for p in &m.writes {
            let en = b.expr(&p.enable, 0)?;
            let a = b.expr(&p.address, 0)?;
            let data = b.expr(&p.data, m.width)?;
            writes.push(Op::Scatter {
                base: b.mem[i],
                depth: m.depth as usize,
                address: b.word(a)?,
                data: b.snapshot(data, m.width).slot,
                enable: b.word(en)?,
            });
        }
    }
    for (id, a) in d
        .assertions
        .iter()
        .filter(|a| owns(a.source.instance))
        .enumerate()
    {
        let mut v = b.expr(&a.condition, 0)?;
        if let Some(id) = a.reset_mask {
            let r = b.reset(id.0)?;
            v = b.binary(B::OrOr, v, r, 1)?;
        }
        let s = b.word(v)?;
        b.ops.push(Op::Check(s, id));
    }
    if d.assertions.iter().any(|a| owns(a.source.instance)) {
        b.ops.push(Op::Stop);
    }
    b.ops.extend(writes);
    for (dst, src) in commit {
        b.copy(dst, src);
    }
    let mut inputs = vec![false; d.signals.len()];
    for p in d.top_ports() {
        inputs[p.signal.0] = p.direction == HwDirection::In;
    }
    let clock = d.clocks.first().map(|c| b.signals[c.signal.0].slot);
    if d.clocks.first().is_some_and(|c| !inputs[c.signal.0]) {
        return Err(invalid("requires an external top clock"));
    }
    Ok(FastProgram {
        comb_ends,
        clock_used_in_comb: clock.is_some_and(|clk| comb.iter().any(|op| op.reads(clk))),
        comb,
        edge: b.ops,
        slots: b.peak,
        clock,
        signals: b.signals.into_iter().map(|v| (v.slot, v.width)).collect(),
        inputs,
    })
}
