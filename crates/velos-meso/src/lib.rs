//! velos-meso: Mesoscopic queue model with BPR travel time and graduated buffer zone transitions.
//!
//! Provides CPU-only mesoscopic simulation for peripheral network zones using BPR-based
//! spatial queues. Includes a 100m graduated buffer zone for smooth meso-micro transitions
//! with C1-continuous IDM parameter interpolation.

pub mod buffer_zone;
pub mod queue_model;
pub mod zone_config;

use thiserror::Error;

/// Errors produced by the mesoscopic simulation module.
#[derive(Debug, Error)]
pub enum MesoError {
    /// Failed to parse zone configuration file.
    #[error("zone config parse error: {0}")]
    ZoneConfigParse(String),

    /// I/O error reading zone configuration.
    #[error("zone config I/O error: {0}")]
    ZoneConfigIo(#[from] std::io::Error),
}
