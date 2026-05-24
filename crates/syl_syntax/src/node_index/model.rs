use syl_span::{SourceRange, Span};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct AstNodeId(u64);

impl AstNodeId {
    pub(super) fn new(raw: u64) -> Self {
        Self(raw.max(1))
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum AstNodeKind {
    File,
    ErrorItem,
    PackageItem,
    UseItem,
    ConstItem,
    FnItem,
    EnumItem,
    EnumVariant,
    BundleItem,
    InterfaceItem,
    MapItem,
    CellItem,
    ModuleItem,
    ExternModuleItem,
    ResultBinding,
    PortDecl,
    Param,
    GenericParam,
    FieldDecl,
    Attribute,
    ViewDecl,
    ViewField,
    Block,
    ErrorStmt,
    ConstStmt,
    LetStmt,
    AliasStmt,
    VarStmt,
    SignalStmt,
    RegStmt,
    NextStmt,
    InstStmt,
    WhileStmt,
    ElabIfStmt,
    ElabForStmt,
    ExprStmt,
    ReturnStmt,
    RegReset,
    IdentExpr,
    IntExpr,
    StrExpr,
    BoolExpr,
    UnaryExpr,
    BinaryExpr,
    CallExpr,
    GenericAppExpr,
    AggregateExpr,
    FieldExpr,
    IndexExpr,
    GroupExpr,
    BlockExpr,
    MatchExpr,
    SelectExpr,
    InstExpr,
    CompileErrorExpr,
    RangeExpr,
    NamedExpr,
    InstArg,
    SelectArm,
    MatchArm,
    WildcardPattern,
    IdentPattern,
    IntPattern,
    BoolPattern,
    PathPattern,
    PathType,
    ArrayType,
    GenericType,
    ViewSelectType,
}

impl From<AstNodeKind> for &'static str {
    fn from(value: AstNodeKind) -> Self {
        match value {
            AstNodeKind::File => "file",
            AstNodeKind::ErrorItem => "error_item",
            AstNodeKind::PackageItem => "package_item",
            AstNodeKind::UseItem => "use_item",
            AstNodeKind::ConstItem => "const_item",
            AstNodeKind::FnItem => "fn_item",
            AstNodeKind::EnumItem => "enum_item",
            AstNodeKind::EnumVariant => "enum_variant",
            AstNodeKind::BundleItem => "bundle_item",
            AstNodeKind::InterfaceItem => "interface_item",
            AstNodeKind::MapItem => "map_item",
            AstNodeKind::CellItem => "cell_item",
            AstNodeKind::ModuleItem => "module_item",
            AstNodeKind::ExternModuleItem => "extern_module_item",
            AstNodeKind::ResultBinding => "result_binding",
            AstNodeKind::PortDecl => "port_decl",
            AstNodeKind::Param => "param",
            AstNodeKind::GenericParam => "generic_param",
            AstNodeKind::FieldDecl => "field_decl",
            AstNodeKind::Attribute => "attribute",
            AstNodeKind::ViewDecl => "view_decl",
            AstNodeKind::ViewField => "view_field",
            AstNodeKind::Block => "block",
            AstNodeKind::ErrorStmt => "error_stmt",
            AstNodeKind::ConstStmt => "const_stmt",
            AstNodeKind::LetStmt => "let_stmt",
            AstNodeKind::AliasStmt => "alias_stmt",
            AstNodeKind::VarStmt => "var_stmt",
            AstNodeKind::SignalStmt => "signal_stmt",
            AstNodeKind::RegStmt => "reg_stmt",
            AstNodeKind::NextStmt => "next_stmt",
            AstNodeKind::InstStmt => "inst_stmt",
            AstNodeKind::WhileStmt => "while_stmt",
            AstNodeKind::ElabIfStmt => "elab_if_stmt",
            AstNodeKind::ElabForStmt => "elab_for_stmt",
            AstNodeKind::ExprStmt => "expr_stmt",
            AstNodeKind::ReturnStmt => "return_stmt",
            AstNodeKind::RegReset => "reg_reset",
            AstNodeKind::IdentExpr => "ident_expr",
            AstNodeKind::IntExpr => "int_expr",
            AstNodeKind::StrExpr => "str_expr",
            AstNodeKind::BoolExpr => "bool_expr",
            AstNodeKind::UnaryExpr => "unary_expr",
            AstNodeKind::BinaryExpr => "binary_expr",
            AstNodeKind::CallExpr => "call_expr",
            AstNodeKind::GenericAppExpr => "generic_app_expr",
            AstNodeKind::AggregateExpr => "aggregate_expr",
            AstNodeKind::FieldExpr => "field_expr",
            AstNodeKind::IndexExpr => "index_expr",
            AstNodeKind::GroupExpr => "group_expr",
            AstNodeKind::BlockExpr => "block_expr",
            AstNodeKind::MatchExpr => "match_expr",
            AstNodeKind::SelectExpr => "select_expr",
            AstNodeKind::InstExpr => "inst_expr",
            AstNodeKind::CompileErrorExpr => "compile_error_expr",
            AstNodeKind::RangeExpr => "range_expr",
            AstNodeKind::NamedExpr => "named_expr",
            AstNodeKind::InstArg => "inst_arg",
            AstNodeKind::SelectArm => "select_arm",
            AstNodeKind::MatchArm => "match_arm",
            AstNodeKind::WildcardPattern => "wildcard_pattern",
            AstNodeKind::IdentPattern => "ident_pattern",
            AstNodeKind::IntPattern => "int_pattern",
            AstNodeKind::BoolPattern => "bool_pattern",
            AstNodeKind::PathPattern => "path_pattern",
            AstNodeKind::PathType => "path_type",
            AstNodeKind::ArrayType => "array_type",
            AstNodeKind::GenericType => "generic_type",
            AstNodeKind::ViewSelectType => "view_select_type",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AstNodeRecord {
    id: AstNodeId,
    kind: AstNodeKind,
    span: Span,
    range: SourceRange,
    parent: Option<AstNodeId>,
}

impl AstNodeRecord {
    pub(super) fn new(
        id: AstNodeId,
        kind: AstNodeKind,
        span: Span,
        range: SourceRange,
        parent: Option<AstNodeId>,
    ) -> Self {
        Self {
            id,
            kind,
            span,
            range,
            parent,
        }
    }

    pub fn id(&self) -> AstNodeId {
        self.id
    }

    pub fn kind(&self) -> AstNodeKind {
        self.kind
    }

    pub fn span(&self) -> Span {
        self.span
    }

    pub fn range(&self) -> SourceRange {
        self.range
    }

    pub fn parent(&self) -> Option<AstNodeId> {
        self.parent
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct AstNodeIndex {
    root_id: Option<AstNodeId>,
    nodes: Vec<AstNodeRecord>,
}

impl AstNodeIndex {
    pub fn root_id(&self) -> Option<AstNodeId> {
        self.root_id
    }

    pub fn nodes(&self) -> &[AstNodeRecord] {
        &self.nodes
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn record(&self, id: AstNodeId) -> Option<&AstNodeRecord> {
        self.nodes.iter().find(|record| record.id() == id)
    }

    pub fn find_by_span(&self, span: Span) -> Option<&AstNodeRecord> {
        self.nodes.iter().find(|record| record.span() == span)
    }

    pub(super) fn from_parts(root_id: AstNodeId, nodes: Vec<AstNodeRecord>) -> Self {
        Self {
            root_id: Some(root_id),
            nodes,
        }
    }
}
