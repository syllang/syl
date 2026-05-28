use super::{AstNodeIndexBuilder, NodeHandle, binary_op_label, select_mode_label, unary_op_label};
use crate::{
    Block, CallArg, Expr, MatchArm, NamedExpr, Pattern, RegReset, SelectArm, Stmt, TypeExpr,
};

impl<'a> AstNodeIndexBuilder<'a> {
    pub(super) fn visit_block(&mut self, block: &Block, parent: NodeHandle) {
        let id = self.push_kind(super::AstNodeKind::Block, block.span, Some(parent));
        for stmt in &block.stmts {
            self.visit_stmt(stmt, id);
        }
        if let Some(tail) = block.tail.as_deref() {
            self.visit_expr(tail, id);
        }
    }

    pub(super) fn visit_stmt(&mut self, stmt: &Stmt, parent: NodeHandle) {
        match stmt {
            Stmt::Error { span } => {
                self.push_kind(super::AstNodeKind::ErrorStmt, *span, Some(parent));
            }
            Stmt::Const {
                name,
                ty,
                value,
                span,
            } => {
                let id = self.push_name(super::AstNodeKind::ConstStmt, *span, Some(parent), name);
                if let Some(ty) = ty {
                    self.visit_type_expr(ty, id);
                }
                self.visit_expr(value, id);
            }
            Stmt::Let {
                name,
                ty,
                value,
                span,
            } => {
                let id = self.push_name(super::AstNodeKind::LetStmt, *span, Some(parent), name);
                if let Some(ty) = ty {
                    self.visit_type_expr(ty, id);
                }
                if let Some(value) = value {
                    self.visit_expr(value, id);
                }
            }
            Stmt::Var {
                name,
                ty,
                value,
                span,
            } => {
                let id = self.push_name(super::AstNodeKind::VarStmt, *span, Some(parent), name);
                if let Some(ty) = ty {
                    self.visit_type_expr(ty, id);
                }
                if let Some(value) = value {
                    self.visit_expr(value, id);
                }
            }
            Stmt::Signal {
                name,
                ty,
                value,
                span,
            } => {
                let id = self.push_name(super::AstNodeKind::SignalStmt, *span, Some(parent), name);
                if let Some(ty) = ty {
                    self.visit_type_expr(ty, id);
                }
                if let Some(value) = value {
                    self.visit_expr(value, id);
                }
            }
            Stmt::Reg {
                name,
                ty,
                reset,
                span,
            } => {
                let id = self.push_name(super::AstNodeKind::RegStmt, *span, Some(parent), name);
                if let Some(ty) = ty {
                    self.visit_type_expr(ty, id);
                }
                if let Some(reset) = reset {
                    self.visit_reg_reset(reset, id);
                }
            }
            Stmt::Assign {
                target,
                value,
                span,
            } => {
                let id = self.push_kind(super::AstNodeKind::AssignStmt, *span, Some(parent));
                self.visit_expr(target, id);
                self.visit_expr(value, id);
            }
            Stmt::Drive {
                target,
                value,
                span,
            } => {
                let id = self.push_kind(super::AstNodeKind::DriveStmt, *span, Some(parent));
                self.visit_expr(target, id);
                self.visit_expr(value, id);
            }
            Stmt::Next { name, value, span } => {
                let id = self.push_name(super::AstNodeKind::NextStmt, *span, Some(parent), name);
                self.visit_expr(value, id);
            }
            Stmt::While { cond, body, span } => {
                let id = self.push_kind(super::AstNodeKind::WhileStmt, *span, Some(parent));
                self.visit_expr(cond, id);
                self.visit_block(body, id);
            }
            Stmt::ElabIf {
                cond,
                then_block,
                else_block,
                span,
            } => {
                let id = self.push_kind(super::AstNodeKind::ElabIfStmt, *span, Some(parent));
                self.visit_expr(cond, id);
                self.visit_block(then_block, id);
                if let Some(block) = else_block {
                    self.visit_block(block, id);
                }
            }
            Stmt::ElabFor {
                name,
                range,
                body,
                span,
            } => {
                let id = self.push_name(super::AstNodeKind::ElabForStmt, *span, Some(parent), name);
                self.visit_expr(range, id);
                self.visit_block(body, id);
            }
            Stmt::Expr(expr) => {
                let id = self.push_kind(super::AstNodeKind::ExprStmt, expr.span(), Some(parent));
                self.visit_expr(expr, id);
            }
            Stmt::Return(expr, span) => {
                let id = self.push_kind(super::AstNodeKind::ReturnStmt, *span, Some(parent));
                if let Some(expr) = expr {
                    self.visit_expr(expr, id);
                }
            }
        }
    }

    pub(super) fn visit_reg_reset(&mut self, item: &RegReset, parent: NodeHandle) {
        let id = self.push_kind(super::AstNodeKind::RegReset, item.span, Some(parent));
        if let Some(domain) = &item.domain {
            self.visit_expr(domain, id);
        }
        self.visit_expr(&item.value, id);
    }

    pub(super) fn visit_expr(&mut self, expr: &Expr, parent: NodeHandle) {
        match expr {
            Expr::Ident(name, span) => {
                self.push_name(super::AstNodeKind::IdentExpr, *span, Some(parent), name);
            }
            Expr::Int(value, span) => {
                self.push_int(super::AstNodeKind::IntExpr, *span, Some(parent), *value);
            }
            Expr::Str(_, span) => {
                self.push_text(super::AstNodeKind::StrExpr, *span, Some(parent));
            }
            Expr::Bool(value, span) => {
                self.push_bool(super::AstNodeKind::BoolExpr, *span, Some(parent), *value);
            }
            Expr::Unary { op, expr, span } => {
                let id = self.push_tag(
                    super::AstNodeKind::UnaryExpr,
                    *span,
                    Some(parent),
                    unary_op_label(*op),
                );
                self.visit_expr(expr, id);
            }
            Expr::Binary {
                op,
                left,
                right,
                span,
            } => {
                let id = self.push_tag(
                    super::AstNodeKind::BinaryExpr,
                    *span,
                    Some(parent),
                    binary_op_label(*op),
                );
                self.visit_expr(left, id);
                self.visit_expr(right, id);
            }
            Expr::Call { callee, args, span } => {
                let id = self.push_kind(super::AstNodeKind::CallExpr, *span, Some(parent));
                self.visit_expr(callee, id);
                for arg in args {
                    self.visit_call_arg(arg, id);
                }
            }
            Expr::GenericApp { callee, args, span } => {
                let id = self.push_kind(super::AstNodeKind::GenericAppExpr, *span, Some(parent));
                self.visit_expr(callee, id);
                for arg in args {
                    self.visit_type_expr(arg, id);
                }
            }
            Expr::Aggregate { ty, fields, span } => {
                let id = self.push_kind(super::AstNodeKind::AggregateExpr, *span, Some(parent));
                self.visit_type_expr(ty, id);
                for field in fields {
                    self.visit_named_expr(field, id);
                }
            }
            Expr::Field { base, field, span } => {
                let id = self.push_name(super::AstNodeKind::FieldExpr, *span, Some(parent), field);
                self.visit_expr(base, id);
            }
            Expr::Index { base, index, span } => {
                let id = self.push_kind(super::AstNodeKind::IndexExpr, *span, Some(parent));
                self.visit_expr(base, id);
                self.visit_expr(index, id);
            }
            Expr::Group(expr, span) => {
                let id = self.push_kind(super::AstNodeKind::GroupExpr, *span, Some(parent));
                self.visit_expr(expr, id);
            }
            Expr::Block(block) => {
                let id = self.push_kind(super::AstNodeKind::BlockExpr, block.span, Some(parent));
                self.visit_block(block, id);
            }
            Expr::Match { expr, arms, span } => {
                let id = self.push_kind(super::AstNodeKind::MatchExpr, *span, Some(parent));
                self.visit_expr(expr, id);
                for arm in arms {
                    self.visit_match_arm(arm, id);
                }
            }
            Expr::Select { mode, arms, span } => {
                let id = self.push_tag(
                    super::AstNodeKind::SelectExpr,
                    *span,
                    Some(parent),
                    select_mode_label(mode),
                );
                for arm in arms {
                    self.visit_select_arm(arm, id);
                }
            }
            Expr::Place {
                callee,
                args,
                span,
                inplace: _, // 暂不计入 node index
            } => {
                let id = self.push_kind(super::AstNodeKind::PlaceExpr, *span, Some(parent));
                self.visit_expr(callee, id);
                for arg in args {
                    self.visit_call_arg(arg, id);
                }
            }
            Expr::For {
                name,
                range,
                body,
                span,
            } => {
                let id = self.push_name(super::AstNodeKind::ForExpr, *span, Some(parent), name);
                self.visit_expr(range, id);
                self.visit_block(body, id);
            }
            Expr::CompileError { message, span } => {
                let id = self.push_kind(super::AstNodeKind::CompileErrorExpr, *span, Some(parent));
                self.visit_expr(message, id);
            }
            Expr::Range { start, end, span } => {
                let id = self.push_kind(super::AstNodeKind::RangeExpr, *span, Some(parent));
                self.visit_expr(start, id);
                self.visit_expr(end, id);
            }
        }
    }

    pub(super) fn visit_named_expr(&mut self, item: &NamedExpr, parent: NodeHandle) {
        let id = self.push_name(
            super::AstNodeKind::NamedExpr,
            item.span,
            Some(parent),
            &item.name,
        );
        self.visit_expr(&item.value, id);
    }

    pub(super) fn visit_call_arg(&mut self, item: &CallArg, parent: NodeHandle) {
        let id = match item.name.as_deref() {
            Some(name) => {
                self.push_name(super::AstNodeKind::CallArg, item.span, Some(parent), name)
            }
            None => self.push_tag(
                super::AstNodeKind::CallArg,
                item.span,
                Some(parent),
                "positional",
            ),
        };
        self.visit_expr(&item.value, id);
    }

    pub(super) fn visit_select_arm(&mut self, item: &SelectArm, parent: NodeHandle) {
        let id = self.push_kind(super::AstNodeKind::SelectArm, item.span, Some(parent));
        self.visit_expr(&item.pattern, id);
        self.visit_expr(&item.value, id);
    }

    pub(super) fn visit_match_arm(&mut self, item: &MatchArm, parent: NodeHandle) {
        let id = self.push_kind(super::AstNodeKind::MatchArm, item.span, Some(parent));
        self.visit_pattern(&item.pattern, id);
        self.visit_expr(&item.value, id);
    }

    pub(super) fn visit_pattern(&mut self, pattern: &Pattern, parent: NodeHandle) {
        match pattern {
            Pattern::Wildcard(span) => {
                self.push_kind(super::AstNodeKind::WildcardPattern, *span, Some(parent));
            }
            Pattern::Ident(name, span) => {
                self.push_name(super::AstNodeKind::IdentPattern, *span, Some(parent), name);
            }
            Pattern::Int(value, span) => {
                self.push_int(super::AstNodeKind::IntPattern, *span, Some(parent), *value);
            }
            Pattern::Bool(value, span) => {
                self.push_bool(super::AstNodeKind::BoolPattern, *span, Some(parent), *value);
            }
            Pattern::Path(path, span) => {
                self.push_path(super::AstNodeKind::PathPattern, *span, Some(parent), path);
            }
        }
    }

    pub(super) fn visit_type_expr(&mut self, ty: &TypeExpr, parent: NodeHandle) {
        match ty {
            TypeExpr::Path(path, span) => {
                self.push_path(super::AstNodeKind::PathType, *span, Some(parent), path);
            }
            TypeExpr::Array { len, elem, span } => {
                let id = self.push_kind(super::AstNodeKind::ArrayType, *span, Some(parent));
                self.visit_expr(len, id);
                self.visit_type_expr(elem, id);
            }
            TypeExpr::Generic { base, args, span } => {
                let id = self.push_kind(super::AstNodeKind::GenericType, *span, Some(parent));
                self.visit_type_expr(base, id);
                for arg in args {
                    self.visit_type_expr(arg, id);
                }
            }
            TypeExpr::ViewSelect { base, view, span } => {
                let id = self.push_name(
                    super::AstNodeKind::ViewSelectType,
                    *span,
                    Some(parent),
                    view,
                );
                self.visit_type_expr(base, id);
            }
        }
    }
}
