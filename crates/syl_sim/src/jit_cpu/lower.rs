//! Straight-line word operations stay in SSA until a phase boundary.
use super::*;
use crate::HwBinaryOp as B;
use crate::fast_cpu::kernel::FastOp as Op;
use cranelift_codegen::ir::condcodes::IntCC as C;

pub(super) struct Frame<'a, 'f> {
    pub b: &'a mut FunctionBuilder<'f>,
    pub plane: Value,
    pub values: Vec<Value>,
    pub failed: Value,
}

impl Frame<'_, '_> {
    pub fn imm(&mut self, n: u64) -> Value {
        self.b.ins().iconst(I64, n as i64)
    }
    pub fn read(&self, slot: usize) -> Value {
        self.values[slot]
    }
    pub fn store(&mut self, slot: usize, value: Value) {
        self.b
            .ins()
            .store(MemFlags::trusted(), value, self.plane, offset(slot));
    }
    fn compare(&mut self, op: C, x: Value, y: Value) -> Value {
        let v = self.b.ins().icmp(op, x, y);
        self.b.ins().uextend(I64, v)
    }
    fn divrem(&mut self, x: Value, y: Value, signed: bool, rem: bool) -> Value {
        // Guard the operands before division: integer div can trap even if
        // its result is discarded by a subsequent select.
        let zero = self.imm(0);
        let one = self.imm(1);
        let by_zero = self.b.ins().icmp_imm(C::Equal, y, 0);
        let unsafe_divisor = if signed {
            let min = self.b.ins().icmp_imm(C::Equal, x, i64::MIN);
            let neg_one = self.b.ins().icmp_imm(C::Equal, y, -1);
            let overflow = self.b.ins().band(min, neg_one);
            self.b.ins().bor(by_zero, overflow)
        } else {
            by_zero
        };
        let divisor = self.b.ins().select(unsafe_divisor, one, y);
        let v = match (signed, rem) {
            (true, true) => self.b.ins().srem(x, divisor),
            (true, false) => self.b.ins().sdiv(x, divisor),
            (false, true) => self.b.ins().urem(x, divisor),
            (false, false) => self.b.ins().udiv(x, divisor),
        };
        self.b.ins().select(by_zero, zero, v)
    }
    fn binary(&mut self, op: B, x: Value, y: Value, signed: bool) -> Value {
        match op {
            B::Add => self.b.ins().iadd(x, y),
            B::Sub => self.b.ins().isub(x, y),
            B::Mul => self.b.ins().imul(x, y),
            B::BitAnd => self.b.ins().band(x, y),
            B::BitOr => self.b.ins().bor(x, y),
            B::BitXor => self.b.ins().bxor(x, y),
            B::Shl => {
                let fits = self.b.ins().icmp_imm(C::UnsignedLessThan, y, 64);
                let v = self.b.ins().ishl(x, y);
                let zero = self.imm(0);
                self.b.ins().select(fits, v, zero)
            }
            B::Div | B::Rem => self.divrem(x, y, signed, op == B::Rem),
            B::Eq => self.compare(C::Equal, x, y),
            B::NotEq => self.compare(C::NotEqual, x, y),
            B::Lt => self.compare(
                if signed {
                    C::SignedLessThan
                } else {
                    C::UnsignedLessThan
                },
                x,
                y,
            ),
            B::LtEq => self.compare(
                if signed {
                    C::SignedLessThanOrEqual
                } else {
                    C::UnsignedLessThanOrEqual
                },
                x,
                y,
            ),
            B::Gt => self.compare(
                if signed {
                    C::SignedGreaterThan
                } else {
                    C::UnsignedGreaterThan
                },
                x,
                y,
            ),
            B::GtEq => self.compare(
                if signed {
                    C::SignedGreaterThanOrEqual
                } else {
                    C::UnsignedGreaterThanOrEqual
                },
                x,
                y,
            ),
            B::AndAnd | B::OrOr => {
                let a = self.b.ins().icmp_imm(C::NotEqual, x, 0);
                let b = self.b.ins().icmp_imm(C::NotEqual, y, 0);
                let v = if op == B::AndAnd {
                    self.b.ins().band(a, b)
                } else {
                    self.b.ins().bor(a, b)
                };
                self.b.ins().uextend(I64, v)
            }
            _ => unreachable!("validated fast binary operation"),
        }
    }
    fn address(&mut self, address: usize, depth: usize) -> (Value, Value) {
        let index = self.read(address);
        let valid = self
            .b
            .ins()
            .icmp_imm(C::UnsignedLessThan, index, depth as i64);
        let zero = self.imm(0);
        let safe = self.b.ins().select(valid, index, zero);
        let bytes = self.b.ins().ishl_imm(safe, 3);
        (valid, self.b.ins().iadd(self.plane, bytes))
    }
    pub fn execute(&mut self, ops: &[Op]) {
        for &op in ops {
            let v = match op {
                Op::Const(_, n) => self.imm(n),
                Op::Lookup { a, b, table, .. } => {
                    let a = self.read(a);
                    let b = self.read(b);
                    let x = self.b.ins().band_imm(a, (1i64 << table.a_bits) - 1);
                    let y = self.b.ins().ishl_imm(b, i64::from(table.a_bits));
                    let key = self.b.ins().bor(x, y);
                    let key = self.b.ins().band_imm(key, table.input_mask as i64);
                    let shift = self.b.ins().ishl_imm(key, i64::from(table.stride));
                    let values = self.imm(table.values);
                    let v = self.b.ins().ushr(values, shift);
                    self.b.ins().band_imm(v, table.output_mask as i64)
                }
                Op::Copy(_, s) => self.read(s),
                Op::Bits {
                    dst,
                    source,
                    shift,
                    mask,
                    merge,
                } => {
                    let src = self.read(source);
                    let v = if shift >= 0 {
                        self.b.ins().ishl_imm(src, i64::from(shift))
                    } else {
                        self.b.ins().ushr_imm(src, i64::from(-shift))
                    };
                    let v = self.b.ins().band_imm(v, mask as i64);
                    if merge {
                        let prev = self.read(dst);
                        let keep = self.b.ins().band_imm(prev, !mask as i64);
                        self.b.ins().bor(v, keep)
                    } else {
                        v
                    }
                }
                Op::Binary { a, b, op, mask, .. } => {
                    let (x, y) = (self.read(a), self.read(b));
                    let v = self.binary(op, x, y, false);
                    self.b.ins().band_imm(v, mask as i64)
                }
                Op::SignedBinary {
                    a, b, op, width, ..
                } => {
                    let mut extend = |slot| {
                        let v = self.read(slot);
                        let n = i64::from(64 - width);
                        let v = self.b.ins().ishl_imm(v, n);
                        self.b.ins().sshr_imm(v, n)
                    };
                    let (x, y) = (extend(a), extend(b));
                    let v = self.binary(op, x, y, true);
                    self.b.ins().band_imm(v, (u64::MAX >> (64 - width)) as i64)
                }
                Op::Mux { cond, a, b, .. } => {
                    let (c, x, y) = (self.read(cond), self.read(a), self.read(b));
                    self.b.ins().select(c, x, y)
                }
                Op::Gather {
                    base,
                    depth,
                    address,
                    ..
                } if depth != 0 => {
                    let (valid, ptr) = self.address(address, depth);
                    let v = self
                        .b
                        .ins()
                        .load(I64, MemFlags::trusted(), ptr, offset(base));
                    let zero = self.imm(0);
                    self.b.ins().select(valid, v, zero)
                }
                Op::Gather { .. } => self.imm(0),
                Op::Scatter {
                    base,
                    depth,
                    address,
                    data,
                    enable,
                } if depth != 0 => {
                    let (valid, ptr) = self.address(address, depth);
                    let (v, en) = (self.read(data), self.read(enable));
                    let en = self.b.ins().icmp_imm(C::NotEqual, en, 0);
                    let write = self.b.ins().band(valid, en);
                    let old = self
                        .b
                        .ins()
                        .load(I64, MemFlags::trusted(), ptr, offset(base));
                    let value = self.b.ins().select(write, v, old);
                    self.b
                        .ins()
                        .store(MemFlags::trusted(), value, ptr, offset(base));
                    continue;
                }
                Op::Check(s, id) => {
                    let condition = self.read(s);
                    let id = self.imm(id as u64);
                    let first = self.b.ins().umin(self.failed, id);
                    self.failed = self.b.ins().select(condition, self.failed, first);
                    continue;
                }
                Op::Scatter { .. } | Op::Stop => continue,
            };
            self.values[op.dst().unwrap()] = v;
        }
    }
}
