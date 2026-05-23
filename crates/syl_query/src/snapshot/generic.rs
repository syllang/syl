use syl_span::Span;
use syl_syntax::{
    AstFile, CallableItem, Expr, ExternModuleItem, GenericParam, Item, Param, TypeExpr,
};

#[non_exhaustive]
pub(super) struct GenericDefinitionResolver {
    cursor: Span,
}

impl GenericDefinitionResolver {
    pub(super) fn new(cursor: Span) -> Self {
        Self { cursor }
    }

    pub(super) fn resolve_file<'a>(&self, file: &'a AstFile) -> Option<&'a GenericParam> {
        file.items
            .iter()
            .filter(|item| self.item_span(item).is_some_and(|span| self.contains(span)))
            .find_map(|item| self.resolve_item(item))
    }

    fn resolve_item<'a>(&self, item: &'a Item) -> Option<&'a GenericParam> {
        match item {
            Item::Bundle(item) => self.resolve_generic_decl(&item.generics).or_else(|| {
                item.fields
                    .iter()
                    .find_map(|field| self.resolve_type(&item.generics, &field.ty))
            }),
            Item::Interface(item) => self.resolve_generic_decl(&item.generics).or_else(|| {
                item.fields
                    .iter()
                    .find_map(|field| self.resolve_type(&item.generics, &field.ty))
            }),
            Item::Map(item) => self
                .resolve_generic_decl(&item.generics)
                .or_else(|| self.resolve_param_types(&item.generics, &item.params))
                .or_else(|| self.resolve_optional_type(&item.generics, item.ret_ty.as_ref())),
            Item::Cell(item) | Item::Module(item) => self
                .resolve_generic_decl(&item.generics)
                .or_else(|| self.resolve_callable(item)),
            Item::ExternModule(item) => self
                .resolve_generic_decl(&item.generics)
                .or_else(|| self.resolve_extern_module(item)),
            Item::Package(_) | Item::Use(_) | Item::Const(_) | Item::Fn(_) | Item::Enum(_) => None,
            _ => None,
        }
    }

    fn resolve_generic_decl<'a>(&self, generics: &'a [GenericParam]) -> Option<&'a GenericParam> {
        generics
            .iter()
            .find(|generic| self.contains(self.name_span(generic)))
    }

    fn resolve_callable<'a>(&self, item: &'a CallableItem) -> Option<&'a GenericParam> {
        self.resolve_param_types(&item.generics, &item.params)
            .or_else(|| {
                item.ports
                    .iter()
                    .find_map(|port| self.resolve_type(&item.generics, &port.ty))
            })
            .or_else(|| {
                item.result
                    .as_ref()
                    .and_then(|result| self.resolve_type(&item.generics, &result.ty))
            })
    }

    fn resolve_extern_module<'a>(&self, item: &'a ExternModuleItem) -> Option<&'a GenericParam> {
        self.resolve_param_types(&item.generics, &item.params)
            .or_else(|| {
                item.ports
                    .iter()
                    .find_map(|port| self.resolve_type(&item.generics, &port.ty))
            })
            .or_else(|| {
                item.result
                    .as_ref()
                    .and_then(|result| self.resolve_type(&item.generics, &result.ty))
            })
    }

    fn resolve_param_types<'a>(
        &self,
        generics: &'a [GenericParam],
        params: &'a [Param],
    ) -> Option<&'a GenericParam> {
        params
            .iter()
            .find_map(|param| self.resolve_type(generics, &param.ty))
    }

    fn resolve_optional_type<'a>(
        &self,
        generics: &'a [GenericParam],
        ty: Option<&TypeExpr>,
    ) -> Option<&'a GenericParam> {
        ty.and_then(|ty| self.resolve_type(generics, ty))
    }

    fn resolve_type<'a>(
        &self,
        generics: &'a [GenericParam],
        ty: &TypeExpr,
    ) -> Option<&'a GenericParam> {
        if !self.contains(ty.span()) {
            return None;
        }
        match ty {
            TypeExpr::Path(path, span) if path.len() == 1 && self.contains(*span) => {
                generics.iter().find(|generic| generic.name == path[0])
            }
            TypeExpr::Array { elem, .. } => self.resolve_type(generics, elem),
            TypeExpr::Generic { base, args, .. } => self
                .resolve_type(generics, base)
                .or_else(|| args.iter().find_map(|arg| self.resolve_type(generics, arg))),
            TypeExpr::ViewSelect { base, .. } => self.resolve_type(generics, base),
            TypeExpr::Path(_, _) => None,
            _ => None,
        }
    }

    fn contains(&self, span: Span) -> bool {
        span.source == self.cursor.source
            && self.cursor.start >= span.start
            && self.cursor.start <= span.end
    }

    fn name_span(&self, generic: &GenericParam) -> Span {
        Span::new_in(
            generic.span.source,
            generic.span.start,
            generic.span.start.saturating_add(generic.name.len()),
        )
    }

    fn item_span(&self, item: &Item) -> Option<Span> {
        match item {
            Item::Package(item) => Some(item.span),
            Item::Use(item) => Some(item.span),
            Item::Const(item) => Some(item.span),
            Item::Fn(item) => Some(item.span),
            Item::Enum(item) => Some(item.span),
            Item::Bundle(item) => Some(item.span),
            Item::Interface(item) => Some(item.span),
            Item::Map(item) => Some(item.span),
            Item::Cell(item) | Item::Module(item) => Some(item.span),
            Item::ExternModule(item) => Some(item.span),
            Item::Error(item) => Some(item.span),
            _ => None,
        }
    }
}

#[non_exhaustive]
pub(super) struct GenericParamHover<'a> {
    generic: &'a GenericParam,
}

impl<'a> GenericParamHover<'a> {
    pub(super) fn new(generic: &'a GenericParam) -> Self {
        Self { generic }
    }

    pub(super) fn contents(&self) -> String {
        let mut text = format!("generic {}", self.generic.name);
        if let Some(kind) = &self.generic.kind {
            text.push_str(": ");
            text.push_str(&TypeExprLabel::new(kind).contents());
        }
        if let Some(default) = &self.generic.default {
            text.push_str(" = ");
            text.push_str(&ExprLabel::new(default).contents());
        }
        text
    }
}

#[non_exhaustive]
struct TypeExprLabel<'a> {
    ty: &'a TypeExpr,
}

impl<'a> TypeExprLabel<'a> {
    fn new(ty: &'a TypeExpr) -> Self {
        Self { ty }
    }

    fn contents(&self) -> String {
        match self.ty {
            TypeExpr::Path(path, _) => path.join("."),
            TypeExpr::Array { len, elem, .. } => {
                format!(
                    "[{}] {}",
                    ExprLabel::new(len).contents(),
                    TypeExprLabel::new(elem).contents()
                )
            }
            TypeExpr::Generic { base, args, .. } => {
                let args = args
                    .iter()
                    .map(|arg| TypeExprLabel::new(arg).contents())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}<{args}>", TypeExprLabel::new(base).contents())
            }
            TypeExpr::ViewSelect { base, view, .. } => {
                format!("{}.{}", TypeExprLabel::new(base).contents(), view)
            }
            _ => "<type>".to_string(),
        }
    }
}

#[non_exhaustive]
struct ExprLabel<'a> {
    expr: &'a Expr,
}

impl<'a> ExprLabel<'a> {
    fn new(expr: &'a Expr) -> Self {
        Self { expr }
    }

    fn contents(&self) -> String {
        match self.expr {
            Expr::Ident(name, _) => name.clone(),
            Expr::Int(value, _) => value.to_string(),
            Expr::Bool(value, _) => value.to_string(),
            Expr::Str(value, _) => format!("\"{value}\""),
            _ => "<expr>".to_string(),
        }
    }
}
