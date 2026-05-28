use crate::{CancellationToken, ProjectError};
use std::{
    fmt,
    sync::{Mutex, OnceLock},
};
use syl_elab::{ElaborationOutput, HardwareCompiler};
use syl_sema::{
    HirAnalysis, HirAnalysisOutput, OpaqueSummaryTable, SemanticCompiler, SemanticSourceFile,
    StageOutput, TirAnalysis,
};
use syl_span::Diagnostic;
use syl_syntax::AstFile;

const CANCELLATION_HANDOFF_YIELDS: usize = 1024;

#[non_exhaustive]
pub struct SemanticCache {
    sources: Vec<SemanticCacheSource>,
    opaque_summary_overlay: OpaqueSummaryTable,
    hir: OnceLock<HirAnalysisOutput>,
    tir: OnceLock<StageOutput<TirAnalysis>>,
    opaque_summaries: OnceLock<OpaqueSummaryTable>,
    elaboration_init: Mutex<()>,
    elaboration: OnceLock<ElaborationOutput>,
    diagnostics: OnceLock<Vec<Diagnostic>>,
}

impl SemanticCache {
    pub fn new_sources(
        sources: Vec<SemanticCacheSource>,
        opaque_summary_overlay: OpaqueSummaryTable,
    ) -> Self {
        Self {
            sources,
            opaque_summary_overlay,
            hir: OnceLock::new(),
            tir: OnceLock::new(),
            opaque_summaries: OnceLock::new(),
            elaboration_init: Mutex::new(()),
            elaboration: OnceLock::new(),
            diagnostics: OnceLock::new(),
        }
    }

    pub(crate) fn hir_output(&self) -> &HirAnalysisOutput {
        self.hir.get_or_init(|| {
            let sources = self.semantic_sources();
            SemanticCompiler::new()
                .session_sources(sources)
                .resolve_hir_partial()
        })
    }

    pub(crate) fn hir_output_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<&HirAnalysisOutput, ProjectError> {
        let cancellation = || token.is_cancelled();
        self.hir_output_with_probe(&cancellation)
    }

    fn hir_output_with_probe<F: Fn() -> bool + ?Sized>(
        &self,
        token: &F,
    ) -> Result<&HirAnalysisOutput, ProjectError> {
        let cached = self.hir.get().is_some();
        self.check_cancellation_before(token, cached)?;
        let output = self.hir_output();
        if !cached {
            self.check_cancellation_after_uncached_stage(token)?;
        }
        Ok(output)
    }

    pub(crate) fn hir(&self) -> &HirAnalysis {
        self.hir_output().stage()
    }

    pub(crate) fn hir_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<&HirAnalysis, ProjectError> {
        Ok(self.hir_output_with_token(token)?.stage())
    }

    fn tir_output(&self) -> &StageOutput<TirAnalysis> {
        self.tir.get_or_init(|| self.hir().check_tir_partial())
    }

    fn tir_output_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<&StageOutput<TirAnalysis>, ProjectError> {
        let cancellation = || token.is_cancelled();
        self.tir_output_with_probe(&cancellation)
    }

    fn tir_output_with_probe<F: Fn() -> bool + ?Sized>(
        &self,
        token: &F,
    ) -> Result<&StageOutput<TirAnalysis>, ProjectError> {
        let _ = self.hir_output_with_probe(token)?;
        let cached = self.tir.get().is_some();
        self.check_cancellation_before(token, cached)?;
        let output = self.tir_output();
        if !cached {
            self.check_cancellation_after_uncached_stage(token)?;
        }
        Ok(output)
    }

    pub(crate) fn tir(&self) -> Option<&TirAnalysis> {
        self.tir_output().partial_stage()
    }

    pub(crate) fn tir_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<Option<&TirAnalysis>, ProjectError> {
        Ok(self.tir_output_with_token(token)?.partial_stage())
    }

    pub(crate) fn opaque_summaries(&self) -> Option<&OpaqueSummaryTable> {
        if let Some(tir) = self.tir() {
            return Some(
                self.opaque_summaries
                    .get_or_init(|| tir.opaque_summaries().merged(&self.opaque_summary_overlay)),
            );
        }
        (!self.opaque_summary_overlay.is_empty()).then_some(&self.opaque_summary_overlay)
    }

    pub(crate) fn opaque_summaries_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<Option<&OpaqueSummaryTable>, ProjectError> {
        if let Some(tir) = self.tir_with_token(token)? {
            return Ok(Some(self.opaque_summaries.get_or_init(|| {
                tir.opaque_summaries().merged(&self.opaque_summary_overlay)
            })));
        }
        Ok((!self.opaque_summary_overlay.is_empty()).then_some(&self.opaque_summary_overlay))
    }

    pub(crate) fn elaboration_output(&self) -> Option<&ElaborationOutput> {
        if !self.hir_output().diagnostics().is_empty() {
            return None;
        }
        if !self.tir_output().diagnostics().is_empty() {
            return None;
        }
        let tir = self.tir()?;
        Some(self.elaboration.get_or_init(|| {
            HardwareCompiler::with_opaque_summaries(self.opaque_summary_overlay.clone())
                .output_for_tir(tir)
        }))
    }

    pub(crate) fn elaboration_output_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<Option<&ElaborationOutput>, ProjectError> {
        let cancellation = || token.is_cancelled();
        self.elaboration_output_with_probe(&cancellation)
    }

    fn elaboration_output_with_probe<F: Fn() -> bool + ?Sized>(
        &self,
        token: &F,
    ) -> Result<Option<&ElaborationOutput>, ProjectError> {
        if !self.hir_output_with_probe(token)?.diagnostics().is_empty() {
            return Ok(None);
        }
        let tir = self.tir_output_with_probe(token)?;
        if !tir.diagnostics().is_empty() {
            return Ok(None);
        }
        let Some(tir) = tir.partial_stage() else {
            return Ok(None);
        };
        let cached = self.elaboration.get().is_some();
        self.check_cancellation_before(token, cached)?;
        if cached {
            return Ok(Some(self.elaboration.get().expect(
                "cached elaboration output must exist before returning it",
            )));
        }

        let _guard = self
            .elaboration_init
            .lock()
            .expect("semantic elaboration init lock poisoned; cache state may be inconsistent");
        if let Some(output) = self.elaboration.get() {
            return Ok(Some(output));
        }

        let output = HardwareCompiler::with_opaque_summaries(self.opaque_summary_overlay.clone())
            .output_for_tir_with_token(tir, token);
        self.check_cancellation_after_uncached_stage(token)?;
        let _ = self.elaboration.set(output);
        Ok(Some(self.elaboration.get().expect(
            "elaboration cache must be initialized before returning output",
        )))
    }

    pub(crate) fn diagnostics(&self) -> Vec<Diagnostic> {
        self.diagnostics
            .get_or_init(|| {
                if !self.hir_output().diagnostics().is_empty() {
                    return self.hir_output().diagnostics().to_vec();
                }
                let tir = self.tir_output();
                if !tir.diagnostics().is_empty() {
                    return tir.diagnostics().to_vec();
                }
                self.elaboration_output()
                    .map(|output| output.diagnostics().to_vec())
                    .unwrap_or_default()
            })
            .clone()
    }

    pub(crate) fn hir_diagnostics_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<&[Diagnostic], ProjectError> {
        Ok(self.hir_output_with_token(token)?.diagnostics())
    }

    pub(crate) fn tir_diagnostics_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<&[Diagnostic], ProjectError> {
        Ok(self.tir_output_with_token(token)?.diagnostics())
    }

    pub(crate) fn elaboration_diagnostics_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<&[Diagnostic], ProjectError> {
        Ok(match self.elaboration_output_with_token(token)? {
            Some(output) => output.diagnostics(),
            None => &[],
        })
    }

    pub(crate) fn is_hir_cached(&self) -> bool {
        self.hir.get().is_some()
    }

    pub(crate) fn is_tir_cached(&self) -> bool {
        self.tir.get().is_some()
    }

    pub(crate) fn is_elaboration_cached(&self) -> bool {
        self.elaboration.get().is_some()
    }

    fn check_cancellation_before<F: Fn() -> bool + ?Sized>(
        &self,
        token: &F,
        cached: bool,
    ) -> Result<(), ProjectError> {
        if cached || !token() {
            return Ok(());
        }
        Err(ProjectError::Cancelled)
    }

    fn check_cancellation_after_uncached_stage<F: Fn() -> bool + ?Sized>(
        &self,
        token: &F,
    ) -> Result<(), ProjectError> {
        // Stage bodies are synchronous; give a peer request that observed the newly-filled cache a
        // bounded handoff window to publish cancellation before the next expensive stage starts.
        for _ in 0..CANCELLATION_HANDOFF_YIELDS {
            if token() {
                return Err(ProjectError::Cancelled);
            }
            std::thread::yield_now();
        }
        if token() {
            return Err(ProjectError::Cancelled);
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct SemanticCacheSource {
    module_path: Vec<String>,
    ast: AstFile,
}

impl SemanticCacheSource {
    pub fn new(module_path: Vec<String>, ast: AstFile) -> Self {
        Self { module_path, ast }
    }
}

impl SemanticCache {
    fn semantic_sources(&self) -> Vec<SemanticSourceFile<'_>> {
        self.sources
            .iter()
            .map(|source| SemanticSourceFile::new(source.module_path.clone(), &source.ast))
            .collect()
    }
}

impl fmt::Debug for SemanticCache {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SemanticCache")
            .field("source_count", &self.sources.len())
            .field(
                "opaque_summary_overlay_count",
                &self.opaque_summary_overlay.len(),
            )
            .field("hir_cached", &self.is_hir_cached())
            .field("tir_cached", &self.is_tir_cached())
            .field(
                "opaque_summaries_cached",
                &self.opaque_summaries.get().is_some(),
            )
            .field("elaboration_cached", &self.is_elaboration_cached())
            .field("diagnostics_cached", &self.diagnostics.get().is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use syl_sema::OpaqueSummaryTable;
    use syl_syntax::SourceParser;

    fn cache_from_source(source: &str) -> SemanticCache {
        let ast = SourceParser::new(source)
            .parse_file()
            .expect("test source must parse");
        SemanticCache::new_sources(
            vec![SemanticCacheSource::new(vec!["top".to_string()], ast)],
            OpaqueSummaryTable::new(),
        )
    }

    #[test]
    fn elaboration_output_with_token_returns_cancelled_without_caching() {
        let cache = cache_from_source("cell Top(y: out Bit) { y := 1 }\n");
        let _ = cache.hir_output();
        let _ = cache.tir_output();
        let checks = Cell::new(0);
        let token = || {
            let next = checks.get() + 1;
            checks.set(next);
            next > 2
        };

        let err = cache
            .elaboration_output_with_probe(&token)
            .expect_err("mid-pipeline cancellation must bubble out of elaboration");

        assert!(matches!(err, ProjectError::Cancelled));
        assert!(!cache.is_elaboration_cached());
    }
}
