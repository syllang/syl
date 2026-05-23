use super::{
    MapArg, MapExpr, MapFunction, MapIrProgram, MapLocalRef, MapMatchArm, MapNamedExpr,
    MapSelectArm,
};

impl MapIrProgram {
    pub(crate) fn param_count(&self) -> usize {
        self.maps.values().map(MapFunction::param_count).sum()
    }

    pub(crate) fn resolved_param_count(&self) -> usize {
        self.maps
            .values()
            .map(MapFunction::resolved_param_count)
            .sum()
    }

    pub(crate) fn local_ref_count(&self) -> usize {
        self.maps.values().map(MapFunction::local_ref_count).sum()
    }

    pub(crate) fn resolved_local_ref_count(&self) -> usize {
        self.maps
            .values()
            .map(MapFunction::resolved_local_ref_count)
            .sum()
    }
}

impl MapFunction {
    fn param_count(&self) -> usize {
        self.params.len()
    }

    fn resolved_param_count(&self) -> usize {
        self.params
            .iter()
            .filter(|param| param.id().is_some())
            .count()
    }

    fn local_ref_count(&self) -> usize {
        self.body.local_ref_count()
    }

    fn resolved_local_ref_count(&self) -> usize {
        self.body.resolved_local_ref_count()
    }
}

impl MapExpr {
    fn local_ref_count(&self) -> usize {
        match self {
            Self::Ident(_) => 1,
            Self::Unary { expr, .. } => expr.local_ref_count(),
            Self::Binary { left, right, .. } => left.local_ref_count() + right.local_ref_count(),
            Self::Call { args, .. } => args.iter().map(MapArg::local_ref_count).sum(),
            Self::Aggregate { fields, .. } => {
                fields.iter().map(MapNamedExpr::local_ref_count).sum()
            }
            Self::Field { base, .. } => base.local_ref_count(),
            Self::Index { base, index } => base.local_ref_count() + index.local_ref_count(),
            Self::Match { value, arms } => {
                value.local_ref_count()
                    + arms.iter().map(MapMatchArm::local_ref_count).sum::<usize>()
            }
            Self::Select { arms, .. } => arms.iter().map(MapSelectArm::local_ref_count).sum(),
            Self::Int(_) | Self::Bool(_) | Self::Str(_) | Self::BuiltinZero => 0,
        }
    }

    fn resolved_local_ref_count(&self) -> usize {
        match self {
            Self::Ident(local) => local.resolved_local_ref_count(),
            Self::Unary { expr, .. } => expr.resolved_local_ref_count(),
            Self::Binary { left, right, .. } => {
                left.resolved_local_ref_count() + right.resolved_local_ref_count()
            }
            Self::Call { args, .. } => args.iter().map(MapArg::resolved_local_ref_count).sum(),
            Self::Aggregate { fields, .. } => fields
                .iter()
                .map(MapNamedExpr::resolved_local_ref_count)
                .sum(),
            Self::Field { base, .. } => base.resolved_local_ref_count(),
            Self::Index { base, index } => {
                base.resolved_local_ref_count() + index.resolved_local_ref_count()
            }
            Self::Match { value, arms } => {
                value.resolved_local_ref_count()
                    + arms
                        .iter()
                        .map(MapMatchArm::resolved_local_ref_count)
                        .sum::<usize>()
            }
            Self::Select { arms, .. } => arms
                .iter()
                .map(MapSelectArm::resolved_local_ref_count)
                .sum(),
            Self::Int(_) | Self::Bool(_) | Self::Str(_) | Self::BuiltinZero => 0,
        }
    }
}

impl MapArg {
    fn local_ref_count(&self) -> usize {
        self.value().local_ref_count()
    }

    fn resolved_local_ref_count(&self) -> usize {
        self.value().resolved_local_ref_count()
    }
}

impl MapNamedExpr {
    fn local_ref_count(&self) -> usize {
        self.value().local_ref_count()
    }

    fn resolved_local_ref_count(&self) -> usize {
        self.value().resolved_local_ref_count()
    }
}

impl MapMatchArm {
    fn local_ref_count(&self) -> usize {
        self.value().local_ref_count()
    }

    fn resolved_local_ref_count(&self) -> usize {
        self.value().resolved_local_ref_count()
    }
}

impl MapSelectArm {
    fn local_ref_count(&self) -> usize {
        self.pattern().local_ref_count() + self.value().local_ref_count()
    }

    fn resolved_local_ref_count(&self) -> usize {
        self.pattern().resolved_local_ref_count() + self.value().resolved_local_ref_count()
    }
}

impl MapLocalRef {
    fn resolved_local_ref_count(&self) -> usize {
        usize::from(self.id().is_some())
    }
}
