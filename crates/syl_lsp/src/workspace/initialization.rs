use std::path::PathBuf;
use tower_lsp::lsp_types::{InitializeParams, Url};

#[non_exhaustive]
pub(super) struct WorkspaceInitialization {
    pub(super) roots: Vec<PathBuf>,
    pub(super) fallback_uri: Option<Url>,
}

impl WorkspaceInitialization {
    pub(super) fn new(params: &InitializeParams) -> Self {
        let mut initialization = Self {
            roots: Vec::new(),
            fallback_uri: None,
        };
        initialization.add_workspace_folders(params);
        initialization.add_root_uri(params);
        initialization.add_root_path(params);
        initialization
    }

    fn add_workspace_folders(&mut self, params: &InitializeParams) {
        if let Some(folders) = &params.workspace_folders {
            for folder in folders {
                self.add_uri(&folder.uri);
            }
        }
    }

    fn add_root_uri(&mut self, params: &InitializeParams) {
        if !self.roots.is_empty() {
            return;
        }
        if let Some(uri) = &params.root_uri {
            self.add_uri(uri);
        }
    }

    #[allow(deprecated)]
    fn add_root_path(&mut self, params: &InitializeParams) {
        if !self.roots.is_empty() {
            return;
        }
        if let Some(path) = &params.root_path {
            self.add_path_with_fallback(PathBuf::from(path));
        }
    }

    fn add_uri(&mut self, uri: &Url) {
        let Ok(path) = uri.to_file_path() else {
            return;
        };
        if self.fallback_uri.is_none() {
            self.fallback_uri = Some(uri.clone());
        }
        self.add_path(path);
    }

    fn add_path(&mut self, path: PathBuf) {
        if !self.roots.contains(&path) {
            self.roots.push(path);
        }
    }

    fn add_path_with_fallback(&mut self, path: PathBuf) {
        if self.fallback_uri.is_none()
            && let Ok(uri) = Url::from_file_path(&path)
        {
            self.fallback_uri = Some(uri);
        }
        self.add_path(path);
    }
}
