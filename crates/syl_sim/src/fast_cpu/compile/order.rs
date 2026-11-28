use super::{B, Builder, E, HwUnaryOp, Op, Result, Selector, View};

impl Builder<'_> {
    // Inverting the sign bit before an unsigned comparison orders packed words
    // as signed. Recover the original slices so the high limb can be compared
    // directly without materializing either concatenation.
    pub(super) fn sign_flipped_operand<'a>(&self, expr: &'a E, width: u32) -> Option<&'a E> {
        let E::Concat(parts) = expr else { return None };
        let [
            E::Unary {
                op: HwUnaryOp::Not,
                expr: sign,
            },
            E::Project {
                value,
                selector: Selector::Slice { high, low: 0 },
            },
        ] = parts.as_slice()
        else {
            return None;
        };
        let E::Project {
            value: base,
            selector: Selector::Slice { high: msb, low },
        } = sign.as_ref()
        else {
            return None;
        };
        (*msb == width - 1
            && low == msb
            && *high == width - 2
            && base == value
            && self.width(value).ok() == Some(width))
        .then_some(value)
    }
    pub(super) fn signed_word_binary(
        &mut self,
        op: B,
        a: View,
        b: View,
        width: u32,
    ) -> Result<View> {
        let a = self.word(a)?;
        let b = self.word(b)?;
        let dst = self.alloc(if matches!(op, B::Div | B::Rem) {
            width
        } else {
            1
        });
        self.ops.push(Op::SignedBinary {
            dst: dst.slot,
            a,
            b,
            op,
            width,
        });
        Ok(dst)
    }

    // A more significant unequal limb decides the result. Inclusive comparisons
    // need their equality case only at the least significant limb.
    pub(super) fn wide_order(
        &mut self,
        op: B,
        (a, b): (View, View),
        bits: u32,
        signed: bool,
    ) -> Result<View> {
        let strict = match op {
            B::Lt | B::LtEq => B::Lt,
            B::Gt | B::GtEq => B::Gt,
            _ => unreachable!("ordered comparison"),
        };
        let mut result = self.zero;
        let mut prefix_equal = self.const_word(1, 1);
        for i in (0..bits.div_ceil(64)).rev() {
            let low = i * 64;
            let w = (bits - low).min(64);
            let (x, y) = (a.limb(low, w, self.zero), b.limb(low, w, self.zero));
            let predicate = if i == 0 { op } else { strict };
            let top = i == (bits - 1) / 64;
            let ordered = if signed && top {
                self.signed_word_binary(predicate, x, y, w)?
            } else {
                self.binary(predicate, x, y, 1)?
            };
            result = if top {
                ordered
            } else {
                self.mux(prefix_equal, ordered, result, 1)?
            };
            if i != 0 {
                let equal = self.binary(B::Eq, x, y, 1)?;
                prefix_equal = if top {
                    equal
                } else {
                    self.binary(B::BitAnd, prefix_equal, equal, 1)?
                };
            }
        }
        Ok(result)
    }
}
