use super::{
    CellSummaryCollector, CreateFact, CreateFactInput, CreateKind, DriveEffect, DriveFact,
    DriverFacts, ReadFact,
};
use crate::{
    CompileError, DriverError,
    driver_place::{
        DriverExpr, DriverObjectTable, DriverPlace, DriverPlaceError, DriverPlaceResolver,
    },
    eir::{EirDesign, EirDriveKind, EirObjectKind},
};

#[non_exhaustive]
pub(crate) struct DriverFactsCollector<'a> {
    eir: &'a EirDesign,
    objects: DriverObjectTable,
    drives: Vec<DriveFact>,
    reads: Vec<ReadFact>,
    creates: Vec<CreateFact>,
    errors: Vec<CompileError>,
}

impl<'a> DriverFactsCollector<'a> {
    pub(crate) fn new(eir: &'a EirDesign) -> Self {
        Self {
            eir,
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
        let cell_summaries = CellSummaryCollector::new(&self.drives, &self.reads, &self.creates)
            .collect()
            .finish();
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
                    (next_target, DriveEffect::Next { storage_target })
                }
            };
            self.drives.push(DriveFact::new(
                drive.module(),
                target,
                effect,
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        LoweringError,
        eir::{EirDesignAssembler, EirItem, EirModule},
        eir_expr::EirExpr,
        eir_origin::EirOrigin,
        eir_place::EirPlace,
    };
    use syl_span::{SourceId, Span};

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
        let design = match EirDesignAssembler::assemble(vec![module]) {
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
        let design = match EirDesignAssembler::assemble(vec![module]) {
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
