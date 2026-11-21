/// Register reset sampling mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResetMode {
    #[default]
    Sync,
    Async,
}

/// Logic level that asserts a reset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResetPolarity {
    #[default]
    ActiveHigh,
    ActiveLow,
}
