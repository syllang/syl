//! Input publication. The host keeps one Salsa database and advances it in
//! place, so declaration memos survive a new snapshot. A snapshot copies the
//! inputs it published and, once those inputs move, answers from that copy.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex};

use salsa::Setter;
use syl_sema::OpaqueSummaryTable;

use super::{FileSpec, RevisionInput, SourceFileInput, SylDatabase};

pub(crate) struct Generation {
    pub(crate) db: SylDatabase,
    pub(crate) revision: RevisionInput,
    inputs: BTreeMap<String, SourceFileInput>,
}

impl fmt::Debug for Generation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Generation")
            .field("input_count", &self.inputs.len())
            .finish_non_exhaustive()
    }
}

impl Generation {
    fn new() -> Self {
        let db = SylDatabase::new();
        let revision = RevisionInput::new(&db, Vec::new(), OpaqueSummaryTable::new());
        Self {
            db,
            revision,
            inputs: BTreeMap::new(),
        }
    }

    pub(crate) fn from_specs(specs: &[FileSpec], opaque: &OpaqueSummaryTable) -> Self {
        let mut generation = Self::new();
        generation.install(specs, opaque);
        generation
    }

    pub(crate) fn file_specs(&self) -> Vec<FileSpec> {
        self.revision
            .files(&self.db)
            .iter()
            .copied()
            .map(|file| FileSpec {
                uri: file.uri(&self.db).clone(),
                text: file.text(&self.db).clone(),
                source_id: *file.source_id(&self.db),
                module_path: file.module_path(&self.db).clone(),
                version: *file.version(&self.db),
            })
            .collect()
    }

    pub(crate) fn inputs_changed(&self, specs: &[FileSpec], opaque: &OpaqueSummaryTable) -> bool {
        if self.revision.opaque(&self.db) != opaque {
            return true;
        }
        if self.inputs.len() != specs.len() {
            return true;
        }
        for spec in specs {
            let Some(input) = self.inputs.get(&spec.uri).copied() else {
                return true;
            };
            if input.text(&self.db) != &spec.text
                || *input.source_id(&self.db) != spec.source_id
                || input.module_path(&self.db) != &spec.module_path
                || *input.version(&self.db) != spec.version
            {
                return true;
            }
        }
        let current = self.revision.files(&self.db);
        current.len() != specs.len()
            || current
                .iter()
                .zip(specs)
                .any(|(input, spec)| input.uri(&self.db) != &spec.uri)
    }

    fn install(&mut self, specs: &[FileSpec], opaque: &OpaqueSummaryTable) {
        let mut ids = Vec::with_capacity(specs.len());
        for spec in specs {
            let input = if let Some(existing) = self.inputs.get(&spec.uri).copied() {
                let text = existing.text(&self.db).clone();
                if text != spec.text {
                    existing.set_text(&mut self.db).to(spec.text.clone());
                }
                let source_id = *existing.source_id(&self.db);
                if source_id != spec.source_id {
                    existing.set_source_id(&mut self.db).to(spec.source_id);
                }
                let module_path = existing.module_path(&self.db).clone();
                if module_path != spec.module_path {
                    existing
                        .set_module_path(&mut self.db)
                        .to(spec.module_path.clone());
                }
                let version = *existing.version(&self.db);
                if version != spec.version {
                    existing.set_version(&mut self.db).to(spec.version);
                }
                existing
            } else {
                let created = SourceFileInput::new(
                    &self.db,
                    spec.uri.clone(),
                    spec.text.clone(),
                    spec.source_id,
                    spec.module_path.clone(),
                    spec.version,
                );
                self.inputs.insert(spec.uri.clone(), created);
                created
            };
            ids.push(input);
        }
        self.inputs
            .retain(|uri, _| specs.iter().any(|spec| &spec.uri == uri));
        let files_changed = self.revision.files(&self.db) != &ids;
        let opaque_changed = self.revision.opaque(&self.db) != opaque;
        if files_changed {
            self.revision.set_files(&mut self.db).to(ids);
        }
        if opaque_changed {
            self.revision.set_opaque(&mut self.db).to(opaque.clone());
        }
    }
}

pub(crate) struct QueryStore {
    current: Arc<Mutex<Generation>>,
}

impl fmt::Debug for QueryStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("QueryStore").finish_non_exhaustive()
    }
}

impl QueryStore {
    pub(crate) fn new() -> Self {
        Self {
            current: Arc::new(Mutex::new(Generation::new())),
        }
    }

    pub(crate) fn generation(&self) -> Arc<Mutex<Generation>> {
        Arc::clone(&self.current)
    }

    pub(crate) fn publish(
        &mut self,
        specs: &[FileSpec],
        opaque: &OpaqueSummaryTable,
        _live: usize,
    ) {
        let mut current = lock(&self.current);
        if current.inputs_changed(specs, opaque) {
            current.install(specs, opaque);
        }
    }
}

fn lock(generation: &Mutex<Generation>) -> std::sync::MutexGuard<'_, Generation> {
    generation
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
