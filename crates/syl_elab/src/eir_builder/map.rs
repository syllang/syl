use crate::{
    eir_builder::{EirBuilder, Env},
    eir::{EirBinaryOp, EirExpr, EirSelectArm, EirSelectMode, EirUnaryOp},
    map_ir::{
        MapArg, MapBinaryOp, MapExpr, MapFunction, MapGenericArg, MapMatchArm, MapNamedExpr,
        MapPattern, MapSelectArm, MapTypeRef, MapUnaryOp,
    },
    program::{ElabCallArg, ElabDefKind, ElabExpr, ElabExprNode, ElabResolution},
};
use std::collections::HashMap;
use syl_hir::DefId;

use super::ty::MapTypeLowerer;

struct MapElabArgBinding<'a> {
    map: &'a MapFunction,
    args: &'a [ElabCallArg],
    caller_env: &'a Env,
    map_env: &'a mut Env,
    replacements: &'a HashMap<String, MapTypeRef>,
}

struct MapArgBinding<'a> {
    map: &'a MapFunction,
    args: &'a [MapArg],
    caller_env: &'a Env,
    map_env: &'a mut Env,
    replacements: &'a HashMap<String, MapTypeRef>,
}

impl<'a> EirBuilder<'a> {
    pub(super) fn map_call_expr_from_elab(
        &self,
        callee: &ElabExpr,
        args: &[ElabCallArg],
        env: &Env,
    ) -> EirExpr {
        let Some((map_id, map_name)) = self.map_callee_from_elab(callee, env) else {
            return self.unknown_call_from_elab(callee, args, env);
        };
        let Some(map) = self.map_ir.get(map_id) else {
            return EirExpr::call(
                map_name,
                args.iter()
                    .map(|arg| self.elab_expr(&arg.value, env))
                    .collect(),
            );
        };
        let mut map_env = Env::with_owner(map_id);
        let generic_args = self.elab_generic_type_args(callee);
        let replacements = self.map_generic_replacements(map, &generic_args);
        self.bind_map_elab_args(MapElabArgBinding {
            map,
            args,
            caller_env: env,
            map_env: &mut map_env,
            replacements: &replacements,
        });
        self.map_expr(map.body(), &map_env)
    }

    pub(super) fn map_call_expr(
        &self,
        callee: DefId,
        generic_args: &[MapGenericArg],
        args: &[MapArg],
        env: &Env,
    ) -> EirExpr {
        let Some(map) = self.map_ir.get(callee) else {
            let name = self.program.def_name(callee).unwrap_or("<unknown>");
            return EirExpr::call(
                name,
                args.iter()
                    .map(|arg| self.map_expr(arg.value(), env))
                    .collect(),
            );
        };
        let mut map_env = Env::with_owner(callee);
        let replacements = self.map_generic_replacements(map, generic_args);
        self.bind_map_args(MapArgBinding {
            map,
            args,
            caller_env: env,
            map_env: &mut map_env,
            replacements: &replacements,
        });
        self.map_expr(map.body(), &map_env)
    }

    pub(super) fn map_extension_call_expr(
        &self,
        callee: DefId,
        generic_args: &[MapGenericArg],
        args: &[ElabCallArg],
        env: &Env,
    ) -> EirExpr {
        let Some(map) = self.map_ir.get(callee) else {
            let name = self.program.def_name(callee).unwrap_or("<unknown>");
            return EirExpr::call(
                name,
                args.iter()
                    .map(|arg| self.elab_expr(&arg.value, env))
                    .collect(),
            );
        };
        let mut map_env = Env::with_owner(callee);
        let replacements = self.map_generic_replacements(map, generic_args);
        self.bind_map_elab_args(MapElabArgBinding {
            map,
            args,
            caller_env: env,
            map_env: &mut map_env,
            replacements: &replacements,
        });
        self.map_expr(map.body(), &map_env)
    }

    pub(super) fn map_callee_from_elab(
        &self,
        callee: &ElabExpr,
        env: &Env,
    ) -> Option<(DefId, String)> {
        let owner = env.owner?;
        let root = self.elab_callee_root(callee)?;
        let Some(ElabResolution::Def(def)) = self.program.expr_resolution(owner, root) else {
            return None;
        };
        if self.program.def_kind(def) != Some(ElabDefKind::Map) {
            return None;
        }
        let name = self.program.def_name(def)?.to_string();
        Some((def, name))
    }

    fn unknown_call_from_elab(
        &self,
        callee: &ElabExpr,
        args: &[ElabCallArg],
        env: &Env,
    ) -> EirExpr {
        let Some(name) = self.elab_expr_name(callee) else {
            return EirExpr::unsupported("call callee is not a name");
        };
        EirExpr::call(
            name,
            args.iter()
                .map(|arg| self.elab_expr(&arg.value, env))
                .collect(),
        )
    }

    fn bind_map_elab_args(&self, request: MapElabArgBinding<'_>) {
        let MapElabArgBinding {
            map,
            args,
            caller_env,
            map_env,
            replacements,
        } = request;
        let mut type_lowerer = MapTypeLowerer::new();
        for (idx, param) in map.params().iter().enumerate() {
            let actual = args
                .iter()
                .find(|arg| arg.name.as_deref() == Some(param.name()))
                .or_else(|| args.get(idx));
            if let Some(actual) = actual {
                let ty = param.ty().subst(replacements);
                map_env.insert(
                    param.name(),
                    self.elab_expr(&actual.value, caller_env),
                    type_lowerer.lower_type_ref(&ty),
                );
            }
        }
    }

    fn bind_map_args(&self, request: MapArgBinding<'_>) {
        let MapArgBinding {
            map,
            args,
            caller_env,
            map_env,
            replacements,
        } = request;
        let mut type_lowerer = MapTypeLowerer::new();
        for (idx, param) in map.params().iter().enumerate() {
            let actual = args
                .iter()
                .find(|arg| arg.name() == Some(param.name()))
                .or_else(|| args.get(idx));
            if let Some(actual) = actual {
                let ty = param.ty().subst(replacements);
                map_env.insert(
                    param.name(),
                    self.map_expr(actual.value(), caller_env),
                    type_lowerer.lower_type_ref(&ty),
                );
            }
        }
    }

    fn map_generic_replacements(
        &self,
        map: &MapFunction,
        args: &[MapGenericArg],
    ) -> HashMap<String, MapTypeRef> {
        let mut replacements = HashMap::new();
        for (idx, generic) in map.generics().iter().enumerate() {
            if let Some(arg) = args.get(idx) {
                replacements.insert(generic.to_string(), arg.ty().clone());
            }
        }
        replacements
    }

    pub(super) fn elab_generic_type_args(&self, expr: &ElabExpr) -> Vec<MapGenericArg> {
        let mut current = expr;
        loop {
            match &current.node {
                ElabExprNode::GenericApp { args, .. } => {
                    return args.iter().map(MapGenericArg::from).collect();
                }
                ElabExprNode::Group(expr) => current = expr,
                _ => return Vec::new(),
            }
        }
    }

    fn map_expr(&self, expr: &MapExpr, env: &Env) -> EirExpr {
        match expr {
            MapExpr::Ident(local) => env
                .vars
                .get(local.name())
                .map(|var| var.code.clone())
                .unwrap_or_else(|| EirExpr::ident(local.name())),
            MapExpr::Int(value) => EirExpr::Int(*value),
            MapExpr::Bool(value) => EirExpr::Bool(*value),
            MapExpr::Str(value) => EirExpr::Str(value.clone()),
            MapExpr::Unary { op, expr } => match op {
                MapUnaryOp::Neg => EirExpr::unary(EirUnaryOp::Neg, self.map_expr(expr, env)),
                MapUnaryOp::Not | MapUnaryOp::NotWord => {
                    EirExpr::unary(EirUnaryOp::Not, self.map_expr(expr, env))
                }
                MapUnaryOp::Unsupported => EirExpr::unsupported("unsupported map unary operator"),
                _ => EirExpr::unsupported("unsupported map unary operator"),
            },
            MapExpr::Binary { op, left, right } => {
                let Ok(op) = EirBinaryOp::try_from(*op) else {
                    return EirExpr::unsupported("unsupported map binary operator");
                };
                EirExpr::binary(op, self.map_expr(left, env), self.map_expr(right, env))
            }
            MapExpr::BuiltinHighZ => EirExpr::high_z(),
            MapExpr::BuiltinZero => EirExpr::zero(),
            MapExpr::Call {
                callee,
                generic_args,
                args,
            } => self.map_call_expr(*callee, generic_args, args, env),
            MapExpr::Aggregate { ty, fields } => self.map_aggregate_expr(ty, fields, env),
            MapExpr::Field { base, field } => self.map_field_expr(base, field, env),
            MapExpr::Index { base, index } => {
                EirExpr::index(self.map_expr(base, env), self.map_expr(index, env))
            }
            MapExpr::Match { value, arms } => self.map_match_expr(value, arms, env),
            MapExpr::Select { mode, arms } => self.map_select_expr(*mode, arms, env),
            _ => EirExpr::unsupported("unsupported map expression"),
        }
    }

    fn map_aggregate_expr(&self, ty: &MapTypeRef, fields: &[MapNamedExpr], env: &Env) -> EirExpr {
        let ty = MapTypeLowerer::new().lower_type_ref(ty);
        let Some(bundle) = self.bundle_for_type(env.owner, &ty) else {
            return EirExpr::unsupported("map aggregate type is not a known bundle");
        };
        let mut parts = Vec::new();
        for field in &bundle.fields {
            if let Some(value) = fields.iter().find(|value| value.name() == field.name) {
                parts.push(self.map_expr(value.value(), env));
            } else {
                parts.push(EirExpr::unsupported(format!(
                    "missing map aggregate field {}",
                    field.name
                )));
            }
        }
        EirExpr::Concat(parts)
    }

    fn map_field_expr(&self, base: &MapExpr, field: &str, env: &Env) -> EirExpr {
        let base_code = self.map_expr(base, env);
        let base_key = base_code.fact_key();
        if let MapExpr::Ident(local) = base
            && let Some(var) = env.vars.get(local.name())
            && let Some(expr) = self.view_field_ref(&var.code, &var.ty, field)
        {
            return expr;
        }
        if let MapExpr::Ident(local) = base
            && let Some(var) = env.vars.get(local.name())
            && let Some(expr) = self.bundle_field_ref(env.owner, &var.code, &var.ty, field)
        {
            return expr;
        }
        if let Some(var) = env.vars.get(&base_key)
            && let Some(expr) = self.view_field_ref(&var.code, &var.ty, field)
        {
            return expr;
        }
        if let Some(var) = env.vars.get(&base_key)
            && let Some(expr) = self.bundle_field_ref(env.owner, &var.code, &var.ty, field)
        {
            return expr;
        }
        EirExpr::ident(format!("{base_key}_{field}"))
    }

    fn map_match_expr(&self, target: &MapExpr, arms: &[MapMatchArm], env: &Env) -> EirExpr {
        let mut fallback = None;
        for arm in arms.iter().rev() {
            let value = self.map_expr(arm.value(), env);
            match arm.pattern() {
                MapPattern::Wildcard => fallback = Some(value),
                MapPattern::Ident(name) if name == "default" => fallback = Some(value),
                pattern => {
                    let cond = self.map_match_pattern_condition(target, pattern, env);
                    fallback = Some(match fallback {
                        Some(next) => EirExpr::mux(cond, value, next),
                        None => value,
                    });
                }
            }
        }
        fallback.unwrap_or_else(|| EirExpr::unsupported("empty map match expression"))
    }

    fn map_match_pattern_condition(
        &self,
        target: &MapExpr,
        pattern: &MapPattern,
        env: &Env,
    ) -> EirExpr {
        EirExpr::binary(
            EirBinaryOp::Eq,
            self.map_expr(target, env),
            self.map_match_pattern_value(pattern, env),
        )
    }

    fn map_match_pattern_value(&self, pattern: &MapPattern, env: &Env) -> EirExpr {
        match pattern {
            MapPattern::Path(path) => path
                .last()
                .map(|variant| {
                    env.owner
                        .and_then(|owner| self.program.enum_variant_value(owner, path))
                        .map(EirExpr::Int)
                        .unwrap_or_else(|| EirExpr::ident(variant))
                })
                .unwrap_or_else(|| EirExpr::unsupported("empty map match path pattern")),
            MapPattern::Ident(name) => env
                .owner
                .and_then(|owner| self.program.enum_variant_value_by_name(Some(owner), name))
                .map(EirExpr::Int)
                .unwrap_or_else(|| EirExpr::ident(name)),
            MapPattern::Int(value) => EirExpr::Int(*value),
            MapPattern::Bool(value) => EirExpr::Bool(*value),
            MapPattern::Wildcard => EirExpr::unsupported("wildcard is not a condition"),
            MapPattern::Unsupported => EirExpr::unsupported("unsupported match pattern"),
            _ => EirExpr::unsupported("unsupported match pattern"),
        }
    }

    fn map_select_expr(
        &self,
        mode: crate::map_ir::MapSelectMode,
        arms: &[MapSelectArm],
        env: &Env,
    ) -> EirExpr {
        let mode = match mode {
            crate::map_ir::MapSelectMode::Priority => EirSelectMode::Priority,
            crate::map_ir::MapSelectMode::Unique => EirSelectMode::Unique,
            _ => return EirExpr::unsupported("unsupported map select mode"),
        };
        let mut select_arms = Vec::new();
        let mut default = None;
        for arm in arms {
            let value = self.map_expr(arm.value(), env);
            match arm.pattern() {
                MapExpr::Ident(local) if local.name() == "default" => default = Some(value),
                pattern => select_arms.push(EirSelectArm::new(self.map_expr(pattern, env), value)),
            }
        }
        default.map_or_else(
            || EirExpr::unsupported("map select expression has no default arm"),
            |default| EirExpr::select(mode, select_arms, default),
        )
    }
}

impl TryFrom<MapBinaryOp> for EirBinaryOp {
    type Error = ();

    fn try_from(op: MapBinaryOp) -> Result<Self, Self::Error> {
        match op {
            MapBinaryOp::OrOr => Ok(Self::OrOr),
            MapBinaryOp::AndAnd => Ok(Self::AndAnd),
            MapBinaryOp::Eq => Ok(Self::Eq),
            MapBinaryOp::NotEq => Ok(Self::NotEq),
            MapBinaryOp::Lt => Ok(Self::Lt),
            MapBinaryOp::LtEq => Ok(Self::LtEq),
            MapBinaryOp::Gt => Ok(Self::Gt),
            MapBinaryOp::GtEq => Ok(Self::GtEq),
            MapBinaryOp::Add => Ok(Self::Add),
            MapBinaryOp::Sub => Ok(Self::Sub),
            MapBinaryOp::Mul => Ok(Self::Mul),
            MapBinaryOp::Div => Ok(Self::Div),
            MapBinaryOp::Rem => Ok(Self::Rem),
            MapBinaryOp::Shl => Ok(Self::Shl),
            MapBinaryOp::BitAnd => Ok(Self::BitAnd),
            MapBinaryOp::BitOr => Ok(Self::BitOr),
            MapBinaryOp::BitXor => Ok(Self::BitXor),
            _ => Err(()),
        }
    }
}
