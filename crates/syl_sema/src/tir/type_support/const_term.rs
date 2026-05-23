use super::TypePhaseChecker;
use crate::{
    hir::HirLocalKind,
    hir_view::HirDesignViewExt,
    mir::{MirConstExpr, MirConstExprFacts, MirTypeRef},
};
use syl_hir::{DefId, LocalId};

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TirConstTerm {
    Unknown,
    NatLiteral(u64),
    BoolLiteral(bool),
    // Equality includes semantic resolution so same labels from different locals do not collapse.
    Named {
        name: String,
        resolution: Option<TirConstResolution>,
    },
    Expr {
        label: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TirConstResolution {
    Def(DefId),
    Local(LocalId),
}

impl TirConstTerm {
    pub fn label(&self) -> String {
        match self {
            Self::Unknown => "<unknown>".to_string(),
            Self::NatLiteral(value) => value.to_string(),
            Self::BoolLiteral(value) => value.to_string(),
            Self::Named { name, .. } => name.clone(),
            Self::Expr { label } => label.clone(),
        }
    }

    #[cfg(test)]
    pub(super) fn local(&self) -> Option<LocalId> {
        match self {
            Self::Named {
                resolution: Some(TirConstResolution::Local(id)),
                ..
            } => Some(*id),
            _ => None,
        }
    }
}

#[non_exhaustive]
pub(crate) struct TirConstTermResolver<'checker> {
    checker: &'checker TypePhaseChecker,
    owner: DefId,
}

impl<'checker> TirConstTermResolver<'checker> {
    pub(crate) fn new(checker: &'checker TypePhaseChecker, owner: DefId) -> Self {
        Self { checker, owner }
    }

    pub(crate) fn resolve_mir_type_ref(&self, ty: &MirTypeRef) -> TirConstTerm {
        if let Some(path) = ty.path() {
            let Some(name) = path.last() else {
                return TirConstTerm::Unknown;
            };
            if let Ok(value) = name.parse::<u64>() {
                return TirConstTerm::NatLiteral(value);
            }
            return match name.as_str() {
                "true" => TirConstTerm::BoolLiteral(true),
                "false" => TirConstTerm::BoolLiteral(false),
                _ => TirConstTerm::Named {
                    name: name.clone(),
                    resolution: self.resolve_mir_type_path(path, ty),
                },
            };
        }
        TirConstTerm::Expr {
            label: self.checker.mir_type_label(self.owner, ty),
        }
    }

    pub(crate) fn resolve_mir_const_expr(&self, expr: &MirConstExpr) -> TirConstTerm {
        if let Some(value) = expr.nat_value() {
            return TirConstTerm::NatLiteral(value);
        }
        if let Some(value) = expr.bool_value() {
            return TirConstTerm::BoolLiteral(value);
        }
        if let Some(name) = expr.ident() {
            return TirConstTerm::Named {
                name: name.to_string(),
                resolution: self.resolve_owner_generic_or_param(name),
            };
        }
        TirConstTerm::Expr {
            label: expr.fact_key(),
        }
    }

    fn resolve_mir_type_path(
        &self,
        path: &[String],
        ty: &MirTypeRef,
    ) -> Option<TirConstResolution> {
        match path {
            [name] => self.resolve_owner_generic_or_param(name).or_else(|| {
                self.checker
                    .hir
                    .resolve_def_id(self.owner, name)
                    .map(TirConstResolution::Def)
            }),
            _ => self
                .checker
                .hir
                .type_def_for_mir_type(self.owner, ty)
                .map(TirConstResolution::Def),
        }
    }

    fn resolve_owner_generic_or_param(&self, name: &str) -> Option<TirConstResolution> {
        self.checker
            .hir
            .locals
            .iter()
            .find(|local| {
                local.owner == self.owner
                    && local.name == name
                    && matches!(local.kind, HirLocalKind::Generic | HirLocalKind::Param)
            })
            .map(|local| TirConstResolution::Local(local.id))
    }
}

impl std::fmt::Display for TirConstTerm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label())
    }
}
