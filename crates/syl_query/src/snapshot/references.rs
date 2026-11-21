use super::{
    editor_api::{check, map_error},
    ident_span::{self, contains},
};
use crate::{LocationResult, QueryError};
use syl_hir::{HirDesign, HirLocalKind};
use syl_sema::{HirAnalysis, HirFactId, SemanticResolution};
use syl_session::{AnalysisSnapshot, CancellationToken, DocumentUri};
use syl_span::{SourcePosition, Span};
use syl_syntax::{AstNodeKind, token::TokenKind};

#[derive(Clone, Copy)]
pub(super) struct Site {
    pub target: SemanticResolution,
    pub span: Span,
    pub declaration: bool,
}

pub(super) struct Resolved<'a> {
    pub hir: &'a HirAnalysis,
    pub target: SemanticResolution,
    pub cursor_span: Span,
    pub sites: Vec<Site>,
}

pub(super) fn query(
    snapshot: &AnalysisSnapshot,
    uri: &DocumentUri,
    position: SourcePosition,
    include_declaration: bool,
    token: &CancellationToken,
) -> Result<Vec<LocationResult>, QueryError> {
    let Some(resolved) = resolve(snapshot, uri, position, token)? else {
        return Ok(Vec::new());
    };
    Ok(locations(snapshot, &resolved, include_declaration))
}

pub(super) fn resolve<'a>(
    snapshot: &'a AnalysisSnapshot,
    uri: &DocumentUri,
    position: SourcePosition,
    token: &CancellationToken,
) -> Result<Option<Resolved<'a>>, QueryError> {
    check(token)?;
    let Some(cursor) = ident_span::cursor(snapshot, uri, position) else {
        return Ok(None);
    };
    let Some(hir) = snapshot
        .hir_analysis_for_uri_with_token(uri, token)
        .map_err(map_error)?
    else {
        return Ok(None);
    };
    let sites = collect_sites(snapshot, hir, token)?;
    let Some(site) = at_cursor(&sites, cursor) else {
        return Ok(None);
    };
    let global = match site.target {
        SemanticResolution::Def(_) => true,
        SemanticResolution::Local(id) => hir
            .design()
            .local(id)
            .is_some_and(|l| matches!(l.kind, HirLocalKind::Param)),
        _ => false,
    };
    if !global {
        return Ok(Some(Resolved {
            hir,
            target: site.target,
            cursor_span: site.span,
            sites,
        }));
    }
    // IDs belong to their analysis arena. Resolve the cursor again after widening.
    let hir = snapshot.hir_analysis_with_token(token).map_err(map_error)?;
    let sites = collect_sites(snapshot, hir, token)?;
    Ok(at_cursor(&sites, cursor).map(|site| Resolved {
        hir,
        target: site.target,
        cursor_span: site.span,
        sites,
    }))
}

fn at_cursor(sites: &[Site], cursor: Span) -> Option<Site> {
    sites
        .iter()
        .filter(|site| contains(site.span, cursor))
        .min_by_key(|site| {
            (
                usize::from(site.span.end == cursor.start),
                site.span.end - site.span.start,
            )
        })
        .copied()
}

pub(super) fn name(hir: &HirDesign, target: SemanticResolution) -> Option<&str> {
    match target {
        SemanticResolution::Def(id) => hir.def_name(id),
        SemanticResolution::Local(id) => hir.local(id).map(|l| l.name.as_str()),
        _ => None,
    }
}

pub(super) fn locations(
    snapshot: &AnalysisSnapshot,
    resolved: &Resolved<'_>,
    include: bool,
) -> Vec<LocationResult> {
    let mut result: Vec<_> = resolved
        .sites
        .iter()
        .filter(|s| s.target == resolved.target && (include || !s.declaration))
        .filter_map(|s| ident_span::location(snapshot, s.span))
        .collect();
    result.sort_by(|a, b| {
        (&a.uri, ident_span::range_key(a.range)).cmp(&(&b.uri, ident_span::range_key(b.range)))
    });
    result.dedup();
    result
}

pub(super) fn collect_sites(
    snapshot: &AnalysisSnapshot,
    hir: &HirAnalysis,
    token: &CancellationToken,
) -> Result<Vec<Site>, QueryError> {
    let design = hir.design();
    let map = snapshot.source_map();
    let mut sites = Vec::new();
    for def in design.defs() {
        check(token)?;
        add(
            &mut sites,
            SemanticResolution::Def(def.id),
            ident_span::def_name_span(map, def),
            true,
        );
    }
    for local in design.locals() {
        check(token)?;
        add(
            &mut sites,
            SemanticResolution::Local(local.id),
            ident_span::local_name_span(map, local),
            true,
        );
    }
    for (fact, target) in hir.resolution().entries() {
        check(token)?;
        if let HirFactId::Expr(id) = fact
            && let Some(expr) = design.expr(id)
            && let Some(name) = name(design, target)
        {
            add(
                &mut sites,
                target,
                ident_span::name_span(map, expr.span, name, false),
                false,
            );
        }
    }
    for import in hir.resolution().graph().imports() {
        check(token)?;
        if let Some(id) = import.target()
            && let Some(name) = design.def_name(id)
        {
            add(
                &mut sites,
                SemanticResolution::Def(id),
                ident_span::name_span(map, import.span(), name, true),
                false,
            );
        }
    }
    for ty in design.type_refs() {
        check(token)?;
        let Some(path) = ty.ty.path() else {
            continue;
        };
        let Some(leaf) = path.last() else {
            continue;
        };
        let generic = design.locals().iter().find(|l| {
            l.owner == ty.owner
                && l.name == *leaf
                && path.len() == 1
                && matches!(l.kind, HirLocalKind::Generic)
        });
        let target = generic
            .map(|l| SemanticResolution::Local(l.id))
            .or_else(|| {
                design
                    .resolved_type_def_for_ref(ty)
                    .map(SemanticResolution::Def)
            });
        if let Some(target) = target {
            add(
                &mut sites,
                target,
                ident_span::name_span(map, ty.span, leaf, true),
                false,
            );
        }
    }
    supplemental_sites(snapshot, hir, token, &mut sites)?;
    Ok(sites)
}

fn add(sites: &mut Vec<Site>, target: SemanticResolution, span: Option<Span>, declaration: bool) {
    if let Some(span) = span {
        sites.push(Site {
            target,
            span,
            declaration,
        });
    }
}

fn supplemental_sites(
    snapshot: &AnalysisSnapshot,
    hir: &HirAnalysis,
    token: &CancellationToken,
    sites: &mut Vec<Site>,
) -> Result<(), QueryError> {
    let design = hir.design();
    for file in snapshot.files() {
        check(token)?;
        if !design
            .defs()
            .iter()
            .any(|d| d.span.source == file.source_id())
        {
            continue;
        }
        let index = file.ast_node_index();
        for node in index.nodes() {
            check(token)?;
            if node.kind() == AstNodeKind::NextStmt {
                let tokens = ident_span::tokens(snapshot.source_map(), node.span());
                let Some(ident) = tokens.get(1) else {
                    continue;
                };
                let TokenKind::Ident(name) = &ident.kind else {
                    continue;
                };
                let Some(owner) = design.defs().iter().find(|d| contains(d.span, node.span()))
                else {
                    continue;
                };
                let local = design
                    .locals()
                    .iter()
                    .filter(|l| {
                        l.owner == owner.id && l.name == *name && l.span.start < node.span().start
                    })
                    .filter(|l| {
                        index
                            .nodes()
                            .iter()
                            .filter(|n| {
                                n.kind() == AstNodeKind::Block && contains(n.span(), l.span)
                            })
                            .min_by_key(|n| n.span().end - n.span().start)
                            .is_none_or(|scope| contains(scope.span(), node.span()))
                    })
                    .max_by_key(|l| l.span.start);
                if let Some(local) = local.filter(|l| matches!(l.kind, HirLocalKind::Reg)) {
                    add(
                        sites,
                        SemanticResolution::Local(local.id),
                        Some(ident.span),
                        false,
                    );
                }
            } else if node.kind() == AstNodeKind::CallArg {
                let tokens = ident_span::tokens(snapshot.source_map(), node.span());
                let [ident, colon, ..] = tokens.as_slice() else {
                    continue;
                };
                let TokenKind::Ident(name) = &ident.kind else {
                    continue;
                };
                if colon.kind != TokenKind::Colon {
                    continue;
                }
                let Some(call) = node.parent().and_then(|id| index.record(id)) else {
                    continue;
                };
                if !matches!(call.kind(), AstNodeKind::CallExpr | AstNodeKind::PlaceExpr) {
                    continue;
                }
                // The callee is the first direct child, before any argument node.
                let Some(callee) = index.nodes().iter().find(|n| n.parent() == Some(call.id()))
                else {
                    continue;
                };
                let target = design
                    .exprs()
                    .iter()
                    .filter(|e| contains(callee.span(), e.span))
                    .filter_map(|e| match hir.resolution().get(HirFactId::Expr(e.id)) {
                        Some(SemanticResolution::Def(id)) => Some((e.span.end - e.span.start, id)),
                        _ => None,
                    })
                    .min_by_key(|(width, _)| *width)
                    .map(|(_, id)| id);
                if let Some(owner) = target
                    && let Some(local) = design.locals().iter().find(|l| {
                        l.owner == owner && l.name == *name && matches!(l.kind, HirLocalKind::Param)
                    })
                {
                    add(
                        sites,
                        SemanticResolution::Local(local.id),
                        Some(ident.span),
                        false,
                    );
                }
            }
        }
    }
    Ok(())
}
