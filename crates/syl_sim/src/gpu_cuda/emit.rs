//! Word expressions address a bit range in the packed SoA signal arena.
//! Lane-local signals are resolved through the selected family's slot map.
use crate::{
    HwBinaryOp as Op, HwResetPolarity, HwUnaryOp, Selector, SignalId, SimDesign, SimExpr as E,
    SimTarget,
};
use std::fmt::Write;

pub(super) struct Emit<'a> {
    pub design: &'a SimDesign,
    pub bases: &'a [usize],
    pub local: Option<&'a [usize]>,
    pub lane: usize,
    pub lanes: usize,
}

impl Emit<'_> {
    pub fn slot(&self, id: SignalId) -> Result<String, String> {
        if let Some(slots) = self.local
            && self.design.signal(id).source.instance.0 != 0
        {
            let index = slots
                .iter()
                .position(|s| *s == id.0)
                .ok_or("cross-family dependency")?;
            return Ok(format!("family_slot_map[lane][{index}]"));
        }
        Ok(self.bases[id.0].to_string())
    }
    fn target(&self, t: &SimTarget) -> Result<(String, String, u32), String> {
        let mut low = 0;
        let mut width = self.design.signal(t.signal).width;
        for sel in &t.selectors {
            let Selector::Slice { low: l, high } = *sel else {
                return Err("dynamic signal selectors are unsupported".into());
            };
            if high >= width || high < l {
                return Err("invalid static slice".into());
            }
            low += l;
            width = high - l + 1;
        }
        if width > 64 {
            return Err(format!("wide unsliced expression on {:?}", t.signal));
        }
        let offset = if self.local.is_some()
            && self.design.signal(t.signal).source.instance.0 == 0
            && !t.selectors.is_empty()
        {
            if low as usize != self.lane * width as usize
                || self.design.signal(t.signal).width as usize != self.lanes * width as usize
            {
                return Err("non-uniform family port slice".into());
            }
            format!("(lane*{width})")
        } else {
            low.to_string()
        };
        Ok((self.slot(t.signal)?, offset, width))
    }
    fn width(&self, e: &E) -> Result<u32, String> {
        Ok(match e {
            E::Read(t) => self.target(t)?.2,
            E::Int(n) => (65 - n.leading_zeros()).max(32),
            E::Zero
            | E::Bool(_)
            | E::Unary {
                op: HwUnaryOp::Not, ..
            } => 1,
            E::Concat(parts) => parts
                .iter()
                .try_fold(0, |w, p| self.width(p).map(|v| v + w))?,
            E::Project { selector, .. } => selector.width(),
            E::MemoryRead { memory, .. } => self.design.memories[memory.0].width,
            E::Unary { expr, .. } => self.width(expr)?,
            E::Binary { op, left, right } => match op {
                Op::Lt
                | Op::Gt
                | Op::Eq
                | Op::NotEq
                | Op::GtEq
                | Op::LtEq
                | Op::AndAnd
                | Op::OrOr => 1,
                Op::Shl => self.width(left)?,
                _ => self.width(left)?.max(self.width(right)?),
            },
            E::Mux {
                then_value,
                else_value,
                ..
            } => self.width(then_value)?.max(self.width(else_value)?),
            E::Select { arms, default, .. } => {
                arms.iter().try_fold(self.width(default)?, |w, (_, v)| {
                    self.width(v).map(|n| w.max(n))
                })?
            }
            E::HighZ => return Err("four-state logic is unsupported".into()),
        })
    }
    fn expr(&self, e: &E, context: u32) -> Result<String, String> {
        let width = self.width(e)?.max(context);
        if width > 64 {
            return Err("expression exceeds 64 bits".into());
        }
        let v = match e {
            E::Read(t) => {
                if self.local.is_some()
                    && self.design.signal(t.signal).source.instance.0 == 0
                    && !self
                        .design
                        .top_ports()
                        .iter()
                        .any(|p| p.signal == t.signal && p.direction == crate::HwDirection::In)
                {
                    return Err("family input must read a top input or its own local state".into());
                }
                let (s, l, w) = self.target(t)?;
                return Ok(format!("f.read({s},{l},{w})"));
            }
            E::Bool(b) => u8::from(*b).to_string(),
            E::Zero => "0ull".into(),
            E::Int(n) => format!("{n}ull"),
            E::Unary {
                op: HwUnaryOp::Not,
                expr,
            } => format!("!({})", self.expr(expr, 0)?),
            E::Binary { op, left, right } => {
                let op = match op {
                    Op::Add => "+",
                    Op::Sub => "-",
                    Op::Mul => "*",
                    Op::Eq => "==",
                    Op::NotEq => "!=",
                    Op::BitAnd => "&",
                    Op::BitOr => "|",
                    Op::BitXor => "^",
                    Op::AndAnd => "&&",
                    Op::OrOr => "||",
                    _ => return Err(format!("unsupported binary operation {op:?}")),
                };
                let operand_width = match op {
                    "&&" | "||" => 0,
                    "==" | "!=" => self.width(left)?.max(self.width(right)?),
                    _ => width,
                };
                let a = self.expr(left, operand_width)?;
                let b = self.expr(right, operand_width)?;
                format!("(({a}) {op} ({b}))")
            }
            E::Mux {
                cond,
                then_value,
                else_value,
            } => format!(
                "({} ? {} : {})",
                self.expr(cond, 0)?,
                self.expr(then_value, width)?,
                self.expr(else_value, width)?
            ),
            E::Select { arms, default, .. } => {
                let mut v = self.expr(default, width)?;
                for (g, a) in arms.iter().rev() {
                    v = format!("({} ? {} : {v})", self.expr(g, 0)?, self.expr(a, width)?);
                }
                v
            }
            E::Concat(parts) => {
                let mut v = String::new();
                for p in parts {
                    let val = self.expr(p, 0)?;
                    v = if v.is_empty() {
                        val
                    } else {
                        format!("(({v} << {}) | {val})", self.width(p)?)
                    };
                }
                v
            }
            E::Project {
                value,
                selector: Selector::Slice { low, high },
            } => format!(
                "(({} >> {low}) & mask({}))",
                self.expr(value, 0)?,
                high - low + 1
            ),
            E::MemoryRead { memory, port } => self.read_mem(
                memory.0,
                &self.design.memories[memory.0].comb_reads[*port].address,
            )?,
            _ => return Err(format!("unsupported expression {e:?}")),
        };
        Ok(format!("(({v}) & mask({width}))"))
    }
    fn mem_base(&self, m: usize) -> usize {
        self.design.memories[..m]
            .iter()
            .map(|v| v.depth as usize)
            .sum()
    }
    fn read_mem(&self, m: usize, address: &E) -> Result<String, String> {
        Ok(format!(
            "f.read_mem({},{},{})",
            self.mem_base(m),
            self.design.memories[m].depth,
            self.expr(address, 0)?
        ))
    }
    pub fn assign(&self, t: &SimTarget, v: &E) -> Result<String, String> {
        let (s, l, w) = self.target(t)?;
        Ok(format!("f.put({s},{l},{w},{});\n", self.expr(v, w)?))
    }
    fn reset(&self, id: crate::ResetId) -> Result<String, String> {
        let r = &self.design.resets[id.0];
        Ok(format!(
            "(f.word({}) == {})",
            self.slot(r.signal)?,
            u8::from(r.polarity == HwResetPolarity::ActiveHigh)
        ))
    }
    pub fn edge(&self) -> Result<String, String> {
        let mut sample = String::new();
        let mut commit = String::new();
        let owns = |id: usize| self.local.is_none_or(|slots| slots.contains(&id));
        for (i, r) in self
            .design
            .regs
            .iter()
            .filter(|r| owns(r.signal.0))
            .enumerate()
        {
            let mut val = self.expr(&r.next, r.width)?;
            if let Some((id, rv)) = &r.reset {
                val = format!(
                    "({} ? {} : {val})",
                    self.reset(*id)?,
                    self.expr(rv, r.width)?
                );
            }
            writeln!(sample, "Word next{i} = {val};").unwrap();
            writeln!(
                commit,
                "f.word({}) = next{i} & mask({});",
                self.slot(r.signal)?,
                r.width
            )
            .unwrap();
        }
        for (i, m) in self.design.memories.iter().enumerate() {
            for (j, r) in m.sync_reads.iter().enumerate() {
                let (s, l, w) = self.target(&r.target)?;
                writeln!(
                    sample,
                    "bool re{i}_{j} = {}; Word rd{i}_{j} = {};",
                    self.expr(&r.enable, 0)?,
                    self.read_mem(i, &r.address)?
                )
                .unwrap();
                writeln!(commit, "if(re{i}_{j}) f.put({s},{l},{w},rd{i}_{j});").unwrap();
            }
            for (j, p) in m.writes.iter().enumerate() {
                writeln!(
                    sample,
                    "bool we{i}_{j} = {}; Word wa{i}_{j} = {}, wd{i}_{j} = {};",
                    self.expr(&p.enable, 0)?,
                    self.expr(&p.address, 0)?,
                    self.expr(&p.data, m.width)?
                )
                .unwrap();
                writeln!(commit,"if(we{i}_{j} && wa{i}_{j}<{}) f.mem[({}+wa{i}_{j})*f.batch+f.b] = wd{i}_{j} & mask({});",m.depth,self.mem_base(i),m.width).unwrap();
            }
        }
        for (i, a) in self.design.assertions.iter().enumerate() {
            let disabled = a
                .reset_mask
                .map(|id| self.reset(id))
                .transpose()?
                .unwrap_or_else(|| "false".into());
            writeln!(sample,"if(!{disabled} && !({})) atomicMin(f.failure, ((Word(cycle-1)*f.batch + f.b)*ASSERT_COUNT)+{i});",self.expr(&a.condition,0)?).unwrap();
        }
        Ok(sample + &commit)
    }
}
