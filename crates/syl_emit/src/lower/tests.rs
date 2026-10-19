use super::SvEmitter;

#[test]
fn normalizes_bool_param_literals() {
    assert_eq!(SvEmitter::normalize_param_value("true"), "1'b1");
    assert_eq!(SvEmitter::normalize_param_value(" false "), "1'b0");
    assert_eq!(SvEmitter::normalize_param_value("WIDTH"), "WIDTH");
}
