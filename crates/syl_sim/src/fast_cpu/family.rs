//! Pack independent sibling subtrees into the existing slot-major SIMD kernel.
//! Scalar boundary code retains the concrete parent's arbitrary packed slices.
use super::{FastOp, FastProgram, compile, kernel};
use crate::{
    AssertId, CombId, InstanceFamily, InstanceId, RuntimeError, SimDesign, batch_run::Ports,
};

/// A verified set of sibling subtrees with identical local word programs.
pub struct FastFamilyProgram {
    local: FastProgram,
    lanes: usize,
    ports: Ports,
    external: FastProgram,
    parent_edge: Vec<FastOp>,
    parent_assertions: Vec<AssertId>,
    pre: Vec<FastOp>,
    post: Vec<FastOp>,
    inbound: Vec<(usize, usize)>,
    outbound: Vec<(usize, usize)>,
    assertions: Vec<Vec<AssertId>>,
}

fn invalid(reason: &str) -> RuntimeError {
    RuntimeError::Invalid(format!("CPU instance family: {reason}"))
}

impl FastFamilyProgram {
    pub fn new(d: &SimDesign) -> Result<Self, RuntimeError> {
        let family = d
            .instance_families()
            .iter()
            .filter(|f| {
                f.members.len() >= 4
                    && f.members
                        .iter()
                        .all(|id| d.instances[id.0].parent == Some(InstanceId(0)))
            })
            .max_by_key(|f| f.members.len())
            .ok_or_else(|| invalid("requires at least four sibling members"))?;
        Self::build(d, family)
    }

    fn build(d: &SimDesign, family: &InstanceFamily) -> Result<Self, RuntimeError> {
        let lanes = family.members.len();
        let mut owner = vec![None; d.instances.len()];
        for (lane, id) in family.members.iter().enumerate() {
            owner[id.0] = Some(lane);
        }
        for (i, inst) in d.instances.iter().enumerate().skip(1) {
            owner[i] = owner[i].or_else(|| inst.parent.and_then(|p| owner[p.0]));
            if owner[i].is_none() {
                return Err(invalid("additional sibling subtree"));
            }
        }
        let flat = FastProgram::new(d)?;
        let root = |instance: InstanceId| owner[instance.0].is_none();
        let parent_edge = if d.regs.iter().any(|r| root(r.source.instance))
            || d.memories.iter().any(|m| root(m.source.instance))
            || d.assertions.iter().any(|a| root(a.source.instance))
        {
            compile::compile_flat(d, &[], root)?.edge
        } else {
            Vec::new()
        };
        let parent_assertions = d
            .assertions
            .iter()
            .enumerate()
            .filter_map(|(i, a)| root(a.source.instance).then_some(AssertId(i)))
            .collect();
        let mut mem_base = d
            .signals
            .iter()
            .map(|s| s.width.div_ceil(64) as usize)
            .sum::<usize>();
        let mut member_memories = std::collections::BTreeSet::new();
        for mem in &d.memories {
            if !root(mem.source.instance) {
                member_memories.insert(mem_base);
            }
            mem_base += mem.depth as usize;
        }
        let lane_of = |id: crate::SignalId| owner[d.signal(id).source.instance.0];
        let mut local = vec![Vec::<CombId>::new(); lanes];
        let mut to_local = vec![false; d.signals.len()];
        let mut from_local = vec![false; d.signals.len()];
        let mut pre = Vec::new();
        let mut post = Vec::new();
        let mut after_local = vec![false; d.signals.len()];
        let mut start = 0;
        for (&id, &end) in d.comb_eval_order.iter().zip(&flat.comb_ends) {
            let a = &d.comb[id.0];
            let ops = &flat.comb[start..end];
            start = end;
            let target = a.target.signal;
            let mut reads = Vec::new();
            crate::schedule::collect(&a.value, d, &mut reads);
            let reads_member_ram = ops
                .iter()
                .any(|op| matches!(op,FastOp::Gather {base,..} if member_memories.contains(base)));
            if reads_member_ram && lane_of(target).is_none() {
                return Err(invalid("top reads private member RAM"));
            }
            let needs_member = reads
                .iter()
                .any(|r| lane_of(r.signal).is_some() || after_local[r.signal.0])
                || reads_member_ram;
            match lane_of(target) {
                Some(i) if needs_member => {
                    if reads.iter().any(|r| lane_of(r.signal) != Some(i)) {
                        return Err(invalid("member depends on another subtree"));
                    }
                    local[i].push(id);
                }
                Some(_) => {
                    pre.extend_from_slice(ops);
                    to_local[target.0] = true;
                }
                None if needs_member => {
                    post.extend_from_slice(ops);
                    after_local[target.0] = true;
                    for r in reads {
                        if lane_of(r.signal).is_some() {
                            from_local[r.signal.0] = true;
                        }
                    }
                }
                None => pre.extend_from_slice(ops),
            }
        }
        // Parent registers and assertions may sample a member signal directly.
        let mut sampled = vec![false; flat.slots];
        for operand in parent_edge.iter().flat_map(|op| op.operands()).flatten() {
            sampled[operand] = true;
        }
        for (i, &(base, width)) in flat.signals.iter().enumerate() {
            if lane_of(crate::SignalId(i)).is_some()
                && sampled[base..base + width.div_ceil(64) as usize]
                    .iter()
                    .any(|&v| v)
            {
                from_local[i] = true;
            }
        }
        // Global clock and reset signals may be sampled directly by a member edge.
        let mut shared = d
            .clocks
            .iter()
            .map(|c| c.signal.0)
            .chain(d.resets.iter().map(|r| r.signal.0))
            .filter(|&s| owner[d.signals[s].source.instance.0].is_none())
            .collect::<Vec<_>>();
        shared.sort_unstable();
        shared.dedup();
        for &s in &shared {
            to_local[s] = true;
        }
        let mut first: Option<FastProgram> = None;
        let mut inbound = Vec::new();
        let mut outbound = Vec::new();
        let mut assertions = Vec::new();
        for (lane, order) in local.iter().enumerate() {
            let owns = |id: InstanceId| owner[id.0] == Some(lane);
            let slots = shared.iter().copied().chain(
                d.signals
                    .iter()
                    .enumerate()
                    .filter_map(|(i, s)| owns(s.source.instance).then_some(i)),
            );
            let p = compile::compile_scope(d, order, slots, owns, false)?;
            if let Some(ref sample) = first
                && (p.comb != sample.comb || p.edge != sample.edge || p.slots != sample.slots)
            {
                return Err(invalid("member programs differ"));
            }
            for (s, &(base, w)) in p.signals.iter().enumerate().filter(|(_, (_, w))| *w != 0) {
                for i in 0..w.div_ceil(64) as usize {
                    let pair = (flat.signals[s].0 + i, (base + i) * lanes + lane);
                    if to_local[s] {
                        inbound.push(pair);
                    }
                    if from_local[s] {
                        outbound.push(pair);
                    }
                }
            }
            assertions.push(
                d.assertions
                    .iter()
                    .enumerate()
                    .filter_map(|(i, a)| owns(a.source.instance).then_some(AssertId(i)))
                    .collect(),
            );
            if first.is_none() {
                first = Some(p);
            }
        }
        let local = first.unwrap();
        if local.storage_bytes(lanes).is_none() || flat.storage_bytes(1).is_none() {
            return Err(invalid("state exceeds host address space"));
        }
        Ok(Self {
            local,
            external: flat,
            parent_edge,
            parent_assertions,
            lanes,
            ports: Ports::new(d),
            pre,
            post,
            inbound,
            outbound,
            assertions,
        })
    }

    pub fn run_trajectory(&self, seed: u64, cycles: u64) -> crate::SingleTraceResult {
        let ports = &self.ports;
        let mut scalar = vec![0; self.external.slots];
        let mut members = vec![0; self.local.slots * self.lanes];
        let mut scratch = vec![0; self.lanes];
        let mut failure = None;
        let comb = |scalar: &mut [u64], members: &mut [u64], scratch: &mut [u64]| {
            kernel::execute_scalar(&self.pre, scalar, false);
            for &(parent, local) in &self.inbound {
                members[local] = scalar[parent];
            }
            kernel::execute_family(&self.local.comb, members, scratch, &[]);
        };
        let gather = |scalar: &mut [u64], members: &[u64]| {
            for &(parent, local) in &self.outbound {
                scalar[parent] = members[local];
            }
            kernel::execute_scalar(&self.post, scalar, false);
        };
        for cycle in 1..=cycles {
            for p in &ports.inputs {
                for limb in 0..p.width.div_ceil(64) {
                    scalar[self.external.signals[p.id.0].0 + limb as usize] =
                        p.word(seed, 0, cycle, limb);
                }
            }
            if let Some(clk) = self.external.clock {
                scalar[clk] = 1;
            }
            comb(&mut scalar, &mut members, &mut scratch);
            let parent_failure = if self.parent_edge.is_empty() {
                None
            } else {
                gather(&mut scalar, &members);
                kernel::execute_scalar(&self.parent_edge, &mut scalar, false)
                    .map(|(_, id)| self.parent_assertions[id])
            };
            let member_failure = kernel::execute_family(
                &self.local.edge,
                &mut members,
                &mut scratch,
                &self.assertions,
            )
            .map(|(_, id)| AssertId(id));
            if failure.is_none() {
                failure = parent_failure
                    .into_iter()
                    .chain(member_failure)
                    .min()
                    .map(|id| (cycle, id));
            }
        }
        if let Some(clk) = self.external.clock {
            scalar[clk] = 0;
        }
        comb(&mut scalar, &mut members, &mut scratch);
        gather(&mut scalar, &members);
        let outputs = ports
            .outputs
            .iter()
            .map(|&(id, limb)| {
                let (base, w) = self.external.signals[id.0];
                scalar[base + limb as usize] & super::mask((w - 64 * limb).min(64))
            })
            .collect();
        crate::SingleTraceResult { outputs, failure }
    }
}
