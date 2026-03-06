//! velos-net: Road graph, OSM import, spatial index, and routing for VELOS.
//!
//! This crate provides the foundational road network that all vehicle simulation
//! depends on. Agents need edges to drive on, spatial queries for neighbor
//! detection, and routing for path assignment.

pub mod error;
pub mod graph;
pub mod osm_import;
pub mod projection;
pub mod routing;
pub mod spatial;

pub use error::NetError;
pub use graph::{RoadClass, RoadEdge, RoadGraph, RoadNode};
pub use osm_import::import_osm;
pub use projection::EquirectangularProjection;
pub use routing::find_route;
pub use spatial::{AgentPoint, SpatialIndex};
