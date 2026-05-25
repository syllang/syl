use super::DriverExpr;
use crate::eir_expr::{EirBinaryOp, EirBound, EirExpr, EirUnaryOp};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub(crate) struct DriverBound {
    source: String,
    value: Option<u64>,
    formula: Option<DriverBoundFormula>,
}

impl DriverBound {
    #[cfg(test)]
    pub(crate) fn new(source: impl Into<String>) -> Self {
        Self::from_source(source)
    }

    #[cfg(test)]
    pub(crate) fn from_source(source: impl Into<String>) -> Self {
        let source = source.into();
        let formula = DriverBoundFormula::from_source(&source);
        Self::from_parts(source, formula)
    }

    fn from_formula(source: impl Into<String>, formula: DriverBoundFormula) -> Self {
        Self::from_parts(source.into(), Some(formula))
    }

    pub(crate) fn from_eir_bound(bound: &EirBound) -> Self {
        match DriverBoundFormula::from_eir_expr(bound.expr()) {
            Some(formula) => Self::from_formula(bound.source(), formula),
            None => Self::from_parts(bound.source().to_string(), None),
        }
    }

    #[cfg(test)]
    pub(crate) fn from_eir_expr(expr: &EirExpr) -> Self {
        let source = expr.fact_key();
        match DriverBoundFormula::from_eir_expr(expr) {
            Some(formula) => Self::from_formula(source, formula),
            None => Self::from_parts(source, None),
        }
    }

    fn from_parts(source: String, formula: Option<DriverBoundFormula>) -> Self {
        let value = formula
            .as_ref()
            .and_then(DriverBoundFormula::constant_value);
        Self {
            source,
            value,
            formula,
        }
    }

    pub(crate) fn source(&self) -> &str {
        &self.source
    }

    pub(crate) fn value(&self) -> Option<u64> {
        self.value
    }

    pub(crate) fn is_strictly_greater_than(&self, other: &Self) -> bool {
        let Some(left) = &self.formula else {
            return false;
        };
        let Some(right) = &other.formula else {
            return false;
        };
        left.is_strictly_greater_than(right)
    }

    #[cfg(test)]
    pub(crate) fn is_symbolic_extent(&self, extent: &str) -> bool {
        let Some(formula) = &self.formula else {
            return false;
        };
        DriverBoundFormula::from_source(extent)
            .as_ref()
            .is_some_and(|extent| formula == extent)
    }

    pub(crate) fn has_same_formula(&self, other: &Self) -> bool {
        let Some(formula) = &self.formula else {
            return false;
        };
        other
            .formula
            .as_ref()
            .is_some_and(|expected| formula == expected)
    }

    #[cfg(test)]
    pub(crate) fn is_symbolic_extent_times(&self, extent: &str, width: &Self) -> bool {
        let Some(formula) = &self.formula else {
            return false;
        };
        let Some(width) = &width.formula else {
            return false;
        };
        DriverBoundFormula::from_source(extent)
            .and_then(|extent| extent.multiply(width))
            .as_ref()
            .is_some_and(|expected| formula == expected)
    }

    pub(crate) fn has_product_formula(&self, extent: &Self, width: &Self) -> bool {
        let Some(formula) = &self.formula else {
            return false;
        };
        let Some(extent) = &extent.formula else {
            return false;
        };
        let Some(width) = &width.formula else {
            return false;
        };
        extent
            .multiply(width)
            .as_ref()
            .is_some_and(|expected| formula == expected)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub(crate) struct DriverBitRange {
    low: DriverBound,
    high: DriverBound,
}

impl DriverBitRange {
    #[cfg(test)]
    pub(crate) fn new(low: impl Into<String>, high: impl Into<String>) -> Self {
        Self {
            low: DriverBound::new(low),
            high: DriverBound::new(high),
        }
    }

    pub(crate) fn from_eir_bounds(low: &EirBound, high: &EirBound) -> Self {
        Self {
            low: DriverBound::from_eir_bound(low),
            high: DriverBound::from_eir_bound(high),
        }
    }

    pub(crate) fn low(&self) -> &DriverBound {
        &self.low
    }

    pub(crate) fn high(&self) -> &DriverBound {
        &self.high
    }

    pub(crate) fn display(&self) -> String {
        format!("{},{}", self.high.source(), self.low.source())
    }

    pub(crate) fn may_overlap(&self, other: &Self) -> bool {
        if let (Some(left), Some(right)) = (self.static_range(), other.static_range()) {
            return left.may_overlap(&right);
        }
        if let (Some(left_low), Some(right_high)) = (self.low.value(), other.high.value())
            && left_low > right_high
        {
            return false;
        }
        if let (Some(right_low), Some(left_high)) = (other.low.value(), self.high.value())
            && right_low > left_high
        {
            return false;
        }
        if self.low.is_strictly_greater_than(&other.high)
            || other.low.is_strictly_greater_than(&self.high)
        {
            return false;
        }
        true
    }

    pub(crate) fn may_contain_index(&self, index: u64) -> bool {
        if let Some(range) = self.static_range() {
            return range.contains(index);
        }
        if self.low.value().is_some_and(|low| index < low) {
            return false;
        }
        if self.high.value().is_some_and(|high| index > high) {
            return false;
        }
        true
    }

    pub(crate) fn static_range(&self) -> Option<DriverStaticRange> {
        Some(DriverStaticRange::new(
            self.low.value()?,
            self.high.value()?,
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) struct DriverStaticRange {
    low: u64,
    high: u64,
}

impl DriverStaticRange {
    pub(crate) fn new(first: u64, second: u64) -> Self {
        Self {
            low: first.min(second),
            high: first.max(second),
        }
    }

    pub(crate) fn from_indexed_part(index: &DriverExpr, width: &DriverBound) -> Option<Self> {
        let index = index.as_int()?;
        let width = width.value()?;
        if width == 0 {
            return None;
        }
        let low = index.checked_mul(width)?;
        let high = low.checked_add(width.checked_sub(1)?)?;
        Some(Self::new(low, high))
    }

    pub(crate) fn contains(&self, value: u64) -> bool {
        self.low <= value && value <= self.high
    }

    pub(crate) fn contains_range(&self, other: &Self) -> bool {
        self.low <= other.low && other.high <= self.high
    }

    pub(crate) fn checked_width(&self) -> Option<u64> {
        self.high.checked_sub(self.low)?.checked_add(1)
    }

    pub(crate) fn low(&self) -> u64 {
        self.low
    }

    pub(crate) fn high(&self) -> u64 {
        self.high
    }

    pub(crate) fn may_overlap(&self, other: &Self) -> bool {
        self.low <= other.high && other.low <= self.high
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub(crate) struct DriverBoundFormula {
    terms: BTreeMap<String, i128>,
    constant: i128,
}

impl DriverBoundFormula {
    pub(crate) fn from_eir_expr(expr: &EirExpr) -> Option<Self> {
        match expr {
            EirExpr::Ident(name) => Some(Self::from_symbol(SymbolicFactor::new(name).normalized())),
            EirExpr::Int(value) => Some(Self::from_constant(*value)),
            EirExpr::HighZ => None,
            EirExpr::Zero => Some(Self::zero()),
            EirExpr::Unary { op, expr } => match op {
                EirUnaryOp::Neg => Self::zero().subtract(&Self::from_eir_expr(expr)?),
                EirUnaryOp::Not => None,
            },
            EirExpr::Binary { op, left, right } => {
                let left = Self::from_eir_expr(left)?;
                let right = Self::from_eir_expr(right)?;
                match op {
                    EirBinaryOp::Add => left.add(&right),
                    EirBinaryOp::Sub => left.subtract(&right),
                    EirBinaryOp::Mul => left.multiply(&right),
                    EirBinaryOp::OrOr
                    | EirBinaryOp::AndAnd
                    | EirBinaryOp::Eq
                    | EirBinaryOp::NotEq
                    | EirBinaryOp::Lt
                    | EirBinaryOp::LtEq
                    | EirBinaryOp::Gt
                    | EirBinaryOp::GtEq
                    | EirBinaryOp::Div
                    | EirBinaryOp::Rem
                    | EirBinaryOp::Shl
                    | EirBinaryOp::BitAnd
                    | EirBinaryOp::BitOr
                    | EirBinaryOp::BitXor => None,
                }
            }
            EirExpr::Bool(_)
            | EirExpr::Str(_)
            | EirExpr::Mux { .. }
            | EirExpr::Select { .. }
            | EirExpr::Concat(_)
            | EirExpr::Slice { .. }
            | EirExpr::IndexedPartSelect { .. }
            | EirExpr::Index { .. }
            | EirExpr::Call { .. }
            | EirExpr::Unsupported { .. } => None,
        }
    }

    #[cfg(test)]
    fn from_source(source: &str) -> Option<Self> {
        DriverBoundExpression::new(source).parse()
    }

    fn zero() -> Self {
        Self {
            terms: BTreeMap::new(),
            constant: 0,
        }
    }

    fn from_constant(value: u64) -> Self {
        Self {
            terms: BTreeMap::new(),
            constant: i128::from(value),
        }
    }

    fn from_symbol(source: impl Into<String>) -> Self {
        let mut terms = BTreeMap::new();
        terms.insert(source.into(), 1);
        Self { terms, constant: 0 }
    }

    fn constant_value(&self) -> Option<u64> {
        if !self.terms.is_empty() || self.constant < 0 {
            return None;
        }
        u64::try_from(self.constant).ok()
    }

    fn add(&self, other: &Self) -> Option<Self> {
        let mut result = self.clone();
        result.constant = result.constant.checked_add(other.constant)?;
        for (term, coeff) in &other.terms {
            result.add_term(term, *coeff)?;
        }
        Some(result)
    }

    fn subtract(&self, other: &Self) -> Option<Self> {
        let mut result = self.clone();
        result.constant = result.constant.checked_sub(other.constant)?;
        for (term, coeff) in &other.terms {
            result.add_term(term, coeff.checked_neg()?)?;
        }
        Some(result)
    }

    fn multiply(&self, other: &Self) -> Option<Self> {
        if self.is_zero() || other.is_zero() {
            return Some(Self::zero());
        }
        if let Some(value) = self.signed_constant() {
            return other.scale(value);
        }
        if let Some(value) = other.signed_constant() {
            return self.scale(value);
        }
        Some(Self::from_symbol(
            SymbolicProduct::new([self.canonical(), other.canonical()]).canonical(),
        ))
    }

    fn is_strictly_greater_than(&self, other: &Self) -> bool {
        let Some(diff) = self.subtract(other) else {
            return false;
        };
        diff.constant > 0 && diff.terms.values().all(|coeff| *coeff >= 0)
    }

    fn add_term(&mut self, term: &str, coeff: i128) -> Option<()> {
        let current = self.terms.get(term).copied().unwrap_or(0);
        let next = current.checked_add(coeff)?;
        if next == 0 {
            self.terms.remove(term);
        } else {
            self.terms.insert(term.to_string(), next);
        }
        Some(())
    }

    fn is_zero(&self) -> bool {
        self.constant == 0 && self.terms.is_empty()
    }

    fn signed_constant(&self) -> Option<i128> {
        self.terms.is_empty().then_some(self.constant)
    }

    fn scale(&self, value: i128) -> Option<Self> {
        let mut result = Self {
            terms: BTreeMap::new(),
            constant: self.constant.checked_mul(value)?,
        };
        for (term, coeff) in &self.terms {
            result.add_term(term, coeff.checked_mul(value)?)?;
        }
        Some(result)
    }

    fn canonical(&self) -> String {
        let mut parts = Vec::new();
        for (term, coeff) in &self.terms {
            parts.push(Self::term_canonical(term, *coeff));
        }
        if self.constant != 0 || parts.is_empty() {
            parts.push(self.constant.to_string());
        }
        parts.join("+")
    }

    fn term_canonical(term: &str, coeff: i128) -> String {
        if coeff == 1 {
            return term.to_string();
        }
        if coeff == -1 {
            return format!("-{term}");
        }
        format!("{coeff}*{term}")
    }
}

#[cfg(test)]
#[non_exhaustive]
struct DriverBoundExpression<'a> {
    source: &'a str,
    cursor: usize,
}

#[cfg(test)]
impl<'a> DriverBoundExpression<'a> {
    fn new(source: &'a str) -> Self {
        Self { source, cursor: 0 }
    }

    fn parse(mut self) -> Option<DriverBoundFormula> {
        let value = self.parse_sum()?;
        self.skip_ws();
        self.is_at_end().then_some(value)
    }

    fn parse_sum(&mut self) -> Option<DriverBoundFormula> {
        let mut value = self.parse_product()?;
        loop {
            self.skip_ws();
            if self.consume(b'+') {
                value = value.add(&self.parse_product()?)?;
            } else if self.consume(b'-') {
                value = value.subtract(&self.parse_product()?)?;
            } else {
                return Some(value);
            }
        }
    }

    fn parse_product(&mut self) -> Option<DriverBoundFormula> {
        let mut value = self.parse_atom()?;
        loop {
            self.skip_ws();
            if self.consume(b'*') {
                value = value.multiply(&self.parse_atom()?)?;
            } else {
                return Some(value);
            }
        }
    }

    fn parse_atom(&mut self) -> Option<DriverBoundFormula> {
        self.skip_ws();
        if self.consume(b'(') {
            let value = self.parse_sum()?;
            self.skip_ws();
            return self.consume(b')').then_some(value);
        }
        self.parse_number().or_else(|| self.parse_symbol())
    }

    fn parse_number(&mut self) -> Option<DriverBoundFormula> {
        self.skip_ws();
        let start = self.cursor;
        while !self.is_at_end() && self.source.as_bytes()[self.cursor].is_ascii_digit() {
            self.cursor += 1;
        }
        (self.cursor > start)
            .then(|| self.source[start..self.cursor].parse().ok())
            .flatten()
            .map(DriverBoundFormula::from_constant)
    }

    fn parse_symbol(&mut self) -> Option<DriverBoundFormula> {
        self.skip_ws();
        let start = self.cursor;
        while !self.is_at_end() && self.is_symbol_byte(self.source.as_bytes()[self.cursor]) {
            self.cursor += 1;
        }
        (self.cursor > start).then(|| {
            DriverBoundFormula::from_symbol(
                SymbolicFactor::new(&self.source[start..self.cursor]).normalized(),
            )
        })
    }

    fn skip_ws(&mut self) {
        while !self.is_at_end() && self.source.as_bytes()[self.cursor].is_ascii_whitespace() {
            self.cursor += 1;
        }
    }

    fn consume(&mut self, byte: u8) -> bool {
        if self.is_at_end() || self.source.as_bytes()[self.cursor] != byte {
            return false;
        }
        self.cursor += 1;
        true
    }

    fn is_at_end(&self) -> bool {
        self.cursor >= self.source.len()
    }

    fn is_symbol_byte(&self, byte: u8) -> bool {
        !byte.is_ascii_whitespace() && !matches!(byte, b'+' | b'-' | b'*' | b'(' | b')')
    }
}

#[non_exhaustive]
struct SymbolicProduct {
    factors: Vec<String>,
}

impl SymbolicProduct {
    fn new<const N: usize>(factors: [String; N]) -> Self {
        let mut product = Self {
            factors: Vec::new(),
        };
        for factor in factors {
            product.extend(factor);
        }
        product.normalize();
        product
    }

    fn canonical(&self) -> String {
        self.factors.join("*")
    }

    fn extend(&mut self, factor: String) {
        for nested in TopLevelProduct::new(&factor).factors() {
            self.factors.push(nested);
        }
    }

    fn normalize(&mut self) {
        self.factors.sort();
    }
}

#[non_exhaustive]
struct SymbolicFactor<'a> {
    source: &'a str,
}

impl<'a> SymbolicFactor<'a> {
    fn new(source: &'a str) -> Self {
        Self { source }
    }

    fn normalized(&self) -> String {
        let mut source = self.source.trim();
        while let Some(inner) = Self::strip_outer_parens(source) {
            source = inner.trim();
        }
        source.chars().filter(|ch| !ch.is_whitespace()).collect()
    }

    fn strip_outer_parens(source: &str) -> Option<&str> {
        if !source.starts_with('(') || !source.ends_with(')') {
            return None;
        }
        let mut depth = 0usize;
        for (idx, byte) in source.bytes().enumerate() {
            match byte {
                b'(' => depth = depth.saturating_add(1),
                b')' => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 && idx + 1 < source.len() {
                        return None;
                    }
                }
                _ => {}
            }
        }
        (depth == 0).then_some(&source[1..source.len().saturating_sub(1)])
    }
}

#[non_exhaustive]
struct TopLevelProduct<'a> {
    source: &'a str,
}

impl<'a> TopLevelProduct<'a> {
    fn new(source: &'a str) -> Self {
        Self { source }
    }

    fn factors(&self) -> Vec<String> {
        let mut factors = Vec::new();
        let mut depth = 0usize;
        let mut start = 0usize;
        for (idx, byte) in self.source.bytes().enumerate() {
            match byte {
                b'(' => depth = depth.saturating_add(1),
                b')' => {
                    if let Some(next_depth) = depth.checked_sub(1) {
                        depth = next_depth;
                    }
                }
                b'*' if depth == 0 => {
                    factors.push(self.source[start..idx].to_string());
                    start = idx + 1;
                }
                _ => {}
            }
        }
        factors.push(self.source[start..].to_string());
        factors
    }
}

#[cfg(test)]
mod tests {
    use super::DriverBound;
    use crate::eir_expr::{EirBinaryOp, EirExpr};

    #[test]
    fn symbolic_extent_ignores_neutral_one_and_parentheses() {
        let bound = DriverBound::new("((N) * (1))");

        assert!(bound.is_symbolic_extent("N"));
    }

    #[test]
    fn symbolic_extent_times_accepts_commuted_product() {
        let total = DriverBound::new("(W + 1) * N");
        let width = DriverBound::new("W + 1");

        assert!(total.is_symbolic_extent_times("N", &width));
    }

    #[test]
    fn symbolic_extent_times_normalizes_numeric_width() {
        let total = DriverBound::new("(N)*(8)");
        let width = DriverBound::new("8");

        assert!(total.is_symbolic_extent_times("N", &width));
    }

    #[test]
    fn eir_expr_constructs_bound_formula_without_source_parsing() {
        let width = EirExpr::binary(EirBinaryOp::Add, EirExpr::ident("W"), EirExpr::Int(1));
        let total = DriverBound::from_eir_expr(&EirExpr::binary(
            EirBinaryOp::Mul,
            EirExpr::ident("N"),
            width.clone(),
        ));

        assert!(total.is_symbolic_extent_times("N", &DriverBound::from_eir_expr(&width)));
    }
}
