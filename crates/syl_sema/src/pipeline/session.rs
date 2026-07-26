use super::{
    analysis::HirAnalysis,
    input::SemanticSourceFile,
    output::{HirAnalysisOutput, SemanticOutput},
};
use crate::{CompileError, HirResolver};
use syl_span::Diagnostic;
use syl_syntax::AstFile;

/// Entry point for semantic analysis over one or more parsed AST files.
///
/// The session only holds source inputs. Callers choose how far to run the
/// pipeline (HIR only vs HIR+TIR) and whether errors fail-fast or accumulate.
///
/// # Main usage flow
///
/// ```text
///  AST files / SemanticSourceFile[]
///              |
///              v
///    +---------------------+
///    |   SemanticSession   |   new(&[AstFile])
///    |  (input container)  |   new_sources(sources)
///    +----------+----------+
///               |
///       +-------+---------------------------+
///       |                                   |
///       v                                   v
///  resolve_hir()                  resolve_hir_partial()
///  Result<HirAnalysis, E>         HirAnalysisOutput
///  (fail-fast)                    (HIR + diagnostics)
///       |                                   |
///       +----------------+------------------+
///                        |
///                        v
///                 HirAnalysis
///            /       |        \
///           /        |         \
///          v         v          v
///   check_tir()  check_tir_  definition_at /
///   (fail-fast)  partial()   hover_at /
///                (diags)     completion_*
///          |         |
///          +----+----+
///               |
///               v
///          TirAnalysis
///       (facts, opaque
///        summaries, ...)
///
///  One-shot HIR+TIR (no intermediate handle):
///
///    session.check()  ------------>  SemanticOutput
///       |                               |- tir()
///       |                               |- facts()
///       +- diagnostics() shortcut       |- opaque_summaries()
///                                       `- diagnostics()
/// ```
///
/// Typical paths:
/// - **Strict compile**: `resolve_hir()?` then `hir.check_tir()?` (or just
///   `check()` when only the final TIR / facts / diagnostics are needed).
/// - **IDE / partial**: `resolve_hir_partial()` for HIR queries under errors;
///   `check()` when TIR facts are useful even if earlier stages reported
///   diagnostics.
#[derive(Debug)]
#[non_exhaustive]
pub struct SemanticSession<'files> {
    sources: Vec<SemanticSourceFile<'files>>,
}

impl<'files> SemanticSession<'files> {
    /// Build a session from AST files, assigning default module paths
    /// `file0`, `file1`, ...
    pub fn new(files: &'files [AstFile]) -> Self {
        let sources = files
            .iter()
            .enumerate()
            .map(|(index, ast)| SemanticSourceFile::new(vec![format!("file{index}")], ast))
            .collect();
        Self { sources }
    }

    /// Build a session from sources that already carry module paths.
    pub fn new_sources(sources: Vec<SemanticSourceFile<'files>>) -> Self {
        Self { sources }
    }

    /// Lower AST → HIR, failing on the first resolution error.
    pub fn resolve_hir(&self) -> Result<HirAnalysis, CompileError> {
        HirResolver::new_sources(self.semantic_sources())
            .resolve()
            .map(HirAnalysis::new)
    }

    /// Lower AST → HIR while collecting resolution diagnostics.
    ///
    /// Always returns a stage handle so callers can still query partial HIR.
    pub fn resolve_hir_partial(&self) -> HirAnalysisOutput {
        let (design, errors) = HirResolver::new_sources(self.semantic_sources()).resolve_partial();
        let diagnostics = errors.into_iter().map(Diagnostic::from).collect();
        HirAnalysisOutput::new(HirAnalysis::new(design), diagnostics)
    }

    /// Run HIR resolution then TIR checking, collecting diagnostics.
    ///
    /// On HIR failure, `tir()` is `None` and only HIR diagnostics are present.
    pub fn check(&self) -> SemanticOutput {
        let hir = match self.resolve_hir_collect() {
            Ok(hir) => hir,
            Err(errors) => {
                return SemanticOutput::new(
                    None,
                    errors.into_iter().map(Diagnostic::from).collect(),
                );
            }
        };
        let tir = hir.check_tir_partial();
        let diagnostics = tir.diagnostics().to_vec();
        SemanticOutput::new(tir.into_stage(), diagnostics)
    }

    /// Convenience for `check().diagnostics().to_vec()`.
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.check().diagnostics().to_vec()
    }

    fn resolve_hir_collect(&self) -> Result<HirAnalysis, Vec<CompileError>> {
        HirResolver::new_sources(self.semantic_sources())
            .resolve_collect()
            .map(HirAnalysis::new)
    }

    fn semantic_sources(&self) -> Vec<SemanticSourceFile<'files>> {
        self.sources
            .iter()
            .map(|source| SemanticSourceFile::new(source.module_path().to_vec(), source.ast()))
            .collect()
    }
}
