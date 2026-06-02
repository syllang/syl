#[cfg(test)]
use super::TirDesign;
use super::{BindingRef, TypePhaseChecker};
use crate::hir::resolve::HirResolution;
use crate::ir::mir::MirTypeRef;
use crate::{CompileError, TirError};
use crate::{
    hir::view::HirDesignViewExt,
    hir::{HirBodyExpr, HirDefKind, HirExprNode, HirLocalKind},
};
use syl_hir::{DefId, LocalId};
use syl_syntax::BinaryOp;

mod const_term;
mod id;
mod table;

use super::consts::TirConstKind;
pub use const_term::TirConstTerm;
pub(super) use const_term::TirConstTermResolver;
pub use id::TypeId;
pub use table::TirTypeTable;

#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum TirType {
    Unknown,
    Nat,
    Bool,
    Bit,
    Clock {
        domain: Option<Box<TirType>>,
    },
    Reset {
        domain: Option<Box<TirType>>,
    },
    Domain,
    Str,
    UInt {
        width: TirConstTerm,
    },
    Bits {
        width: TirConstTerm,
    },
    SInt {
        width: TirConstTerm,
    },
    Array {
        len: TirConstTerm,
        elem: Box<TirType>,
    },
    View {
        base: Box<TirType>,
        view: String,
    },
    Named {
        name: String,
        def: Option<DefId>,
        generic: Option<LocalId>,
        kind: Option<HirDefKind>,
        args: Vec<TirGenericArg>,
    },
}

impl PartialEq for TirType {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Unknown, Self::Unknown)
            | (Self::Nat, Self::Nat)
            | (Self::Bool, Self::Bool)
            | (Self::Bit, Self::Bit)
            | (Self::Domain, Self::Domain)
            | (Self::Str, Self::Str) => true,
            (Self::Clock { domain: left }, Self::Clock { domain: right })
            | (Self::Reset { domain: left }, Self::Reset { domain: right }) => left == right,
            (Self::UInt { width: left }, Self::UInt { width: right }) => left == right,
            (Self::Bits { width: left }, Self::Bits { width: right }) => left == right,
            (Self::SInt { width: left }, Self::SInt { width: right }) => left == right,
            (
                Self::Array {
                    len: left_len,
                    elem: left_elem,
                },
                Self::Array {
                    len: right_len,
                    elem: right_elem,
                },
            ) => left_len == right_len && left_elem == right_elem,
            (
                Self::View {
                    base: left_base,
                    view: left_view,
                },
                Self::View {
                    base: right_base,
                    view: right_view,
                },
            ) => left_base == right_base && left_view == right_view,
            (
                Self::Named {
                    def: left_def,
                    generic: left_generic,
                    args: left_args,
                    ..
                },
                Self::Named {
                    def: right_def,
                    generic: right_generic,
                    args: right_args,
                    ..
                },
            ) => {
                if left_def.is_some() || right_def.is_some() {
                    return left_def == right_def && left_args == right_args;
                }
                if left_generic.is_some() || right_generic.is_some() {
                    return left_generic == right_generic && left_args == right_args;
                }
                false
            }
            _ => false,
        }
    }
}

impl Eq for TirType {}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TirGenericArg {
    Type(Box<TirType>),
    Const(TirConstTerm),
}

impl TirGenericArg {
    fn label(&self) -> String {
        match self {
            Self::Type(ty) => ty.label(),
            Self::Const(term) => term.label(),
        }
    }
}

impl TirType {
    pub fn label(&self) -> String {
        match self {
            Self::Unknown => "<unknown>".to_string(),
            Self::Nat => "nat".to_string(),
            Self::Bool => "bool".to_string(),
            Self::Bit => "Bit".to_string(),
            Self::Clock { domain: None } => "Clock".to_string(),
            Self::Clock {
                domain: Some(domain),
            } => format!("Clock<{}>", domain.label()),
            Self::Reset { domain: None } => "Reset".to_string(),
            Self::Reset {
                domain: Some(domain),
            } => format!("Reset<{}>", domain.label()),
            Self::Domain => "Domain".to_string(),
            Self::Str => "string".to_string(),
            Self::UInt { width } => format!("UInt<{width}>"),
            Self::Bits { width } => format!("Bits<{width}>"),
            Self::SInt { width } => format!("SInt<{width}>"),
            Self::Array { len, elem } => format!("[{}; {len}]", elem.label()),
            Self::View { base, view } => format!("{}.{view}", base.label()),
            Self::Named { name, args, .. } if args.is_empty() => name.clone(),
            Self::Named { name, args, .. } => {
                let args = args
                    .iter()
                    .map(TirGenericArg::label)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{name}<{args}>")
            }
        }
    }

    pub fn definition(&self) -> Option<DefId> {
        match self {
            Self::Named { def, .. } => *def,
            Self::View { base, .. } => base.definition(),
            Self::Array { elem, .. } => elem.definition(),
            _ => None,
        }
    }

    pub fn generic_args(&self) -> &[TirGenericArg] {
        match self {
            Self::Named { args, .. } => args,
            Self::View { base, .. } => base.generic_args(),
            _ => &[],
        }
    }

    fn selected_view(&self) -> Option<&str> {
        match self {
            Self::View { view, .. } => Some(view.as_str()),
            Self::Array { elem, .. } => elem.selected_view(),
            _ => None,
        }
    }

    #[cfg(test)]
    pub fn definition_kind(&self) -> Option<HirDefKind> {
        match self {
            Self::Named { kind, .. } => *kind,
            Self::View { base, .. } => base.definition_kind(),
            Self::Array { elem, .. } => elem.definition_kind(),
            _ => None,
        }
    }

    pub(super) fn with_args(self, args: Vec<TirGenericArg>) -> Self {
        match self {
            Self::Named {
                name,
                def,
                generic,
                kind,
                ..
            } => Self::Named {
                name,
                def,
                generic,
                kind,
                args,
            },
            other => other,
        }
    }
}

#[cfg(test)]
impl TirDesign {
    pub fn binding_type_id(&self, binding: BindingRef) -> Option<TypeId> {
        self.binding_types.get(&binding).copied()
    }

    pub fn binding_type_generic_local(&self, binding: BindingRef) -> Option<LocalId> {
        let ty = self.binding_types.get(&binding)?;
        match self.type_table.get(*ty)? {
            TirType::Named { generic, .. } => *generic,
            _ => None,
        }
    }

    pub fn binding_uint_width_local(&self, binding: BindingRef) -> Option<LocalId> {
        let ty = self.binding_types.get(&binding)?;
        match self.type_table.get(*ty)? {
            TirType::UInt { width } => width.local(),
            _ => None,
        }
    }
}

impl TypePhaseChecker {
    pub(super) fn infer_expr_type(&self, owner: DefId, expr: &HirBodyExpr) -> TirType {
        match &expr.node {
            HirExprNode::Int(_) => TirType::Nat,
            HirExprNode::Bool(_) => TirType::Bool,
            HirExprNode::Str(_) => TirType::Str,
            HirExprNode::Ident(_) => self
                .hir
                .expr_resolution(owner, expr)
                .ok()
                .flatten()
                .and_then(|resolution| self.type_for_resolution(resolution))
                .unwrap_or(TirType::Unknown),
            HirExprNode::GenericApp { callee, .. } | HirExprNode::Group(callee) => {
                self.infer_expr_type(owner, callee)
            }
            HirExprNode::Field { base, field } => {
                self.infer_field_expr_type(expr, owner, base, field)
            }
            HirExprNode::Index { base, .. } => self.infer_index_type(owner, base),
            HirExprNode::Unary { expr, .. } => self.infer_expr_type(owner, expr),
            HirExprNode::Binary { op, left, .. } if self.binary_returns_bool(*op) => {
                let _left_type = self.infer_expr_type(owner, left);
                TirType::Bool
            }
            HirExprNode::Binary { op, left, .. } if self.binary_returns_bit(*op) => {
                let _left_type = self.infer_expr_type(owner, left);
                TirType::Bit
            }
            HirExprNode::Binary { left, .. } => self.infer_expr_type(owner, left),
            HirExprNode::Call { callee, .. } => self.infer_call_type(owner, callee),
            _ => TirType::Unknown,
        }
    }

    pub(super) fn type_from_mir_type_ref(
        &self,
        owner: DefId,
        ty: &MirTypeRef,
    ) -> Result<TirType, CompileError> {
        let const_terms = TirConstTermResolver::new(self, owner);
        if let Some(path) = ty.path() {
            return match path.last().map(String::as_str) {
                Some("nat" | "Nat") => Ok(TirType::Nat),
                Some("bool" | "Bool") => Ok(TirType::Bool),
                Some("string" | "Str") => Ok(TirType::Str),
                Some("Bit") => Ok(TirType::Bit),
                Some("Clock") => Ok(TirType::Clock { domain: None }),
                Some("Reset") => Ok(TirType::Reset { domain: None }),
                Some("Domain") => Ok(TirType::Domain),
                Some(name) => self.named_type_from_mir_path(owner, ty, name),
                None => Ok(TirType::Unknown),
            };
        }
        if let Some(base) = ty.generic_base() {
            if matches!(base.type_name(), Some("Clock" | "Reset")) {
                let domain = ty
                    .args()
                    .and_then(|args| args.first())
                    .and_then(|arg| self.type_from_mir_type_ref(owner, arg).ok())
                    .map(Box::new);
                return match base.type_name() {
                    Some("Clock") => Ok(TirType::Clock { domain }),
                    Some("Reset") => Ok(TirType::Reset { domain }),
                    _ => Ok(TirType::Unknown),
                };
            }
            if matches!(base.type_name(), Some("UInt" | "Bits" | "SInt")) {
                let width = ty
                    .args()
                    .and_then(|args| args.first())
                    .map(|arg| const_terms.resolve_mir_type_ref(arg))
                    .unwrap_or(TirConstTerm::NatLiteral(1));
                return match base.type_name() {
                    Some("UInt") => Ok(TirType::UInt { width }),
                    Some("Bits") => Ok(TirType::Bits { width }),
                    Some("SInt") => Ok(TirType::SInt { width }),
                    _ => Ok(TirType::Unknown),
                };
            }
            let base_ty = self.type_from_mir_type_ref(owner, base)?;
            let args = self.generic_args_from_mir_type_refs(
                owner,
                &base_ty,
                ty.args().unwrap_or_default(),
            )?;
            return Ok(base_ty.with_args(args));
        }
        if let Some((len, elem)) = ty.array() {
            return Ok(TirType::Array {
                len: const_terms.resolve_mir_const_expr(len),
                elem: Box::new(self.type_from_mir_type_ref(owner, elem)?),
            });
        }
        if let Some((base, view)) = ty.view_select() {
            let base_ty = self.type_from_mir_type_ref(owner, base)?;
            return Ok(TirType::View {
                base: Box::new(base_ty),
                view: view.to_string(),
            });
        }
        Ok(TirType::Unknown)
    }

    fn named_type_from_mir_path(
        &self,
        owner: DefId,
        ty: &MirTypeRef,
        name: &str,
    ) -> Result<TirType, CompileError> {
        if let Some(generic) = self.owner_generic_id(owner, name) {
            return Ok(TirType::Named {
                name: name.to_string(),
                def: None,
                generic: Some(generic),
                kind: None,
                args: Vec::new(),
            });
        }
        let Some(def) = self.hir.type_def_for_mir_type(owner, ty) else {
            return Err(CompileError::lowering_at(
                TirError::UnknownType {
                    name: name.to_string(),
                },
                ty.span(),
            ));
        };
        let kind = self.hir.def_kind(def);
        if !matches!(
            kind,
            Some(HirDefKind::Enum | HirDefKind::Bundle | HirDefKind::Interface)
        ) {
            return Err(CompileError::lowering_at(
                TirError::UnknownType {
                    name: name.to_string(),
                },
                ty.span(),
            ));
        }
        Ok(TirType::Named {
            name: name.to_string(),
            def: Some(def),
            generic: None,
            kind,
            args: Vec::new(),
        })
    }

    fn owner_generic_id(&self, owner: DefId, name: &str) -> Option<LocalId> {
        self.hir
            .locals
            .iter()
            .find(|local| {
                local.owner == owner
                    && local.name == name
                    && matches!(local.kind, HirLocalKind::Generic)
            })
            .map(|local| local.id)
    }

    fn generic_args_from_mir_type_refs(
        &self,
        owner: DefId,
        base: &TirType,
        args: &[MirTypeRef],
    ) -> Result<Vec<TirGenericArg>, CompileError> {
        let base_def = base.definition();
        args.iter()
            .enumerate()
            .map(|(index, arg)| {
                if self.generic_param_expects_const(base_def, index) {
                    Ok(TirGenericArg::Const(
                        TirConstTermResolver::new(self, owner).resolve_mir_type_ref(arg),
                    ))
                } else {
                    self.type_from_mir_type_ref(owner, arg)
                        .map(Box::new)
                        .map(TirGenericArg::Type)
                }
            })
            .collect()
    }

    pub(super) fn generic_param_expects_const(
        &self,
        base_def: Option<DefId>,
        index: usize,
    ) -> bool {
        let Some(base_def) = base_def else {
            return false;
        };
        self.hir
            .bundles
            .get(&base_def)
            .and_then(|item| item.generics.get(index))
            .or_else(|| {
                self.hir
                    .interfaces
                    .get(&base_def)
                    .and_then(|item| item.generics.get(index))
            })
            .and_then(|generic| generic.kind.as_ref())
            .and_then(|kind| self.mir_type_kind(kind))
            .is_some()
    }

    fn infer_field_expr_type(
        &self,
        expr: &HirBodyExpr,
        owner: DefId,
        base: &HirBodyExpr,
        field: &str,
    ) -> TirType {
        if let Some((enum_def, _)) = self.hir.enum_variant_expr(expr) {
            return self.named_type_for_def(enum_def);
        }
        self.infer_field_type(owner, base, field)
    }

    fn infer_field_type(&self, owner: DefId, base: &HirBodyExpr, field: &str) -> TirType {
        let base_ty = self.infer_expr_type(owner, base);
        let Some(type_def) = base_ty.definition() else {
            return TirType::Unknown;
        };
        let Some(field_ty) = self
            .hir
            .member_field_type(type_def, base_ty.selected_view(), field)
        else {
            return TirType::Unknown;
        };
        self.type_from_mir_type_ref(type_def, &field_ty)
            .unwrap_or(TirType::Unknown)
    }

    fn named_type_for_def(&self, def: DefId) -> TirType {
        TirType::Named {
            name: self.hir.def_name(def).unwrap_or("<unknown>").to_string(),
            def: Some(def),
            generic: None,
            kind: self.hir.def_kind(def),
            args: Vec::new(),
        }
    }

    fn infer_index_type(&self, owner: DefId, base: &HirBodyExpr) -> TirType {
        match self.infer_expr_type(owner, base) {
            TirType::Array { elem, .. } => *elem,
            TirType::UInt { .. } | TirType::Bits { .. } | TirType::SInt { .. } | TirType::Bit => {
                TirType::Bit
            }
            _ => TirType::Unknown,
        }
    }

    fn infer_call_type(&self, owner: DefId, callee: &HirBodyExpr) -> TirType {
        let Some(map_def) = self.map_callee_def(owner, callee).or_else(|| {
            self.extension_method_call(owner, callee)
                .filter(|call| self.hir.def_kind(call.method) == Some(HirDefKind::Map))
                .map(|call| call.method)
        }) else {
            return self.infer_expr_type(owner, callee);
        };
        super::return_type::MapReturnTypeResolver::new(self, owner, map_def, callee)
            .resolve()
            .unwrap_or(TirType::Unknown)
    }

    pub(super) fn mir_type_kind(&self, ty: &MirTypeRef) -> Option<TirConstKind> {
        let mut current = ty;
        loop {
            if let Some(name) = current.path_name() {
                return match name {
                    "nat" | "Nat" => Some(TirConstKind::Nat),
                    "bool" | "Bool" => Some(TirConstKind::Bool),
                    _ => None,
                };
            }
            if let Some(base) = current.generic_base() {
                current = base;
                continue;
            }
            if let Some((base, _)) = current.view_select() {
                current = base;
                continue;
            }
            if let Some((_, elem)) = current.array() {
                current = elem;
                continue;
            }
            return None;
        }
    }

    fn type_for_resolution(&self, resolution: HirResolution) -> Option<TirType> {
        let id = match resolution {
            HirResolution::Def(id) => self.binding_types.get(&BindingRef::Def(id)),
            HirResolution::Local(id) => self.binding_types.get(&BindingRef::Local(id)),
            _ => None,
        }?;
        self.type_table.get(*id).cloned()
    }

    fn mir_type_label(&self, owner: DefId, ty: &MirTypeRef) -> String {
        self.type_from_mir_type_ref(owner, ty)
            .map_or_else(|_| "<unknown>".to_string(), |ty| ty.label())
    }

    fn binary_returns_bool(&self, op: BinaryOp) -> bool {
        matches!(
            op,
            BinaryOp::EqEq
                | BinaryOp::NotEq
                | BinaryOp::Lt
                | BinaryOp::LtEq
                | BinaryOp::Gt
                | BinaryOp::GtEq
                | BinaryOp::AndAnd
                | BinaryOp::OrOr
        )
    }

    fn binary_returns_bit(&self, op: BinaryOp) -> bool {
        matches!(
            op,
            BinaryOp::AndWord | BinaryOp::OrWord | BinaryOp::XorWord | BinaryOp::EqWord
        )
    }
}
