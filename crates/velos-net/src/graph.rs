//! Road graph representation backed by petgraph DiGraph.
//!
//! Stores the directed road network with nodes at intersections (or way endpoints)
//! and edges representing road segments with lane counts, speed limits, and geometry.

use petgraph::graph::{DiGraph, NodeIndex};

/// Classification of road segments, matching OSM `highway` tag values
/// that are imported for the HCMC District 1 simulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RoadClass {
    Primary,
    Secondary,
    Tertiary,
    Residential,
}

/// A node in the road graph, representing an intersection or way endpoint.
#[derive(Debug, Clone)]
pub struct RoadNode {
    /// Position in local metres [x_east, y_north].
    pub pos: [f64; 2],
}

/// A directed edge in the road graph, representing one direction of a road segment.
#[derive(Debug, Clone)]
pub struct RoadEdge {
    /// Length of the edge in metres (Euclidean along geometry).
    pub length_m: f64,
    /// Speed limit in metres per second.
    pub speed_limit_mps: f64,
    /// Number of lanes in this direction.
    pub lane_count: u8,
    /// Whether the original OSM way was marked oneway.
    pub oneway: bool,
    /// Road classification.
    pub road_class: RoadClass,
    /// Polyline geometry in local metres, including start and end points.
    pub geometry: Vec<[f64; 2]>,
}

/// Wrapper around `petgraph::graph::DiGraph<RoadNode, RoadEdge>` providing
/// convenient accessors for the road network.
pub struct RoadGraph {
    inner: DiGraph<RoadNode, RoadEdge>,
}

impl RoadGraph {
    /// Create a new `RoadGraph` from an existing `DiGraph`.
    pub fn new(graph: DiGraph<RoadNode, RoadEdge>) -> Self {
        Self { inner: graph }
    }

    /// Number of nodes (intersections) in the graph.
    pub fn node_count(&self) -> usize {
        self.inner.node_count()
    }

    /// Number of directed edges in the graph.
    pub fn edge_count(&self) -> usize {
        self.inner.edge_count()
    }

    /// Borrow the underlying `DiGraph`.
    pub fn inner(&self) -> &DiGraph<RoadNode, RoadEdge> {
        &self.inner
    }

    /// Mutably borrow the underlying `DiGraph`.
    pub fn inner_mut(&mut self) -> &mut DiGraph<RoadNode, RoadEdge> {
        &mut self.inner
    }

    /// Get the position of a node in local metres.
    ///
    /// # Panics
    /// Panics if the node index is out of bounds.
    pub fn node_position(&self, idx: NodeIndex) -> [f64; 2] {
        self.inner[idx].pos
    }
}

impl std::fmt::Debug for RoadGraph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RoadGraph")
            .field("nodes", &self.node_count())
            .field("edges", &self.edge_count())
            .finish()
    }
}
