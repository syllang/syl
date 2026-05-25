use crate::{DocumentSymbolKind, DocumentSymbolResult};
use syl_session::{AnalysisFile, AnalysisSnapshot};
use syl_span::Span;
use syl_syntax::{
    Block, BundleItem, CallableItem, EnumItem, ExternModuleItem, FieldDecl, FnItem, GenericParam,
    InterfaceItem, Item, MapItem, Param, PortDecl, ResultBinding, Stmt, ViewDecl,
};

#[non_exhaustive]
pub(super) struct DocumentSymbolCollector<'a> {
    snapshot: &'a AnalysisSnapshot,
    file: &'a AnalysisFile,
    selection: SymbolSelection<'a>,
}

impl<'a> DocumentSymbolCollector<'a> {
    pub(super) fn new(snapshot: &'a AnalysisSnapshot, file: &'a AnalysisFile) -> Self {
        Self {
            snapshot,
            file,
            selection: SymbolSelection::new(snapshot),
        }
    }

    pub(super) fn collect(&self) -> Vec<DocumentSymbolResult> {
        self.file
            .ast()
            .items
            .iter()
            .filter_map(|item| self.item_symbol(item))
            .collect()
    }

    fn item_symbol(&self, item: &Item) -> Option<DocumentSymbolResult> {
        match item {
            Item::Package(item) => self.symbol(
                item.path.join("."),
                DocumentSymbolKind::Package,
                item.span,
                Vec::new(),
            ),
            Item::Const(item) => self.symbol(
                item.name.clone(),
                DocumentSymbolKind::Constant,
                item.span,
                Vec::new(),
            ),
            Item::Fn(item) => self.fn_symbol(item),
            Item::Map(item) => self.map_symbol(item),
            Item::Enum(item) => self.enum_symbol(item),
            Item::Bundle(item) => self.bundle_symbol(item),
            Item::Interface(item) => self.interface_symbol(item),
            Item::Cell(item) | Item::Module(item) => self.callable_symbol(item),
            Item::ExternModule(item) => self.extern_module_symbol(item),
            _ => None,
        }
    }

    fn fn_symbol(&self, item: &FnItem) -> Option<DocumentSymbolResult> {
        let mut children = self.param_symbols(&item.params);
        children.extend(self.block_symbols(&item.body));
        self.symbol(
            item.name.clone(),
            DocumentSymbolKind::Function,
            item.span,
            children,
        )
    }

    fn map_symbol(&self, item: &MapItem) -> Option<DocumentSymbolResult> {
        let mut children = self.generic_symbols(&item.generics);
        children.extend(self.param_symbols(&item.params));
        self.symbol(
            item.name.clone(),
            DocumentSymbolKind::Function,
            item.span,
            children,
        )
    }

    fn enum_symbol(&self, item: &EnumItem) -> Option<DocumentSymbolResult> {
        let children = item
            .variants
            .iter()
            .filter_map(|variant| {
                self.symbol(
                    variant.name.clone(),
                    DocumentSymbolKind::EnumMember,
                    variant.span,
                    Vec::new(),
                )
            })
            .collect();
        self.symbol(
            item.name.clone(),
            DocumentSymbolKind::Type,
            item.span,
            children,
        )
    }

    fn bundle_symbol(&self, item: &BundleItem) -> Option<DocumentSymbolResult> {
        let mut children = self.generic_symbols(&item.generics);
        children.extend(self.field_symbols(&item.fields));
        self.symbol(
            item.name.clone(),
            DocumentSymbolKind::Type,
            item.span,
            children,
        )
    }

    fn interface_symbol(&self, item: &InterfaceItem) -> Option<DocumentSymbolResult> {
        let mut children = self.generic_symbols(&item.generics);
        children.extend(self.field_symbols(&item.fields));
        children.extend(item.views.iter().filter_map(|view| self.view_symbol(view)));
        self.symbol(
            item.name.clone(),
            DocumentSymbolKind::Type,
            item.span,
            children,
        )
    }

    fn callable_symbol(&self, item: &CallableItem) -> Option<DocumentSymbolResult> {
        let mut children = self.generic_symbols(&item.generics);
        children.extend(self.param_symbols(&item.params));
        children.extend(self.port_symbols(&item.ports));
        children.extend(
            item.result
                .iter()
                .filter_map(|result| self.result_symbol(result)),
        );
        children.extend(self.block_symbols(&item.body));
        self.symbol(
            item.name.clone(),
            DocumentSymbolKind::Module,
            item.span,
            children,
        )
    }

    fn extern_module_symbol(&self, item: &ExternModuleItem) -> Option<DocumentSymbolResult> {
        let mut children = self.generic_symbols(&item.generics);
        children.extend(self.param_symbols(&item.params));
        children.extend(self.port_symbols(&item.ports));
        children.extend(
            item.result
                .iter()
                .filter_map(|result| self.result_symbol(result)),
        );
        self.symbol(
            item.name.clone(),
            DocumentSymbolKind::Module,
            item.span,
            children,
        )
    }

    fn generic_symbols(&self, generics: &[GenericParam]) -> Vec<DocumentSymbolResult> {
        generics
            .iter()
            .filter_map(|generic| {
                self.symbol(
                    generic.name.clone(),
                    DocumentSymbolKind::Parameter,
                    generic.span,
                    Vec::new(),
                )
            })
            .collect()
    }

    fn param_symbols(&self, params: &[Param]) -> Vec<DocumentSymbolResult> {
        params
            .iter()
            .filter_map(|param| {
                self.symbol(
                    param.name.clone(),
                    DocumentSymbolKind::Parameter,
                    param.span,
                    Vec::new(),
                )
            })
            .collect()
    }

    fn port_symbols(&self, ports: &[PortDecl]) -> Vec<DocumentSymbolResult> {
        ports
            .iter()
            .filter_map(|port| {
                self.symbol(
                    port.name.clone(),
                    DocumentSymbolKind::Parameter,
                    port.span,
                    Vec::new(),
                )
            })
            .collect()
    }

    fn field_symbols(&self, fields: &[FieldDecl]) -> Vec<DocumentSymbolResult> {
        fields
            .iter()
            .filter_map(|field| {
                self.symbol(
                    field.name.clone(),
                    DocumentSymbolKind::Field,
                    field.span,
                    Vec::new(),
                )
            })
            .collect()
    }

    fn view_symbol(&self, view: &ViewDecl) -> Option<DocumentSymbolResult> {
        let children = view
            .fields
            .iter()
            .filter_map(|field| {
                self.symbol(
                    field.name.clone(),
                    DocumentSymbolKind::Field,
                    field.span,
                    Vec::new(),
                )
            })
            .collect();
        self.symbol(
            view.name.clone(),
            DocumentSymbolKind::View,
            view.span,
            children,
        )
    }

    fn result_symbol(&self, result: &ResultBinding) -> Option<DocumentSymbolResult> {
        self.symbol(
            result.name.clone(),
            DocumentSymbolKind::Field,
            result.span,
            Vec::new(),
        )
    }

    fn block_symbols(&self, block: &Block) -> Vec<DocumentSymbolResult> {
        let mut symbols = Vec::new();
        for stmt in &block.stmts {
            symbols.extend(self.stmt_symbols(stmt));
        }
        symbols
    }

    fn stmt_symbols(&self, stmt: &Stmt) -> Vec<DocumentSymbolResult> {
        match stmt {
            Stmt::Const { name, span, .. } => self.leaf(name, DocumentSymbolKind::Constant, *span),
            Stmt::Let { name, span, .. }
            | Stmt::Var { name, span, .. }
            | Stmt::Signal { name, span, .. }
            | Stmt::Reg { name, span, .. } => self.leaf(name, DocumentSymbolKind::Variable, *span),
            Stmt::ElabFor {
                name, body, span, ..
            } => self.loop_symbol(name, body, *span),
            Stmt::ElabIf {
                then_block,
                else_block,
                ..
            } => {
                let mut symbols = self.block_symbols(then_block);
                if let Some(else_block) = else_block {
                    symbols.extend(self.block_symbols(else_block));
                }
                symbols
            }
            Stmt::While { body, .. } => self.block_symbols(body),
            Stmt::Error { .. } | Stmt::Next { .. } | Stmt::Expr(_) | Stmt::Return(_, _) => {
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn leaf(&self, name: &str, kind: DocumentSymbolKind, span: Span) -> Vec<DocumentSymbolResult> {
        self.symbol(name.to_string(), kind, span, Vec::new())
            .into_iter()
            .collect()
    }

    fn loop_symbol(&self, name: &str, body: &Block, span: Span) -> Vec<DocumentSymbolResult> {
        let children = self.block_symbols(body);
        self.symbol(
            name.to_string(),
            DocumentSymbolKind::Variable,
            span,
            children,
        )
        .into_iter()
        .collect()
    }

    fn symbol(
        &self,
        name: String,
        kind: DocumentSymbolKind,
        span: Span,
        children: Vec<DocumentSymbolResult>,
    ) -> Option<DocumentSymbolResult> {
        let range = self.snapshot.source_map().utf16_range(span)?;
        let selection_span = self.selection.span_for(&name, span);
        let selection_range = self
            .snapshot
            .source_map()
            .utf16_range(selection_span)
            .unwrap_or(range);
        Some(DocumentSymbolResult {
            name,
            kind,
            range,
            selection_range,
            children,
        })
    }
}

#[non_exhaustive]
struct SymbolSelection<'a> {
    snapshot: &'a AnalysisSnapshot,
}

impl<'a> SymbolSelection<'a> {
    fn new(snapshot: &'a AnalysisSnapshot) -> Self {
        Self { snapshot }
    }

    fn span_for(&self, name: &str, fallback: Span) -> Span {
        let Some(source) = self.snapshot.source_map().file(fallback.source) else {
            return fallback;
        };
        let Some(segment) = source.text().get(fallback.start..fallback.end) else {
            return fallback;
        };
        let Some(relative_start) = segment.find(name) else {
            return fallback;
        };
        let Some(start) = fallback.start.checked_add(relative_start) else {
            return fallback;
        };
        let Some(end) = start.checked_add(name.len()) else {
            return fallback;
        };
        Span::new_in(fallback.source, start, end)
    }
}
