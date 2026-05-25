use super::{HirDefKind, HirLocalKind};

impl From<HirDefKind> for &'static str {
    fn from(kind: HirDefKind) -> Self {
        match kind {
            HirDefKind::Const => "const",
            HirDefKind::Fn => "fn",
            HirDefKind::Enum => "enum",
            HirDefKind::Bundle => "bundle",
            HirDefKind::Interface => "interface",
            HirDefKind::Map => "map",
            HirDefKind::Cell => "cell",
            HirDefKind::Module => "module",
            HirDefKind::ExternModule => "extern module",
        }
    }
}

impl From<HirLocalKind> for &'static str {
    fn from(kind: HirLocalKind) -> Self {
        match kind {
            HirLocalKind::Generic => "generic",
            HirLocalKind::Param => "param",
            HirLocalKind::Result => "result",
            HirLocalKind::Const => "const",
            HirLocalKind::Let => "let",
            HirLocalKind::Var => "var",
            HirLocalKind::Signal => "signal",
            HirLocalKind::Reg => "reg",
            HirLocalKind::Instance => "instance",
            HirLocalKind::Loop => "loop",
        }
    }
}
