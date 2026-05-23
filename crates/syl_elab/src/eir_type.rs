use crate::{
    eir_build::{EirBuilder, Env},
    eir_expr::{EirBinaryOp, EirBound, EirExpr},
    map_ir::{MapBinaryOp, MapConstExpr, MapTypeRef, MapUnaryOp},
    mir::{MirBinaryOp, MirConstExpr, MirConstExprFacts, MirTypeRef, MirUnaryOp},
    program::{
        ElabBundleItem, ElabConstItem, ElabDefKind, ElabEnumItem, ElabInterfaceItem, ElabProgram,
    },
};
use std::collections::HashMap;
use syl_hir::DefId;

#[non_exhaustive]
pub(super) struct MapTypeLowerer {
    const_expr_depth: usize,
}

impl MapTypeLowerer {
    pub(super) fn new() -> Self {
        Self {
            const_expr_depth: 0,
        }
    }

    pub(super) fn lower_type_ref(&mut self, ty: &MapTypeRef) -> MirTypeRef {
        if let Some(path) = ty.path() {
            return MirTypeRef::path_type(path.to_vec(), ty.span());
        }
        if let Some((len, elem)) = ty.array() {
            return MirTypeRef::array_type(
                self.lower_const_expr(len),
                self.lower_type_ref(elem),
                ty.span(),
            );
        }
        if let Some(base) = ty.generic_base() {
            let args = ty
                .args()
                .unwrap_or_default()
                .iter()
                .map(|arg| self.lower_type_ref(arg))
                .collect();
            return MirTypeRef::generic_type(self.lower_type_ref(base), args, ty.span());
        }
        if let Some((base, view)) = ty.view_select() {
            return MirTypeRef::view_select_type(
                self.lower_type_ref(base),
                view.to_string(),
                ty.span(),
            );
        }
        MirTypeRef::unsupported(ty.span())
    }

    fn lower_const_expr(&mut self, expr: &MapConstExpr) -> MirConstExpr {
        self.const_expr_depth = self.const_expr_depth.saturating_add(1);
        let lowered = self.lower_const_expr_inner(expr);
        self.const_expr_depth = self.const_expr_depth.saturating_sub(1);
        lowered
    }

    fn lower_const_expr_inner(&mut self, expr: &MapConstExpr) -> MirConstExpr {
        debug_assert!(self.const_expr_depth < 1024);
        if let Some(value) = expr.int_value() {
            return MirConstExpr::int(value, expr.span());
        }
        if let Some(value) = expr.bool_value() {
            return MirConstExpr::bool_value_expr(value, expr.span());
        }
        if let Some((op, inner)) = expr.unary() {
            let op = match op {
                MapUnaryOp::Neg => MirUnaryOp::Neg,
                MapUnaryOp::Not => MirUnaryOp::Not,
                MapUnaryOp::NotWord => MirUnaryOp::NotWord,
                MapUnaryOp::Unsupported => MirUnaryOp::Unsupported,
                _ => MirUnaryOp::Unsupported,
            };
            return MirConstExpr::unary_expr(op, self.lower_const_expr(inner), expr.span());
        }
        if let Some((op, left, right)) = expr.binary() {
            let op = match op {
                MapBinaryOp::Assign => MirBinaryOp::Assign,
                MapBinaryOp::OrOr => MirBinaryOp::OrOr,
                MapBinaryOp::AndAnd => MirBinaryOp::AndAnd,
                MapBinaryOp::Eq => MirBinaryOp::Eq,
                MapBinaryOp::NotEq => MirBinaryOp::NotEq,
                MapBinaryOp::Lt => MirBinaryOp::Lt,
                MapBinaryOp::LtEq => MirBinaryOp::LtEq,
                MapBinaryOp::Gt => MirBinaryOp::Gt,
                MapBinaryOp::GtEq => MirBinaryOp::GtEq,
                MapBinaryOp::Add => MirBinaryOp::Add,
                MapBinaryOp::Sub => MirBinaryOp::Sub,
                MapBinaryOp::Mul => MirBinaryOp::Mul,
                MapBinaryOp::Div => MirBinaryOp::Div,
                MapBinaryOp::Rem => MirBinaryOp::Rem,
                MapBinaryOp::Shl => MirBinaryOp::Shl,
                MapBinaryOp::Field => MirBinaryOp::Field,
                MapBinaryOp::BitAnd => MirBinaryOp::BitAnd,
                MapBinaryOp::BitOr => MirBinaryOp::BitOr,
                MapBinaryOp::BitXor => MirBinaryOp::BitXor,
                MapBinaryOp::Unsupported => MirBinaryOp::Unsupported,
                _ => MirBinaryOp::Unsupported,
            };
            return MirConstExpr::binary_expr(
                op,
                self.lower_const_expr(left),
                self.lower_const_expr(right),
                expr.span(),
            );
        }
        let name = expr
            .ident()
            .map(str::to_string)
            .unwrap_or_else(|| expr.fact_key());
        MirConstExpr::ident_expr(name, expr.span())
    }
}

impl<'a> EirBuilder<'a> {
    pub(super) fn subst_bundle_field_type(
        &self,
        owner: Option<DefId>,
        bundle_ty: &MirTypeRef,
        field_ty: &MirTypeRef,
    ) -> MirTypeRef {
        let Some(args) = self.type_args(bundle_ty) else {
            return field_ty.clone();
        };
        let Some(bundle) = self.bundle_for_type(owner, bundle_ty) else {
            return field_ty.clone();
        };
        let mut replacements = HashMap::new();
        for (idx, generic) in bundle.generics.iter().enumerate() {
            if let Some(arg) = args.get(idx) {
                replacements.insert(generic.name.clone(), arg.clone());
            }
        }
        self.subst_type_vars(field_ty, &replacements)
    }

    pub(super) fn width(&self, owner: Option<DefId>, ty: &MirTypeRef) -> String {
        if let Some(path) = ty.path() {
            return self.path_width(owner, ty, path);
        }
        if let Some((len, elem)) = ty.array() {
            return format!("({})*({})", len.fact_key(), self.width(owner, elem));
        }
        if ty.view_select().is_some() {
            return "1".to_string();
        }
        if ty.args().is_some() {
            return self.generic_width(owner, ty);
        }
        "1".to_string()
    }

    pub(super) fn width_bound(&self, owner: Option<DefId>, ty: &MirTypeRef) -> EirBound {
        EirBound::new(self.width(owner, ty), self.width_expr(owner, ty))
    }

    pub(super) fn width_expr(&self, owner: Option<DefId>, ty: &MirTypeRef) -> EirExpr {
        if let Some(path) = ty.path() {
            return self.path_width_expr(owner, ty, path);
        }
        if let Some((len, elem)) = ty.array() {
            return EirExpr::binary(
                EirBinaryOp::Mul,
                self.const_expr_value(len),
                self.width_expr(owner, elem),
            );
        }
        if ty.view_select().is_some() {
            return EirExpr::Int(1);
        }
        if ty.args().is_some() {
            return self.generic_width_expr(owner, ty);
        }
        EirExpr::Int(1)
    }

    pub(super) fn bundle_width(&self, owner: Option<DefId>, ty: &MirTypeRef) -> String {
        let Some(bundle) = self.bundle_for_type(owner, ty) else {
            return "1".to_string();
        };
        let parts: Vec<String> = bundle
            .fields
            .iter()
            .map(|field| self.width(owner, &self.subst_bundle_field_type(owner, ty, &field.ty)))
            .collect();
        if parts.is_empty() {
            "1".to_string()
        } else {
            parts.join(" + ")
        }
    }

    pub(super) fn bundle_width_expr(&self, owner: Option<DefId>, ty: &MirTypeRef) -> EirExpr {
        let Some(bundle) = self.bundle_for_type(owner, ty) else {
            return EirExpr::Int(1);
        };
        let mut fields = bundle.fields.iter();
        let Some(first) = fields.next() else {
            return EirExpr::Int(1);
        };
        let first_ty = self.subst_bundle_field_type(owner, ty, &first.ty);
        fields.fold(self.width_expr(owner, &first_ty), |sum, field| {
            let field_ty = self.subst_bundle_field_type(owner, ty, &field.ty);
            EirExpr::binary(EirBinaryOp::Add, sum, self.width_expr(owner, &field_ty))
        })
    }

    pub(super) fn type_value(&self, owner: Option<DefId>, ty: &MirTypeRef) -> String {
        if let Some(path) = ty.path() {
            return path
                .last()
                .map(|name| {
                    self.const_for_type_value(owner, ty)
                        .map(|item| {
                            self.elab_expr(&item.value, &self.type_env(owner))
                                .fact_key()
                        })
                        .unwrap_or_else(|| name.clone())
                })
                .unwrap_or_else(|| "1".to_string());
        }
        self.width(owner, ty)
    }

    pub(super) fn type_value_expr(&self, owner: Option<DefId>, ty: &MirTypeRef) -> EirExpr {
        if let Some(name) = ty.path_name() {
            if let Some(item) = self.const_for_type_value(owner, ty) {
                return self.elab_expr(&item.value, &self.type_env(owner));
            }
            if let Ok(value) = name.parse::<u64>() {
                return EirExpr::Int(value);
            }
            if name == "true" {
                return EirExpr::Bool(true);
            }
            if name == "false" {
                return EirExpr::Bool(false);
            }
            return EirExpr::ident(name);
        }
        self.width_expr(owner, ty)
    }

    pub(super) fn const_for_name(
        &self,
        owner: Option<DefId>,
        name: &str,
    ) -> Option<&ElabConstItem> {
        let def = self.program.resolve_def_id(owner?, name)?;
        self.program.const_by_def(def)
    }

    pub(super) fn type_env(&self, owner: Option<DefId>) -> Env {
        match owner {
            Some(owner) => Env::with_owner(owner),
            None => Env::default(),
        }
    }

    pub(super) fn canonicalize_callsite_type(
        &self,
        owner: Option<DefId>,
        ty: &MirTypeRef,
    ) -> MirTypeRef {
        owner
            .map(|owner| self.canonicalize_type_for_owner(owner, ty))
            .unwrap_or_else(|| ty.clone())
    }

    pub(super) fn type_name(&self, ty: &MirTypeRef) -> Option<String> {
        ty.type_name().map(str::to_string)
    }

    pub(super) fn type_args<'b>(&self, ty: &'b MirTypeRef) -> Option<&'b [MirTypeRef]> {
        ty.args()
    }

    pub(super) fn subst_type_vars(
        &self,
        ty: &MirTypeRef,
        replacements: &HashMap<String, MirTypeRef>,
    ) -> MirTypeRef {
        ty.subst(replacements)
    }

    pub(super) fn type_arg_value(&self, owner: Option<DefId>, ty: &MirTypeRef) -> EirExpr {
        if let Some(name) = ty.path_name() {
            if let Ok(value) = name.parse::<u64>() {
                return EirExpr::Int(value);
            }
            if name == "true" {
                return EirExpr::Bool(true);
            }
            if name == "false" {
                return EirExpr::Bool(false);
            }
            return EirExpr::ident(name);
        }
        EirExpr::ident(self.type_value(owner, ty))
    }

    pub(super) fn array_len_key(&self, len: &MirConstExpr) -> String {
        len.fact_key()
    }

    pub(super) fn array_len_expr(&self, len: &MirConstExpr) -> EirExpr {
        self.const_expr_value(len)
    }

    fn path_width(&self, owner: Option<DefId>, ty: &MirTypeRef, path: &[String]) -> String {
        match path.last().map(String::as_str) {
            Some("Bit" | "Bool" | "Clock" | "Reset") => "1".to_string(),
            Some("Nat") => "32".to_string(),
            Some(name) => {
                if let Some(enm) = self.enum_for_type(owner, ty) {
                    self.enum_width(enm.variants).to_string()
                } else if self.bundle_for_type(owner, ty).is_some() {
                    self.bundle_width(owner, ty)
                } else {
                    format!("{name}_WIDTH")
                }
            }
            None => "1".to_string(),
        }
    }

    fn path_width_expr(&self, owner: Option<DefId>, ty: &MirTypeRef, path: &[String]) -> EirExpr {
        match path.last().map(String::as_str) {
            Some("Bit" | "Bool" | "Clock" | "Reset") => EirExpr::Int(1),
            Some("Nat") => EirExpr::Int(32),
            Some(name) => {
                if let Some(enm) = self.enum_for_type(owner, ty) {
                    EirExpr::Int(self.enum_width(enm.variants).try_into().unwrap_or(u64::MAX))
                } else if self.bundle_for_type(owner, ty).is_some() {
                    self.bundle_width_expr(owner, ty)
                } else {
                    EirExpr::ident(format!("{name}_WIDTH"))
                }
            }
            None => EirExpr::Int(1),
        }
    }

    fn generic_width(&self, owner: Option<DefId>, ty: &MirTypeRef) -> String {
        let is_int = matches!(ty.type_name(), Some("UInt" | "Bits" | "SInt"));
        if is_int {
            return ty
                .args()
                .and_then(|args| args.first())
                .map(|arg| self.type_value(owner, arg))
                .unwrap_or_else(|| "1".to_string());
        }
        self.bundle_width(owner, ty)
    }

    fn generic_width_expr(&self, owner: Option<DefId>, ty: &MirTypeRef) -> EirExpr {
        let is_int = matches!(ty.type_name(), Some("UInt" | "Bits" | "SInt"));
        if is_int {
            return ty
                .args()
                .and_then(|args| args.first())
                .map(|arg| self.type_value_expr(owner, arg))
                .unwrap_or(EirExpr::Int(1));
        }
        self.bundle_width_expr(owner, ty)
    }

    fn const_expr_value(&self, expr: &MirConstExpr) -> EirExpr {
        if let Some(value) = expr.int_value() {
            return EirExpr::Int(value);
        }
        if let Some(value) = expr.bool_value() {
            return EirExpr::Bool(value);
        }
        if let Some(name) = expr.ident() {
            return EirExpr::ident(name);
        }
        EirExpr::ident(expr.fact_key())
    }

    pub(super) fn const_for_type_value(
        &self,
        owner: Option<DefId>,
        ty: &MirTypeRef,
    ) -> Option<&ElabConstItem> {
        ElabTypeDefinitionResolver::new(self.program).const_item(owner, ty)
    }

    pub(super) fn enum_for_type(
        &self,
        owner: Option<DefId>,
        ty: &MirTypeRef,
    ) -> Option<&ElabEnumItem> {
        ElabTypeDefinitionResolver::new(self.program).enum_item(owner, ty)
    }

    pub(super) fn bundle_for_type(
        &self,
        owner: Option<DefId>,
        ty: &MirTypeRef,
    ) -> Option<&ElabBundleItem> {
        ElabTypeDefinitionResolver::new(self.program).bundle(owner, ty)
    }

    pub(super) fn interface_for_type(
        &self,
        owner: Option<DefId>,
        ty: &MirTypeRef,
    ) -> Option<&ElabInterfaceItem> {
        ElabTypeDefinitionResolver::new(self.program).interface(owner, ty)
    }

    fn canonicalize_type_for_owner(&self, owner: DefId, ty: &MirTypeRef) -> MirTypeRef {
        if let Some(path) = ty.path() {
            return self.canonicalize_path_type(owner, ty, path);
        }
        if let Some((len, elem)) = ty.array() {
            return MirTypeRef::array_type(
                len.clone(),
                self.canonicalize_type_for_owner(owner, elem),
                ty.span(),
            );
        }
        if let Some((base, view)) = ty.view_select() {
            return MirTypeRef::view_select_type(
                self.canonicalize_type_for_owner(owner, base),
                view.to_string(),
                ty.span(),
            );
        }
        if let Some(base) = ty.generic_base() {
            let args = ty
                .args()
                .unwrap_or_default()
                .iter()
                .map(|arg| self.canonicalize_type_for_owner(owner, arg))
                .collect();
            return MirTypeRef::generic_type(
                self.canonicalize_type_for_owner(owner, base),
                args,
                ty.span(),
            );
        }
        ty.clone()
    }

    fn canonicalize_path_type(&self, owner: DefId, ty: &MirTypeRef, path: &[String]) -> MirTypeRef {
        let def = if path.len() == 1 {
            self.program.resolve_def_id(owner, &path[0])
        } else {
            self.program.canonical_def_id(path)
        };
        let Some(def) = def else {
            return ty.clone();
        };
        if !matches!(
            self.program.def_kind(def),
            Some(ElabDefKind::Enum | ElabDefKind::Bundle | ElabDefKind::Interface)
        ) {
            return ty.clone();
        }
        let Some(canonical_path) = self.program.canonical_path(def) else {
            return ty.clone();
        };
        MirTypeRef::path_type(canonical_path.segments().to_vec(), ty.span())
    }
}

#[non_exhaustive]
struct ElabTypeDefinitionResolver<'a> {
    program: &'a ElabProgram,
}

impl<'a> ElabTypeDefinitionResolver<'a> {
    fn new(program: &'a ElabProgram) -> Self {
        Self { program }
    }

    fn const_item(&self, owner: Option<DefId>, ty: &MirTypeRef) -> Option<&'a ElabConstItem> {
        let def = self.def_id(owner?, ty)?;
        self.program.const_by_def(def)
    }

    fn enum_item(&self, owner: Option<DefId>, ty: &MirTypeRef) -> Option<&'a ElabEnumItem> {
        let def = self.def_id(owner?, ty)?;
        self.program.enum_by_def(def)
    }

    fn bundle(&self, owner: Option<DefId>, ty: &MirTypeRef) -> Option<&'a ElabBundleItem> {
        let def = self.def_id(owner?, ty)?;
        self.program.bundle_by_def(def)
    }

    fn interface(&self, owner: Option<DefId>, ty: &MirTypeRef) -> Option<&'a ElabInterfaceItem> {
        let def = self.def_id(owner?, ty)?;
        self.program.interface_by_def(def)
    }

    fn def_id(&self, owner: DefId, ty: &MirTypeRef) -> Option<DefId> {
        self.def_id_structural(owner, ty)
    }

    fn def_id_structural(&self, owner: DefId, ty: &MirTypeRef) -> Option<DefId> {
        if let Some(path) = ty.path() {
            return self.path_def_id(owner, path);
        }
        if let Some((base, _)) = ty.view_select() {
            return self.def_id_structural(owner, base);
        }
        if let Some(base) = ty.generic_base() {
            return self.def_id_structural(owner, base);
        }
        if let Some((_, elem)) = ty.array() {
            return self.def_id_structural(owner, elem);
        }
        None
    }

    fn path_def_id(&self, owner: DefId, path: &[String]) -> Option<DefId> {
        if path.len() == 1 {
            return self.program.resolve_def_id(owner, &path[0]);
        }
        self.program.canonical_def_id(path)
    }
}
