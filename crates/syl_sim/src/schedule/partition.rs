//! Propagate slice boundaries through transparent word copies. Each resulting
//! interval is an executable assignment, even when consumers need different
//! parts of a port at different points during the same settle.
use super::{collect, indices, range};
use crate::design::*;
use std::collections::{BTreeSet, VecDeque};

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

fn copy(assign: &CombAssign, design: &SimDesign) -> Option<(Interval, Interval)> {
    let SimExpr::Read(source) = &assign.value else {
        return None;
    };
    if source
        .selectors
        .iter()
        .chain(&assign.target.selectors)
        .any(|s| matches!(s, Selector::Index { .. }))
    {
        return None;
    }
    let source = Interval::of(source, design);
    let target = Interval::of(&assign.target, design);
    (source.width() == target.width()).then_some((source, target))
}

pub(super) fn split(design: &mut SimDesign) {
    let mut cuts = vec![BTreeSet::new(); design.signals.len()];
    let mut links = vec![Vec::new(); design.signals.len()];
    for assign in &design.comb {
        let mut targets = vec![&assign.target];
        collect(&assign.value, design, &mut targets);
        indices(&assign.target.selectors, design, &mut targets);
        for target in targets {
            let span = Interval::of(target, design);
            cuts[span.signal.0].extend([span.start, span.end]);
        }
        if let Some((src, dst)) = copy(assign, design) {
            links[src.signal.0].push((src, dst));
            links[dst.signal.0].push((dst, src));
        }
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
    for assign in std::mem::take(&mut design.comb) {
        if let Some((src, dst)) = copy(&assign, design) {
            let mut boundaries = cuts[src.signal.0]
                .range((src.start + 1)..src.end)
                .peekable();
            if boundaries.peek().is_some() {
                let mut low = 0;
                for end in boundaries.copied().chain([src.end]) {
                    let offset = end - src.start;
                    design.comb.push(CombAssign {
                        target: dst.target(low, offset),
                        value: SimExpr::Read(src.target(low, offset)),
                    });
                    low = offset;
                }
                continue;
            }
        }
        design.comb.push(assign);
    }
}
