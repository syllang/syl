use super::TypePhaseChecker;
use crate::{
    hir::resolve::HirResolution,
    hir::view::HirDesignViewExt,
    hir::{
        HirBodyExpr, HirCallArg, HirConstItem, HirExprNode, HirFnItem, HirSignatureGenericParam,
    },
};
use std::collections::BTreeMap;
use syl_hir::{DefId, LocalId};
use syl_syntax::BinaryOp;

#[derive(Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub(super) enum TirConstKind {
    Nat,
    Bool,
}

#[derive(Clone)]
#[non_exhaustive]
pub(super) struct TirConstEnv {
    owner: DefId,
    bindings: BTreeMap<LocalId, TirConstBinding>,
}

#[derive(Clone, Copy)]
struct TirConstBinding {
    kind: TirConstKind,
    value: Option<TirConstValue>,
    mutable: bool,
}

impl TirConstBinding {
    fn immutable(kind: TirConstKind, value: Option<TirConstValue>) -> Self {
        Self {
            kind,
            value,
            mutable: false,
        }
    }

    fn mutable(kind: TirConstKind, value: Option<TirConstValue>) -> Self {
        Self {
            kind,
            value,
            mutable: true,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum TirConstValue {
    Nat(u64),
    Bool(bool),
}

impl TirConstEnv {
    pub(super) fn from_generics(
        owner: DefId,
        generics: &[HirSignatureGenericParam],
        checker: &TypePhaseChecker,
    ) -> Self {
        let mut env = Self {
            owner,
            bindings: BTreeMap::new(),
        };
        for generic in generics {
            if let Some(kind) = generic
                .kind
                .as_ref()
                .and_then(|ty| checker.mir_type_kind(ty))
                && let Some(id) = generic.id
            {
                env.bindings.insert(id, TirConstBinding::immutable(kind, None));
            }
        }
        env
    }

    pub(super) fn with_local_binding(
        &self,
        id: LocalId,
        kind: TirConstKind,
        value: Option<TirConstValue>,
    ) -> Self {
        let mut env = self.clone();
        env.bindings
            .insert(id, TirConstBinding::immutable(kind, value));
        env
    }

    pub(super) fn with_mutable_local(
        &self,
        id: LocalId,
        kind: TirConstKind,
        value: Option<TirConstValue>,
    ) -> Self {
        let mut env = self.clone();
        env.bindings.insert(id, TirConstBinding::mutable(kind, value));
        env
    }

    pub(super) fn assign_local(
        &self,
        id: LocalId,
        value: Option<TirConstValue>,
    ) -> Option<Self> {
        let mut env = self.clone();
        let binding = env.bindings.get_mut(&id)?;
        if !binding.mutable {
            return None;
        }
        binding.value = value;
        Some(env)
    }

    pub(super) fn kind_for_local(&self, id: LocalId) -> Option<TirConstKind> {
        self.bindings.get(&id).map(|binding| binding.kind)
    }

    pub(super) fn is_mutable_local(&self, id: LocalId) -> bool {
        self.bindings.get(&id).is_some_and(|binding| binding.mutable)
    }

    pub(super) fn apply_visible_mutations_from(&self, nested: &Self) -> Self {
        let mut env = self.clone();
        for (id, binding) in &mut env.bindings {
            if !binding.mutable {
                continue;
            }
            if let Some(updated) = nested.bindings.get(id) {
                binding.value = updated.value;
            }
        }
        env
    }

    pub(super) fn merge_visible_mutations_from(&self, nested: &Self) -> Self {
        let mut env = self.clone();
        for (id, binding) in &mut env.bindings {
            if !binding.mutable {
                continue;
            }
            if let Some(updated) = nested.bindings.get(id) {
                binding.value = (updated.value == binding.value).then_some(updated.value).flatten();
            }
        }
        env
    }

    pub(super) fn merge_branch_mutations(&self, then_env: &Self, else_env: &Self) -> Self {
        let mut env = self.clone();
        for (id, binding) in &mut env.bindings {
            if !binding.mutable {
                continue;
            }
            let then_value = then_env.bindings.get(id).map(|entry| entry.value);
            let else_value = else_env.bindings.get(id).map(|entry| entry.value);
            binding.value = if then_value == else_value {
                then_value.flatten()
            } else {
                None
            };
        }
        env
    }

    pub(super) fn expr_kind(
        &self,
        expr: &HirBodyExpr,
        checker: &TypePhaseChecker,
    ) -> Option<TirConstKind> {
        match &expr.node {
            HirExprNode::Ident(_) => self
                .local_binding(expr, checker)
                .map(|binding| binding.kind)
                .or_else(|| self.const_kind(expr, checker)),
            HirExprNode::Int(_) => Some(TirConstKind::Nat),
            HirExprNode::Bool(_) => Some(TirConstKind::Bool),
            HirExprNode::Group(inner) => self.expr_kind(inner, checker),
            HirExprNode::Field { base, field } => self
                .field_value_expr(base, field, checker)
                .and_then(|value| self.expr_kind(value, checker)),
            HirExprNode::Unary { op, expr } => {
                let kind = self.expr_kind(expr, checker)?;
                match (op, kind) {
                    (syl_syntax::UnaryOp::Not, TirConstKind::Bool) => Some(TirConstKind::Bool),
                    _ => None,
                }
            }
            HirExprNode::Binary {
                op, left, right, ..
            } => self.binary_kind(*op, left, right, checker),
            HirExprNode::Call { callee, args } => self.call_kind(callee, args, checker),
            HirExprNode::GenericApp { callee, .. } => self.expr_kind(callee, checker),
            HirExprNode::Range { .. }
            | HirExprNode::Str(_)
            | HirExprNode::Index { .. }
            | HirExprNode::Place { .. }
            | HirExprNode::For { .. }
            | HirExprNode::Aggregate { .. }
            | HirExprNode::Match { .. }
            | HirExprNode::Select { .. }
            | HirExprNode::CompileError { .. }
            | HirExprNode::Block(_)
            | HirExprNode::Unsupported => None,
            _ => None,
        }
    }

    pub(super) fn const_bool_value(
        &self,
        expr: &HirBodyExpr,
        checker: &TypePhaseChecker,
    ) -> Option<bool> {
        match &expr.node {
            HirExprNode::Bool(value) => Some(*value),
            HirExprNode::Ident(_) => {
                if let Some(binding) = self.local_binding(expr, checker) {
                    return match binding.value {
                        Some(TirConstValue::Bool(value)) => Some(value),
                        Some(TirConstValue::Nat(_)) | None => self
                            .const_item(expr, checker)
                            .and_then(|item| self.const_bool_value(&item.value, checker)),
                    };
                }
                self.const_item(expr, checker)
                    .and_then(|item| self.const_bool_value(&item.value, checker))
            }
            HirExprNode::Group(expr) => self.const_bool_value(expr, checker),
            HirExprNode::Field { base, field } => self
                .field_value_expr(base, field, checker)
                .and_then(|value| self.const_bool_value(value, checker)),
            HirExprNode::Unary {
                op: syl_syntax::UnaryOp::Not,
                expr,
            } => self.const_bool_value(expr, checker).map(|value| !value),
            HirExprNode::Binary {
                op, left, right, ..
            } => self.const_binary_bool_value(*op, left, right, checker),
            _ => None,
        }
    }

    fn const_binary_bool_value(
        &self,
        op: BinaryOp,
        left: &HirBodyExpr,
        right: &HirBodyExpr,
        checker: &TypePhaseChecker,
    ) -> Option<bool> {
        match op {
            BinaryOp::EqEq => {
                if let (Some(lhs), Some(rhs)) = (
                    self.const_nat_value(left, checker),
                    self.const_nat_value(right, checker),
                ) {
                    return Some(lhs == rhs);
                }
                if let (Some(lhs), Some(rhs)) = (
                    self.const_bool_value(left, checker),
                    self.const_bool_value(right, checker),
                ) {
                    return Some(lhs == rhs);
                }
                None
            }
            BinaryOp::NotEq => {
                if let (Some(lhs), Some(rhs)) = (
                    self.const_nat_value(left, checker),
                    self.const_nat_value(right, checker),
                ) {
                    return Some(lhs != rhs);
                }
                if let (Some(lhs), Some(rhs)) = (
                    self.const_bool_value(left, checker),
                    self.const_bool_value(right, checker),
                ) {
                    return Some(lhs != rhs);
                }
                None
            }
            BinaryOp::Lt => {
                Some(self.const_nat_value(left, checker)? < self.const_nat_value(right, checker)?)
            }
            BinaryOp::LtEq => {
                Some(self.const_nat_value(left, checker)? <= self.const_nat_value(right, checker)?)
            }
            BinaryOp::Gt => {
                Some(self.const_nat_value(left, checker)? > self.const_nat_value(right, checker)?)
            }
            BinaryOp::GtEq => {
                Some(self.const_nat_value(left, checker)? >= self.const_nat_value(right, checker)?)
            }
            BinaryOp::AndAnd => Some(
                self.const_bool_value(left, checker)? && self.const_bool_value(right, checker)?,
            ),
            BinaryOp::OrOr => Some(
                self.const_bool_value(left, checker)? || self.const_bool_value(right, checker)?,
            ),
            _ => None,
        }
    }

    pub(super) fn const_range_bounds(
        &self,
        expr: &HirBodyExpr,
        checker: &TypePhaseChecker,
    ) -> Option<(u64, u64)> {
        let HirExprNode::Range { start, end } = &expr.node else {
            return None;
        };
        Some((
            self.const_nat_value(start, checker)?,
            self.const_nat_value(end, checker)?,
        ))
    }

    fn const_nat_value(&self, expr: &HirBodyExpr, checker: &TypePhaseChecker) -> Option<u64> {
        match &expr.node {
            HirExprNode::Int(value) => Some(*value),
            HirExprNode::Ident(_) => {
                if let Some(binding) = self.local_binding(expr, checker) {
                    return match binding.value {
                        Some(TirConstValue::Nat(value)) => Some(value),
                        Some(TirConstValue::Bool(_)) | None => self
                            .const_item(expr, checker)
                            .and_then(|item| self.const_nat_value(&item.value, checker)),
                    };
                }
                self.const_item(expr, checker)
                    .and_then(|item| self.const_nat_value(&item.value, checker))
            }
            HirExprNode::Group(expr) => self.const_nat_value(expr, checker),
            HirExprNode::Field { base, field } => self
                .field_value_expr(base, field, checker)
                .and_then(|value| self.const_nat_value(value, checker)),
            HirExprNode::Binary {
                op, left, right, ..
            } => {
                let lhs = self.const_nat_value(left, checker)?;
                let rhs = self.const_nat_value(right, checker)?;
                match op {
                    BinaryOp::Add => Some(lhs + rhs),
                    BinaryOp::Sub => Some(lhs.saturating_sub(rhs)),
                    BinaryOp::Mul => Some(lhs * rhs),
                    BinaryOp::Div if rhs != 0 => Some(lhs / rhs),
                    BinaryOp::Rem if rhs != 0 => Some(lhs % rhs),
                    BinaryOp::Shl => Some(lhs << rhs),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn binary_kind(
        &self,
        op: BinaryOp,
        left: &HirBodyExpr,
        right: &HirBodyExpr,
        checker: &TypePhaseChecker,
    ) -> Option<TirConstKind> {
        let lhs = self.expr_kind(left, checker)?;
        let rhs = self.expr_kind(right, checker)?;
        match op {
            BinaryOp::EqEq
            | BinaryOp::NotEq
            | BinaryOp::Lt
            | BinaryOp::LtEq
            | BinaryOp::Gt
            | BinaryOp::GtEq
                if lhs == rhs =>
            {
                Some(TirConstKind::Bool)
            }
            BinaryOp::AndAnd | BinaryOp::OrOr
                if lhs == TirConstKind::Bool && rhs == TirConstKind::Bool =>
            {
                Some(TirConstKind::Bool)
            }
            BinaryOp::Add
            | BinaryOp::Sub
            | BinaryOp::Mul
            | BinaryOp::Div
            | BinaryOp::Rem
            | BinaryOp::Shl
                if lhs == TirConstKind::Nat && rhs == TirConstKind::Nat =>
            {
                Some(TirConstKind::Nat)
            }
            _ => None,
        }
    }

    fn call_kind(
        &self,
        callee: &HirBodyExpr,
        args: &[HirCallArg],
        checker: &TypePhaseChecker,
    ) -> Option<TirConstKind> {
        let item = self.fn_item(callee, checker)?;
        for arg in args {
            self.expr_kind(&arg.value, checker)?;
        }
        item.ret_ty
            .as_ref()
            .and_then(|ty| checker.mir_type_kind(&ty.ty))
    }

    fn field_value_expr<'a>(
        &self,
        base: &'a HirBodyExpr,
        field: &str,
        checker: &'a TypePhaseChecker,
    ) -> Option<&'a HirBodyExpr> {
        match &base.node {
            HirExprNode::Group(inner) => self.field_value_expr(inner, field, checker),
            HirExprNode::Ident(name) => {
                if self.bindings.contains_key(name) {
                    return None;
                }
                self.const_item(base, checker)
                    .and_then(|item| self.field_value_expr(&item.value, field, checker))
            }
            HirExprNode::Aggregate { fields, .. } => fields
                .iter()
                .find(|named| named.name == field)
                .map(|named| &named.value),
            HirExprNode::Field { base, field: inner } => {
                let value = self.field_value_expr(base, inner, checker)?;
                self.field_value_expr(value, field, checker)
            }
            _ => None,
        }
    }

    fn const_kind(&self, expr: &HirBodyExpr, checker: &TypePhaseChecker) -> Option<TirConstKind> {
        self.const_item(expr, checker)
            .and_then(|item| item.ty.as_ref())
            .and_then(|ty| checker.mir_type_kind(ty))
    }

    pub(super) fn value_for_kind(
        &self,
        kind: TirConstKind,
        expr: &HirBodyExpr,
        checker: &TypePhaseChecker,
    ) -> Option<TirConstValue> {
        match kind {
            TirConstKind::Nat => self
                .const_nat_value(expr, checker)
                .map(TirConstValue::Nat),
            TirConstKind::Bool => self
                .const_bool_value(expr, checker)
                .map(TirConstValue::Bool),
        }
    }

    fn const_item<'a>(
        &self,
        expr: &HirBodyExpr,
        checker: &'a TypePhaseChecker,
    ) -> Option<&'a HirConstItem> {
        let def = self.def_for_expr(expr, checker)?;
        checker.hir.const_by_def(def)
    }

    fn fn_item<'a>(
        &self,
        callee: &HirBodyExpr,
        checker: &'a TypePhaseChecker,
    ) -> Option<&'a HirFnItem> {
        let root = self.callee_root(callee)?;
        let def = self.def_for_expr(root, checker)?;
        checker.hir.fns.get(&def)
    }

    fn def_for_expr(&self, expr: &HirBodyExpr, checker: &TypePhaseChecker) -> Option<DefId> {
        let Some(HirResolution::Def(def)) = checker.hir.expr_resolution(self.owner, expr).ok()?
        else {
            return None;
        };
        Some(def)
    }

    fn local_binding<'a>(
        &'a self,
        expr: &HirBodyExpr,
        checker: &TypePhaseChecker,
    ) -> Option<&'a TirConstBinding> {
        let Some(HirResolution::Local(id)) = checker.hir.expr_resolution(self.owner, expr).ok()?
        else {
            return None;
        };
        self.bindings.get(&id)
    }

    fn callee_root<'a>(&self, expr: &'a HirBodyExpr) -> Option<&'a HirBodyExpr> {
        let mut current = expr;
        loop {
            match &current.node {
                HirExprNode::Ident(_) => return Some(current),
                HirExprNode::GenericApp { callee, .. } | HirExprNode::Group(callee) => {
                    current = callee;
                }
                _ => return None,
            }
        }
    }
}
