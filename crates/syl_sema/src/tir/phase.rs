#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Phase {
    Const,
    Comb,
    Hardware,
}
