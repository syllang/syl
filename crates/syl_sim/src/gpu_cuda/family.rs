//! Emit one transition function for a block of independent member subtrees.
//! The leader owns parent-side signals, memory and assertions.
use super::emit::{Emit, Resources};
use crate::{InstanceFamily, InstanceId, SimDesign, SimExpr};
use std::fmt::Write;

fn mapped_table(
    name: &str,
    lanes: &[Resources],
    indices: impl Fn(&Resources) -> &[usize],
    value: impl Fn(usize) -> usize,
) -> Result<String, String> {
    let width = indices(&lanes[0]).len();
    if lanes.iter().any(|r| indices(r).len() != width) {
        return Err(format!("members have different resource counts for {name}"));
    }
    if width == 0 {
        return Ok(String::new());
    }
    let rows = lanes
        .iter()
        .map(|r| {
            let values = indices(r)
                .iter()
                .map(|&i| value(i).to_string())
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{values}}}")
        })
        .collect::<Vec<_>>()
        .join(",\n");
    Ok(format!(
        "__device__ const size_t {name}[{}][{width}] = {{{rows}}};\n",
        lanes.len()
    ))
}

pub(super) fn functions(
    d: &SimDesign,
    bases: &[usize],
    family: &InstanceFamily,
) -> Result<String, String> {
    let f = family.members.len();
    if !(1..=1024).contains(&f) {
        return Err("family must fit in one CUDA block".into());
    }
    let mut owner = vec![None; d.instances.len()];
    for (i, id) in family.members.iter().enumerate() {
        if d.instances[id.0].parent != Some(InstanceId(0)) {
            return Err("family must consist of direct children".into());
        }
        owner[id.0] = Some(i);
    }
    for (i, inst) in d.instances.iter().enumerate().skip(1) {
        owner[i] = owner[i].or(inst.parent.and_then(|p| owner[p.0]));
        if owner[i].is_none() {
            return Err("design contains another instance outside the selected family".into());
        }
    }
    let lane_for = |id: crate::SignalId| owner[d.signal(id).source.instance.0];
    let lanes = (0..f)
        .map(|i| Resources::select(d, |id| owner[id.0] == Some(i)))
        .collect::<Vec<_>>();
    let mut out = mapped_table("family_slot_map", &lanes, |r| &r.slots, |i| bases[i])?;
    out += &mapped_table(
        "family_mem_base",
        &lanes,
        |r| &r.memories,
        |i| d.memories[..i].iter().map(|m| m.depth as usize).sum(),
    )?;
    out += &mapped_table("family_assert_id", &lanes, |r| &r.assertions, |i| i)?;

    let top = Emit {
        design: d,
        bases,
        local: None,
        lane: 0,
        lanes: f,
    };
    let root = Resources::select(d, |id| id == InstanceId(0));
    let mut top_comb = String::new();
    let mut first: Option<(String, String, (String, String))> = None;
    for (lane, local) in lanes.iter().enumerate() {
        let e = Emit {
            design: d,
            bases,
            local: Some(local),
            lane,
            lanes: f,
        };
        let mut comb = String::new();
        let mut gather = String::new();
        for id in &d.comb_eval_order {
            let a = &d.comb[id.0];
            if let Some(i) = lane_for(a.target.signal) {
                if i == lane {
                    comb += &e.assign(&a.target, &a.value)?;
                }
            } else if let SimExpr::Read(t) = &a.value
                && let Some(i) = lane_for(t.signal)
            {
                if i == lane {
                    gather += &e.assign(&a.target, &a.value)?;
                }
            } else if lane == 0 {
                top_comb += &top.assign(&a.target, &a.value)?;
            }
        }
        let edge = e.edge(local)?;
        if let Some((c, g, r)) = &first {
            // Independent assignments may be interleaved differently by the scheduler.
            let sorted = |s: &str| {
                let mut lines = s.lines().map(str::to_owned).collect::<Vec<_>>();
                lines.sort();
                lines
            };
            if sorted(c) != sorted(&comb) || sorted(g) != sorted(&gather) || r != &edge {
                return Err("member wiring or sequential logic differs across the family".into());
            }
        } else {
            first = Some((comb, gather, edge));
        }
    }
    let (comb, gather, (sample, commit)) = first.unwrap();
    writeln!(
        out,
        "__device__ void eval_comb(Frame f, unsigned lane) {{\n{comb}}}"
    )
    .unwrap();
    writeln!(out,"__device__ void gather(Frame f) {{ for(unsigned lane=0;lane<FAMILY;++lane) {{\n{gather}}}\n{top_comb}}}").unwrap();
    let (parent_sample, parent_commit) = top.edge(&root)?;
    writeln!(
        out,
        "__device__ void step_clock(Frame f, unsigned lane, unsigned cycle) {{\n{sample}"
    )
    .unwrap();
    if !parent_sample.is_empty() {
        // Members have sampled their old values. Gather parent inputs and execute
        // the parent edge before any lane is allowed to publish its updates.
        writeln!(out,"__syncthreads();\nif (lane==0) {{ gather(f);\n{parent_sample}{parent_commit}}}\n__syncthreads();").unwrap();
    }
    writeln!(out, "{commit}}}").unwrap();
    Ok(out)
}
