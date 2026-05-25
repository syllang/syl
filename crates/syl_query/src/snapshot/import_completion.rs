use crate::{CompletionItem, CompletionItemKind, CompletionResult};
use std::collections::BTreeMap;
use syl_session::{AnalysisFile, AnalysisSnapshot, DocumentUri};
use syl_span::Span;
use syl_syntax::{AstFile, CallableItem, ExternModuleItem, Item};

#[non_exhaustive]
pub(super) struct ImportPathCompletion<'a> {
    snapshot: &'a AnalysisSnapshot,
    uri: &'a DocumentUri,
    cursor: Span,
}

impl<'a> ImportPathCompletion<'a> {
    pub(super) fn new(snapshot: &'a AnalysisSnapshot, uri: &'a DocumentUri, cursor: Span) -> Self {
        Self {
            snapshot,
            uri,
            cursor,
        }
    }

    pub(super) fn complete(&self) -> CompletionResult {
        let Some(prefix) = self.import_prefix() else {
            return CompletionResult::default();
        };
        let mut candidates = BTreeMap::new();
        for file in self.snapshot.files() {
            ImportDefinitionCollector::new(file).collect_into(&mut candidates, &prefix);
        }
        CompletionResult {
            items: candidates
                .into_iter()
                .map(|(label, kind)| CompletionItem { label, kind })
                .collect(),
        }
    }

    fn import_prefix(&self) -> Option<ImportPathPrefix> {
        let file = self.snapshot.file_by_uri(self.uri)?;
        let source = self.snapshot.source_map().file(file.source_id())?;
        if let Some(prefix) = self.import_prefix_from_ast(file.ast(), source.text()) {
            return Some(prefix);
        }
        self.import_prefix_from_source(source.text())
    }

    fn import_prefix_from_ast(&self, file: &AstFile, source: &str) -> Option<ImportPathPrefix> {
        let use_span = file.items.iter().find_map(|item| match item {
            Item::Use(item) if self.contains(item.span) => Some(item.span),
            _ => None,
        })?;
        let text = source.get(use_span.start..self.cursor.start)?;
        ImportPathPrefix::parse(text)
    }

    fn import_prefix_from_source(&self, source: &str) -> Option<ImportPathPrefix> {
        let before_cursor = source.get(..self.cursor.start)?;
        let text = before_cursor
            .rsplit_once('\n')
            .map(|(_, line)| line)
            .unwrap_or(before_cursor);
        ImportPathPrefix::parse(text)
    }

    fn contains(&self, span: Span) -> bool {
        span.source == self.cursor.source
            && span.start <= self.cursor.start
            && self.cursor.end <= span.end
    }
}

#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
struct ImportPathPrefix {
    parent: Vec<String>,
    partial: String,
}

impl ImportPathPrefix {
    fn parse(text: &str) -> Option<Self> {
        let path = text.trim_start().strip_prefix("use")?.trim_start();
        let path = path.trim_end_matches(';').trim_end();
        let mut segments = path
            .split('.')
            .map(str::trim)
            .map(str::to_string)
            .collect::<Vec<_>>();
        let partial = if path.ends_with('.') {
            String::new()
        } else {
            segments.pop().unwrap_or_default()
        };
        segments.retain(|segment| !segment.is_empty());
        Some(Self {
            parent: segments,
            partial,
        })
    }
}

#[non_exhaustive]
struct ImportDefinitionCollector<'a> {
    file: &'a AstFile,
    package: Vec<String>,
}

impl<'a> ImportDefinitionCollector<'a> {
    fn new(file: &'a AnalysisFile) -> Self {
        Self {
            file: file.ast(),
            package: file.module_path().to_vec(),
        }
    }

    fn collect_into(
        &self,
        candidates: &mut BTreeMap<String, CompletionItemKind>,
        prefix: &ImportPathPrefix,
    ) {
        for item in &self.file.items {
            let Some(definition) = ImportDefinition::new(&self.package, item) else {
                continue;
            };
            definition.add_candidate(candidates, prefix);
        }
    }
}

#[non_exhaustive]
struct ImportDefinition {
    path: Vec<String>,
    kind: CompletionItemKind,
}

impl ImportDefinition {
    fn new(package: &[String], item: &Item) -> Option<Self> {
        let (name, kind) = Self::item_name_kind(item)?;
        let mut path = package.to_vec();
        path.push(name);
        Some(Self { path, kind })
    }

    fn add_candidate(
        &self,
        candidates: &mut BTreeMap<String, CompletionItemKind>,
        prefix: &ImportPathPrefix,
    ) {
        if !self.path.starts_with(&prefix.parent) || self.path.len() <= prefix.parent.len() {
            return;
        }
        let label = &self.path[prefix.parent.len()];
        if !label.starts_with(&prefix.partial) {
            return;
        }
        let kind = if self.path.len() == prefix.parent.len() + 1 {
            self.kind.clone()
        } else {
            CompletionItemKind::Module
        };
        candidates.entry(label.clone()).or_insert(kind);
    }

    fn item_name_kind(item: &Item) -> Option<(String, CompletionItemKind)> {
        match item {
            Item::Const(item) => Some((item.name.clone(), CompletionItemKind::Constant)),
            Item::Fn(item) => Some((item.name.clone(), CompletionItemKind::Function)),
            Item::Enum(item) => Some((item.name.clone(), CompletionItemKind::Type)),
            Item::Bundle(item) => Some((item.name.clone(), CompletionItemKind::Type)),
            Item::Interface(item) => Some((item.name.clone(), CompletionItemKind::Type)),
            Item::Map(item) => Some((item.name.clone(), CompletionItemKind::Function)),
            Item::Cell(item) | Item::Module(item) => Some(Self::callable_name_kind(item)),
            Item::ExternModule(item) => Some(Self::extern_name_kind(item)),
            Item::Use(_) | Item::Error(_) => None,
            _ => None,
        }
    }

    fn callable_name_kind(item: &CallableItem) -> (String, CompletionItemKind) {
        (item.name.clone(), CompletionItemKind::Module)
    }

    fn extern_name_kind(item: &ExternModuleItem) -> (String, CompletionItemKind) {
        (item.name.clone(), CompletionItemKind::Module)
    }
}
