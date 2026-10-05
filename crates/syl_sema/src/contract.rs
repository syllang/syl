//! Span-free declaration facts and the body-derived hardware contract.
//!
//! Salsa does not live in this crate. `syl_session` turns these values into
//! tracked queries. A body-only edit must leave [`signature_stub_file`] and
//! [`declaration_contracts`] unchanged; a `reg` in a body changes only
//! [`HardwareContract`].

use syl_span::Span;
use syl_syntax::{AstFile, Block, Expr, Item, Stmt};

/// Identity of one declaration inside a module.
///
/// `discriminator` is the occurrence index of the same kind and name. It is
/// not stable across inserts. Arena ids are not part of this key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct SymbolKey {
    pub module: Vec<String>,
    pub kind: SymbolKind,
    pub name: String,
    pub discriminator: u32,
}

impl SymbolKey {
    pub fn new(
        module: Vec<String>,
        kind: SymbolKind,
        name: impl Into<String>,
        discriminator: u32,
    ) -> Self {
        Self {
            module,
            kind,
            name: name.into(),
            discriminator,
        }
    }
}

/// Declaration kinds that can appear in a signature projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SymbolKind {
    Error,
    Use,
    Const,
    Fn,
    Enum,
    Struct,
    Bundle,
    Interface,
    Map,
    Cell,
    ExternCell,
}

impl SymbolKind {
    pub fn tag(self) -> u8 {
        match self {
            Self::Error => 0,
            Self::Use => 1,
            Self::Const => 2,
            Self::Fn => 3,
            Self::Enum => 4,
            Self::Struct => 5,
            Self::Bundle => 6,
            Self::Interface => 7,
            Self::Map => 8,
            Self::Cell => 9,
            Self::ExternCell => 10,
        }
    }

    pub fn from_tag(tag: u8) -> Self {
        match tag {
            1 => Self::Use,
            2 => Self::Const,
            3 => Self::Fn,
            4 => Self::Enum,
            5 => Self::Struct,
            6 => Self::Bundle,
            7 => Self::Interface,
            8 => Self::Map,
            9 => Self::Cell,
            10 => Self::ExternCell,
            _ => Self::Error,
        }
    }
}

/// Narrow facts a downstream owner needs from a declaration.
///
/// `signature_source` keeps const values and enum variant values. Function,
/// cell, and map bodies are removed. Spans are not part of the value.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct DeclarationContract {
    pub key: SymbolKey,
    pub doc: Option<String>,
    pub signature_source: String,
    pub use_path: Option<Vec<String>>,
}

impl DeclarationContract {
    pub fn new(
        key: SymbolKey,
        doc: Option<String>,
        signature_source: impl Into<String>,
        use_path: Option<Vec<String>>,
    ) -> Self {
        Self {
            key,
            doc,
            signature_source: signature_source.into(),
            use_path,
        }
    }
}

/// Hardware facts that are derived from a body rather than a signature.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct HardwareContract {
    pub key: SymbolKey,
    pub has_storage: bool,
}

impl HardwareContract {
    pub fn new(key: SymbolKey, has_storage: bool) -> Self {
        Self { key, has_storage }
    }
}

/// Read-only view of declaration and hardware facts.
///
/// Import resolution reads [`imported_declaration`](Self::imported_declaration)
/// for the one named symbol. `hardware` is what elaboration reads. A default
/// `declarations` returns nothing so batch test doubles can override one side.
pub trait ContractProvider {
    fn declaration(&self, key: &SymbolKey) -> Option<DeclarationContract>;
    fn hardware(&self, key: &SymbolKey) -> Option<HardwareContract>;

    fn declarations(&self, module: &[String]) -> Vec<DeclarationContract> {
        let _ = module;
        Vec::new()
    }

    /// The one declaration an import path names. The default scans
    /// [`declarations`](Self::declarations); session queries override this so an
    /// importer does not read every signature in the module.
    fn imported_declaration(&self, module: &[String], name: &str) -> Option<DeclarationContract> {
        self.declarations(module)
            .into_iter()
            .find(|contract| contract.key.name == name)
    }
}

/// Keys in source order. Repeated names get increasing discriminators.
pub fn file_symbol_keys(module: &[String], file: &AstFile) -> Vec<SymbolKey> {
    let mut seen = Vec::<(SymbolKind, String, u32)>::new();
    file.items
        .iter()
        .map(|item| {
            let kind = item_kind(item);
            let name = item_name(item);
            let discriminator = seen
                .iter()
                .filter(|(seen_kind, seen_name, _)| *seen_kind == kind && seen_name == &name)
                .count() as u32;
            seen.push((kind, name.clone(), discriminator));
            SymbolKey {
                module: module.to_vec(),
                kind,
                name,
                discriminator,
            }
        })
        .collect()
}

pub fn declaration_contracts(
    module: &[String],
    text: &str,
    file: &AstFile,
) -> Vec<DeclarationContract> {
    file_symbol_keys(module, file)
        .into_iter()
        .zip(file.items.iter())
        .map(|(key, item)| DeclarationContract {
            doc: item_doc(item),
            signature_source: signature_source(text, item),
            use_path: item_use_path(item),
            key,
        })
        .collect()
}

pub fn hardware_contract_for(module: &[String], file: &AstFile) -> Vec<HardwareContract> {
    file_symbol_keys(module, file)
        .into_iter()
        .zip(file.items.iter())
        .map(|(key, item)| HardwareContract {
            has_storage: item_has_storage(item),
            key,
        })
        .collect()
}

/// AST a cross-module consumer lowers against.
///
/// Function, cell, and map bodies are emptied and the item span stops at the
/// body. Signature token spans are kept, so a body-only edit compares equal.
pub fn signature_stub_file(file: &AstFile) -> AstFile {
    AstFile::with_source_doc(
        file.source_id,
        file.doc.clone(),
        file.items.iter().cloned().map(stub_item).collect(),
    )
}

pub fn item_has_storage(item: &Item) -> bool {
    match item {
        Item::Fn(item) => block_has_storage(&item.body),
        Item::Cell(item) => block_has_storage(&item.body),
        Item::Map(_)
        | Item::Const(_)
        | Item::Enum(_)
        | Item::Struct(_)
        | Item::Bundle(_)
        | Item::Interface(_)
        | Item::ExternCell(_)
        | Item::Use(_)
        | Item::Error(_) => false,
        _ => false,
    }
}

fn stub_item(item: Item) -> Item {
    match item {
        Item::Fn(mut item) => {
            let start = item.body.span.start;
            item.span.end = start;
            item.body = empty_block(item.body.span);
            Item::Fn(item)
        }
        Item::Cell(mut item) => {
            let start = item.body.span.start;
            item.span.end = start;
            item.body = empty_block(item.body.span);
            Item::Cell(item)
        }
        Item::Map(mut item) => {
            let body_span = item.body.span();
            item.span.end = body_span.start;
            item.body = Expr::Int(
                0,
                Span::new_in(body_span.source, body_span.start, body_span.start),
            );
            Item::Map(item)
        }
        other => other,
    }
}

fn empty_block(body: Span) -> Block {
    Block::new(
        Vec::new(),
        None,
        Span::new_in(body.source, body.start, body.start),
    )
}

fn signature_source(text: &str, item: &Item) -> String {
    let item_span = item.span();
    let Some(whole) = text.get(item_span.start..item_span.end) else {
        return String::new();
    };
    let body = match item {
        Item::Fn(item) => Some(item.body.span),
        Item::Cell(item) => Some(item.body.span),
        Item::Map(item) => Some(item.body.span()),
        _ => None,
    };
    let Some(body) = body else {
        return whole.to_string();
    };
    if body.start < item_span.start || body.end > item_span.end {
        return whole.to_string();
    }
    let mut signature = String::new();
    signature.push_str(&text[item_span.start..body.start]);
    signature.push_str(match item {
        Item::Map(_) => "0",
        _ => "{ }",
    });
    signature.push_str(&text[body.end..item_span.end]);
    signature
}

fn block_has_storage(block: &Block) -> bool {
    block.stmts.iter().any(stmt_has_storage)
}

fn stmt_has_storage(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Reg { .. } => true,
        Stmt::While { body, .. } | Stmt::ElabFor { body, .. } => block_has_storage(body),
        Stmt::ElabIf {
            then_block,
            else_block,
            ..
        } => block_has_storage(then_block) || else_block.as_ref().is_some_and(block_has_storage),
        Stmt::Error { .. }
        | Stmt::Const { .. }
        | Stmt::Let { .. }
        | Stmt::Var { .. }
        | Stmt::Signal { .. }
        | Stmt::Assign { .. }
        | Stmt::Drive { .. }
        | Stmt::Next { .. }
        | Stmt::Expr(_)
        | Stmt::Return(_, _) => false,
        _ => false,
    }
}

fn item_kind(item: &Item) -> SymbolKind {
    match item {
        Item::Error(_) => SymbolKind::Error,
        Item::Use(_) => SymbolKind::Use,
        Item::Const(_) => SymbolKind::Const,
        Item::Fn(_) => SymbolKind::Fn,
        Item::Enum(_) => SymbolKind::Enum,
        Item::Struct(_) => SymbolKind::Struct,
        Item::Bundle(_) => SymbolKind::Bundle,
        Item::Interface(_) => SymbolKind::Interface,
        Item::Map(_) => SymbolKind::Map,
        Item::Cell(_) => SymbolKind::Cell,
        Item::ExternCell(_) => SymbolKind::ExternCell,
        _ => SymbolKind::Error,
    }
}

fn item_name(item: &Item) -> String {
    match item {
        Item::Error(_) => String::new(),
        Item::Use(item) => item.path.last().cloned().unwrap_or_default(),
        Item::Const(item) => item.name.clone(),
        Item::Fn(item) => item.name.clone(),
        Item::Enum(item) => item.name.clone(),
        Item::Struct(item) => item.name.clone(),
        Item::Bundle(item) => item.name.clone(),
        Item::Interface(item) => item.name.clone(),
        Item::Map(item) => item.name.clone(),
        Item::Cell(item) => item.name.clone(),
        Item::ExternCell(item) => item.name.clone(),
        _ => String::new(),
    }
}

fn item_doc(item: &Item) -> Option<String> {
    match item {
        Item::Use(item) => item.doc.clone(),
        Item::Const(item) => item.doc.clone(),
        Item::Fn(item) => item.doc.clone(),
        Item::Enum(item) => item.doc.clone(),
        Item::Struct(item) => item.doc.clone(),
        Item::Bundle(item) => item.doc.clone(),
        Item::Interface(item) => item.doc.clone(),
        Item::Map(item) => item.doc.clone(),
        Item::Cell(item) => item.doc.clone(),
        Item::ExternCell(item) => item.doc.clone(),
        Item::Error(_) => None,
        _ => None,
    }
}

fn item_use_path(item: &Item) -> Option<Vec<String>> {
    match item {
        Item::Use(item) => Some(item.path.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
