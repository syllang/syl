use super::{EirExpr, EirOrigin, EirPlace, EirSelectArm};
use super::{EirItem, EirModule, EirReset};
use crate::{CompileError, EirError};

#[non_exhaustive]
pub(crate) struct EirValidator<'a> {
    modules: &'a [EirModule],
}

impl<'a> EirValidator<'a> {
    pub(crate) fn new(modules: &'a [EirModule]) -> Self {
        Self { modules }
    }

    pub(crate) fn validate(&self) -> Result<(), CompileError> {
        for module in self.modules {
            self.check_items(module.items())?;
        }
        Ok(())
    }

    fn check_items(&self, items: &[EirItem]) -> Result<(), CompileError> {
        for item in items {
            self.check_item(item)?;
        }
        Ok(())
    }

    fn check_item(&self, item: &EirItem) -> Result<(), CompileError> {
        match item {
            EirItem::StaticParam { value, origin, .. } => self.check_expr(value, origin),
            EirItem::Signal { .. } | EirItem::Storage { .. } => Ok(()),
            EirItem::Drive {
                lhs,
                rhs,
                reads,
                origin,
            } => {
                self.check_place(lhs, origin)?;
                self.check_expr(rhs, origin)?;
                self.check_exprs(reads, origin)
            }
            EirItem::ClockedStorage {
                clock,
                target,
                reset,
                next,
                reads,
                origin,
            } => {
                self.check_expr(clock, origin)?;
                self.check_place(target, origin)?;
                self.check_reset(reset.as_deref(), origin)?;
                self.check_expr(next, origin)?;
                self.check_exprs(reads, origin)
            }
            EirItem::CellExpansion(expansion) => self.check_items(expansion.items()),
            EirItem::Instance(instance) => {
                for connection in instance.connections() {
                    self.check_expr(connection.actual(), instance.origin())?;
                }
                Ok(())
            }
            EirItem::SymbolicStaticIf {
                cond,
                then_items,
                else_items,
                origin,
                ..
            } => {
                self.check_expr(cond, origin)?;
                self.check_items(then_items)?;
                self.check_items(else_items)
            }
            EirItem::SymbolicStaticFor {
                start,
                end,
                items,
                origin,
                ..
            } => {
                self.check_expr(start, origin)?;
                self.check_expr(end, origin)?;
                self.check_items(items)
            }
            EirItem::ClockedAssert {
                clock,
                trigger,
                reads,
                message,
                origin,
            } => {
                self.check_expr(clock, origin)?;
                self.check_expr(trigger, origin)?;
                self.check_exprs(reads, origin)?;
                self.check_expr(message, origin)
            }
            EirItem::InitialError { message, origin } => self.check_expr(message, origin),
        }
    }

    fn check_reset(
        &self,
        reset: Option<&EirReset>,
        origin: &EirOrigin,
    ) -> Result<(), CompileError> {
        if let Some(reset) = reset {
            self.check_expr(reset.condition(), origin)?;
            self.check_expr(reset.value(), origin)?;
        }
        Ok(())
    }

    fn check_place(&self, place: &EirPlace, origin: &EirOrigin) -> Result<(), CompileError> {
        match place {
            EirPlace::Ident(_) => Ok(()),
            EirPlace::Slice { base, high, low } => {
                self.check_place(base, origin)?;
                self.check_expr(high.expr(), origin)?;
                self.check_expr(low.expr(), origin)
            }
            EirPlace::IndexedPartSelect { base, index, width } => {
                self.check_place(base, origin)?;
                self.check_expr(index, origin)?;
                self.check_expr(width.expr(), origin)
            }
            EirPlace::Index { base, index } => {
                self.check_place(base, origin)?;
                self.check_expr(index, origin)
            }
        }
    }

    fn check_exprs(&self, exprs: &[EirExpr], origin: &EirOrigin) -> Result<(), CompileError> {
        for expr in exprs {
            self.check_expr(expr, origin)?;
        }
        Ok(())
    }

    fn check_expr(&self, expr: &EirExpr, origin: &EirOrigin) -> Result<(), CompileError> {
        match expr {
            EirExpr::Unsupported { .. } => Err(CompileError::lowering_at(
                EirError::UnsupportedHardwareValueExpression,
                origin.span(),
            )),
            EirExpr::Unary { expr, .. } => self.check_expr(expr, origin),
            EirExpr::Binary { left, right, .. } => {
                self.check_expr(left, origin)?;
                self.check_expr(right, origin)
            }
            EirExpr::Mux {
                cond,
                then_value,
                else_value,
            } => {
                self.check_expr(cond, origin)?;
                self.check_expr(then_value, origin)?;
                self.check_expr(else_value, origin)
            }
            EirExpr::Select { arms, default, .. } => {
                for arm in arms {
                    self.check_select_arm(arm, origin)?;
                }
                self.check_expr(default, origin)
            }
            EirExpr::Concat(parts) => self.check_exprs(parts, origin),
            EirExpr::Slice { value, high, low } => {
                self.check_expr(value, origin)?;
                self.check_expr(high.expr(), origin)?;
                self.check_expr(low.expr(), origin)
            }
            EirExpr::IndexedPartSelect {
                value,
                index,
                width,
            } => {
                self.check_expr(value, origin)?;
                self.check_expr(index, origin)?;
                self.check_expr(width.expr(), origin)
            }
            EirExpr::Index { value, index } => {
                self.check_expr(value, origin)?;
                self.check_expr(index, origin)
            }
            EirExpr::Call { args, .. } => self.check_exprs(args, origin),
            EirExpr::Ident(_)
            | EirExpr::Int(_)
            | EirExpr::Bool(_)
            | EirExpr::Str(_)
            | EirExpr::HighZ
            | EirExpr::Zero => Ok(()),
        }
    }

    fn check_select_arm(&self, arm: &EirSelectArm, origin: &EirOrigin) -> Result<(), CompileError> {
        self.check_expr(arm.guard(), origin)?;
        self.check_expr(arm.value(), origin)
    }
}

#[cfg(test)]
mod tests;
