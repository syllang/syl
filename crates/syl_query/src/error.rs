use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum QueryError {
    #[error("analysis query was cancelled")]
    Cancelled,
}
