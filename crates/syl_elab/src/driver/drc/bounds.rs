use crate::{
    CompileError, DriverError,
    driver::place::{DriverBound, DriverObjectTable, DriverPlace, DriverStaticRange},
    eir::{EirGuard, EirOrigin},
};

use super::loop_bounds::{DriverLoopBounds, DriverLoopGuard};

#[non_exhaustive]
pub(super) struct DriverBoundsChecker<'a> {
    objects: &'a DriverObjectTable,
}

impl<'a> DriverBoundsChecker<'a> {
    pub(super) fn new(objects: &'a DriverObjectTable) -> Self {
        Self { objects }
    }

    pub(super) fn check_place(
        &self,
        place: &DriverPlace,
        guard: &EirGuard,
        origin: &EirOrigin,
    ) -> Result<(), CompileError> {
        let checker = PlaceBounds::new(place, guard, self.objects);
        if checker.is_within_bounds() {
            return Ok(());
        }
        Err(CompileError::driver_error(
            DriverError::DriverPlaceOutOfBounds {
                place: place.display(),
                root: checker.root_name(),
            },
            origin.span(),
        ))
    }
}

#[non_exhaustive]
struct PlaceBounds<'a> {
    place: &'a DriverPlace,
    guard: &'a EirGuard,
    objects: &'a DriverObjectTable,
}

impl<'a> PlaceBounds<'a> {
    fn new(place: &'a DriverPlace, guard: &'a EirGuard, objects: &'a DriverObjectTable) -> Self {
        Self {
            place,
            guard,
            objects,
        }
    }

    fn is_within_bounds(&self) -> bool {
        match self.range_for(self.place) {
            BoundsResult::Valid(_) => true,
            BoundsResult::Unknown => {
                self.is_unprojected_place(self.place)
                    || self.is_symbolic_slice_projection(self.place)
                    || self.symbolic_projection_is_proven()
            }
            BoundsResult::Invalid => false,
        }
    }

    fn root_name(&self) -> String {
        self.root_place(self.place)
            .map(DriverPlace::display)
            .unwrap_or_else(|| self.place.display())
    }

    fn range_for(&self, place: &DriverPlace) -> BoundsResult {
        match place {
            DriverPlace::Object(object) => self.object_range(object.id()),
            DriverPlace::Expr(_) => BoundsResult::Unknown,
            DriverPlace::Slice { base, range } => {
                let base_range = self.range_for(base);
                let relative = range.static_range();
                self.apply_projection(base_range, relative)
            }
            DriverPlace::IndexedPartSelect { base, index, width } => {
                let base_range = self.range_for(base);
                let relative = DriverStaticRange::from_indexed_part(index, width);
                self.apply_projection(base_range, relative)
            }
            DriverPlace::Index { base, index } => {
                if matches!(base.as_ref(), DriverPlace::Index { .. }) {
                    return BoundsResult::Unknown;
                }
                let base_range = self.range_for(base);
                let relative = index
                    .as_int()
                    .map(|index| DriverStaticRange::new(index, index));
                self.apply_projection(base_range, relative)
            }
        }
    }

    fn object_range(&self, id: syl_hw::ObjectId) -> BoundsResult {
        let Some(width) = self.objects.width(id).and_then(|width| width.value()) else {
            return BoundsResult::Unknown;
        };
        if width == 0 {
            return BoundsResult::Invalid;
        }
        match width.checked_sub(1) {
            Some(high) => BoundsResult::Valid(DriverStaticRange::new(0, high)),
            None => BoundsResult::Invalid,
        }
    }

    fn apply_projection(
        &self,
        base: BoundsResult,
        relative: Option<DriverStaticRange>,
    ) -> BoundsResult {
        match (base, relative) {
            (BoundsResult::Invalid, _) => BoundsResult::Invalid,
            (BoundsResult::Valid(_), None) => BoundsResult::Invalid,
            (BoundsResult::Unknown, _) => BoundsResult::Unknown,
            (BoundsResult::Valid(base), Some(relative)) => {
                let Some(base_width) = base.checked_width() else {
                    return BoundsResult::Invalid;
                };
                if relative.high() >= base_width {
                    BoundsResult::Invalid
                } else {
                    BoundsResult::Valid(relative)
                }
            }
        }
    }

    fn is_unprojected_place(&self, place: &DriverPlace) -> bool {
        matches!(place, DriverPlace::Object(_) | DriverPlace::Expr(_))
    }

    fn is_symbolic_slice_projection(&self, place: &DriverPlace) -> bool {
        match place {
            DriverPlace::Slice { base, .. } => {
                self.is_unprojected_place(base) || self.is_symbolic_slice_projection(base)
            }
            DriverPlace::Object(_)
            | DriverPlace::IndexedPartSelect { .. }
            | DriverPlace::Index { .. }
            | DriverPlace::Expr(_) => false,
        }
    }

    fn symbolic_projection_is_proven(&self) -> bool {
        SymbolicProjectionProof::new(self.place, self.guard, self.objects).is_proven()
    }

    fn root_place<'b>(&self, place: &'b DriverPlace) -> Option<&'b DriverPlace> {
        let mut current = place;
        loop {
            match current {
                DriverPlace::Slice { base, .. }
                | DriverPlace::IndexedPartSelect { base, .. }
                | DriverPlace::Index { base, .. } => current = base,
                DriverPlace::Object(_) => return Some(current),
                DriverPlace::Expr(_) => return None,
            }
        }
    }
}

#[derive(Clone, Copy)]
#[non_exhaustive]
enum BoundsResult {
    Valid(DriverStaticRange),
    Unknown,
    Invalid,
}

#[non_exhaustive]
struct SymbolicProjectionProof<'a> {
    place: &'a DriverPlace,
    guard: &'a EirGuard,
    objects: &'a DriverObjectTable,
}

impl<'a> SymbolicProjectionProof<'a> {
    fn new(place: &'a DriverPlace, guard: &'a EirGuard, objects: &'a DriverObjectTable) -> Self {
        Self {
            place,
            guard,
            objects,
        }
    }

    fn is_proven(&self) -> bool {
        let Some(loop_bounds) = DriverLoopGuard::new(self.guard).single_loop_bounds() else {
            return false;
        };
        SymbolicLoopProjection::new(self.place, loop_bounds, self.objects).is_in_bounds()
    }
}

#[non_exhaustive]
struct SymbolicLoopProjection<'a> {
    place: &'a DriverPlace,
    loop_bounds: DriverLoopBounds<'a>,
    objects: &'a DriverObjectTable,
}

impl<'a> SymbolicLoopProjection<'a> {
    fn new(
        place: &'a DriverPlace,
        loop_bounds: DriverLoopBounds<'a>,
        objects: &'a DriverObjectTable,
    ) -> Self {
        Self {
            place,
            loop_bounds,
            objects,
        }
    }

    fn is_in_bounds(&self) -> bool {
        match self.place {
            DriverPlace::Index { base, index } => {
                self.base_width(base)
                    .is_some_and(|width| self.loop_bounds.covers_bit_array(width))
                    && self.loop_bounds.index_matches(index)
            }
            DriverPlace::IndexedPartSelect { base, index, width } => {
                self.base_width(base)
                    .is_some_and(|base_width| self.loop_bounds.covers_part_array(base_width, width))
                    && self.loop_bounds.index_matches(index)
            }
            DriverPlace::Slice { base, .. } => {
                SymbolicLoopProjection::new(base, self.loop_bounds.clone(), self.objects)
                    .is_in_bounds()
            }
            DriverPlace::Object(_) | DriverPlace::Expr(_) => false,
        }
    }

    fn base_width(&self, place: &DriverPlace) -> Option<&DriverBound> {
        match place {
            DriverPlace::Object(object) => self.objects.width(object.id()),
            DriverPlace::Slice { base, .. }
            | DriverPlace::IndexedPartSelect { base, .. }
            | DriverPlace::Index { base, .. } => self.base_width(base),
            DriverPlace::Expr(_) => None,
        }
    }
}
