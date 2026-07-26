mod diagnostic;
#[cfg(test)]
mod tests;
mod validator;

use crate::{ParametricHwDesign, ParametricHwModule};

pub use diagnostic::{HwBindingKind, HwValidationDiagnostic, HwValidationReport};
use validator::Validator;

/// Structural validator for [`ParametricHwDesign`] (backend-neutral checks).
///
/// Catches duplicate names, missing bindings, and similar HWIR invariants
/// before any target-language emission.
///
/// # Main usage flow
///
/// ```text
///  ParametricHwDesign
///         |
///         v
///  HwValidator::validate  -->  Ok(()) | HwValidationReport
///
///  Prefer HwNormalizer when the next step is emission:
///  it validates then wraps the design as NormalizedParametricHwDesign.
/// ```
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

/// Validates a parametric HW design and wraps it for backend consumption.
///
/// # Main usage flow
///
/// ```text
///  ParametricHwDesign
///         |
///         v
///  HwNormalizer::normalize
///         |
///         v
///  NormalizedParametricHwDesign  -->  SystemVerilogBackend (internal path)
///         |                           or any consumer that requires validated HWIR
///         +--> design() / modules()
/// ```
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

/// A validated [`ParametricHwDesign`], ready for backend lowering.
///
/// Proof-carrying wrapper: construction only succeeds after
/// [`HwNormalizer::normalize`] (or equivalent validation).
///
/// # Main usage flow
///
/// ```text
///  HwNormalizer::normalize(&hwir)?
///         |
///         v
///  NormalizedParametricHwDesign
///         |
///         +--> design()   // &ParametricHwDesign
///         +--> modules()
///         +--> debug_dump()
///         |
///         v
///  backend lower / emit
/// ```
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
