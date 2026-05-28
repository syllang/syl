use crate::driver::DriveFact;
use crate::eir::{EirBinaryOp, EirExpr, EirSelectArm, EirUnaryOp};
use std::collections::BTreeMap;

#[non_exhaustive]
pub(super) struct DriveConflict<'a> {
    previous: &'a DriveFact,
    current: &'a DriveFact,
}

impl<'a> DriveConflict<'a> {
    pub(super) fn new(previous: &'a DriveFact, current: &'a DriveFact) -> Self {
        Self { previous, current }
    }

    pub(super) fn can_conflict(&self) -> bool {
        DriveActivity::from_drive(self.previous)
            .can_overlap(&DriveActivity::from_drive(self.current))
    }
}

#[derive(Clone)]
enum DriveActivity {
    Inactive,
    Active(Option<BoolFormula>),
}

impl DriveActivity {
    fn from_drive(drive: &DriveFact) -> Self {
        DriveActivityClassifier::new(drive.value()).classify()
    }

    fn can_overlap(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Inactive, _) | (_, Self::Inactive) => false,
            (Self::Active(Some(left)), Self::Active(Some(right))) => {
                !BoolProof::new(left, right).is_mutually_exclusive()
            }
            (Self::Active(_), Self::Active(_)) => true,
        }
    }
}

struct DriveActivityClassifier<'a> {
    value: Option<&'a EirExpr>,
}

impl<'a> DriveActivityClassifier<'a> {
    fn new(value: Option<&'a EirExpr>) -> Self {
        Self { value }
    }

    fn classify(&self) -> DriveActivity {
        let Some(value) = self.value else {
            return DriveActivity::Active(Some(BoolFormula::True));
        };
        self.classify_expr(value)
    }

    fn classify_expr(&self, expr: &EirExpr) -> DriveActivity {
        match expr {
            EirExpr::HighZ => DriveActivity::Inactive,
            EirExpr::Unsupported { .. } => DriveActivity::Active(None),
            EirExpr::Mux {
                cond,
                then_value,
                else_value,
            } => self.classify_mux(cond, then_value, else_value),
            EirExpr::Select { arms, default, .. } => self.classify_select(arms, default),
            _ => DriveActivity::Active(Some(BoolFormula::True)),
        }
    }

    fn classify_mux(
        &self,
        cond: &EirExpr,
        then_value: &EirExpr,
        else_value: &EirExpr,
    ) -> DriveActivity {
        let mut enables = Vec::new();
        if self.is_active_value(then_value) {
            enables.push(BoolFormula::from_expr(cond));
        }
        if self.is_active_value(else_value) {
            enables.push(BoolFormula::from_expr(cond).negated());
        }
        self.activity_from_enables(enables)
    }

    fn classify_select(&self, arms: &[EirSelectArm], default: &EirExpr) -> DriveActivity {
        if self.is_active_value(default) {
            return DriveActivity::Active(Some(BoolFormula::True));
        }
        let enables = arms
            .iter()
            .filter(|arm| self.is_active_value(arm.value()))
            .map(|arm| BoolFormula::from_expr(arm.guard()))
            .collect();
        self.activity_from_enables(enables)
    }

    fn activity_from_enables(&self, enables: Vec<BoolFormula>) -> DriveActivity {
        match enables.len() {
            0 => DriveActivity::Inactive,
            1 => DriveActivity::Active(enables.into_iter().next()),
            _ => DriveActivity::Active(Some(BoolFormula::Or(enables))),
        }
    }

    fn is_active_value(&self, expr: &EirExpr) -> bool {
        !matches!(self.classify_expr(expr), DriveActivity::Inactive)
    }
}

#[derive(Clone)]
enum BoolFormula {
    True,
    False,
    Atom(String),
    Not(Box<BoolFormula>),
    And(Vec<BoolFormula>),
    Or(Vec<BoolFormula>),
}

impl BoolFormula {
    fn from_expr(expr: &EirExpr) -> Self {
        match expr {
            EirExpr::Bool(true) => Self::True,
            EirExpr::Bool(false) => Self::False,
            EirExpr::Unary {
                op: EirUnaryOp::Not,
                expr,
            } => Self::from_expr(expr).negated(),
            EirExpr::Binary {
                op: EirBinaryOp::AndAnd,
                left,
                right,
            } => Self::And(vec![Self::from_expr(left), Self::from_expr(right)]),
            EirExpr::Binary {
                op: EirBinaryOp::OrOr,
                left,
                right,
            } => Self::Or(vec![Self::from_expr(left), Self::from_expr(right)]),
            _ => Self::Atom(expr.fact_key()),
        }
    }

    fn negated(self) -> Self {
        match self {
            Self::True => Self::False,
            Self::False => Self::True,
            Self::Atom(_) => Self::Not(Box::new(self)),
            Self::Not(inner) => *inner,
            Self::And(items) => Self::Or(items.into_iter().map(Self::negated).collect()),
            Self::Or(items) => Self::And(items.into_iter().map(Self::negated).collect()),
        }
    }
}

struct BoolProof<'a> {
    left: &'a BoolFormula,
    right: &'a BoolFormula,
}

impl<'a> BoolProof<'a> {
    fn new(left: &'a BoolFormula, right: &'a BoolFormula) -> Self {
        Self { left, right }
    }

    fn is_mutually_exclusive(&self) -> bool {
        let Some(left_terms) = Dnf::from_formula(self.left).terms else {
            return false;
        };
        let Some(right_terms) = Dnf::from_formula(self.right).terms else {
            return false;
        };
        left_terms
            .iter()
            .all(|left| right_terms.iter().all(|right| left.contradicts(right)))
    }
}

struct Dnf {
    terms: Option<Vec<BoolTerm>>,
}

impl Dnf {
    const MAX_TERMS: usize = 64;

    fn from_formula(formula: &BoolFormula) -> Self {
        Self {
            terms: Self::terms(formula),
        }
    }

    fn terms(formula: &BoolFormula) -> Option<Vec<BoolTerm>> {
        match formula {
            BoolFormula::True => Some(vec![BoolTerm::new()]),
            BoolFormula::False => Some(Vec::new()),
            BoolFormula::Atom(name) => Some(vec![BoolTerm::from_literal(name.clone(), true)]),
            BoolFormula::Not(inner) => match inner.as_ref() {
                BoolFormula::Atom(name) => Some(vec![BoolTerm::from_literal(name.clone(), false)]),
                _ => None,
            },
            BoolFormula::And(items) => {
                let mut product = vec![BoolTerm::new()];
                for item in items {
                    product = Self::and_terms(product, Self::terms(item)?)?;
                }
                Some(product)
            }
            BoolFormula::Or(items) => {
                let mut terms = Vec::new();
                for item in items {
                    terms.extend(Self::terms(item)?);
                    if terms.len() > Self::MAX_TERMS {
                        return None;
                    }
                }
                Some(terms)
            }
        }
    }

    fn and_terms(left: Vec<BoolTerm>, right: Vec<BoolTerm>) -> Option<Vec<BoolTerm>> {
        let mut out = Vec::new();
        for left_term in &left {
            for right_term in &right {
                if let Some(term) = left_term.merged(right_term) {
                    out.push(term);
                    if out.len() > Self::MAX_TERMS {
                        return None;
                    }
                }
            }
        }
        Some(out)
    }
}

#[derive(Clone)]
struct BoolTerm {
    literals: BTreeMap<String, bool>,
}

impl BoolTerm {
    fn new() -> Self {
        Self {
            literals: BTreeMap::new(),
        }
    }

    fn from_literal(name: String, value: bool) -> Self {
        let mut term = Self::new();
        term.literals.insert(name, value);
        term
    }

    fn merged(&self, other: &Self) -> Option<Self> {
        let mut merged = self.literals.clone();
        for (name, value) in &other.literals {
            if merged.get(name).is_some_and(|existing| existing != value) {
                return None;
            }
            merged.insert(name.clone(), *value);
        }
        Some(Self { literals: merged })
    }

    fn contradicts(&self, other: &Self) -> bool {
        self.literals
            .iter()
            .any(|(name, value)| other.literals.get(name).is_some_and(|other| other != value))
    }
}
