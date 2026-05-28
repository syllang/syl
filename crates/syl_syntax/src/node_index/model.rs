use strum_macros::IntoStaticStr;
use syl_span::{SourceRange, Span};

/// Opaque identifier for a single AST node in the node index.
///
/// The inner value is a unique 1-based index assigned during parsing.
/// Zero is reserved and never assigned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct AstNodeId(u64);

impl AstNodeId {
    pub(super) fn new(raw: u64) -> Self {
        Self(raw.max(1))
    }

    /// Returns the raw numeric identifier.
    pub fn get(self) -> u64 {
        self.0
    }
}

/// What kind of AST construct a node in the index represents.
///
/// Each variant corresponds to a specific parse tree node type.
/// Serialized as `snake_case` for diagnostic output.
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

/// A single entry in the AST node index, tracking identity, kind,
/// source location, and parent relationship.
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

    /// Returns the unique identifier of this node.
    pub fn id(&self) -> AstNodeId {
        self.id
    }

    /// Returns the kind of AST construct this node represents.
    pub fn kind(&self) -> AstNodeKind {
        self.kind
    }

    /// Returns the byte span of this node in the source file.
    pub fn span(&self) -> Span {
        self.span
    }

    /// Returns the UTF-16 source range (for LSP position conversion).
    pub fn range(&self) -> SourceRange {
        self.range
    }

    /// Returns the parent node's ID, if any.
    pub fn parent(&self) -> Option<AstNodeId> {
        self.parent
    }
}

/// A flat index mapping every AST node to its kind, source span, and parent.
///
/// Built during parsing, the node index enables fast lookups from
/// a source position back to the enclosing AST node (useful for LSP
/// features like hover, go-to-definition, and diagnostics).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct AstNodeIndex {
    root_id: Option<AstNodeId>,
    nodes: Vec<AstNodeRecord>,
}

impl AstNodeIndex {
    /// Returns the root node's ID, if the index is non-empty.
    pub fn root_id(&self) -> Option<AstNodeId> {
        self.root_id
    }

    /// Returns all node records in insertion order.
    pub fn nodes(&self) -> &[AstNodeRecord] {
        &self.nodes
    }

    /// Returns the number of nodes in the index.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Returns `true` if the index is empty.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Looks up a node record by its identifier.
    pub fn record(&self, id: AstNodeId) -> Option<&AstNodeRecord> {
        self.nodes.iter().find(|record| record.id() == id)
    }

    /// Finds the first node whose span matches exactly.
    ///
    /// Matching is strict on the full `Span` value, including `source`, so a
    /// span with the same byte offsets but a different `SourceId` will not
    /// match.
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
