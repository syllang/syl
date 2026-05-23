use super::{
    DriverExpr, DriverPlace,
    bounds::{DriverBitRange, DriverBound, DriverStaticRange},
};

#[non_exhaustive]
pub(super) struct DriverPlaceOverlap<'a> {
    left: &'a DriverPlace,
    right: &'a DriverPlace,
}

impl<'a> DriverPlaceOverlap<'a> {
    pub(super) fn new(left: &'a DriverPlace, right: &'a DriverPlace) -> Self {
        Self { left, right }
    }

    pub(super) fn may_overlap(&self) -> bool {
        PlaceFact::from(self.left).may_overlap(&PlaceFact::from(self.right))
    }
}

#[non_exhaustive]
struct PlaceFact<'a> {
    root: PlaceRoot<'a>,
    projections: Vec<PlaceProjection<'a>>,
}

#[non_exhaustive]
enum PlaceRoot<'a> {
    Ident(&'a str),
    Object { id: syl_hw::ObjectId, name: &'a str },
    Expr(&'a DriverExpr),
}

#[non_exhaustive]
enum PlaceProjection<'a> {
    Slice {
        range: &'a DriverBitRange,
    },
    IndexedPartSelect {
        index: &'a DriverExpr,
        width: &'a DriverBound,
    },
    Index(&'a DriverExpr),
}

impl<'a> From<&'a DriverPlace> for PlaceFact<'a> {
    fn from(place: &'a DriverPlace) -> Self {
        let mut projections = Vec::new();
        let mut current = place;
        let root = loop {
            match current {
                DriverPlace::Ident(root) => break PlaceRoot::Ident(root),
                DriverPlace::Object(object) => {
                    break PlaceRoot::Object {
                        id: object.id(),
                        name: object.name(),
                    };
                }
                DriverPlace::Slice { base, range } => {
                    projections.push(PlaceProjection::Slice { range });
                    current = base;
                }
                DriverPlace::IndexedPartSelect { base, index, width } => {
                    projections.push(PlaceProjection::IndexedPartSelect { index, width });
                    current = base;
                }
                DriverPlace::Index { base, index } => {
                    projections.push(PlaceProjection::Index(index));
                    current = base;
                }
                DriverPlace::Expr(expr) => break PlaceRoot::Expr(expr),
            }
        };
        projections.reverse();
        Self { root, projections }
    }
}

impl PlaceFact<'_> {
    fn may_overlap(&self, other: &Self) -> bool {
        self.root.may_overlap(&other.root)
            && self
                .projections
                .iter()
                .zip(&other.projections)
                .all(|(left, right)| ProjectionOverlap::new(left, right).may_overlap())
    }
}

impl PlaceRoot<'_> {
    fn may_overlap(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Object { id: left, .. }, Self::Object { id: right, .. }) => left == right,
            (Self::Object { name: left, .. }, Self::Ident(right))
            | (Self::Ident(right), Self::Object { name: left, .. }) => left == right,
            (Self::Ident(left), Self::Ident(right)) => left == right,
            (Self::Expr(left), Self::Expr(right)) => left == right,
            (Self::Ident(root), Self::Expr(expr)) | (Self::Expr(expr), Self::Ident(root)) => {
                expr.references_root(root)
            }
            (Self::Object { name, .. }, Self::Expr(expr))
            | (Self::Expr(expr), Self::Object { name, .. }) => expr.references_root(name),
        }
    }
}

#[non_exhaustive]
struct ProjectionOverlap<'a> {
    left: &'a PlaceProjection<'a>,
    right: &'a PlaceProjection<'a>,
}

impl<'a> ProjectionOverlap<'a> {
    fn new(left: &'a PlaceProjection<'a>, right: &'a PlaceProjection<'a>) -> Self {
        Self { left, right }
    }

    fn may_overlap(&self) -> bool {
        match (self.left, self.right) {
            (PlaceProjection::Index(left), PlaceProjection::Index(right)) => {
                self.indexes_may_overlap(left, right)
            }
            (
                PlaceProjection::IndexedPartSelect {
                    index: left_index,
                    width: left_width,
                },
                PlaceProjection::IndexedPartSelect {
                    index: right_index,
                    width: right_width,
                },
            ) => self.indexed_ranges_may_overlap(left_index, left_width, right_index, right_width),
            (PlaceProjection::Slice { range: left }, PlaceProjection::Slice { range: right }) => {
                left == right || left.may_overlap(right)
            }
            (
                PlaceProjection::IndexedPartSelect { index, width },
                PlaceProjection::Slice { range },
            )
            | (
                PlaceProjection::Slice { range },
                PlaceProjection::IndexedPartSelect { index, width },
            ) => self.indexed_range_may_overlap_slice(index, width, range),
            (PlaceProjection::IndexedPartSelect { index, width }, PlaceProjection::Index(bit))
            | (PlaceProjection::Index(bit), PlaceProjection::IndexedPartSelect { index, width }) => {
                self.indexed_range_may_contain_index(index, width, bit)
            }
            (PlaceProjection::Slice { range }, PlaceProjection::Index(index))
            | (PlaceProjection::Index(index), PlaceProjection::Slice { range }) => {
                self.index_may_overlap_range(index, range)
            }
        }
    }

    fn indexes_may_overlap(&self, left: &DriverExpr, right: &DriverExpr) -> bool {
        match (left.as_int(), right.as_int()) {
            (Some(left), Some(right)) => left == right,
            _ => true,
        }
    }

    fn indexed_ranges_may_overlap(
        &self,
        left_index: &DriverExpr,
        left_width: &DriverBound,
        right_index: &DriverExpr,
        right_width: &DriverBound,
    ) -> bool {
        if left_width == right_width
            && let (Some(left), Some(right)) = (left_index.as_int(), right_index.as_int())
            && left != right
        {
            return false;
        }
        let Some(left) = DriverStaticRange::from_indexed_part(left_index, left_width) else {
            return true;
        };
        let Some(right) = DriverStaticRange::from_indexed_part(right_index, right_width) else {
            return true;
        };
        left.may_overlap(&right)
    }

    fn indexed_range_may_overlap_slice(
        &self,
        index: &DriverExpr,
        width: &DriverBound,
        slice: &DriverBitRange,
    ) -> bool {
        let Some(indexed_range) = DriverStaticRange::from_indexed_part(index, width) else {
            return true;
        };
        let Some(slice_range) = slice.static_range() else {
            return true;
        };
        indexed_range.may_overlap(&slice_range)
    }

    fn indexed_range_may_contain_index(
        &self,
        part_index: &DriverExpr,
        width: &DriverBound,
        bit_index: &DriverExpr,
    ) -> bool {
        let Some(bit_index) = bit_index.as_int() else {
            return true;
        };
        let Some(range) = DriverStaticRange::from_indexed_part(part_index, width) else {
            return true;
        };
        range.contains(bit_index)
    }

    fn index_may_overlap_range(&self, index: &DriverExpr, range: &DriverBitRange) -> bool {
        let Some(index) = index.as_int() else {
            return true;
        };
        range.may_contain_index(index)
    }
}

#[cfg(test)]
mod tests {
    use super::{DriverPlace, DriverPlaceOverlap};
    use crate::driver_place::{DriverBitRange, DriverObject};
    use syl_hw::ObjectId;

    #[test]
    fn unknown_slice_bounds_are_conservative() {
        let left = DriverPlace::Slice {
            base: Box::new(DriverPlace::Ident("word".to_string())),
            range: DriverBitRange::new("0", "HI"),
        };
        let right = DriverPlace::Slice {
            base: Box::new(DriverPlace::Ident("word".to_string())),
            range: DriverBitRange::new("4", "7"),
        };

        assert!(DriverPlaceOverlap::new(&left, &right).may_overlap());
    }

    #[test]
    fn arithmetic_slice_bounds_resolve_before_overlap_check() {
        let computed_bit_zero = DriverPlace::Slice {
            base: Box::new(DriverPlace::Ident("rsp".to_string())),
            range: DriverBitRange::new("0", "(0) + (1) - 1"),
        };
        let literal_bit_one = DriverPlace::Slice {
            base: Box::new(DriverPlace::Ident("rsp".to_string())),
            range: DriverBitRange::new("1", "1"),
        };

        assert!(!computed_bit_zero.overlaps(&literal_bit_one));
    }

    #[test]
    fn symbolic_high_with_known_low_can_be_disjoint() {
        let generic_upper_field = DriverPlace::Slice {
            base: Box::new(DriverPlace::Ident("rsp".to_string())),
            range: DriverBitRange::new("(0) + (1)", "((0) + (1)) + (W) - 1"),
        };
        let literal_bit_zero = DriverPlace::Slice {
            base: Box::new(DriverPlace::Ident("rsp".to_string())),
            range: DriverBitRange::new("0", "(0) + (1) - 1"),
        };

        assert!(!generic_upper_field.overlaps(&literal_bit_zero));
    }

    #[test]
    fn adjacent_symbolic_field_slices_are_disjoint() {
        let lower_field = DriverPlace::Slice {
            base: Box::new(DriverPlace::Ident("pair".to_string())),
            range: DriverBitRange::new("0", "(0) + (W) - 1"),
        };
        let upper_field = DriverPlace::Slice {
            base: Box::new(DriverPlace::Ident("pair".to_string())),
            range: DriverBitRange::new("(0) + (W)", "((0) + (W)) + (W) - 1"),
        };

        assert!(!lower_field.overlaps(&upper_field));
        assert!(!upper_field.overlaps(&lower_field));
    }

    #[test]
    fn intersecting_symbolic_field_slices_remain_conservative() {
        let left = DriverPlace::Slice {
            base: Box::new(DriverPlace::Ident("word".to_string())),
            range: DriverBitRange::new("0", "(0) + (W) - 1"),
        };
        let right = DriverPlace::Slice {
            base: Box::new(DriverPlace::Ident("word".to_string())),
            range: DriverBitRange::new("(W) - 1", "((W) - 1) + (4) - 1"),
        };

        assert!(left.overlaps(&right));
        assert!(right.overlaps(&left));
    }

    #[test]
    fn object_identity_distinguishes_equal_display_names() {
        let first = DriverPlace::Object(DriverObject::new(ObjectId::new(1), "bus"));
        let second = DriverPlace::Object(DriverObject::new(ObjectId::new(2), "bus"));

        assert_eq!(first.display(), second.display());
        assert!(!first.overlaps(&second));
    }

    #[test]
    fn static_indexed_part_and_slice_can_be_disjoint() {
        let part = DriverPlace::IndexedPartSelect {
            base: Box::new(DriverPlace::Ident("word".to_string())),
            index: super::DriverExpr::Int(0),
            width: super::DriverBound::new("4"),
        };
        let slice = DriverPlace::Slice {
            base: Box::new(DriverPlace::Ident("word".to_string())),
            range: DriverBitRange::new("4", "7"),
        };

        assert!(!part.overlaps(&slice));
        assert!(!slice.overlaps(&part));
    }

    #[test]
    fn static_indexed_part_and_slice_overlap_when_ranges_intersect() {
        let part = DriverPlace::IndexedPartSelect {
            base: Box::new(DriverPlace::Ident("word".to_string())),
            index: super::DriverExpr::Int(1),
            width: super::DriverBound::new("4"),
        };
        let slice = DriverPlace::Slice {
            base: Box::new(DriverPlace::Ident("word".to_string())),
            range: DriverBitRange::new("6", "9"),
        };

        assert!(part.overlaps(&slice));
        assert!(slice.overlaps(&part));
    }
}
