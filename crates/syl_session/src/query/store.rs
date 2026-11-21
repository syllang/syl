//! Input publication. The host keeps one Salsa database and advances it in
//! place, so declaration memos survive a new snapshot. A snapshot copies the
//! inputs it published and, once those inputs move, answers from that copy.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::{Arc, Mutex, RwLock};

use salsa::Setter;
use syl_sema::OpaqueSummaryTable;

use super::{FileSpec, ParsedFileOutput, RevisionInput, SourceFileInput, SylDatabase, parsed_file};
use crate::ProjectError;

pub(crate) struct Generation {
    pub(crate) db: SylDatabase,
    pub(crate) revision: RevisionInput,
    pub(crate) generation_id: u64,
    inputs: BTreeMap<String, SourceFileInput>,
}

impl fmt::Debug for Generation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Generation")
            .field("generation_id", &self.generation_id)
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
            generation_id: 0,
            inputs: BTreeMap::new(),
        }
    }

    pub(crate) fn from_specs(specs: &[FileSpec], opaque: &OpaqueSummaryTable) -> Self {
        let mut generation = Self::new();
        let mut staging = PublishStaging::new(&mut generation);
        for spec in specs {
            staging.stage(spec);
        }
        staging.finish(opaque);
        generation
    }
}

/// One discovery pass holds exclusive access to the host generation. Invalidate
/// old snapshots before the first mutation, even if discovery later fails.
pub(crate) struct PublishStaging<'a> {
    generation: &'a mut Generation,
    changed: bool,
    seen: BTreeSet<String>,
    ids: Vec<SourceFileInput>,
}

impl<'a> PublishStaging<'a> {
    fn new(generation: &'a mut Generation) -> Self {
        Self {
            generation,
            changed: false,
            seen: BTreeSet::new(),
            ids: Vec::new(),
        }
    }

    fn mark_changed(&mut self) {
        if !self.changed {
            self.generation.generation_id += 1;
            self.changed = true;
        }
    }

    fn stage(&mut self, spec: &FileSpec) -> SourceFileInput {
        let input = if let Some(existing) = self.generation.inputs.get(&spec.uri).copied() {
            let db = &self.generation.db;
            let text_changed = existing.text(db) != &spec.text;
            let source_changed = *existing.source_id(db) != spec.source_id;
            let module_changed = existing.module_path(db) != &spec.module_path;
            let version_changed = *existing.version(db) != spec.version;
            if text_changed || source_changed || module_changed || version_changed {
                self.mark_changed();
                let db = &mut self.generation.db;
                if text_changed {
                    existing.set_text(db).to(spec.text.clone());
                }
                if source_changed {
                    existing.set_source_id(db).to(spec.source_id);
                }
                if module_changed {
                    existing.set_module_path(db).to(spec.module_path.clone());
                }
                if version_changed {
                    existing.set_version(db).to(spec.version);
                }
            }
            existing
        } else {
            self.mark_changed();
            let created = SourceFileInput::new(
                &self.generation.db,
                spec.uri.clone(),
                spec.text.clone(),
                spec.source_id,
                spec.module_path.clone(),
                spec.version,
            );
            self.generation.inputs.insert(spec.uri.clone(), created);
            created
        };
        if self.seen.insert(spec.uri.clone()) {
            self.ids.push(input);
        }
        input
    }

    pub(crate) fn stage_and_parse(
        &mut self,
        spec: &FileSpec,
    ) -> (SourceFileInput, Arc<ParsedFileOutput>) {
        let input = self.stage(spec);
        (input, Arc::clone(parsed_file(&self.generation.db, input)))
    }

    fn finish(mut self, opaque: &OpaqueSummaryTable) {
        let files_changed = self.generation.revision.files(&self.generation.db) != &self.ids;
        let opaque_changed = self.generation.revision.opaque(&self.generation.db) != opaque;
        let removed = self.generation.inputs.len() != self.seen.len();
        if files_changed || opaque_changed || removed {
            self.mark_changed();
        }
        let generation = &mut self.generation;
        generation.inputs.retain(|uri, _| self.seen.contains(uri));
        if files_changed {
            generation
                .revision
                .set_files(&mut generation.db)
                .to(self.ids);
        }
        if opaque_changed {
            generation
                .revision
                .set_opaque(&mut generation.db)
                .to(opaque.clone());
        }
    }
}

pub(crate) struct QueryStore {
    current: Arc<Mutex<Generation>>,
    /// Held for the whole query. Publish takes the write side, so two queries
    /// can share one Salsa database and a publish waits until they finish.
    inflight: Arc<RwLock<()>>,
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
            inflight: Arc::new(RwLock::new(())),
        }
    }

    pub(crate) fn generation(&self) -> Arc<Mutex<Generation>> {
        Arc::clone(&self.current)
    }

    pub(crate) fn inflight(&self) -> Arc<RwLock<()>> {
        Arc::clone(&self.inflight)
    }

    pub(crate) fn resolve_and_publish<R>(
        &mut self,
        opaque: &OpaqueSummaryTable,
        build: impl FnOnce(&mut PublishStaging<'_>) -> Result<R, ProjectError>,
    ) -> Result<R, ProjectError> {
        let _inflight = self
            .inflight
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut current = lock(&self.current);
        let mut staging = PublishStaging::new(&mut current);
        let resolved = build(&mut staging)?;
        staging.finish(opaque);
        Ok(resolved)
    }
}

fn lock(generation: &Mutex<Generation>) -> std::sync::MutexGuard<'_, Generation> {
    generation
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
