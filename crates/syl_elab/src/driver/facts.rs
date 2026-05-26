use super::{
    CellSummaryCollector, CreateFact, CreateFactInput, CreateKind, DriveEffect, DriveFact,
    DriverFacts, ReadFact,
};
use crate::{
    CompileError, DriverError,
    driver_place::{
        DriverExpr, DriverObjectTable, DriverPlace, DriverPlaceError, DriverPlaceResolver,
    },
    eir::{EirDesign, EirDriveKind, EirInstance, EirItem, EirObjectKind},
};
use std::collections::BTreeMap;
use syl_sema::{OpaqueItemKind, OpaqueSummaryTable};
use syl_span::SourceId;

#[non_exhaustive]
pub(crate) struct DriverFactsCollector<'a> {
    eir: &'a EirDesign,
    opaque_summaries: OpaqueSummaryTable,
    objects: DriverObjectTable,
    drives: Vec<DriveFact>,
    reads: Vec<ReadFact>,
    creates: Vec<CreateFact>,
    errors: Vec<CompileError>,
}

impl<'a> DriverFactsCollector<'a> {
    #[cfg(test)]
    pub(crate) fn new(eir: &'a EirDesign) -> Self {
        Self::with_opaque_summaries(eir, &OpaqueSummaryTable::new())
    }

    pub(crate) fn with_opaque_summaries(
        eir: &'a EirDesign,
        opaque_summaries: &OpaqueSummaryTable,
    ) -> Self {
        Self {
            eir,
            opaque_summaries: opaque_summaries.clone(),
            objects: DriverObjectTable::new(),
            drives: Vec::new(),
            reads: Vec::new(),
            creates: Vec::new(),
            errors: Vec::new(),
        }
    }

    pub(crate) fn collect(mut self) -> Result<DriverFacts, Vec<CompileError>> {
        self.index_objects();
        self.collect_creates();
        self.collect_drive_facts();
        self.collect_read_facts();
        if !self.errors.is_empty() {
            return Err(self.errors);
        }
        let mut cell_summaries =
            CellSummaryCollector::new(&self.drives, &self.reads, &self.creates)
                .collect()
                .finish();
        cell_summaries.extend(self.collect_source_instance_summaries());
        Ok(DriverFacts::new(
            self.objects,
            self.drives,
            self.reads,
            self.creates,
            cell_summaries,
        ))
    }

    fn index_objects(&mut self) {
        self.objects = DriverObjectTable::new();
        for module in self.eir.modules() {
            for port in module.ports() {
                self.objects
                    .intern_with_bound(module.name(), port.name(), port.width_bound());
            }
        }
        for object in self.eir.objects() {
            self.objects
                .intern_with_bound(object.module(), object.name(), object.width_bound());
        }
    }

    fn collect_creates(&mut self) {
        for object in self.eir.objects() {
            let kind = match object.kind() {
                EirObjectKind::Signal => CreateKind::Signal,
                EirObjectKind::Storage => CreateKind::Storage,
            };
            let object_id = self.objects.intern(object.module(), object.name());
            self.creates.push(CreateFact::new(CreateFactInput {
                module: object.module().to_string(),
                name: object.name().to_string(),
                object_id,
                kind,
                activity: object.activity(),
                origin: object.origin().clone(),
            }));
        }
    }

    fn collect_drive_facts(&mut self) {
        for drive in self.eir.drives() {
            let target = match self
                .place_resolver(drive.module())
                .resolve_place(drive.target_place())
            {
                Ok(target) => target,
                Err(error) => {
                    self.errors
                        .push(self.place_error(error, drive.origin().span()));
                    continue;
                }
            };
            let (target, effect) = match drive.kind() {
                EirDriveKind::Continuous => (target, DriveEffect::Continuous),
                EirDriveKind::Next => {
                    let storage_target = target;
                    let next_target = DriverPlace::Expr(DriverExpr::Ident(format!(
                        "{}.next",
                        storage_target.display()
                    )));
                    (
                        next_target,
                        DriveEffect::Next {
                            storage_target: Box::new(storage_target),
                        },
                    )
                }
            };
            self.drives.push(DriveFact::new(
                drive.module(),
                target,
                effect,
                drive.value().cloned(),
                drive.guard().clone(),
                drive.origin().clone(),
            ));
        }
    }

    fn collect_read_facts(&mut self) {
        for read in self.eir.reads() {
            let source = match self
                .place_resolver(read.module())
                .resolve_place(read.source_place())
            {
                Ok(source) => source,
                Err(error) => {
                    self.errors
                        .push(self.place_error(error, read.origin().span()));
                    continue;
                }
            };
            self.reads.push(ReadFact::new(
                read.module(),
                source,
                read.guard().clone(),
                read.origin().clone(),
            ));
        }
    }

    fn place_resolver<'module, 'objects>(
        &'objects self,
        module: &'module str,
    ) -> DriverPlaceResolver<'module, 'objects> {
        DriverPlaceResolver::new(module, &self.objects)
    }

    fn place_error(&self, error: DriverPlaceError, span: syl_span::Span) -> CompileError {
        let kind = match error {
            DriverPlaceError::UnsupportedExpr => DriverError::UnsupportedHardwareValueExpression,
            DriverPlaceError::UnknownObject { module, name } => {
                DriverError::UnknownHardwareObject { module, name }
            }
        };
        CompileError::driver_error(kind, span)
    }

    fn collect_source_instance_summaries(&self) -> Vec<super::DriverCellSummary> {
        let mut summaries = BTreeMap::new();
        for module in self.eir.modules() {
            self.collect_source_instance_summaries_from_items(module.items(), &mut summaries);
        }
        for drive in &self.drives {
            if let Some(summary) =
                summaries.get_mut(&DriverSummaryOriginKey::from_origin(drive.origin()))
            {
                summary.add_drive(drive.target_place().clone());
            }
        }
        for read in &self.reads {
            if let Some(summary) =
                summaries.get_mut(&DriverSummaryOriginKey::from_origin(read.origin()))
            {
                summary.add_read(read.source_place().clone());
            }
        }
        summaries.into_values().collect()
    }

    fn collect_source_instance_summaries_from_items(
        &self,
        items: &[EirItem],
        summaries: &mut BTreeMap<DriverSummaryOriginKey, super::DriverCellSummary>,
    ) {
        for item in items {
            match item {
                EirItem::CellExpansion(expansion) => {
                    self.collect_source_instance_summaries_from_items(expansion.items(), summaries);
                }
                EirItem::Instance(instance) => {
                    self.insert_source_instance_summary(instance, summaries);
                }
                EirItem::SymbolicStaticIf {
                    then_items,
                    else_items,
                    ..
                } => {
                    self.collect_source_instance_summaries_from_items(then_items, summaries);
                    self.collect_source_instance_summaries_from_items(else_items, summaries);
                }
                EirItem::SymbolicStaticFor { items, .. } => {
                    self.collect_source_instance_summaries_from_items(items, summaries);
                }
                _ => {}
            }
        }
    }

    fn insert_source_instance_summary(
        &self,
        instance: &EirInstance,
        summaries: &mut BTreeMap<DriverSummaryOriginKey, super::DriverCellSummary>,
    ) {
        let Some(summary) = self.opaque_summaries.get(instance.module()) else {
            return;
        };
        if summary.kind() != OpaqueItemKind::SourceCell {
            return;
        }
        let key = DriverSummaryOriginKey::from_origin(instance.origin());
        summaries.entry(key).or_insert_with(|| {
            super::DriverCellSummary::new(
                summary.callable(),
                instance.source_name(),
                instance.origin().clone(),
            )
        });
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct DriverSummaryOriginKey {
    source: SourceId,
    start: usize,
    end: usize,
    expansion_stack: Vec<DriverSummaryExpansionKey>,
}

impl DriverSummaryOriginKey {
    fn from_origin(origin: &crate::eir_origin::EirOrigin) -> Self {
        let span = origin.span();
        Self {
            source: span.source,
            start: span.start,
            end: span.end,
            expansion_stack: origin
                .expansion_stack()
                .iter()
                .map(DriverSummaryExpansionKey::from_expansion)
                .collect(),
        }
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct DriverSummaryExpansionKey {
    callable: String,
    instance: String,
    source: SourceId,
    start: usize,
    end: usize,
}

impl DriverSummaryExpansionKey {
    fn from_expansion(expansion: &crate::eir_origin::EirExpansion) -> Self {
        let span = expansion.span();
        Self {
            callable: expansion.callable().to_string(),
            instance: expansion.instance().to_string(),
            source: span.source,
            start: span.start,
            end: span.end,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        LoweringError,
        eir::{
            EirDesign, EirDesignComposer, EirFactCollector, EirItem, EirModule, EirRawDesign,
            EirValidator,
        },
        eir_expr::EirExpr,
        eir_origin::EirOrigin,
        eir_place::EirPlace,
    };
    use std::sync::Arc;
    use syl_sema::OpaqueSummaryTable;
    use syl_span::{SourceId, Span};

    fn validated_design(modules: Vec<EirModule>) -> Result<EirDesign, CompileError> {
        let raw = Arc::new(EirRawDesign::new(modules));
        EirValidator::new(raw.modules()).validate()?;
        let facts = Arc::new(EirFactCollector::collect(
            raw.modules(),
            &OpaqueSummaryTable::new(),
        )?);
        Ok(EirDesignComposer::compose(raw, facts))
    }

    #[test]
    fn rejects_unresolved_drive_root() {
        let span = Span::new_in(SourceId::new(0), 4, 12);
        let origin = EirOrigin::new(span, Vec::new());
        let module = EirModule::new(
            "Top",
            Vec::new(),
            Vec::new(),
            vec![EirItem::Drive {
                lhs: EirPlace::Ident("missing".to_string()),
                rhs: EirExpr::Int(0),
                reads: Vec::new(),
                origin,
            }],
        );
        let design = match validated_design(vec![module]) {
            Ok(design) => design,
            Err(error) => panic!("test EIR should pass structural validation: {error}"),
        };

        let errors = match DriverFactsCollector::new(&design).collect() {
            Ok(_) => panic!("driver facts pass must reject unresolved drive roots"),
            Err(errors) => errors,
        };

        assert!(matches!(
            errors.first(),
            Some(CompileError::Lowering { kind, .. })
                if matches!(
                    kind.as_ref(),
                    LoweringError::Driver(DriverError::UnknownHardwareObject { module, name })
                        if module == "Top" && name == "missing"
                )
        ));
        assert_eq!(errors[0].diagnostic().span, span);
    }

    #[test]
    fn next_drive_facts_keep_effect_summary_separate_from_target_name() {
        let span = Span::new_in(SourceId::new(1), 20, 30);
        let origin = EirOrigin::new(span, Vec::new());
        let module = EirModule::new(
            "Top",
            Vec::new(),
            Vec::new(),
            vec![
                EirItem::Storage {
                    width: "1".into(),
                    name: "state".to_string(),
                    origin: origin.clone(),
                },
                EirItem::ClockedStorage {
                    clock: EirExpr::ident("clk"),
                    target: EirPlace::Ident("state".to_string()),
                    reset: None,
                    next: EirExpr::Int(0),
                    reads: Vec::new(),
                    origin,
                },
            ],
        );
        let design = match validated_design(vec![module]) {
            Ok(design) => design,
            Err(error) => panic!("test EIR should assemble: {error}"),
        };

        let facts = match DriverFactsCollector::new(&design).collect() {
            Ok(facts) => facts,
            Err(errors) => panic!("facts pass should succeed: {errors:?}"),
        };
        let drive = facts
            .drives()
            .first()
            .expect("clocked storage must produce a drive fact");

        assert_eq!(drive.target_place().display(), "state.next");
        assert!(matches!(
            drive.effect(),
            DriveEffect::Next { storage_target }
                if storage_target.display() == "state"
        ));
    }
}
