use super::super::{BasicBlock, BlockId, ConstFunctionParts};
use super::*;
use crate::ir::mir::MirTypeRef;
use syl_hir::DefId;

#[test]
fn evaluator_uses_custom_type_oracle() {
    struct FakeOracle;

    impl ConstTypeOracle for FakeOracle {
        fn const_kind_for_type(&self, ty: &MirTypeRef) -> Option<ConstKind> {
            match ty.type_name() {
                Some("Token") => Some(ConstKind::Bool),
                _ => None,
            }
        }
    }

    let oracle = FakeOracle;
    let evaluator = ConstEvaluator::with_dependencies(&NoopFunctionStore, &oracle);
    let ty = MirTypeRef::path_type(vec!["Token".to_string()], Span::new(0, 1));

    assert_eq!(evaluator.kind_for_type(&ty), Some(ConstKind::Bool));
}

#[test]
fn evaluator_uses_custom_function_store() {
    struct FakeFunctionStore {
        def: DefId,
        function: ConstFunction,
    }

    impl ConstFunctionStore for FakeFunctionStore {
        fn function(&self, def: DefId) -> Option<&ConstFunction> {
            if def == self.def {
                Some(&self.function)
            } else {
                None
            }
        }
    }

    struct NoopOracle;

    impl ConstTypeOracle for NoopOracle {
        fn const_kind_for_type(&self, _ty: &MirTypeRef) -> Option<ConstKind> {
            None
        }
    }

    let def = DefId::new(7);
    let span = Span::new(0, 1);
    let function = ConstFunction::new(ConstFunctionParts {
        def,
        name: "answer".to_string(),
        params: Vec::new(),
        ret_kind: Some(ConstKind::Nat),
        locals: Vec::new(),
        blocks: vec![BasicBlock::new(
            BlockId::new(0),
            Vec::new(),
            Terminator::Return(Some(ConstExpr::nat(7, span))),
        )],
        entry: BlockId::new(0),
        span,
        unsupported: false,
        unsupported_span: None,
    });
    let store = FakeFunctionStore { def, function };
    let oracle = NoopOracle;
    let mut evaluator = ConstEvaluator::with_dependencies(&store, &oracle);
    let expr = ConstExpr::call(def, Vec::new(), span);
    let mut env = ConstEvalEnv::with_owner(None);

    assert_eq!(
        evaluator
            .expr_value(&expr, &mut env)
            .expect("fake function store must resolve the call"),
        ConstValue::Nat(7)
    );
}

struct NoopFunctionStore;

impl ConstFunctionStore for NoopFunctionStore {
    fn function(&self, _def: DefId) -> Option<&ConstFunction> {
        None
    }
}
