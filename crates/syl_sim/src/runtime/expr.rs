use super::*;
use HwBinaryOp as Op;
use SimExpr as E;

fn width(selector: &Selector) -> u32 {
    match selector {
        Selector::Slice { high, low } => high - low + 1,
        Selector::Index { width, .. } => *width,
    }
}
fn predicate(op: Op) -> bool {
    matches!(
        op,
        Op::Eq | Op::NotEq | Op::Lt | Op::LtEq | Op::Gt | Op::GtEq | Op::AndAnd | Op::OrOr
    )
}

impl Simulator<'_> {
    pub(super) fn target_width(&self, t: &SimTarget) -> u32 {
        t.selectors
            .last()
            .map_or(self.design.signal(t.signal).width, width)
    }
    // SV propagates context back through arithmetic and the arms of a
    // conditional. Concatenation, predicates and shift distances are self-sized.
    fn natural(&self, e: &E) -> Result<u32> {
        Ok(match e {
            E::Read(t) => self.target_width(t),
            E::Int(n) => (64 - n.leading_zeros()).max(32),
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
            E::Project { selector, .. } => width(selector),
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
        Ok(low.filter(|n| n.checked_add(width(s)).is_some_and(|end| end <= parent)))
    }
    pub(super) fn target(&mut self, t: &SimTarget, site: Site) -> Result<Option<(SignalId, u32)>> {
        let s = self.design.signal(t.signal);
        let (id, mut low) = if let Some(alias) = &s.alias {
            let Some(base) = self.target(alias, site)? else {
                return Ok(None);
            };
            base
        } else {
            (t.signal, 0)
        };
        let mut parent = s.width;
        for sel in &t.selectors {
            let Some(offset) = self.offset(sel, parent, site)? else {
                return Ok(None);
            };
            low += offset;
            parent = width(sel);
        }
        Ok(Some((id, low)))
    }
    pub(super) fn read_target(&mut self, t: &SimTarget, site: Site) -> Result<Value> {
        let w = self.target_width(t);
        Ok(match self.target(t, site)? {
            Some((id, low)) => self.values[id.0].slice(low, w),
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
        let w = context.max(self.natural(e)?);
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
                HwUnaryOp::Not => self.eval(expr, 0, site)?.logical_not(),
                HwUnaryOp::Neg => Value::zero(w)
                    .binary(Op::Sub, &self.eval(expr, w, site)?)
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
                let a = self.eval(left, operands, site)?;
                let b = self.eval(right, if *op == Op::Shl { 0 } else { operands }, site)?;
                a.binary(*op, &b).map_err(RuntimeError::Invalid)?
            }
            E::Mux {
                cond,
                then_value,
                else_value,
            } => match self.eval(cond, 0, site)?.truth() {
                Logic::One => self.eval(then_value, w, site)?,
                Logic::Zero => self.eval(else_value, w, site)?,
                cond => Value::mux(
                    cond,
                    &self.eval(then_value, w, site)?,
                    &self.eval(else_value, w, site)?,
                ),
            },
            E::Select {
                mode,
                arms,
                default,
            } => {
                let mut guards = Vec::with_capacity(arms.len());
                for (g, _) in arms {
                    guards.push(self.eval(g, 0, site)?.truth())
                }
                if mode.is_unique() && guards.iter().filter(|g| **g == Logic::One).count() > 1 {
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
                let mut result =
                    self.eval(winner.map_or(default.as_ref(), |i| &arms[i].1), w, site)?;
                for i in (0..winner.unwrap_or(arms.len())).rev() {
                    if guards[i] == Logic::X {
                        result = Value::mux(Logic::X, &self.eval(&arms[i].1, w, site)?, &result)
                    }
                }
                result
            }
            E::Concat(parts) => {
                let values = parts
                    .iter()
                    .map(|p| self.eval(p, 0, site))
                    .collect::<Result<Vec<_>>>()?;
                Value::concat(&values)
                    .ok_or_else(|| RuntimeError::Invalid("concatenation too wide".into()))?
            }
            E::Project { value, selector } => {
                let v = self.eval(value, 0, site)?;
                let sw = width(selector);
                match self.offset(selector, v.width(), site)? {
                    Some(low) => v.slice(low, sw),
                    None => Value::x(sw),
                }
            }
        };
        Ok(self.project(if v.width() == w { v } else { v.resized(w) }))
    }
}
