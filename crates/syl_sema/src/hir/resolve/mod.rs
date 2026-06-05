use crate::{
    CompileError, HirError,
    hir::view::HirDesignViewExt,
    hir::{
        HirBlock, HirBodyExpr, HirCallArg, HirCallable, HirCallableItem, HirConstItem, HirDefKind,
        HirDesign, HirEnumVariantKey, HirExprNode, HirExternCellItem, HirFnItem, HirInterfaceItem,
        HirMapItem, HirMatchArm, HirNamedExpr, HirSelectArm, HirSignatureGenericParam,
        HirSignatureParam, HirStmt,
    },
};
use std::collections::BTreeMap;
pub(crate) use syl_hir::resolution::HirResolution;
use syl_hir::{DefId, ExprId, LocalId, name::HirPath};
use syl_span::Span;

#[non_exhaustive]
pub(crate) struct HirNameResolver<'a> {
    design: &'a mut HirDesign,
    scopes: ScopeStack,
    errors: Vec<CompileError>,
    error_mode: HirNameErrorMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HirNameErrorMode {
    First,
    All,
}

impl<'a> HirNameResolver<'a> {
    pub(crate) fn new(design: &'a mut HirDesign) -> Self {
        Self::with_error_mode(design, HirNameErrorMode::First)
    }

    pub(crate) fn new_collect(design: &'a mut HirDesign) -> Self {
        Self::with_error_mode(design, HirNameErrorMode::All)
    }

    fn with_error_mode(design: &'a mut HirDesign, error_mode: HirNameErrorMode) -> Self {
        Self {
            design,
            scopes: ScopeStack::new(),
            errors: Vec::new(),
            error_mode,
        }
    }

    pub(crate) fn resolve(mut self) -> Result<(), CompileError> {
        self.resolve_names();
        match self.errors.into_iter().next() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    pub(crate) fn resolve_collect(mut self) -> Result<(), Vec<CompileError>> {
        self.resolve_names();
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(self.errors)
        }
    }

    fn resolve_names(&mut self) {
        let consts: Vec<_> = self
            .design
            .consts
            .iter()
            .map(|(owner, item)| (*owner, item.clone()))
            .collect();
        for (owner, item) in consts {
            self.resolve_const(owner, &item);
        }
        let fns: Vec<_> = self
            .design
            .fns
            .iter()
            .map(|(owner, item)| (*owner, item.clone()))
            .collect();
        for (owner, item) in fns {
            self.resolve_fn(owner, &item);
        }
        let enums: Vec<_> = self.design.enums.keys().copied().collect();
        for owner in enums {
            self.resolve_enum(owner);
        }
        let interfaces: Vec<_> = self
            .design
            .interfaces
            .iter()
            .map(|(owner, item)| (*owner, item.clone()))
            .collect();
        for (owner, item) in interfaces {
            self.resolve_interface(owner, &item);
        }
        let maps: Vec<_> = self
            .design
            .maps
            .iter()
            .map(|(owner, item)| (*owner, item.clone()))
            .collect();
        for (owner, item) in maps {
            self.resolve_map(owner, &item);
        }
        let callables: Vec<_> = self
            .design
            .callables
            .iter()
            .map(|(owner, callable)| (*owner, callable.clone()))
            .collect();
        for (owner, callable) in callables {
            self.resolve_callable_ref(owner, &callable);
        }
    }

    fn resolve_const(&mut self, owner: DefId, item: &HirConstItem) {
        self.with_scope(owner, |this| {
            this.resolve_expr(owner, &item.value);
        });
    }

    fn resolve_fn(&mut self, owner: DefId, item: &HirFnItem) {
        self.with_scope(owner, |this| {
            this.push_params(owner, &item.params);
            this.resolve_block(owner, &item.body);
        });
    }

    fn resolve_enum(&mut self, owner: DefId) {
        self.with_scope(owner, |_| {});
    }

    fn resolve_interface(&mut self, owner: DefId, item: &HirInterfaceItem) {
        self.with_scope(owner, |this| {
            this.push_generics(owner, &item.generics);
        });
    }

    fn resolve_map(&mut self, owner: DefId, item: &HirMapItem) {
        self.with_scope(owner, |this| {
            this.push_generics(owner, &item.generics);
            this.push_params(owner, &item.params);
            this.resolve_expr(owner, &item.body);
        });
    }

    fn resolve_callable_ref(&mut self, owner: DefId, callable: &HirCallable) {
        match callable {
            HirCallable::Cell(item) => {
                self.resolve_callable(owner, item);
            }
            HirCallable::Extern(item) => self.resolve_extern(owner, item),
            _ => {}
        }
    }

    fn resolve_callable(&mut self, owner: DefId, item: &HirCallableItem) {
        self.with_scope(owner, |this| {
            this.push_generics(owner, &item.generics);
            this.push_params(owner, &item.params);
            if let Some(result) = &item.result {
                this.push_local_id(&result.name, result.id);
            }
            this.resolve_block(owner, &item.body);
        });
    }

    fn resolve_extern(&mut self, owner: DefId, item: &HirExternCellItem) {
        self.with_scope(owner, |this| {
            this.push_generics(owner, &item.generics);
            this.push_params(owner, &item.params);
            if let Some(result) = &item.result {
                this.push_local_id(&result.name, result.id);
            }
        });
    }

    fn with_scope(&mut self, _owner: DefId, f: impl FnOnce(&mut Self)) {
        self.scopes.push();
        f(self);
        self.scopes.pop();
    }

    fn push_generics(&mut self, owner: DefId, generics: &[HirSignatureGenericParam]) {
        for generic in generics {
            if let Some(default) = &generic.default {
                self.resolve_expr(owner, default);
            }
            self.push_local_id(&generic.name, generic.id);
        }
    }

    fn push_params(&mut self, _owner: DefId, params: &[HirSignatureParam]) {
        for param in params {
            self.push_local_id(&param.name, param.id);
        }
    }

    fn push_local_id(&mut self, name: &str, id: Option<LocalId>) {
        if let Some(id) = id {
            self.scopes.insert(name, id);
        }
    }

    fn resolve_block(&mut self, owner: DefId, body: &HirBlock) {
        self.scopes.push();
        for stmt in &body.stmts {
            self.resolve_stmt(owner, stmt);
        }
        if let Some(tail) = &body.tail {
            self.resolve_expr(owner, tail);
        }
        self.scopes.pop();
    }

    fn resolve_stmt(&mut self, owner: DefId, stmt: &HirStmt) {
        match stmt {
            HirStmt::Const {
                id, name, value, ..
            }
            | HirStmt::Let {
                id,
                name,
                value: Some(value),
                ..
            }
            | HirStmt::Var {
                id,
                name,
                value: Some(value),
                ..
            }
            | HirStmt::Signal {
                id,
                name,
                value: Some(value),
                ..
            } => {
                self.resolve_expr(owner, value);
                self.push_local_id(name, *id);
            }
            HirStmt::Let {
                id,
                name,
                value: None,
                ..
            }
            | HirStmt::Var {
                id,
                name,
                value: None,
                ..
            }
            | HirStmt::Signal {
                id,
                name,
                value: None,
                ..
            }
            | HirStmt::Reg { id, name, .. } => {
                self.push_local_id(name, *id);
            }
            HirStmt::Assign { target, value, .. } | HirStmt::Drive { target, value, .. } => {
                self.resolve_expr(owner, target);
                self.resolve_expr(owner, value);
            }
            HirStmt::Next { value, .. } => self.resolve_expr(owner, value),
            HirStmt::While { cond, body, .. } => {
                self.resolve_expr(owner, cond);
                self.resolve_block(owner, body);
            }
            HirStmt::ElabIf {
                cond,
                then_block,
                else_block,
                ..
            } => {
                self.resolve_expr(owner, cond);
                self.resolve_block(owner, then_block);
                if let Some(block) = else_block {
                    self.resolve_block(owner, block);
                }
            }
            HirStmt::ElabFor { id, name, body, .. } => {
                self.scopes.push();
                self.push_local_id(name, *id);
                self.resolve_block(owner, body);
                self.scopes.pop();
            }
            HirStmt::Expr(expr) => self.resolve_expr(owner, expr),
            HirStmt::Return(Some(expr), _) => self.resolve_expr(owner, expr),
            HirStmt::Return(None, _) | HirStmt::Error { .. } => {}
            _ => {}
        }
    }

    fn resolve_expr(&mut self, owner: DefId, expr: &HirBodyExpr) {
        let id = self.record_expr_resolution(owner, expr);
        match &expr.node {
            HirExprNode::Ident(_)
            | HirExprNode::Int(_)
            | HirExprNode::Str(_)
            | HirExprNode::Bool(_)
            | HirExprNode::Unsupported => {
                let _id = id;
            }
            HirExprNode::Unary { expr, .. } | HirExprNode::Group(expr) => {
                self.resolve_expr(owner, expr);
            }
            HirExprNode::Binary { left, right, .. } => {
                self.resolve_expr(owner, left);
                self.resolve_expr(owner, right);
            }
            HirExprNode::Call { callee, args } => {
                self.resolve_call_callee(owner, callee);
                self.resolve_args(owner, args);
            }
            HirExprNode::Place { callee, args, .. } => {
                self.resolve_expr(owner, callee);
                self.resolve_args(owner, args);
            }
            HirExprNode::GenericApp { callee, .. } => self.resolve_expr(owner, callee),
            HirExprNode::Aggregate { fields, .. } => self.resolve_named_exprs(owner, fields),
            HirExprNode::Field { base, field } => {
                self.resolve_expr(owner, base);
                self.validate_enum_variant_expr(expr, base, field);
            }
            HirExprNode::Index { base, index } => {
                self.resolve_expr(owner, base);
                self.resolve_expr(owner, index);
            }
            HirExprNode::Block(block) => self.resolve_block(owner, block),
            HirExprNode::Match { expr, arms } => {
                self.resolve_expr(owner, expr);
                self.resolve_match_arms(owner, arms);
            }
            HirExprNode::Select { arms, .. } => self.resolve_select_arms(owner, arms),
            HirExprNode::CompileError { message } => self.resolve_expr(owner, message),
            HirExprNode::Range { start, end } => {
                self.resolve_expr(owner, start);
                self.resolve_expr(owner, end);
            }
            HirExprNode::For {
                id,
                name,
                range,
                body,
            } => {
                self.resolve_expr(owner, range);
                self.scopes.push();
                self.push_local_id(name, *id);
                self.resolve_block(owner, body);
                self.scopes.pop();
            }
            _ => {}
        }
    }

    fn record_expr_resolution(&mut self, owner: DefId, expr: &HirBodyExpr) -> Option<ExprId> {
        let id = expr.id();
        debug_assert!(
            self.design.exprs.get(id.get()).is_some_and(
                |registered| registered.owner == owner && registered.span == expr.span()
            )
        );
        let HirExprNode::Ident(name) = &expr.node else {
            return Some(id);
        };
        if let Some(local) = self.scopes.resolve(name) {
            self.design
                .register_expr_resolution(id, HirResolution::Local(local));
        } else if let Some(def) = self.resolve_visible_def(owner, name, expr.span()) {
            self.design
                .register_expr_resolution(id, HirResolution::Def(def));
        } else {
            self.record_unresolved_name(name, expr.span());
        }
        Some(id)
    }

    fn record_unresolved_name(&mut self, name: &str, span: Span) {
        if self.error_mode == HirNameErrorMode::First && !self.errors.is_empty() {
            return;
        }
        self.errors.push(CompileError::lowering_at(
            HirError::UnresolvedName {
                name: name.to_string(),
            },
            span,
        ));
    }

    fn resolve_call_callee(&mut self, owner: DefId, callee: &HirBodyExpr) {
        let Some(root) = BuiltinCallCallee::new(callee).root() else {
            self.resolve_expr(owner, callee);
            return;
        };
        let HirExprNode::Ident(name) = &root.node else {
            self.resolve_expr(owner, callee);
            return;
        };
        if self.scopes.resolve(name).is_some() || self.visible_def_exists(owner, name) {
            self.resolve_expr(owner, callee);
            return;
        }
        if BuiltinCallCallee::new(callee).intrinsic().is_some() {
            let _id = root.id();
            return;
        }
        self.resolve_expr(owner, callee);
    }

    fn resolve_args(&mut self, owner: DefId, args: &[HirCallArg]) {
        for arg in args {
            self.resolve_expr(owner, &arg.value);
        }
    }

    fn resolve_named_exprs(&mut self, owner: DefId, fields: &[HirNamedExpr]) {
        for field in fields {
            self.resolve_expr(owner, &field.value);
        }
    }

    fn resolve_match_arms(&mut self, owner: DefId, arms: &[HirMatchArm]) {
        for arm in arms {
            self.resolve_expr(owner, &arm.value);
        }
    }

    fn resolve_select_arms(&mut self, owner: DefId, arms: &[HirSelectArm]) {
        for arm in arms {
            if !self.is_default_select_pattern(&arm.pattern) {
                self.resolve_expr(owner, &arm.pattern);
            }
            self.resolve_expr(owner, &arm.value);
        }
    }

    fn is_default_select_pattern(&self, expr: &HirBodyExpr) -> bool {
        matches!(&expr.node, HirExprNode::Ident(name) if name == "default")
    }

    fn validate_enum_variant_expr(&mut self, expr: &HirBodyExpr, base: &HirBodyExpr, field: &str) {
        let Some(HirResolution::Def(def)) = self.design.expr_resolutions.get(&base.id()).copied()
        else {
            return;
        };
        if self.design.def_kind(def) != Some(HirDefKind::Enum) {
            return;
        }
        if self
            .design
            .enum_variants
            .contains_key(&HirEnumVariantKey::new(def, field))
        {
            return;
        }
        let enum_name = self.design.def_name(def).unwrap_or("<unknown>");
        self.record_unresolved_name(&format!("{enum_name}.{field}"), expr.span());
    }

    fn visible_def_exists(&self, owner: DefId, name: &str) -> bool {
        let Some(package) = self.owner_package_path(owner) else {
            return false;
        };
        self.design
            .canonical_def_names
            .contains_key(&package.with_leaf(name))
            || self.design.imports.iter().any(|import| {
                import.package_path == package
                    && import.path.last().is_some_and(|leaf| leaf == name)
            })
    }

    fn resolve_visible_def(&mut self, owner: DefId, name: &str, span: Span) -> Option<DefId> {
        let package = self.owner_package_path(owner)?;
        if let Some(def) = self
            .design
            .canonical_def_names
            .get(&package.with_leaf(name))
            .copied()
        {
            return Some(def);
        }
        let candidates = self
            .design
            .imports
            .iter()
            .filter(|import| import.package_path == package)
            .filter(|import| import.path.last().is_some_and(|leaf| leaf == name))
            .filter_map(|import| {
                self.design
                    .canonical_def_names
                    .get(&HirPath::new(import.path.clone()))
                    .map(|def| (import.path.join("."), *def))
            })
            .collect::<Vec<_>>();
        match candidates.as_slice() {
            [(_, def)] => Some(*def),
            [] => None,
            _ => {
                self.record_ambiguous_import(
                    name,
                    candidates
                        .iter()
                        .map(|(path, _)| path.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    span,
                );
                None
            }
        }
    }

    fn owner_package_path(&self, owner: DefId) -> Option<HirPath> {
        self.design
            .defs
            .get(owner.get())
            .map(|def| def.canonical_path.parent())
    }

    fn record_ambiguous_import(&mut self, name: &str, candidates: String, span: Span) {
        if self.error_mode == HirNameErrorMode::First && !self.errors.is_empty() {
            return;
        }
        self.errors.push(CompileError::lowering_at(
            HirError::AmbiguousImport {
                name: name.to_string(),
                candidates,
            },
            span,
        ));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
enum HirBuiltinIntrinsic {
    Zero,
    Assert,
    Error,
}

#[non_exhaustive]
struct BuiltinCallCallee<'a> {
    callee: &'a HirBodyExpr,
}

impl<'a> BuiltinCallCallee<'a> {
    fn new(callee: &'a HirBodyExpr) -> Self {
        Self { callee }
    }

    fn intrinsic(&self) -> Option<HirBuiltinIntrinsic> {
        let root = self.root()?;
        let HirExprNode::Ident(name) = &root.node else {
            return None;
        };
        match name.as_str() {
            "z" => Some(HirBuiltinIntrinsic::Zero),
            "zero" => Some(HirBuiltinIntrinsic::Zero),
            "assert" => Some(HirBuiltinIntrinsic::Assert),
            "error" => Some(HirBuiltinIntrinsic::Error),
            _ => None,
        }
    }

    fn root(&self) -> Option<&'a HirBodyExpr> {
        let mut current = self.callee;
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

struct ScopeStack {
    frames: Vec<BTreeMap<String, LocalId>>,
}

impl ScopeStack {
    fn new() -> Self {
        Self { frames: Vec::new() }
    }

    fn push(&mut self) {
        self.frames.push(BTreeMap::new());
    }

    fn pop(&mut self) {
        self.frames.pop();
    }

    fn insert(&mut self, name: &str, id: LocalId) {
        if let Some(frame) = self.frames.last_mut() {
            frame.insert(name.to_string(), id);
        }
    }

    fn resolve(&self, name: &str) -> Option<LocalId> {
        self.frames
            .iter()
            .rev()
            .find_map(|frame| frame.get(name).copied())
    }
}
