use super::{EirBound, EirExpr, EirGuard, EirGuardFrame, EirOrigin, EirPlace};
use super::{
    EirDesignFacts, EirDirection, EirDrive, EirDriveInput, EirDriveKind, EirInstance, EirItem,
    EirModule, EirObject, EirObjectKind, EirRead, EirSignalActivity,
};
use crate::{CompileError, DriverError, EirError};
use std::collections::{BTreeMap, BTreeSet};
use syl_sema::OpaqueSummaryTable;

#[non_exhaustive]
pub(crate) struct EirFactCollector {
    pub(crate) objects: Vec<EirObject>,
    pub(crate) drives: Vec<EirDrive>,
    pub(crate) reads: Vec<EirRead>,
    opaque_summaries: OpaqueSummaryTable,
    module_ports: BTreeMap<String, Vec<(String, EirDirection)>>,
    guard_stack: Vec<EirGuardFrame>,
    module: String,
}

impl EirFactCollector {
    pub(crate) fn collect(
        modules: &[EirModule],
        opaque_summaries: &OpaqueSummaryTable,
    ) -> Result<EirDesignFacts, CompileError> {
        let mut collector = Self::new(opaque_summaries);
        collector.collect_modules(modules)?;
        Ok(EirDesignFacts::new(
            collector.objects,
            collector.drives,
            collector.reads,
        ))
    }

    pub(crate) fn new(opaque_summaries: &OpaqueSummaryTable) -> Self {
        Self {
            objects: Vec::new(),
            drives: Vec::new(),
            reads: Vec::new(),
            opaque_summaries: opaque_summaries.clone(),
            module_ports: BTreeMap::new(),
            guard_stack: Vec::new(),
            module: String::new(),
        }
    }

    pub(crate) fn collect_modules(&mut self, modules: &[EirModule]) -> Result<(), CompileError> {
        self.index_module_ports(modules);
        for module in modules {
            self.module = module.name().to_string();
            self.guard_stack.clear();
            self.collect_items(module.items())?;
        }
        Ok(())
    }

    fn index_module_ports(&mut self, modules: &[EirModule]) {
        self.module_ports.clear();
        for module in modules {
            let ports = module
                .ports()
                .iter()
                .map(|port| (port.name().to_string(), port.direction()))
                .collect();
            self.module_ports.insert(module.name().to_string(), ports);
        }
    }

    fn collect_items(&mut self, items: &[EirItem]) -> Result<(), CompileError> {
        for item in items {
            match item {
                EirItem::StaticParam { .. } => {}
                EirItem::Signal {
                    activity,
                    width,
                    name,
                    origin,
                    ..
                } => self.objects.push(EirObject::new(super::EirObjectInput {
                    module: self.module.clone(),
                    name: name.clone(),
                    width: width.clone(),
                    kind: EirObjectKind::Signal,
                    activity: *activity,
                    origin: origin.clone(),
                })),
                EirItem::Storage {
                    width,
                    name,
                    origin,
                    ..
                } => self.objects.push(EirObject::new(super::EirObjectInput {
                    module: self.module.clone(),
                    name: name.clone(),
                    width: width.clone(),
                    kind: EirObjectKind::Storage,
                    activity: EirSignalActivity::Required,
                    origin: origin.clone(),
                })),
                EirItem::Drive {
                    lhs,
                    rhs,
                    reads,
                    origin,
                    ..
                } => {
                    self.drives.push(EirDrive::new(EirDriveInput {
                        module: self.module.clone(),
                        target: lhs.clone(),
                        kind: EirDriveKind::Continuous,
                        value: Some(rhs.clone()),
                        guard: self.guard(),
                        origin: origin.clone(),
                    }));
                    self.record_reads(reads, origin)?;
                }
                EirItem::ClockedStorage {
                    target,
                    reads,
                    origin,
                    ..
                } => {
                    self.drives.push(EirDrive::new(EirDriveInput {
                        module: self.module.clone(),
                        target: target.clone(),
                        kind: EirDriveKind::Next,
                        value: None,
                        guard: self.guard(),
                        origin: origin.clone(),
                    }));
                    self.record_reads(reads, origin)?;
                }
                EirItem::CellExpansion(expansion) => {
                    let _cell_identity = (expansion.callable(), expansion.instance());
                    self.collect_items(expansion.items())?;
                }
                EirItem::SymbolicStaticIf {
                    label,
                    origin,
                    then_items,
                    else_items,
                    ..
                } => {
                    let span = origin.span();
                    self.with_guard(EirGuardFrame::if_then(label, span), then_items)?;
                    self.with_guard(EirGuardFrame::if_else(label, span), else_items)?;
                }
                EirItem::SymbolicStaticFor {
                    index,
                    start,
                    end,
                    label,
                    origin,
                    items,
                    ..
                } => self.with_guard(
                    EirGuardFrame::loop_frame(
                        label,
                        index,
                        EirBound::from_expr(start.clone()),
                        EirBound::from_expr(end.clone()),
                        origin.span(),
                    ),
                    items,
                )?,
                EirItem::Instance(instance) => self.record_instance_facts(instance)?,
                EirItem::InitialError { .. } => {}
            }
        }
        Ok(())
    }

    fn with_guard(&mut self, guard: EirGuardFrame, items: &[EirItem]) -> Result<(), CompileError> {
        self.guard_stack.push(guard);
        self.collect_items(items)?;
        self.guard_stack.pop();
        Ok(())
    }

    fn record_reads(&mut self, reads: &[EirExpr], origin: &EirOrigin) -> Result<(), CompileError> {
        let guard = self.guard();
        for read in reads {
            let place = EirPlace::try_from(read).map_err(|_| {
                CompileError::lowering_at(
                    EirError::UnsupportedHardwareValueExpression,
                    origin.span(),
                )
            })?;
            self.reads.push(EirRead::new(
                &self.module,
                place,
                guard.clone(),
                origin.clone(),
            ));
        }
        Ok(())
    }

    fn record_instance_facts(&mut self, instance: &EirInstance) -> Result<(), CompileError> {
        if let Some(summary) = self.opaque_summaries.get(instance.module()).cloned() {
            return self.record_summary_instance_facts(instance, &summary);
        }
        self.record_signature_instance_facts(instance)
    }

    fn record_signature_instance_facts(
        &mut self,
        instance: &EirInstance,
    ) -> Result<(), CompileError> {
        let guard = self.guard();
        let ports = self
            .module_ports
            .get(instance.module())
            .cloned()
            .ok_or_else(|| {
                CompileError::lowering_at(
                    DriverError::UnknownHardwareObject {
                        module: self.module.clone(),
                        name: instance.module().to_string(),
                    },
                    instance.origin().span(),
                )
            })?;
        let hardware_roots = self.hardware_read_roots();
        for connection in instance.connections() {
            let Some((_, direction)) = ports
                .iter()
                .find(|(formal, _)| formal == connection.formal())
            else {
                return Err(CompileError::lowering_at(
                    DriverError::UnknownParameter {
                        name: connection.formal().to_string(),
                        callable: instance.module().to_string(),
                    },
                    instance.origin().span(),
                ));
            };
            match direction {
                EirDirection::In => {
                    for place in
                        EirReadPlaceCollector::new(&hardware_roots).collect(connection.actual())
                    {
                        self.reads.push(EirRead::new(
                            &self.module,
                            place,
                            guard.clone(),
                            instance.origin().clone(),
                        ));
                    }
                }
                EirDirection::InOut => {
                    for place in
                        EirReadPlaceCollector::new(&hardware_roots).collect(connection.actual())
                    {
                        self.reads.push(EirRead::new(
                            &self.module,
                            place,
                            guard.clone(),
                            instance.origin().clone(),
                        ));
                    }
                    let place = EirPlace::try_from(connection.actual()).map_err(|_| {
                        CompileError::lowering_at(
                            EirError::UnsupportedHardwareValueExpression,
                            instance.origin().span(),
                        )
                    })?;
                    self.drives.push(EirDrive::new(EirDriveInput {
                        module: self.module.clone(),
                        target: place,
                        kind: EirDriveKind::Continuous,
                        value: Some(EirExpr::Unsupported {
                            message: "instance inout enable is unknown".to_string(),
                        }),
                        guard: guard.clone(),
                        origin: instance.origin().clone(),
                    }));
                }
                EirDirection::Out => {
                    let place = EirPlace::try_from(connection.actual()).map_err(|_| {
                        CompileError::lowering_at(
                            EirError::UnsupportedHardwareValueExpression,
                            instance.origin().span(),
                        )
                    })?;
                    self.drives.push(EirDrive::new(EirDriveInput {
                        module: self.module.clone(),
                        target: place,
                        kind: EirDriveKind::Continuous,
                        value: None,
                        guard: guard.clone(),
                        origin: instance.origin().clone(),
                    }));
                }
            }
        }
        Ok(())
    }

    fn record_summary_instance_facts(
        &mut self,
        instance: &EirInstance,
        summary: &syl_sema::OpaqueItemSummary,
    ) -> Result<(), CompileError> {
        let guard = self.guard();
        let hardware_roots = self.hardware_read_roots();
        for path in summary.driven_fields() {
            let actual = self.instance_connection(instance, &path.flattened())?;
            let place = EirPlace::try_from(actual).map_err(|_| {
                CompileError::lowering_at(
                    EirError::UnsupportedHardwareValueExpression,
                    instance.origin().span(),
                )
            })?;
            self.drives.push(EirDrive::new(EirDriveInput {
                module: self.module.clone(),
                target: place,
                kind: EirDriveKind::Continuous,
                value: None,
                guard: guard.clone(),
                origin: instance.origin().clone(),
            }));
        }
        for path in summary.consumed_fields() {
            let actual = self.instance_connection(instance, &path.flattened())?;
            for place in EirReadPlaceCollector::new(&hardware_roots).collect(actual) {
                self.reads.push(EirRead::new(
                    &self.module,
                    place,
                    guard.clone(),
                    instance.origin().clone(),
                ));
            }
        }
        Ok(())
    }

    fn instance_connection<'a>(
        &self,
        instance: &'a EirInstance,
        formal: &str,
    ) -> Result<&'a EirExpr, CompileError> {
        instance
            .connections()
            .iter()
            .find(|connection| connection.formal() == formal)
            .map(|connection| connection.actual())
            .ok_or_else(|| {
                CompileError::lowering_at(
                    DriverError::UnknownParameter {
                        name: formal.to_string(),
                        callable: instance.module().to_string(),
                    },
                    instance.origin().span(),
                )
            })
    }

    fn hardware_read_roots(&self) -> BTreeSet<String> {
        let mut roots = BTreeSet::new();
        if let Some(ports) = self.module_ports.get(&self.module) {
            for (port, _) in ports {
                roots.insert(port.clone());
            }
        }
        for object in &self.objects {
            if object.module() == self.module {
                roots.insert(object.name().to_string());
            }
        }
        roots
    }

    fn guard(&self) -> EirGuard {
        if self.guard_stack.is_empty() {
            EirGuard::root()
        } else {
            EirGuard::from_frames(&self.guard_stack)
        }
    }
}

#[non_exhaustive]
struct EirReadPlaceCollector<'a> {
    places: Vec<EirPlace>,
    hardware_roots: &'a BTreeSet<String>,
}

impl<'a> EirReadPlaceCollector<'a> {
    fn new(hardware_roots: &'a BTreeSet<String>) -> Self {
        Self {
            places: Vec::new(),
            hardware_roots,
        }
    }

    fn collect(mut self, expr: &EirExpr) -> Vec<EirPlace> {
        self.collect_expr(expr);
        self.places.sort_by_key(|place| place.to_expr().fact_key());
        self.places.dedup_by_key(|place| place.to_expr().fact_key());
        self.places
    }

    fn collect_expr(&mut self, expr: &EirExpr) {
        self.collect_expr_with_root_filter(expr, RootFilter::AllowUnknown);
    }

    fn collect_known_root_expr(&mut self, expr: &EirExpr) {
        self.collect_expr_with_root_filter(expr, RootFilter::KnownOnly);
    }

    fn collect_expr_with_root_filter(&mut self, expr: &EirExpr, root_filter: RootFilter) {
        if let Ok(place) = EirPlace::try_from(expr) {
            if root_filter.allows(&place, self.hardware_roots) {
                self.places.push(place);
                self.collect_projection_indices(expr);
            }
            return;
        }
        match expr {
            EirExpr::Unary { expr, .. } => self.collect_expr_with_root_filter(expr, root_filter),
            EirExpr::Binary { left, right, .. } => {
                self.collect_expr_with_root_filter(left, root_filter);
                self.collect_expr_with_root_filter(right, root_filter);
            }
            EirExpr::Mux {
                cond,
                then_value,
                else_value,
            } => {
                self.collect_expr_with_root_filter(cond, root_filter);
                self.collect_expr_with_root_filter(then_value, root_filter);
                self.collect_expr_with_root_filter(else_value, root_filter);
            }
            EirExpr::Select { arms, default, .. } => {
                for arm in arms {
                    self.collect_expr_with_root_filter(arm.guard(), root_filter);
                    self.collect_expr_with_root_filter(arm.value(), root_filter);
                }
                self.collect_expr_with_root_filter(default, root_filter);
            }
            EirExpr::Concat(parts) => {
                for part in parts {
                    self.collect_expr_with_root_filter(part, root_filter);
                }
            }
            EirExpr::Slice { value, .. } => self.collect_expr_with_root_filter(value, root_filter),
            EirExpr::IndexedPartSelect { value, index, .. } | EirExpr::Index { value, index } => {
                self.collect_expr_with_root_filter(value, root_filter);
                self.collect_expr_with_root_filter(index, root_filter);
            }
            EirExpr::Call { args, .. } => {
                for arg in args {
                    self.collect_expr_with_root_filter(arg, root_filter);
                }
            }
            EirExpr::Ident(_)
            | EirExpr::Int(_)
            | EirExpr::Bool(_)
            | EirExpr::Str(_)
            | EirExpr::HighZ
            | EirExpr::Zero
            | EirExpr::Unsupported { .. } => {}
        }
    }

    fn collect_projection_indices(&mut self, expr: &EirExpr) {
        match expr {
            EirExpr::Slice { value, .. } => self.collect_projection_indices(value),
            EirExpr::IndexedPartSelect { value, index, .. } | EirExpr::Index { value, index } => {
                self.collect_projection_indices(value);
                self.collect_known_root_expr(index);
            }
            EirExpr::Ident(_)
            | EirExpr::Int(_)
            | EirExpr::Bool(_)
            | EirExpr::Str(_)
            | EirExpr::HighZ
            | EirExpr::Zero
            | EirExpr::Unary { .. }
            | EirExpr::Binary { .. }
            | EirExpr::Mux { .. }
            | EirExpr::Select { .. }
            | EirExpr::Concat(_)
            | EirExpr::Call { .. }
            | EirExpr::Unsupported { .. } => {}
        }
    }
}

#[derive(Clone, Copy)]
#[non_exhaustive]
enum RootFilter {
    AllowUnknown,
    KnownOnly,
}

impl RootFilter {
    fn allows(&self, place: &EirPlace, hardware_roots: &BTreeSet<String>) -> bool {
        match self {
            Self::AllowUnknown => true,
            Self::KnownOnly => EirPlaceRoot::new(place)
                .name()
                .is_some_and(|root| hardware_roots.contains(root)),
        }
    }
}

#[non_exhaustive]
struct EirPlaceRoot<'a> {
    place: &'a EirPlace,
}

impl<'a> EirPlaceRoot<'a> {
    fn new(place: &'a EirPlace) -> Self {
        Self { place }
    }

    fn name(&self) -> Option<&'a str> {
        let mut place = self.place;
        loop {
            match place {
                EirPlace::Ident(name) => return Some(name),
                EirPlace::Slice { base, .. }
                | EirPlace::IndexedPartSelect { base, .. }
                | EirPlace::Index { base, .. } => place = base,
            }
        }
    }
}
