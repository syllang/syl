use super::{
    editor_api::{check, map_error},
    ident_span,
};
use crate::{InlayHintKind, InlayHintResult, QueryError};
use std::collections::HashMap;
use syl_hir::{DefId, HirDefKind, HirLocalKind, MirTypeRef};
use syl_sema::{
    CapabilityKind, DomainFact, HirFactId, Layout, LayoutConst, TirAnalysis,
    ir::const_mir::ConstValue, tir::TirType,
};
use syl_session::{AnalysisSnapshot, CancellationToken, DocumentUri};
use syl_span::{SourceRange, Span};
use syl_syntax::{AstNodeKind, token::TokenKind};

pub(super) fn query(
    snapshot: &AnalysisSnapshot,
    uri: &DocumentUri,
    range: SourceRange,
    token: &CancellationToken,
) -> Result<Vec<InlayHintResult>, QueryError> {
    check(token)?;
    let Some(file) = snapshot.file_by_uri(uri) else {
        return Ok(Vec::new());
    };
    let Some(source) = snapshot.source_map().file(file.source_id()) else {
        return Ok(Vec::new());
    };
    let start = source.byte_offset_for_utf16_position(range.start);
    let end = source.byte_offset_for_utf16_position(range.end);
    if start > end {
        return Ok(Vec::new());
    }
    let Some(tir) = snapshot
        .tir_analysis_for_uri_with_token(uri, token)
        .map_err(map_error)?
    else {
        return Ok(Vec::new());
    };
    let hir = tir.design().hir();
    let mut hints = Vec::new();
    let mut emit = |offset, label, tooltip, padding_left| {
        if offset < start || offset > end {
            return;
        }
        if let Some(range) = source.utf16_range(Span::new_in(file.source_id(), offset, offset)) {
            hints.push(InlayHintResult {
                position: range.end,
                label,
                kind: InlayHintKind::Type,
                tooltip: Some(tooltip),
                padding_left,
                padding_right: false,
            });
        }
    };
    for local in hir
        .locals()
        .iter()
        .filter(|l| l.span.source == file.source_id())
    {
        check(token)?;
        if local.span.end < start || local.span.start > end {
            continue;
        }
        let Some(name_span) = ident_span::local_name_span(snapshot.source_map(), local) else {
            continue;
        };
        let fact = HirFactId::Local(local.id);
        if matches!(
            local.kind,
            HirLocalKind::Let
                | HirLocalKind::Var
                | HirLocalKind::Signal
                | HirLocalKind::Reg
                | HirLocalKind::Const
        ) {
            let tokens = ident_span::tokens(snapshot.source_map(), local.span);
            let annotated = tokens
                .iter()
                .find(|t| t.span.start >= name_span.end)
                .is_some_and(|t| t.kind == TokenKind::Colon);
            if !annotated {
                let label = tir
                    .facts()
                    .types()
                    .ty(fact)
                    .filter(|t| !matches!(t, TirType::Unknown))
                    .map(TirType::label)
                    .or_else(|| initializer_label(tir, file.ast_node_index(), local.span));
                if let Some(label) = label {
                    emit(
                        name_span.end,
                        format!(": {label}"),
                        format!("inferred type: {label}"),
                        false,
                    );
                }
            }
        }
        let port = matches!(local.kind, HirLocalKind::Param | HirLocalKind::Result)
            && hir
                .def(local.owner)
                .is_some_and(|d| matches!(d.kind, HirDefKind::Cell | HirDefKind::ExternCell));
        if port {
            let annotation = hir
                .type_refs()
                .iter()
                .filter(|t| t.owner == local.owner && ident_span::contains(local.span, t.span))
                .max_by_key(|t| t.span.end - t.span.start);
            let layout = tir
                .facts()
                .types()
                .get(fact)
                .and_then(|id| tir.facts().layouts().get(id));
            let width = layout
                .and_then(layout_width)
                .or_else(|| annotation.and_then(|t| type_width(tir, local.owner, &t.ty, 0)));
            if let Some(bits) = width
                && let Some(ty) = annotation
            {
                let literal = ty.ty.path_name() == Some("Bit")
                    || (matches!(ty.ty.type_name(), Some("UInt" | "Bits" | "SInt"))
                        && ty
                            .ty
                            .args()
                            .and_then(|a| a.first())
                            .and_then(|a| a.path_name())
                            .and_then(|s| s.parse::<u64>().ok())
                            == Some(bits));
                if !literal {
                    emit(
                        local.span.end,
                        format!("[{bits}b]"),
                        format!("port width: {bits} bits"),
                        true,
                    );
                }
            }
            if let Some((domain, kind)) = domain(tir, fact) {
                emit(
                    local.span.end,
                    format!("@{domain}"),
                    format!("{kind} domain: {domain}"),
                    true,
                );
            }
        }
        if matches!(local.kind, HirLocalKind::Reg) {
            let reset = file.ast_node_index().nodes().iter().find(|n| {
                n.kind() == AstNodeKind::RegReset && ident_span::contains(local.span, n.span())
            });
            if let Some(reset) = reset {
                let first = file
                    .ast_node_index()
                    .nodes()
                    .iter()
                    .find(|n| n.parent() == Some(reset.id()));
                if let Some(expr) =
                    first.and_then(|n| hir.exprs().iter().find(|e| e.span == n.span()))
                    && let Some((domain, kind)) =
                        domain(tir, HirFactId::Expr(expr.id)).or_else(|| {
                            let tokens = ident_span::tokens(snapshot.source_map(), expr.span);
                            let [
                                syl_syntax::token::Token {
                                    kind: TokenKind::Ident(name),
                                    ..
                                },
                            ] = tokens.as_slice()
                            else {
                                return None;
                            };
                            let mut candidates = hir
                                .locals()
                                .iter()
                                .filter(|l| l.owner == local.owner && l.name == *name);
                            let candidate = candidates.next()?;
                            if candidates.next().is_some() {
                                return None;
                            }
                            domain(tir, HirFactId::Local(candidate.id))
                        })
                {
                    let offset = hir
                        .type_refs()
                        .iter()
                        .filter(|t| {
                            t.owner == local.owner && ident_span::contains(local.span, t.span)
                        })
                        .map(|t| t.span.end)
                        .max()
                        .unwrap_or(name_span.end);
                    emit(
                        offset,
                        format!("@{domain}"),
                        format!("{kind} domain: {domain}"),
                        true,
                    );
                }
            }
        }
    }
    hints.sort_by(|a, b| {
        (a.position.line, a.position.character, &a.label).cmp(&(
            b.position.line,
            b.position.character,
            &b.label,
        ))
    });
    check(token)?;
    Ok(hints)
}

fn domain(tir: &TirAnalysis, fact: HirFactId) -> Option<(String, &'static str)> {
    let fact = if tir.facts().capabilities().get(fact).is_none() {
        match fact {
            HirFactId::Expr(id) => match tir.design().hir().expr_resolutions().get(&id) {
                Some(syl_hir::HirResolution::Local(id)) => HirFactId::Local(*id),
                _ => fact,
            },
            _ => fact,
        }
    } else {
        fact
    };
    let (domain, kind) = match tir.facts().capabilities().get(fact)?.kind() {
        CapabilityKind::Clock { domain } => (domain, "clock"),
        CapabilityKind::Reset { domain } => (domain, "reset"),
        _ => return None,
    };
    // The domain fact's type ID can describe the generic's kind (Domain).
    // The clock/reset type retains its actual domain argument's identity.
    let argument = match tir.facts().types().ty(fact) {
        Some(TirType::Clock { domain } | TirType::Reset { domain }) => domain.as_deref(),
        _ => None,
    };
    let label = if let Some(TirType::Named { name, .. }) = argument {
        name.clone()
    } else {
        match domain {
            DomainFact::Named(id) => tir.facts().types().type_table().get(*id)?.label(),
            DomainFact::BuiltinDomain => "builtin".into(),
            _ => "default".into(),
        }
    };
    Some((label, kind))
}

fn layout_width(layout: &Layout) -> Option<u64> {
    match layout {
        Layout::Bit => Some(1),
        Layout::Word {
            width: LayoutConst::Known(w),
            ..
        } => Some(*w),
        Layout::Array {
            len: LayoutConst::Known(n),
            elem,
        } => layout_width(elem).map(|w| n.saturating_mul(w)),
        _ => None,
    }
}

fn number(tir: &TirAnalysis, owner: DefId, name: &str) -> Option<u64> {
    name.parse().ok().or_else(|| {
        let id = tir
            .design()
            .hir()
            .visible_def_ids(owner)
            .into_iter()
            .find(|id| tir.design().hir().def_name(*id) == Some(name))?;
        match tir.facts().consts().value(HirFactId::Def(id))? {
            ConstValue::Nat(value) => Some(value),
            _ => None,
        }
    })
}

fn type_width(tir: &TirAnalysis, owner: DefId, ty: &MirTypeRef, depth: usize) -> Option<u64> {
    if depth > 32 {
        return None;
    }
    if let Some((len, elem)) = ty.array() {
        let len = len
            .nat_value()
            .or_else(|| len.ident().and_then(|s| number(tir, owner, s)))?;
        return Some(len.saturating_mul(type_width(tir, owner, elem, depth + 1)?));
    }
    match ty.type_name()? {
        "Bit" => return Some(1),
        "UInt" | "Bits" | "SInt" => return number(tir, owner, ty.args()?.first()?.path_name()?),
        "Clock" | "Reset" | "Domain" => return None,
        _ => {}
    }
    let hir = tir.design().hir();
    let def = hir.type_def_for_mir_type(owner, ty)?;
    if let Some(item) = hir.enums().get(&def) {
        if let Some(width) = &item.width {
            return number(tir, def, width.path_name()?);
        }
        let maximum = tir
            .design()
            .enum_variant_values()
            .iter()
            .filter(|(key, _)| key.enum_def == def)
            .map(|(_, value)| *value)
            .max();
        let max = maximum.unwrap_or(item.variants.len().saturating_sub(1) as u64);
        return Some(u64::from((64 - max.leading_zeros()).max(1)));
    }
    let (generics, fields) = if let Some(item) = hir.bundles().get(&def) {
        (&item.generics, &item.fields)
    } else if let Some(item) = hir.structs().get(&def) {
        (&item.generics, &item.fields)
    } else if let Some(item) = hir.interfaces().get(&def) {
        (&item.generics, &item.fields)
    } else {
        return None;
    };
    let substitutions: HashMap<_, _> = generics
        .iter()
        .zip(ty.args().unwrap_or_default())
        .map(|(g, a)| (g.name.clone(), a.clone()))
        .collect();
    fields.iter().try_fold(0u64, |sum, field| {
        type_width(tir, def, &field.ty.subst(&substitutions), depth + 1)
            .map(|width| sum.saturating_add(width))
    })
}

// Software bodies need not have a stored local type. Only project a known
// initializer fact (or a literal's intrinsic type), never guess an unknown type.
fn initializer_label(
    tir: &TirAnalysis,
    index: &syl_syntax::AstNodeIndex,
    span: Span,
) -> Option<String> {
    let binding = index.nodes().iter().find(|n| {
        n.span() == span
            && matches!(
                n.kind(),
                AstNodeKind::LetStmt
                    | AstNodeKind::VarStmt
                    | AstNodeKind::SignalStmt
                    | AstNodeKind::ConstStmt
            )
    })?;
    let value = index
        .nodes()
        .iter()
        .find(|n| n.parent() == Some(binding.id()))?;
    let hir = tir.design().hir();
    if let Some(expr) = hir.exprs().iter().find(|e| e.span == value.span()) {
        if let Some(ty) = tir
            .facts()
            .types()
            .ty(HirFactId::Expr(expr.id))
            .filter(|t| !matches!(t, TirType::Unknown))
        {
            return Some(ty.label());
        }
        let fact = match hir.expr_resolutions().get(&expr.id) {
            Some(syl_hir::HirResolution::Local(id)) => Some(HirFactId::Local(*id)),
            Some(syl_hir::HirResolution::Def(id)) => Some(HirFactId::Def(*id)),
            _ => None,
        };
        if let Some(ty) = fact
            .and_then(|f| tir.facts().types().ty(f))
            .filter(|t| !matches!(t, TirType::Unknown))
        {
            return Some(ty.label());
        }
    }
    match value.kind() {
        AstNodeKind::IntExpr => Some(TirType::Nat.label()),
        AstNodeKind::BoolExpr => Some(TirType::Bool.label()),
        AstNodeKind::StrExpr => Some(TirType::Str.label()),
        _ => None,
    }
}
