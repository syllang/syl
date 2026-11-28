use crate::HwBinaryOp as B;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FastOp {
    Const(usize, u64),
    Copy(usize, usize),
    Bits {
        dst: usize,
        source: usize,
        shift: i32,
        mask: u64,
        merge: bool,
    },
    Binary {
        dst: usize,
        a: usize,
        b: usize,
        op: B,
        mask: u64,
    },
    SignedBinary {
        dst: usize,
        a: usize,
        b: usize,
        op: B,
        width: u32,
    },
    Mux {
        dst: usize,
        cond: usize,
        a: usize,
        b: usize,
    },
    Gather {
        dst: usize,
        base: usize,
        depth: usize,
        address: usize,
    },
    Scatter {
        base: usize,
        depth: usize,
        address: usize,
        data: usize,
        enable: usize,
    },
    Lookup {
        dst: usize,
        a: usize,
        b: usize,
        table: super::table::Table,
    },
    Check(usize, usize),
    Stop,
}

pub(crate) fn execute(
    ops: &[FastOp],
    plane: &mut [u64],
    temp: &mut [u64],
    stop_on_failure: bool,
) -> Option<(usize, usize)> {
    let n = temp.len();
    if n == 1 {
        return execute_scalar(ops, plane, stop_on_failure);
    }
    execute_rows(ops, plane, temp, stop_on_failure, &[])
}

pub(super) fn execute_family(
    ops: &[FastOp],
    plane: &mut [u64],
    temp: &mut [u64],
    ids: &[Vec<crate::AssertId>],
) -> Option<(usize, usize)> {
    #[cfg(target_arch = "x86_64")]
    if std::is_x86_feature_detected!("avx2") {
        // SAFETY: checked on the executing CPU. Rows have no alignment requirement.
        return unsafe { family_avx2(ops, plane, temp, ids) };
    }
    execute_rows(ops, plane, temp, false, ids)
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn family_avx2(
    ops: &[FastOp],
    plane: &mut [u64],
    temp: &mut [u64],
    ids: &[Vec<crate::AssertId>],
) -> Option<(usize, usize)> {
    execute_rows(ops, plane, temp, false, ids)
}

#[inline(always)]
fn execute_rows(
    ops: &[FastOp],
    plane: &mut [u64],
    temp: &mut [u64],
    stop_on_failure: bool,
    ids: &[Vec<crate::AssertId>],
) -> Option<(usize, usize)> {
    let n = temp.len();
    let mut failure = None;
    for &op in ops {
        let row = |slot: usize| &plane[slot * n..(slot + 1) * n];
        macro_rules! binary {
            ($a:expr,$b:expr,$f:expr) => {{
                let a = row($a);
                let b = row($b);
                for i in 0..n {
                    temp[i] = ($f)(a[i], b[i]);
                }
            }};
        }
        let dst = match op {
            FastOp::Const(d, v) => {
                plane[d * n..(d + 1) * n].fill(v);
                continue;
            }
            FastOp::Copy(d, s) => {
                plane.copy_within(s * n..(s + 1) * n, d * n);
                continue;
            }
            FastOp::Bits {
                dst,
                source,
                shift,
                mask,
                merge,
            } => {
                let src = row(source);
                if shift >= 0 {
                    for i in 0..n {
                        temp[i] = (src[i] << shift) & mask;
                    }
                } else {
                    for i in 0..n {
                        temp[i] = (src[i] >> -shift) & mask;
                    }
                }
                if merge {
                    let old = row(dst);
                    for i in 0..n {
                        temp[i] |= old[i] & !mask;
                    }
                }
                dst
            }
            FastOp::Binary {
                dst,
                a,
                b,
                op,
                mask,
            } => {
                match op {
                    B::Add => binary!(a, b, |x: u64, y| x.wrapping_add(y) & mask),
                    B::Sub => binary!(a, b, |x: u64, y| x.wrapping_sub(y) & mask),
                    B::Mul => binary!(a, b, |x: u64, y| x.wrapping_mul(y) & mask),
                    B::Div => binary!(a, b, |x: u64, y| x.checked_div(y).unwrap_or(0)),
                    B::Rem => binary!(a, b, |x: u64, y| x.checked_rem(y).unwrap_or(0)),
                    B::Shl => binary!(a, b, |x: u64, y: u64| if y < 64 {
                        (x << y) & mask
                    } else {
                        0
                    }),
                    B::BitAnd => binary!(a, b, |x, y| x & y),
                    B::BitOr => binary!(a, b, |x, y| x | y),
                    B::BitXor => binary!(a, b, |x, y| x ^ y),
                    B::Eq => binary!(a, b, |x, y| u64::from(x == y)),
                    B::NotEq => binary!(a, b, |x, y| u64::from(x != y)),
                    B::Lt => binary!(a, b, |x, y| u64::from(x < y)),
                    B::LtEq => binary!(a, b, |x, y| u64::from(x <= y)),
                    B::Gt => binary!(a, b, |x, y| u64::from(x > y)),
                    B::GtEq => binary!(a, b, |x, y| u64::from(x >= y)),
                    B::AndAnd => binary!(a, b, |x, y| u64::from(x != 0 && y != 0)),
                    B::OrOr => binary!(a, b, |x, y| u64::from(x != 0 || y != 0)),
                    _ => unreachable!("checked during compilation"),
                }
                dst
            }
            FastOp::Lookup { dst, a, b, table } => {
                binary!(a, b, |x, y| table.get(x, y));
                dst
            }
            FastOp::SignedBinary {
                dst,
                a,
                b,
                op,
                width,
            } => {
                let shift = 64 - width;
                let signed = |v: u64| ((v << shift) as i64) >> shift;
                let mask = super::mask(width);
                match op {
                    B::Lt => binary!(a, b, |x, y| u64::from(signed(x) < signed(y))),
                    B::LtEq => binary!(a, b, |x, y| u64::from(signed(x) <= signed(y))),
                    B::Gt => binary!(a, b, |x, y| u64::from(signed(x) > signed(y))),
                    B::GtEq => binary!(a, b, |x, y| u64::from(signed(x) >= signed(y))),
                    B::Div => binary!(a, b, |x, y| if y == 0 {
                        0
                    } else {
                        signed(x).wrapping_div(signed(y)) as u64 & mask
                    }),
                    B::Rem => binary!(a, b, |x, y| if y == 0 {
                        0
                    } else {
                        signed(x).wrapping_rem(signed(y)) as u64 & mask
                    }),
                    _ => unreachable!("checked during compilation"),
                }
                dst
            }
            FastOp::Mux { dst, cond, a, b } => {
                let c = row(cond);
                let a = row(a);
                let b = row(b);
                for i in 0..n {
                    temp[i] = if c[i] != 0 { a[i] } else { b[i] };
                }
                dst
            }
            FastOp::Gather {
                dst,
                base,
                depth,
                address,
            } => {
                let addr = row(address);
                for i in 0..n {
                    let a = addr[i];
                    temp[i] = if a < depth as u64 {
                        plane[(base + a as usize) * n + i]
                    } else {
                        0
                    };
                }
                dst
            }
            FastOp::Scatter {
                base,
                depth,
                address,
                data,
                enable,
            } => {
                for i in 0..n {
                    let addr = plane[address * n + i];
                    if plane[enable * n + i] != 0 && addr < depth as u64 {
                        plane[(base + addr as usize) * n + i] = plane[data * n + i];
                    }
                }
                continue;
            }
            FastOp::Check(s, id) => {
                // Batch order is (lane, assertion). A family belongs to one
                // trajectory, so reduce mapped global assertion IDs instead.
                let candidate = if ids.is_empty() {
                    row(s).iter().position(|&v| v == 0).map(|i| (i, id))
                } else {
                    row(s)
                        .iter()
                        .zip(ids)
                        .filter(|(v, _)| **v == 0)
                        .map(|(_, map)| (0, map[id].0))
                        .min()
                };
                if let Some(candidate) = candidate {
                    failure =
                        Some(failure.map_or(candidate, |old: (usize, usize)| old.min(candidate)));
                }
                continue;
            }
            FastOp::Stop => {
                if stop_on_failure && failure.is_some() {
                    return failure;
                }
                continue;
            }
        };
        plane[dst * n..(dst + 1) * n].copy_from_slice(temp);
    }
    failure
}

impl FastOp {
    pub(crate) fn operands(self) -> [Option<usize>; 3] {
        use FastOp::*;
        match self {
            Const(..) | Stop => [None; 3],
            Copy(_, s) | Check(s, _) | Gather { address: s, .. } => [Some(s), None, None],
            Bits {
                source, merge, dst, ..
            } => [Some(source), merge.then_some(dst), None],
            Lookup { a, b, .. } | Binary { a, b, .. } | SignedBinary { a, b, .. } => {
                [Some(a), Some(b), None]
            }
            Mux { cond, a, b, .. } => [Some(cond), Some(a), Some(b)],
            Scatter {
                address,
                data,
                enable,
                ..
            } => [Some(address), Some(data), Some(enable)],
        }
    }

    pub(crate) fn dst(self) -> Option<usize> {
        use FastOp::*;
        match self {
            Const(d, _)
            | Copy(d, _)
            | Bits { dst: d, .. }
            | Lookup { dst: d, .. }
            | Binary { dst: d, .. }
            | SignedBinary { dst: d, .. }
            | Mux { dst: d, .. }
            | Gather { dst: d, .. } => Some(d),
            Scatter { .. } | Check(..) | Stop => None,
        }
    }

    pub(super) fn reads(self, slot: usize) -> bool {
        self.operands().contains(&Some(slot))
    }
}

fn scalar_binary(x: u64, y: u64, op: B, mask: u64) -> u64 {
    match op {
        B::Add => x.wrapping_add(y) & mask,
        B::Sub => x.wrapping_sub(y) & mask,
        B::Mul => x.wrapping_mul(y) & mask,
        B::Div => x.checked_div(y).unwrap_or(0),
        B::Rem => x.checked_rem(y).unwrap_or(0),
        B::Shl => x.checked_shl((y.min(64)) as u32).unwrap_or(0) & mask,
        B::BitAnd => x & y,
        B::BitOr => x | y,
        B::BitXor => x ^ y,
        B::Eq => u64::from(x == y),
        B::NotEq => u64::from(x != y),
        B::Lt => u64::from(x < y),
        B::LtEq => u64::from(x <= y),
        B::Gt => u64::from(x > y),
        B::GtEq => u64::from(x >= y),
        B::AndAnd => u64::from(x != 0 && y != 0),
        B::OrOr => u64::from(x != 0 || y != 0),
        _ => unreachable!("validated binary operation"),
    }
}

pub(crate) fn execute_scalar(
    ops: &[FastOp],
    plane: &mut [u64],
    stop_on_failure: bool,
) -> Option<(usize, usize)> {
    scalar_words(ops, plane, stop_on_failure)
}

pub(super) fn scalar_words<P: std::ops::IndexMut<usize, Output = u64> + ?Sized>(
    ops: &[FastOp],
    plane: &mut P,
    stop_on_failure: bool,
) -> Option<(usize, usize)> {
    use FastOp::*;
    let mut failure = None;
    let mut ops = ops.iter().peekable();
    while let Some(&op) = ops.next() {
        match op {
            Const(d, v) => plane[d] = v,
            Lookup { dst, a, b, table } => plane[dst] = table.get(plane[a], plane[b]),
            Copy(d, s) => plane[d] = plane[s],
            Bits {
                dst,
                source,
                shift,
                mask,
                merge,
            } => {
                let src = plane[source];
                let shifted = if shift >= 0 {
                    src << shift
                } else {
                    src >> -shift
                };
                let v = (shifted & mask) | if merge { plane[dst] & !mask } else { 0 };
                plane[dst] = v;
                match ops.peek() {
                    Some(&&Copy(d, s)) if s == dst => {
                        plane[d] = v;
                        ops.next();
                    }
                    Some(&&Binary {
                        dst: out,
                        a,
                        b,
                        op,
                        mask,
                    }) if a == dst || b == dst => {
                        // The extraction result is already in a scalar register.
                        let x = if a == dst { v } else { plane[a] };
                        let y = if b == dst { v } else { plane[b] };
                        plane[out] = scalar_binary(x, y, op, mask);
                        ops.next();
                    }
                    _ => {}
                }
            }
            Binary {
                dst,
                a,
                b,
                op,
                mask,
            } => plane[dst] = scalar_binary(plane[a], plane[b], op, mask),
            SignedBinary {
                dst,
                a,
                b,
                op,
                width,
            } => {
                let shift = 64 - width;
                let signed = |v: u64| ((v << shift) as i64) >> shift;
                let (x, y) = (signed(plane[a]), signed(plane[b]));
                plane[dst] = match op {
                    B::Div if y != 0 => x.wrapping_div(y) as u64 & super::mask(width),
                    B::Rem if y != 0 => x.wrapping_rem(y) as u64 & super::mask(width),
                    B::Div | B::Rem => 0,
                    B::Lt => u64::from(x < y),
                    B::LtEq => u64::from(x <= y),
                    B::Gt => u64::from(x > y),
                    B::GtEq => u64::from(x >= y),
                    _ => unreachable!("validated signed operation"),
                };
            }
            Mux { dst, cond, a, b } => plane[dst] = plane[if plane[cond] != 0 { a } else { b }],
            Gather {
                dst,
                base,
                depth,
                address,
            } => {
                let index = plane[address];
                plane[dst] = if index < depth as u64 {
                    plane[base + index as usize]
                } else {
                    0
                };
            }
            Scatter {
                base,
                depth,
                address,
                data,
                enable,
            } => {
                let index = plane[address];
                if plane[enable] != 0 && index < depth as u64 {
                    plane[base + index as usize] = plane[data];
                }
            }
            Check(s, id) if plane[s] == 0 => {
                failure = Some(failure.map_or(id, |old: usize| old.min(id)))
            }
            Stop if stop_on_failure && failure.is_some() => break,
            Check(..) | Stop => {}
        }
    }
    failure.map(|id| (0, id))
}
