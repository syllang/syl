/// Reset sampling mode retained from syntax.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HirResetMode {
    Sync,
    Async,
}
impl From<syl_syntax::ResetMode> for HirResetMode {
    fn from(mode: syl_syntax::ResetMode) -> Self {
        match mode {
            syl_syntax::ResetMode::Async => Self::Async,
            _ => Self::Sync,
        }
    }
}
/// Reset assertion polarity retained from syntax.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HirResetPolarity {
    ActiveHigh,
    ActiveLow,
}
impl From<syl_syntax::ResetPolarity> for HirResetPolarity {
    fn from(polarity: syl_syntax::ResetPolarity) -> Self {
        match polarity {
            syl_syntax::ResetPolarity::ActiveLow => Self::ActiveLow,
            _ => Self::ActiveHigh,
        }
    }
}
