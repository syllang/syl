mod model;

pub use model::{AstNodeId, AstNodeIndex, AstNodeKind, AstNodeRecord};

use crate::{
    AstFile, Attribute, Block, BundleItem, CallableItem, EnumItem, ErrorItem, Expr,
    ExternModuleItem, FieldDecl, FnItem, GenericParam, InstArg, InterfaceItem, Item, MapItem,
    MatchArm, NamedExpr, Param, Pattern, PortDecl, RegReset, ResultBinding, SelectArm, Stmt,
    TypeExpr, UseItem, ViewDecl, ViewField,
};
use std::collections::BTreeMap;
use syl_span::{SourceFile, SourcePosition, SourceRange, Span};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct NodeSeed {
    kind: AstNodeKind,
    text: Box<str>,
}

struct AstNodeIndexBuilder<'a> {
    file: &'a AstFile,
    source: &'a str,
    source_file: SourceFile,
    occurrences: BTreeMap<NodeSeed, usize>,
    nodes: Vec<AstNodeRecord>,
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
            occurrences: BTreeMap::new(),
            nodes: Vec::new(),
        }
    }

    fn build(mut self) -> AstNodeIndex {
        let root_span = Span::new_in(self.source_file.id(), 0, self.source.len());
        let root_id = self.push(AstNodeKind::File, root_span, None);
        for item in &self.file.items {
            self.visit_item(item, root_id);
        }
        AstNodeIndex::from_parts(root_id, self.nodes)
    }

    fn push(&mut self, kind: AstNodeKind, span: Span, parent: Option<AstNodeId>) -> AstNodeId {
        let text = self.source_text(span);
        let occurrence = self.bump_occurrence(kind, text.as_ref());
        let id = stable_node_id(kind, text.as_ref(), occurrence);
        let range = self
            .source_file
            .utf16_range(span)
            .unwrap_or_else(zero_range);
        self.nodes
            .push(AstNodeRecord::new(id, kind, span, range, parent));
        id
    }

    fn visit_item(&mut self, item: &Item, parent: AstNodeId) {
        match item {
            Item::Error(item) => {
                self.visit_error_item(item, parent);
            }
            Item::Package(item) => {
                self.push(AstNodeKind::PackageItem, item.span, Some(parent));
            }
            Item::Use(item) => {
                self.visit_use_item(item, parent);
            }
            Item::Const(item) => {
                let id = self.push(AstNodeKind::ConstItem, item.span, Some(parent));
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

    fn visit_error_item(&mut self, item: &ErrorItem, parent: AstNodeId) {
        self.push(AstNodeKind::ErrorItem, item.span, Some(parent));
    }

    fn visit_use_item(&mut self, item: &UseItem, parent: AstNodeId) {
        self.push(AstNodeKind::UseItem, item.span, Some(parent));
    }

    fn visit_fn_item(&mut self, item: &FnItem, parent: AstNodeId) {
        let id = self.push(AstNodeKind::FnItem, item.span, Some(parent));
        for param in &item.params {
            self.visit_param(param, id);
        }
        if let Some(ty) = &item.ret_ty {
            self.visit_type_expr(ty, id);
        }
        self.visit_block(&item.body, id);
    }

    fn visit_enum_item(&mut self, item: &EnumItem, parent: AstNodeId) {
        let id = self.push(AstNodeKind::EnumItem, item.span, Some(parent));
        for variant in &item.variants {
            self.push(AstNodeKind::EnumVariant, variant.span, Some(id));
        }
    }

    fn visit_bundle_item(&mut self, item: &BundleItem, parent: AstNodeId) {
        let id = self.push(AstNodeKind::BundleItem, item.span, Some(parent));
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

    fn visit_interface_item(&mut self, item: &InterfaceItem, parent: AstNodeId) {
        let id = self.push(AstNodeKind::InterfaceItem, item.span, Some(parent));
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

    fn visit_map_item(&mut self, item: &MapItem, parent: AstNodeId) {
        let id = self.push(AstNodeKind::MapItem, item.span, Some(parent));
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

    fn visit_callable_item(&mut self, item: &CallableItem, parent: AstNodeId, kind: AstNodeKind) {
        let id = self.push(kind, item.span, Some(parent));
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

    fn visit_extern_module_item(&mut self, item: &ExternModuleItem, parent: AstNodeId) {
        let id = self.push(AstNodeKind::ExternModuleItem, item.span, Some(parent));
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

    fn visit_result_binding(&mut self, item: &ResultBinding, parent: AstNodeId) {
        let id = self.push(AstNodeKind::ResultBinding, item.span, Some(parent));
        self.visit_type_expr(&item.ty, id);
    }

    fn visit_port_decl(&mut self, item: &PortDecl, parent: AstNodeId) {
        let id = self.push(AstNodeKind::PortDecl, item.span, Some(parent));
        self.visit_type_expr(&item.ty, id);
    }

    fn visit_param(&mut self, item: &Param, parent: AstNodeId) {
        let id = self.push(AstNodeKind::Param, item.span, Some(parent));
        self.visit_type_expr(&item.ty, id);
    }

    fn visit_generic_param(&mut self, item: &GenericParam, parent: AstNodeId) {
        let id = self.push(AstNodeKind::GenericParam, item.span, Some(parent));
        if let Some(kind) = &item.kind {
            self.visit_type_expr(kind, id);
        }
        if let Some(default) = &item.default {
            self.visit_expr(default, id);
        }
    }

    fn visit_field_decl(&mut self, item: &FieldDecl, parent: AstNodeId) {
        let id = self.push(AstNodeKind::FieldDecl, item.span, Some(parent));
        self.visit_type_expr(&item.ty, id);
    }

    fn visit_attribute(&mut self, item: &Attribute, parent: AstNodeId) {
        let id = self.push(AstNodeKind::Attribute, item.span, Some(parent));
        for arg in &item.args {
            self.visit_expr(arg, id);
        }
    }

    fn visit_view_decl(&mut self, item: &ViewDecl, parent: AstNodeId) {
        let id = self.push(AstNodeKind::ViewDecl, item.span, Some(parent));
        for field in &item.fields {
            self.visit_view_field(field, id);
        }
    }

    fn visit_view_field(&mut self, item: &ViewField, parent: AstNodeId) {
        self.push(AstNodeKind::ViewField, item.span, Some(parent));
    }

    fn visit_block(&mut self, block: &Block, parent: AstNodeId) {
        let id = self.push(AstNodeKind::Block, block.span, Some(parent));
        for stmt in &block.stmts {
            self.visit_stmt(stmt, id);
        }
        if let Some(tail) = block.tail.as_deref() {
            self.visit_expr(tail, id);
        }
    }

    fn visit_stmt(&mut self, stmt: &Stmt, parent: AstNodeId) {
        match stmt {
            Stmt::Error { span } => {
                self.push(AstNodeKind::ErrorStmt, *span, Some(parent));
            }
            Stmt::Const {
                ty, value, span, ..
            } => {
                let id = self.push(AstNodeKind::ConstStmt, *span, Some(parent));
                if let Some(ty) = ty {
                    self.visit_type_expr(ty, id);
                }
                self.visit_expr(value, id);
            }
            Stmt::Let {
                ty, value, span, ..
            } => {
                let id = self.push(AstNodeKind::LetStmt, *span, Some(parent));
                if let Some(ty) = ty {
                    self.visit_type_expr(ty, id);
                }
                if let Some(value) = value {
                    self.visit_expr(value, id);
                }
            }
            Stmt::Alias { value, span, .. } => {
                let id = self.push(AstNodeKind::AliasStmt, *span, Some(parent));
                self.visit_expr(value, id);
            }
            Stmt::Var {
                ty, value, span, ..
            } => {
                let id = self.push(AstNodeKind::VarStmt, *span, Some(parent));
                if let Some(ty) = ty {
                    self.visit_type_expr(ty, id);
                }
                if let Some(value) = value {
                    self.visit_expr(value, id);
                }
            }
            Stmt::Signal {
                ty, value, span, ..
            } => {
                let id = self.push(AstNodeKind::SignalStmt, *span, Some(parent));
                if let Some(ty) = ty {
                    self.visit_type_expr(ty, id);
                }
                if let Some(value) = value {
                    self.visit_expr(value, id);
                }
            }
            Stmt::Reg {
                ty, reset, span, ..
            } => {
                let id = self.push(AstNodeKind::RegStmt, *span, Some(parent));
                if let Some(ty) = ty {
                    self.visit_type_expr(ty, id);
                }
                if let Some(reset) = reset {
                    self.visit_reg_reset(reset, id);
                }
            }
            Stmt::Next { value, span, .. } => {
                let id = self.push(AstNodeKind::NextStmt, *span, Some(parent));
                self.visit_expr(value, id);
            }
            Stmt::Inst {
                name, callee, span, ..
            } => {
                let id = self.push(AstNodeKind::InstStmt, *span, Some(parent));
                self.visit_expr(name, id);
                self.visit_expr(callee, id);
            }
            Stmt::While { cond, body, span } => {
                let id = self.push(AstNodeKind::WhileStmt, *span, Some(parent));
                self.visit_expr(cond, id);
                self.visit_block(body, id);
            }
            Stmt::ElabIf {
                cond,
                then_block,
                else_block,
                span,
            } => {
                let id = self.push(AstNodeKind::ElabIfStmt, *span, Some(parent));
                self.visit_expr(cond, id);
                self.visit_block(then_block, id);
                if let Some(block) = else_block {
                    self.visit_block(block, id);
                }
            }
            Stmt::ElabFor {
                range, body, span, ..
            } => {
                let id = self.push(AstNodeKind::ElabForStmt, *span, Some(parent));
                self.visit_expr(range, id);
                self.visit_block(body, id);
            }
            Stmt::Expr(expr) => {
                let id = self.push(AstNodeKind::ExprStmt, expr.span(), Some(parent));
                self.visit_expr(expr, id);
            }
            Stmt::Return(expr, span) => {
                let id = self.push(AstNodeKind::ReturnStmt, *span, Some(parent));
                if let Some(expr) = expr {
                    self.visit_expr(expr, id);
                }
            }
        }
    }

    fn visit_reg_reset(&mut self, item: &RegReset, parent: AstNodeId) {
        let id = self.push(AstNodeKind::RegReset, item.span, Some(parent));
        if let Some(domain) = &item.domain {
            self.visit_expr(domain, id);
        }
        self.visit_expr(&item.value, id);
    }

    fn visit_expr(&mut self, expr: &Expr, parent: AstNodeId) {
        match expr {
            Expr::Ident(_, span) => {
                self.push(AstNodeKind::IdentExpr, *span, Some(parent));
            }
            Expr::Int(_, span) => {
                self.push(AstNodeKind::IntExpr, *span, Some(parent));
            }
            Expr::Str(_, span) => {
                self.push(AstNodeKind::StrExpr, *span, Some(parent));
            }
            Expr::Bool(_, span) => {
                self.push(AstNodeKind::BoolExpr, *span, Some(parent));
            }
            Expr::Unary { expr, span, .. } => {
                let id = self.push(AstNodeKind::UnaryExpr, *span, Some(parent));
                self.visit_expr(expr, id);
            }
            Expr::Binary {
                left, right, span, ..
            } => {
                let id = self.push(AstNodeKind::BinaryExpr, *span, Some(parent));
                self.visit_expr(left, id);
                self.visit_expr(right, id);
            }
            Expr::Call { callee, args, span } => {
                let id = self.push(AstNodeKind::CallExpr, *span, Some(parent));
                self.visit_expr(callee, id);
                for arg in args {
                    self.visit_inst_arg(arg, id);
                }
            }
            Expr::GenericApp { callee, args, span } => {
                let id = self.push(AstNodeKind::GenericAppExpr, *span, Some(parent));
                self.visit_expr(callee, id);
                for arg in args {
                    self.visit_type_expr(arg, id);
                }
            }
            Expr::Aggregate { ty, fields, span } => {
                let id = self.push(AstNodeKind::AggregateExpr, *span, Some(parent));
                self.visit_type_expr(ty, id);
                for field in fields {
                    self.visit_named_expr(field, id);
                }
            }
            Expr::Field { base, span, .. } => {
                let id = self.push(AstNodeKind::FieldExpr, *span, Some(parent));
                self.visit_expr(base, id);
            }
            Expr::Index { base, index, span } => {
                let id = self.push(AstNodeKind::IndexExpr, *span, Some(parent));
                self.visit_expr(base, id);
                self.visit_expr(index, id);
            }
            Expr::Group(expr, span) => {
                let id = self.push(AstNodeKind::GroupExpr, *span, Some(parent));
                self.visit_expr(expr, id);
            }
            Expr::Block(block) => {
                let id = self.push(AstNodeKind::BlockExpr, block.span, Some(parent));
                self.visit_block(block, id);
            }
            Expr::Match { expr, arms, span } => {
                let id = self.push(AstNodeKind::MatchExpr, *span, Some(parent));
                self.visit_expr(expr, id);
                for arm in arms {
                    self.visit_match_arm(arm, id);
                }
            }
            Expr::Select { arms, span, .. } => {
                let id = self.push(AstNodeKind::SelectExpr, *span, Some(parent));
                for arm in arms {
                    self.visit_select_arm(arm, id);
                }
            }
            Expr::Inst { callee, args, span } => {
                let id = self.push(AstNodeKind::InstExpr, *span, Some(parent));
                self.visit_expr(callee, id);
                for arg in args {
                    self.visit_inst_arg(arg, id);
                }
            }
            Expr::CompileError { message, span } => {
                let id = self.push(AstNodeKind::CompileErrorExpr, *span, Some(parent));
                self.visit_expr(message, id);
            }
            Expr::Range { start, end, span } => {
                let id = self.push(AstNodeKind::RangeExpr, *span, Some(parent));
                self.visit_expr(start, id);
                self.visit_expr(end, id);
            }
        }
    }

    fn visit_named_expr(&mut self, item: &NamedExpr, parent: AstNodeId) {
        let id = self.push(AstNodeKind::NamedExpr, item.span, Some(parent));
        self.visit_expr(&item.value, id);
    }

    fn visit_inst_arg(&mut self, item: &InstArg, parent: AstNodeId) {
        let id = self.push(AstNodeKind::InstArg, item.span, Some(parent));
        self.visit_expr(&item.value, id);
    }

    fn visit_select_arm(&mut self, item: &SelectArm, parent: AstNodeId) {
        let id = self.push(AstNodeKind::SelectArm, item.span, Some(parent));
        self.visit_expr(&item.pattern, id);
        self.visit_expr(&item.value, id);
    }

    fn visit_match_arm(&mut self, item: &MatchArm, parent: AstNodeId) {
        let id = self.push(AstNodeKind::MatchArm, item.span, Some(parent));
        self.visit_pattern(&item.pattern, id);
        self.visit_expr(&item.value, id);
    }

    fn visit_pattern(&mut self, pattern: &Pattern, parent: AstNodeId) {
        match pattern {
            Pattern::Wildcard(span) => {
                self.push(AstNodeKind::WildcardPattern, *span, Some(parent));
            }
            Pattern::Ident(_, span) => {
                self.push(AstNodeKind::IdentPattern, *span, Some(parent));
            }
            Pattern::Int(_, span) => {
                self.push(AstNodeKind::IntPattern, *span, Some(parent));
            }
            Pattern::Bool(_, span) => {
                self.push(AstNodeKind::BoolPattern, *span, Some(parent));
            }
            Pattern::Path(_, span) => {
                self.push(AstNodeKind::PathPattern, *span, Some(parent));
            }
        }
    }

    fn visit_type_expr(&mut self, ty: &TypeExpr, parent: AstNodeId) {
        match ty {
            TypeExpr::Path(_, span) => {
                self.push(AstNodeKind::PathType, *span, Some(parent));
            }
            TypeExpr::Array { len, elem, span } => {
                let id = self.push(AstNodeKind::ArrayType, *span, Some(parent));
                self.visit_expr(len, id);
                self.visit_type_expr(elem, id);
            }
            TypeExpr::Generic { base, args, span } => {
                let id = self.push(AstNodeKind::GenericType, *span, Some(parent));
                self.visit_type_expr(base, id);
                for arg in args {
                    self.visit_type_expr(arg, id);
                }
            }
            TypeExpr::ViewSelect { base, span, .. } => {
                let id = self.push(AstNodeKind::ViewSelectType, *span, Some(parent));
                self.visit_type_expr(base, id);
            }
        }
    }

    fn bump_occurrence(&mut self, kind: AstNodeKind, text: &str) -> usize {
        let entry = self
            .occurrences
            .entry(NodeSeed {
                kind,
                text: text.into(),
            })
            .or_insert(0);
        *entry = entry.saturating_add(1);
        *entry
    }

    fn source_text(&self, span: Span) -> Box<str> {
        self.source
            .get(span.start..span.end)
            .unwrap_or_default()
            .into()
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

fn stable_node_id(kind: AstNodeKind, text: &str, occurrence: usize) -> AstNodeId {
    const FNV_OFFSET: u64 = 14_695_981_039_346_656_037;
    const FNV_PRIME: u64 = 1_099_511_628_211;

    let mut hash = FNV_OFFSET;
    for byte in <&'static str>::from(kind).bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash ^= u64::from(b':');
    hash = hash.wrapping_mul(FNV_PRIME);
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash ^= u64::from(b'#');
    hash = hash.wrapping_mul(FNV_PRIME);
    let occurrence = u64::try_from(occurrence).unwrap_or(u64::MAX);
    for byte in occurrence.to_le_bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    AstNodeId::new(hash)
}

fn zero_range() -> SourceRange {
    SourceRange::new(SourcePosition::new(0, 0), SourcePosition::new(0, 0))
}
