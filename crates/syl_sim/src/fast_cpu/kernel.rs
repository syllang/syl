use crate::HwBinaryOp as B;

#[derive(Clone, Copy)]
pub(super) enum FastOp {
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
    Check(usize),
    Stop,
}

pub(super) fn execute(ops: &[FastOp], plane: &mut [u64], temp: &mut [u64]) -> Option<usize> {
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
            FastOp::Check(s) => {
                if let Some(i) = row(s).iter().position(|&v| v == 0) {
                    failure = Some(failure.map_or(i, |old: usize| old.min(i)));
                }
                continue;
            }
            FastOp::Stop => {
                if failure.is_some() {
                    return failure;
                }
                continue;
            }
        };
        plane[dst * n..(dst + 1) * n].copy_from_slice(temp);
    }
    failure
}
