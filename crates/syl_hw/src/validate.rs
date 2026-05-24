mod diagnostic;
#[cfg(test)]
mod tests;
mod validator;

use crate::{ParametricHwDesign, ParametricHwModule};

pub use diagnostic::{HwBindingKind, HwValidationDiagnostic, HwValidationReport};
use validator::Validator;

#[derive(Debug, Default)]
#[non_exhaustive]
pub struct HwValidator;

impl HwValidator {
    pub fn new() -> Self {
        Self
    }

    pub fn validate(&self, design: &ParametricHwDesign) -> Result<(), HwValidationReport> {
        let mut validator = Validator::new(design);
        validator.validate();
        validator.finish()
    }
}

#[derive(Debug, Default)]
#[non_exhaustive]
pub struct HwNormalizer;

impl HwNormalizer {
    pub fn new() -> Self {
        Self
    }

    pub fn normalize<'a>(
        &self,
        design: &'a ParametricHwDesign,
    ) -> Result<NormalizedParametricHwDesign<'a>, HwValidationReport> {
        HwValidator::new().validate(design)?;
        Ok(NormalizedParametricHwDesign::new(design))
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct NormalizedParametricHwDesign<'a> {
    design: &'a ParametricHwDesign,
}

impl<'a> NormalizedParametricHwDesign<'a> {
    fn new(design: &'a ParametricHwDesign) -> Self {
        Self { design }
    }

    pub fn design(&self) -> &'a ParametricHwDesign {
        self.design
    }

    pub fn debug_dump(&self) -> String {
        self.design.debug_dump()
    }

    pub fn modules(&self) -> &'a [ParametricHwModule] {
        self.design.modules()
    }
}
