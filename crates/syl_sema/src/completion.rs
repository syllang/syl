use syl_span::Span;

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CompletionItem {
    label: String,
    kind: CompletionKind,
    span: Span,
}

impl CompletionItem {
    pub fn new(label: String, kind: CompletionKind, span: Span) -> Self {
        Self { label, kind, span }
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn kind(&self) -> CompletionKind {
        self.kind
    }

    pub fn span(&self) -> Span {
        self.span
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CompletionKind {
    Const,
    Fn,
    Enum,
    Bundle,
    Interface,
    Map,
    Cell,
    Module,
    ExternModule,
    Generic,
    Param,
    Result,
    Let,
    Var,
    Signal,
    Reg,
    Instance,
    Loop,
    Field,
    View,
    ViewField,
    Unknown,
}

impl CompletionKind {
    pub fn from_member_kind(kind: &crate::hir::HirMemberKind) -> Self {
        match kind {
            crate::hir::HirMemberKind::Field { .. } => Self::Field,
            crate::hir::HirMemberKind::View => Self::View,
            crate::hir::HirMemberKind::ViewField { .. } => Self::ViewField,
            _ => Self::Unknown,
        }
    }

    pub fn is_type(self) -> bool {
        matches!(
            self,
            Self::Enum | Self::Bundle | Self::Interface | Self::Generic
        )
    }

    pub fn is_field(self) -> bool {
        matches!(self, Self::Field | Self::ViewField)
    }

    pub fn is_definition(self) -> bool {
        matches!(
            self,
            Self::Const
                | Self::Fn
                | Self::Enum
                | Self::Bundle
                | Self::Interface
                | Self::Map
                | Self::Cell
                | Self::Module
                | Self::ExternModule
        )
    }

    pub fn is_value_or_callable_or_local(self) -> bool {
        self.is_callable()
            || matches!(
                self,
                Self::Const
                    | Self::Generic
                    | Self::Param
                    | Self::Result
                    | Self::Let
                    | Self::Var
                    | Self::Signal
                    | Self::Reg
                    | Self::Instance
                    | Self::Loop
            )
    }

    pub fn is_callable(self) -> bool {
        matches!(
            self,
            Self::Fn | Self::Map | Self::Cell | Self::Module | Self::ExternModule
        )
    }
}

impl From<crate::hir::HirDefKind> for CompletionKind {
    fn from(kind: crate::hir::HirDefKind) -> Self {
        match kind {
            crate::hir::HirDefKind::Const => Self::Const,
            crate::hir::HirDefKind::Fn => Self::Fn,
            crate::hir::HirDefKind::Enum => Self::Enum,
            crate::hir::HirDefKind::Bundle => Self::Bundle,
            crate::hir::HirDefKind::Interface => Self::Interface,
            crate::hir::HirDefKind::Map => Self::Map,
            crate::hir::HirDefKind::Cell => Self::Cell,
            crate::hir::HirDefKind::Module => Self::Module,
            crate::hir::HirDefKind::ExternModule => Self::ExternModule,
            _ => Self::Unknown,
        }
    }
}

impl From<crate::hir::HirLocalKind> for CompletionKind {
    fn from(kind: crate::hir::HirLocalKind) -> Self {
        match kind {
            crate::hir::HirLocalKind::Generic => Self::Generic,
            crate::hir::HirLocalKind::Param => Self::Param,
            crate::hir::HirLocalKind::Result => Self::Result,
            crate::hir::HirLocalKind::Const => Self::Const,
            crate::hir::HirLocalKind::Let => Self::Let,
            crate::hir::HirLocalKind::Var => Self::Var,
            crate::hir::HirLocalKind::Signal => Self::Signal,
            crate::hir::HirLocalKind::Reg => Self::Reg,
            crate::hir::HirLocalKind::Instance => Self::Instance,
            crate::hir::HirLocalKind::Loop => Self::Loop,
            _ => Self::Unknown,
        }
    }
}
