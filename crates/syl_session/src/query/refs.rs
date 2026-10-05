//! Names referenced by a signature stub.
//!
//! Generic parameters and value parameters shadow a const. A cell port is
//! not a value parameter: it has a direction and does not hide `const WIDTH`
//! in `UInt<WIDTH>`. Field, result, and variant names do not either.
//! Declaration names themselves are not reads.

use std::collections::BTreeSet;

use syl_syntax::{
    Attribute, Block, CallArg, EnumVariant, Expr, FieldDecl, GenericParam, Item, MatchArm,
    NamedExpr, Param, Pattern, PortDecl, RegReset, ResultBinding, SelectArm, Stmt, TypeExpr,
};

/// Names a signature stub refers to. Generic parameters and undirected value
/// parameters are skipped. Fields, results, variants, and directed ports do
/// not hide a const used in a type.
pub(super) fn signature_references(item: &Item) -> Vec<String> {
    let binders = signature_binders(item);
    let mut refs = Refs {
        binders: &binders,
        names: Vec::new(),
    };
    refs.item(item);
    refs.names
}

fn signature_binders(item: &Item) -> BTreeSet<String> {
    let mut binders = BTreeSet::new();
    match item {
        Item::Const(item) => {
            binders.insert(item.name.clone());
        }
        Item::Fn(item) => {
            binders.insert(item.name.clone());
            bind_params(&item.params, &mut binders);
        }
        Item::Enum(item) => {
            binders.insert(item.name.clone());
        }
        Item::Struct(item) => {
            binders.insert(item.name.clone());
            bind_generics(&item.generics, &mut binders);
        }
        Item::Bundle(item) => {
            binders.insert(item.name.clone());
            bind_generics(&item.generics, &mut binders);
        }
        Item::Interface(item) => {
            binders.insert(item.name.clone());
            bind_generics(&item.generics, &mut binders);
        }
        Item::Map(item) => {
            binders.insert(item.name.clone());
            bind_generics(&item.generics, &mut binders);
            bind_params(&item.params, &mut binders);
        }
        Item::Cell(item) => {
            binders.insert(item.name.clone());
            bind_generics(&item.generics, &mut binders);
            bind_params(&item.params, &mut binders);
        }
        Item::ExternCell(item) => {
            binders.insert(item.name.clone());
            bind_generics(&item.generics, &mut binders);
            bind_params(&item.params, &mut binders);
        }
        Item::Use(_) | Item::Error(_) => {}
        _ => {}
    }
    binders
}

fn bind_generics(generics: &[GenericParam], binders: &mut BTreeSet<String>) {
    for generic in generics {
        binders.insert(generic.name.clone());
    }
}

fn bind_params(params: &[Param], binders: &mut BTreeSet<String>) {
    for param in params {
        // A direction marks a cell port. Only undirected value parameters
        // shadow a const of the same name.
        if param.dir.is_none() {
            binders.insert(param.name.clone());
        }
    }
}

struct Refs<'a> {
    binders: &'a BTreeSet<String>,
    names: Vec<String>,
}

impl Refs<'_> {
    fn add(&mut self, name: &str) {
        if !self.binders.contains(name) {
            self.names.push(name.to_string());
        }
    }

    fn item(&mut self, item: &Item) {
        match item {
            Item::Const(item) => {
                self.ty_opt(item.ty.as_ref());
                self.expr(&item.value);
            }
            Item::Fn(item) => {
                self.params(&item.params);
                self.ty_opt(item.ret_ty.as_ref());
                self.block(&item.body);
            }
            Item::Enum(item) => {
                self.ty_opt(item.width.as_ref());
                for variant in &item.variants {
                    self.variant(variant);
                }
            }
            Item::Struct(item) => {
                self.generics(&item.generics);
                self.fields(&item.fields);
            }
            Item::Bundle(item) => {
                self.generics(&item.generics);
                self.fields(&item.fields);
                self.attrs(&item.attrs);
            }
            Item::Interface(item) => {
                self.generics(&item.generics);
                self.fields(&item.fields);
            }
            Item::Map(item) => {
                self.generics(&item.generics);
                self.params(&item.params);
                self.ty_opt(item.ret_ty.as_ref());
                self.expr(&item.body);
            }
            Item::Cell(item) => {
                self.generics(&item.generics);
                self.params(&item.params);
                self.ports(&item.ports);
                self.result(item.result.as_ref());
                self.block(&item.body);
            }
            Item::ExternCell(item) => {
                self.generics(&item.generics);
                self.params(&item.params);
                self.ports(&item.ports);
                self.result(item.result.as_ref());
            }
            Item::Use(_) | Item::Error(_) => {}
            _ => {}
        }
    }

    fn generics(&mut self, generics: &[GenericParam]) {
        for generic in generics {
            self.ty_opt(generic.kind.as_ref());
            if let Some(default) = &generic.default {
                self.expr(default);
            }
        }
    }

    fn params(&mut self, params: &[Param]) {
        for param in params {
            self.ty(&param.ty);
        }
    }

    fn fields(&mut self, fields: &[FieldDecl]) {
        for field in fields {
            self.ty(&field.ty);
        }
    }

    fn ports(&mut self, ports: &[PortDecl]) {
        for port in ports {
            self.ty(&port.ty);
        }
    }

    fn result(&mut self, result: Option<&ResultBinding>) {
        if let Some(result) = result {
            self.ty(&result.ty);
        }
    }

    fn variant(&mut self, variant: &EnumVariant) {
        if let Some(value) = &variant.value {
            self.expr(value);
        }
    }

    fn attrs(&mut self, attrs: &[Attribute]) {
        for attr in attrs {
            self.add(&attr.name);
            for arg in &attr.args {
                self.expr(arg);
            }
        }
    }

    fn block(&mut self, block: &Block) {
        for stmt in &block.stmts {
            self.stmt(stmt);
        }
        if let Some(tail) = &block.tail {
            self.expr(tail);
        }
    }

    fn stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Const { ty, value, .. } => {
                self.ty_opt(ty.as_ref());
                self.expr(value);
            }
            Stmt::Let { ty, value, .. }
            | Stmt::Var { ty, value, .. }
            | Stmt::Signal { ty, value, .. } => {
                self.ty_opt(ty.as_ref());
                if let Some(value) = value {
                    self.expr(value);
                }
            }
            Stmt::Reg { ty, reset, .. } => {
                self.ty_opt(ty.as_ref());
                if let Some(reset) = reset {
                    self.reset(reset);
                }
            }
            Stmt::Assign { target, value, .. } | Stmt::Drive { target, value, .. } => {
                self.expr(target);
                self.expr(value);
            }
            Stmt::Next { value, .. } => self.expr(value),
            Stmt::While { cond, body, .. } => {
                self.expr(cond);
                self.block(body);
            }
            Stmt::ElabIf {
                cond,
                then_block,
                else_block,
                ..
            } => {
                self.expr(cond);
                self.block(then_block);
                if let Some(else_block) = else_block {
                    self.block(else_block);
                }
            }
            Stmt::ElabFor { range, body, .. } => {
                self.expr(range);
                self.block(body);
            }
            Stmt::Expr(expr) => self.expr(expr),
            Stmt::Return(Some(value), _) => self.expr(value),
            Stmt::Return(None, _) => {}
            Stmt::Error { .. } => {}
            _ => {}
        }
    }

    fn reset(&mut self, reset: &RegReset) {
        if let Some(domain) = &reset.domain {
            self.expr(domain);
        }
        self.expr(&reset.value);
    }

    fn ty_opt(&mut self, ty: Option<&TypeExpr>) {
        if let Some(ty) = ty {
            self.ty(ty);
        }
    }

    fn ty(&mut self, ty: &TypeExpr) {
        match ty {
            TypeExpr::Path(segments, _) => {
                for segment in segments {
                    self.add(segment);
                }
            }
            TypeExpr::Array { len, elem, .. } => {
                self.expr(len);
                self.ty(elem);
            }
            TypeExpr::Generic { base, args, .. } => {
                self.ty(base);
                for arg in args {
                    self.ty(arg);
                }
            }
            TypeExpr::ViewSelect { base, .. } => self.ty(base),
            _ => {}
        }
    }

    fn expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Ident(name, _) => self.add(name),
            Expr::Unary { expr, .. } => self.expr(expr),
            Expr::Binary { left, right, .. } => {
                self.expr(left);
                self.expr(right);
            }
            Expr::Call { callee, args, .. } | Expr::Place { callee, args, .. } => {
                self.expr(callee);
                self.call_args(args);
            }
            Expr::GenericApp { callee, args, .. } => {
                self.expr(callee);
                for arg in args {
                    self.ty(arg);
                }
            }
            Expr::Aggregate { ty, fields, .. } => {
                self.ty(ty);
                for field in fields {
                    self.named_expr(field);
                }
            }
            Expr::Field { base, .. } => self.expr(base),
            Expr::Index { base, index, .. } => {
                self.expr(base);
                self.expr(index);
            }
            Expr::Group(expr, _) => self.expr(expr),
            Expr::Block(block) => self.block(block),
            Expr::Match { expr, arms, .. } => {
                self.expr(expr);
                for arm in arms {
                    self.match_arm(arm);
                }
            }
            Expr::Select { arms, .. } => {
                for arm in arms {
                    self.select_arm(arm);
                }
            }
            Expr::For { range, body, .. } => {
                self.expr(range);
                self.block(body);
            }
            Expr::CompileError { message, .. } => self.expr(message),
            Expr::Range { start, end, .. } => {
                self.expr(start);
                self.expr(end);
            }
            Expr::Int(_, _) | Expr::Str(_, _) | Expr::Bool(_, _) => {}
            _ => {}
        }
    }

    fn call_args(&mut self, args: &[CallArg]) {
        for arg in args {
            self.expr(&arg.value);
        }
    }

    fn named_expr(&mut self, field: &NamedExpr) {
        self.expr(&field.value);
    }

    fn match_arm(&mut self, arm: &MatchArm) {
        self.pattern(&arm.pattern);
        self.expr(&arm.value);
    }

    fn select_arm(&mut self, arm: &SelectArm) {
        self.expr(&arm.pattern);
        self.expr(&arm.value);
    }

    fn pattern(&mut self, pattern: &Pattern) {
        match pattern {
            Pattern::Path(segments, _) => {
                for segment in segments {
                    self.add(segment);
                }
            }
            Pattern::Wildcard(_)
            | Pattern::Ident(_, _)
            | Pattern::Int(_, _)
            | Pattern::Bool(_, _) => {}
            _ => {}
        }
    }
}
