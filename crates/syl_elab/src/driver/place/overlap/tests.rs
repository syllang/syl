use super::DriverBitRange;
use super::{DriverPlace, DriverPlaceOverlap};

#[test]
fn unknown_slice_bounds_are_conservative() {
    let left = DriverPlace::Slice {
        base: Box::new(DriverPlace::test_object(0, "word")),
        range: DriverBitRange::new("0", "HI"),
    };
    let right = DriverPlace::Slice {
        base: Box::new(DriverPlace::test_object(0, "word")),
        range: DriverBitRange::new("4", "7"),
    };

    assert!(DriverPlaceOverlap::new(&left, &right).may_overlap());
}

#[test]
fn arithmetic_slice_bounds_resolve_before_overlap_check() {
    let computed_bit_zero = DriverPlace::Slice {
        base: Box::new(DriverPlace::test_object(0, "rsp")),
        range: DriverBitRange::new("0", "(0) + (1) - 1"),
    };
    let literal_bit_one = DriverPlace::Slice {
        base: Box::new(DriverPlace::test_object(0, "rsp")),
        range: DriverBitRange::new("1", "1"),
    };

    assert!(!computed_bit_zero.overlaps(&literal_bit_one));
}

#[test]
fn symbolic_high_with_known_low_can_be_disjoint() {
    let generic_upper_field = DriverPlace::Slice {
        base: Box::new(DriverPlace::test_object(0, "rsp")),
        range: DriverBitRange::new("(0) + (1)", "((0) + (1)) + (W) - 1"),
    };
    let literal_bit_zero = DriverPlace::Slice {
        base: Box::new(DriverPlace::test_object(0, "rsp")),
        range: DriverBitRange::new("0", "(0) + (1) - 1"),
    };

    assert!(!generic_upper_field.overlaps(&literal_bit_zero));
}

#[test]
fn adjacent_symbolic_field_slices_are_disjoint() {
    let lower_field = DriverPlace::Slice {
        base: Box::new(DriverPlace::test_object(0, "pair")),
        range: DriverBitRange::new("0", "(0) + (W) - 1"),
    };
    let upper_field = DriverPlace::Slice {
        base: Box::new(DriverPlace::test_object(0, "pair")),
        range: DriverBitRange::new("(0) + (W)", "((0) + (W)) + (W) - 1"),
    };

    assert!(!lower_field.overlaps(&upper_field));
    assert!(!upper_field.overlaps(&lower_field));
}

#[test]
fn intersecting_symbolic_field_slices_remain_conservative() {
    let left = DriverPlace::Slice {
        base: Box::new(DriverPlace::test_object(0, "word")),
        range: DriverBitRange::new("0", "(0) + (W) - 1"),
    };
    let right = DriverPlace::Slice {
        base: Box::new(DriverPlace::test_object(0, "word")),
        range: DriverBitRange::new("(W) - 1", "((W) - 1) + (4) - 1"),
    };

    assert!(left.overlaps(&right));
    assert!(right.overlaps(&left));
}

#[test]
fn object_identity_distinguishes_equal_display_names() {
    let first = DriverPlace::test_object(1, "bus");
    let second = DriverPlace::test_object(2, "bus");

    assert_eq!(first.display(), second.display());
    assert!(!first.overlaps(&second));
}

#[test]
fn static_indexed_part_and_slice_can_be_disjoint() {
    let part = DriverPlace::IndexedPartSelect {
        base: Box::new(DriverPlace::test_object(0, "word")),
        index: super::DriverExpr::Int(0),
        width: super::DriverBound::new("4"),
    };
    let slice = DriverPlace::Slice {
        base: Box::new(DriverPlace::test_object(0, "word")),
        range: DriverBitRange::new("4", "7"),
    };

    assert!(!part.overlaps(&slice));
    assert!(!slice.overlaps(&part));
}

#[test]
fn static_indexed_part_and_slice_overlap_when_ranges_intersect() {
    let part = DriverPlace::IndexedPartSelect {
        base: Box::new(DriverPlace::test_object(0, "word")),
        index: super::DriverExpr::Int(1),
        width: super::DriverBound::new("4"),
    };
    let slice = DriverPlace::Slice {
        base: Box::new(DriverPlace::test_object(0, "word")),
        range: DriverBitRange::new("6", "9"),
    };

    assert!(part.overlaps(&slice));
    assert!(slice.overlaps(&part));
}
