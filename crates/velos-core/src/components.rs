//! ECS component definitions for agent state.
//! All fields use f64 for CPU-side precision.

/// World-space position of an agent in metres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    /// East-west coordinate (metres).
    pub x: f64,
    /// North-south coordinate (metres).
    pub y: f64,
}

/// Kinematic state of an agent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Kinematics {
    /// Velocity in the x direction (m/s).
    pub vx: f64,
    /// Velocity in the y direction (m/s).
    pub vy: f64,
    /// Scalar speed magnitude (m/s).
    pub speed: f64,
    /// Heading angle in radians (0 = east, CCW positive).
    pub heading: f64,
}

/// Vehicle type tag for an agent in the ECS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VehicleType {
    Motorbike,
    Car,
    Pedestrian,
}

/// Agent's position along a road edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoadPosition {
    /// Index of the current edge in the road graph.
    pub edge_index: u32,
    /// Current lane (0-based from right).
    pub lane: u8,
    /// Distance along edge from start node (metres).
    pub offset_m: f64,
}

/// Agent's assigned route as a sequence of node indices.
#[derive(Debug, Clone)]
pub struct Route {
    /// Sequence of node indices forming the path.
    pub path: Vec<u32>,
    /// Current index into `path` (the node we are heading toward).
    pub current_step: usize,
}

/// Lateral offset for motorbike sublane positioning.
///
/// Only attached to motorbike agents. Tracks continuous lateral position
/// across the road width (measured from right edge in metres).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LateralOffset {
    /// Current lateral offset from road right edge (metres).
    pub lateral_offset: f64,
    /// Target lateral position from gap-seeking or swarming (metres).
    pub desired_lateral: f64,
}

/// Agent timing state for gridlock detection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaitState {
    /// Simulation time when speed first hit zero.
    pub stopped_since: f64,
    /// True if the agent is waiting at a red signal (not gridlock).
    pub at_red_signal: bool,
}
