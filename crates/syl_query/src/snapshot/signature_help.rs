use super::{
    editor_api::{check, map_error},
    ident_span,
};
use crate::{ParameterInfoResult, QueryError, SignatureHelpResult, SignatureInfoResult};
use syl_hir::{DefId, HirDefKind, HirDesign, HirSignatureParam};
use syl_sema::{HirAnalysis, HirFactId, SemanticResolution};
use syl_session::{AnalysisSnapshot, CancellationToken, DocumentUri};
use syl_span::{SourcePosition, Span};
use syl_syntax::token::{Token, TokenKind};

pub(super) fn query(
    snapshot: &AnalysisSnapshot,
    uri: &DocumentUri,
    position: SourcePosition,
    token: &CancellationToken,
) -> Result<Option<SignatureHelpResult>, QueryError> {
    check(token)?;
    let Some(cursor) = ident_span::cursor(snapshot, uri, position) else {
        return Ok(None);
    };
    let tokens = ident_span::tokens(
        snapshot.source_map(),
        Span::new_in(cursor.source, 0, cursor.start),
    );
    let Some((open, callee)) = active_call(&tokens) else {
        return Ok(None);
    };
    let method = callee > 0 && tokens[callee - 1].kind == TokenKind::Dot;
    let Some(hir) = snapshot
        .hir_analysis_for_uri_with_token(uri, token)
        .map_err(map_error)?
    else {
        return Ok(None);
    };
    let module_path = snapshot
        .file_by_uri(uri)
        .map(|f| f.module_path())
        .unwrap_or_default();
    let Some(def) = resolve_callee(hir, &tokens, callee, cursor, module_path) else {
        return Ok(None);
    };
    check(token)?;
    let Some(mut signature) = signature(snapshot, hir.design(), def, method) else {
        return Ok(None);
    };
    let params = parameters(hir.design(), def).unwrap_or_default();
    let names: Vec<_> = params
        .iter()
        .filter(|p| !method || !p.is_receiver())
        .map(|p| p.name.as_str())
        .collect();
    let active = active_parameter(&tokens[open + 1..], &names);
    signature.active_parameter = active;
    Ok(Some(SignatureHelpResult {
        signatures: vec![signature],
        active_signature: 0,
        active_parameter: active,
    }))
}

fn active_call(tokens: &[Token]) -> Option<(usize, usize)> {
    let mut stack = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        match token.kind {
            TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => stack.push(i),
            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                stack.pop();
            }
            _ => {}
        }
    }
    for open in stack.into_iter().rev() {
        if tokens[open].kind != TokenKind::LParen || open == 0 {
            continue;
        }
        let mut callee = open - 1;
        if tokens[callee].kind == TokenKind::Gt {
            let mut depth = 1;
            while callee > 0 {
                callee -= 1;
                match tokens[callee].kind {
                    TokenKind::Gt => depth += 1,
                    TokenKind::Lt => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
            }
            callee = callee.checked_sub(1)?;
        }
        if matches!(tokens[callee].kind, TokenKind::Ident(_)) {
            if callee > 0
                && matches!(
                    tokens[callee - 1].kind,
                    TokenKind::KwFn | TokenKind::KwCell | TokenKind::KwMap | TokenKind::KwExtern
                )
            {
                return None;
            }
            return Some((open, callee));
        }
    }
    None
}

fn resolve_callee(
    hir: &HirAnalysis,
    tokens: &[Token],
    callee: usize,
    cursor: Span,
    module_path: &[String],
) -> Option<DefId> {
    let method = callee > 0 && tokens[callee - 1].kind == TokenKind::Dot;
    let span = tokens[callee].span;
    let design = hir.design();
    let resolved = design
        .exprs()
        .iter()
        .filter(|e| ident_span::contains(e.span, span))
        .filter_map(|e| match hir.resolution().get(HirFactId::Expr(e.id)) {
            Some(SemanticResolution::Def(id)) => Some((e.span.end - e.span.start, id)),
            _ => None,
        })
        .min_by_key(|(width, _)| *width)
        .map(|(_, id)| id);
    if resolved.is_some() {
        return resolved;
    }
    let TokenKind::Ident(name) = &tokens[callee].kind else {
        return None;
    };
    let owner = design
        .defs()
        .iter()
        .find(|d| ident_span::contains(d.span, cursor));
    if method && callee >= 2 {
        let base = tokens[callee - 2].span;
        let local = design
            .exprs()
            .iter()
            .find(|e| e.span == base)
            .and_then(|e| match hir.resolution().get(HirFactId::Expr(e.id)) {
                Some(SemanticResolution::Local(id)) => design.local(id),
                _ => None,
            });
        if let Some(local) = local {
            let ty = design
                .type_refs()
                .iter()
                .find(|t| ident_span::contains(local.span, t.span))?;
            let receiver = design.resolved_type_def_for_ref(ty)?;
            let methods = design.extension_methods_for(receiver, name);
            return (methods.len() == 1).then(|| methods[0]);
        }
        return None;
    }
    let visible = if let Some(owner) = owner {
        design.visible_def_ids(owner.id)
    } else {
        let mut ids: Vec<_> = design
            .defs()
            .iter()
            .filter(|d| d.canonical_path.parent().segments() == module_path)
            .map(|d| d.id)
            .collect();
        ids.extend(
            hir.resolution()
                .graph()
                .imports()
                .iter()
                .filter(|i| i.span().source == cursor.source)
                .filter_map(|i| i.target()),
        );
        ids
    };
    let mut matches = visible
        .into_iter()
        .filter(|id| design.def_name(*id) == Some(name));
    let id = matches.next()?;
    matches.next().is_none().then_some(id)
}

fn parameters(hir: &HirDesign, def: DefId) -> Option<&[HirSignatureParam]> {
    hir.callables()
        .get(&def)
        .map(|c| c.params())
        .or_else(|| hir.fns().get(&def).map(|f| f.params.as_slice()))
        .or_else(|| hir.maps().get(&def).map(|m| m.params.as_slice()))
}

fn signature(
    snapshot: &AnalysisSnapshot,
    hir: &HirDesign,
    id: DefId,
    method: bool,
) -> Option<SignatureInfoResult> {
    let def = hir.def(id)?;
    if !matches!(
        def.kind,
        HirDefKind::Cell | HirDefKind::ExternCell | HirDefKind::Fn | HirDefKind::Map
    ) {
        return None;
    }
    let source = snapshot.source_map().file(def.span.source)?;
    let tokens = ident_span::tokens(snapshot.source_map(), def.span);
    let mut depth = 0usize;
    let open = tokens.iter().position(|t| match t.kind {
        TokenKind::Lt => {
            depth += 1;
            false
        }
        TokenKind::Gt => {
            depth = depth.saturating_sub(1);
            false
        }
        TokenKind::LParen => depth == 0,
        _ => false,
    })?;
    let mut depth = 1usize;
    let close = (open + 1..tokens.len()).find(|i| {
        match tokens[*i].kind {
            TokenKind::LParen => depth += 1,
            TokenKind::RParen => depth -= 1,
            _ => {}
        }
        depth == 0
    })?;
    let head = source
        .text()
        .get(def.span.start..tokens[open].span.start)?
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let tail_end = tokens[close + 1..]
        .iter()
        .find(|t| matches!(t.kind, TokenKind::LBrace | TokenKind::Eq | TokenKind::Semi))
        .map_or(def.span.end, |t| t.span.start);
    let tail = source
        .text()
        .get(tokens[close].span.end..tail_end)?
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let parameters: Vec<_> = parameters(hir, id)?
        .iter()
        .filter(|p| !method || !p.is_receiver())
        .map(|p| ParameterInfoResult {
            label: format!(
                "{}{}",
                if p.is_receiver() { "this " } else { "" },
                source
                    .text()
                    .get(p.span.start..p.span.end)
                    .unwrap_or(&p.name)
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            documentation: p.doc.clone(),
        })
        .collect();
    let joined = parameters
        .iter()
        .map(|p| p.label.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    Some(SignatureInfoResult {
        label: format!(
            "{head}({joined}){}",
            if tail.is_empty() {
                String::new()
            } else {
                format!(" {tail}")
            }
        ),
        documentation: hir.doc_for_item(id).map(str::to_owned),
        parameters,
        active_parameter: None,
    })
}

fn active_parameter(tokens: &[Token], names: &[&str]) -> Option<usize> {
    if names.is_empty() {
        return None;
    }
    let mut depth = 0usize;
    let mut count = 0;
    let mut fragment = 0;
    let mut i = 0;
    while i < tokens.len() {
        match tokens[i].kind {
            TokenKind::Lt => {
                if let Some(close) = generic_arguments_end(tokens, i) {
                    i = close;
                }
            }
            TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => depth += 1,
            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                depth = depth.saturating_sub(1)
            }
            TokenKind::Comma if depth == 0 => {
                count += 1;
                fragment = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    if let [
        Token {
            kind: TokenKind::Ident(name),
            ..
        },
        Token {
            kind: TokenKind::Colon,
            ..
        },
        ..,
    ] = &tokens[fragment..]
        && let Some(index) = names.iter().position(|n| *n == name)
    {
        return Some(index);
    }
    Some(count.min(names.len() - 1))
}

fn generic_arguments_end(tokens: &[Token], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, token) in tokens.iter().enumerate().skip(open) {
        match token.kind {
            TokenKind::Lt => depth += 1,
            TokenKind::Gt => {
                depth -= 1;
                if depth == 0 {
                    // Match the parser's generic-application lookahead: the
                    // closing angle must introduce a call or aggregate body.
                    return matches!(
                        tokens.get(i + 1).map(|t| &t.kind),
                        Some(TokenKind::LParen | TokenKind::LBrace)
                    )
                    .then_some(i);
                }
            }
            _ => {}
        }
    }
    None
}
