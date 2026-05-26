use strum_macros::IntoStaticStr;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
#[non_exhaustive]
#[strum(serialize_all = "snake_case")]
pub enum AstNodeKind {
    File,
    ErrorItem,
    UseItem,
    ConstItem,
    FnItem,
    EnumItem,
    EnumVariant,
    BundleItem,
    InterfaceItem,
    MapItem,
    CellItem,
    ExternCellItem,
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
    VarStmt,
    SignalStmt,
    RegStmt,
    AssignStmt,
    DriveStmt,
    NextStmt,
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
    PlaceExpr,
    ForExpr,
    CompileErrorExpr,
    RangeExpr,
    NamedExpr,
    CallArg,
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
