//! Word expressions address a bit range in the packed SoA signal arena.
//! Lane-local signals are resolved through the selected family's slot map.
use crate::{
    HwBinaryOp as Op, HwResetPolarity, HwUnaryOp, Selector, SignalId, SimDesign, SimExpr as E,
    SimTarget,
};
use std::fmt::Write;

/// Concrete arena indices owned by a single partition (or the entire DUT).
pub(super) struct Resources {
    pub slots: Vec<usize>,
    pub regs: Vec<usize>,
    pub memories: Vec<usize>,
    pub assertions: Vec<usize>,
}

impl Resources {
    pub fn select(d: &SimDesign, owns: impl Fn(crate::InstanceId) -> bool) -> Self {
        Self {
            slots: d
                .signals
                .iter()
                .enumerate()
                .filter_map(|(i, s)| owns(s.source.instance).then_some(i))
                .collect(),
            regs: d
                .regs
                .iter()
                .enumerate()
                .filter_map(|(i, r)| owns(r.source.instance).then_some(i))
                .collect(),
            memories: d
                .memories
                .iter()
                .enumerate()
                .filter_map(|(i, m)| owns(m.source.instance).then_some(i))
                .collect(),
            assertions: d
                .assertions
                .iter()
                .enumerate()
                .filter_map(|(i, a)| owns(a.source.instance).then_some(i))
                .collect(),
        }
    }
}

pub(super) struct Emit<'a> {
    pub design: &'a SimDesign,
    pub bases: &'a [usize],
    pub local: Option<&'a Resources>,
    pub lane: usize,
    pub lanes: usize,
}

impl Emit<'_> {
    pub fn slot(&self, id: SignalId) -> Result<String, String> {
        if let Some(resources) = self.local
            && self.design.signal(id).source.instance.0 != 0
        {
            let index = resources
                .slots
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
        if width > 128 {
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
        if width > 128 {
            return Err("expression exceeds 128 bits".into());
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
                let read = if w > 64 { "read_wide" } else { "read" };
                let value = format!("f.{read}({s},{l},{w})");
                return Ok(if width > 64 {
                    format!("Wide({value})")
                } else {
                    value
                });
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
                        format!("((Wide({v}) << {}) | {val})", self.width(p)?)
                    };
                }
                v
            }
            E::Project {
                value,
                selector: Selector::Slice { low, high },
            } => format!(
                "(({} >> {low}) & wide_mask({}))",
                self.expr(value, 0)?,
                high - low + 1
            ),
            E::MemoryRead { memory, port } => self.read_mem(
                memory.0,
                &self.design.memories[memory.0].comb_reads[*port].address,
            )?,
            _ => return Err(format!("unsupported expression {e:?}")),
        };
        let mask = if width > 64 { "wide_mask" } else { "mask" };
        Ok(format!("(({v}) & {mask}({width}))"))
    }
    fn mem_base(&self, m: usize) -> Result<String, String> {
        if let Some(local) = self.local {
            let slot = local
                .memories
                .iter()
                .position(|&id| id == m)
                .ok_or("cross-family memory dependency")?;
            return Ok(format!("family_mem_base[lane][{slot}]"));
        }
        Ok(self.design.memories[..m]
            .iter()
            .map(|v| u64::from(v.depth))
            .sum::<u64>()
            .to_string())
    }
    fn read_mem(&self, m: usize, address: &E) -> Result<String, String> {
        Ok(format!(
            "f.read_mem({},{},{})",
            self.mem_base(m)?,
            self.design.memories[m].depth,
            self.expr(address, 0)?
        ))
    }
    pub fn assign(&self, t: &SimTarget, v: &E) -> Result<String, String> {
        let (s, l, w) = self.target(t)?;
        let put = if w > 64 { "put_wide" } else { "put" };
        Ok(format!("f.{put}({s},{l},{w},{});\n", self.expr(v, w)?))
    }
    fn reset(&self, id: crate::ResetId) -> Result<String, String> {
        let r = &self.design.resets[id.0];
        Ok(format!(
            "(f.word({}) == {})",
            self.slot(r.signal)?,
            u8::from(r.polarity == HwResetPolarity::ActiveHigh)
        ))
    }
    pub fn edge(&self, resources: &Resources) -> Result<(String, String), String> {
        let mut sample = String::new();
        let mut commit = String::new();
        for (i, &id) in resources.regs.iter().enumerate() {
            let r = &self.design.regs[id];
            let mut val = self.expr(&r.next, r.width)?;
            if let Some((id, rv)) = &r.reset {
                val = format!(
                    "({} ? {} : {val})",
                    self.reset(*id)?,
                    self.expr(rv, r.width)?
                );
            }
            if r.width > 64 {
                writeln!(sample, "Wide next{i} = {val};").unwrap();
                writeln!(
                    commit,
                    "f.put_wide({},0,{},next{i});",
                    self.slot(r.signal)?,
                    r.width
                )
                .unwrap();
                continue;
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
        for (i, &id) in resources.memories.iter().enumerate() {
            let m = &self.design.memories[id];
            for (j, r) in m.sync_reads.iter().enumerate() {
                let (s, l, w) = self.target(&r.target)?;
                writeln!(
                    sample,
                    "bool re{i}_{j} = {}; Word rd{i}_{j} = {};",
                    self.expr(&r.enable, 0)?,
                    self.read_mem(id, &r.address)?
                )
                .unwrap();
                let put = if w > 64 { "put_wide" } else { "put" };
                writeln!(commit, "if(re{i}_{j}) f.{put}({s},{l},{w},rd{i}_{j});").unwrap();
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
                writeln!(commit,"if(we{i}_{j} && wa{i}_{j}<{}) f.mem[({}+wa{i}_{j})*f.batch+f.b] = wd{i}_{j} & mask({});",m.depth,self.mem_base(id)?,m.width).unwrap();
            }
        }
        for (i, &id) in resources.assertions.iter().enumerate() {
            let a = &self.design.assertions[id];
            let assert_id = if self.local.is_some() {
                format!("family_assert_id[lane][{i}]")
            } else {
                id.to_string()
            };
            let disabled = a
                .reset_mask
                .map(|id| self.reset(id))
                .transpose()?
                .unwrap_or_else(|| "false".into());
            writeln!(sample,"if(!{disabled} && !({})) atomicMin(f.failure, ((Word(cycle-1)*f.batch + f.b)*ASSERT_COUNT)+{assert_id});",self.expr(&a.condition,0)?).unwrap();
        }
        Ok((sample, commit))
    }
}
