use crate::{DocumentUri, DocumentVersion, ProjectError, SourceDocument};
use std::{collections::BTreeMap, path::PathBuf};

use super::cache::{DocumentKey, SnapshotKey};

#[derive(Debug, Default)]
#[non_exhaustive]
pub(crate) struct DocumentStore {
    roots: Vec<PathBuf>,
    overlays: BTreeMap<DocumentUri, SourceDocument>,
}

impl DocumentStore {
    pub(crate) fn set_roots(&mut self, roots: Vec<PathBuf>) {
        self.roots = roots;
    }

    pub(crate) fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    pub(crate) fn open_document(
        &mut self,
        uri: DocumentUri,
        text: String,
        version: DocumentVersion,
    ) -> Option<DocumentKey> {
        let path = uri.to_file_path();
        let document = SourceDocument::from_overlay(uri.clone(), text, version, path);
        self.overlays
            .insert(uri, document)
            .map(|document| DocumentKey::from_document(&document))
    }

    pub(crate) fn update_document(
        &mut self,
        uri: &DocumentUri,
        text: String,
        version: DocumentVersion,
    ) -> Result<Option<DocumentKey>, ProjectError> {
        let document = self
            .overlays
            .get_mut(uri)
            .ok_or_else(|| ProjectError::DocumentNotOpen {
                uri: uri.to_string(),
            })?;
        if version <= document.version() {
            return Err(ProjectError::StaleDocumentVersion {
                uri: uri.to_string(),
                requested: version.get(),
                current: document.version().get(),
            });
        }
        let previous = DocumentKey::new(uri.clone(), document.version());
        document.replace_text(text, version);
        Ok(Some(previous))
    }

    pub(crate) fn next_document_version(
        &self,
        uri: &DocumentUri,
    ) -> Result<DocumentVersion, ProjectError> {
        self.overlays
            .get(uri)
            .map(|document| document.version().next())
            .ok_or_else(|| ProjectError::DocumentNotOpen {
                uri: uri.to_string(),
            })
    }

    pub(crate) fn close_document(&mut self, uri: &DocumentUri) -> Option<SourceDocument> {
        self.overlays.remove(uri)
    }

    pub(crate) fn overlay(&self, uri: &DocumentUri) -> Option<&SourceDocument> {
        self.overlays.get(uri)
    }

    pub(crate) fn snapshot_inputs(&self) -> DocumentInputs<'_> {
        DocumentInputs {
            roots: self.roots.clone(),
            overlays: &self.overlays,
        }
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub(crate) struct DocumentInputs<'a> {
    roots: Vec<PathBuf>,
    overlays: &'a BTreeMap<DocumentUri, SourceDocument>,
}

impl<'a> DocumentInputs<'a> {
    pub(crate) fn into_resolver_inputs(
        self,
    ) -> (Vec<PathBuf>, &'a BTreeMap<DocumentUri, SourceDocument>) {
        (self.roots, self.overlays)
    }

    pub(crate) fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    pub(crate) fn overlays(&self) -> &BTreeMap<DocumentUri, SourceDocument> {
        self.overlays
    }

    pub(crate) fn snapshot_key(&self) -> SnapshotKey {
        SnapshotKey::from_inputs(self)
    }
}
