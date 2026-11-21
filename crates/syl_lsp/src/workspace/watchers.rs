use super::WorkspaceState;
use crate::SylLanguageServer;
use std::sync::Arc;
use tower_lsp::lsp_types::{
    DidChangeWatchedFilesRegistrationOptions, FileSystemWatcher, GlobPattern, MessageType,
    Registration, Unregistration, WatchKind,
};

const REGISTRATION_ID: &str = "syl-workspace-watchers";
const METHOD: &str = "workspace/didChangeWatchedFiles";

impl WorkspaceState {
    pub(super) fn registration(&self) -> Registration {
        let patterns = ["**/*.syl".to_string(), "**/syl.toml".to_string()]
            .into_iter()
            .chain(
                self.manifest_path
                    .iter()
                    .chain(&self.watched_opaque_paths)
                    .map(|p| p.to_string_lossy().into_owned()),
            );
        let watchers = patterns
            .map(|p| FileSystemWatcher {
                glob_pattern: GlobPattern::String(p),
                kind: Some(WatchKind::all()),
            })
            .collect();
        Registration {
            id: REGISTRATION_ID.into(),
            method: METHOD.into(),
            register_options: Some(
                serde_json::to_value(DidChangeWatchedFilesRegistrationOptions { watchers })
                    .expect("watcher options are serializable"),
            ),
        }
    }
}

impl SylLanguageServer {
    pub(crate) async fn register_file_watchers(&self) {
        let registration_lock = Arc::clone(&self.workspace.lock().await.watcher_registration);
        let _registration = registration_lock.lock().await;
        let (registration, was_registered) = {
            let workspace = self.workspace.lock().await;
            if !workspace.dynamic_watcher_registration {
                return;
            }
            (workspace.registration(), workspace.watcher_registered)
        };
        if was_registered {
            if let Err(error) = self
                .client
                .unregister_capability(vec![Unregistration {
                    id: REGISTRATION_ID.into(),
                    method: METHOD.into(),
                }])
                .await
            {
                self.client
                    .log_message(
                        MessageType::WARNING,
                        format!("Syl watcher unregistration failed: {error}"),
                    )
                    .await;
                return;
            }
            self.workspace.lock().await.watcher_registered = false;
        }
        match self.client.register_capability(vec![registration]).await {
            Ok(()) => self.workspace.lock().await.watcher_registered = true,
            Err(error) => self.client.log_message(MessageType::WARNING, format!("Syl watcher registration failed; static file notifications remain supported: {error}")).await,
        }
    }
}
