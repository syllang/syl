use super::SemanticDiagnosticStage;
use syl_span::{Diagnostic, DiagnosticRelatedInfo, Span};
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum HirError {
    #[error("duplicate const {name}")]
    DuplicateConst { name: String },
    #[error("duplicate fn {name}")]
    DuplicateFn { name: String },
    #[error("duplicate enum {name}")]
    DuplicateEnum { name: String },
    #[error("duplicate enum variant {name}")]
    DuplicateEnumVariant { name: String },
    #[error("empty enum {name}")]
    EmptyEnum { name: String },
    #[error("duplicate struct {name}")]
    DuplicateStruct { name: String },
    #[error("duplicate bundle {name}")]
    DuplicateBundle { name: String },
    #[error("duplicate interface {name}")]
    DuplicateInterface { name: String },
    #[error("duplicate map {name}")]
    DuplicateMap { name: String },
    #[error("duplicate callable {name}")]
    DuplicateCallable { name: String },
    #[error("unresolved name {name}")]
    UnresolvedName { name: String },
    #[error("internal HIR expression id is missing for expression at {start}..{end}")]
    MissingHirExpr { start: usize, end: usize },
    #[error("internal HIR definition id is missing for {name}")]
    MissingHirDef { name: String },
    #[error("internal HIR local id is missing for {name} at {start}..{end}")]
    MissingHirLocal {
        name: String,
        start: usize,
        end: usize,
    },
    #[error("unknown import {path} at {start}..{end}")]
    UnknownImport {
        path: String,
        start: usize,
        end: usize,
    },
    #[error("ambiguous import {name}; candidates: {candidates}")]
    AmbiguousImport { name: String, candidates: String },
}

impl HirError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::DuplicateConst { .. } => "E_MIDDLE_DUPLICATE_CONST",
            Self::DuplicateFn { .. } => "E_MIDDLE_DUPLICATE_FN",
            Self::DuplicateEnum { .. } => "E_MIDDLE_DUPLICATE_ENUM",
            Self::DuplicateEnumVariant { .. } => "E_MIDDLE_DUPLICATE_ENUM_VARIANT",
            Self::EmptyEnum { .. } => "E_MIDDLE_EMPTY_ENUM",
            Self::DuplicateStruct { .. } => "E_MIDDLE_DUPLICATE_STRUCT",
            Self::DuplicateBundle { .. } => "E_MIDDLE_DUPLICATE_BUNDLE",
            Self::DuplicateInterface { .. } => "E_MIDDLE_DUPLICATE_INTERFACE",
            Self::DuplicateMap { .. } => "E_MIDDLE_DUPLICATE_MAP",
            Self::DuplicateCallable { .. } => "E_MIDDLE_DUPLICATE_CALLABLE",
            Self::UnresolvedName { .. } => "E_MIDDLE_UNRESOLVED_NAME",
            Self::MissingHirExpr { .. } => "E_MIDDLE_MISSING_HIR_EXPR",
            Self::MissingHirDef { .. } => "E_MIDDLE_MISSING_HIR_DEF",
            Self::MissingHirLocal { .. } => "E_MIDDLE_MISSING_HIR_LOCAL",
            Self::UnknownImport { .. } => "E_MIDDLE_UNKNOWN_IMPORT",
            Self::AmbiguousImport { .. } => "E_MIDDLE_AMBIGUOUS_IMPORT",
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TirError {
    #[error("elaboration if requires bool condition")]
    ElaborationIfRequiresBool,
    #[error("{context} requires nat expression")]
    RequiresNatExpression { context: String },
    #[error("duplicate enum discriminant {value} in {enum_name}")]
    DuplicateEnumDiscriminant { enum_name: String, value: u64 },
    #[error("enum discriminant {variant}={value} does not fit width {width} in {enum_name}")]
    EnumDiscriminantOutOfRange {
        enum_name: String,
        variant: String,
        value: u64,
        width: u64,
    },
    #[error("enum discriminant {variant}={value} is not one-hot in {enum_name}")]
    EnumDiscriminantNotOneHot {
        enum_name: String,
        variant: String,
        value: u64,
    },
    #[error("unknown type {name}")]
    UnknownType { name: String },
    #[error("expression is not valid in elaboration context")]
    InvalidElaborationExpression,
    #[error("bool is a const/proposition type and cannot be used as a hardware value")]
    BoolInHardwareValue,
    #[error("hardware expressions cannot use const/proposition operator {op}")]
    SoftwareOperatorInHardware { op: String },
    #[error("select expression requires a default arm")]
    SelectRequiresDefault,
    #[error("match expression requires at least one arm")]
    MatchRequiresArm,
    #[error("aggregate field {field} is missing for {ty}")]
    MissingAggregateField { ty: String, field: String },
    #[error("aggregate field {field} does not exist on {ty}")]
    UnknownAggregateField { ty: String, field: String },
    #[error("unknown method {method} for {receiver}")]
    UnknownMethod { receiver: String, method: String },
    #[error("ambiguous method {method} for {receiver}; candidates: {candidates}")]
    AmbiguousMethod {
        receiver: String,
        method: String,
        candidates: String,
    },
}

impl TirError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::ElaborationIfRequiresBool => "E_MIDDLE_ELAB_IF_REQUIRES_BOOL",
            Self::RequiresNatExpression { .. } => "E_MIDDLE_REQUIRES_NAT",
            Self::DuplicateEnumDiscriminant { .. } => "E_MIDDLE_DUPLICATE_ENUM_DISCRIMINANT",
            Self::EnumDiscriminantOutOfRange { .. } => "E_MIDDLE_ENUM_DISCRIMINANT_OUT_OF_RANGE",
            Self::EnumDiscriminantNotOneHot { .. } => "E_MIDDLE_ENUM_DISCRIMINANT_NOT_ONEHOT",
            Self::UnknownType { .. } => "E_MIDDLE_UNKNOWN_TYPE",
            Self::InvalidElaborationExpression => "E_MIDDLE_INVALID_ELAB_EXPR",
            Self::BoolInHardwareValue => "E_MIDDLE_BOOL_IN_HARDWARE_VALUE",
            Self::SoftwareOperatorInHardware { .. } => "E_MIDDLE_SOFTWARE_OP_IN_HARDWARE",
            Self::SelectRequiresDefault => "E_MIDDLE_SELECT_REQUIRES_DEFAULT",
            Self::MatchRequiresArm => "E_MIDDLE_MATCH_REQUIRES_ARM",
            Self::MissingAggregateField { .. } => "E_MIDDLE_MISSING_AGGREGATE_FIELD",
            Self::UnknownAggregateField { .. } => "E_MIDDLE_UNKNOWN_AGGREGATE_FIELD",
            Self::UnknownMethod { .. } => "E_MIDDLE_UNKNOWN_METHOD",
            Self::AmbiguousMethod { .. } => "E_MIDDLE_AMBIGUOUS_METHOD",
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConstEvalError {
    #[error("{context} is not an elaboration-time expression: {source}")]
    NotElaborationTimeExpression {
        context: String,
        source: Box<LoweringError>,
    },
    #[error("{name} is a hardware value")]
    HardwareValueInConstContext { name: String },
    #[error("unknown elaboration identifier {name}")]
    UnknownElaborationIdentifier { name: String },
    #[error("invalid const unary expression")]
    InvalidConstUnaryExpression,
    #[error("expression is not valid in elaboration context")]
    InvalidElaborationExpression,
    #[error("invalid const binary expression")]
    InvalidConstBinaryExpression,
    #[error("const equality operands have different types")]
    ConstEqualityTypeMismatch,
    #[error("const comparison operands have different types")]
    ConstComparisonTypeMismatch,
    #[error("const logical operands have different types")]
    ConstLogicalTypeMismatch,
    #[error("const arithmetic operands have different types")]
    ConstArithmeticTypeMismatch,
    #[error("const evaluation exceeded sandbox limit of {limit} steps")]
    StepLimitExceeded { limit: usize },
}

impl ConstEvalError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotElaborationTimeExpression { .. } => "E_MIDDLE_NOT_ELAB_TIME",
            Self::HardwareValueInConstContext { .. } => "E_MIDDLE_HARDWARE_VALUE_IN_CONST",
            Self::UnknownElaborationIdentifier { .. } => "E_MIDDLE_UNKNOWN_ELAB_IDENTIFIER",
            Self::InvalidConstUnaryExpression => "E_MIDDLE_INVALID_CONST_UNARY",
            Self::InvalidElaborationExpression => "E_MIDDLE_INVALID_ELAB_EXPR",
            Self::InvalidConstBinaryExpression => "E_MIDDLE_INVALID_CONST_BINARY",
            Self::ConstEqualityTypeMismatch => "E_MIDDLE_CONST_EQUALITY_TYPE_MISMATCH",
            Self::ConstComparisonTypeMismatch => "E_MIDDLE_CONST_COMPARISON_TYPE_MISMATCH",
            Self::ConstLogicalTypeMismatch => "E_MIDDLE_CONST_LOGICAL_TYPE_MISMATCH",
            Self::ConstArithmeticTypeMismatch => "E_MIDDLE_CONST_ARITH_TYPE_MISMATCH",
            Self::StepLimitExceeded { .. } => "E_MIDDLE_CONST_STEP_LIMIT",
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CapabilityError {
    #[error("{target} is not drivable in this scope")]
    NotDrivable { target: String },
    #[error("{target} is not readable in this scope")]
    NotReadable { target: String },
    #[error("unresolved name {name}")]
    UnresolvedName { name: String },
    #[error("select guard must be a hardware Bit expression")]
    SelectGuardRequiresBit,
    #[error("expression is not supported by hardware value lowering")]
    UnsupportedHardwareValueExpression,
    #[error("unknown interface {name}")]
    UnknownInterface { name: String },
    #[error("unknown view {name}")]
    UnknownView { name: String },
}

impl CapabilityError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotDrivable { .. } => "E_MIDDLE_NOT_DRIVABLE",
            Self::NotReadable { .. } => "E_MIDDLE_NOT_READABLE",
            Self::UnresolvedName { .. } => "E_MIDDLE_UNRESOLVED_NAME",
            Self::SelectGuardRequiresBit => "E_MIDDLE_SELECT_GUARD_REQUIRES_BIT",
            Self::UnsupportedHardwareValueExpression => "E_MIDDLE_UNSUPPORTED_HW_VALUE_EXPR",
            Self::UnknownInterface { .. } => "E_MIDDLE_UNKNOWN_INTERFACE",
            Self::UnknownView { .. } => "E_MIDDLE_UNKNOWN_VIEW",
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EirError {
    #[error("while/return are not legal in hardware bodies")]
    IllegalHardwareStatement,
    #[error("local const/let/var lowering is not implemented")]
    LocalBindingLoweringUnsupported,
    #[error("signal requires an explicit type in the HWIR lowerer")]
    SignalRequiresType,
    #[error("reg requires a type")]
    RegisterRequiresType,
    #[error("reg requires one matching Clock port or exactly one Clock port in scope")]
    RegisterRequiresClock,
    #[error("reg reset requires a Reset expression or exactly one Reset port in scope")]
    RegisterRequiresReset,
    #[error("invalid interface type")]
    InvalidInterfaceType,
    #[error("unknown interface {name}")]
    UnknownInterface { name: String },
    #[error("unknown view {name}")]
    UnknownView { name: String },
    #[error("instance callee must be a name")]
    InstanceCalleeMustBeName,
    #[error("unknown parameter {name} for {callable}")]
    UnknownParameter { name: String, callable: String },
    #[error("too many positional arguments for {callable}")]
    TooManyPositionalArguments { callable: String },
    #[error("duplicate argument for parameter {name}")]
    DuplicateArgument { name: String },
    #[error("duplicate connection for port {name}")]
    DuplicateConnection { name: String },
    #[error("map expressions cannot contain assignment")]
    AssignmentInMap,
    #[error("map expressions cannot call hardware generator {name}")]
    HardwareGeneratorCallInMap { name: String },
    #[error("hardware value expressions cannot call generator {name}; use place")]
    HardwareGeneratorCallInExpression { name: String },
    #[error("hardware value expressions cannot call unknown function {name}")]
    UnknownHardwareValueCall { name: String },
    #[error("reg {name} cannot be driven directly; use next {name} := ...")]
    ContinuousDriveTargetIsReg { name: String },
    #[error("expression is not supported by hardware value lowering")]
    UnsupportedHardwareValueExpression,
    #[error("aggregate field {field} is missing for {ty}")]
    MissingAggregateField { ty: String, field: String },
    #[error("aggregate field {field} does not exist on {ty}")]
    UnknownAggregateField { ty: String, field: String },
    #[error("expression is not valid in elaboration context")]
    InvalidElaborationExpression,
    #[error("unknown elaboration identifier {name}")]
    UnknownElaborationIdentifier { name: String },
    #[error("inplace requires a cell with a visible body, but {name} is declared extern")]
    InplaceOnExternCell { name: String },
}

impl EirError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::IllegalHardwareStatement => "E_MIDDLE_ILLEGAL_HARDWARE_STATEMENT",
            Self::LocalBindingLoweringUnsupported => "E_MIDDLE_LOCAL_BINDING_UNSUPPORTED",
            Self::SignalRequiresType => "E_MIDDLE_SIGNAL_REQUIRES_TYPE",
            Self::RegisterRequiresType => "E_MIDDLE_REGISTER_REQUIRES_TYPE",
            Self::RegisterRequiresClock => "E_MIDDLE_REGISTER_REQUIRES_CLOCK",
            Self::RegisterRequiresReset => "E_MIDDLE_REGISTER_REQUIRES_RESET",
            Self::InvalidInterfaceType => "E_MIDDLE_INVALID_INTERFACE_TYPE",
            Self::UnknownInterface { .. } => "E_MIDDLE_UNKNOWN_INTERFACE",
            Self::UnknownView { .. } => "E_MIDDLE_UNKNOWN_VIEW",
            Self::InstanceCalleeMustBeName => "E_MIDDLE_INSTANCE_CALLEE_MUST_BE_NAME",
            Self::UnknownParameter { .. } => "E_MIDDLE_UNKNOWN_PARAMETER",
            Self::TooManyPositionalArguments { .. } => "E_MIDDLE_TOO_MANY_POSITIONAL_ARGS",
            Self::DuplicateArgument { .. } => "E_MIDDLE_DUPLICATE_ARGUMENT",
            Self::DuplicateConnection { .. } => "E_MIDDLE_DUPLICATE_CONNECTION",
            Self::AssignmentInMap => "E_MIDDLE_ASSIGNMENT_IN_MAP",
            Self::HardwareGeneratorCallInMap { .. } => "E_MIDDLE_HW_GENERATOR_CALL_IN_MAP",
            Self::HardwareGeneratorCallInExpression { .. } => "E_MIDDLE_HW_GENERATOR_CALL_IN_EXPR",
            Self::UnknownHardwareValueCall { .. } => "E_MIDDLE_UNKNOWN_HW_VALUE_CALL",
            Self::ContinuousDriveTargetIsReg { .. } => "E_MIDDLE_CONTINUOUS_DRIVE_TO_REG",
            Self::UnsupportedHardwareValueExpression => "E_MIDDLE_UNSUPPORTED_HW_VALUE_EXPR",
            Self::MissingAggregateField { .. } => "E_MIDDLE_MISSING_AGGREGATE_FIELD",
            Self::UnknownAggregateField { .. } => "E_MIDDLE_UNKNOWN_AGGREGATE_FIELD",
            Self::InvalidElaborationExpression => "E_MIDDLE_INVALID_ELAB_EXPR",
            Self::UnknownElaborationIdentifier { .. } => "E_MIDDLE_UNKNOWN_ELAB_IDENTIFIER",
            Self::InplaceOnExternCell { .. } => "E_MIDDLE_INPLACE_ON_EXTERN_CELL",
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DriverError {
    #[error("unknown hardware object {name} in module {module}")]
    UnknownHardwareObject { module: String, name: String },
    #[error("next target {name} is not a reg")]
    NextTargetIsNotReg { name: String },
    #[error("continuous drive target {name} is a reg; use next {name} := ...")]
    ContinuousDriveTargetIsReg { name: String },
    #[error("duplicate next driver for {name}")]
    DuplicateNextDriver { name: String },
    #[error("duplicate hardware driver for {name}")]
    DuplicateHardwareDriver { name: String },
    #[error("out {name} is not driven")]
    UndrivenOut { name: String },
    #[error("{place} is outside the bounds of {root}")]
    DriverPlaceOutOfBounds { place: String, root: String },
    #[error("{name} is read before it is fully driven")]
    UndrivenRead { name: String },
    #[error("signal {name} is not driven")]
    UndrivenSignal { name: String },
    #[error("cell boundary {callable} instance {instance} has no available summary ({status})")]
    MissingCellSummary {
        callable: String,
        instance: String,
        status: String,
    },
    #[error("expression is not supported by hardware value lowering")]
    UnsupportedHardwareValueExpression,
    #[error("unknown parameter {name} for {callable}")]
    UnknownParameter { name: String, callable: String },
}

impl DriverError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownHardwareObject { .. } => "E_MIDDLE_UNKNOWN_HW_OBJECT",
            Self::NextTargetIsNotReg { .. } => "E_MIDDLE_NEXT_TARGET_NOT_REG",
            Self::ContinuousDriveTargetIsReg { .. } => "E_MIDDLE_CONTINUOUS_DRIVE_TO_REG",
            Self::DuplicateNextDriver { .. } => "E_MIDDLE_DUPLICATE_NEXT_DRIVER",
            Self::DuplicateHardwareDriver { .. } => "E_MIDDLE_DUPLICATE_HARDWARE_DRIVER",
            Self::UndrivenOut { .. } => "E_MIDDLE_UNDRIVEN_OUT",
            Self::DriverPlaceOutOfBounds { .. } => "E_MIDDLE_DRIVER_PLACE_OUT_OF_BOUNDS",
            Self::UndrivenRead { .. } => "E_MIDDLE_UNDRIVEN_READ",
            Self::UndrivenSignal { .. } => "E_MIDDLE_UNDRIVEN_SIGNAL",
            Self::MissingCellSummary { .. } => "E_MIDDLE_MISSING_CELL_SUMMARY",
            Self::UnsupportedHardwareValueExpression => "E_MIDDLE_UNSUPPORTED_HW_VALUE_EXPR",
            Self::UnknownParameter { .. } => "E_MIDDLE_UNKNOWN_PARAMETER",
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum HwirError {
    #[error("expression is not supported by hardware value lowering")]
    UnsupportedHardwareValueExpression,
}

impl HwirError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnsupportedHardwareValueExpression => "E_MIDDLE_UNSUPPORTED_HW_VALUE_EXPR",
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LoweringError {
    #[error("{0}")]
    Hir(#[from] HirError),
    #[error("{0}")]
    Tir(#[from] TirError),
    #[error("{0}")]
    Const(#[from] ConstEvalError),
    #[error("{0}")]
    Capability(#[from] CapabilityError),
    #[error("{0}")]
    Eir(#[from] EirError),
    #[error("{0}")]
    Driver(#[from] DriverError),
    #[error("{0}")]
    Hwir(#[from] HwirError),
}

impl LoweringError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Hir(error) => error.code(),
            Self::Tir(error) => error.code(),
            Self::Const(error) => error.code(),
            Self::Capability(error) => error.code(),
            Self::Eir(error) => error.code(),
            Self::Driver(error) => error.code(),
            Self::Hwir(error) => error.code(),
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CompileError {
    #[error("{kind}")]
    Lowering {
        kind: Box<LoweringError>,
        diagnostic: Box<Diagnostic>,
    },
}

impl CompileError {
    pub fn kind(&self) -> &LoweringError {
        match self {
            Self::Lowering { kind, .. } => kind.as_ref(),
        }
    }

    pub fn diagnostic(&self) -> &Diagnostic {
        match self {
            Self::Lowering { diagnostic, .. } => diagnostic,
        }
    }

    pub fn to_diagnostic(&self) -> Diagnostic {
        Diagnostic::from(self)
    }

    pub fn lowering_at(kind: impl Into<LoweringError>, span: Span) -> Self {
        let kind = kind.into();
        let code = kind.code();
        let message = kind.to_string();
        Self::Lowering {
            kind: Box::new(kind),
            diagnostic: Box::new(Self::diagnostic_for(
                SemanticDiagnosticStage::Lowering,
                span,
                code,
                message,
                [],
            )),
        }
    }

    pub fn driver_error(kind: impl Into<LoweringError>, span: Span) -> Self {
        let kind = kind.into();
        let code = kind.code();
        let message = kind.to_string();
        Self::Lowering {
            kind: Box::new(kind),
            diagnostic: Box::new(Self::diagnostic_for(
                SemanticDiagnosticStage::Driver,
                span,
                code,
                message,
                [],
            )),
        }
    }

    pub fn driver_error_with_related(
        kind: impl Into<LoweringError>,
        span: Span,
        related: impl IntoIterator<Item = (Span, String)>,
    ) -> Self {
        let kind = kind.into();
        let code = kind.code();
        let message = kind.to_string();
        let diagnostic = Self::diagnostic_for(
            SemanticDiagnosticStage::Driver,
            span,
            code,
            message,
            related,
        );
        Self::Lowering {
            kind: Box::new(kind),
            diagnostic: Box::new(diagnostic),
        }
    }

    fn diagnostic_for(
        stage: SemanticDiagnosticStage,
        span: Span,
        code: &'static str,
        message: impl Into<String>,
        related: impl IntoIterator<Item = (Span, String)>,
    ) -> Diagnostic {
        let stage: &'static str = stage.into();
        let mut diagnostic = Diagnostic::new(span, message)
            .with_code(code)
            .with_source(format!("syl_sema::{stage}"));
        for (span, message) in related {
            diagnostic = diagnostic.with_related(DiagnosticRelatedInfo::new(span, message));
        }
        diagnostic
    }
}

impl From<&CompileError> for Diagnostic {
    fn from(value: &CompileError) -> Self {
        match value {
            CompileError::Lowering { diagnostic, .. } => diagnostic.as_ref().clone(),
        }
    }
}

impl From<CompileError> for Diagnostic {
    fn from(value: CompileError) -> Self {
        Diagnostic::from(&value)
    }
}
