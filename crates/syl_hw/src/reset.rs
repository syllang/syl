/// Whether a register reset is sampled or asynchronous.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum HwResetMode {
    #[default]
    Sync,
    Async,
}

/// Logic level that asserts a hardware reset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum HwResetPolarity {
    #[default]
    ActiveHigh,
    ActiveLow,
}
