//! Propagate slice boundaries through bitwise expressions and word copies. Each resulting
//! interval is an executable assignment, even when consumers need different
//! parts of a port at different points during the same settle.
use super::{collect, indices, range};
use crate::design::*;
use std::collections::{BTreeSet, VecDeque};
use syl_hw::HwBinaryOp;

#[derive(Clone, Copy)]
struct Interval {
    signal: SignalId,
    start: u32,
    end: u32,
}

impl Interval {
    fn of(target: &SimTarget, design: &SimDesign) -> Self {
        let (start, last) = range(target, design);
        Self {
            signal: target.signal,
            start,
            end: last + 1,
        }
    }
    fn fixed(target: &SimTarget, design: &SimDesign) -> Option<Self> {
        target
            .selectors
            .iter()
            .all(|s| matches!(s, Selector::Slice { .. }))
            .then(|| Self::of(target, design))
    }
    fn width(self) -> u32 {
        self.end - self.start
    }
    fn target(self, low: u32, end: u32) -> SimTarget {
        SimTarget {
            signal: self.signal,
            selectors: vec![Selector::Slice {
                low: self.start + low,
                high: self.start + (end - 1),
            }],
        }
    }
}

/// Only fixed reads and lane-wise operations admit this mapping. Index
/// expressions, comparisons and arithmetic keep their whole-word dependencies.
enum Aligned {
    Read(Interval),
    Bits(u64),
    HighZ,
    Binary(HwBinaryOp, Box<Self>, Box<Self>),
}

impl Aligned {
    fn from_expr(expr: &SimExpr, design: &SimDesign) -> Option<Self> {
        Some(match expr {
            SimExpr::Read(target) => Self::Read(Interval::fixed(target, design)?),
            SimExpr::Int(n) => Self::Bits(*n),
            SimExpr::Bool(b) => Self::Bits(u64::from(*b)),
            SimExpr::Zero => Self::Bits(0),
            SimExpr::HighZ => Self::HighZ,
            SimExpr::Binary {
                op: op @ (HwBinaryOp::BitAnd | HwBinaryOp::BitOr | HwBinaryOp::BitXor),
                left,
                right,
            } => Self::Binary(
                *op,
                Box::new(Self::from_expr(left, design)?),
                Box::new(Self::from_expr(right, design)?),
            ),
            _ => return None,
        })
    }

    fn project(&self, low: u32, end: u32) -> SimExpr {
        match self {
            // The source width is a cut point, so an interval is either all
            // padding or all source bits, including for unequal-width operands.
            Self::Read(src) if low >= src.width() => SimExpr::Zero,
            Self::Read(src) => SimExpr::Read(src.target(low, end)),
            Self::Bits(n) => {
                let mask = u64::MAX.checked_shl(end - low).unwrap_or(0);
                SimExpr::Int(n.checked_shr(low).unwrap_or(0) & !mask)
            }
            Self::HighZ => SimExpr::HighZ,
            Self::Binary(op, left, right) => SimExpr::Binary {
                op: *op,
                left: Box::new(left.project(low, end)),
                right: Box::new(right.project(low, end)),
            },
        }
    }
}

pub(super) fn split(design: &mut SimDesign) {
    let mut cuts = vec![BTreeSet::new(); design.signals.len()];
    let mut links = vec![Vec::new(); design.signals.len()];
    let mut plans = Vec::new();
    for assign in &design.comb {
        let mut reads = Vec::new();
        collect(&assign.value, design, &mut reads);
        let dst = Interval::of(&assign.target, design);
        cuts[dst.signal.0].extend([dst.start, dst.end]);
        let plan = Interval::fixed(&assign.target, design)
            .and_then(|_| Aligned::from_expr(&assign.value, design));
        let mut truncates = false;
        indices(&assign.target.selectors, design, &mut reads);
        for target in reads {
            let src = Interval::of(target, design);
            cuts[src.signal.0].extend([src.start, src.end]);
            if plan.is_some() {
                let width = src.width().min(dst.width());
                truncates |= src.width() > width;
                let src = Interval {
                    end: src.start + width,
                    ..src
                };
                let dst = Interval {
                    end: dst.start + width,
                    ..dst
                };
                cuts[src.signal.0].insert(src.end);
                cuts[dst.signal.0].insert(dst.end);
                links[src.signal.0].push((src, dst));
                links[dst.signal.0].push((dst, src));
            }
        }
        plans.push(plan.map(|value| (dst, value, truncates)));
    }
    let mut queue: VecDeque<_> = cuts
        .iter()
        .enumerate()
        .flat_map(|(i, set)| set.iter().map(move |&cut| (i, cut)))
        .collect();
    while let Some((i, cut)) = queue.pop_front() {
        for &(from, to) in &links[i] {
            if cut > from.start && cut < from.end {
                let mapped = to.start + (cut - from.start);
                if cuts[to.signal.0].insert(mapped) {
                    queue.push_back((to.signal.0, mapped));
                }
            }
        }
    }
    for (assign, plan) in std::mem::take(&mut design.comb).into_iter().zip(plans) {
        if let Some((dst, value, truncates)) = plan {
            let mut boundaries = cuts[dst.signal.0]
                .range((dst.start + 1)..dst.end)
                .peekable();
            if truncates || boundaries.peek().is_some() {
                let mut low = 0;
                for end in boundaries.copied().chain([dst.end]) {
                    let offset = end - dst.start;
                    design.comb.push(CombAssign {
                        span: assign.span,
                        target: if low == 0 && offset == dst.width() {
                            assign.target.clone()
                        } else {
                            dst.target(low, offset)
                        },
                        value: value.project(low, offset),
                    });
                    low = offset;
                }
                continue;
            }
        }
        design.comb.push(assign);
    }
}
