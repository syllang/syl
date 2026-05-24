use super::{EirDesign, EirModule, facts::EirFactCollector, validate::EirValidator};
use crate::CompileError;

#[non_exhaustive]
pub(crate) struct EirDesignAssembler;

impl EirDesignAssembler {
    pub(crate) fn assemble(modules: Vec<EirModule>) -> Result<EirDesign, CompileError> {
        EirValidator::new(&modules).validate()?;
        let facts = EirFactCollector::collect(&modules)?;
        Ok(EirDesign::from_parts(modules, facts))
    }
}
