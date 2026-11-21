mod initialization;
#[cfg(test)]
mod tests;
mod watchers;

use initialization::WorkspaceInitialization;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use syl_session::{
    AnalysisHost, DocumentUri, MANIFEST_FILE_NAME, ProjectConfig, ProjectError, ProjectManifest,
};
use tower_lsp::lsp_types::{FileChangeType, FileEvent, InitializeParams, Url};

#[derive(Debug, Default)]
pub(crate) struct WorkspaceState {
    roots: Vec<PathBuf>,
    fallback_uri: Option<Url>,
    manifest_path: Option<PathBuf>,
    watched_opaque_paths: Vec<PathBuf>,
    load_inputs: Vec<PathBuf>,
    dynamic_watcher_registration: bool,
    watcher_registered: bool,
    watcher_registration: Arc<tokio::sync::Mutex<()>>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum WatchAction {
    ReloadManifest,
    ReloadOpaqueSummaries,
    RefreshSylFiles { structural_change: bool },
    Ignore,
}

impl WorkspaceState {
    pub(crate) fn new(params: &InitializeParams) -> Self {
        let initialization = WorkspaceInitialization::new(params);
        Self {
            roots: initialization.roots,
            fallback_uri: initialization.fallback_uri,
            dynamic_watcher_registration: params
                .capabilities
                .workspace
                .as_ref()
                .and_then(|w| w.did_change_watched_files.as_ref())
                .and_then(|c| c.dynamic_registration)
                .unwrap_or(false),
            ..Self::default()
        }
    }

    pub(crate) fn error_uri(&self) -> Option<Url> {
        self.manifest_path
            .as_ref()
            .and_then(|p| Url::from_file_path(p).ok())
            .or_else(|| self.fallback_uri.clone())
    }

    pub(crate) fn configure_host(&mut self, host: &mut AnalysisHost) -> Result<(), ProjectError> {
        let Some(primary) = self.roots.first() else {
            return Ok(());
        };
        self.manifest_path = ProjectManifest::discover(primary)?;
        if let Some(path) = &self.manifest_path {
            let manifest = ProjectManifest::load(path)?;
            // Retain watcher targets even if loading a summary fails, so a fix is observable.
            self.watched_opaque_paths = manifest.opaque_summary_paths().to_vec();
            host.apply_manifest(&manifest)?;
            self.load_inputs = if manifest.workspace_roots().is_empty() {
                self.roots.clone()
            } else {
                manifest.workspace_roots().to_vec()
            };
        } else {
            let config = self.roots.iter().fold(ProjectConfig::new(), |c, p| {
                c.with_workspace_root(p.clone())
            });
            host.reconfigure(config);
            host.clear_opaque_summaries();
            self.watched_opaque_paths.clear();
            self.load_inputs.clone_from(&self.roots);
        }
        host.refresh_inputs(&self.load_inputs)
    }

    pub(crate) fn classify_file_events(
        &self,
        events: &[FileEvent],
        host: &AnalysisHost,
    ) -> WatchAction {
        let mut opaque = false;
        let mut changed = false;
        let mut structural = false;
        for event in events {
            let Ok(path) = event.uri.to_file_path() else {
                continue;
            };
            if path.file_name().is_some_and(|n| n == MANIFEST_FILE_NAME)
                || self
                    .manifest_path
                    .as_ref()
                    .is_some_and(|p| same_path(p, &path))
            {
                return WatchAction::ReloadManifest;
            }
            if self
                .watched_opaque_paths
                .iter()
                .any(|p| same_path(p, &path))
            {
                opaque = true;
            }
            if path.extension().is_some_and(|e| e == "syl") {
                structural |=
                    event.typ == FileChangeType::CREATED || event.typ == FileChangeType::DELETED;
                changed |= host
                    .overlay(&DocumentUri::new(event.uri.as_str()))
                    .is_none();
            }
        }
        if opaque {
            WatchAction::ReloadOpaqueSummaries
        } else if structural || changed {
            WatchAction::RefreshSylFiles {
                structural_change: structural,
            }
        } else {
            WatchAction::Ignore
        }
    }

    pub(crate) fn apply_watch_action(
        &mut self,
        host: &mut AnalysisHost,
        action: WatchAction,
    ) -> Result<(), ProjectError> {
        match action {
            WatchAction::ReloadManifest | WatchAction::ReloadOpaqueSummaries => {
                self.configure_host(host)
            }
            WatchAction::RefreshSylFiles {
                structural_change: true,
            } => host.refresh_inputs(&self.load_inputs),
            WatchAction::RefreshSylFiles {
                structural_change: false,
            }
            | WatchAction::Ignore => Ok(()),
        }
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    fn normalized(path: &Path) -> PathBuf {
        if let Ok(real) = path.canonicalize() {
            return real;
        }
        let mut result = PathBuf::new();
        for c in path.components() {
            match c {
                Component::CurDir => {}
                Component::ParentDir => {
                    result.pop();
                }
                _ => result.push(c),
            }
        }
        result
    }
    normalized(a) == normalized(b)
}

impl crate::SylLanguageServer {
    pub(crate) async fn handle_watched_files(
        &self,
        params: tower_lsp::lsp_types::DidChangeWatchedFilesParams,
    ) {
        let (result, generation, fallback_uri, watchers_changed) = {
            let mut workspace = self.workspace.lock().await;
            let mut host = self.host.lock().await;
            let action = workspace.classify_file_events(&params.changes, &host);
            if action == WatchAction::Ignore {
                return;
            }
            let generation = self.next_diagnostics_generation();
            let previous = (
                workspace.manifest_path.clone(),
                workspace.watched_opaque_paths.clone(),
            );
            let result = workspace.apply_watch_action(&mut host, action);
            self.drc_scheduler.inputs_changed();
            let changed = previous
                != (
                    workspace.manifest_path.clone(),
                    workspace.watched_opaque_paths.clone(),
                );
            (result, generation, workspace.error_uri(), changed)
        };
        if watchers_changed {
            self.register_file_watchers().await;
        }
        if let Err(error) = result {
            if let Some(uri) = fallback_uri {
                self.publish_project_error(uri, error, generation).await;
            }
            return;
        }
        if self.drc_scheduler.is_current(generation) {
            *self.initialization_error.lock().await = None;
        }
        self.schedule_publish(
            fallback_uri,
            self.drc_scheduler.debounce_delay(),
            generation,
        );
    }
}
