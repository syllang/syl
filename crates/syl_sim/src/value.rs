//! Unsigned packed vectors. Each word contains a value plane and an unknown
//! plane. In the unknown plane, value=0 encodes X and value=1 encodes Z.
use num_bigint::BigUint;
use smallvec::{SmallVec, smallvec};
use std::{cmp::Ordering, fmt, str::FromStr};
use syl_hw::HwBinaryOp as Op;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Logic {
    Zero,
    One,
    X,
    Z,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Word {
    data: u64,
    unknown: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Value {
    width: u32,
    words: SmallVec<[Word; 1]>,
}

impl Value {
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn zero(width: u32) -> Self {
        Self::fill(width, Word::default())
    }
    pub fn x(width: u32) -> Self {
        Self::fill(
            width,
            Word {
                data: 0,
                unknown: u64::MAX,
            },
        )
    }
    pub fn z(width: u32) -> Self {
        Self::fill(
            width,
            Word {
                data: u64::MAX,
                unknown: u64::MAX,
            },
        )
    }
    fn fill(width: u32, word: Word) -> Self {
        let mut v = Self {
            width,
            words: smallvec![word; width.div_ceil(64) as usize],
        };
        v.mask();
        v
    }
    fn mask(&mut self) {
        if !self.width.is_multiple_of(64) {
            let mask = u64::MAX >> (64 - self.width % 64);
            let last = self.words.last_mut().unwrap();
            last.data &= mask;
            last.unknown &= mask;
        }
    }
    pub fn from_u64(width: u32, n: u64) -> Self {
        let mut v = Self::zero(width);
        if let Some(w) = v.words.first_mut() {
            w.data = n;
        }
        v.mask();
        v
    }
    fn scalar(b: bool) -> Self {
        Self::from_u64(1, b.into())
    }
    pub fn as_u64(&self) -> Option<u64> {
        (self.is_known() && self.words.iter().skip(1).all(|w| w.data == 0))
            .then(|| self.word(0).data)
    }
    pub fn is_known(&self) -> bool {
        self.words.iter().all(|w| w.unknown == 0)
    }
    /// Project onto the two-state domain, mapping X and Z bits to zero.
    pub fn into_two_state(mut self) -> Self {
        for w in &mut self.words {
            w.data &= !w.unknown;
            w.unknown = 0;
        }
        self
    }
    pub fn has_x(&self) -> bool {
        self.words.iter().any(|w| w.unknown & !w.data != 0)
    }
    pub fn has_z(&self) -> bool {
        self.words.iter().any(|w| w.unknown & w.data != 0)
    }
    fn word(&self, i: usize) -> Word {
        self.words.get(i).copied().unwrap_or_default()
    }
    pub fn bit(&self, i: u32) -> Logic {
        if i >= self.width {
            return Logic::X;
        }
        let w = self.words[(i / 64) as usize];
        let mask = 1 << (i % 64);
        match (w.unknown & mask != 0, w.data & mask != 0) {
            (false, false) => Logic::Zero,
            (false, true) => Logic::One,
            (true, false) => Logic::X,
            (true, true) => Logic::Z,
        }
    }
    fn set_bit(&mut self, i: u32, b: Logic) {
        let w = &mut self.words[(i / 64) as usize];
        let mask = 1 << (i % 64);
        w.data = (w.data & !mask) | (u64::from(matches!(b, Logic::One | Logic::Z)) << (i % 64));
        w.unknown = (w.unknown & !mask) | (u64::from(matches!(b, Logic::X | Logic::Z)) << (i % 64));
    }
    pub fn resized(&self, width: u32) -> Self {
        let mut v = self.clone();
        v.words.resize(width.div_ceil(64) as usize, Word::default());
        v.width = width;
        v.mask();
        v
    }
    pub fn slice(&self, low: u32, width: u32) -> Self {
        if low.checked_add(width).is_none_or(|end| end > self.width) {
            return Self::x(width);
        }
        if low == 0 {
            return self.resized(width);
        }
        let mut v = Self::zero(width);
        for i in 0..width {
            v.set_bit(i, self.bit(low + i));
        }
        v
    }
    pub(crate) fn insert(&mut self, low: u32, v: &Self) {
        if low == 0 && self.width == v.width {
            *self = v.clone();
            return;
        }
        for i in 0..v.width {
            self.set_bit(low + i, v.bit(i));
        }
    }
    /// Combine independent drivers on a shared net. Z releases a bit and
    /// conflicting driven levels resolve to X.
    pub(crate) fn resolve(&mut self, low: u32, driver: &Self) {
        for i in 0..driver.width() {
            let value = match (self.bit(low + i), driver.bit(i)) {
                (Logic::Z, b) | (b, Logic::Z) => b,
                (a, b) if a == b => a,
                _ => Logic::X,
            };
            self.set_bit(low + i, value);
        }
    }
    pub fn concat(parts: &[Self]) -> Option<Self> {
        let width = parts.iter().try_fold(0u32, |w, v| w.checked_add(v.width))?;
        let mut v = Self::zero(width);
        let mut low = 0;
        for p in parts.iter().rev() {
            v.insert(low, p);
            low += p.width;
        }
        Some(v)
    }
    pub fn to_binary(&self) -> String {
        (0..self.width)
            .rev()
            .map(|i| match self.bit(i) {
                Logic::Zero => '0',
                Logic::One => '1',
                Logic::X => 'x',
                Logic::Z => 'z',
            })
            .collect()
    }
    pub fn to_hex(&self) -> String {
        (0..self.width.div_ceil(4))
            .rev()
            .map(|n| {
                let group = self.slice(n * 4, 4.min(self.width - n * 4));
                if let Some(d) = group.as_u64() {
                    char::from_digit(d as u32, 16).unwrap()
                } else if group == Self::z(group.width) {
                    'z'
                } else {
                    'x'
                }
            })
            .collect()
    }
    pub fn truth(&self) -> Logic {
        if self.words.iter().any(|w| w.data & !w.unknown != 0) {
            Logic::One
        } else if self.is_known() {
            Logic::Zero
        } else {
            Logic::X
        }
    }
    pub(crate) fn logical_not(&self) -> Self {
        match self.truth() {
            Logic::Zero => Self::scalar(true),
            Logic::One => Self::scalar(false),
            _ => Self::x(1),
        }
    }
    pub fn mux(cond: Logic, a: &Self, b: &Self) -> Self {
        let width = a.width.max(b.width);
        match cond {
            Logic::One => a.resized(width),
            Logic::Zero => b.resized(width),
            _ => {
                let mut out = Self::zero(width);
                for (i, w) in out.words.iter_mut().enumerate() {
                    let l = a.word(i);
                    let r = b.word(i);
                    let diff = (l.data ^ r.data) | (l.unknown ^ r.unknown);
                    w.data = l.data & !diff;
                    w.unknown = l.unknown | diff;
                }
                out
            }
        }
    }
    pub fn binary(&self, op: Op, rhs: &Self) -> Result<Self, String> {
        use Op::*;
        let width = self.width.max(rhs.width);
        match op {
            BitAnd | BitOr | BitXor => {
                let mut out = Self::zero(width);
                for (i, w) in out.words.iter_mut().enumerate() {
                    let a = self.word(i);
                    let b = rhs.word(i);
                    let x = a.unknown | b.unknown;
                    *w = match op {
                        BitAnd => Word {
                            data: a.data & b.data & !x,
                            unknown: x & (a.data | a.unknown) & (b.data | b.unknown),
                        },
                        BitOr => {
                            let ones = (a.data & !a.unknown) | (b.data & !b.unknown);
                            Word {
                                data: ones,
                                unknown: x & !ones,
                            }
                        }
                        _ => Word {
                            data: (a.data ^ b.data) & !x,
                            unknown: x,
                        },
                    };
                }
                Ok(out)
            }
            AndAnd | OrOr => {
                let pair = (self.truth(), rhs.truth());
                let value = match (op, pair) {
                    (AndAnd, (Logic::Zero, _) | (_, Logic::Zero)) => Some(false),
                    (AndAnd, (Logic::One, Logic::One)) => Some(true),
                    (OrOr, (Logic::One, _) | (_, Logic::One)) => Some(true),
                    (OrOr, (Logic::Zero, Logic::Zero)) => Some(false),
                    _ => None,
                };
                Ok(value.map_or_else(|| Self::x(1), Self::scalar))
            }
            Eq | NotEq => {
                let different = (0..self.words.len().max(rhs.words.len())).any(|i| {
                    let a = self.word(i);
                    let b = rhs.word(i);
                    (a.data ^ b.data) & !(a.unknown | b.unknown) != 0
                });
                Ok(if different {
                    Self::scalar(op == NotEq)
                } else if self.is_known() && rhs.is_known() {
                    Self::scalar(op == Eq)
                } else {
                    Self::x(1)
                })
            }
            Shl => {
                if !rhs.is_known() {
                    return Ok(Self::x(self.width));
                }
                let mut out = Self::zero(self.width);
                if let Some(n) = rhs.as_u64().filter(|n| *n < u64::from(self.width)) {
                    let n = n as u32;
                    out.insert(n, &self.slice(0, self.width - n));
                }
                Ok(out)
            }
            Lt | LtEq | Gt | GtEq => {
                if !self.is_known() || !rhs.is_known() {
                    return Ok(Self::x(1));
                }
                let order = (0..self.words.len().max(rhs.words.len()))
                    .rev()
                    .map(|i| self.word(i).data.cmp(&rhs.word(i).data))
                    .find(|o| *o != Ordering::Equal)
                    .unwrap_or(Ordering::Equal);
                Ok(Self::scalar(match op {
                    Lt => order.is_lt(),
                    LtEq => order.is_le(),
                    Gt => order.is_gt(),
                    _ => order.is_ge(),
                }))
            }
            Add | Sub | Mul | Div | Rem => {
                if !self.is_known()
                    || !rhs.is_known()
                    || (matches!(op, Div | Rem) && rhs.truth() == Logic::Zero)
                {
                    return Ok(Self::x(width));
                }
                if width <= 64 {
                    let a = self.word(0).data;
                    let b = rhs.word(0).data;
                    return Ok(Self::from_u64(
                        width,
                        match op {
                            Add => a.wrapping_add(b),
                            Sub => a.wrapping_sub(b),
                            Mul => a.wrapping_mul(b),
                            Div => a / b,
                            _ => a % b,
                        },
                    ));
                }
                let integer = |v: &Self| {
                    BigUint::new(
                        v.words
                            .iter()
                            .flat_map(|w| [w.data as u32, (w.data >> 32) as u32])
                            .collect(),
                    )
                };
                let a = integer(self);
                let b = integer(rhs);
                let n = match op {
                    Add => a + b,
                    Sub if a < b => (BigUint::from(1u8) << width) + a - b,
                    Sub => a - b,
                    Mul => a * b,
                    Div => a / b,
                    _ => a % b,
                };
                let mut v = Self::zero(width);
                for (w, n) in v.words.iter_mut().zip(n.iter_u64_digits()) {
                    w.data = n
                }
                v.mask();
                Ok(v)
            }
            _ => Err(format!("unsupported binary operator {op:?}")),
        }
    }
}
impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_binary())
    }
}
impl FromStr for Value {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        let s = s.strip_prefix("0b").unwrap_or(s);
        let width = u32::try_from(s.len()).map_err(|_| "vector too wide")?;
        if width == 0 {
            return Err("empty bit string".into());
        }
        let mut v = Self::zero(width);
        for (i, c) in s.bytes().rev().enumerate() {
            v.set_bit(
                i as u32,
                match c {
                    b'0' => Logic::Zero,
                    b'1' => Logic::One,
                    b'x' | b'X' => Logic::X,
                    b'z' | b'Z' => Logic::Z,
                    _ => return Err(format!("invalid binary digit in {s:?}")),
                },
            );
        }
        Ok(v)
    }
}

#[cfg(test)]
#[path = "value_tests.rs"]
mod tests;
