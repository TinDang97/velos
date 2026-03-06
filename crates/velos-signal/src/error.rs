//! Error types for the velos-signal crate.

use thiserror::Error;

/// Errors that can occur in signal controller operations.
#[derive(Debug, Error)]
pub enum SignalError {
    /// Signal plan has no phases.
    #[error("signal plan must have at least one phase")]
    EmptyPlan,

    /// Approach index is out of range.
    #[error("approach index {index} exceeds configured count {count}")]
    InvalidApproach {
        /// The requested approach index.
        index: usize,
        /// Total number of approaches.
        count: usize,
    },
}
