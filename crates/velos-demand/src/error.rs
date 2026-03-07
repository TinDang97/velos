//! Error types for the velos-demand crate.

use thiserror::Error;

/// Errors that can occur in demand generation.
#[derive(Debug, Error)]
pub enum DemandError {
    /// Hour value is outside valid range [0.0, 24.0).
    #[error("invalid time: hour {hour} is outside valid range [0.0, 24.0)")]
    InvalidTime { hour: f64 },

    /// Zone identifier is not recognized.
    #[error("invalid zone: {name}")]
    InvalidZone { name: String },

    /// I/O error reading data files.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// CSV/GTFS parse error.
    #[error("parse error in {file} line {line}: {reason}")]
    Parse {
        file: String,
        line: usize,
        reason: String,
    },

    /// Required GTFS file is missing.
    #[error("missing required file: {path}")]
    MissingFile { path: String },
}
