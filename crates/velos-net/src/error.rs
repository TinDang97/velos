//! Error types for the velos-net crate.

use thiserror::Error;

/// Errors that can occur in the road network subsystem.
#[derive(Debug, Error)]
pub enum NetError {
    /// I/O error reading files.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Error parsing OSM PBF data.
    #[error("OSM parse error: {0}")]
    OsmParse(String),

    /// No path found between two nodes.
    #[error("no path found from {from} to {to}")]
    NoPathFound { from: u32, to: u32 },
}
