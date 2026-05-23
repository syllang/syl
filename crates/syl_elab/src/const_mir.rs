mod lower;

pub(crate) use syl_sema::const_mir::{ConstExpr, ConstFunction};

use crate::{CompileError, tir::TirDesign};

#[non_exhaustive]
pub(crate) struct ConstMirProgram {
    inner: syl_sema::const_mir::ConstMirProgram,
}

impl ConstMirProgram {
    fn new(inner: syl_sema::const_mir::ConstMirProgram) -> Self {
        Self { inner }
    }

    pub(crate) fn evaluator(&self) -> syl_sema::const_eval::ConstEvaluator<'_> {
        self.inner.evaluator()
    }

    pub(crate) fn function(&self, id: syl_hir::DefId) -> Option<&ConstFunction> {
        self.inner.function(id)
    }

    pub(crate) fn node_count(&self) -> usize {
        self.inner.node_count()
    }

    pub(crate) fn local_ref_count(&self) -> usize {
        self.inner.local_ref_count()
    }

    pub(crate) fn resolved_local_ref_count(&self) -> usize {
        self.inner.resolved_local_ref_count()
    }
}

#[non_exhaustive]
pub(crate) struct ConstMirBuilder<'a> {
    inner: syl_sema::const_mir::ConstMirBuilder<'a>,
}

impl<'a> ConstMirBuilder<'a> {
    pub(crate) fn new(tir: &'a TirDesign) -> Self {
        Self {
            inner: syl_sema::const_mir::ConstMirBuilder::new(tir),
        }
    }

    pub(crate) fn build(self) -> Result<ConstMirProgram, CompileError> {
        self.inner.build().map(ConstMirProgram::new)
    }
}
