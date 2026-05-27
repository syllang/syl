use super::{CompileError, sv_ir::*};
use syl_hw::{
    HwBinaryOp, HwConnection, HwDirection, HwExpr, HwInstance, HwItem, HwParam, HwParamBind,
    HwPort, HwReset, HwSelectArm, HwSelectMode, HwUnaryOp, ParametricHwDesign, ParametricHwItem,
    ParametricHwModule,
};

#[non_exhaustive]
pub(super) struct SvEmitter<'a> {
    hwir: &'a ParametricHwDesign,
}

impl<'a> SvEmitter<'a> {
    pub(super) fn new(hwir: &'a ParametricHwDesign) -> Self {
        Self { hwir }
    }

    pub(super) fn lower(&self) -> Result<SvDesign, CompileError> {
        let mut modules = Vec::new();
        for module in self.hwir.modules() {
            modules.push(self.lower_module(module)?);
        }
        Ok(SvDesign::new(modules))
    }

    fn lower_module(&self, module: &ParametricHwModule) -> Result<SvModule, CompileError> {
        let params = module
            .params()
            .iter()
            .map(|param| self.lower_param(param))
            .collect();
        let ports = module
            .ports()
            .iter()
            .map(|port| self.lower_port(port))
            .collect::<Result<Vec<_>, _>>()?;
        let items = module
            .items()
            .iter()
            .map(|item| self.lower_item(item))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(SvModule::new(module.name(), params, ports, items)
            .with_doc(module.doc().map(ToOwned::to_owned)))
    }

    fn lower_param(&self, param: &HwParam) -> SvParam {
        SvParam::new(param.name(), param.default()).with_doc(param.doc().map(ToOwned::to_owned))
    }

    fn lower_port(&self, port: &HwPort) -> Result<SvPort, CompileError> {
        Ok(SvPort::new(
            self.lower_direction(port.direction())?,
            port.width(),
            port.name(),
        )
        .with_doc(port.doc().map(ToOwned::to_owned)))
    }

    fn lower_direction(&self, direction: HwDirection) -> Result<SvDirection, CompileError> {
        match direction {
            HwDirection::In => Ok(SvDirection::Input),
            HwDirection::InOut => Ok(SvDirection::InOut),
            HwDirection::Out => Ok(SvDirection::Output),
            _ => Err(CompileError::unsupported_hwir("unknown port direction")),
        }
    }

    fn lower_item(&self, item: &ParametricHwItem) -> Result<SvItem, CompileError> {
        match item {
            ParametricHwItem::Core { item, .. } => self.lower_core_item(item),
            ParametricHwItem::StaticIf {
                cond,
                label,
                then_items,
                else_items,
                ..
            } => Ok(SvItem::GenerateIf {
                cond: self.lower_expr(cond)?,
                label: label.clone(),
                then_items: self.lower_items(then_items)?,
                else_items: self.lower_items(else_items)?,
            }),
            ParametricHwItem::StaticFor {
                index,
                start,
                end,
                label,
                items,
                ..
            } => Ok(SvItem::GenerateFor {
                genvar: index.clone(),
                start: self.lower_expr(start)?,
                end: self.lower_expr(end)?,
                label: label.clone(),
                items: self.lower_items(items)?,
            }),
            _ => Err(CompileError::unsupported_hwir(
                "unknown parametric HWIR item",
            )),
        }
    }

    fn lower_core_item(&self, item: &HwItem) -> Result<SvItem, CompileError> {
        match item {
            HwItem::StaticParam { name, value } => Ok(SvItem::LocalParam {
                name: name.clone(),
                value: self.lower_expr(value)?,
            }),
            HwItem::SignalDecl { width, name } => Ok(SvItem::Wire {
                width: width.clone(),
                name: name.clone(),
            }),
            HwItem::StorageDecl { width, name } => Ok(SvItem::Reg {
                width: width.clone(),
                name: name.clone(),
            }),
            HwItem::ContinuousDrive { lhs, rhs } => Ok(SvItem::Assign {
                lhs: self.lower_expr(lhs)?,
                rhs: self.lower_expr(rhs)?,
            }),
            HwItem::ClockedStorage {
                clock,
                target,
                reset,
                next,
            } => Ok(SvItem::AlwaysReg {
                clock: self.lower_expr(clock)?,
                target: self.lower_expr(target)?,
                reset: reset
                    .as_ref()
                    .map(|reset| self.lower_reset(reset))
                    .transpose()?,
                next: self.lower_expr(next)?,
            }),
            HwItem::Instance(instance) => Ok(SvItem::Instance(self.lower_instance(instance)?)),
            HwItem::InitialError { message } => Ok(SvItem::InitialError {
                message: self.lower_expr(message)?,
            }),
            _ => Err(CompileError::unsupported_hwir("unknown HWIR item")),
        }
    }

    fn lower_items(&self, items: &[ParametricHwItem]) -> Result<Vec<SvItem>, CompileError> {
        items.iter().map(|item| self.lower_item(item)).collect()
    }

    fn lower_reset(&self, reset: &HwReset) -> Result<SvReset, CompileError> {
        Ok(SvReset::new(
            self.lower_expr(reset.condition())?,
            self.lower_expr(reset.value())?,
        ))
    }

    fn lower_instance(&self, instance: &HwInstance) -> Result<SvInstance, CompileError> {
        Ok(SvInstance::new(
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
                .map(|conn| self.lower_connection(conn))
                .collect::<Result<Vec<_>, _>>()?,
        ))
    }

    fn lower_param_bind(&self, param: &HwParamBind) -> SvParamBind {
        SvParamBind::new(param.name(), param.value())
    }

    fn lower_connection(&self, conn: &HwConnection) -> Result<SvConnection, CompileError> {
        Ok(SvConnection::new(
            conn.formal(),
            self.lower_expr(conn.actual())?,
        ))
    }

    fn lower_select_arm(&self, arm: &HwSelectArm) -> Result<SvSelectArm, CompileError> {
        Ok(SvSelectArm::new(
            self.lower_expr(arm.guard())?,
            self.lower_expr(arm.value())?,
        ))
    }

    fn lower_select_mode(&self, mode: HwSelectMode) -> SvSelectMode {
        if mode.is_unique() {
            SvSelectMode::Unique
        } else {
            SvSelectMode::Priority
        }
    }

    fn lower_expr(&self, expr: &HwExpr) -> Result<SvExpr, CompileError> {
        match expr {
            HwExpr::Ident(name) => Ok(SvExpr::Ident(name.clone())),
            HwExpr::Int(value) => Ok(SvExpr::Int(*value)),
            HwExpr::Bool(value) => Ok(SvExpr::Bool(*value)),
            HwExpr::Str(value) => Ok(SvExpr::Str(value.clone())),
            HwExpr::HighZ => Ok(SvExpr::HighZ),
            HwExpr::Zero => Ok(SvExpr::Zero),
            HwExpr::Unary { op, expr } => Ok(SvExpr::Unary {
                op: self.lower_unary_op(*op)?,
                expr: Box::new(self.lower_expr(expr)?),
            }),
            HwExpr::Binary { op, left, right } => Ok(SvExpr::Binary {
                op: self.lower_binary_op(*op)?,
                left: Box::new(self.lower_expr(left)?),
                right: Box::new(self.lower_expr(right)?),
            }),
            HwExpr::Mux {
                cond,
                then_value,
                else_value,
            } => Ok(SvExpr::Mux {
                cond: Box::new(self.lower_expr(cond)?),
                then_value: Box::new(self.lower_expr(then_value)?),
                else_value: Box::new(self.lower_expr(else_value)?),
            }),
            HwExpr::Select {
                mode,
                arms,
                default,
            } => Ok(SvExpr::Select {
                mode: self.lower_select_mode(*mode),
                arms: arms
                    .iter()
                    .map(|arm| self.lower_select_arm(arm))
                    .collect::<Result<Vec<_>, _>>()?,
                default: Box::new(self.lower_expr(default)?),
            }),
            HwExpr::Concat(parts) => Ok(SvExpr::Concat(
                parts
                    .iter()
                    .map(|part| self.lower_expr(part))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            HwExpr::Slice { value, high, low } => Ok(SvExpr::Slice {
                value: Box::new(self.lower_expr(value)?),
                high: high.clone(),
                low: low.clone(),
            }),
            HwExpr::IndexedPartSelect {
                value,
                index,
                width,
            } => Ok(SvExpr::IndexedPartSelect {
                value: Box::new(self.lower_expr(value)?),
                index: Box::new(self.lower_expr(index)?),
                width: width.clone(),
            }),
            HwExpr::Index { value, index } => Ok(SvExpr::Index {
                value: Box::new(self.lower_expr(value)?),
                index: Box::new(self.lower_expr(index)?),
            }),
            HwExpr::Call { name, args } => Ok(SvExpr::Call {
                name: self.lower_call_name(name),
                args: args
                    .iter()
                    .map(|arg| self.lower_expr(arg))
                    .collect::<Result<Vec<_>, _>>()?,
            }),
            _ => Err(CompileError::unsupported_hwir("unknown HWIR expression")),
        }
    }

    fn lower_call_name(&self, name: &str) -> String {
        match name {
            "clog2" => "$clog2".to_string(),
            _ => name.to_string(),
        }
    }

    fn lower_unary_op(&self, op: HwUnaryOp) -> Result<SvUnaryOp, CompileError> {
        match op {
            HwUnaryOp::Neg => Ok(SvUnaryOp::Neg),
            HwUnaryOp::Not => Ok(SvUnaryOp::Not),
            _ => Err(CompileError::unsupported_hwir("unknown unary operator")),
        }
    }

    fn lower_binary_op(&self, op: HwBinaryOp) -> Result<SvBinaryOp, CompileError> {
        match op {
            HwBinaryOp::OrOr => Ok(SvBinaryOp::OrOr),
            HwBinaryOp::AndAnd => Ok(SvBinaryOp::AndAnd),
            HwBinaryOp::Eq => Ok(SvBinaryOp::Eq),
            HwBinaryOp::NotEq => Ok(SvBinaryOp::NotEq),
            HwBinaryOp::Lt => Ok(SvBinaryOp::Lt),
            HwBinaryOp::LtEq => Ok(SvBinaryOp::LtEq),
            HwBinaryOp::Gt => Ok(SvBinaryOp::Gt),
            HwBinaryOp::GtEq => Ok(SvBinaryOp::GtEq),
            HwBinaryOp::Add => Ok(SvBinaryOp::Add),
            HwBinaryOp::Sub => Ok(SvBinaryOp::Sub),
            HwBinaryOp::Mul => Ok(SvBinaryOp::Mul),
            HwBinaryOp::Div => Ok(SvBinaryOp::Div),
            HwBinaryOp::Rem => Ok(SvBinaryOp::Rem),
            HwBinaryOp::Shl => Ok(SvBinaryOp::Shl),
            HwBinaryOp::BitAnd => Ok(SvBinaryOp::BitAnd),
            HwBinaryOp::BitOr => Ok(SvBinaryOp::BitOr),
            HwBinaryOp::BitXor => Ok(SvBinaryOp::BitXor),
            _ => Err(CompileError::unsupported_hwir("unknown binary operator")),
        }
    }
}
