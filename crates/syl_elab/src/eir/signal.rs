use super::{EirBound, EirExpr, EirGuard, EirOrigin, EirPlace};

#[non_exhaustive]
pub(crate) struct EirObject {
    module: String,
    name: String,
    width: EirBound,
    kind: EirObjectKind,
    activity: EirSignalActivity,
    origin: EirOrigin,
}

#[non_exhaustive]
pub(crate) struct EirObjectInput {
    pub(crate) module: String,
    pub(crate) name: String,
    pub(crate) width: EirBound,
    pub(crate) kind: EirObjectKind,
    pub(crate) activity: EirSignalActivity,
    pub(crate) origin: EirOrigin,
}

impl EirObject {
    pub(crate) fn new(input: EirObjectInput) -> Self {
        Self {
            module: input.module,
            name: input.name,
            width: input.width,
            kind: input.kind,
            activity: input.activity,
            origin: input.origin,
        }
    }

    pub(crate) fn module(&self) -> &str {
        &self.module
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn width_bound(&self) -> &EirBound {
        &self.width
    }

    pub(crate) fn kind(&self) -> EirObjectKind {
        self.kind
    }

    pub(crate) fn activity(&self) -> EirSignalActivity {
        self.activity
    }

    pub(crate) fn origin(&self) -> &EirOrigin {
        &self.origin
    }
}

#[derive(Clone, Copy)]
#[non_exhaustive]
pub(crate) enum EirObjectKind {
    Signal,
    Storage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum EirSignalActivity {
    Required,
    Optional,
}

#[non_exhaustive]
pub(crate) struct EirDrive {
    module: String,
    target: EirPlace,
    value: Option<EirExpr>,
    kind: EirDriveKind,
    guard: EirGuard,
    origin: EirOrigin,
}

#[non_exhaustive]
pub(crate) struct EirDriveInput {
    pub(crate) module: String,
    pub(crate) target: EirPlace,
    pub(crate) kind: EirDriveKind,
    pub(crate) value: Option<EirExpr>,
    pub(crate) guard: EirGuard,
    pub(crate) origin: EirOrigin,
}

impl EirDrive {
    pub(crate) fn new(input: EirDriveInput) -> Self {
        Self {
            module: input.module,
            target: input.target,
            value: input.value,
            kind: input.kind,
            guard: input.guard,
            origin: input.origin,
        }
    }

    pub(crate) fn module(&self) -> &str {
        &self.module
    }

    pub(crate) fn target_place(&self) -> &EirPlace {
        &self.target
    }

    pub(crate) fn value(&self) -> Option<&EirExpr> {
        self.value.as_ref()
    }

    pub(crate) fn kind(&self) -> EirDriveKind {
        self.kind
    }

    pub(crate) fn guard(&self) -> &EirGuard {
        &self.guard
    }

    pub(crate) fn origin(&self) -> &EirOrigin {
        &self.origin
    }
}

#[derive(Clone, Copy)]
#[non_exhaustive]
pub(crate) enum EirDriveKind {
    Continuous,
    Next,
}

#[non_exhaustive]
pub(crate) struct EirRead {
    module: String,
    source: EirPlace,
    guard: EirGuard,
    origin: EirOrigin,
}

impl EirRead {
    pub(crate) fn new(
        module: impl Into<String>,
        source: EirPlace,
        guard: EirGuard,
        origin: EirOrigin,
    ) -> Self {
        Self {
            module: module.into(),
            source,
            guard,
            origin,
        }
    }

    pub(crate) fn module(&self) -> &str {
        &self.module
    }

    pub(crate) fn source_place(&self) -> &EirPlace {
        &self.source
    }

    pub(crate) fn guard(&self) -> &EirGuard {
        &self.guard
    }

    pub(crate) fn origin(&self) -> &EirOrigin {
        &self.origin
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub(crate) struct EirReset {
    condition: EirExpr,
    value: EirExpr,
}

impl EirReset {
    pub(crate) fn new(condition: EirExpr, value: EirExpr) -> Self {
        Self { condition, value }
    }

    pub(crate) fn condition(&self) -> &EirExpr {
        &self.condition
    }

    pub(crate) fn value(&self) -> &EirExpr {
        &self.value
    }
}
