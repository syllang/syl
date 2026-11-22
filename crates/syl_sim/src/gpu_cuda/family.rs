//! Partition a top-level array of independent modules at its port connections.
//! The leader packs output slices after all lanes publish local results.
use super::emit::Emit;
use crate::{HwDirection, InstanceFamily, SimDesign, SimExpr};
use std::fmt::Write;

pub(super) fn functions(
    d: &SimDesign,
    bases: &[usize],
    family: &InstanceFamily,
) -> Result<String, String> {
    if !d.memories.is_empty()
        || !d.assertions.is_empty()
        || family.members.len() > 1024
        || family.members.is_empty()
    {
        return Err(
            "family backend requires independent register-only members fitting one block".into(),
        );
    }
    let mut owner = vec![None; d.instances.len()];
    for (i, id) in family.members.iter().enumerate() {
        if d.instances[id.0].parent != Some(crate::InstanceId(0)) {
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
    if d.regs.iter().any(|r| lane_for(r.signal).is_none()) {
        return Err("top register requires a separate sequential partition".into());
    }
    let f = family.members.len();
    let mut rows = Vec::new();
    let mut first: Option<(String, String, String)> = None;
    for lane in 0..f {
        let slots = (0..d.signals.len())
            .filter(|i| lane_for(crate::SignalId(*i)) == Some(lane))
            .collect::<Vec<_>>();
        rows.push(format!(
            "{{{}}}",
            slots
                .iter()
                .map(|i| bases[*i].to_string())
                .collect::<Vec<_>>()
                .join(",")
        ));
        let e = Emit {
            design: d,
            bases,
            local: Some(&slots),
            lane,
            lanes: f,
        };
        let mut comb = String::new();
        let mut gather = String::new();
        for id in &d.comb_eval_order {
            let a = &d.comb[id.0];
            if let Some(i) = lane_for(a.target.signal) {
                if i == lane {
                    comb.push_str(&e.assign(&a.target, &a.value)?);
                }
            } else {
                let SimExpr::Read(t) = &a.value else {
                    return Err("top gather requires a direct lane output".into());
                };
                if !d
                    .top_ports()
                    .iter()
                    .any(|p| p.signal == a.target.signal && p.direction == HwDirection::Out)
                    || lane_for(t.signal).is_none()
                {
                    return Err("family gather must drive a top output".into());
                }
                if lane_for(t.signal) == Some(lane) {
                    gather.push_str(&e.assign(&a.target, &a.value)?);
                }
            }
        }
        let edge = e.edge()?;
        if let Some((c, g, r)) = &first {
            let sorted = |s: &str| {
                let mut lines = s.lines().map(str::to_owned).collect::<Vec<_>>();
                lines.sort();
                lines
            };
            if sorted(c) != sorted(&comb) || sorted(g) != sorted(&gather) || r != &edge {
                return Err("member wiring or sequential logic differs across the family".into());
            }
        } else {
            first = Some((comb, gather, edge))
        }
    }
    let (comb, gather, edge) = first.unwrap();
    let width = d
        .signals
        .iter()
        .filter(|s| owner[s.source.instance.0] == Some(0))
        .count();
    let mut out = format!(
        "__device__ const unsigned family_slot_map[{f}][{width}] = {{{}}};\n",
        rows.join(",\n")
    );
    writeln!(
        out,
        "__device__ void eval_comb(Frame f, unsigned lane) {{\n{comb}}}"
    )
    .unwrap();
    writeln!(
        out,
        "__device__ void step_clock(Frame f, unsigned lane, unsigned cycle) {{\n{edge}}}"
    )
    .unwrap();
    writeln!(out,"__device__ void gather(Frame f) {{ for(unsigned lane=0;lane<FAMILY;++lane) {{\n{gather}}} }}").unwrap();
    Ok(out)
}
