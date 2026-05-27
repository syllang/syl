use super::{CreateFact, CreateKind, DriveFact, guard_coverage::GuardCoverage};
use crate::{
    CompileError, DriverError,
    driver_place::{DriverObjectTable, DriverPlace, DriverStaticRange},
    eir::EirSignalActivity,
    eir::EirGuard,
};
use syl_hw::ObjectId;

#[non_exhaustive]
pub(super) struct DriverSignalActivityChecker<'a> {
    creates: &'a [CreateFact],
    drives: &'a [DriveFact],
    objects: &'a DriverObjectTable,
}

impl<'a> DriverSignalActivityChecker<'a> {
    pub(super) fn new(
        creates: &'a [CreateFact],
        drives: &'a [DriveFact],
        objects: &'a DriverObjectTable,
    ) -> Self {
        Self {
            creates,
            drives,
            objects,
        }
    }

    pub(super) fn collect_errors(&self) -> Vec<CompileError> {
        let mut errors = Vec::new();
        for create in self.creates {
            if !matches!(create.kind(), CreateKind::Signal) {
                continue;
            }
            if !matches!(create.activity(), EirSignalActivity::Required) {
                continue;
            }
            if SignalCoverage::new(create, self.drives, self.objects).is_complete() {
                continue;
            }
            errors.push(CompileError::driver_error(
                DriverError::UndrivenSignal {
                    name: create.name().to_string(),
                },
                create.origin().span(),
            ));
        }
        errors
    }
}

#[non_exhaustive]
struct SignalCoverage<'a> {
    create: &'a CreateFact,
    drives: &'a [DriveFact],
    objects: &'a DriverObjectTable,
}

impl<'a> SignalCoverage<'a> {
    fn new(
        create: &'a CreateFact,
        drives: &'a [DriveFact],
        objects: &'a DriverObjectTable,
    ) -> Self {
        Self {
            create,
            drives,
            objects,
        }
    }

    fn is_complete(&self) -> bool {
        let Some(width) = self
            .objects
            .width(self.create.object_id())
            .and_then(|width| width.value())
        else {
            return self.whole_root_guards().covers_unconditionally();
        };
        if width == 0 {
            return false;
        }
        let Some(high) = width.checked_sub(1) else {
            return false;
        };
        let required = DriverStaticRange::new(0, high);
        let extractor = SignalCoverageExtractor::new(self.create.object_id(), required);
        let mut claims = Vec::new();
        for drive in self.drives {
            if drive.module() != self.create.module() {
                continue;
            }
            if let Some(range) = extractor.range_for(drive.target_place()) {
                claims.push(SignalCoverageClaim::new(range, drive.guard()));
            }
        }
        SignalSegmentCoverage::new(required, claims).is_complete()
    }

    fn whole_root_guards(&self) -> GuardCoverage<'a> {
        let mut guards = GuardCoverage::new();
        for drive in self.drives {
            if drive.module() == self.create.module()
                && matches!(
                    drive.target_place(),
                    DriverPlace::Object(object) if object.id() == self.create.object_id()
                )
            {
                guards.add(drive.guard());
            }
        }
        guards
    }
}

#[non_exhaustive]
struct SignalCoverageExtractor {
    object_id: ObjectId,
    root_range: DriverStaticRange,
}

impl SignalCoverageExtractor {
    fn new(object_id: ObjectId, root_range: DriverStaticRange) -> Self {
        Self {
            object_id,
            root_range,
        }
    }

    fn range_for(&self, place: &DriverPlace) -> Option<DriverStaticRange> {
        match place {
            DriverPlace::Object(object) if object.id() == self.object_id => Some(self.root_range),
            DriverPlace::Slice { base, range } => {
                let base_range = self.range_for(base)?;
                let relative = range.static_range()?;
                self.apply_relative_range(base_range, relative)
            }
            DriverPlace::IndexedPartSelect { base, index, width } => {
                let base_range = self.range_for(base)?;
                let relative = DriverStaticRange::from_indexed_part(index, width)?;
                self.apply_relative_range(base_range, relative)
            }
            DriverPlace::Index { base, index } => {
                let base_range = self.range_for(base)?;
                let index = index.as_int()?;
                self.apply_relative_range(base_range, DriverStaticRange::new(index, index))
            }
            DriverPlace::Ident(_) | DriverPlace::Object(_) | DriverPlace::Expr(_) => None,
        }
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
struct SignalCoverageClaim<'a> {
    range: DriverStaticRange,
    guard: &'a EirGuard,
}

impl<'a> SignalCoverageClaim<'a> {
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
struct SignalSegmentCoverage<'a> {
    required: DriverStaticRange,
    claims: Vec<SignalCoverageClaim<'a>>,
}

impl<'a> SignalSegmentCoverage<'a> {
    fn new(required: DriverStaticRange, claims: Vec<SignalCoverageClaim<'a>>) -> Self {
        Self { required, claims }
    }

    fn is_complete(&self) -> bool {
        let mut boundaries = SignalCoverageBoundaries::new(self.required);
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
                .all(|segment| self.segment_is_covered(*segment, &clipped))
    }

    fn segment_is_covered(
        &self,
        segment: DriverStaticRange,
        claims: &[SignalCoverageClaim<'a>],
    ) -> bool {
        let mut guards = GuardCoverage::new();
        for claim in claims {
            if claim.range.contains_range(&segment) {
                guards.add(claim.guard);
            }
        }
        guards.covers_unconditionally()
    }
}

#[non_exhaustive]
struct SignalCoverageBoundaries {
    required: DriverStaticRange,
    points: Vec<u64>,
}

impl SignalCoverageBoundaries {
    fn new(required: DriverStaticRange) -> Self {
        Self {
            required,
            points: vec![required.low(), required.high().saturating_add(1)],
        }
    }

    fn add_claim(&mut self, claim: &SignalCoverageClaim<'_>) {
        self.points.push(claim.range.low());
        self.points.push(claim.range.high().saturating_add(1));
    }

    fn into_segments(mut self) -> Vec<DriverStaticRange> {
        self.points.sort_unstable();
        self.points.dedup();
        let mut segments = Vec::new();
        for pair in self.points.windows(2) {
            let low = pair[0];
            let Some(high) = pair[1].checked_sub(1) else {
                continue;
            };
            if low <= high
                && self
                    .required
                    .contains_range(&DriverStaticRange::new(low, high))
            {
                segments.push(DriverStaticRange::new(low, high));
            }
        }
        segments
    }
}
