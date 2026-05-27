use crate::{
    CompileError,
    driver::{CreateKind, DriverFacts},
    driver_place::{DriverExpr, DriverPlace},
    eir::{EirBinaryOp, EirExpansion, EirGuard, EirGuardFrame, EirOrigin, EirUnaryOp},
    hardware_metadata::{
        HardwareCellSummary, HardwareCreateFact, HardwareCreateKind, HardwareDriveFact,
        HardwareMetadata, HardwareReadFact,
    },
};
use syl_hw::{HwExpansion, HwGuard, HwGuardFrame, HwOrigin, HwPlace, HwPlaceExpr};
use syl_sema::OpaqueSummaryTable;

#[non_exhaustive]
pub(crate) struct HardwareMetadataLowerer<'a> {
    facts: &'a DriverFacts,
}

impl<'a> HardwareMetadataLowerer<'a> {
    pub(crate) fn new(facts: &'a DriverFacts) -> Self {
        Self { facts }
    }

    pub(crate) fn lower(
        &self,
        opaque_summaries: &OpaqueSummaryTable,
    ) -> Result<HardwareMetadata, CompileError> {
        Ok(HardwareMetadata::new(
            self.lower_driver_facts()?,
            self.lower_read_facts()?,
            self.lower_create_facts()?,
            self.lower_cell_summaries()?,
            opaque_summaries.clone(),
        ))
    }

    fn lower_driver_facts(&self) -> Result<Vec<HardwareDriveFact>, CompileError> {
        self.facts
            .drives()
            .iter()
            .map(|fact| {
                Ok(HardwareDriveFact::new(
                    fact.module(),
                    self.lower_driver_place(fact.target_place())?,
                    self.lower_guard(fact.guard()),
                    self.lower_origin(fact.origin()),
                ))
            })
            .collect()
    }

    fn lower_read_facts(&self) -> Result<Vec<HardwareReadFact>, CompileError> {
        self.facts
            .reads()
            .iter()
            .map(|fact| {
                Ok(HardwareReadFact::new(
                    fact.module(),
                    self.lower_driver_place(fact.source_place())?,
                    self.lower_guard(fact.guard()),
                    self.lower_origin(fact.origin()),
                ))
            })
            .collect()
    }

    fn lower_create_facts(&self) -> Result<Vec<HardwareCreateFact>, CompileError> {
        self.facts
            .creates()
            .iter()
            .map(|fact| {
                Ok(HardwareCreateFact::new(
                    fact.module(),
                    fact.name(),
                    fact.object_id(),
                    self.lower_create_kind(fact.kind()),
                    self.lower_origin(fact.origin()),
                ))
            })
            .collect()
    }

    fn lower_cell_summaries(&self) -> Result<Vec<HardwareCellSummary>, CompileError> {
        self.facts
            .summary_cells()
            .iter()
            .map(|summary| {
                Ok(HardwareCellSummary::builder(
                    summary.callable(),
                    summary.instance(),
                    self.lower_origin(summary.origin()),
                )
                .drives(self.lower_driver_places(summary.drives())?)
                .reads(self.lower_driver_places(summary.reads())?)
                .creates(summary.creates().to_vec())
                .build())
            })
            .collect()
    }

    fn lower_driver_places(&self, places: &[DriverPlace]) -> Result<Vec<HwPlace>, CompileError> {
        places
            .iter()
            .map(|place| self.lower_driver_place(place))
            .collect()
    }

    fn lower_create_kind(&self, kind: CreateKind) -> HardwareCreateKind {
        match kind {
            CreateKind::Signal => HardwareCreateKind::Signal,
            CreateKind::Storage => HardwareCreateKind::Storage,
        }
    }

    fn lower_driver_place(&self, place: &DriverPlace) -> Result<HwPlace, CompileError> {
        match place {
            DriverPlace::Ident(name) => Ok(HwPlace::Ident(name.clone())),
            DriverPlace::Object(object) => Ok(HwPlace::Object {
                id: object.id(),
                name: object.name().to_string(),
            }),
            DriverPlace::Slice { base, range } => Ok(HwPlace::Slice {
                base: Box::new(self.lower_driver_place(base)?),
                high: range.high().source().to_string(),
                low: range.low().source().to_string(),
            }),
            DriverPlace::IndexedPartSelect { base, index, width } => {
                Ok(HwPlace::IndexedPartSelect {
                    base: Box::new(self.lower_driver_place(base)?),
                    index: self.lower_driver_place_expr(index),
                    width: width.source().to_string(),
                })
            }
            DriverPlace::Index { base, index } => Ok(HwPlace::Index {
                base: Box::new(self.lower_driver_place(base)?),
                index: self.lower_driver_place_expr(index),
            }),
            DriverPlace::Expr(expr) => Ok(HwPlace::Expr(self.lower_driver_place_expr(expr))),
        }
    }

    fn lower_driver_place_expr(&self, expr: &DriverExpr) -> HwPlaceExpr {
        match expr {
            DriverExpr::Ident(name) => HwPlaceExpr::Ident(name.clone()),
            DriverExpr::Int(value) => HwPlaceExpr::Int(*value),
            DriverExpr::Bool(value) => HwPlaceExpr::Bool(*value),
            DriverExpr::Str(value) => HwPlaceExpr::Str(value.clone()),
            DriverExpr::Zero => HwPlaceExpr::Zero,
            DriverExpr::Unary { op, expr } => HwPlaceExpr::Op {
                name: self.driver_unary_name(*op).to_string(),
                args: vec![self.lower_driver_place_expr(expr)],
            },
            DriverExpr::Binary { op, left, right } => HwPlaceExpr::Op {
                name: self.driver_binary_name(*op).to_string(),
                args: vec![
                    self.lower_driver_place_expr(left),
                    self.lower_driver_place_expr(right),
                ],
            },
            DriverExpr::Mux {
                cond,
                then_value,
                else_value,
            } => HwPlaceExpr::Op {
                name: "mux".to_string(),
                args: vec![
                    self.lower_driver_place_expr(cond),
                    self.lower_driver_place_expr(then_value),
                    self.lower_driver_place_expr(else_value),
                ],
            },
            DriverExpr::Concat(parts) => HwPlaceExpr::Op {
                name: "concat".to_string(),
                args: parts
                    .iter()
                    .map(|part| self.lower_driver_place_expr(part))
                    .collect(),
            },
            DriverExpr::Slice { value, range } => HwPlaceExpr::Op {
                name: "slice".to_string(),
                args: vec![
                    self.lower_driver_place_expr(value),
                    HwPlaceExpr::Str(range.high().source().to_string()),
                    HwPlaceExpr::Str(range.low().source().to_string()),
                ],
            },
            DriverExpr::IndexedPartSelect {
                value,
                index,
                width,
            } => HwPlaceExpr::Op {
                name: "part".to_string(),
                args: vec![
                    self.lower_driver_place_expr(value),
                    self.lower_driver_place_expr(index),
                    HwPlaceExpr::Str(width.source().to_string()),
                ],
            },
            DriverExpr::Index { value, index } => HwPlaceExpr::Op {
                name: "idx".to_string(),
                args: vec![
                    self.lower_driver_place_expr(value),
                    self.lower_driver_place_expr(index),
                ],
            },
            DriverExpr::Call { name, args } => HwPlaceExpr::Op {
                name: name.clone(),
                args: args
                    .iter()
                    .map(|arg| self.lower_driver_place_expr(arg))
                    .collect(),
            },
        }
    }

    fn driver_unary_name(&self, op: EirUnaryOp) -> &'static str {
        match op {
            EirUnaryOp::Neg => "neg",
            EirUnaryOp::Not => "not",
        }
    }

    fn driver_binary_name(&self, op: EirBinaryOp) -> &'static str {
        match op {
            EirBinaryOp::OrOr => "logic_or",
            EirBinaryOp::AndAnd => "logic_and",
            EirBinaryOp::Eq => "eq",
            EirBinaryOp::NotEq => "not_eq",
            EirBinaryOp::Lt => "lt",
            EirBinaryOp::LtEq => "lt_eq",
            EirBinaryOp::Gt => "gt",
            EirBinaryOp::GtEq => "gt_eq",
            EirBinaryOp::Add => "add",
            EirBinaryOp::Sub => "sub",
            EirBinaryOp::Mul => "mul",
            EirBinaryOp::Div => "div",
            EirBinaryOp::Rem => "rem",
            EirBinaryOp::Shl => "shl",
            EirBinaryOp::BitAnd => "bit_and",
            EirBinaryOp::BitOr => "bit_or",
            EirBinaryOp::BitXor => "bit_xor",
        }
    }

    fn lower_guard(&self, guard: &EirGuard) -> HwGuard {
        HwGuard::new(
            guard
                .frames()
                .iter()
                .map(|frame| self.lower_guard_frame(frame))
                .collect(),
        )
    }

    fn lower_guard_frame(&self, frame: &EirGuardFrame) -> HwGuardFrame {
        match frame {
            EirGuardFrame::IfThen { label } => HwGuardFrame::IfThen {
                label: label.display().to_string(),
            },
            EirGuardFrame::IfElse { label } => HwGuardFrame::IfElse {
                label: label.display().to_string(),
            },
            EirGuardFrame::Loop { label, .. } => HwGuardFrame::Loop {
                label: label.display().to_string(),
            },
        }
    }

    fn lower_origin(&self, origin: &EirOrigin) -> HwOrigin {
        HwOrigin::new(
            origin.span().source,
            origin.span().start,
            origin.span().end,
            origin
                .expansion_stack()
                .iter()
                .map(|expansion| self.lower_expansion(expansion))
                .collect(),
        )
    }

    fn lower_expansion(&self, expansion: &EirExpansion) -> HwExpansion {
        let span = expansion.span();
        HwExpansion::new(
            expansion.callable(),
            expansion.instance(),
            span.source,
            span.start,
            span.end,
        )
    }
}
