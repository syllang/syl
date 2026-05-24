use syl_elab::{CompileError, ElaborationOutput, HardwareCompiler};
use syl_hw::ParametricHwDesign;
use syl_sema::{SemanticCompiler, SemanticSession};
use syl_span::SourceId;
use syl_syntax::AstFile;
use syl_syntax::SourceParser;

#[derive(Debug, Default)]
pub struct MiddleCompiler {
    semantic: SemanticCompiler,
    hardware: HardwareCompiler,
}

impl MiddleCompiler {
    pub fn new() -> Self {
        Self {
            semantic: SemanticCompiler::new(),
            hardware: HardwareCompiler::new(),
        }
    }

    #[allow(dead_code)]
    pub fn compile_files(&self, files: &[AstFile]) -> Result<ParametricHwDesign, CompileError> {
        let hir = self.semantic.session(files).resolve_hir()?;
        let tir = hir.check_tir()?;
        self.hardware.compile_tir(&tir)
    }

    #[allow(dead_code)]
    pub fn output_files(&self, files: &[AstFile]) -> Result<ElaborationOutput, CompileError> {
        let hir = self.semantic.session(files).resolve_hir()?;
        let tir = hir.check_tir()?;
        Ok(self.hardware.output_for_tir(&tir))
    }

    #[allow(dead_code)]
    pub fn compile_sources(&self, sources: &[&str]) -> Result<ParametricHwDesign, String> {
        let files = parse_sources(sources)?;
        self.compile_files(&files).map_err(|err| err.to_string())
    }

    #[allow(dead_code)]
    pub fn output_sources(&self, sources: &[&str]) -> Result<ElaborationOutput, String> {
        let files = parse_sources(sources)?;
        self.output_files(&files).map_err(|err| err.to_string())
    }

    #[allow(dead_code)]
    pub fn session<'files>(&self, files: &'files [AstFile]) -> SemanticSession<'files> {
        self.semantic.session(files)
    }
}

#[allow(dead_code)]
fn parse_sources(sources: &[&str]) -> Result<Vec<AstFile>, String> {
    let mut files = Vec::new();
    for (source_id, source) in sources.iter().enumerate() {
        let file = SourceParser::new_in(source, SourceId::new(source_id))
            .parse_file()
            .map_err(|errs| {
                errs.iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n")
            })?;
        files.push(file);
    }
    Ok(files)
}
