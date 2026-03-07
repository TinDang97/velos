//! SPaT (Signal Phase and Timing) broadcast and GLOSA advisory speed.
//!
//! Provides `SpatBroadcast` for current signal state communication and
//! `glosa_speed` for computing optimal approach speed to arrive on green.

use crate::plan::PhaseState;

/// SPaT broadcast data for one intersection.
///
/// Contains the current phase state for each approach and the time until
/// the next phase change. Broadcast to agents within `broadcast_range_m()`.
#[derive(Debug, Clone)]
pub struct SpatBroadcast {
    /// Current phase state per approach index.
    pub approach_states: Vec<PhaseState>,
    /// Seconds until the next phase transition.
    pub time_to_next_change: f64,
    /// Total cycle time in seconds.
    pub cycle_time: f64,
}

/// Broadcast range for SPaT messages (metres).
///
/// Agents within this distance of a signalised intersection receive
/// SPaT data for GLOSA advisory speed computation.
pub fn broadcast_range_m() -> f64 {
    200.0
}

/// Compute Green Light Optimal Speed Advisory (GLOSA).
///
/// Returns the optimal approach speed for an agent to arrive at a signal
/// just as it turns green, avoiding unnecessary stops.
///
/// # Arguments
/// * `distance_m` -- distance to the signal stop line (metres)
/// * `time_to_green_s` -- seconds until the signal turns green (0 if already green)
/// * `v_max` -- maximum permitted speed (m/s)
///
/// # Returns
/// * `v_max` if signal is already green (time_to_green == 0)
/// * `0.0` if required speed exceeds v_max (cannot make it)
/// * `0.0` if required speed is below 3.0 m/s (too slow to be practical)
/// * Otherwise the required constant speed to arrive exactly on green
pub fn glosa_speed(distance_m: f64, time_to_green_s: f64, v_max: f64) -> f64 {
    if time_to_green_s <= 0.0 {
        return v_max;
    }

    let required_speed = distance_m / time_to_green_s;

    if required_speed > v_max {
        // Cannot reach signal before green ends at max speed
        return 0.0;
    }

    if required_speed < 3.0 {
        // Too slow to be practical -- stop and wait
        return 0.0;
    }

    required_speed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glosa_already_green() {
        assert!((glosa_speed(100.0, 0.0, 13.89) - 13.89).abs() < f64::EPSILON);
    }

    #[test]
    fn glosa_negative_time_treated_as_green() {
        assert!((glosa_speed(100.0, -1.0, 13.89) - 13.89).abs() < f64::EPSILON);
    }

    #[test]
    fn glosa_feasible_speed() {
        let speed = glosa_speed(100.0, 10.0, 13.89);
        assert!((speed - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn glosa_too_fast() {
        assert!((glosa_speed(200.0, 5.0, 13.89) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn glosa_too_slow() {
        assert!((glosa_speed(5.0, 10.0, 13.89) - 0.0).abs() < f64::EPSILON);
    }
}
