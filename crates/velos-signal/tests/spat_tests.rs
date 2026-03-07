//! Tests for SPaT broadcast and GLOSA speed computation.

use velos_signal::plan::PhaseState;
use velos_signal::spat::{broadcast_range_m, glosa_speed, SpatBroadcast};

#[test]
fn spat_broadcast_has_approach_states_and_timing() {
    let spat = SpatBroadcast {
        approach_states: vec![PhaseState::Green, PhaseState::Red],
        time_to_next_change: 15.0,
        cycle_time: 90.0,
    };
    assert_eq!(spat.approach_states.len(), 2);
    assert_eq!(spat.approach_states[0], PhaseState::Green);
    assert_eq!(spat.approach_states[1], PhaseState::Red);
    assert!((spat.time_to_next_change - 15.0).abs() < f64::EPSILON);
    assert!((spat.cycle_time - 90.0).abs() < f64::EPSILON);
}

#[test]
fn broadcast_range_is_200m() {
    assert!((broadcast_range_m() - 200.0).abs() < f64::EPSILON);
}

#[test]
fn glosa_returns_max_speed_when_already_green() {
    let result = glosa_speed(100.0, 0.0, 13.89);
    assert!((result - 13.89).abs() < f64::EPSILON);
}

#[test]
fn glosa_returns_zero_when_cannot_reach_in_time() {
    // distance = 200m, time_to_green = 5s -> required = 40 m/s > max_speed 13.89
    let result = glosa_speed(200.0, 5.0, 13.89);
    assert!((result - 0.0).abs() < f64::EPSILON);
}

#[test]
fn glosa_returns_required_speed_when_feasible() {
    // distance = 100m, time_to_green = 10s -> required = 10 m/s, within max 13.89
    let result = glosa_speed(100.0, 10.0, 13.89);
    assert!((result - 10.0).abs() < f64::EPSILON);
}

#[test]
fn glosa_returns_zero_when_too_slow() {
    // distance = 5m, time_to_green = 10s -> required = 0.5 m/s < 3.0
    let result = glosa_speed(5.0, 10.0, 13.89);
    assert!((result - 0.0).abs() < f64::EPSILON);
}
