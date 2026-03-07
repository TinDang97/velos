//! Traffic sign ECS components and GPU buffer types.
//!
//! Provides `TrafficSign` as an ECS component for attaching signs to road
//! edges, `GpuSign` as a 16-byte GPU-friendly representation, and CPU
//! reference functions for sign interaction behavior.
//!
//! Sign types: SpeedLimit, Stop, Yield, NoTurn, SchoolZone.

/// Traffic sign type discriminant.
///
/// Numeric values match WGSL shader constants for GPU sign processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SignType {
    /// Posted speed limit sign (value = limit in m/s).
    SpeedLimit = 0,
    /// Stop sign (value = gap acceptance time in seconds, default 2.0).
    Stop = 1,
    /// Yield/give way sign (value unused).
    Yield = 2,
    /// No-turn restriction (enforced at pathfinding level, not in shader).
    NoTurn = 3,
    /// School zone with time window (value = reduced speed limit in m/s).
    SchoolZone = 4,
}

/// Traffic sign ECS component attached to road edges.
///
/// Each sign has a type, a float value (interpretation depends on type),
/// and a position on a specific edge. School zones have an optional
/// active time window.
#[derive(Debug, Clone)]
pub struct TrafficSign {
    /// Type of traffic sign.
    pub sign_type: SignType,
    /// Sign value (speed limit in m/s, gap accept time, etc.).
    pub value: f64,
    /// Edge ID where the sign is located.
    pub edge_id: u32,
    /// Offset along the edge in metres.
    pub offset_m: f64,
    /// Optional active time window (start_hour, end_hour) for school zones.
    pub time_window: Option<(f64, f64)>,
}

impl TrafficSign {
    /// Convert to GPU-friendly representation.
    pub fn to_gpu(&self) -> GpuSign {
        GpuSign {
            sign_type: self.sign_type as u32,
            value: self.value as f32,
            edge_id: self.edge_id,
            offset_m: self.offset_m as f32,
        }
    }
}

/// GPU-friendly traffic sign struct (16 bytes, Pod/Zeroable).
///
/// Matches the WGSL `GpuSign` struct layout for direct buffer upload.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct GpuSign {
    /// Sign type discriminant (matches `SignType` enum values).
    pub sign_type: u32,
    /// Sign value (speed limit in m/s, gap time, etc.).
    pub value: f32,
    /// Edge ID where the sign is located.
    pub edge_id: u32,
    /// Offset along the edge in metres.
    pub offset_m: f32,
}

// Safety: GpuSign is repr(C) with all-primitive fields, no padding.
unsafe impl bytemuck::Pod for GpuSign {}
unsafe impl bytemuck::Zeroable for GpuSign {}

/// Speed limit sign effect on desired speed.
///
/// Reduces `current_v0` to at most `posted_limit` when within 50m of the sign.
/// Beyond 50m the speed is unchanged.
///
/// # Arguments
/// * `current_v0` -- agent's current desired speed (m/s)
/// * `posted_limit` -- speed limit from the sign (m/s)
/// * `distance_to_sign` -- distance to the sign location (metres)
pub fn speed_limit_effect(current_v0: f64, posted_limit: f64, distance_to_sign: f64) -> f64 {
    if distance_to_sign <= 50.0 {
        current_v0.min(posted_limit)
    } else {
        current_v0
    }
}

/// Stop sign: should the agent come to a full stop?
///
/// Returns `true` if the agent is within 2m of the stop sign and still moving.
///
/// # Arguments
/// * `distance_to_sign` -- distance to the stop line (metres)
/// * `current_speed` -- agent's current speed (m/s)
pub fn stop_sign_should_stop(distance_to_sign: f64, current_speed: f64) -> bool {
    distance_to_sign <= 2.0 && current_speed > 0.1
}

/// School zone active check.
///
/// Returns `true` if the current simulation time falls within the school zone
/// time window (specified in hours).
///
/// # Arguments
/// * `sim_time` -- simulation time in seconds since midnight
/// * `start_h` -- window start in hours (e.g., 7.0 for 7:00 AM)
/// * `end_h` -- window end in hours (e.g., 8.0 for 8:00 AM)
pub fn school_zone_active(sim_time: f64, start_h: f64, end_h: f64) -> bool {
    let current_hour = sim_time / 3600.0;
    current_hour >= start_h && current_hour < end_h
}

/// Yield sign: should the agent stop?
///
/// Returns `true` if the agent is within 2m of the yield sign AND there is
/// conflicting traffic. Without conflicting traffic, the agent proceeds.
///
/// # Arguments
/// * `distance` -- distance to the yield point (metres)
/// * `has_conflicting_traffic` -- whether there is traffic to yield to
pub fn yield_sign_should_stop(distance: f64, has_conflicting_traffic: bool) -> bool {
    distance <= 2.0 && has_conflicting_traffic
}

/// Default school zone speed limit (20 km/h = 5.56 m/s).
pub const SCHOOL_ZONE_SPEED: f64 = 5.56;

/// Default stop sign gap acceptance time (seconds).
pub const STOP_SIGN_GAP_ACCEPT: f64 = 2.0;

#[cfg(test)]
mod tests {
    use super::*;
    use bytemuck::Zeroable;

    #[test]
    fn gpu_sign_size() {
        assert_eq!(std::mem::size_of::<GpuSign>(), 16);
    }

    #[test]
    fn gpu_sign_is_zeroed() {
        let z = GpuSign::zeroed();
        assert_eq!(z.sign_type, 0);
        assert_eq!(z.value, 0.0);
        assert_eq!(z.edge_id, 0);
        assert_eq!(z.offset_m, 0.0);
    }

    #[test]
    fn sign_type_round_trip() {
        let sign = TrafficSign {
            sign_type: SignType::SchoolZone,
            value: SCHOOL_ZONE_SPEED,
            edge_id: 99,
            offset_m: 50.0,
            time_window: Some((7.0, 16.0)),
        };
        let gpu = sign.to_gpu();
        assert_eq!(gpu.sign_type, 4); // SchoolZone
    }
}
