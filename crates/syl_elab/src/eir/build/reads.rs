use crate::{
    eir::{EirExpr, EirPlace},
    program::{ElabExpr, ElabExprNode, ElabLocalKind, ElabResolution, ElabStmt},
};
use std::collections::BTreeMap;

use super::{EirBuilder, Env};

impl<'a, C> EirBuilder<'a, C>
where
    C: crate::const_eval::ConstValueElaborator + ?Sized,
{
    pub(super) fn elab_read_places(&self, expr: &ElabExpr, env: &Env) -> Vec<EirExpr> {
        let mut reads = BTreeMap::new();
        self.collect_elab_read_places(expr, env, &mut reads);
        reads.into_values().collect()
    }

    fn collect_elab_read_places(
        &self,
        expr: &ElabExpr,
        env: &Env,
        reads: &mut BTreeMap<String, EirExpr>,
    ) {
        match &expr.node {
            ElabExprNode::Ident(_) | ElabExprNode::Field { .. } | ElabExprNode::Index { .. } => {
                if self.is_hardware_read_place(expr, env) {
                    self.collect_eir_value_read_places(&self.elab_expr(expr, env), env, reads);
                }
                self.collect_elab_read_children(expr, env, reads);
            }
            _ => self.collect_elab_read_children(expr, env, reads),
        }
    }

    fn is_hardware_read_place(&self, expr: &ElabExpr, env: &Env) -> bool {
        let Some(root) = self.read_place_root(expr) else {
            return false;
        };
        let ElabExprNode::Ident(name) = &root.node else {
            return false;
        };
        if env.owner.is_none() {
            return env.var(name).is_some_and(|var| !var.software_local);
        }
        if env.var(name).is_some_and(|var| var.software_local) {
            return false;
        }
        let Some(kind) = self.read_root_kind(root, env) else {
            return false;
        };
        matches!(
            kind,
            ElabLocalKind::Param
                | ElabLocalKind::Result
                | ElabLocalKind::Let
                | ElabLocalKind::Signal
                | ElabLocalKind::Mem
                | ElabLocalKind::Reg
                | ElabLocalKind::Instance
        )
    }

    fn read_root_kind(&self, root: &ElabExpr, env: &Env) -> Option<ElabLocalKind> {
        let owner = env.owner?;
        let resolution = self.program.expr_resolution(owner, root)?;
        let ElabResolution::Local(local) = resolution else {
            return None;
        };
        self.program.local_kind(local)
    }

    fn read_place_root<'b>(&self, expr: &'b ElabExpr) -> Option<&'b ElabExpr> {
        let mut current = expr;
        loop {
            match &current.node {
                ElabExprNode::Ident(_) => return Some(current),
                ElabExprNode::Field { base, .. } | ElabExprNode::Index { base, .. } => {
                    current = base;
                }
                ElabExprNode::Group(inner) => current = inner,
                _ => return None,
            }
        }
    }

    fn collect_elab_read_children(
        &self,
        expr: &ElabExpr,
        env: &Env,
        reads: &mut BTreeMap<String, EirExpr>,
    ) {
        match &expr.node {
            ElabExprNode::Unary { expr, .. }
            | ElabExprNode::Group(expr)
            | ElabExprNode::GenericApp { callee: expr, .. } => {
                self.collect_elab_read_places(expr, env, reads);
            }
            ElabExprNode::Binary { left, right, .. } => {
                self.collect_elab_read_places(left, env, reads);
                self.collect_elab_read_places(right, env, reads);
            }
            ElabExprNode::Call { callee, args } => {
                if self.map_callee_from_elab(callee, env).is_some() {
                    let value = self.map_call_expr_from_elab(callee, args, env);
                    self.collect_eir_value_read_places(&value, env, reads);
                    return;
                }
                if let Some(value) = self.extension_map_call_expr(callee, args, env) {
                    self.collect_eir_value_read_places(&value, env, reads);
                    return;
                }
                self.collect_elab_read_places(callee, env, reads);
                for arg in args {
                    self.collect_elab_read_places(&arg.value, env, reads);
                }
            }
            ElabExprNode::Place { callee, args, .. } => {
                self.collect_elab_read_places(callee, env, reads);
                for arg in args {
                    self.collect_elab_read_places(&arg.value, env, reads);
                }
            }
            ElabExprNode::Aggregate { fields, .. } => {
                for field in fields {
                    self.collect_elab_read_places(&field.value, env, reads);
                }
            }
            ElabExprNode::Match { expr, arms } => {
                self.collect_elab_read_places(expr, env, reads);
                for arm in arms {
                    self.collect_elab_read_places(&arm.value, env, reads);
                }
            }
            ElabExprNode::Select { arms, .. } => {
                for arm in arms {
                    self.collect_elab_read_places(&arm.pattern, env, reads);
                    self.collect_elab_read_places(&arm.value, env, reads);
                }
            }
            ElabExprNode::CompileError { message } => {
                self.collect_elab_read_places(message, env, reads);
            }
            ElabExprNode::For { range, body, .. } => {
                self.collect_elab_read_places(range, env, reads);
                if let Some(tail) = body.tail.as_deref() {
                    self.collect_elab_read_places(tail, env, reads);
                }
                for stmt in &body.stmts {
                    match stmt {
                        ElabStmt::Expr(expr) => self.collect_elab_read_places(expr, env, reads),
                        ElabStmt::Drive { value, .. } => {
                            self.collect_elab_read_places(value, env, reads);
                        }
                        _ => {}
                    }
                }
            }
            ElabExprNode::Ident(_)
            | ElabExprNode::Int(_)
            | ElabExprNode::Str(_)
            | ElabExprNode::Bool(_)
            | ElabExprNode::Field { .. }
            | ElabExprNode::Index { .. }
            | ElabExprNode::Block(_)
            | ElabExprNode::Range { .. }
            | ElabExprNode::Unsupported => {}
        }
    }

    fn collect_eir_value_read_places(
        &self,
        expr: &EirExpr,
        env: &Env,
        reads: &mut BTreeMap<String, EirExpr>,
    ) {
        if EirPlace::try_from(expr).is_ok() {
            let mut root = expr;
            while let EirExpr::Index { value, .. }
            | EirExpr::Slice { value, .. }
            | EirExpr::IndexedPartSelect { value, .. } = root
            {
                root = value;
            }
            // Let bindings inline their value. Recognize the expanded memory
            // root so unpacked reads never use packed-vector bounds checks.
            let read = if env.vars.values().any(|v| v.is_memory && &v.code == root) {
                root
            } else {
                expr
            };
            reads.entry(read.fact_key()).or_insert_with(|| read.clone());
            self.collect_eir_projection_indices(expr, env, reads);
            return;
        }
        match expr {
            EirExpr::Unary { expr, .. } => self.collect_eir_value_read_places(expr, env, reads),
            EirExpr::Binary { left, right, .. } => {
                self.collect_eir_value_read_places(left, env, reads);
                self.collect_eir_value_read_places(right, env, reads);
            }
            EirExpr::Mux {
                cond,
                then_value,
                else_value,
            } => {
                self.collect_eir_value_read_places(cond, env, reads);
                self.collect_eir_value_read_places(then_value, env, reads);
                self.collect_eir_value_read_places(else_value, env, reads);
            }
            EirExpr::Select { arms, default, .. } => {
                for arm in arms {
                    self.collect_eir_value_read_places(arm.guard(), env, reads);
                    self.collect_eir_value_read_places(arm.value(), env, reads);
                }
                self.collect_eir_value_read_places(default, env, reads);
            }
            EirExpr::Concat(parts) => {
                for part in parts {
                    self.collect_eir_value_read_places(part, env, reads);
                }
            }
            EirExpr::Slice { value, .. } => {
                self.collect_eir_value_read_places(value, env, reads);
            }
            EirExpr::IndexedPartSelect { value, index, .. } => {
                self.collect_eir_value_read_places(value, env, reads);
                self.collect_eir_value_read_places(index, env, reads);
            }
            EirExpr::Index { value, index } => {
                self.collect_eir_value_read_places(value, env, reads);
                self.collect_eir_value_read_places(index, env, reads);
            }
            EirExpr::Call { args, .. } => {
                for arg in args {
                    self.collect_eir_value_read_places(arg, env, reads);
                }
            }
            EirExpr::Ident(_)
            | EirExpr::Int(_)
            | EirExpr::Bool(_)
            | EirExpr::Str(_)
            | EirExpr::HighZ
            | EirExpr::Zero
            | EirExpr::Unsupported { .. } => {}
        }
    }

    fn collect_eir_projection_indices(
        &self,
        expr: &EirExpr,
        env: &Env,
        reads: &mut BTreeMap<String, EirExpr>,
    ) {
        match expr {
            EirExpr::Index { value, index } | EirExpr::IndexedPartSelect { value, index, .. } => {
                self.collect_eir_projection_indices(value, env, reads);
                self.collect_eir_value_read_places(index, env, reads);
            }
            EirExpr::Slice { value, .. } => self.collect_eir_projection_indices(value, env, reads),
            _ => {}
        }
    }
}
