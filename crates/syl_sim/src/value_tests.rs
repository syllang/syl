use super::*;
fn v(bits: &str) -> Value {
    bits.parse().unwrap()
}
#[test]
fn controlling_values_and_unknown_mux_bits() {
    let a = v("0zx1");
    let b = v("10z1");
    assert_eq!(a.binary(Op::BitAnd, &b).unwrap(), v("00x1"));
    assert_eq!(a.binary(Op::BitOr, &b).unwrap(), v("1xx1"));
    assert_eq!(a.binary(Op::BitXor, &b).unwrap(), v("1xx0"));
    assert_eq!(Value::mux(Logic::X, &v("zz01"), &v("z101")), v("zx01"));
    assert_eq!(a.binary(Op::Eq, &b).unwrap(), v("0"));
    assert_eq!(a.binary(Op::Eq, &a).unwrap(), v("x"));
    assert_eq!(v("0z0").logical_not(), v("x"));
    assert_eq!(a.logical_not(), v("0"));
    assert_eq!(v("zzzz1010xxxx").to_hex(), "zax");
    assert!(a.has_x() && a.has_z());
    assert_eq!(Value::concat(&[v("zz"), v("01")]).unwrap(), v("zz01"));
}
#[test]
fn wide_unsigned_arithmetic_and_dirty_high_bits() {
    let width = 137;
    let one = Value::from_u64(width, 1);
    let left = one.binary(Op::Shl, &Value::from_u64(8, 96)).unwrap();
    let n = left.binary(Op::Add, &one).unwrap();
    assert_eq!(n.slice(96, 1), v("1"));
    assert_eq!(n.binary(Op::Sub, &one).unwrap(), left);
    let three = Value::from_u64(width, 3);
    let product = n.binary(Op::Mul, &three).unwrap();
    assert_eq!(product.binary(Op::Div, &three).unwrap(), n);
    assert_eq!(product.binary(Op::Rem, &three).unwrap(), Value::zero(width));
    assert_eq!(
        Value::zero(width).binary(Op::Sub, &one).unwrap().to_hex(),
        format!("1{}", "f".repeat(34))
    );
    assert_eq!(n.binary(Op::Gt, &one).unwrap(), v("1"));
    assert_eq!(n.binary(Op::Div, &Value::zero(1)).unwrap(), Value::x(width));
    assert_eq!(n.binary(Op::Add, &Value::z(1)).unwrap(), Value::x(width));
    assert_eq!(left.slice(136, 2), Value::x(2));
    assert_eq!(v("z1").resized(65).slice(2, 63), Value::zero(63));
}
