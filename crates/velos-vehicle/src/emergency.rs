//! Emergency vehicle priority behavior: yield cone detection and yield logic.
//!
//! CPU reference implementation for emergency vehicle interactions.
//! The GPU shader in `wave_front.wgsl` mirrors this logic for production dispatch.

use std::f64::consts::FRAC_PI_4;

/// Emergency vehicle state tracking.
#[derive(Debug, Clone, Copy)]
pub struct EmergencyState {
    /// Whether sirens/lights are active (triggers yield behavior in surrounding agents).
    pub active: bool,
    /// Detection range for the yield cone in meters (default 50m).
    pub siren_range: f64,
}

impl Default for EmergencyState {
    fn default() -> Self {
        Self {
            active: false,
            siren_range: 50.0,
        }
    }
}

/// A directional cone ahead of an emergency vehicle where surrounding agents must yield.
#[derive(Debug, Clone, Copy)]
pub struct YieldCone {
    /// X coordinate of the cone origin (emergency vehicle position).
    pub origin_x: f64,
    /// Y coordinate of the cone origin.
    pub origin_y: f64,
    /// X component of the cone direction (unit vector).
    pub direction_x: f64,
    /// Y component of the cone direction (unit vector).
    pub direction_y: f64,
    /// Maximum detection range in meters.
    pub range: f64,
    /// Half-angle of the cone in radians (pi/4 = 45 degrees for 90-degree total cone).
    pub half_angle: f64,
}

/// Compute the yield cone for an emergency vehicle.
///
/// # Arguments
/// * `pos_x`, `pos_y` - Emergency vehicle position
/// * `heading` - Heading angle in radians (0 = east, pi/2 = north)
/// * `range` - Detection range in meters
pub fn compute_yield_cone(pos_x: f64, pos_y: f64, heading: f64, range: f64) -> YieldCone {
    YieldCone {
        origin_x: pos_x,
        origin_y: pos_y,
        direction_x: heading.cos(),
        direction_y: heading.sin(),
        range,
        half_angle: FRAC_PI_4, // 45 degrees = 90-degree total cone
    }
}

/// Check whether an agent at the given position should yield to an emergency vehicle.
///
/// Returns `true` if the agent is within the cone range AND within the cone angle.
pub fn should_yield(agent_x: f64, agent_y: f64, cone: &YieldCone) -> bool {
    // Vector from cone origin to agent
    let dx = agent_x - cone.origin_x;
    let dy = agent_y - cone.origin_y;

    // Distance check
    let dist_sq = dx * dx + dy * dy;
    let range_sq = cone.range * cone.range;
    if dist_sq > range_sq {
        return false;
    }

    // Zero distance edge case (agent at same position as emergency)
    let dist = dist_sq.sqrt();
    if dist < 1e-9 {
        return false;
    }

    // Angle check: dot product of normalized agent vector with cone direction
    let dot = (dx * cone.direction_x + dy * cone.direction_y) / dist;
    // dot = cos(angle), so agent is in cone if angle <= half_angle
    // i.e., cos(angle) >= cos(half_angle)
    dot >= cone.half_angle.cos()
}

/// Target speed for yielding agents: 1.4 m/s (5 km/h).
pub fn yield_speed_target() -> f64 {
    1.4
}
