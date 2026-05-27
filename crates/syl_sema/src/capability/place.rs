use crate::{
    hir::resolve::HirResolution,
    hir::view::HirDesignViewExt,
    hir::{HirBodyExpr, HirDesign, HirExprNode},
};
use syl_hir::{DefId, LocalId};
use syl_span::Span;

#[non_exhaustive]
pub(super) struct Place {
    root: PlaceRoot,
    projections: Vec<String>,
    field: Option<String>,
    field_suffixes: Vec<String>,
    span: Span,
}

#[non_exhaustive]
pub(super) struct PlaceResolver<'a> {
    hir: &'a HirDesign,
    owner: DefId,
    expr: &'a HirBodyExpr,
}

#[non_exhaustive]
pub(super) enum PlaceResolution {
    Place(Place),
    NotPlace,
    UnresolvedName { name: String, span: Span },
}

impl<'a> PlaceResolver<'a> {
    pub(super) fn new(hir: &'a HirDesign, owner: DefId, expr: &'a HirBodyExpr) -> Self {
        Self { hir, owner, expr }
    }

    pub(super) fn resolve(&self) -> PlaceResolution {
        let mut expr = self.expr;
        let mut components = Vec::new();
        loop {
            match &expr.node {
                HirExprNode::Ident(name) => {
                    let root = match self.root_for_ident(expr, name) {
                        RootLookup::Local(root) => root,
                        RootLookup::NotPlace => return PlaceResolution::NotPlace,
                        RootLookup::UnresolvedName { name, span } => {
                            return PlaceResolution::UnresolvedName { name, span };
                        }
                    };
                    let mut place = Place {
                        root,
                        projections: Vec::new(),
                        field: None,
                        field_suffixes: Vec::new(),
                        span: self.expr.span(),
                    };
                    for component in components.iter().rev() {
                        match component {
                            PlaceComponent::Index(index) => place.projections.push(index.clone()),
                            PlaceComponent::Field(field) => {
                                if place.field.is_some() {
                                    place.field_suffixes.push(field.clone());
                                } else {
                                    place.field = Some(field.clone());
                                }
                            }
                        }
                    }
                    return PlaceResolution::Place(place);
                }
                HirExprNode::Field { base, field } => {
                    components.push(PlaceComponent::Field(field.clone()));
                    expr = base;
                }
                HirExprNode::Index { base, index } => {
                    components.push(PlaceComponent::Index(
                        IndexProjection { expr: index }.display(),
                    ));
                    expr = base;
                }
                HirExprNode::Group(base) => expr = base,
                _ => return PlaceResolution::NotPlace,
            }
        }
    }

    fn root_for_ident(&self, expr: &HirBodyExpr, name: &str) -> RootLookup {
        match self.hir.expr_resolution(self.owner, expr) {
            Ok(Some(HirResolution::Local(id))) => RootLookup::Local(PlaceRoot::new(id, name)),
            Ok(Some(HirResolution::Def(_))) => RootLookup::NotPlace,
            Ok(Some(_)) => RootLookup::NotPlace,
            Ok(None) | Err(_) => RootLookup::UnresolvedName {
                name: name.to_string(),
                span: expr.span(),
            },
        }
    }
}

#[non_exhaustive]
enum RootLookup {
    Local(PlaceRoot),
    NotPlace,
    UnresolvedName { name: String, span: Span },
}

#[derive(Clone)]
#[non_exhaustive]
struct PlaceRoot {
    id: LocalId,
    name: String,
}

impl PlaceRoot {
    fn new(id: LocalId, name: &str) -> Self {
        Self {
            id,
            name: name.to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct PlaceKey {
    root: LocalId,
    projections: Vec<String>,
    field: Option<String>,
    field_suffixes: Vec<String>,
}

#[non_exhaustive]
enum PlaceComponent {
    Field(String),
    Index(String),
}

#[non_exhaustive]
struct IndexProjection<'a> {
    expr: &'a HirBodyExpr,
}

impl<'a> IndexProjection<'a> {
    fn display(&self) -> String {
        let mut expr = self.expr;
        while let HirExprNode::Group(inner) = &expr.node {
            expr = inner;
        }
        match &expr.node {
            HirExprNode::Ident(name) => name.clone(),
            HirExprNode::Int(value) => value.to_string(),
            HirExprNode::Bool(value) => value.to_string(),
            _ => format!("expr@{}", expr.span().start),
        }
    }
}

impl Place {
    pub(super) fn display(&self) -> String {
        let mut out = self.root.name.clone();
        for projection in &self.projections {
            out.push('[');
            out.push_str(projection);
            out.push(']');
        }
        if let Some(field) = &self.field {
            out.push('.');
            out.push_str(field);
        }
        for field in &self.field_suffixes {
            out.push('.');
            out.push_str(field);
        }
        out
    }

    pub(super) fn span(&self) -> Span {
        self.span
    }

    pub(super) fn root_name(&self) -> &str {
        &self.root.name
    }

    pub(super) fn root_id(&self) -> LocalId {
        self.root.id
    }

    pub(super) fn has_field(&self) -> bool {
        self.field.is_some()
    }

    pub(super) fn field(&self) -> Option<&str> {
        self.field.as_deref()
    }

    pub(super) fn field_place(&self, field: &str) -> Self {
        Self {
            root: self.root.clone(),
            projections: self.projections.clone(),
            field: Some(field.to_string()),
            field_suffixes: Vec::new(),
            span: self.span,
        }
    }

    pub(super) fn key(&self) -> PlaceKey {
        PlaceKey {
            root: self.root_id(),
            projections: self.projections.clone(),
            field: self.field.clone(),
            field_suffixes: self.field_suffixes.clone(),
        }
    }
}
