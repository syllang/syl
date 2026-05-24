use crate::{
    CompletionItem, CompletionItemKind, CompletionResult, DefinitionResult, DocumentSymbolResult,
    GroupedDiagnostics, HoverResult, QueryError,
};
use syl_sema::OpaqueSummaryTable;
use syl_sema::completion::CompletionKind;
use syl_session::{AnalysisSnapshot, CancellationToken, DocumentUri, Project, ProjectError};
use syl_span::{SourcePosition, Span};
use syl_syntax::{AstFile, Item};

use super::{
    completion_context::{CompletionAnalyzer, CompletionContext},
    diagnostics::DiagnosticQueryEngine,
    document_symbols::DocumentSymbolCollector,
    generic::{GenericDefinitionResolver, GenericParamHover},
    import_completion::ImportPathCompletion,
};

/// Protocol-neutral query operations layered over a session-owned analysis snapshot.
///
/// This is a trait rather than inherent snapshot methods so `syl_session` owns
/// persisted state while `syl_query` owns editor-facing semantic operations.
pub trait AnalysisQueries {
    fn opaque_summaries(&self) -> Option<&OpaqueSummaryTable>;

    fn opaque_summaries_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<Option<&OpaqueSummaryTable>, QueryError>;

    fn definition(&self, uri: &DocumentUri, position: SourcePosition) -> Option<DefinitionResult>;

    fn definition_with_token(
        &self,
        uri: &DocumentUri,
        position: SourcePosition,
        token: &CancellationToken,
    ) -> Result<Option<DefinitionResult>, QueryError>;

    fn definition_at(
        &self,
        uri: &DocumentUri,
        utf16_position: SourcePosition,
    ) -> Option<DefinitionResult>;

    fn definition_at_with_token(
        &self,
        uri: &DocumentUri,
        utf16_position: SourcePosition,
        token: &CancellationToken,
    ) -> Result<Option<DefinitionResult>, QueryError>;

    fn hover(&self, uri: &DocumentUri, position: SourcePosition) -> Option<HoverResult>;

    fn hover_with_token(
        &self,
        uri: &DocumentUri,
        position: SourcePosition,
        token: &CancellationToken,
    ) -> Result<Option<HoverResult>, QueryError>;

    fn hover_at(&self, uri: &DocumentUri, utf16_position: SourcePosition) -> Option<HoverResult>;

    fn hover_at_with_token(
        &self,
        uri: &DocumentUri,
        utf16_position: SourcePosition,
        token: &CancellationToken,
    ) -> Result<Option<HoverResult>, QueryError>;

    fn completion(&self, uri: &DocumentUri, position: SourcePosition) -> CompletionResult;

    fn completion_with_token(
        &self,
        uri: &DocumentUri,
        position: SourcePosition,
        token: &CancellationToken,
    ) -> Result<CompletionResult, QueryError>;

    fn completions_at(&self, uri: &DocumentUri, utf16_position: SourcePosition)
    -> CompletionResult;

    fn completions_at_with_token(
        &self,
        uri: &DocumentUri,
        utf16_position: SourcePosition,
        token: &CancellationToken,
    ) -> Result<CompletionResult, QueryError>;

    fn document_symbols(&self, uri: &DocumentUri) -> Vec<DocumentSymbolResult>;

    fn symbols(&self, uri: &DocumentUri) -> Vec<DocumentSymbolResult>;

    fn all_document_diagnostics(&self) -> Vec<crate::DocumentDiagnostics>;

    fn grouped_diagnostics(&self) -> GroupedDiagnostics;

    fn grouped_diagnostics_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<GroupedDiagnostics, QueryError>;

    fn document_diagnostics(&self, uri: &DocumentUri) -> Option<crate::DocumentDiagnostics>;

    fn document_diagnostics_with_token(
        &self,
        uri: &DocumentUri,
        token: &CancellationToken,
    ) -> Result<Option<crate::DocumentDiagnostics>, QueryError>;

    fn diagnostics_for(&self, uri: &DocumentUri) -> Vec<crate::DiagnosticResult>;

    fn diagnostics_for_with_token(
        &self,
        uri: &DocumentUri,
        token: &CancellationToken,
    ) -> Result<Vec<crate::DiagnosticResult>, QueryError>;
}

impl AnalysisQueries for AnalysisSnapshot {
    fn opaque_summaries(&self) -> Option<&OpaqueSummaryTable> {
        self.opaque_summaries()
    }

    fn opaque_summaries_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<Option<&OpaqueSummaryTable>, QueryError> {
        self.opaque_summaries_with_token(token)
            .map_err(map_project_error)
    }

    fn definition(&self, uri: &DocumentUri, position: SourcePosition) -> Option<DefinitionResult> {
        self.definition_at(uri, position)
    }

    fn definition_with_token(
        &self,
        uri: &DocumentUri,
        position: SourcePosition,
        token: &CancellationToken,
    ) -> Result<Option<DefinitionResult>, QueryError> {
        self.definition_at_with_token(uri, position, token)
    }

    fn definition_at(
        &self,
        uri: &DocumentUri,
        utf16_position: SourcePosition,
    ) -> Option<DefinitionResult> {
        self.definition_at_with_token(uri, utf16_position, &CancellationToken::new())
            .unwrap_or(None)
    }

    fn definition_at_with_token(
        &self,
        uri: &DocumentUri,
        utf16_position: SourcePosition,
        token: &CancellationToken,
    ) -> Result<Option<DefinitionResult>, QueryError> {
        SnapshotQueryEngine::new(self).definition_at(uri, utf16_position, token)
    }

    fn hover(&self, uri: &DocumentUri, position: SourcePosition) -> Option<HoverResult> {
        self.hover_at(uri, position)
    }

    fn hover_with_token(
        &self,
        uri: &DocumentUri,
        position: SourcePosition,
        token: &CancellationToken,
    ) -> Result<Option<HoverResult>, QueryError> {
        self.hover_at_with_token(uri, position, token)
    }

    fn hover_at(&self, uri: &DocumentUri, utf16_position: SourcePosition) -> Option<HoverResult> {
        self.hover_at_with_token(uri, utf16_position, &CancellationToken::new())
            .unwrap_or(None)
    }

    fn hover_at_with_token(
        &self,
        uri: &DocumentUri,
        utf16_position: SourcePosition,
        token: &CancellationToken,
    ) -> Result<Option<HoverResult>, QueryError> {
        SnapshotQueryEngine::new(self).hover_at(uri, utf16_position, token)
    }

    fn completion(&self, uri: &DocumentUri, position: SourcePosition) -> CompletionResult {
        self.completions_at(uri, position)
    }

    fn completion_with_token(
        &self,
        uri: &DocumentUri,
        position: SourcePosition,
        token: &CancellationToken,
    ) -> Result<CompletionResult, QueryError> {
        self.completions_at_with_token(uri, position, token)
    }

    fn completions_at(
        &self,
        uri: &DocumentUri,
        utf16_position: SourcePosition,
    ) -> CompletionResult {
        self.completions_at_with_token(uri, utf16_position, &CancellationToken::new())
            .unwrap_or_else(|_| CompletionResult::default())
    }

    fn completions_at_with_token(
        &self,
        uri: &DocumentUri,
        utf16_position: SourcePosition,
        token: &CancellationToken,
    ) -> Result<CompletionResult, QueryError> {
        SnapshotQueryEngine::new(self).completions_at(uri, utf16_position, token)
    }

    fn document_symbols(&self, uri: &DocumentUri) -> Vec<DocumentSymbolResult> {
        self.symbols(uri)
    }

    fn symbols(&self, uri: &DocumentUri) -> Vec<DocumentSymbolResult> {
        let Some(file) = self.file_by_uri(uri) else {
            return Vec::new();
        };
        DocumentSymbolCollector::new(self, file).collect()
    }

    fn all_document_diagnostics(&self) -> Vec<crate::DocumentDiagnostics> {
        DiagnosticQueryEngine::new(self).all_document_diagnostics()
    }

    fn grouped_diagnostics(&self) -> GroupedDiagnostics {
        DiagnosticQueryEngine::new(self).grouped_diagnostics()
    }

    fn grouped_diagnostics_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<GroupedDiagnostics, QueryError> {
        DiagnosticQueryEngine::new(self).grouped_diagnostics_with_token(token)
    }

    fn document_diagnostics(&self, uri: &DocumentUri) -> Option<crate::DocumentDiagnostics> {
        DiagnosticQueryEngine::new(self).document_diagnostics(uri)
    }

    fn document_diagnostics_with_token(
        &self,
        uri: &DocumentUri,
        token: &CancellationToken,
    ) -> Result<Option<crate::DocumentDiagnostics>, QueryError> {
        self.grouped_diagnostics_with_token(token).map(|grouped| {
            grouped
                .packages()
                .iter()
                .flat_map(|package| package.documents().iter())
                .find(|document| document.uri() == uri)
                .cloned()
        })
    }

    fn diagnostics_for(&self, uri: &DocumentUri) -> Vec<crate::DiagnosticResult> {
        DiagnosticQueryEngine::new(self).diagnostics_for(uri)
    }

    fn diagnostics_for_with_token(
        &self,
        uri: &DocumentUri,
        token: &CancellationToken,
    ) -> Result<Vec<crate::DiagnosticResult>, QueryError> {
        self.document_diagnostics_with_token(uri, token)
            .map(|document| {
                document
                    .map(|document| document.diagnostics().to_vec())
                    .unwrap_or_default()
            })
    }
}

#[non_exhaustive]
struct SnapshotQueryEngine<'a> {
    snapshot: &'a AnalysisSnapshot,
}

impl<'a> SnapshotQueryEngine<'a> {
    fn new(snapshot: &'a AnalysisSnapshot) -> Self {
        Self { snapshot }
    }

    fn definition_at(
        &self,
        uri: &DocumentUri,
        utf16_position: SourcePosition,
        token: &CancellationToken,
    ) -> Result<Option<DefinitionResult>, QueryError> {
        let Some(span) = self.span_at(uri, utf16_position) else {
            return Ok(None);
        };
        let Some(hir) = self
            .snapshot
            .hir_analysis_for_uri_with_token(uri, token)
            .map_err(map_project_error)?
        else {
            return Ok(self.generic_definition(uri, span));
        };
        if let Some(definition) = hir.definition_at(span) {
            let Some(file) = self.snapshot.source_map().file(definition.span().source) else {
                return Ok(None);
            };
            let Some(range) = self.snapshot.source_map().utf16_range(definition.span()) else {
                return Ok(None);
            };
            return Ok(Some(DefinitionResult {
                uri: DocumentUri::new(file.uri()),
                range,
            }));
        }
        Ok(self.generic_definition(uri, span))
    }

    fn hover_at(
        &self,
        uri: &DocumentUri,
        utf16_position: SourcePosition,
        token: &CancellationToken,
    ) -> Result<Option<HoverResult>, QueryError> {
        let Some(span) = self.span_at(uri, utf16_position) else {
            return Ok(None);
        };
        let hir = self
            .snapshot
            .hir_analysis_for_uri_with_token(uri, token)
            .map_err(map_project_error)?;
        if let Some(tir) = self
            .snapshot
            .tir_analysis_for_uri_with_token(uri, token)
            .map_err(map_project_error)?
            && let Some(hover) = tir.hover_at(span)
        {
            return Ok(Some(HoverResult {
                contents: hover.text().to_string(),
                range: self.snapshot.source_map().utf16_range(hover.span()),
            }));
        }
        if let Some(hover) = self.generic_hover(uri, span) {
            return Ok(Some(hover));
        }
        let Some(hir) = hir else {
            return Ok(None);
        };
        let Some(hover) = hir.hover_at(span) else {
            return Ok(None);
        };
        Ok(Some(HoverResult {
            contents: hover.text().to_string(),
            range: self.snapshot.source_map().utf16_range(hover.span()),
        }))
    }

    fn completions_at(
        &self,
        uri: &DocumentUri,
        utf16_position: SourcePosition,
        token: &CancellationToken,
    ) -> Result<CompletionResult, QueryError> {
        let span = self.span_at(uri, utf16_position);
        let context = self.snapshot.file_by_uri(uri).and_then(|file| {
            let span = span?;
            let source = self.snapshot.source_map().file(file.source_id())?;
            CompletionAnalyzer::new(file.ast(), span, source.text()).analyze()
        });
        if let Some(span) = span {
            let collector = CompletionCollector::new(context.as_ref());
            if matches!(context, Some(CompletionContext::ImportPath)) {
                return Ok(ImportPathCompletion::new(self.snapshot, uri, span).complete());
            }
            let Some(hir) = self
                .snapshot
                .hir_analysis_for_uri_with_token(uri, token)
                .map_err(map_project_error)?
            else {
                return Ok(CompletionResult {
                    items: self
                        .snapshot
                        .file_by_uri(uri)
                        .map(|file| collector.ast_items(file.ast()))
                        .unwrap_or_default(),
                });
            };
            if matches!(context, Some(CompletionContext::FieldAccess)) {
                return Ok(CompletionResult {
                    items: hir
                        .member_completion_items_at(span)
                        .into_iter()
                        .map(|item| CompletionItem {
                            label: item.label().to_string(),
                            kind: collector.kind_for(item.kind()),
                        })
                        .collect(),
                });
            }
            return Ok(CompletionResult {
                items: hir
                    .completion_items_at(span)
                    .into_iter()
                    .filter(|item| {
                        context
                            .as_ref()
                            .is_none_or(|context| context.accepts_semantic_kind(item.kind()))
                    })
                    .map(|item| CompletionItem {
                        label: item.label().to_string(),
                        kind: collector.kind_for(item.kind()),
                    })
                    .collect(),
            });
        }
        let mut items = Vec::new();
        if let Some(file) = self.snapshot.file_by_uri(uri) {
            items.extend(CompletionCollector::new(context.as_ref()).ast_items(file.ast()));
        }
        Ok(CompletionResult { items })
    }

    fn span_at(&self, uri: &DocumentUri, position: SourcePosition) -> Option<Span> {
        let file = self.snapshot.file_by_uri(uri)?;
        let source = self.snapshot.source_map().file(file.source_id())?;
        let offset = source.byte_offset_for_utf16_position(position);
        Some(Span::new_in(file.source_id(), offset, offset))
    }

    fn generic_definition(&self, uri: &DocumentUri, span: Span) -> Option<DefinitionResult> {
        let file = self.snapshot.file_by_uri(uri)?;
        let generic = GenericDefinitionResolver::new(span).resolve_file(file.ast())?;
        let range = self.snapshot.source_map().utf16_range(generic.span)?;
        Some(DefinitionResult {
            uri: file.uri().clone(),
            range,
        })
    }

    fn generic_hover(&self, uri: &DocumentUri, span: Span) -> Option<HoverResult> {
        let file = self.snapshot.file_by_uri(uri)?;
        let generic = GenericDefinitionResolver::new(span).resolve_file(file.ast())?;
        Some(HoverResult {
            contents: GenericParamHover::new(generic).contents(),
            range: self.snapshot.source_map().utf16_range(generic.span),
        })
    }
}

fn map_project_error(error: ProjectError) -> QueryError {
    match error {
        ProjectError::Cancelled => QueryError::Cancelled,
        other => unreachable!("query snapshots only surface cancellation, got {other}"),
    }
}

/// Protocol-neutral diagnostic queries for a session-owned project snapshot.
pub trait ProjectQueries {
    fn all_document_diagnostics(&self) -> Vec<crate::DocumentDiagnostics>;

    fn grouped_diagnostics(&self) -> GroupedDiagnostics;

    fn document_diagnostics(&self, uri: &DocumentUri) -> Option<crate::DocumentDiagnostics>;

    fn diagnostics_for(&self, uri: &DocumentUri) -> Vec<crate::DiagnosticResult>;
}

impl ProjectQueries for Project {
    fn all_document_diagnostics(&self) -> Vec<crate::DocumentDiagnostics> {
        self.snapshot().all_document_diagnostics()
    }

    fn grouped_diagnostics(&self) -> GroupedDiagnostics {
        self.snapshot().grouped_diagnostics()
    }

    fn document_diagnostics(&self, uri: &DocumentUri) -> Option<crate::DocumentDiagnostics> {
        self.snapshot().document_diagnostics(uri)
    }

    fn diagnostics_for(&self, uri: &DocumentUri) -> Vec<crate::DiagnosticResult> {
        self.snapshot().diagnostics_for(uri)
    }
}

#[non_exhaustive]
struct CompletionCollector<'a> {
    context: Option<&'a CompletionContext>,
}

impl<'a> CompletionCollector<'a> {
    fn new(context: Option<&'a CompletionContext>) -> Self {
        Self { context }
    }

    fn kind_for(&self, kind: CompletionKind) -> CompletionItemKind {
        match kind {
            CompletionKind::Module
            | CompletionKind::Cell
            | CompletionKind::ExternModule
            | CompletionKind::Instance => CompletionItemKind::Module,
            CompletionKind::Fn | CompletionKind::Map => CompletionItemKind::Function,
            CompletionKind::Enum | CompletionKind::Bundle | CompletionKind::Interface => {
                CompletionItemKind::Type
            }
            CompletionKind::Const | CompletionKind::Generic => CompletionItemKind::Constant,
            CompletionKind::Field | CompletionKind::View | CompletionKind::ViewField => {
                CompletionItemKind::Field
            }
            _ => CompletionItemKind::Keyword,
        }
    }

    fn ast_items(&self, file: &AstFile) -> Vec<CompletionItem> {
        file.items
            .iter()
            .filter_map(|item| match item {
                Item::Const(item)
                    if self.context.is_none_or(|context| {
                        context.accepts_item_kind(CompletionItemKind::Constant)
                    }) =>
                {
                    Some(CompletionItem {
                        label: item.name.clone(),
                        kind: CompletionItemKind::Constant,
                    })
                }
                Item::Fn(item)
                    if self.context.is_none_or(|context| {
                        context.accepts_item_kind(CompletionItemKind::Function)
                    }) =>
                {
                    Some(CompletionItem {
                        label: item.name.clone(),
                        kind: CompletionItemKind::Function,
                    })
                }
                Item::Map(item)
                    if self.context.is_none_or(|context| {
                        context.accepts_item_kind(CompletionItemKind::Function)
                    }) =>
                {
                    Some(CompletionItem {
                        label: item.name.clone(),
                        kind: CompletionItemKind::Function,
                    })
                }
                Item::Enum(item)
                    if self.context.is_none_or(|context| {
                        context.accepts_item_kind(CompletionItemKind::Type)
                    }) =>
                {
                    Some(CompletionItem {
                        label: item.name.clone(),
                        kind: CompletionItemKind::Type,
                    })
                }
                Item::Bundle(item)
                    if self.context.is_none_or(|context| {
                        context.accepts_item_kind(CompletionItemKind::Type)
                    }) =>
                {
                    Some(CompletionItem {
                        label: item.name.clone(),
                        kind: CompletionItemKind::Type,
                    })
                }
                Item::Interface(item)
                    if self.context.is_none_or(|context| {
                        context.accepts_item_kind(CompletionItemKind::Type)
                    }) =>
                {
                    Some(CompletionItem {
                        label: item.name.clone(),
                        kind: CompletionItemKind::Type,
                    })
                }
                Item::Cell(item) | Item::Module(item)
                    if self.context.is_none_or(|context| {
                        context.accepts_item_kind(CompletionItemKind::Module)
                    }) =>
                {
                    Some(CompletionItem {
                        label: item.name.clone(),
                        kind: CompletionItemKind::Module,
                    })
                }
                Item::ExternModule(item)
                    if self.context.is_none_or(|context| {
                        context.accepts_item_kind(CompletionItemKind::Module)
                    }) =>
                {
                    Some(CompletionItem {
                        label: item.name.clone(),
                        kind: CompletionItemKind::Module,
                    })
                }
                Item::Package(_) | Item::Use(_) | Item::Error(_) => None,
                _ => None,
            })
            .collect()
    }
}
