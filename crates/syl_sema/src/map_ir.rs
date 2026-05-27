use crate::{
    CompileError, EirError, TirError,
    hir::{
        HirBodyExpr, HirCallArg, HirExprNode, HirMapItem, HirMatchArm, HirNamedExpr, HirSelectArm,
    },
    hir_resolve::HirResolution,
    hir_view::HirDesignViewExt,
    tir::{BuiltinIntrinsic, BuiltinResolver, TirDesign},
};
use std::collections::BTreeMap;
use syl_hir::{DefId, LocalId};

mod metrics;
mod types;

pub use types::{
    MapBinaryOp, MapConstExpr, MapGenericArg, MapPattern, MapSelectMode, MapTypeRef, MapUnaryOp,
};

#[non_exhaustive]
pub struct MapIrProgram {
    maps: BTreeMap<DefId, MapFunction>,
}

impl MapIrProgram {
    fn new(maps: BTreeMap<DefId, MapFunction>) -> Self {
        Self { maps }
    }

    pub fn get(&self, id: DefId) -> Option<&MapFunction> {
        self.maps.get(&id)
    }

    pub fn len(&self) -> usize {
        self.maps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.maps.is_empty()
    }
}

#[non_exhaustive]
pub struct MapFunction {
    generics: Vec<String>,
    params: Vec<MapParam>,
    body: MapExpr,
}

impl MapFunction {
    fn new(generics: Vec<String>, params: Vec<MapParam>, body: MapExpr) -> Self {
        Self {
            generics,
            params,
            body,
        }
    }

    pub fn generics(&self) -> &[String] {
        &self.generics
    }

    pub fn params(&self) -> &[MapParam] {
        &self.params
    }

    pub fn body(&self) -> &MapExpr {
        &self.body
    }
}

#[non_exhaustive]
pub struct MapParam {
    id: Option<LocalId>,
    name: String,
    ty: MapTypeRef,
}

impl MapParam {
    fn new(id: Option<LocalId>, name: String, ty: MapTypeRef) -> Self {
        Self { id, name, ty }
    }

    pub fn id(&self) -> Option<LocalId> {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn ty(&self) -> &MapTypeRef {
        &self.ty
    }
}

#[non_exhaustive]
pub struct MapLocalRef {
    id: Option<LocalId>,
    name: String,
}

impl MapLocalRef {
    fn new(id: Option<LocalId>, name: String) -> Self {
        Self { id, name }
    }

    pub fn id(&self) -> Option<LocalId> {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

#[non_exhaustive]
pub enum MapExpr {
    Ident(MapLocalRef),
    Int(u64),
    Bool(bool),
    Str(String),
    Unary {
        op: MapUnaryOp,
        expr: Box<MapExpr>,
    },
    Binary {
        op: MapBinaryOp,
        left: Box<MapExpr>,
        right: Box<MapExpr>,
    },
    BuiltinHighZ,
    BuiltinZero,
    Call {
        callee: DefId,
        generic_args: Vec<MapGenericArg>,
        args: Vec<MapArg>,
    },
    Aggregate {
        ty: MapTypeRef,
        fields: Vec<MapNamedExpr>,
    },
    Field {
        base: Box<MapExpr>,
        field: String,
    },
    Index {
        base: Box<MapExpr>,
        index: Box<MapExpr>,
    },
    Match {
        value: Box<MapExpr>,
        arms: Vec<MapMatchArm>,
    },
    Select {
        mode: MapSelectMode,
        arms: Vec<MapSelectArm>,
    },
}

#[non_exhaustive]
pub struct MapArg {
    name: Option<String>,
    value: MapExpr,
}

impl MapArg {
    fn new(name: Option<String>, value: MapExpr) -> Self {
        Self { name, value }
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn value(&self) -> &MapExpr {
        &self.value
    }
}

#[non_exhaustive]
pub struct MapNamedExpr {
    name: String,
    value: MapExpr,
}

impl MapNamedExpr {
    fn new(name: String, value: MapExpr) -> Self {
        Self { name, value }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn value(&self) -> &MapExpr {
        &self.value
    }
}

#[non_exhaustive]
pub struct MapMatchArm {
    pattern: MapPattern,
    value: MapExpr,
}

impl MapMatchArm {
    fn new(pattern: MapPattern, value: MapExpr) -> Self {
        Self { pattern, value }
    }

    pub fn pattern(&self) -> &MapPattern {
        &self.pattern
    }

    pub fn value(&self) -> &MapExpr {
        &self.value
    }
}

#[non_exhaustive]
pub struct MapSelectArm {
    pattern: MapExpr,
    value: MapExpr,
}

impl MapSelectArm {
    fn new(pattern: MapExpr, value: MapExpr) -> Self {
        Self { pattern, value }
    }

    pub fn pattern(&self) -> &MapExpr {
        &self.pattern
    }

    pub fn value(&self) -> &MapExpr {
        &self.value
    }
}

#[non_exhaustive]
pub struct MapIrBuilder<'a> {
    tir: &'a TirDesign,
}

impl<'a> MapIrBuilder<'a> {
    pub fn new(tir: &'a TirDesign) -> Self {
        Self { tir }
    }

    pub fn build(self) -> Result<MapIrProgram, CompileError> {
        let mut maps = BTreeMap::new();
        for (owner, map) in &self.tir.hir().maps {
            maps.insert(*owner, self.lower_map(*owner, map)?);
        }
        Ok(MapIrProgram::new(maps))
    }

    fn lower_map(&self, owner: DefId, map: &HirMapItem) -> Result<MapFunction, CompileError> {
        let generics = map
            .generics
            .iter()
            .map(|generic| generic.name.clone())
            .collect();
        let params = map
            .params
            .iter()
            .map(|param| MapParam::new(param.id, param.name.clone(), MapTypeRef::from(&param.ty)))
            .collect();
        Ok(MapFunction::new(
            generics,
            params,
            self.lower_expr(owner, &map.body)?,
        ))
    }

    fn lower_expr(&self, owner: DefId, expr: &HirBodyExpr) -> Result<MapExpr, CompileError> {
        let lowered = match &expr.node {
            HirExprNode::Ident(name) => MapExpr::Ident(self.local_ref_for_expr(owner, expr, name)),
            HirExprNode::Int(value) => MapExpr::Int(*value),
            HirExprNode::Bool(value) => MapExpr::Bool(*value),
            HirExprNode::Str(value) => MapExpr::Str(value.clone()),
            HirExprNode::Unary { op, expr } => {
                let op = MapUnaryOp::from(*op);
                if matches!(op, MapUnaryOp::Unsupported) {
                    return Err(CompileError::lowering_at(
                        TirError::InvalidElaborationExpression,
                        expr.span(),
                    ));
                }
                MapExpr::Unary {
                    op,
                    expr: Box::new(self.lower_expr(owner, expr)?),
                }
            }
            HirExprNode::Binary {
                op, left, right, ..
            } => {
                let op = MapBinaryOp::from(*op);
                if matches!(op, MapBinaryOp::Unsupported | MapBinaryOp::Field) {
                    return Err(CompileError::lowering_at(
                        TirError::InvalidElaborationExpression,
                        expr.span(),
                    ));
                }
                MapExpr::Binary {
                    op,
                    left: Box::new(self.lower_expr(owner, left)?),
                    right: Box::new(self.lower_expr(owner, right)?),
                }
            }
            HirExprNode::Call { callee, args } => return self.lower_call(owner, callee, args),
            HirExprNode::GenericApp { callee, .. } | HirExprNode::Group(callee) => {
                return self.lower_expr(owner, callee);
            }
            HirExprNode::Aggregate { ty, fields } => MapExpr::Aggregate {
                ty: MapTypeRef::from(ty.as_ref()),
                fields: self.lower_named_exprs(owner, fields)?,
            },
            HirExprNode::Field { base, field } => {
                if let Some(value) = self.enum_variant_value(expr) {
                    MapExpr::Int(value)
                } else {
                    MapExpr::Field {
                        base: Box::new(self.lower_expr(owner, base)?),
                        field: field.clone(),
                    }
                }
            }
            HirExprNode::Index { base, index } => MapExpr::Index {
                base: Box::new(self.lower_expr(owner, base)?),
                index: Box::new(self.lower_expr(owner, index)?),
            },
            HirExprNode::Match { expr, arms } => MapExpr::Match {
                value: Box::new(self.lower_expr(owner, expr)?),
                arms: self.lower_match_arms(owner, expr, arms)?,
            },
            HirExprNode::Select { mode, arms } => MapExpr::Select {
                mode: MapSelectMode::from(*mode),
                arms: self.lower_select_arms(owner, arms)?,
            },
            HirExprNode::Block(_)
            | HirExprNode::Place { .. }
            | HirExprNode::For { .. }
            | HirExprNode::CompileError { .. }
            | HirExprNode::Range { .. }
            | HirExprNode::Unsupported => {
                return Err(CompileError::lowering_at(
                    TirError::InvalidElaborationExpression,
                    expr.span(),
                ));
            }
            _ => {
                return Err(CompileError::lowering_at(
                    TirError::InvalidElaborationExpression,
                    expr.span(),
                ));
            }
        };
        Ok(lowered)
    }

    fn lower_call(
        &self,
        owner: DefId,
        callee: &HirBodyExpr,
        args: &[HirCallArg],
    ) -> Result<MapExpr, CompileError> {
        if let Some(call) = self.tir.extension_method_call(owner, callee) {
            let mut lowered_args = vec![MapArg::new(None, self.lower_expr(owner, call.receiver)?)];
            lowered_args.extend(
                args.iter()
                    .map(|arg| {
                        Ok(MapArg::new(
                            arg.name.clone(),
                            self.lower_expr(owner, &arg.value)?,
                        ))
                    })
                    .collect::<Result<Vec<_>, CompileError>>()?,
            );
            let mut generic_args = self.generic_args(callee);
            if generic_args.is_empty() {
                generic_args = call.inferred_args.iter().map(MapGenericArg::from).collect();
            }
            return Ok(MapExpr::Call {
                callee: call.method,
                generic_args,
                args: lowered_args,
            });
        }
        let generic_args = self.generic_args(callee);
        match BuiltinResolver::new(self.tir.hir(), Some(owner)).resolve_call_callee(callee) {
            Some(BuiltinIntrinsic::HighZ) => return Ok(MapExpr::BuiltinHighZ),
            Some(BuiltinIntrinsic::Zero) => return Ok(MapExpr::BuiltinZero),
            _ => {}
        }
        let Some(callee_name) = self.expr_name(callee) else {
            return Err(CompileError::lowering_at(
                TirError::InvalidElaborationExpression,
                callee.span(),
            ));
        };
        let lowered_args = args
            .iter()
            .map(|arg| {
                Ok(MapArg::new(
                    arg.name.clone(),
                    self.lower_expr(owner, &arg.value)?,
                ))
            })
            .collect::<Result<Vec<_>, CompileError>>()?;
        let Some(root) = self.callee_root(callee) else {
            return Err(CompileError::lowering_at(
                TirError::InvalidElaborationExpression,
                callee.span(),
            ));
        };
        let Ok(Some(HirResolution::Def(def))) = self.tir.hir().expr_resolution(owner, root) else {
            return Err(CompileError::lowering_at(
                EirError::UnknownHardwareValueCall { name: callee_name },
                callee.span(),
            ));
        };
        if self.tir.hir().def_kind(def) == Some(crate::hir::HirDefKind::Map) {
            Ok(MapExpr::Call {
                callee: def,
                generic_args,
                args: lowered_args,
            })
        } else {
            Err(CompileError::lowering_at(
                EirError::UnknownHardwareValueCall { name: callee_name },
                callee.span(),
            ))
        }
    }

    fn enum_variant_value(&self, expr: &HirBodyExpr) -> Option<u64> {
        let (enum_def, variant) = self.tir.hir().enum_variant_expr(expr)?;
        self.tir
            .enum_variant_values()
            .get(&crate::hir::HirEnumVariantKey::new(enum_def, variant))
            .copied()
    }

    fn lower_named_exprs(
        &self,
        owner: DefId,
        fields: &[HirNamedExpr],
    ) -> Result<Vec<MapNamedExpr>, CompileError> {
        fields
            .iter()
            .map(|field| {
                Ok(MapNamedExpr::new(
                    field.name.clone(),
                    self.lower_expr(owner, &field.value)?,
                ))
            })
            .collect()
    }

    fn lower_match_arms(
        &self,
        owner: DefId,
        value: &HirBodyExpr,
        arms: &[HirMatchArm],
    ) -> Result<Vec<MapMatchArm>, CompileError> {
        let target_enum_name = self.match_target_enum_name(value);
        arms.iter()
            .map(|arm| {
                Ok(MapMatchArm::new(
                    self.lower_match_pattern(&arm.pattern, target_enum_name.as_deref()),
                    self.lower_expr(owner, &arm.value)?,
                ))
            })
            .collect()
    }

    fn lower_match_pattern(
        &self,
        pattern: &crate::mir::MirPattern,
        target_enum_name: Option<&str>,
    ) -> MapPattern {
        match pattern {
            crate::mir::MirPattern::Path(path, _) if path.len() == 1 => {
                let Some(enum_name) = target_enum_name else {
                    return MapPattern::Path(path.clone());
                };
                MapPattern::Path(vec![enum_name.to_string(), path[0].clone()])
            }
            _ => MapPattern::from(pattern),
        }
    }

    fn match_target_enum_name(&self, value: &HirBodyExpr) -> Option<String> {
        let ty = self
            .tir
            .expr_types()
            .get(&value.id())
            .and_then(|ty| self.tir.type_table().get(*ty))?;
        let def = ty.definition()?;
        (self.tir.hir().def_kind(def) == Some(crate::hir::HirDefKind::Enum))
            .then(|| self.tir.hir().def_name(def).map(str::to_string))
            .flatten()
    }

    fn lower_select_arms(
        &self,
        owner: DefId,
        arms: &[HirSelectArm],
    ) -> Result<Vec<MapSelectArm>, CompileError> {
        arms.iter()
            .map(|arm| {
                Ok(MapSelectArm::new(
                    self.lower_expr(owner, &arm.pattern)?,
                    self.lower_expr(owner, &arm.value)?,
                ))
            })
            .collect()
    }

    fn callee_root<'b>(&self, expr: &'b HirBodyExpr) -> Option<&'b HirBodyExpr> {
        let mut current = expr;
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

    fn expr_name(&self, expr: &HirBodyExpr) -> Option<String> {
        let mut current = expr;
        loop {
            match &current.node {
                HirExprNode::Ident(name) => return Some(name.clone()),
                HirExprNode::GenericApp { callee, .. } | HirExprNode::Group(callee) => {
                    current = callee;
                }
                _ => return None,
            }
        }
    }

    fn generic_args(&self, expr: &HirBodyExpr) -> Vec<MapGenericArg> {
        let mut current = expr;
        loop {
            match &current.node {
                HirExprNode::GenericApp { args, .. } => {
                    return args.iter().map(MapGenericArg::from).collect();
                }
                HirExprNode::Group(expr) => current = expr,
                _ => return Vec::new(),
            }
        }
    }

    fn local_ref_for_expr(&self, owner: DefId, expr: &HirBodyExpr, name: &str) -> MapLocalRef {
        let id = self
            .tir
            .hir()
            .expr_resolution(owner, expr)
            .ok()
            .flatten()
            .and_then(|resolution| match resolution {
                HirResolution::Local(id) => Some(id),
                HirResolution::Def(_) => None,
                _ => None,
            });
        MapLocalRef::new(id, name.to_string())
    }
}
