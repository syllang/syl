mod anchor;
mod model;
mod visit;

pub use model::{AstNodeId, AstNodeIndex, AstNodeKind, AstNodeRecord};

use crate::{
    AstFile, Attribute, BinaryOp, BundleItem, CallableItem, EnumItem, ErrorItem, ExternModuleItem,
    FieldDecl, FnItem, GenericParam, InterfaceItem, Item, MapItem, Param, ParamDirection, PortDecl,
    ResultBinding, SelectMode, UnaryOp, UseItem, ViewDecl, ViewDirection, ViewField,
};
use anchor::{
    PendingNode, finalize_nodes, local_seed_bool, local_seed_int, local_seed_kind, local_seed_name,
    local_seed_named_tag, local_seed_path, local_seed_tag, local_seed_text,
};
use syl_span::{SourceFile, SourcePosition, SourceRange, Span};

type NodeHandle = usize;

struct NamedTagSeed<'a> {
    tag: &'a str,
    name: &'a str,
}

struct AstNodeIndexBuilder<'a> {
    file: &'a AstFile,
    source: &'a str,
    source_file: SourceFile,
    nodes: Vec<PendingNode>,
}

impl<'a> AstNodeIndexBuilder<'a> {
    fn new(file: &'a AstFile, source: &'a str) -> Self {
        let source_id = file
            .items
            .first()
            .map(|item| item.span().source)
            .unwrap_or_default();
        let source_file = SourceFile::new(source_id, "<syntax>", source);
        Self {
            file,
            source,
            source_file,
            nodes: Vec::new(),
        }
    }

    fn build(mut self) -> AstNodeIndex {
        let root_span = Span::new_in(self.source_file.id(), 0, self.source.len());
        let root = self.push_kind(AstNodeKind::File, root_span, None);
        for item in &self.file.items {
            self.visit_item(item, root);
        }
        let (root_id, records) = finalize_nodes(root, self.nodes);
        AstNodeIndex::from_parts(root_id, records)
    }

    fn push_seed(
        &mut self,
        kind: AstNodeKind,
        span: Span,
        parent: Option<NodeHandle>,
        local_seed: u64,
    ) -> NodeHandle {
        let range = self
            .source_file
            .utf16_range(span)
            .unwrap_or_else(zero_range);
        self.nodes
            .push(PendingNode::new(kind, span, range, parent, local_seed));
        self.nodes.len().saturating_sub(1)
    }

    fn push_kind(
        &mut self,
        kind: AstNodeKind,
        span: Span,
        parent: Option<NodeHandle>,
    ) -> NodeHandle {
        self.push_seed(kind, span, parent, local_seed_kind())
    }

    fn push_name(
        &mut self,
        kind: AstNodeKind,
        span: Span,
        parent: Option<NodeHandle>,
        name: &str,
    ) -> NodeHandle {
        self.push_seed(kind, span, parent, local_seed_name(name))
    }

    fn push_named_tag(
        &mut self,
        kind: AstNodeKind,
        span: Span,
        parent: Option<NodeHandle>,
        named_tag: NamedTagSeed<'_>,
    ) -> NodeHandle {
        self.push_seed(
            kind,
            span,
            parent,
            local_seed_named_tag(named_tag.tag, named_tag.name),
        )
    }

    fn push_path(
        &mut self,
        kind: AstNodeKind,
        span: Span,
        parent: Option<NodeHandle>,
        path: &[String],
    ) -> NodeHandle {
        self.push_seed(kind, span, parent, local_seed_path(path))
    }

    fn push_tag(
        &mut self,
        kind: AstNodeKind,
        span: Span,
        parent: Option<NodeHandle>,
        tag: &str,
    ) -> NodeHandle {
        self.push_seed(kind, span, parent, local_seed_tag(tag))
    }

    fn push_int(
        &mut self,
        kind: AstNodeKind,
        span: Span,
        parent: Option<NodeHandle>,
        value: u64,
    ) -> NodeHandle {
        self.push_seed(kind, span, parent, local_seed_int(value))
    }

    fn push_bool(
        &mut self,
        kind: AstNodeKind,
        span: Span,
        parent: Option<NodeHandle>,
        value: bool,
    ) -> NodeHandle {
        self.push_seed(kind, span, parent, local_seed_bool(value))
    }

    fn push_text(
        &mut self,
        kind: AstNodeKind,
        span: Span,
        parent: Option<NodeHandle>,
    ) -> NodeHandle {
        let local_seed = {
            let text = self.source_text(span);
            local_seed_text(text)
        };
        self.push_seed(kind, span, parent, local_seed)
    }

    fn visit_item(&mut self, item: &Item, parent: NodeHandle) {
        match item {
            Item::Error(item) => {
                self.visit_error_item(item, parent);
            }
            Item::Package(item) => {
                self.push_path(
                    AstNodeKind::PackageItem,
                    item.span,
                    Some(parent),
                    &item.path,
                );
            }
            Item::Use(item) => {
                self.visit_use_item(item, parent);
            }
            Item::Const(item) => {
                let id =
                    self.push_name(AstNodeKind::ConstItem, item.span, Some(parent), &item.name);
                if let Some(ty) = &item.ty {
                    self.visit_type_expr(ty, id);
                }
                self.visit_expr(&item.value, id);
            }
            Item::Fn(item) => {
                self.visit_fn_item(item, parent);
            }
            Item::Enum(item) => {
                self.visit_enum_item(item, parent);
            }
            Item::Bundle(item) => {
                self.visit_bundle_item(item, parent);
            }
            Item::Interface(item) => {
                self.visit_interface_item(item, parent);
            }
            Item::Map(item) => {
                self.visit_map_item(item, parent);
            }
            Item::Cell(item) => {
                self.visit_callable_item(item, parent, AstNodeKind::CellItem);
            }
            Item::Module(item) => {
                self.visit_callable_item(item, parent, AstNodeKind::ModuleItem);
            }
            Item::ExternModule(item) => {
                self.visit_extern_module_item(item, parent);
            }
        }
    }

    fn visit_error_item(&mut self, item: &ErrorItem, parent: NodeHandle) {
        self.push_kind(AstNodeKind::ErrorItem, item.span, Some(parent));
    }

    fn visit_use_item(&mut self, item: &UseItem, parent: NodeHandle) {
        self.push_path(AstNodeKind::UseItem, item.span, Some(parent), &item.path);
    }

    fn visit_fn_item(&mut self, item: &FnItem, parent: NodeHandle) {
        let id = self.push_name(AstNodeKind::FnItem, item.span, Some(parent), &item.name);
        for param in &item.params {
            self.visit_param(param, id);
        }
        if let Some(ty) = &item.ret_ty {
            self.visit_type_expr(ty, id);
        }
        self.visit_block(&item.body, id);
    }

    fn visit_enum_item(&mut self, item: &EnumItem, parent: NodeHandle) {
        let id = self.push_name(AstNodeKind::EnumItem, item.span, Some(parent), &item.name);
        for variant in &item.variants {
            self.push_name(
                AstNodeKind::EnumVariant,
                variant.span,
                Some(id),
                &variant.name,
            );
        }
    }

    fn visit_bundle_item(&mut self, item: &BundleItem, parent: NodeHandle) {
        let id = self.push_name(AstNodeKind::BundleItem, item.span, Some(parent), &item.name);
        for generic in &item.generics {
            self.visit_generic_param(generic, id);
        }
        for field in &item.fields {
            self.visit_field_decl(field, id);
        }
        for attr in &item.attrs {
            self.visit_attribute(attr, id);
        }
    }

    fn visit_interface_item(&mut self, item: &InterfaceItem, parent: NodeHandle) {
        let id = self.push_name(
            AstNodeKind::InterfaceItem,
            item.span,
            Some(parent),
            &item.name,
        );
        for generic in &item.generics {
            self.visit_generic_param(generic, id);
        }
        for field in &item.fields {
            self.visit_field_decl(field, id);
        }
        for view in &item.views {
            self.visit_view_decl(view, id);
        }
    }

    fn visit_map_item(&mut self, item: &MapItem, parent: NodeHandle) {
        let id = self.push_name(AstNodeKind::MapItem, item.span, Some(parent), &item.name);
        for generic in &item.generics {
            self.visit_generic_param(generic, id);
        }
        for param in &item.params {
            self.visit_param(param, id);
        }
        if let Some(ty) = &item.ret_ty {
            self.visit_type_expr(ty, id);
        }
        self.visit_expr(&item.body, id);
    }

    fn visit_callable_item(&mut self, item: &CallableItem, parent: NodeHandle, kind: AstNodeKind) {
        let id = self.push_name(kind, item.span, Some(parent), &item.name);
        for generic in &item.generics {
            self.visit_generic_param(generic, id);
        }
        for param in &item.params {
            self.visit_param(param, id);
        }
        for port in &item.ports {
            self.visit_port_decl(port, id);
        }
        if let Some(result) = &item.result {
            self.visit_result_binding(result, id);
        }
        self.visit_block(&item.body, id);
    }

    fn visit_extern_module_item(&mut self, item: &ExternModuleItem, parent: NodeHandle) {
        let id = self.push_name(
            AstNodeKind::ExternModuleItem,
            item.span,
            Some(parent),
            &item.name,
        );
        for generic in &item.generics {
            self.visit_generic_param(generic, id);
        }
        for param in &item.params {
            self.visit_param(param, id);
        }
        for port in &item.ports {
            self.visit_port_decl(port, id);
        }
        if let Some(result) = &item.result {
            self.visit_result_binding(result, id);
        }
    }

    fn visit_result_binding(&mut self, item: &ResultBinding, parent: NodeHandle) {
        let id = self.push_name(
            AstNodeKind::ResultBinding,
            item.span,
            Some(parent),
            &item.name,
        );
        self.visit_type_expr(&item.ty, id);
    }

    fn visit_port_decl(&mut self, item: &PortDecl, parent: NodeHandle) {
        let id = self.push_named_tag(
            AstNodeKind::PortDecl,
            item.span,
            Some(parent),
            NamedTagSeed {
                tag: param_direction_label(Some(&item.dir)),
                name: &item.name,
            },
        );
        self.visit_type_expr(&item.ty, id);
    }

    fn visit_param(&mut self, item: &Param, parent: NodeHandle) {
        let id = self.push_named_tag(
            AstNodeKind::Param,
            item.span,
            Some(parent),
            NamedTagSeed {
                tag: param_direction_label(item.dir.as_ref()),
                name: &item.name,
            },
        );
        self.visit_type_expr(&item.ty, id);
    }

    fn visit_generic_param(&mut self, item: &GenericParam, parent: NodeHandle) {
        let id = self.push_name(
            AstNodeKind::GenericParam,
            item.span,
            Some(parent),
            &item.name,
        );
        if let Some(kind) = &item.kind {
            self.visit_type_expr(kind, id);
        }
        if let Some(default) = &item.default {
            self.visit_expr(default, id);
        }
    }

    fn visit_field_decl(&mut self, item: &FieldDecl, parent: NodeHandle) {
        let id = self.push_name(AstNodeKind::FieldDecl, item.span, Some(parent), &item.name);
        self.visit_type_expr(&item.ty, id);
    }

    fn visit_attribute(&mut self, item: &Attribute, parent: NodeHandle) {
        let id = self.push_name(AstNodeKind::Attribute, item.span, Some(parent), &item.name);
        for arg in &item.args {
            self.visit_expr(arg, id);
        }
    }

    fn visit_view_decl(&mut self, item: &ViewDecl, parent: NodeHandle) {
        let id = self.push_name(AstNodeKind::ViewDecl, item.span, Some(parent), &item.name);
        for field in &item.fields {
            self.visit_view_field(field, id);
        }
    }

    fn visit_view_field(&mut self, item: &ViewField, parent: NodeHandle) {
        self.push_named_tag(
            AstNodeKind::ViewField,
            item.span,
            Some(parent),
            NamedTagSeed {
                tag: view_direction_label(&item.dir),
                name: &item.name,
            },
        );
    }

    fn source_text(&self, span: Span) -> &str {
        self.source.get(span.start..span.end).unwrap_or_default()
    }
}

impl AstNodeIndex {
    fn build(file: &AstFile, source: &str) -> Self {
        AstNodeIndexBuilder::new(file, source).build()
    }
}

impl AstFile {
    pub fn build_node_index(&self, source: &str) -> AstNodeIndex {
        AstNodeIndex::build(self, source)
    }
}

fn binary_op_label(op: BinaryOp) -> &'static str {
    <&'static str>::from(op)
}

fn unary_op_label(op: UnaryOp) -> &'static str {
    <&'static str>::from(op)
}

fn select_mode_label(mode: &SelectMode) -> &'static str {
    match mode {
        SelectMode::Priority => "priority",
        SelectMode::Unique => "unique",
    }
}

fn param_direction_label(direction: Option<&ParamDirection>) -> &'static str {
    match direction {
        Some(ParamDirection::In) => "in",
        Some(ParamDirection::InOut) => "inout",
        Some(ParamDirection::Out) => "out",
        None => "value",
    }
}

fn view_direction_label(direction: &ViewDirection) -> &'static str {
    match direction {
        ViewDirection::In => "in",
        ViewDirection::InOut => "inout",
        ViewDirection::Out => "out",
    }
}

fn zero_range() -> SourceRange {
    SourceRange::new(SourcePosition::new(0, 0), SourcePosition::new(0, 0))
}
