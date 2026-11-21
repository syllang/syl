use super::AnalysisSnapshot;
use crate::query::{catch_query, make_unit, unit_elaboration_query};
use crate::{CancellationToken, DocumentUri, ProjectError};
use syl_span::Diagnostic;

impl AnalysisSnapshot {
    /// Run cancellable hardware diagnostics for the package owning `uri`.
    ///
    /// An unknown URI returns `None`. Errors in this package's import closure
    /// skip elaboration and return an empty list; unrelated packages are isolated.
    pub fn package_elaboration_diagnostics_with_token(
        &self,
        uri: &DocumentUri,
        token: &CancellationToken,
    ) -> Result<Option<Vec<Diagnostic>>, ProjectError> {
        Self::check_cancellation(token)?;
        let Some(package) = self.package_semantics.entry_for_uri(uri) else {
            return Ok(None);
        };
        let sources = self.package_source_ids(package.closure());
        if self
            .diagnostics()
            .iter()
            .any(|d| sources.contains(&d.span.source))
        {
            return Ok(Some(Vec::new()));
        }
        Self::check_cancellation(token)?;
        let (name, local, closure) = Self::shard_inputs(package);
        let output = catch_query(token, || {
            self.with_db(|db, revision| {
                let unit = make_unit(db, &name, local, closure);
                unit_elaboration_query(db, revision, unit).clone()
            })
        })?;
        Ok(Some(
            output.map(|o| o.diagnostics().to_vec()).unwrap_or_default(),
        ))
    }
}

#[cfg(test)]
#[path = "elab_diagnostics_tests.rs"]
mod tests;
