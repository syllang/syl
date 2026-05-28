mod diagnostic;
#[cfg(test)]
mod tests;
mod validator;

use crate::{ParametricHwDesign, ParametricHwModule};

pub use diagnostic::{HwBindingKind, HwValidationDiagnostic, HwValidationReport};
use validator::Validator;

/// Validates a parametric HW design for structural correctness
/// (duplicate names, missing bindings, etc.).
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct HwValidator;

impl HwValidator {
    pub fn new() -> Self {
        Self
    }

    /// Validates the design, returning `Ok(())` or a report of all errors.
    pub fn validate(&self, design: &ParametricHwDesign) -> Result<(), HwValidationReport> {
        let mut validator = Validator::new(design);
        validator.validate();
        validator.finish()
    }
}

/// Normalizes a parametric HW design — validates and wraps for downstream consumption.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct HwNormalizer;

impl HwNormalizer {
    pub fn new() -> Self {
        Self
    }

    /// Validates and returns a `NormalizedParametricHwDesign`.
    pub fn normalize<'a>(
        &self,
        design: &'a ParametricHwDesign,
    ) -> Result<NormalizedParametricHwDesign<'a>, HwValidationReport> {
        HwValidator::new().validate(design)?;
        Ok(NormalizedParametricHwDesign::new(design))
    }
}

/// A validated parametric HW design, ready for the SystemVerilog backend.
#[derive(Debug)]
#[non_exhaustive]
pub struct NormalizedParametricHwDesign<'a> {
    design: &'a ParametricHwDesign,
}

impl<'a> NormalizedParametricHwDesign<'a> {
    fn new(design: &'a ParametricHwDesign) -> Self {
        Self { design }
    }

    /// Returns a reference to the inner parametric design.
    pub fn design(&self) -> &'a ParametricHwDesign {
        self.design
    }

    /// Returns a summary string for debugging.
    pub fn debug_dump(&self) -> String {
        self.design.debug_dump()
    }

    pub fn modules(&self) -> &'a [ParametricHwModule] {
        self.design.modules()
    }
}
