//! Conservative launch/sample analysis, composed through formal module ports.
//! Clocked boundaries cut data dependencies; combinational edges reach a finite
//! fixed point. Source origins travel with launches for actionable diagnostics.
use crate::{
    CompileError, DriverError,
    driver::DriverFacts,
    eir::{
        EirDesign, EirDirection, EirExpr, EirInstance, EirItem, EirModule, EirOrigin, EirPortDomain,
    },
};
use std::collections::{BTreeMap, BTreeSet};
use syl_sema::{OpaqueSummaryTable, SummaryCdcCrossing};

#[derive(Clone, Debug, PartialEq, Eq)]
struct ClockOrigin {
    origin: EirOrigin,
    source_signal: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Flow {
    launches: BTreeMap<String, ClockOrigin>,
    inputs: BTreeSet<String>,
}
impl Flow {
    fn merge(&mut self, other: &Self) -> bool {
        let before = (self.launches.len(), self.inputs.len());
        for (clock, origin) in &other.launches {
            self.launches
                .entry(clock.clone())
                .or_insert_with(|| origin.clone());
        }
        self.inputs.extend(other.inputs.iter().cloned());
        before != (self.launches.len(), self.inputs.len())
    }
    fn launched(clock: String, source_signal: String, origin: &EirOrigin) -> Self {
        Self {
            launches: BTreeMap::from([(
                clock,
                ClockOrigin {
                    origin: origin.clone(),
                    source_signal,
                },
            )]),
            inputs: BTreeSet::new(),
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ModuleCdcSummary {
    port_launch_clocks: BTreeMap<String, BTreeMap<String, ClockOrigin>>,
    port_sample_clocks: BTreeMap<String, BTreeSet<String>>,
    comb_passthrough: BTreeMap<String, BTreeSet<String>>,
    trusted_async_inputs: BTreeMap<String, BTreeSet<String>>,
    trusted_crossing_outputs: BTreeMap<String, String>,
}
struct Sample {
    clock: String,
    target: String,
    reads: Vec<EirExpr>,
    origin: EirOrigin,
}

pub(super) struct CdcChecker<'a> {
    eir: &'a EirDesign,
    _facts: &'a DriverFacts,
    opaque: &'a OpaqueSummaryTable,
}
impl<'a> CdcChecker<'a> {
    pub(super) fn new(
        eir: &'a EirDesign,
        facts: &'a DriverFacts,
        opaque: &'a OpaqueSummaryTable,
    ) -> Self {
        Self {
            eir,
            _facts: facts,
            opaque,
        }
    }
    pub(super) fn collect_errors(&self) -> Vec<CompileError> {
        // Compose children before parents, independent of source declaration order.
        // Iterate summaries as well as local edges to reach a design fixed point.
        let ordered = dependency_order(self.eir);
        let mut summaries = BTreeMap::new();
        loop {
            let mut changed = false;
            for module in &ordered {
                let summary = self.analyze(module, &summaries).0;
                if summaries.get(module.name()) != Some(&summary) {
                    summaries.insert(module.name().to_owned(), summary);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        let mut errors = Vec::new();
        let mut seen = BTreeSet::new();
        for module in self.eir.modules().iter().filter(|m| !m.is_extern()) {
            let (_, flows, samples) = self.analyze(module, &summaries);
            for sample in samples {
                let flow = expression_flow(&sample.reads, &flows);
                for (source_clock, source) in flow.launches {
                    if source_clock == sample.clock {
                        continue;
                    }
                    let key = (
                        module.name().to_owned(),
                        sample.target.clone(),
                        source.source_signal.clone(),
                        source_clock.clone(),
                        sample.clock.clone(),
                    );
                    if seen.insert(key) {
                        errors.push(CompileError::driver_error_with_related(
                            DriverError::CdcViolation {
                                source_domain: source_clock.clone(),
                                target_domain: sample.clock.clone(),
                                signal: source.source_signal.clone(),
                                target: sample.target.clone(),
                            },
                            sample.origin.span(),
                            vec![(
                                source.origin.span(),
                                format!("{} launched by {source_clock}", source.source_signal),
                            )],
                        ));
                    }
                }
            }
        }
        errors
    }

    fn crossings<'m>(&'m self, module: &'m EirModule) -> &'m [SummaryCdcCrossing] {
        self.opaque
            .get(&module.canonical_path().join("."))
            .or_else(|| self.opaque.get(module.name()))
            .filter(|s| !s.cdc_crossings().is_empty())
            .map(|s| s.cdc_crossings())
            .unwrap_or_else(|| module.cdc_crossings())
    }

    fn analyze(
        &self,
        module: &EirModule,
        summaries: &BTreeMap<String, ModuleCdcSummary>,
    ) -> (ModuleCdcSummary, BTreeMap<String, Flow>, Vec<Sample>) {
        let mut items = Vec::new();
        flatten(module.items(), &mut items);
        let aliases = items
            .iter()
            .filter_map(|i| {
                if let EirItem::Drive {
                    lhs: crate::eir::EirPlace::Ident(name),
                    rhs: EirExpr::Ident(source),
                    ..
                } = i
                {
                    Some((name.clone(), source.clone()))
                } else {
                    None
                }
            })
            .collect::<BTreeMap<_, _>>();
        let clock_key = |expr: &EirExpr| canonical_clock(&expr.fact_key(), &aliases);
        let mut summary = ModuleCdcSummary::default();
        for crossing in self.crossings(module) {
            summary
                .trusted_async_inputs
                .entry(crossing.input_port().into())
                .or_default()
                .insert(canonical_clock(crossing.dst_clock(), &aliases));
            summary.trusted_crossing_outputs.insert(
                crossing.output_port().into(),
                canonical_clock(crossing.dst_clock(), &aliases),
            );
        }
        let mut flows = BTreeMap::<String, Flow>::new();
        let mut samples = Vec::new();
        let mut memory_reads = Vec::new();
        let mut memory_data = Vec::new();
        let mut edges = Vec::<(String, Vec<EirExpr>)>::new();
        let data_inputs = module
            .ports()
            .iter()
            .filter(|p| {
                p.direction() != EirDirection::Out && matches!(p.domain(), EirPortDomain::Data)
            })
            .map(|p| p.name().to_owned())
            .collect::<BTreeSet<_>>();
        for input in &data_inputs {
            flows
                .entry(input.clone())
                .or_default()
                .inputs
                .insert(input.clone());
        }
        if module.is_extern() {
            let clocks = module
                .ports()
                .iter()
                .filter(|p| matches!(p.domain(), EirPortDomain::Clock { .. }))
                .map(|p| p.name().to_owned())
                .collect::<BTreeSet<_>>();
            for input in &data_inputs {
                let sample_clocks = summary
                    .trusted_async_inputs
                    .get(input)
                    .cloned()
                    .unwrap_or_else(|| clocks.clone());
                for clock in sample_clocks {
                    samples.push(Sample {
                        clock,
                        target: input.clone(),
                        reads: vec![EirExpr::ident(input)],
                        origin: module
                            .ports()
                            .iter()
                            .find(|p| p.name() == input)
                            .unwrap()
                            .origin()
                            .clone(),
                    });
                }
            }
            for output in module.ports().iter().filter(|p| {
                p.direction() != EirDirection::In && matches!(p.domain(), EirPortDomain::Data)
            }) {
                if clocks.is_empty() {
                    edges.push((
                        output.name().into(),
                        data_inputs.iter().map(EirExpr::ident).collect(),
                    ));
                } else {
                    for clock in &clocks {
                        flows
                            .entry(output.name().into())
                            .or_default()
                            .merge(&Flow::launched(
                                clock.clone(),
                                output.name().into(),
                                output.origin(),
                            ));
                    }
                }
            }
        }
        for item in items {
            match item {
                EirItem::ClockedStorage {
                    clock,
                    target,
                    reads,
                    origin,
                    ..
                }
                | EirItem::MemorySyncRead {
                    clock,
                    target,
                    reads,
                    origin,
                    ..
                } => {
                    if let Some(root) = place_root(&target.to_expr()) {
                        flows.entry(root.into()).or_default().merge(&Flow::launched(
                            clock_key(clock),
                            root.into(),
                            origin,
                        ));
                        let mut sample_reads = reads.clone();
                        if let EirItem::MemorySyncRead { memory, .. } = item {
                            sample_reads.push(EirExpr::ident(memory));
                            memory_reads.push((samples.len(), memory.clone(), root.to_owned()));
                        }
                        samples.push(Sample {
                            clock: clock_key(clock),
                            target: root.into(),
                            reads: sample_reads,
                            origin: origin.clone(),
                        });
                    }
                }
                EirItem::MemoryWrite {
                    clock,
                    memory,
                    data,
                    reads,
                    origin,
                    ..
                } => {
                    memory_data.push((memory.clone(), data.clone()));
                    flows
                        .entry(memory.clone())
                        .or_default()
                        .merge(&Flow::launched(clock_key(clock), memory.clone(), origin));
                    samples.push(Sample {
                        clock: clock_key(clock),
                        target: memory.clone(),
                        reads: reads.clone(),
                        origin: origin.clone(),
                    });
                }
                EirItem::ClockedLog {
                    clock,
                    reads,
                    origin,
                    ..
                }
                | EirItem::ClockedAssert {
                    clock,
                    reads,
                    origin,
                    ..
                } => samples.push(Sample {
                    clock: clock_key(clock),
                    target: if matches!(item, EirItem::ClockedLog { .. }) {
                        "log"
                    } else {
                        "assert"
                    }
                    .into(),
                    reads: reads
                        .iter()
                        .filter(|r| {
                            !module.ports().iter().any(|p| {
                                matches!(p.domain(), EirPortDomain::Reset { .. })
                                    && canonical_clock(&r.fact_key(), &aliases) == p.name()
                            })
                        })
                        .cloned()
                        .collect(),
                    origin: origin.clone(),
                }),
                EirItem::Drive { lhs, reads, .. } => {
                    if let Some(root) = place_root(&lhs.to_expr()) {
                        edges.push((root.into(), reads.clone()));
                    }
                }
                EirItem::Instance(instance) => {
                    let Some(child) = summaries.get(instance.module()) else {
                        continue;
                    };
                    self.instance_relations(
                        instance,
                        child,
                        &aliases,
                        &mut flows,
                        &mut edges,
                        &mut samples,
                    );
                }
                _ => {}
            }
        }
        // A trusted output is a new launch in the destination domain. Never
        // union the asynchronous input's launch through its implementation.
        for (output, clock) in &summary.trusted_crossing_outputs {
            if let Some(port) = module.ports().iter().find(|p| p.name() == output) {
                flows.insert(
                    output.clone(),
                    Flow::launched(clock.clone(), output.clone(), port.origin()),
                );
            }
        }
        loop {
            let mut changed = false;
            for (target, reads) in &edges {
                if summary.trusted_crossing_outputs.contains_key(target) {
                    continue;
                }
                let flow = expression_flow(reads, &flows);
                changed |= flows.entry(target.clone()).or_default().merge(&flow);
            }
            if !changed {
                break;
            }
        }
        // A declared RAM bridge authorizes only its data path into the named
        // destination output. Address/enable samples remain checked normally.
        for (index, memory, target) in memory_reads {
            let sample = &mut samples[index];
            let trusted = self.crossings(module).iter().any(|crossing| {
                canonical_clock(crossing.dst_clock(), &aliases) == sample.clock
                    && combinationally_reaches(&target, crossing.output_port(), &edges)
                    && memory_data.iter().any(|(name, data)| {
                        name == &memory
                            && expression_flow(std::slice::from_ref(data), &flows)
                                .inputs
                                .contains(crossing.input_port())
                    })
            });
            if trusted {
                sample.reads.pop(); // only the synthetic memory-content read
            }
        }
        for sample in &samples {
            for input in expression_flow(&sample.reads, &flows).inputs {
                if summary
                    .trusted_async_inputs
                    .get(&input)
                    .is_some_and(|clocks| clocks.contains(&sample.clock))
                {
                    continue;
                }
                summary
                    .port_sample_clocks
                    .entry(input)
                    .or_default()
                    .insert(sample.clock.clone());
            }
        }
        for port in module.ports().iter().filter(|p| {
            p.direction() != EirDirection::In && matches!(p.domain(), EirPortDomain::Data)
        }) {
            if let Some(flow) = flows.get(port.name()) {
                summary
                    .port_launch_clocks
                    .insert(port.name().into(), flow.launches.clone());
                summary
                    .comb_passthrough
                    .insert(port.name().into(), flow.inputs.clone());
            }
        }
        (summary, flows, samples)
    }

    #[allow(clippy::too_many_arguments)]
    fn instance_relations(
        &self,
        instance: &EirInstance,
        child: &ModuleCdcSummary,
        aliases: &BTreeMap<String, String>,
        flows: &mut BTreeMap<String, Flow>,
        edges: &mut Vec<(String, Vec<EirExpr>)>,
        samples: &mut Vec<Sample>,
    ) {
        let actual = |formal: &str| {
            instance
                .connections()
                .iter()
                .find(|c| c.formal() == formal)
                .map(|c| c.actual())
        };
        for (output, launches) in &child.port_launch_clocks {
            let Some(target) = actual(output).and_then(place_root) else {
                continue;
            };
            for (clock, origin) in launches {
                let Some(clock) = actual(clock) else {
                    continue;
                };
                flows
                    .entry(target.into())
                    .or_default()
                    .launches
                    .entry(canonical_clock(&clock.fact_key(), aliases))
                    .or_insert_with(|| origin.clone());
            }
        }
        for (output, inputs) in &child.comb_passthrough {
            if let Some(target) = actual(output).and_then(place_root) {
                edges.push((
                    target.into(),
                    inputs.iter().filter_map(|i| actual(i).cloned()).collect(),
                ));
            }
        }
        for (input, clocks) in &child.port_sample_clocks {
            let Some(value) = actual(input) else {
                continue;
            };
            for clock in clocks {
                if let Some(clock) = actual(clock) {
                    samples.push(Sample {
                        clock: canonical_clock(&clock.fact_key(), aliases),
                        target: format!("{}.{}", instance.name(), input),
                        reads: vec![value.clone()],
                        origin: instance.origin().clone(),
                    });
                }
            }
        }
    }
}

fn combinationally_reaches(source: &str, output: &str, edges: &[(String, Vec<EirExpr>)]) -> bool {
    let mut flows = BTreeMap::from([(
        source.to_owned(),
        Flow {
            inputs: BTreeSet::from([source.to_owned()]),
            ..Flow::default()
        },
    )]);
    loop {
        let mut changed = false;
        for (target, reads) in edges {
            let flow = expression_flow(reads, &flows);
            changed |= flows.entry(target.clone()).or_default().merge(&flow);
        }
        if !changed {
            return flows.get(output).is_some_and(|f| f.inputs.contains(source));
        }
    }
}

fn dependency_order(design: &EirDesign) -> Vec<&EirModule> {
    fn visit<'a>(
        module: &'a EirModule,
        modules: &BTreeMap<&str, &'a EirModule>,
        seen: &mut BTreeSet<String>,
        ordered: &mut Vec<&'a EirModule>,
    ) {
        if !seen.insert(module.name().into()) {
            return;
        }
        let mut items = Vec::new();
        flatten(module.items(), &mut items);
        let children = items
            .iter()
            .filter_map(|item| match item {
                EirItem::Instance(instance) => Some(instance.module()),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        for child in children {
            if let Some(child) = modules.get(child) {
                visit(child, modules, seen, ordered);
            }
        }
        ordered.push(module);
    }
    let modules = design
        .modules()
        .iter()
        .map(|m| (m.name(), m))
        .collect::<BTreeMap<_, _>>();
    let mut seen = BTreeSet::new();
    let mut ordered = Vec::new();
    for module in modules.values() {
        visit(module, &modules, &mut seen, &mut ordered);
    }
    ordered
}

fn canonical_clock(name: &str, aliases: &BTreeMap<String, String>) -> String {
    let mut key = name;
    let mut visited = BTreeSet::new();
    while visited.insert(key) {
        if let Some(source) = aliases.get(key) {
            key = source;
        } else {
            break;
        }
    }
    key.into()
}
fn place_root(expr: &EirExpr) -> Option<&str> {
    match expr {
        EirExpr::Ident(name) => Some(name),
        EirExpr::Slice { value, .. }
        | EirExpr::Index { value, .. }
        | EirExpr::IndexedPartSelect { value, .. } => place_root(value),
        _ => None,
    }
}
fn expression_flow(exprs: &[EirExpr], flows: &BTreeMap<String, Flow>) -> Flow {
    let mut result = Flow::default();
    for expr in exprs {
        match expr {
            EirExpr::Ident(name) => {
                if let Some(flow) = flows.get(name) {
                    result.merge(flow);
                }
            }
            EirExpr::Unary { expr, .. } | EirExpr::Slice { value: expr, .. } => {
                result.merge(&expression_flow(std::slice::from_ref(expr.as_ref()), flows));
            }
            EirExpr::Binary { left, right, .. }
            | EirExpr::Index {
                value: left,
                index: right,
            }
            | EirExpr::IndexedPartSelect {
                value: left,
                index: right,
                ..
            } => {
                result.merge(&expression_flow(&[*left.clone(), *right.clone()], flows));
            }
            EirExpr::Mux {
                cond,
                then_value,
                else_value,
            } => {
                result.merge(&expression_flow(
                    &[*cond.clone(), *then_value.clone(), *else_value.clone()],
                    flows,
                ));
            }
            EirExpr::Select { arms, default, .. } => {
                for arm in arms {
                    result.merge(&expression_flow(
                        &[arm.guard().clone(), arm.value().clone()],
                        flows,
                    ));
                }
                result.merge(&expression_flow(
                    std::slice::from_ref(default.as_ref()),
                    flows,
                ));
            }
            EirExpr::Concat(parts) | EirExpr::Call { args: parts, .. } => {
                result.merge(&expression_flow(parts, flows));
            }
            _ => {}
        }
    }
    result
}
fn flatten<'a>(items: &'a [EirItem], out: &mut Vec<&'a EirItem>) {
    for item in items {
        match item {
            EirItem::CellExpansion(e) => flatten(e.items(), out),
            EirItem::SymbolicStaticIf {
                then_items,
                else_items,
                ..
            } => {
                flatten(then_items, out);
                flatten(else_items, out);
            }
            EirItem::SymbolicStaticFor { items, .. } => flatten(items, out),
            _ => out.push(item),
        }
    }
}

#[cfg(test)]
mod tests;
