use crate::{constant::Result, design::*};
use std::collections::{BTreeMap, BTreeSet};

/// The graph is on assignments, so independent drivers of disjoint slices
/// retain distinct schedule slots. Edges point from a reader to its producers.
pub(crate) fn order(design: &SimDesign) -> Result<Vec<CombId>> {
    let mut writers = BTreeMap::<SignalId, Vec<(usize, (u32, u32))>>::new();
    for (id, assign) in design.comb.iter().enumerate() {
        writers
            .entry(assign.target.signal)
            .or_default()
            .push((id, range(&assign.target, design)));
    }
    let mut graph = Vec::new();
    for assign in &design.comb {
        let mut reads = Vec::new();
        collect(&assign.value, design, &mut reads);
        indices(&assign.target.selectors, design, &mut reads);
        let mut deps = BTreeSet::new();
        for r in reads {
            let (low, high) = range(r, design);
            if let Some(nodes) = writers.get(&r.signal) {
                deps.extend(
                    nodes
                        .iter()
                        .filter(|(_, (l, h))| low <= *h && *l <= high)
                        .map(|(id, _)| *id),
                );
            }
        }
        graph.push(deps.into_iter().collect::<Vec<_>>());
    }
    tarjan(&graph, design)
}

fn range(target: &SimTarget, design: &SimDesign) -> (u32, u32) {
    let (mut low, mut high) = (0, design.signal(target.signal).width - 1);
    for selector in &target.selectors {
        match selector {
            Selector::Slice { low: l, high: h } => {
                high = low + h;
                low += l;
            }
            Selector::Index { .. } => break,
        }
    }
    (low, high)
}

fn indices<'a>(selectors: &'a [Selector], design: &'a SimDesign, reads: &mut Vec<&'a SimTarget>) {
    for selector in selectors {
        if let Selector::Index { index, .. } = selector {
            collect(index, design, reads)
        }
    }
}

fn collect<'a>(expr: &'a SimExpr, design: &'a SimDesign, reads: &mut Vec<&'a SimTarget>) {
    match expr {
        SimExpr::Read(target) => {
            reads.push(target);
            indices(&target.selectors, design, reads);
        }
        SimExpr::MemoryRead { memory, port } => collect(
            &design.memories[memory.0].comb_reads[*port].address,
            design,
            reads,
        ),
        SimExpr::Project { value, selector } => {
            collect(value, design, reads);
            indices(std::slice::from_ref(selector), design, reads);
        }
        SimExpr::Unary { expr, .. } => collect(expr, design, reads),
        SimExpr::Binary { left, right, .. } => {
            collect(left, design, reads);
            collect(right, design, reads);
        }
        SimExpr::Mux {
            cond,
            then_value,
            else_value,
        } => {
            for expr in [cond, then_value, else_value] {
                collect(expr, design, reads)
            }
        }
        SimExpr::Select { arms, default, .. } => {
            for (guard, value) in arms {
                collect(guard, design, reads);
                collect(value, design, reads);
            }
            collect(default, design, reads);
        }
        SimExpr::Concat(parts) => {
            for p in parts {
                collect(p, design, reads)
            }
        }
        SimExpr::Int(_) | SimExpr::Bool(_) | SimExpr::HighZ | SimExpr::Zero => {}
    }
}

/// Iterative Tarjan DFS. A component with no back edge is emitted after its
/// producers. The explicit traversal stack also handles long wire chains.
fn tarjan(graph: &[Vec<usize>], design: &SimDesign) -> Result<Vec<CombId>> {
    let mut next_index = 0;
    let mut index = vec![usize::MAX; graph.len()];
    let mut low = index.clone();
    let mut active = vec![false; graph.len()];
    let mut stack = Vec::new();
    let mut result = Vec::with_capacity(graph.len());
    for root in 0..graph.len() {
        if index[root] != usize::MAX {
            continue;
        }
        let mut frames = vec![(root, 0)];
        while let Some(&mut (node, ref mut edge)) = frames.last_mut() {
            if index[node] == usize::MAX {
                index[node] = next_index;
                low[node] = next_index;
                next_index += 1;
                active[node] = true;
                stack.push(node);
            }
            if let Some(&child) = graph[node].get(*edge) {
                *edge += 1;
                if index[child] == usize::MAX {
                    frames.push((child, 0))
                } else if active[child] {
                    low[node] = low[node].min(index[child])
                }
                continue;
            }
            frames.pop();
            if let Some(&(parent, _)) = frames.last() {
                low[parent] = low[parent].min(low[node]);
            }
            if low[node] == index[node] {
                let mut members = BTreeSet::new();
                while let Some(member) = stack.pop() {
                    active[member] = false;
                    members.insert(member);
                    if member == node {
                        break;
                    }
                }
                if members.len() > 1 || graph[node].contains(&node) {
                    return Err(cycle(node, &members, graph, design));
                }
                result.push(CombId(node));
            }
        }
    }
    Ok(result)
}

fn cycle(
    start: usize,
    members: &BTreeSet<usize>,
    graph: &[Vec<usize>],
    design: &SimDesign,
) -> SimError {
    let name = |id: usize| {
        design
            .signal(design.comb[id].target.signal)
            .source
            .path
            .clone()
    };
    let mut positions = BTreeMap::new();
    let mut chain = Vec::new();
    let mut node = start;
    while !positions.contains_key(&node) {
        positions.insert(node, chain.len());
        chain.push(name(node));
        node = *graph[node]
            .iter()
            .find(|id| members.contains(id))
            .expect("a cyclic SCC has an internal successor for each node");
    }
    chain.drain(..positions[&node]);
    chain.push(name(node));
    chain.reverse(); // report producer -> reader
    SimError::CombinationalCycle {
        chain,
        members: members.iter().copied().map(name).collect(),
    }
}
