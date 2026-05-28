use super::{
    guard_coverage::GuardCoverage,
    loop_bounds::{DriverLoopBounds, DriverLoopGuard},
};
use crate::{
    CompileError, DriverError,
    driver::place::{DriverBound, DriverExpr, DriverObjectTable, DriverPlace, DriverStaticRange},
    driver::{CreateFact, CreateKind, DriveFact, ReadFact},
    eir::{EirBound, EirGuard},
    eir::{EirDesign, EirDirection, EirPort},
};
use syl_hw::ObjectId;

#[non_exhaustive]
pub(super) struct DriverCompletenessChecker<'a> {
    eir: &'a EirDesign,
    objects: &'a DriverObjectTable,
    drives: &'a [DriveFact],
}

impl<'a> DriverCompletenessChecker<'a> {
    pub(super) fn new(
        eir: &'a EirDesign,
        objects: &'a DriverObjectTable,
        drives: &'a [DriveFact],
    ) -> Self {
        Self {
            eir,
            objects,
            drives,
        }
    }

    pub(super) fn collect_errors(&self) -> Vec<CompileError> {
        let mut errors = Vec::new();
        for module in self.eir.modules() {
            if module.is_extern() {
                continue;
            }
            for port in module.ports() {
                if port.direction() != EirDirection::Out {
                    continue;
                }
                let coverage =
                    PortCoverageChecker::new(module.name(), port, self.objects, self.drives);
                if !coverage.is_complete() {
                    errors.push(CompileError::driver_error(
                        DriverError::UndrivenOut {
                            name: port.name().to_string(),
                        },
                        port.origin().span(),
                    ));
                }
            }
        }
        errors
    }
}

#[non_exhaustive]
pub(super) struct DriverReadCompletenessChecker<'a> {
    objects: &'a DriverObjectTable,
    drives: &'a [DriveFact],
    reads: &'a [ReadFact],
    creates: &'a [CreateFact],
}

impl<'a> DriverReadCompletenessChecker<'a> {
    pub(super) fn new(
        objects: &'a DriverObjectTable,
        drives: &'a [DriveFact],
        reads: &'a [ReadFact],
        creates: &'a [CreateFact],
    ) -> Self {
        Self {
            objects,
            drives,
            reads,
            creates,
        }
    }

    pub(super) fn collect_errors(&self) -> Vec<CompileError> {
        let mut errors = Vec::new();
        for read in self.reads {
            let Some(root) = ReadRoot::new(read.source_place()).local_signal(self.creates) else {
                continue;
            };
            let coverage = ReadCoverageChecker::new(root, read, self.objects, self.drives);
            if !coverage.is_complete() {
                errors.push(CompileError::driver_error(
                    DriverError::UndrivenRead {
                        name: read.source_place().display(),
                    },
                    read.origin().span(),
                ));
            }
        }
        errors
    }
}

#[non_exhaustive]
struct PortCoverageChecker<'a> {
    module: &'a str,
    port: &'a EirPort,
    objects: &'a DriverObjectTable,
    drives: &'a [DriveFact],
}

impl<'a> PortCoverageChecker<'a> {
    fn new(
        module: &'a str,
        port: &'a EirPort,
        objects: &'a DriverObjectTable,
        drives: &'a [DriveFact],
    ) -> Self {
        Self {
            module,
            port,
            objects,
            drives,
        }
    }

    fn is_complete(&self) -> bool {
        let root = CoverageRoot::new(self.objects.object_id(self.module, self.port.name()));
        let Some(width) = DriverBound::from_eir_bound(self.port.width_bound()).value() else {
            return self.whole_root_guards(root).covers_unconditionally()
                || self.symbolic_loop_coverage(root).proves_complete();
        };
        if width == 0 {
            return false;
        }
        let Some(required_high) = width.checked_sub(1) else {
            return false;
        };
        let required = DriverStaticRange::new(0, required_high);
        let mut claims = Vec::new();
        self.collect_static_claims(root, required, &mut claims);
        SegmentCoverage::new(required, claims).is_complete()
    }

    fn symbolic_loop_coverage(&self, root: CoverageRoot) -> SymbolicLoopCoverage<'a> {
        let mut coverage =
            SymbolicLoopCoverage::new(self.port.name(), self.port.width_bound(), root);
        for drive in self.drives {
            if drive.module() != self.module {
                continue;
            }
            coverage.add_drive(drive.target_place(), drive.guard());
        }
        coverage
    }

    fn whole_root_guards(&self, root: CoverageRoot) -> GuardCoverage<'a> {
        let mut guards = GuardCoverage::new();
        for drive in self.drives {
            if drive.module() != self.module {
                continue;
            }
            if root.is_whole_root(drive.target_place(), self.port.name()) {
                guards.add(drive.guard());
            }
        }
        guards
    }

    fn collect_static_claims(
        &self,
        root: CoverageRoot,
        required: DriverStaticRange,
        claims: &mut Vec<CoverageClaim<'a>>,
    ) {
        let extractor = CoverageExtractor::new(self.port.name(), root, required);
        for drive in self.drives {
            if drive.module() != self.module {
                continue;
            }
            if let Some(range) = extractor.range_for(drive.target_place()) {
                claims.push(CoverageClaim::new(range, drive.guard()));
            }
        }
    }
}

#[non_exhaustive]
struct ReadCoverageChecker<'a> {
    root: &'a CreateFact,
    read: &'a ReadFact,
    objects: &'a DriverObjectTable,
    drives: &'a [DriveFact],
}

impl<'a> ReadCoverageChecker<'a> {
    fn new(
        root: &'a CreateFact,
        read: &'a ReadFact,
        objects: &'a DriverObjectTable,
        drives: &'a [DriveFact],
    ) -> Self {
        Self {
            root,
            read,
            objects,
            drives,
        }
    }

    fn is_complete(&self) -> bool {
        let root = CoverageRoot::new(Some(self.root.object_id()));
        let Some(width) = self
            .objects
            .width(self.root.object_id())
            .and_then(|width| width.value())
        else {
            return self.whole_root_guards(root).covers_under(self.read.guard());
        };
        if width == 0 {
            return false;
        }
        let Some(required) = self.read_range(root, width) else {
            return false;
        };
        let mut claims = Vec::new();
        let extractor = CoverageExtractor::new(self.root.name(), root, required);
        for drive in self.drives {
            if drive.module() != self.read.module() {
                continue;
            }
            if let Some(range) = extractor.range_for(drive.target_place()) {
                claims.push(CoverageClaim::new(range, drive.guard()));
            }
        }
        SegmentCoverage::new(required, claims).is_complete_under(self.read.guard())
    }

    fn read_range(&self, root: CoverageRoot, width: u64) -> Option<DriverStaticRange> {
        let required = DriverStaticRange::new(0, width.checked_sub(1)?);
        CoverageExtractor::new(self.root.name(), root, required).range_for(self.read.source_place())
    }

    fn whole_root_guards(&self, root: CoverageRoot) -> GuardCoverage<'a> {
        let mut guards = GuardCoverage::new();
        for drive in self.drives {
            if drive.module() == self.read.module()
                && root.is_whole_root(drive.target_place(), self.root.name())
            {
                guards.add(drive.guard());
            }
        }
        guards
    }
}

#[derive(Clone, Copy)]
#[non_exhaustive]
struct CoverageRoot {
    object_id: Option<ObjectId>,
}

impl CoverageRoot {
    fn new(object_id: Option<ObjectId>) -> Self {
        Self { object_id }
    }

    fn matches(&self, place: &DriverPlace, fallback_name: &str) -> bool {
        match place {
            DriverPlace::Object(object) => self
                .object_id
                .map(|id| id == object.id())
                .unwrap_or_else(|| object.name() == fallback_name),
            DriverPlace::Slice { .. }
            | DriverPlace::IndexedPartSelect { .. }
            | DriverPlace::Index { .. }
            | DriverPlace::Expr(_) => false,
        }
    }

    fn is_whole_root(&self, place: &DriverPlace, fallback_name: &str) -> bool {
        self.matches(place, fallback_name)
    }
}

#[non_exhaustive]
struct ReadRoot<'a> {
    place: &'a DriverPlace,
}

impl<'a> ReadRoot<'a> {
    fn new(place: &'a DriverPlace) -> Self {
        Self { place }
    }

    fn local_signal<'b>(&self, creates: &'b [CreateFact]) -> Option<&'b CreateFact> {
        let id = self.object_id()?;
        creates
            .iter()
            .find(|create| create.object_id() == id && matches!(create.kind(), CreateKind::Signal))
    }

    fn object_id(&self) -> Option<ObjectId> {
        let mut current = self.place;
        loop {
            match current {
                DriverPlace::Object(object) => return Some(object.id()),
                DriverPlace::Slice { base, .. }
                | DriverPlace::IndexedPartSelect { base, .. }
                | DriverPlace::Index { base, .. } => current = base,
                DriverPlace::Expr(_) => return None,
            }
        }
    }
}

#[non_exhaustive]
struct SymbolicLoopCoverage<'a> {
    root_name: &'a str,
    port_width: DriverBound,
    root: CoverageRoot,
    residual_guards: Vec<EirGuard>,
}

impl<'a> SymbolicLoopCoverage<'a> {
    fn new(root_name: &'a str, port_width: &'a EirBound, root: CoverageRoot) -> Self {
        Self {
            root_name,
            port_width: DriverBound::from_eir_bound(port_width),
            root,
            residual_guards: Vec::new(),
        }
    }

    fn add_drive(&mut self, place: &'a DriverPlace, guard: &'a EirGuard) {
        let Some(loop_context) = DriverLoopGuard::new(guard).single_loop_context() else {
            return;
        };
        if SymbolicLoopDrive::new(
            place,
            self.root_name,
            self.root,
            loop_context.bounds(),
            &self.port_width,
        )
        .covers_port()
        {
            self.residual_guards
                .push(loop_context.into_residual_guard());
        }
    }

    fn proves_complete(&self) -> bool {
        let mut coverage = GuardCoverage::new();
        for guard in &self.residual_guards {
            coverage.add(guard);
        }
        coverage.covers_unconditionally()
    }
}

#[non_exhaustive]
struct SymbolicLoopDrive<'a, 'bounds> {
    place: &'a DriverPlace,
    root_name: &'a str,
    root: CoverageRoot,
    loop_bounds: &'bounds DriverLoopBounds<'a>,
    port_width: &'a DriverBound,
}

impl<'a, 'bounds> SymbolicLoopDrive<'a, 'bounds> {
    fn new(
        place: &'a DriverPlace,
        root_name: &'a str,
        root: CoverageRoot,
        loop_bounds: &'bounds DriverLoopBounds<'a>,
        port_width: &'a DriverBound,
    ) -> Self {
        Self {
            place,
            root_name,
            root,
            loop_bounds,
            port_width,
        }
    }

    fn covers_port(&self) -> bool {
        match self.place {
            DriverPlace::IndexedPartSelect { base, index, width } => {
                self.root.matches(base, self.root_name)
                    && self.loop_bounds.index_matches(index)
                    && self.loop_bounds.covers_part_array(self.port_width, width)
            }
            DriverPlace::Index { base, index } => {
                self.root.matches(base, self.root_name)
                    && self.loop_bounds.index_matches(index)
                    && self.loop_bounds.covers_bit_array(self.port_width)
            }
            DriverPlace::Object(_) | DriverPlace::Slice { .. } | DriverPlace::Expr(_) => false,
        }
    }
}

#[non_exhaustive]
struct CoverageExtractor<'a> {
    root: &'a str,
    root_object: CoverageRoot,
    root_range: DriverStaticRange,
}

impl<'a> CoverageExtractor<'a> {
    fn new(root: &'a str, root_object: CoverageRoot, root_range: DriverStaticRange) -> Self {
        Self {
            root,
            root_object,
            root_range,
        }
    }

    fn range_for(&self, place: &DriverPlace) -> Option<DriverStaticRange> {
        match place {
            DriverPlace::Object(_) if self.root_object.matches(place, self.root) => {
                Some(self.root_range)
            }
            DriverPlace::Slice { base, range } => {
                let base_range = self.base_range_for(base)?;
                let relative = range.static_range()?;
                self.apply_relative_range(base_range, relative)
            }
            DriverPlace::IndexedPartSelect { base, index, width } => {
                let base_range = self.base_range_for(base)?;
                let relative = DriverStaticRange::from_indexed_part(index, width)?;
                self.apply_relative_range(base_range, relative)
            }
            DriverPlace::Index { base, index } => {
                let base_range = self.base_range_for(base)?;
                let relative = self.index_range(index)?;
                self.apply_relative_range(base_range, relative)
            }
            DriverPlace::Object(_) | DriverPlace::Expr(_) => None,
        }
    }

    fn base_range_for(&self, place: &DriverPlace) -> Option<DriverStaticRange> {
        self.range_for(place)
    }

    fn index_range(&self, index: &DriverExpr) -> Option<DriverStaticRange> {
        let index = index.as_int()?;
        Some(DriverStaticRange::new(index, index))
    }

    fn apply_relative_range(
        &self,
        base: DriverStaticRange,
        relative: DriverStaticRange,
    ) -> Option<DriverStaticRange> {
        let base_width = base.checked_width()?;
        if relative.high() >= base_width {
            return None;
        }
        let low = base.low().checked_add(relative.low())?;
        let high = base.low().checked_add(relative.high())?;
        Some(DriverStaticRange::new(low, high))
    }
}

#[non_exhaustive]
struct CoverageClaim<'a> {
    range: DriverStaticRange,
    guard: &'a EirGuard,
}

impl<'a> CoverageClaim<'a> {
    fn new(range: DriverStaticRange, guard: &'a EirGuard) -> Self {
        Self { range, guard }
    }

    fn clipped_to(&self, required: DriverStaticRange) -> Option<Self> {
        let low = self.range.low().max(required.low());
        let high = self.range.high().min(required.high());
        if low > high {
            return None;
        }
        Some(Self::new(DriverStaticRange::new(low, high), self.guard))
    }
}

#[non_exhaustive]
struct SegmentCoverage<'a> {
    required: DriverStaticRange,
    claims: Vec<CoverageClaim<'a>>,
}

impl<'a> SegmentCoverage<'a> {
    fn new(required: DriverStaticRange, claims: Vec<CoverageClaim<'a>>) -> Self {
        Self { required, claims }
    }

    fn is_complete(&self) -> bool {
        self.is_complete_with(|guards| guards.covers_unconditionally())
    }

    fn is_complete_under(&self, context: &EirGuard) -> bool {
        self.is_complete_with(|guards| guards.covers_under(context))
    }

    fn is_complete_with(&self, mut covers: impl FnMut(&GuardCoverage<'a>) -> bool) -> bool {
        let mut boundaries = CoverageBoundaries::new(self.required);
        let mut clipped = Vec::new();
        for claim in &self.claims {
            if let Some(claim) = claim.clipped_to(self.required) {
                boundaries.add_claim(&claim);
                clipped.push(claim);
            }
        }
        let segments = boundaries.into_segments();
        !segments.is_empty()
            && segments
                .iter()
                .all(|segment| self.segment_is_covered(*segment, &clipped, &mut covers))
    }

    fn segment_is_covered(
        &self,
        segment: DriverStaticRange,
        claims: &[CoverageClaim<'a>],
        covers: &mut impl FnMut(&GuardCoverage<'a>) -> bool,
    ) -> bool {
        let mut guards = GuardCoverage::new();
        for claim in claims {
            if claim.range.contains_range(&segment) {
                guards.add(claim.guard);
            }
        }
        covers(&guards)
    }
}

#[non_exhaustive]
struct CoverageBoundaries {
    required: DriverStaticRange,
    points: Vec<u64>,
}

impl CoverageBoundaries {
    fn new(required: DriverStaticRange) -> Self {
        let mut points = vec![required.low()];
        if let Some(end) = required.high().checked_add(1) {
            points.push(end);
        }
        Self { required, points }
    }

    fn add_claim(&mut self, claim: &CoverageClaim<'_>) {
        self.points.push(claim.range.low());
        if let Some(end) = claim.range.high().checked_add(1) {
            self.points.push(end);
        }
    }

    fn into_segments(mut self) -> Vec<DriverStaticRange> {
        self.points.sort_unstable();
        self.points.dedup();
        let mut segments = Vec::new();
        for window in self.points.windows(2) {
            let [start, end] = window else {
                continue;
            };
            if start >= end {
                continue;
            }
            let Some(high) = end.checked_sub(1) else {
                continue;
            };
            let segment = DriverStaticRange::new(*start, high);
            if self.required.contains_range(&segment) {
                segments.push(segment);
            }
        }
        segments
    }
}

#[cfg(test)]
mod tests {
    use super::{CoverageRoot, SymbolicLoopCoverage};
    use crate::{
        driver::place::{DriverBound, DriverExpr, DriverPlace},
        eir::{EirBinaryOp, EirBound, EirExpr, EirGuard, EirGuardFrame},
    };
    use syl_span::Span;

    #[test]
    fn symbolic_loop_coverage_requires_branch_residual_coverage() {
        let port_width = EirBound::new(
            "N * 8",
            EirExpr::binary(EirBinaryOp::Mul, EirExpr::ident("N"), EirExpr::Int(8)),
        );
        let mut coverage = SymbolicLoopCoverage::new("out", &port_width, CoverageRoot::new(None));
        let place = DriverPlace::IndexedPartSelect {
            base: Box::new(DriverPlace::test_object(0, "out")),
            index: DriverExpr::Ident("i".to_string()),
            width: DriverBound::new("8"),
        };
        let loop_frame = EirGuardFrame::loop_frame("gen_i", "i", "0", "N", Span::default());
        let then_guard = EirGuard::from_frames(&[
            loop_frame.clone(),
            EirGuardFrame::if_then("branch", Span::default()),
        ]);
        let else_guard = EirGuard::from_frames(&[
            loop_frame,
            EirGuardFrame::if_else("branch", Span::default()),
        ]);

        coverage.add_drive(&place, &then_guard);
        assert!(!coverage.proves_complete());

        coverage.add_drive(&place, &else_guard);
        assert!(coverage.proves_complete());
    }
}
