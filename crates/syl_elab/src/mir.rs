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
            return format!("({}{})", unary_symbol(op), expr.fact_key());
        }
        if let Some((op, left, right)) = self.binary() {
            return format!(
                "({} {} {})",
                left.fact_key(),
                binary_symbol(op),
                right.fact_key()
            );
        }
        "unsupported_const_expr".to_string()
    }
}

fn unary_symbol(op: MirUnaryOp) -> &'static str {
    match op {
        MirUnaryOp::Neg => "-",
        MirUnaryOp::Not => "!",
        MirUnaryOp::NotWord => "not",
        MirUnaryOp::Unsupported => "?",
        _ => "?",
    }
}

fn binary_symbol(op: MirBinaryOp) -> &'static str {
    match op {
        MirBinaryOp::Assign => "=",
        MirBinaryOp::OrOr => "||",
        MirBinaryOp::AndAnd => "&&",
        MirBinaryOp::Eq => "==",
        MirBinaryOp::NotEq => "!=",
        MirBinaryOp::Lt => "<",
        MirBinaryOp::LtEq => "<=",
        MirBinaryOp::Gt => ">",
        MirBinaryOp::GtEq => ">=",
        MirBinaryOp::Add => "+",
        MirBinaryOp::Sub => "-",
        MirBinaryOp::Mul => "*",
        MirBinaryOp::Div => "/",
        MirBinaryOp::Rem => "%",
        MirBinaryOp::Shl => "<<",
        MirBinaryOp::Field => ".",
        MirBinaryOp::BitAnd => "and",
        MirBinaryOp::BitOr => "or",
        MirBinaryOp::BitXor => "xor",
        MirBinaryOp::Unsupported => "?",
        _ => "?",
    }
}
