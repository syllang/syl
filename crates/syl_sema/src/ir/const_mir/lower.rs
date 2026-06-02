use super::{ConstExpr, ConstLocalRef, ConstNamedExpr, ConstStructKind};
use crate::{
    hir::resolve::HirResolution,
    hir::{HirBodyExpr, HirExprNode, HirNamedExpr},
    ir::{
        const_mir::ConstKind,
        mir::{MirBinaryOp, MirTypeRef, MirUnaryOp},
    },
};
use std::collections::BTreeSet;
use syl_hir::{DefId, LocalId};
use syl_span::Span;

use super::ConstMirLoweringContext;

pub(super) struct ExprLowerer<'a> {
    ctx: &'a dyn ConstMirLoweringContext,
    owner: DefId,
    unsupported: bool,
    unsupported_span: Option<Span>,
    const_stack: BTreeSet<DefId>,
}

impl<'a> ExprLowerer<'a> {
    pub(super) fn new(ctx: &'a dyn ConstMirLoweringContext, owner: DefId) -> Self {
        Self {
            ctx,
            owner,
            unsupported: false,
            unsupported_span: None,
            const_stack: ctx
                .is_const_owner(owner)
                .then_some(owner)
                .into_iter()
                .collect(),
        }
    }

    pub(super) fn is_unsupported(&self) -> bool {
        self.unsupported
    }

    pub(super) fn unsupported_span(&self) -> Option<Span> {
        self.unsupported_span
    }

    pub(super) fn mark_unsupported(&mut self, span: Span) {
        self.unsupported = true;
        if self.unsupported_span.is_none() {
            self.unsupported_span = Some(span);
        }
    }

    pub(super) fn lower_local_assignment(
        &mut self,
        target: &HirBodyExpr,
        value: &HirBodyExpr,
    ) -> Option<(ConstLocalRef, ConstExpr)> {
        let HirExprNode::Ident(name) = &target.node else {
            return None;
        };
        Some((
            self.local_ref_for_expr(target, name),
            self.lower_expr(value),
        ))
    }

    pub(super) fn lower_expr(&mut self, expr: &HirBodyExpr) -> ConstExpr {
        match &expr.node {
            HirExprNode::Ident(name) => match self.ctx.expr_resolution(self.owner, expr) {
                Ok(Some(HirResolution::Def(def))) => {
                    let Some(item) = self.ctx.const_by_def(def) else {
                        return self.unsupported_expr(expr.span(), expr.id());
                    };
                    if !self.const_stack.insert(def) {
                        return self.unsupported_expr(expr.span(), expr.id());
                    }
                    let lowered = self.lower_expr(&item.value).with_origin(expr.id());
                    self.const_stack.remove(&def);
                    lowered
                }
                _ => ConstExpr::local(self.local_ref_for_expr(expr, name), expr.span())
                    .with_origin(expr.id()),
            },
            HirExprNode::Int(value) => ConstExpr::nat(*value, expr.span()).with_origin(expr.id()),
            HirExprNode::Bool(value) => {
                ConstExpr::bool_value(*value, expr.span()).with_origin(expr.id())
            }
            HirExprNode::Group(inner) => self.lower_expr(inner),
            HirExprNode::Unary {
                op, expr: inner, ..
            } => {
                let op = MirUnaryOp::from(*op);
                if matches!(op, MirUnaryOp::Unsupported) {
                    return self.unsupported_expr(expr.span(), expr.id());
                }
                ConstExpr::unary(op, self.lower_expr(inner), expr.span()).with_origin(expr.id())
            }
            HirExprNode::Binary {
                op, left, right, ..
            } => {
                let op = MirBinaryOp::from(*op);
                if matches!(op, MirBinaryOp::Assign | MirBinaryOp::Unsupported) {
                    return self.unsupported_expr(expr.span(), expr.id());
                }
                ConstExpr::binary(
                    op,
                    self.lower_expr(left),
                    self.lower_expr(right),
                    expr.span(),
                )
                .with_origin(expr.id())
            }
            HirExprNode::Call { callee, args } => {
                if let Some((method, receiver)) = self.ctx.extension_method_call(self.owner, callee)
                    && self.ctx.function_exists(method)
                {
                    let mut lowered_args = vec![self.lower_expr(receiver)];
                    lowered_args.extend(args.iter().map(|arg| self.lower_expr(&arg.value)));
                    return ConstExpr::call(method, lowered_args, expr.span())
                        .with_origin(expr.id());
                }
                let Some(root) = self.callee_root(callee) else {
                    return self.unsupported_expr(expr.span(), expr.id());
                };
                let Ok(Some(HirResolution::Def(def))) = self.ctx.expr_resolution(self.owner, root)
                else {
                    return self.unsupported_expr(expr.span(), expr.id());
                };
                if !self.ctx.function_exists(def) {
                    return self.unsupported_expr(expr.span(), expr.id());
                }
                ConstExpr::call(
                    def,
                    args.iter().map(|arg| self.lower_expr(&arg.value)).collect(),
                    expr.span(),
                )
                .with_origin(expr.id())
            }
            HirExprNode::Aggregate { ty, fields } => match self.const_kind_for_type(ty) {
                Some(ConstKind::Struct(kind)) => {
                    ConstExpr::aggregate(kind, self.lower_named_exprs(fields), expr.span())
                        .with_origin(expr.id())
                }
                _ => self.unsupported_expr(expr.span(), expr.id()),
            },
            HirExprNode::Field { base, field } => self.enum_variant_expr(expr).unwrap_or_else(|| {
                ConstExpr::field(self.lower_expr(base), field.clone(), expr.span())
                    .with_origin(expr.id())
            }),
            HirExprNode::GenericApp { callee, .. } => self.lower_expr(callee),
            HirExprNode::Unsupported => self.unsupported_expr(expr.span(), expr.id()),
            _ => self.unsupported_expr(expr.span(), expr.id()),
        }
    }

    pub(super) fn local_ref_for_decl(&self, id: Option<LocalId>, name: &str) -> ConstLocalRef {
        ConstLocalRef::new(id, name.to_string())
    }

    pub(super) fn const_kind_for_type(&self, ty: &MirTypeRef) -> Option<ConstKind> {
        let mut current = ty;
        loop {
            if let Some(name) = current.path_name() {
                return match name {
                    "nat" => Some(ConstKind::Nat),
                    "bool" => Some(ConstKind::Bool),
                    _ => self.struct_kind_for_type(current).map(ConstKind::Struct),
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

    fn unsupported_expr(&mut self, span: Span, origin: syl_hir::ExprId) -> ConstExpr {
        self.mark_unsupported(span);
        ConstExpr::unsupported(span).with_origin(origin)
    }

    fn enum_variant_expr(&mut self, expr: &HirBodyExpr) -> Option<ConstExpr> {
        self.ctx
            .enum_variant_value(expr)
            .map(|value| ConstExpr::nat(value, expr.span()).with_origin(expr.id()))
    }

    fn lower_named_exprs(&mut self, fields: &[HirNamedExpr]) -> Vec<ConstNamedExpr> {
        fields
            .iter()
            .map(|field| ConstNamedExpr::new(field.name.clone(), self.lower_expr(&field.value)))
            .collect()
    }

    fn struct_kind_for_type(&self, ty: &MirTypeRef) -> Option<ConstStructKind> {
        self.ctx
            .hir()
            .type_def_for_mir_type(self.owner, ty)
            .filter(|def| self.ctx.hir().structs.contains_key(def))
            .map(ConstStructKind::new)
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

    fn local_ref_for_expr(&self, expr: &HirBodyExpr, name: &str) -> ConstLocalRef {
        let id = self
            .ctx
            .expr_resolution(self.owner, expr)
            .ok()
            .flatten()
            .and_then(|resolution| match resolution {
                HirResolution::Local(id) => Some(id),
                HirResolution::Def(_) => None,
                _ => None,
            });
        ConstLocalRef::new(id, name.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::super::{ConstExprKind, ConstMirBuilder, ConstMirLoweringContext};
    use super::*;
    use crate::hir::{HirConstItem, HirDesign, lower::HirResolver};
    use syl_hir::DefId;
    use syl_span::SourceId;
    use syl_syntax::SourceParser;

    struct FakeContext {
        hir: HirDesign,
    }

    impl ConstMirLoweringContext for FakeContext {
        fn hir(&self) -> &HirDesign {
            &self.hir
        }

        fn is_const_owner(&self, owner: DefId) -> bool {
            self.hir.consts.contains_key(&owner)
        }

        fn expr_resolution(
            &self,
            _owner: DefId,
            expr: &HirBodyExpr,
        ) -> Result<Option<crate::hir::resolve::HirResolution>, crate::CompileError> {
            Ok(self.hir.expr_resolutions.get(&expr.id()).copied())
        }

        fn const_by_def(&self, def: DefId) -> Option<&HirConstItem> {
            self.hir.consts.get(&def)
        }

        fn function_exists(&self, def: DefId) -> bool {
            self.hir.fns.contains_key(&def)
        }

        fn extension_method_call<'a>(
            &self,
            _owner: DefId,
            _callee: &'a HirBodyExpr,
        ) -> Option<(DefId, &'a HirBodyExpr)> {
            None
        }

        fn enum_variant_value(&self, _expr: &HirBodyExpr) -> Option<u64> {
            None
        }
    }

    #[test]
    fn lower_const_expr_uses_context_lookup() {
        let hir = resolve_hir(
            r#"
const answer = 7

fn use_answer() -> nat {
    answer
}
"#,
        );
        let owner = def_id(&hir, "use_answer");
        let lookup_expr = hir
            .fns
            .get(&owner)
            .and_then(|item| item.body.tail.as_ref())
            .expect("fixture function must have a tail expression")
            .clone();
        let lookup_id = lookup_expr.id();
        let ctx = FakeContext { hir };

        let lowered = ConstMirBuilder::with_context(&ctx).lower_const_expr(owner, &lookup_expr);

        match lowered.kind() {
            ConstExprKind::Nat(value) => assert_eq!(*value, 7),
            _ => panic!("expected nat const"),
        }
        assert_eq!(lowered.origin(), Some(lookup_id));
    }

    #[test]
    fn lower_struct_aggregate_expr() {
        let hir = resolve_hir(
            r#"
struct Params {
    width: nat,
    enabled: bool,
}

const params = Params { width: 7, enabled: true }
"#,
        );
        let owner = def_id(&hir, "params");
        let value_expr = hir
            .consts
            .get(&owner)
            .map(|item| item.value.clone())
            .expect("fixture const must exist");
        let lowered = ConstMirBuilder::with_context(&FakeContext { hir }).lower_const_expr(owner, &value_expr);

        match lowered.kind() {
            ConstExprKind::Aggregate { kind, fields } => {
                assert_eq!(kind.def(), def_id(&resolve_hir(
                    r#"
struct Params {
    width: nat,
    enabled: bool,
}

const params = Params { width: 7, enabled: true }
"#,
                ), "Params"));
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].name(), "width");
                assert!(matches!(fields[0].value().kind(), ConstExprKind::Nat(7)));
                assert_eq!(fields[1].name(), "enabled");
                assert!(matches!(fields[1].value().kind(), ConstExprKind::Bool(true)));
            }
            _ => panic!("expected aggregate const"),
        }
    }

    #[test]
    fn lower_struct_field_expr() {
        let hir = resolve_hir(
            r#"
struct Params {
    width: nat,
    enabled: bool,
}

const params = Params { width: 7, enabled: true }

fn use_width() -> nat {
    params.width
}
"#,
        );
        let owner = def_id(&hir, "use_width");
        let field_expr = hir
            .fns
            .get(&owner)
            .and_then(|item| item.body.tail.as_ref())
            .cloned()
            .expect("fixture function must have a field tail expression");
        let lowered = ConstMirBuilder::with_context(&FakeContext { hir }).lower_const_expr(owner, &field_expr);

        match lowered.kind() {
            ConstExprKind::Field { base, field } => {
                assert_eq!(field, "width");
                assert!(matches!(base.kind(), ConstExprKind::Aggregate { .. }));
            }
            _ => panic!("expected field projection const"),
        }
    }

    fn resolve_hir(source: &str) -> HirDesign {
        let file = SourceParser::new_in(source, SourceId::new(0))
            .parse_file()
            .expect("fixture must parse");
        let files = [file];
        HirResolver::new(&files)
            .resolve()
            .expect("fixture must resolve HIR")
    }

    fn def_id(hir: &HirDesign, name: &str) -> DefId {
        hir.defs
            .iter()
            .find(|def| def.name == name)
            .unwrap_or_else(|| panic!("missing definition {name}"))
            .id
    }
}
