use super::*;
use HwBinaryOp as Op;
use SimExpr as E;

fn predicate(op: Op) -> bool {
    matches!(
        op,
        Op::Eq | Op::NotEq | Op::Lt | Op::LtEq | Op::Gt | Op::GtEq | Op::AndAnd | Op::OrOr
    )
}

// Unsized decimal constants are signed. A packed signal, part-select,
// concatenation or unsigned arm makes its enclosing operation unsigned.
fn signed(e: &E) -> bool {
    match e {
        E::Int(_) => true,
        E::Unary {
            op: HwUnaryOp::Neg,
            expr,
        } => signed(expr),
        E::Binary {
            op: Op::Shl, left, ..
        } => signed(left),
        E::Binary { op, left, right } => !predicate(*op) && signed(left) && signed(right),
        E::Mux {
            then_value,
            else_value,
            ..
        } => signed(then_value) && signed(else_value),
        E::Select { arms, default, .. } => signed(default) && arms.iter().all(|(_, v)| signed(v)),
        _ => false,
    }
}
#[derive(Clone, Copy)]
struct ExprType {
    width: u32,
    signed: bool,
}

impl Simulator<'_> {
    pub(super) fn target_width(&self, t: &SimTarget) -> u32 {
        t.selectors
            .last()
            .map_or(self.design.signal(t.signal).width, Selector::width)
    }
    // SV propagates context back through arithmetic and the arms of a
    // conditional. Concatenation, predicates and shift distances are self-sized.
    fn natural(&self, e: &E) -> Result<u32> {
        Ok(match e {
            E::Read(t) => self.target_width(t),
            // Unsized decimal literals include their positive sign bit.
            E::Int(n) => (65 - n.leading_zeros()).max(32),
            E::Bool(_) | E::HighZ | E::Zero => 1,
            E::Unary {
                op: HwUnaryOp::Not, ..
            } => 1,
            E::Unary { expr, .. } => self.natural(expr)?,
            E::Binary { op, .. } if predicate(*op) => 1,
            E::Binary {
                op: Op::Shl, left, ..
            } => self.natural(left)?,
            E::Binary { left, right, .. } => self.natural(left)?.max(self.natural(right)?),
            E::Mux {
                then_value,
                else_value,
                ..
            } => self.natural(then_value)?.max(self.natural(else_value)?),
            E::Select { arms, default, .. } => {
                let mut w = self.natural(default)?;
                for (_, v) in arms {
                    w = w.max(self.natural(v)?)
                }
                w
            }
            E::Concat(parts) => {
                let mut w = 0u32;
                for p in parts {
                    w = w
                        .checked_add(self.natural(p)?)
                        .ok_or_else(|| RuntimeError::Invalid("concatenation too wide".into()))?
                }
                w
            }
            E::Project { selector, .. } => selector.width(),
            E::MemoryRead { memory, .. } => self.design.memories[memory.0].width,
        })
    }
    fn offset(&mut self, s: &Selector, parent: u32, site: Site) -> Result<Option<u32>> {
        let low = match s {
            Selector::Slice { low, .. } => Some(*low),
            Selector::Index { index, width } => self
                .eval(index, 0, site)?
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .and_then(|n| n.checked_mul(*width)),
        };
        Ok(low.filter(|n| n.checked_add(s.width()).is_some_and(|end| end <= parent)))
    }
    pub(super) fn target(&mut self, t: &SimTarget, site: Site) -> Result<Option<(SignalId, u32)>> {
        let s = self.design.signal(t.signal);
        let mut low = 0;
        let mut parent = s.width;
        for sel in &t.selectors {
            let Some(offset) = self.offset(sel, parent, site)? else {
                return Ok(None);
            };
            low += offset;
            parent = sel.width();
        }
        Ok(Some((t.signal, low)))
    }
    pub(super) fn read_target(&mut self, t: &SimTarget, site: Site) -> Result<Value> {
        let w = self.target_width(t);
        Ok(match self.target(t, site)? {
            Some((id, low)) if self.design.signal(id).alias.is_none() => {
                self.read(id).slice(low, w)
            }
            Some((id, low)) => self.net_value(id, site)?.slice(low, w),
            None => Value::x(w),
        })
    }
    pub(super) fn read_word(&mut self, id: MemId, addr: &E, site: Site) -> Result<Value> {
        let address = self.eval(addr, 0, site)?.as_u64();
        Ok(address
            .and_then(|n| usize::try_from(n).ok())
            .and_then(|n| self.memories[id.0].get(n))
            .cloned()
            .unwrap_or_else(|| Value::x(self.design.memories[id.0].width)))
    }
    pub(super) fn eval(&mut self, e: &E, context: u32, site: Site) -> Result<Value> {
        self.eval_as(e, context, site, self.mode)
    }
    pub(super) fn eval_as(
        &mut self,
        e: &E,
        context: u32,
        site: Site,
        mode: StateMode,
    ) -> Result<Value> {
        self.eval_typed(
            e,
            ExprType {
                width: context,
                signed: signed(e),
            },
            site,
            mode,
        )
    }
    fn eval_typed(
        &mut self,
        e: &E,
        mut ty: ExprType,
        site: Site,
        mode: StateMode,
    ) -> Result<Value> {
        ty.width = ty.width.max(self.natural(e)?);
        let w = ty.width;
        let v = match e {
            E::Read(t) => self.read_target(t, site)?,
            E::Int(n) => Value::from_u64(w, *n),
            E::Bool(b) => Value::from_u64(w, u64::from(*b)),
            E::HighZ => Value::z(w),
            E::Zero => Value::zero(w),
            E::MemoryRead { memory, port } => self.read_word(
                *memory,
                &self.design.memories[memory.0].comb_reads[*port].address,
                site,
            )?,
            E::Unary { op, expr } => match op {
                HwUnaryOp::Not => self.eval_as(expr, 0, site, mode)?.logical_not(),
                HwUnaryOp::Neg => Value::zero(w)
                    .binary(Op::Sub, &self.eval_typed(expr, ty, site, mode)?)
                    .map_err(RuntimeError::Invalid)?,
                _ => {
                    return Err(RuntimeError::Invalid(format!(
                        "unsupported unary operator {op:?}"
                    )));
                }
            },
            E::Binary { op, left, right } => {
                let operands = match op {
                    Op::AndAnd | Op::OrOr => 0,
                    op if predicate(*op) => self.natural(left)?.max(self.natural(right)?),
                    _ => w,
                };
                let signed_operands = if predicate(*op) {
                    signed(left) && signed(right)
                } else {
                    ty.signed
                };
                let operand_type = ExprType {
                    width: operands,
                    signed: signed_operands,
                };
                let a = if matches!(op, Op::AndAnd | Op::OrOr) {
                    self.eval_as(left, 0, site, mode)?
                } else {
                    self.eval_typed(left, operand_type, site, mode)?
                };
                let b = if matches!(op, Op::Shl | Op::AndAnd | Op::OrOr) {
                    self.eval_as(right, 0, site, mode)?
                } else {
                    self.eval_typed(right, operand_type, site, mode)?
                };
                if signed_operands {
                    a.signed_binary(*op, &b)
                } else {
                    a.binary(*op, &b)
                }
                .map_err(RuntimeError::Invalid)?
            }
            E::Mux {
                cond,
                then_value,
                else_value,
            } => match self.eval_as(cond, 0, site, mode)?.truth() {
                Logic::One => self.eval_typed(then_value, ty, site, mode)?,
                Logic::Zero => self.eval_typed(else_value, ty, site, mode)?,
                cond => Value::mux(
                    cond,
                    &self.eval_typed(then_value, ty, site, mode)?,
                    &self.eval_typed(else_value, ty, site, mode)?,
                ),
            },
            E::Select {
                mode: select_mode,
                arms,
                default,
            } => {
                let mut guards = Vec::with_capacity(arms.len());
                for (g, _) in arms {
                    guards.push(self.eval_as(g, 0, site, mode)?.truth())
                }
                if select_mode.is_unique()
                    && guards.iter().filter(|g| **g == Logic::One).count() > 1
                {
                    let warning = UniqueOverlapViolation {
                        path: site.path.into(),
                        span: site.span,
                        cycle: self.cycle,
                    };
                    if self.strict_unique {
                        return Err(warning.into());
                    }
                    if self.unique_violations.last() != Some(&warning) {
                        self.unique_violations.push(warning)
                    }
                }
                let winner = guards.iter().position(|g| *g == Logic::One);
                let mut result = self.eval_typed(
                    winner.map_or(default.as_ref(), |i| &arms[i].1),
                    ty,
                    site,
                    mode,
                )?;
                for i in (0..winner.unwrap_or(arms.len())).rev() {
                    if guards[i] == Logic::X {
                        result = Value::mux(
                            Logic::X,
                            &self.eval_typed(&arms[i].1, ty, site, mode)?,
                            &result,
                        )
                    }
                }
                result
            }
            E::Concat(parts) => {
                let values = parts
                    .iter()
                    .map(|p| self.eval_as(p, 0, site, mode))
                    .collect::<Result<Vec<_>>>()?;
                Value::concat(&values)
                    .ok_or_else(|| RuntimeError::Invalid("concatenation too wide".into()))?
            }
            E::Project { value, selector } => {
                let v = self.eval_as(value, 0, site, mode)?;
                let sw = selector.width();
                match self.offset(selector, v.width(), site)? {
                    Some(low) => v.slice(low, sw),
                    None => Value::x(sw),
                }
            }
        };
        let v = if v.width() == w { v } else { v.resized(w) };
        Ok(if mode == StateMode::TwoState {
            v.into_two_state()
        } else {
            v
        })
    }
}
