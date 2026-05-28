pub(crate) use syl_hir::{
    MirBinaryOp, MirConstExpr, MirPattern, MirSelectMode, MirTypeRef, MirUnaryOp,
};

pub(crate) trait MirConstExprFacts {
    fn fact_key(&self) -> String;
}

pub(crate) trait MirTypeRefExt {
    fn view_shape(&self) -> Option<(&MirTypeRef, &str, Option<&MirConstExpr>)>;
}

impl MirTypeRefExt for MirTypeRef {
    fn view_shape(&self) -> Option<(&MirTypeRef, &str, Option<&MirConstExpr>)> {
        if let Some((base, view)) = self.view_select() {
            return Some((base, view, None));
        }
        let (len, elem) = self.array()?;
        let (base, view) = elem.view_select()?;
        Some((base, view, Some(len)))
    }
}

impl MirConstExprFacts for MirConstExpr {
    fn fact_key(&self) -> String {
        if let Some(name) = self.ident() {
            return name.to_string();
        }
        if let Some(value) = self.nat_value() {
            return value.to_string();
        }
        if let Some(value) = self.bool_value() {
            return value.to_string();
        }
        if let Some((op, expr)) = self.unary() {
            return format!("({}{})", <&'static str>::from(op), expr.fact_key());
        }
        if let Some((op, left, right)) = self.binary() {
            return format!(
                "({} {} {})",
                left.fact_key(),
                <&'static str>::from(op),
                right.fact_key()
            );
        }
        "unsupported_const_expr".to_string()
    }
}
