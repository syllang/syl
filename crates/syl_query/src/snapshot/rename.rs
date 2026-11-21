use super::{
    editor_api::check,
    references::{self, Resolved},
};
use crate::{PrepareRenameResult, QueryError, TextEditResult, WorkspaceEditResult};
use syl_sema::{HirFactId, SemanticResolution};
use syl_session::{AnalysisSnapshot, CancellationToken, DocumentUri};
use syl_span::SourcePosition;
use syl_syntax::{
    lexer::{LexemeKind, LosslessLexer},
    token::TokenKind,
};

pub(super) fn builtin(name: &str) -> bool {
    matches!(
        name,
        "Bit"
            | "UInt"
            | "Bits"
            | "SInt"
            | "Clock"
            | "Reset"
            | "Domain"
            | "nat"
            | "bool"
            | "string"
            | "Str"
            | "compile_error"
    )
}

fn invalid(message: impl Into<String>) -> QueryError {
    QueryError::InvalidRename {
        message: message.into(),
    }
}

fn validate(name: &str) -> Result<(), QueryError> {
    let lexed = LosslessLexer::new(name).lex_all_partial();
    if builtin(name)
        || !lexed.diagnostics.is_empty()
        || lexed.lexemes.len() != 1
        || !matches!(
            lexed.lexemes[0].kind,
            LexemeKind::Token(TokenKind::Ident(_))
        )
        || lexed.lexemes[0].span.start != 0
        || lexed.lexemes[0].span.end != name.len()
    {
        return Err(invalid("expected a non-reserved Syl identifier"));
    }
    Ok(())
}

fn renameable(resolved: &Resolved<'_>) -> bool {
    references::name(resolved.hir.design(), resolved.target)
        .is_some_and(|name| name != "this" && !builtin(name))
}

pub(super) fn prepare(
    snapshot: &AnalysisSnapshot,
    uri: &DocumentUri,
    position: SourcePosition,
    token: &CancellationToken,
) -> Result<Option<PrepareRenameResult>, QueryError> {
    let Some(resolved) = references::resolve(snapshot, uri, position, token)? else {
        return Ok(None);
    };
    if !renameable(&resolved) {
        return Ok(None);
    }
    Ok(snapshot
        .source_map()
        .utf16_range(resolved.cursor_span)
        .map(|range| PrepareRenameResult {
            range,
            placeholder: references::name(resolved.hir.design(), resolved.target)
                .unwrap()
                .into(),
        }))
}

pub(super) fn rename(
    snapshot: &AnalysisSnapshot,
    uri: &DocumentUri,
    position: SourcePosition,
    new_name: &str,
    token: &CancellationToken,
) -> Result<Option<WorkspaceEditResult>, QueryError> {
    check(token)?;
    validate(new_name)?;
    let Some(resolved) = references::resolve(snapshot, uri, position, token)? else {
        return Ok(None);
    };
    if !renameable(&resolved) {
        return Ok(None);
    }
    if references::name(resolved.hir.design(), resolved.target) == Some(new_name) {
        return Ok(Some(WorkspaceEditResult::default()));
    }
    collisions(&resolved, new_name, token)?;
    let mut result = WorkspaceEditResult::default();
    for site in references::locations(snapshot, &resolved, true) {
        check(token)?;
        let edits = result.changes.entry(site.uri).or_default();
        if edits.last().is_some_and(|previous| {
            super::ident_span::range_key(previous.range).2 > site.range.start.line
                || (previous.range.end.line == site.range.start.line
                    && previous.range.end.character > site.range.start.character)
        }) {
            return Err(invalid("overlapping reference ranges"));
        }
        edits.push(TextEditResult {
            range: site.range,
            new_text: new_name.into(),
        });
    }
    Ok(Some(result))
}

fn collisions(
    resolved: &Resolved<'_>,
    new_name: &str,
    token: &CancellationToken,
) -> Result<(), QueryError> {
    let hir = resolved.hir.design();
    let graph = resolved.hir.resolution().graph();
    let conflict = match resolved.target {
        SemanticResolution::Def(id) => {
            let Some(definition) = graph.definition(id) else {
                return Ok(());
            };
            let mut packages = vec![definition.package()];
            packages.extend(
                graph
                    .imports()
                    .iter()
                    .filter(|i| i.target() == Some(id))
                    .map(|i| i.package()),
            );
            let mut conflict = false;
            for package in packages {
                check(token)?;
                conflict |= graph
                    .package_definitions(package)
                    .iter()
                    .any(|other| *other != id && hir.def_name(*other) == Some(new_name));
                conflict |= graph
                    .package_imports(package)
                    .iter()
                    .filter_map(|i| graph.import(*i))
                    .any(|i| i.target() != Some(id) && i.path().leaf() == Some(new_name));
            }
            // A new definition name must not become captured by a local at a use site.
            conflict
                || resolved
                    .sites
                    .iter()
                    .filter(|s| s.target == resolved.target && !s.declaration)
                    .any(|site| {
                        hir.defs()
                            .iter()
                            .filter(|d| super::ident_span::contains(d.span, site.span))
                            .any(|owner| {
                                hir.locals()
                                    .iter()
                                    .any(|l| l.owner == owner.id && l.name == new_name)
                            })
                    })
        }
        SemanticResolution::Local(id) => {
            let Some(local) = hir.local(id) else {
                return Ok(());
            };
            hir.locals().iter().any(|l| l.id != id && l.owner == local.owner && l.name == new_name)
                || resolved.hir.resolution().entries().any(|(fact, target)| {
                    matches!((fact, target), (HirFactId::Expr(expr), SemanticResolution::Def(def))
                        if hir.expr(expr).is_some_and(|e| e.owner == local.owner) && hir.def_name(def) == Some(new_name))
                })
        }
        _ => false,
    };
    check(token)?;
    if conflict {
        Err(invalid(format!(
            "'{new_name}' conflicts with an existing binding"
        )))
    } else {
        Ok(())
    }
}
