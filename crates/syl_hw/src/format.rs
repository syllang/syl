//! Typed, ordered message fragments shared by hardware backends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormatStyle {
    Unsigned,
    Signed,
    Bool,
    Hex {
        upper: bool,
        prefix: bool,
        digits: u16,
    },
    Binary {
        prefix: bool,
        digits: u16,
    },
    Debug,
    Enum {
        name: String,
        variants: Vec<(u64, String)>,
        debug: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormatPiece<E, W = String> {
    Literal(String),
    Value {
        expr: E,
        width: Option<W>,
        style: FormatStyle,
    },
}

impl<E, W> FormatPiece<E, W> {
    pub fn try_map<F, V, Err>(
        &self,
        expression: impl FnOnce(&E) -> Result<F, Err>,
        width_fn: impl FnOnce(&W) -> Result<V, Err>,
    ) -> Result<FormatPiece<F, V>, Err> {
        Ok(match self {
            Self::Literal(text) => FormatPiece::Literal(text.clone()),
            Self::Value { expr, width, style } => FormatPiece::Value {
                expr: expression(expr)?,
                width: width.as_ref().map(width_fn).transpose()?,
                style: style.clone(),
            },
        })
    }
}
