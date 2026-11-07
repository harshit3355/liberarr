use crate::summaries::InvalidSummary;

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Adapters map their own failures (SQL, HTTP, filesystem) into these variants; messages never carry content.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0} not found")]
    NotFound(String),
    #[error(transparent)]
    InvalidSummary(#[from] InvalidSummary),
    #[error("storage: {0}")]
    Storage(String),
    #[error("provider: {0}")]
    Provider(String),
}
