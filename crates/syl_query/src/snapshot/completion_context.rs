use crate::CompletionItemKind;
use syl_sema::completion::CompletionKind;
use syl_span::Span;
use syl_syntax::{
    AstFile, Block, CallArg, CallableItem, ConstItem, Expr, ExternModuleItem, FieldDecl, FnItem,
    GenericParam, InterfaceItem, Item, MapItem, MatchArm, NamedExpr, Param, PortDecl, RegReset,
    ResultBinding, SelectArm, Stmt, TypeExpr,
};

#[non_exhaustive]
pub(super) struct CompletionAnalyzer<'a> {
    file: &'a AstFile,
    span: Span,
    source: &'a str,
}

impl<'a> CompletionAnalyzer<'a> {
    pub(super) fn new(file: &'a AstFile, span: Span, source: &'a str) -> Self {
        Self { file, span, source }
    }

    pub(super) fn analyze(&self) -> Option<CompletionContext> {
        CompletionContextInspector::new(self.span)
            .inspect_file(self.file)
            .or_else(|| CompletionSourceAnalyzer::new(self.source, self.span.start).analyze())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(super) enum CompletionContext {
    Type,
    Expression,
    FieldAccess,
    ImportPath,
}

impl CompletionContext {
    pub(super) fn accepts_semantic_kind(self, kind: CompletionKind) -> bool {
        match self {
            Self::Type => kind.is_type(),
            Self::Expression => kind.is_value_or_callable_or_local(),
            Self::FieldAccess => kind.is_field(),
            Self::ImportPath => kind.is_definition(),
        }
    }

    pub(super) fn accepts_item_kind(self, kind: CompletionItemKind) -> bool {
        match self {
            Self::Type => matches!(kind, CompletionItemKind::Type),
            Self::Expression => matches!(
                kind,
                CompletionItemKind::Constant
                    | CompletionItemKind::Function
                    | CompletionItemKind::Module
            ),
            Self::FieldAccess => matches!(kind, CompletionItemKind::Field),
            Self::ImportPath => true,
        }
    }
}

struct CompletionContextInspector {
    span: Span,
}

impl CompletionContextInspector {
    fn new(span: Span) -> Self {
        Self { span }
    }

    fn inspect_file(&self, file: &AstFile) -> Option<CompletionContext> {
        for item in &file.items {
            if let Some(context) = self.inspect_item(item) {
                return Some(context);
            }
        }
        None
    }

    fn inspect_item(&self, item: &Item) -> Option<CompletionContext> {
        match item {
            Item::Use(item) if self.contains(item.span) => Some(CompletionContext::ImportPath),
            Item::Const(item) => self.inspect_const_item(item),
            Item::Fn(item) => self.inspect_fn_item(item),
            Item::Bundle(item) => self.inspect_bundle_item(&item.generics, &item.fields),
            Item::Interface(item) => self.inspect_interface_item(item),
            Item::Map(item) => self.inspect_map_item(item),
            Item::Cell(item) | Item::Module(item) => self.inspect_callable_item(item),
            Item::ExternModule(item) => self.inspect_extern_module_item(item),
            Item::Package(_) | Item::Use(_) | Item::Enum(_) | Item::Error(_) => None,
            _ => None,
        }
    }

    fn inspect_const_item(&self, item: &ConstItem) -> Option<CompletionContext> {
        self.inspect_optional_type(item.ty.as_ref())
            .or_else(|| self.inspect_expr(&item.value))
    }

    fn inspect_fn_item(&self, item: &FnItem) -> Option<CompletionContext> {
        self.inspect_params(&item.params)
            .or_else(|| self.inspect_optional_type(item.ret_ty.as_ref()))
            .or_else(|| self.inspect_block(&item.body))
    }

    fn inspect_bundle_item(
        &self,
        generics: &[GenericParam],
        fields: &[FieldDecl],
    ) -> Option<CompletionContext> {
        self.inspect_generics(generics)
            .or_else(|| self.inspect_fields(fields))
    }

    fn inspect_interface_item(&self, item: &InterfaceItem) -> Option<CompletionContext> {
        self.inspect_generics(&item.generics)
            .or_else(|| self.inspect_fields(&item.fields))
    }

    fn inspect_map_item(&self, item: &MapItem) -> Option<CompletionContext> {
        self.inspect_generics(&item.generics)
            .or_else(|| self.inspect_params(&item.params))
            .or_else(|| self.inspect_optional_type(item.ret_ty.as_ref()))
            .or_else(|| self.inspect_expr(&item.body))
    }

    fn inspect_callable_item(&self, item: &CallableItem) -> Option<CompletionContext> {
        self.inspect_generics(&item.generics)
            .or_else(|| self.inspect_params(&item.params))
            .or_else(|| self.inspect_ports(&item.ports))
            .or_else(|| self.inspect_optional_result(item.result.as_ref()))
            .or_else(|| self.inspect_block(&item.body))
    }

    fn inspect_extern_module_item(&self, item: &ExternModuleItem) -> Option<CompletionContext> {
        self.inspect_generics(&item.generics)
            .or_else(|| self.inspect_params(&item.params))
            .or_else(|| self.inspect_ports(&item.ports))
            .or_else(|| self.inspect_optional_result(item.result.as_ref()))
    }

    fn inspect_generics<'a>(
        &self,
        generics: impl IntoIterator<Item = &'a GenericParam>,
    ) -> Option<CompletionContext> {
        for generic in generics {
            if let Some(context) = self
                .inspect_optional_type(generic.kind.as_ref())
                .or_else(|| self.inspect_optional_expr(generic.default.as_ref()))
            {
                return Some(context);
            }
        }
        None
    }

    fn inspect_params(&self, params: &[Param]) -> Option<CompletionContext> {
        for param in params {
            if let Some(context) = self.inspect_type(&param.ty) {
                return Some(context);
            }
        }
        None
    }

    fn inspect_ports(&self, ports: &[PortDecl]) -> Option<CompletionContext> {
        for port in ports {
            if let Some(context) = self.inspect_type(&port.ty) {
                return Some(context);
            }
        }
        None
    }

    fn inspect_fields(&self, fields: &[FieldDecl]) -> Option<CompletionContext> {
        for field in fields {
            if let Some(context) = self.inspect_type(&field.ty) {
                return Some(context);
            }
        }
        None
    }

    fn inspect_optional_result(&self, result: Option<&ResultBinding>) -> Option<CompletionContext> {
        result.and_then(|result| self.inspect_type(&result.ty))
    }

    fn inspect_optional_type(&self, ty: Option<&TypeExpr>) -> Option<CompletionContext> {
        ty.and_then(|ty| self.inspect_type(ty))
    }

    fn inspect_optional_expr(&self, expr: Option<&Expr>) -> Option<CompletionContext> {
        expr.and_then(|expr| self.inspect_expr(expr))
    }

    fn inspect_block(&self, block: &Block) -> Option<CompletionContext> {
        for stmt in &block.stmts {
            if let Some(context) = self.inspect_stmt(stmt) {
                return Some(context);
            }
        }
        self.inspect_optional_expr(block.tail.as_deref())
    }

    fn inspect_stmt(&self, stmt: &Stmt) -> Option<CompletionContext> {
        match stmt {
            Stmt::Const { ty, value, .. } => self
                .inspect_optional_type(ty.as_ref())
                .or_else(|| self.inspect_expr(value)),
            Stmt::Let { ty, value, .. }
            | Stmt::Var { ty, value, .. }
            | Stmt::Signal { ty, value, .. } => self
                .inspect_optional_type(ty.as_ref())
                .or_else(|| self.inspect_optional_expr(value.as_ref())),
            Stmt::Next { value, .. } | Stmt::Return(Some(value), _) => self.inspect_expr(value),
            Stmt::Reg { ty, reset, .. } => self
                .inspect_optional_type(ty.as_ref())
                .or_else(|| self.inspect_optional_reg_reset(reset.as_ref())),
            Stmt::While { cond, body, .. } => {
                self.inspect_expr(cond).or_else(|| self.inspect_block(body))
            }
            Stmt::ElabIf {
                cond,
                then_block,
                else_block,
                ..
            } => self
                .inspect_expr(cond)
                .or_else(|| self.inspect_block(then_block))
                .or_else(|| {
                    else_block
                        .as_ref()
                        .and_then(|block| self.inspect_block(block))
                }),
            Stmt::ElabFor { range, body, .. } => self
                .inspect_expr(range)
                .or_else(|| self.inspect_block(body)),
            Stmt::Expr(expr) => self.inspect_expr(expr),
            Stmt::Error { .. } | Stmt::Return(None, _) => None,
            _ => None,
        }
    }

    fn inspect_optional_reg_reset(&self, reset: Option<&RegReset>) -> Option<CompletionContext> {
        reset.and_then(|reset| {
            self.inspect_optional_expr(reset.domain.as_ref())
                .or_else(|| self.inspect_expr(&reset.value))
        })
    }

    fn inspect_expr(&self, expr: &Expr) -> Option<CompletionContext> {
        match expr {
            Expr::GenericApp { args, .. } => self
                .inspect_type_args(args)
                .or_else(|| self.expression_context(expr)),
            Expr::Aggregate { ty, fields, .. } => self
                .inspect_type(ty)
                .or_else(|| self.inspect_named_exprs(fields))
                .or_else(|| self.expression_context(expr)),
            Expr::Unary { expr, .. } => self
                .inspect_expr(expr)
                .or_else(|| self.expression_context(expr)),
            Expr::Binary { left, right, .. } => self
                .inspect_expr(left)
                .or_else(|| self.inspect_expr(right))
                .or_else(|| self.expression_context(expr)),
            Expr::Call { callee, args, .. } | Expr::Place { callee, args, .. } => self
                .inspect_expr(callee)
                .or_else(|| self.inspect_call_args(args))
                .or_else(|| self.expression_context(expr)),
            Expr::For { range, body, .. } => self
                .inspect_expr(range)
                .or_else(|| self.inspect_block(body))
                .or_else(|| self.expression_context(expr)),
            Expr::Field { base, .. } => self
                .inspect_expr(base)
                .or_else(|| self.field_access_context(expr, base)),
            Expr::Index { base, index, .. } => self
                .inspect_expr(base)
                .or_else(|| self.inspect_expr(index))
                .or_else(|| self.expression_context(expr)),
            Expr::Group(inner, _) => self
                .inspect_expr(inner)
                .or_else(|| self.expression_context(expr)),
            Expr::Block(block) => self
                .inspect_block(block)
                .or_else(|| self.expression_context(expr)),
            Expr::Match {
                expr: scrutinee,
                arms,
                ..
            } => self
                .inspect_expr(scrutinee)
                .or_else(|| self.inspect_match_arms(arms))
                .or_else(|| self.expression_context(expr)),
            Expr::Select { arms, .. } => self
                .inspect_select_arms(arms)
                .or_else(|| self.expression_context(expr)),
            Expr::CompileError { message, .. } => self
                .inspect_expr(message)
                .or_else(|| self.expression_context(expr)),
            Expr::Range { start, end, .. } => self
                .inspect_expr(start)
                .or_else(|| self.inspect_expr(end))
                .or_else(|| self.expression_context(expr)),
            Expr::Ident(_, _) | Expr::Int(_, _) | Expr::Str(_, _) | Expr::Bool(_, _) => {
                self.expression_context(expr)
            }
            _ => self.expression_context(expr),
        }
    }

    fn inspect_type(&self, ty: &TypeExpr) -> Option<CompletionContext> {
        match ty {
            TypeExpr::Array { len, elem, .. } => self
                .inspect_expr(len)
                .or_else(|| self.inspect_type(elem))
                .or_else(|| self.type_context(ty)),
            TypeExpr::Generic { base, args, .. } => self
                .inspect_type(base)
                .or_else(|| self.inspect_type_args(args))
                .or_else(|| self.type_context(ty)),
            TypeExpr::ViewSelect { base, .. } => {
                self.inspect_type(base).or_else(|| self.type_context(ty))
            }
            TypeExpr::Path(_, _) => self.type_context(ty),
            _ => self.type_context(ty),
        }
    }

    fn inspect_call_args(&self, args: &[CallArg]) -> Option<CompletionContext> {
        for arg in args {
            if let Some(context) = self.inspect_expr(&arg.value) {
                return Some(context);
            }
        }
        None
    }

    fn inspect_named_exprs(&self, fields: &[NamedExpr]) -> Option<CompletionContext> {
        for field in fields {
            if let Some(context) = self.inspect_expr(&field.value) {
                return Some(context);
            }
        }
        None
    }

    fn inspect_match_arms(&self, arms: &[MatchArm]) -> Option<CompletionContext> {
        for arm in arms {
            if let Some(context) = self.inspect_expr(&arm.value) {
                return Some(context);
            }
        }
        None
    }

    fn inspect_select_arms(&self, arms: &[SelectArm]) -> Option<CompletionContext> {
        for arm in arms {
            if let Some(context) = self
                .inspect_expr(&arm.pattern)
                .or_else(|| self.inspect_expr(&arm.value))
            {
                return Some(context);
            }
        }
        None
    }

    fn inspect_type_args(&self, args: &[TypeExpr]) -> Option<CompletionContext> {
        for arg in args {
            if let Some(context) = self.inspect_type(arg) {
                return Some(context);
            }
        }
        None
    }

    fn expression_context(&self, expr: &Expr) -> Option<CompletionContext> {
        self.contains(expr.span())
            .then_some(CompletionContext::Expression)
    }

    fn field_access_context(&self, expr: &Expr, base: &Expr) -> Option<CompletionContext> {
        (self.contains(expr.span()) && base.span().end <= self.span.start)
            .then_some(CompletionContext::FieldAccess)
    }

    fn type_context(&self, ty: &TypeExpr) -> Option<CompletionContext> {
        self.contains(ty.span()).then_some(CompletionContext::Type)
    }

    fn contains(&self, span: Span) -> bool {
        span.source == self.span.source
            && span.start <= self.span.start
            && self.span.end <= span.end
    }
}

#[non_exhaustive]
struct CompletionSourceAnalyzer<'a> {
    source: &'a str,
    offset: usize,
}

impl<'a> CompletionSourceAnalyzer<'a> {
    fn new(source: &'a str, offset: usize) -> Self {
        Self { source, offset }
    }

    fn analyze(&self) -> Option<CompletionContext> {
        if self.import_path_context() {
            return Some(CompletionContext::ImportPath);
        }
        if self.type_context() {
            return Some(CompletionContext::Type);
        }
        if self.field_access_context() {
            return Some(CompletionContext::FieldAccess);
        }
        if self.expression_context() {
            return Some(CompletionContext::Expression);
        }
        None
    }

    fn import_path_context(&self) -> bool {
        let Some(line) = self.current_line_before_cursor() else {
            return false;
        };
        let trimmed = line.trim_start();
        let Some(after_use) = trimmed.strip_prefix("use") else {
            return false;
        };
        (after_use.is_empty() || after_use.starts_with(char::is_whitespace))
            && !after_use.contains(';')
    }

    fn type_context(&self) -> bool {
        let Some(line) = self.current_line_before_cursor() else {
            return false;
        };
        let trimmed = line.trim_start();
        self.after_return_arrow(trimmed)
            || self.after_type_decl_colon(trimmed)
            || self.after_port_direction(trimmed)
    }

    fn field_access_context(&self) -> bool {
        self.current_line_before_cursor()
            .is_some_and(|line| line.trim_end().ends_with('.'))
    }

    fn expression_context(&self) -> bool {
        let Some(line) = self.current_line_before_cursor() else {
            return false;
        };
        let trimmed = line.trim_start();
        let tail = line.trim_end();
        trimmed.starts_with("return ")
            || tail.ends_with(":=")
            || tail.ends_with('=')
            || tail.ends_with("return")
    }

    fn after_return_arrow(&self, line: &str) -> bool {
        let Some((_, after_arrow)) = line.rsplit_once("->") else {
            return false;
        };
        !after_arrow.contains('=')
    }

    fn after_type_decl_colon(&self, line: &str) -> bool {
        let Some(colon) = self.last_type_colon(line) else {
            return false;
        };
        let Some(before_colon) = line.get(..colon) else {
            return false;
        };
        let Some(after_colon) = line.get(colon + ':'.len_utf8()..) else {
            return false;
        };
        self.starts_type_declaration(before_colon.trim_start())
            && !after_colon.contains('=')
            && !after_colon.contains('{')
    }

    fn after_port_direction(&self, line: &str) -> bool {
        let tail = line.trim_end();
        (tail.ends_with(" in") || tail.ends_with(" out")) && line.contains(':')
    }

    fn starts_type_declaration(&self, line: &str) -> bool {
        [
            "const ", "let ", "var ", "signal ", "reg ", "module ", "cell ", "extern ", "fn ",
            "map ",
        ]
        .iter()
        .any(|keyword| line.starts_with(keyword))
    }

    fn last_type_colon(&self, line: &str) -> Option<usize> {
        for (index, ch) in line.char_indices().rev() {
            if ch != ':' {
                continue;
            }
            let after_colon = line.get(index + ch.len_utf8()..)?;
            if after_colon.starts_with('=') {
                continue;
            }
            return Some(index);
        }
        None
    }

    fn current_line_before_cursor(&self) -> Option<&'a str> {
        let before_cursor = self.source.get(..self.offset)?;
        Some(
            before_cursor
                .rsplit_once('\n')
                .map(|(_, line)| line)
                .unwrap_or(before_cursor),
        )
    }
}
