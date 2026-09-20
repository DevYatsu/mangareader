use thiserror::Error;

/// Crate-level error type.
#[derive(Error, Debug)]
pub enum Error {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Auth error: {0}")]
    Auth(String),
}
