//! Canonical data model, SQLite-backed repository, and business services for Mangaread.

pub mod db;
pub mod domain;
pub mod error;
pub mod service;

// Re-export domain types at the crate root for convenience
pub use domain::*;
