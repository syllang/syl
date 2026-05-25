use std::collections::BTreeSet;
use syl_elab::{CompileError, ElaborationOutput, HardwareCompiler};
use syl_hw::ParametricHwDesign;
use syl_sema::{OpaqueSummaryTable, SemanticCompiler, SemanticSession, SemanticSourceFile};
use syl_span::SourceId;
use syl_syntax::AstFile;
use syl_syntax::SourceParser;

#[derive(Debug, Default)]
pub struct MiddleCompiler {
    semantic: SemanticCompiler,
    hardware: HardwareCompiler,
}

#[allow(dead_code)]
pub struct SvOutputProbe<'a> {
    source: &'a str,
}

#[allow(dead_code)]
impl<'a> SvOutputProbe<'a> {
    pub fn new(source: &'a str) -> Self {
        Self { source }
    }

    pub fn module_names(&self) -> Result<BTreeSet<String>, String> {
        let mut names = BTreeSet::new();
        let mut endmodule_count = 0usize;
        for line in self.source.lines().map(str::trim) {
            if let Some(rest) = line.strip_prefix("module ") {
                let name = self.module_name_from_header(rest)?;
                if !names.insert(name.clone()) {
                    return Err(format!("duplicate module declaration {name}"));
                }
            } else if line == "endmodule" {
                endmodule_count += 1;
            }
        }
        if names.len() != endmodule_count {
            return Err(format!(
                "module declarations ({}) do not match endmodule count ({endmodule_count})",
                names.len()
            ));
        }
        Ok(names)
    }

    fn module_name_from_header(&self, header: &str) -> Result<String, String> {
        let name = header
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
            .collect::<String>();
        if name.is_empty() {
            return Err(format!("missing module name in header {header}"));
        }
        Ok(name)
    }
}

impl MiddleCompiler {
    pub fn new() -> Self {
        Self {
            semantic: SemanticCompiler::new(),
            hardware: HardwareCompiler::new(),
        }
    }

    #[allow(dead_code)]
    pub fn with_opaque_summaries(opaque_summaries: OpaqueSummaryTable) -> Self {
        Self {
            semantic: SemanticCompiler::new(),
            hardware: HardwareCompiler::with_opaque_summaries(opaque_summaries),
        }
    }

    #[allow(dead_code)]
    pub fn compile_files(&self, files: &[AstFile]) -> Result<ParametricHwDesign, CompileError> {
        let hir = self.semantic.session(files).resolve_hir()?;
        let tir = hir.check_tir()?;
        self.hardware.compile_tir(&tir)
    }

    #[allow(dead_code)]
    pub fn compile_files_with_paths(
        &self,
        files: &[(Vec<String>, AstFile)],
    ) -> Result<ParametricHwDesign, CompileError> {
        let sources = files
            .iter()
            .map(|(path, ast)| SemanticSourceFile::new(path.clone(), ast))
            .collect();
        let hir = self.semantic.session_sources(sources).resolve_hir()?;
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
    pub fn output_files_with_paths(
        &self,
        files: &[(Vec<String>, AstFile)],
    ) -> Result<ElaborationOutput, CompileError> {
        let sources = files
            .iter()
            .map(|(path, ast)| SemanticSourceFile::new(path.clone(), ast))
            .collect();
        let hir = self.semantic.session_sources(sources).resolve_hir()?;
        let tir = hir.check_tir()?;
        Ok(self.hardware.output_for_tir(&tir))
    }

    #[allow(dead_code)]
    pub fn compile_sources(&self, sources: &[&str]) -> Result<ParametricHwDesign, String> {
        let files = parse_sources(sources)?;
        self.compile_files(&files).map_err(|err| err.to_string())
    }

    #[allow(dead_code)]
    pub fn compile_sources_with_paths(
        &self,
        sources: &[(Vec<String>, &str)],
    ) -> Result<ParametricHwDesign, String> {
        let files = parse_sources_with_paths(sources)?;
        self.compile_files_with_paths(&files)
            .map_err(|err| err.to_string())
    }

    #[allow(dead_code)]
    pub fn output_sources(&self, sources: &[&str]) -> Result<ElaborationOutput, String> {
        let files = parse_sources(sources)?;
        self.output_files(&files).map_err(|err| err.to_string())
    }

    #[allow(dead_code)]
    pub fn output_sources_with_paths(
        &self,
        sources: &[(Vec<String>, &str)],
    ) -> Result<ElaborationOutput, String> {
        let files = parse_sources_with_paths(sources)?;
        self.output_files_with_paths(&files)
            .map_err(|err| err.to_string())
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

#[allow(dead_code)]
fn parse_sources_with_paths(
    sources: &[(Vec<String>, &str)],
) -> Result<Vec<(Vec<String>, AstFile)>, String> {
    let mut files = Vec::new();
    for (source_id, (path, source)) in sources.iter().enumerate() {
        let file = SourceParser::new_in(source, SourceId::new(source_id))
            .parse_file()
            .map_err(|errs| {
                errs.iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n")
            })?;
        files.push((path.clone(), file));
    }
    Ok(files)
}
