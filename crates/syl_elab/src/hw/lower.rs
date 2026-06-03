use crate::{
    CompileError,
    eir::{
        EirBinaryOp, EirConnection, EirDirection, EirExpansion, EirExpr, EirInstance, EirItem,
        EirModule, EirOrigin, EirParam, EirPlace, EirPort, EirReset, EirSelectArm, EirSelectMode,
        EirUnaryOp,
    },
};
use syl_hw::{
    HwBinaryOp, HwConnection, HwDirection, HwExpansion, HwExpr, HwInstance, HwItem, HwOrigin,
    HwParam, HwParamBind, HwPort, HwReset, HwSelectArm, HwSelectMode, HwUnaryOp,
    ParametricHwDesign, ParametricHwItem, ParametricHwModule,
};
use syl_span::Span;

#[non_exhaustive]
pub(crate) struct HwLowerer<'a> {
    eir: &'a crate::eir::EirDesign,
}

impl<'a> HwLowerer<'a> {
    pub(crate) fn new(eir: &'a crate::eir::EirDesign) -> Self {
        Self { eir }
    }

    pub(crate) fn lower(&self) -> Result<ParametricHwDesign, CompileError> {
        Ok(ParametricHwDesign::new(self.lower_modules()?))
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
        )
        .with_doc(module.doc().map(ToOwned::to_owned)))
    }

    fn lower_param(&self, param: &EirParam) -> HwParam {
        HwParam::new(param.name(), param.default()).with_doc(param.doc().map(ToOwned::to_owned))
    }

    fn lower_port(&self, port: &EirPort) -> Result<HwPort, CompileError> {
        Ok(HwPort::new(
            self.lower_direction(port.direction())?,
            port.width(),
            port.name(),
        )
        .with_doc(port.doc().map(ToOwned::to_owned)))
    }

    fn lower_direction(&self, direction: EirDirection) -> Result<HwDirection, CompileError> {
        match direction {
            EirDirection::In => Ok(HwDirection::In),
            EirDirection::InOut => Ok(HwDirection::InOut),
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
            EirItem::ClockedAssert {
                clock,
                trigger,
                message,
                origin,
                ..
            } => Ok(vec![ParametricHwItem::core(
                HwItem::ClockedAssert {
                    clock: self.lower_expr(clock, origin.span())?,
                    trigger: self.lower_expr(trigger, origin.span())?,
                    message: self.lower_expr(message, origin.span())?,
                },
                self.lower_origin(origin),
            )]),
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
            EirExpr::HighZ => Ok(HwExpr::HighZ),
            EirExpr::Zero => Ok(HwExpr::Zero),
            EirExpr::Unary { op, expr } => Ok(HwExpr::Unary {
                op: self.lower_unary_op(*op)?,
                expr: Box::new(self.lower_expr(expr, span)?),
            }),
            EirExpr::Binary { op, left, right } => Ok(HwExpr::Binary {
                op: self.lower_binary_op(*op)?,
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
                mode: self.lower_select_mode(*mode)?,
                arms: self.lower_select_arms(arms, span)?,
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
                syl_sema::HwirError::UnsupportedHardwareValueExpression,
                span,
            )),
        }
    }

    fn lower_select_arms(
        &self,
        arms: &[EirSelectArm],
        span: Span,
    ) -> Result<Vec<HwSelectArm>, CompileError> {
        arms.iter()
            .map(|arm| {
                Ok(HwSelectArm::new(
                    self.lower_expr(arm.guard(), span)?,
                    self.lower_expr(arm.value(), span)?,
                ))
            })
            .collect()
    }

    fn lower_select_mode(&self, mode: EirSelectMode) -> Result<HwSelectMode, CompileError> {
        match mode {
            EirSelectMode::Priority => Ok(HwSelectMode::Priority),
            EirSelectMode::Unique => Ok(HwSelectMode::Unique),
        }
    }

    fn lower_binary_op(&self, op: EirBinaryOp) -> Result<HwBinaryOp, CompileError> {
        match op {
            EirBinaryOp::OrOr => Ok(HwBinaryOp::OrOr),
            EirBinaryOp::AndAnd => Ok(HwBinaryOp::AndAnd),
            EirBinaryOp::Eq => Ok(HwBinaryOp::Eq),
            EirBinaryOp::NotEq => Ok(HwBinaryOp::NotEq),
            EirBinaryOp::Lt => Ok(HwBinaryOp::Lt),
            EirBinaryOp::LtEq => Ok(HwBinaryOp::LtEq),
            EirBinaryOp::Gt => Ok(HwBinaryOp::Gt),
            EirBinaryOp::GtEq => Ok(HwBinaryOp::GtEq),
            EirBinaryOp::Add => Ok(HwBinaryOp::Add),
            EirBinaryOp::Sub => Ok(HwBinaryOp::Sub),
            EirBinaryOp::Mul => Ok(HwBinaryOp::Mul),
            EirBinaryOp::Div => Ok(HwBinaryOp::Div),
            EirBinaryOp::Rem => Ok(HwBinaryOp::Rem),
            EirBinaryOp::Shl => Ok(HwBinaryOp::Shl),
            EirBinaryOp::BitAnd => Ok(HwBinaryOp::BitAnd),
            EirBinaryOp::BitOr => Ok(HwBinaryOp::BitOr),
            EirBinaryOp::BitXor => Ok(HwBinaryOp::BitXor),
        }
    }

    fn lower_unary_op(&self, op: EirUnaryOp) -> Result<HwUnaryOp, CompileError> {
        match op {
            EirUnaryOp::Neg => Ok(HwUnaryOp::Neg),
            EirUnaryOp::Not => Ok(HwUnaryOp::Not),
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
        HwExpansion::new(
            expansion.callable(),
            expansion.instance(),
            expansion.span().source,
            expansion.span().start,
            expansion.span().end,
        )
    }
}
