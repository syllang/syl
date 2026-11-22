use super::{Lower, Scope};
use crate::{
    constant::{Result, dimension, invalid, number},
    design::*,
};
use syl_hw::HwExpr;

impl Lower<'_> {
    pub(super) fn expression(&mut self, expr: &HwExpr, scope: &Scope) -> Result<SimExpr> {
        Ok(match expr {
            HwExpr::Ident(name) => {
                if let Some(value) = scope.params.get(name) {
                    match value {
                        ParamValue::Nat(n) => SimExpr::Int(*n),
                        ParamValue::Bool(b) => SimExpr::Bool(*b),
                    }
                } else {
                    let target = scope.signals.get(name).ok_or_else(|| {
                        SimError::Invalid(format!("unknown signal {}.{name}", scope.path))
                    })?;
                    SimExpr::Read(target.clone())
                }
            }
            HwExpr::Int(n) => SimExpr::Int(*n),
            HwExpr::Bool(b) => SimExpr::Bool(*b),
            HwExpr::Zero => SimExpr::Zero,
            HwExpr::HighZ => SimExpr::HighZ,
            HwExpr::Unary { op, expr } => SimExpr::Unary {
                op: *op,
                expr: self.boxed(expr, scope)?,
            },
            HwExpr::Binary { op, left, right } => SimExpr::Binary {
                op: *op,
                left: self.boxed(left, scope)?,
                right: self.boxed(right, scope)?,
            },
            HwExpr::Mux {
                cond,
                then_value,
                else_value,
            } => SimExpr::Mux {
                cond: self.boxed(cond, scope)?,
                then_value: self.boxed(then_value, scope)?,
                else_value: self.boxed(else_value, scope)?,
            },
            HwExpr::Select {
                mode,
                arms,
                default,
            } => SimExpr::Select {
                mode: *mode,
                arms: arms
                    .iter()
                    .map(|arm| {
                        Ok((
                            self.expression(arm.guard(), scope)?,
                            self.expression(arm.value(), scope)?,
                        ))
                    })
                    .collect::<Result<_>>()?,
                default: self.boxed(default, scope)?,
            },
            HwExpr::Concat(parts) => SimExpr::Concat(
                parts
                    .iter()
                    .map(|p| self.expression(p, scope))
                    .collect::<Result<_>>()?,
            ),
            HwExpr::Slice { value, high, low } => {
                let high = self.bound(high, scope)?;
                let low = self.bound(low, scope)?;
                if low > high || high - low == u32::MAX {
                    return invalid(format!("invalid slice [{high}:{low}]"));
                }
                let value = self.expression(value, scope)?;
                self.project(value, Selector::Slice { high, low })?
            }
            HwExpr::Index { value, index } => {
                let address = self.expression(index, scope)?;
                if let HwExpr::Ident(name) = &**value
                    && let Some(&memory) = scope.memories.get(name)
                {
                    let mem = &mut self.design.memories[memory.0];
                    let port = mem.comb_reads.len();
                    mem.comb_reads.push(MemCombReadPort { address });
                    SimExpr::MemoryRead { memory, port }
                } else {
                    let value = self.expression(value, scope)?;
                    self.project(value, index_selector(address, 1)?)?
                }
            }
            HwExpr::IndexedPartSelect {
                value,
                index,
                width,
            } => {
                let width = dimension(width, &scope.params)?;
                let value = self.expression(value, scope)?;
                let index = self.expression(index, scope)?;
                self.project(value, index_selector(index, width)?)?
            }
            _ => return invalid(format!("unsupported value in {}: {expr:?}", scope.path)),
        })
    }

    fn boxed(&mut self, e: &HwExpr, s: &Scope) -> Result<Box<SimExpr>> {
        Ok(Box::new(self.expression(e, s)?))
    }

    fn bound(&self, text: &str, s: &Scope) -> Result<u32> {
        u32::try_from(number(text, &s.params)?)
            .map_err(|_| SimError::Invalid(format!("slice bound out of range: {text}")))
    }

    fn project(&self, value: SimExpr, selector: Selector) -> Result<SimExpr> {
        if let SimExpr::Read(mut target) = value {
            let width = target
                .selectors
                .last()
                .map(Selector::width)
                .unwrap_or(self.design.signal(target.signal).width);
            if matches!(&selector,Selector::Slice {high,..} if *high >= width)
                || selector.width() > width
            {
                return invalid(format!(
                    "projection {selector:?} outside {width}-bit signal {}",
                    self.design.signal(target.signal).source.path
                ));
            }
            target.selectors.push(selector);
            Ok(SimExpr::Read(target))
        } else {
            Ok(SimExpr::Project {
                value: Box::new(value),
                selector,
            })
        }
    }
}

fn index_selector(index: SimExpr, width: u32) -> Result<Selector> {
    if let SimExpr::Int(index) = index {
        let low = u32::try_from(index).ok().and_then(|i| i.checked_mul(width));
        if let Some(low) = low
            && let Some(high) = low.checked_add(width - 1)
        {
            return Ok(Selector::Slice { high, low });
        }
        return invalid(format!("part index {index} overflows at width {width}"));
    }
    Ok(Selector::Index {
        index: Box::new(index),
        width,
    })
}
