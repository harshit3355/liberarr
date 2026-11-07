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
    /// An input the app refuses by design; the message is shown to the owner as is.
    #[error("{0}")]
    Unsupported(String),
    #[error("device: {0}")]
    Device(String),
}
