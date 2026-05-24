mod check;
mod lower;
mod sv_ir;

use syl_hw::ParametricHwDesign;
use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CompileError {
    #[error("verilog backend error: {kind}")]
    Verilog { kind: VerilogError },
    #[error("unsupported HWIR for SystemVerilog backend: {message}")]
    UnsupportedHwir { message: String },
}

impl CompileError {
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
    #[error("duplicate module name: {name}")]
    DuplicateModule { name: String },
    #[error("duplicate declaration in {module}: {name}")]
    DuplicateDeclaration { module: String, name: String },
    #[error("unknown instance module in {module}: {instance} -> {target}")]
    UnknownInstanceModule {
        module: String,
        instance: String,
        target: String,
    },
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
    #[error("undeclared signal reference in {module}: {name}")]
    UndeclaredSignalReference { module: String, name: String },
}

#[derive(Debug)]
#[non_exhaustive]
pub struct SystemVerilogBackend;

impl SystemVerilogBackend {
    pub fn new() -> Self {
        Self
    }

    pub fn debug_dump(&self, hwir: &ParametricHwDesign) -> Result<String, CompileError> {
        let design = lower::SvEmitter::new(hwir).lower()?;
        Ok(design.debug_dump())
    }

    pub fn emit(&self, hwir: &ParametricHwDesign) -> Result<String, CompileError> {
        let design = lower::SvEmitter::new(hwir).lower()?;
        let text = design.emit_text();
        check::SvValidator::new(&design)
            .with_source(&text)
            .validate()?;
        Ok(text)
    }
}

impl Default for SystemVerilogBackend {
    fn default() -> Self {
        Self::new()
    }
}
