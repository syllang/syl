use super::{editor_api::check, ident_span};
use crate::{DocumentSymbolKind as Kind, QueryError, WorkspaceSymbolResult};
use syl_session::{AnalysisSnapshot, CancellationToken};
use syl_syntax::Item;

pub(super) fn query(
    snapshot: &AnalysisSnapshot,
    query: &str,
    token: &CancellationToken,
) -> Result<Vec<WorkspaceSymbolResult>, QueryError> {
    check(token)?;
    let mut symbols = Vec::new();
    for file in snapshot.files() {
        check(token)?;
        let container = file.module_path().join(".");
        for item in &file.ast().items {
            check(token)?;
            let (name, kind) = match item {
                Item::Const(i) => (&i.name, Kind::Constant),
                Item::Fn(i) => (&i.name, Kind::Function),
                Item::Map(i) => (&i.name, Kind::Function),
                Item::Enum(i) => (&i.name, Kind::Type),
                Item::Struct(i) => (&i.name, Kind::Type),
                Item::Bundle(i) => (&i.name, Kind::Type),
                Item::Interface(i) => (&i.name, Kind::Type),
                Item::Cell(i) => (&i.name, Kind::Module),
                Item::ExternCell(i) => (&i.name, Kind::Module),
                _ => continue,
            };
            let mut add = |name: &str, kind, span, container: &str| {
                if let Some(span) = ident_span::name_span(snapshot.source_map(), span, name, false)
                    && let Some(location) = ident_span::location(snapshot, span)
                {
                    symbols.push(WorkspaceSymbolResult {
                        name: name.into(),
                        kind,
                        container_name: Some(container.into()),
                        location,
                    });
                }
            };
            add(name, kind, item.span(), &container);
            let parent = if container.is_empty() {
                name.clone()
            } else {
                format!("{container}.{name}")
            };
            match item {
                Item::Enum(i) => {
                    for variant in &i.variants {
                        add(&variant.name, Kind::EnumMember, variant.span, &parent);
                    }
                }
                Item::Interface(i) => {
                    for view in &i.views {
                        add(&view.name, Kind::View, view.span, &parent);
                    }
                }
                _ => {}
            }
        }
    }
    if query.is_empty() {
        symbols.sort_by(|a, b| {
            (&a.container_name, &a.name, &a.location.uri).cmp(&(
                &b.container_name,
                &b.name,
                &b.location.uri,
            ))
        });
        return Ok(symbols);
    }
    let mut ranked: Vec<_> = symbols
        .into_iter()
        .filter_map(|s| score(&s.name, query).map(|rank| (rank, s)))
        .collect();
    ranked.sort_by(|(a, x), (b, y)| {
        (
            a,
            x.name.len(),
            &x.name,
            &x.location.uri,
            ident_span::range_key(x.location.range),
        )
            .cmp(&(
                b,
                y.name.len(),
                &y.name,
                &y.location.uri,
                ident_span::range_key(y.location.range),
            ))
    });
    check(token)?;
    Ok(ranked.into_iter().map(|(_, s)| s).collect())
}

fn score(name: &str, query: &str) -> Option<(u8, usize)> {
    let lower = name.to_lowercase();
    let query = query.to_lowercase();
    if lower == query {
        return Some((0, 0));
    }
    if lower.starts_with(&query) {
        return Some((1, 0));
    }
    if lower.contains(&query) {
        return Some((2, 0));
    }
    let mut remaining = query.chars().peekable();
    let mut penalty = 0;
    let mut previous = '_';
    for (index, ch) in name.chars().enumerate() {
        if remaining
            .peek()
            .is_some_and(|wanted| *wanted == ch.to_ascii_lowercase())
        {
            if index != 0 && previous != '_' && !(ch.is_uppercase() && previous.is_lowercase()) {
                penalty += 1;
            }
            remaining.next();
        }
        previous = ch;
    }
    remaining.peek().is_none().then_some((3, penalty))
}
