use std::{error::Error, fmt};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HwBindingKind {
    Module,
    Parameter,
    Port,
    LocalParam,
    Signal,
    Storage,
    Instance,
    GenerateLabel,
    GenerateIndex,
}

impl From<HwBindingKind> for &'static str {
    fn from(value: HwBindingKind) -> Self {
        match value {
            HwBindingKind::Module => "module",
            HwBindingKind::Parameter => "parameter",
            HwBindingKind::Port => "port",
            HwBindingKind::LocalParam => "localparam",
            HwBindingKind::Signal => "signal",
            HwBindingKind::Storage => "storage",
            HwBindingKind::Instance => "instance",
            HwBindingKind::GenerateLabel => "generate label",
            HwBindingKind::GenerateIndex => "generate index",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HwValidationDiagnostic {
    DuplicateModule {
        name: String,
    },
    DuplicateBinding {
        module: String,
        kind: HwBindingKind,
        name: String,
    },
    DuplicateInstanceBinding {
        module: String,
        instance: String,
        kind: HwBindingKind,
        name: String,
    },
    UnknownInstanceTarget {
        module: String,
        instance: String,
        target: String,
    },
    UnknownInstanceParam {
        module: String,
        instance: String,
        target: String,
        name: String,
    },
    UnknownInstancePort {
        module: String,
        instance: String,
        target: String,
        name: String,
    },
    UnknownReference {
        module: String,
        name: String,
    },
    InvalidIdentifier {
        module: Option<String>,
        kind: HwBindingKind,
        name: String,
    },
    InvalidWidth {
        module: String,
        kind: HwBindingKind,
        name: String,
        width: String,
    },
}

impl fmt::Display for HwValidationDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateModule { name } => write!(f, "duplicate HW module name: {name}"),
            Self::DuplicateBinding { module, kind, name } => {
                let kind: &'static str = (*kind).into();
                write!(f, "duplicate {kind} in module {module}: {name}")
            }
            Self::DuplicateInstanceBinding {
                module,
                instance,
                kind,
                name,
            } => {
                let kind: &'static str = (*kind).into();
                write!(
                    f,
                    "duplicate instance {kind} binding in {module}.{instance}: {name}"
                )
            }
            Self::UnknownInstanceTarget {
                module,
                instance,
                target,
            } => write!(
                f,
                "unknown instance target module in {module}.{instance}: {target}"
            ),
            Self::UnknownInstanceParam {
                module,
                instance,
                target,
                name,
            } => write!(
                f,
                "unknown instance parameter in {module}.{instance} for {target}: {name}"
            ),
            Self::UnknownInstancePort {
                module,
                instance,
                target,
                name,
            } => write!(
                f,
                "unknown instance port in {module}.{instance} for {target}: {name}"
            ),
            Self::UnknownReference { module, name } => {
                write!(f, "unknown HW reference in module {module}: {name}")
            }
            Self::InvalidIdentifier { module, kind, name } => {
                let kind: &'static str = (*kind).into();
                if let Some(module) = module {
                    write!(f, "invalid {kind} identifier in module {module}: {name}")
                } else {
                    write!(f, "invalid {kind} identifier: {name}")
                }
            }
            Self::InvalidWidth {
                module,
                kind,
                name,
                width,
            } => {
                let kind: &'static str = (*kind).into();
                write!(
                    f,
                    "invalid {kind} width in module {module} for {name}: {width}"
                )
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct HwValidationReport {
    diagnostics: Vec<HwValidationDiagnostic>,
}

impl HwValidationReport {
    pub fn new(diagnostics: Vec<HwValidationDiagnostic>) -> Self {
        Self { diagnostics }
    }

    pub fn diagnostics(&self) -> &[HwValidationDiagnostic] {
        &self.diagnostics
    }

    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }
}

impl fmt::Display for HwValidationReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some((first, rest)) = self.diagnostics.split_first() {
            write!(f, "{first}")?;
            for diagnostic in rest {
                write!(f, "; {diagnostic}")?;
            }
            return Ok(());
        }
        write!(f, "HW validation failed without diagnostics")
    }
}

impl Error for HwValidationReport {}
