use crate::{
    CompileError,
    driver::{CreateKind, DriverFacts},
    driver_place::DriverPlace,
    eir::{
        EirConnection, EirDirection, EirInstance, EirItem, EirModule, EirParam, EirPort, EirReset,
    },
    eir_expr::{EirBinaryOp, EirExpr, EirSelectMode, EirUnaryOp},
    eir_guard::{EirGuard, EirGuardFrame},
    eir_origin::EirExpansion,
    eir_origin::EirOrigin,
    eir_place::EirPlace,
};
use syl_hw::{
    HwBinaryOp, HwCellSummary, HwConnection, HwCreateFact, HwCreateKind, HwDirection, HwDriveFact,
    HwExpansion, HwExpr, HwGuard, HwGuardFrame, HwInstance, HwItem, HwOrigin, HwParam, HwParamBind,
    HwPlace, HwPlaceExpr, HwPort, HwReadFact, HwReset, HwSelectArm, HwSelectMode, HwUnaryOp,
    ParametricHwDesign, ParametricHwItem, ParametricHwModule,
};
use syl_span::Span;

#[non_exhaustive]
pub(super) struct HwLowerer<'a> {
    eir: &'a crate::eir::EirDesign,
    facts: &'a DriverFacts,
}

impl<'a> HwLowerer<'a> {
    pub(super) fn new(eir: &'a crate::eir::EirDesign, facts: &'a DriverFacts) -> Self {
        Self { eir, facts }
    }

    pub(super) fn lower(&self) -> Result<ParametricHwDesign, CompileError> {
        Ok(ParametricHwDesign::new(
            self.lower_modules()?,
            self.lower_driver_facts()?,
            self.lower_read_facts()?,
            self.lower_create_facts()?,
            self.lower_cell_summaries()?,
        ))
    }

    fn lower_modules(&self) -> Result<Vec<ParametricHwModule>, CompileError> {
        self.eir
            .modules()
            .iter()
            .map(|module| self.lower_module(module))
            .collect()
    }

    fn lower_module(&self, module: &EirModule) -> Result<ParametricHwModule, CompileError> {
        Ok(ParametricHwModule::new(
            module.name(),
            module
                .params()
                .iter()
                .map(|param| self.lower_param(param))
                .collect(),
            module
                .ports()
                .iter()
                .map(|port| self.lower_port(port))
                .collect::<Result<Vec<_>, _>>()?,
            self.lower_items(module.items())?,
        ))
    }

    fn lower_param(&self, param: &EirParam) -> HwParam {
        HwParam::new(param.name(), param.default())
    }

    fn lower_port(&self, port: &EirPort) -> Result<HwPort, CompileError> {
        Ok(HwPort::new(
            self.lower_direction(port.direction())?,
            port.width(),
            port.name(),
        ))
    }

    fn lower_direction(&self, direction: EirDirection) -> Result<HwDirection, CompileError> {
        match direction {
            EirDirection::In => Ok(HwDirection::In),
            EirDirection::Out => Ok(HwDirection::Out),
        }
    }

    fn lower_items(&self, items: &[EirItem]) -> Result<Vec<ParametricHwItem>, CompileError> {
        let mut lowered = Vec::new();
        for item in items {
            lowered.extend(self.lower_item(item)?);
        }
        Ok(lowered)
    }

    fn lower_item(&self, item: &EirItem) -> Result<Vec<ParametricHwItem>, CompileError> {
        match item {
            EirItem::StaticParam {
                name,
                value,
                origin,
            } => Ok(vec![ParametricHwItem::core(
                HwItem::StaticParam {
                    name: name.clone(),
                    value: self.lower_expr(value, origin.span())?,
                },
                self.lower_origin(origin),
            )]),
            EirItem::Signal {
                width,
                name,
                origin,
                ..
            } => Ok(vec![ParametricHwItem::core(
                HwItem::SignalDecl {
                    width: width.source().to_string(),
                    name: name.clone(),
                },
                self.lower_origin(origin),
            )]),
            EirItem::Storage {
                width,
                name,
                origin,
            } => Ok(vec![ParametricHwItem::core(
                HwItem::StorageDecl {
                    width: width.source().to_string(),
                    name: name.clone(),
                },
                self.lower_origin(origin),
            )]),
            EirItem::Drive {
                lhs, rhs, origin, ..
            } => Ok(vec![ParametricHwItem::core(
                HwItem::ContinuousDrive {
                    lhs: self.lower_place_expr(lhs, origin.span())?,
                    rhs: self.lower_expr(rhs, origin.span())?,
                },
                self.lower_origin(origin),
            )]),
            EirItem::ClockedStorage {
                clock,
                target,
                reset,
                next,
                origin,
                ..
            } => Ok(vec![ParametricHwItem::core(
                HwItem::ClockedStorage {
                    clock: self.lower_expr(clock, origin.span())?,
                    target: self.lower_place_expr(target, origin.span())?,
                    reset: reset
                        .as_deref()
                        .map(|reset| self.lower_reset(reset, origin.span()))
                        .transpose()?,
                    next: self.lower_expr(next, origin.span())?,
                },
                self.lower_origin(origin),
            )]),
            EirItem::CellExpansion(expansion) => self.lower_items(expansion.items()),
            EirItem::CellBoundary(_) => Ok(Vec::new()),
            EirItem::Instance(instance) => Ok(vec![ParametricHwItem::core(
                HwItem::Instance(self.lower_instance(instance, instance.origin())?),
                self.lower_origin(instance.origin()),
            )]),
            EirItem::SymbolicStaticIf {
                cond,
                label,
                then_items,
                else_items,
                origin,
            } => Ok(vec![ParametricHwItem::StaticIf {
                cond: self.lower_expr(cond, origin.span())?,
                label: label.clone(),
                then_items: self.lower_items(then_items)?,
                else_items: self.lower_items(else_items)?,
                origin: self.lower_origin(origin),
            }]),
            EirItem::SymbolicStaticFor {
                index,
                start,
                end,
                label,
                items,
                origin,
            } => Ok(vec![ParametricHwItem::StaticFor {
                index: index.clone(),
                start: self.lower_expr(start, origin.span())?,
                end: self.lower_expr(end, origin.span())?,
                label: label.clone(),
                items: self.lower_items(items)?,
                origin: self.lower_origin(origin),
            }]),
            EirItem::InitialError { message, origin } => Ok(vec![ParametricHwItem::core(
                HwItem::InitialError {
                    message: self.lower_expr(message, origin.span())?,
                },
                self.lower_origin(origin),
            )]),
        }
    }

    fn lower_instance(
        &self,
        instance: &EirInstance,
        origin: &EirOrigin,
    ) -> Result<HwInstance, CompileError> {
        Ok(HwInstance::new(
            instance.module(),
            instance
                .params()
                .iter()
                .map(|param| self.lower_param_bind(param))
                .collect(),
            instance.name(),
            instance
                .connections()
                .iter()
                .map(|connection| self.lower_connection(connection, origin.span()))
                .collect::<Result<Vec<_>, _>>()?,
        ))
    }

    fn lower_param_bind(&self, bind: &crate::eir::EirParamBind) -> HwParamBind {
        HwParamBind::new(bind.name(), bind.value())
    }

    fn lower_connection(
        &self,
        connection: &EirConnection,
        span: Span,
    ) -> Result<HwConnection, CompileError> {
        Ok(HwConnection::new(
            connection.formal(),
            self.lower_expr(connection.actual(), span)?,
        ))
    }

    fn lower_reset(&self, reset: &EirReset, span: Span) -> Result<HwReset, CompileError> {
        Ok(HwReset::new(
            self.lower_expr(reset.condition(), span)?,
            self.lower_expr(reset.value(), span)?,
        ))
    }

    fn lower_place_expr(&self, place: &EirPlace, span: Span) -> Result<HwExpr, CompileError> {
        match place {
            EirPlace::Ident(name) => Ok(HwExpr::Ident(name.clone())),
            EirPlace::Slice { base, high, low } => Ok(HwExpr::Slice {
                value: Box::new(self.lower_place_expr(base, span)?),
                high: high.source().to_string(),
                low: low.source().to_string(),
            }),
            EirPlace::IndexedPartSelect { base, index, width } => Ok(HwExpr::IndexedPartSelect {
                value: Box::new(self.lower_place_expr(base, span)?),
                index: Box::new(self.lower_expr(index, span)?),
                width: width.source().to_string(),
            }),
            EirPlace::Index { base, index } => Ok(HwExpr::Index {
                value: Box::new(self.lower_place_expr(base, span)?),
                index: Box::new(self.lower_expr(index, span)?),
            }),
        }
    }

    fn lower_expr(&self, expr: &EirExpr, span: Span) -> Result<HwExpr, CompileError> {
        match expr {
            EirExpr::Ident(name) => Ok(HwExpr::Ident(name.clone())),
            EirExpr::Int(value) => Ok(HwExpr::Int(*value)),
            EirExpr::Bool(value) => Ok(HwExpr::Bool(*value)),
            EirExpr::Str(value) => Ok(HwExpr::Str(value.clone())),
            EirExpr::Zero => Ok(HwExpr::Zero),
            EirExpr::Unary { op, expr } => Ok(HwExpr::Unary {
                op: self.lower_unary_op(*op),
                expr: Box::new(self.lower_expr(expr, span)?),
            }),
            EirExpr::Binary { op, left, right } => Ok(HwExpr::Binary {
                op: self.lower_binary_op(*op),
                left: Box::new(self.lower_expr(left, span)?),
                right: Box::new(self.lower_expr(right, span)?),
            }),
            EirExpr::Mux {
                cond,
                then_value,
                else_value,
            } => Ok(HwExpr::Mux {
                cond: Box::new(self.lower_expr(cond, span)?),
                then_value: Box::new(self.lower_expr(then_value, span)?),
                else_value: Box::new(self.lower_expr(else_value, span)?),
            }),
            EirExpr::Select {
                mode,
                arms,
                default,
            } => Ok(HwExpr::Select {
                mode: self.lower_select_mode(*mode),
                arms: arms
                    .iter()
                    .map(|arm| self.lower_select_arm(arm, span))
                    .collect::<Result<Vec<_>, _>>()?,
                default: Box::new(self.lower_expr(default, span)?),
            }),
            EirExpr::Concat(parts) => Ok(HwExpr::Concat(
                parts
                    .iter()
                    .map(|part| self.lower_expr(part, span))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            EirExpr::Slice { value, high, low } => Ok(HwExpr::Slice {
                value: Box::new(self.lower_expr(value, span)?),
                high: high.source().to_string(),
                low: low.source().to_string(),
            }),
            EirExpr::IndexedPartSelect {
                value,
                index,
                width,
            } => Ok(HwExpr::IndexedPartSelect {
                value: Box::new(self.lower_expr(value, span)?),
                index: Box::new(self.lower_expr(index, span)?),
                width: width.source().to_string(),
            }),
            EirExpr::Index { value, index } => Ok(HwExpr::Index {
                value: Box::new(self.lower_expr(value, span)?),
                index: Box::new(self.lower_expr(index, span)?),
            }),
            EirExpr::Call { name, args } => Ok(HwExpr::Call {
                name: name.clone(),
                args: args
                    .iter()
                    .map(|arg| self.lower_expr(arg, span))
                    .collect::<Result<Vec<_>, _>>()?,
            }),
            EirExpr::Unsupported { .. } => Err(CompileError::lowering_at(
                syl_sema::EirError::UnsupportedHardwareValueExpression,
                span,
            )),
        }
    }

    fn lower_select_arm(
        &self,
        arm: &crate::eir_expr::EirSelectArm,
        span: Span,
    ) -> Result<HwSelectArm, CompileError> {
        Ok(HwSelectArm::new(
            self.lower_expr(arm.guard(), span)?,
            self.lower_expr(arm.value(), span)?,
        ))
    }

    fn lower_select_mode(&self, mode: EirSelectMode) -> HwSelectMode {
        match mode {
            EirSelectMode::Priority => HwSelectMode::Priority,
            EirSelectMode::Unique => HwSelectMode::Unique,
        }
    }

    fn lower_unary_op(&self, op: EirUnaryOp) -> HwUnaryOp {
        match op {
            EirUnaryOp::Neg => HwUnaryOp::Neg,
            EirUnaryOp::Not => HwUnaryOp::Not,
        }
    }

    fn lower_binary_op(&self, op: EirBinaryOp) -> HwBinaryOp {
        match op {
            EirBinaryOp::OrOr => HwBinaryOp::OrOr,
            EirBinaryOp::AndAnd => HwBinaryOp::AndAnd,
            EirBinaryOp::Eq => HwBinaryOp::Eq,
            EirBinaryOp::NotEq => HwBinaryOp::NotEq,
            EirBinaryOp::Lt => HwBinaryOp::Lt,
            EirBinaryOp::LtEq => HwBinaryOp::LtEq,
            EirBinaryOp::Gt => HwBinaryOp::Gt,
            EirBinaryOp::GtEq => HwBinaryOp::GtEq,
            EirBinaryOp::Add => HwBinaryOp::Add,
            EirBinaryOp::Sub => HwBinaryOp::Sub,
            EirBinaryOp::Mul => HwBinaryOp::Mul,
            EirBinaryOp::Div => HwBinaryOp::Div,
            EirBinaryOp::Rem => HwBinaryOp::Rem,
            EirBinaryOp::Shl => HwBinaryOp::Shl,
            EirBinaryOp::BitAnd => HwBinaryOp::BitAnd,
            EirBinaryOp::BitOr => HwBinaryOp::BitOr,
            EirBinaryOp::BitXor => HwBinaryOp::BitXor,
        }
    }

    fn lower_driver_facts(&self) -> Result<Vec<HwDriveFact>, CompileError> {
        self.facts
            .drives()
            .iter()
            .map(|fact| {
                Ok(HwDriveFact::new(
                    fact.module(),
                    self.lower_driver_place(fact.target_place())?,
                    self.lower_guard(fact.guard()),
                    self.lower_origin(fact.origin()),
                ))
            })
            .collect()
    }

    fn lower_read_facts(&self) -> Result<Vec<HwReadFact>, CompileError> {
        self.facts
            .reads()
            .iter()
            .map(|fact| {
                Ok(HwReadFact::new(
                    fact.module(),
                    self.lower_driver_place(fact.source_place())?,
                    self.lower_guard(fact.guard()),
                    self.lower_origin(fact.origin()),
                ))
            })
            .collect()
    }

    fn lower_create_facts(&self) -> Result<Vec<HwCreateFact>, CompileError> {
        self.facts
            .creates()
            .iter()
            .map(|fact| {
                Ok(HwCreateFact::new(
                    fact.module(),
                    fact.name(),
                    fact.object_id(),
                    self.lower_create_kind(fact.kind()),
                    self.lower_origin(fact.origin()),
                ))
            })
            .collect()
    }

    fn lower_cell_summaries(&self) -> Result<Vec<HwCellSummary>, CompileError> {
        self.facts
            .summary_cells()
            .iter()
            .map(|summary| {
                Ok(HwCellSummary::builder(
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

    fn lower_create_kind(&self, kind: CreateKind) -> HwCreateKind {
        match kind {
            CreateKind::Signal => HwCreateKind::Signal,
            CreateKind::Storage => HwCreateKind::Storage,
        }
    }

    fn lower_driver_place(
        &self,
        place: &crate::driver_place::DriverPlace,
    ) -> Result<HwPlace, CompileError> {
        match place {
            crate::driver_place::DriverPlace::Ident(name) => Ok(HwPlace::Ident(name.clone())),
            crate::driver_place::DriverPlace::Object(object) => Ok(HwPlace::Object {
                id: object.id(),
                name: object.name().to_string(),
            }),
            crate::driver_place::DriverPlace::Slice { base, range } => Ok(HwPlace::Slice {
                base: Box::new(self.lower_driver_place(base)?),
                high: range.high().source().to_string(),
                low: range.low().source().to_string(),
            }),
            crate::driver_place::DriverPlace::IndexedPartSelect { base, index, width } => {
                Ok(HwPlace::IndexedPartSelect {
                    base: Box::new(self.lower_driver_place(base)?),
                    index: self.lower_driver_place_expr(index),
                    width: width.source().to_string(),
                })
            }
            crate::driver_place::DriverPlace::Index { base, index } => Ok(HwPlace::Index {
                base: Box::new(self.lower_driver_place(base)?),
                index: self.lower_driver_place_expr(index),
            }),
            crate::driver_place::DriverPlace::Expr(expr) => {
                Ok(HwPlace::Expr(self.lower_driver_place_expr(expr)))
            }
        }
    }

    fn lower_driver_place_expr(&self, expr: &crate::driver_place::DriverExpr) -> HwPlaceExpr {
        match expr {
            crate::driver_place::DriverExpr::Ident(name) => HwPlaceExpr::Ident(name.clone()),
            crate::driver_place::DriverExpr::Int(value) => HwPlaceExpr::Int(*value),
            crate::driver_place::DriverExpr::Bool(value) => HwPlaceExpr::Bool(*value),
            crate::driver_place::DriverExpr::Str(value) => HwPlaceExpr::Str(value.clone()),
            crate::driver_place::DriverExpr::Zero => HwPlaceExpr::Zero,
            crate::driver_place::DriverExpr::Unary { op, expr } => HwPlaceExpr::Op {
                name: self.driver_unary_name(*op).to_string(),
                args: vec![self.lower_driver_place_expr(expr)],
            },
            crate::driver_place::DriverExpr::Binary { op, left, right } => HwPlaceExpr::Op {
                name: self.driver_binary_name(*op).to_string(),
                args: vec![
                    self.lower_driver_place_expr(left),
                    self.lower_driver_place_expr(right),
                ],
            },
            crate::driver_place::DriverExpr::Mux {
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
            crate::driver_place::DriverExpr::Concat(parts) => HwPlaceExpr::Op {
                name: "concat".to_string(),
                args: parts
                    .iter()
                    .map(|part| self.lower_driver_place_expr(part))
                    .collect(),
            },
            crate::driver_place::DriverExpr::Slice { value, range } => HwPlaceExpr::Op {
                name: "slice".to_string(),
                args: vec![
                    self.lower_driver_place_expr(value),
                    HwPlaceExpr::Str(range.high().source().to_string()),
                    HwPlaceExpr::Str(range.low().source().to_string()),
                ],
            },
            crate::driver_place::DriverExpr::IndexedPartSelect {
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
            crate::driver_place::DriverExpr::Index { value, index } => HwPlaceExpr::Op {
                name: "idx".to_string(),
                args: vec![
                    self.lower_driver_place_expr(value),
                    self.lower_driver_place_expr(index),
                ],
            },
            crate::driver_place::DriverExpr::Call { name, args } => HwPlaceExpr::Op {
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
