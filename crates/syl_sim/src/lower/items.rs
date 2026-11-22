use super::{Lower, Scope};
use crate::{
    constant::{Result, dimension, eval, invalid},
    design::*,
};
use syl_hw::{HwBinaryOp, HwExpr, HwItem, HwOrigin, HwResetPolarity, HwUnaryOp, ParametricHwItem};

impl Lower<'_> {
    pub(super) fn block(&mut self, items: &[ParametricHwItem], scope: &mut Scope) -> Result<()> {
        // Predeclare all nets in this lexical scope. Next-state and instance
        // connections can reference a wire declared later in source order.
        for item in items {
            let ParametricHwItem::Core { item, origin } = item else {
                continue;
            };
            match item {
                HwItem::StaticParam { name, value } => {
                    scope
                        .params
                        .insert(name.clone(), ParamValue::Nat(eval(value, &scope.params)?));
                }
                HwItem::SignalDecl { width, name } | HwItem::StorageDecl { width, name } => {
                    self.declare(
                        scope,
                        name,
                        dimension(width, &scope.params)?,
                        scope.source(name, origin),
                    )?;
                }
                HwItem::MemoryDecl {
                    name,
                    elem_width,
                    depth,
                } => {
                    let id = MemId(self.design.memories.len());
                    if scope.memories.insert(name.clone(), id).is_some() {
                        return invalid(format!("duplicate memory {}.{name}", scope.path));
                    }
                    self.design.memories.push(SimMem {
                        source: scope.source(name, origin),
                        width: dimension(elem_width, &scope.params)?,
                        depth: dimension(depth, &scope.params)?,
                        writes: vec![],
                        sync_reads: vec![],
                        comb_reads: vec![],
                    });
                }
                _ => {}
            }
        }
        for item in items {
            match item {
                ParametricHwItem::Core { item, origin } => self.core(item, origin, scope)?,
                ParametricHwItem::StaticIf {
                    cond,
                    label,
                    then_items,
                    else_items,
                    ..
                } => {
                    let mut nested = scope.clone();
                    nested.path = format!("{}.{label}", scope.path);
                    self.block(
                        if eval(cond, &scope.params)? != 0 {
                            then_items
                        } else {
                            else_items
                        },
                        &mut nested,
                    )?;
                }
                ParametricHwItem::StaticFor {
                    index,
                    start,
                    end,
                    label,
                    items,
                    ..
                } => {
                    for i in eval(start, &scope.params)?..eval(end, &scope.params)? {
                        let mut nested = scope.clone();
                        nested.params.insert(index.clone(), ParamValue::Nat(i));
                        nested.indices.push((index.clone(), i));
                        nested.path = format!("{}.{label}[{i}]", scope.path);
                        self.block(items, &mut nested)?;
                    }
                }
                _ => return invalid("unrecognized generated item"),
            }
        }
        Ok(())
    }

    fn core(&mut self, item: &HwItem, origin: &HwOrigin, scope: &Scope) -> Result<()> {
        match item {
            HwItem::StaticParam { .. }
            | HwItem::SignalDecl { .. }
            | HwItem::StorageDecl { .. }
            | HwItem::MemoryDecl { .. } => {}
            HwItem::ContinuousDrive { lhs, rhs } => {
                let target = self.expression(lhs, scope)?.as_target()?.clone();
                let value = self.expression(rhs, scope)?;
                self.design.comb.push(CombAssign { target, value });
            }
            HwItem::Instance(inst) => self.child(inst, origin, scope)?,
            HwItem::ClockedStorage {
                clock,
                target,
                reset,
                next,
            } => {
                let signal = self.expression(target, scope)?.signal()?;
                let clock = self.clock(clock, scope)?;
                let reset = reset
                    .as_ref()
                    .map(|r| {
                        let signal = self.expression(r.condition(), scope)?.signal()?;
                        let id = *self.resets.get(&signal).ok_or_else(|| {
                            SimError::Invalid(format!("unknown reset in {}", scope.path))
                        })?;
                        let domain = &self.design.resets[id.0];
                        if domain.mode != r.mode() || domain.polarity != r.polarity() {
                            return invalid("register and port reset type disagree");
                        }
                        Ok((id, self.expression(r.value(), scope)?))
                    })
                    .transpose()?;
                let next = self.expression(next, scope)?;
                let state = self.design.signal(signal);
                self.design.regs.push(SimReg {
                    source: state.source.clone(),
                    width: state.width,
                    signal,
                    clock,
                    reset,
                    next,
                });
            }
            HwItem::MemoryWrite {
                clock,
                memory,
                enable,
                addr,
                data,
            } => {
                let port = MemWritePort {
                    clock: self.clock(clock, scope)?,
                    enable: self.expression(enable, scope)?,
                    address: self.expression(addr, scope)?,
                    data: self.expression(data, scope)?,
                };
                self.design.memories[scope.memory(memory)?.0]
                    .writes
                    .push(port);
            }
            HwItem::MemorySyncRead {
                clock,
                memory,
                enable,
                addr,
                target,
            } => {
                let port = MemSyncReadPort {
                    clock: self.clock(clock, scope)?,
                    enable: self.expression(enable, scope)?,
                    address: self.expression(addr, scope)?,
                    target: self.expression(target, scope)?.as_target()?.clone(),
                };
                self.design.memories[scope.memory(memory)?.0]
                    .sync_reads
                    .push(port);
            }
            HwItem::ClockedAssert {
                clock,
                trigger,
                message,
            } => {
                let clock = self.clock(clock, scope)?;
                let (reset_mask, condition) = self.assertion(trigger, scope)?;
                let name = format!("assert[{}]", self.design.assertions.len());
                self.design.assertions.push(SimAssert {
                    source: scope.source(&name, origin),
                    clock,
                    reset_mask,
                    condition,
                    message: string(message)?.into(),
                });
            }
            HwItem::InitialError { message } => {
                return invalid(format!("{}: {}", scope.path, string(message)?));
            }
            _ => return invalid(format!("unsupported item in {}: {item:?}", scope.path)),
        }
        Ok(())
    }

    fn clock(&mut self, e: &HwExpr, scope: &Scope) -> Result<DomainId> {
        let signal = self.expression(e, scope)?.signal()?;
        self.clocks.get(&signal).copied().ok_or_else(|| {
            SimError::Invalid(format!(
                "unknown clock {}",
                self.design.signal(signal).source.path
            ))
        })
    }

    fn assertion(&mut self, trigger: &HwExpr, scope: &Scope) -> Result<(Option<ResetId>, SimExpr)> {
        // EIR emits !condition, optionally ANDed with reset deasserted.
        let (mask, failure) = if let HwExpr::Binary {
            op: HwBinaryOp::BitAnd,
            left,
            right,
        } = trigger
        {
            let (reset, polarity) = match &**left {
                HwExpr::Unary {
                    op: HwUnaryOp::Not,
                    expr,
                } => (&**expr, HwResetPolarity::ActiveHigh),
                other => (other, HwResetPolarity::ActiveLow),
            };
            let signal = self.expression(reset, scope)?.signal()?;
            let id = self
                .resets
                .get(&signal)
                .copied()
                .filter(|id| self.design.resets[id.0].polarity == polarity)
                .ok_or_else(|| {
                    SimError::Invalid(format!("invalid assertion reset in {}", scope.path))
                })?;
            (Some(id), &**right)
        } else {
            (None, trigger)
        };
        let HwExpr::Unary {
            op: HwUnaryOp::Not,
            expr,
        } = failure
        else {
            return invalid("expected negated assertion condition");
        };
        Ok((mask, self.expression(expr, scope)?))
    }
}

fn string(expr: &HwExpr) -> Result<&str> {
    match expr {
        HwExpr::Str(s) => Ok(s),
        _ => invalid("expected a string message"),
    }
}
