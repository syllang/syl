mod check;
mod lower;
mod sv_ir;

use syl_hw::{HwNormalizer, ParametricHwDesign};
use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CompileError {
    #[error("invalid HWIR for backend consumption: {report}")]
    InvalidHwir { report: syl_hw::HwValidationReport },
    #[error("verilog backend error: {kind}")]
    Verilog { kind: VerilogError },
    #[error("unsupported HWIR for SystemVerilog backend: {message}")]
    UnsupportedHwir { message: String },
}

impl CompileError {
    pub fn invalid_hwir(report: syl_hw::HwValidationReport) -> Self {
        Self::InvalidHwir { report }
    }

    pub fn verilog(kind: VerilogError) -> Self {
        Self::Verilog { kind }
    }

    pub fn unsupported_hwir(message: impl Into<String>) -> Self {
        Self::UnsupportedHwir {
            message: message.into(),
        }
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum VerilogError {
    #[error("module without name at line {line}")]
    ModuleWithoutName { line: usize },
    #[error("endmodule without matching module at line {line}")]
    UnmatchedEndModule { line: usize },
    #[error("endgenerate without matching generate at line {line}")]
    UnmatchedEndGenerate { line: usize },
    #[error("end without matching begin at line {line}")]
    UnmatchedEnd { line: usize },
    #[error("unmatched delimiter {open} opened at line {open_line}")]
    UnmatchedDelimiter { open: char, open_line: usize },
    #[error(
        "mismatched delimiter {open} opened at line {open_line}, closed by {close} at line {line}"
    )]
    MismatchedDelimiter {
        open: char,
        open_line: usize,
        close: char,
        line: usize,
    },
    #[error("module not closed")]
    UnclosedModule,
    #[error("generate block not closed")]
    UnclosedGenerateBlock,
    #[error("begin block not closed")]
    UnclosedBeginBlock,
    #[error("unsupported function call in {module}: {name}")]
    UnsupportedFunctionCall { module: String, name: String },
}

#[derive(Debug)]
#[non_exhaustive]
pub struct SystemVerilogBackend;

impl SystemVerilogBackend {
    pub fn new() -> Self {
        Self
    }

    pub fn debug_dump(&self, hwir: &ParametricHwDesign) -> Result<String, CompileError> {
        let design = self.compile_sv_design(hwir)?;
        Ok(design.debug_dump())
    }

    pub fn emit(&self, hwir: &ParametricHwDesign) -> Result<String, CompileError> {
        let design = self.compile_sv_design(hwir)?;
        let text = design.emit_text();
        check::SvBackendValidator::new(&design).validate()?;
        check::SvSourceValidator::new().validate(&text)?;
        Ok(text)
    }

    fn compile_sv_design(
        &self,
        hwir: &ParametricHwDesign,
    ) -> Result<sv_ir::SvDesign, CompileError> {
        let normalized = HwNormalizer::new()
            .normalize(hwir)
            .map_err(CompileError::invalid_hwir)?;
        lower::SvEmitter::new(normalized.design()).lower()
    }
}

impl Default for SystemVerilogBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syl_hw::{HwDirection, HwParam, HwPort, ParametricHwModule};

    #[test]
    fn emits_module_param_and_port_docs_as_verilog_comments() {
        let design = ParametricHwDesign::new(vec![
            ParametricHwModule::new(
                "Top",
                vec![HwParam::new("N", "4").with_doc(Some("Width parameter.".to_string()))],
                vec![
                    HwPort::new(HwDirection::In, "N", "x")
                        .with_doc(Some("Input bits.".to_string())),
                ],
                Vec::new(),
            )
            .with_doc(Some("Top module.".to_string())),
        ]);

        let sv = SystemVerilogBackend::new()
            .emit(&design)
            .expect("documented HWIR should emit");

        assert!(sv.contains("// Top module.\nmodule Top"));
        assert!(sv.contains("    // Width parameter.\n    parameter N = 4"));
        assert!(sv.contains("    // Input bits.\n    input [(N)-1:0] x"));
    }
}
