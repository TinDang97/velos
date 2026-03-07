use velos_vehicle::emergency::{compute_yield_cone, should_yield, yield_speed_target, EmergencyState};
use std::f64::consts::PI;

#[test]
fn agent_30m_ahead_in_cone_should_yield() {
    // Emergency vehicle at origin heading east (0 radians)
    let cone = compute_yield_cone(0.0, 0.0, 0.0, 50.0);
    // Agent 30m directly ahead (east)
    assert!(should_yield(30.0, 0.0, &cone));
}

#[test]
fn agent_60m_ahead_should_not_yield_out_of_range() {
    let cone = compute_yield_cone(0.0, 0.0, 0.0, 50.0);
    // Agent 60m ahead -- outside 50m range
    assert!(!should_yield(60.0, 0.0, &cone));
}

#[test]
fn agent_30m_behind_should_not_yield() {
    let cone = compute_yield_cone(0.0, 0.0, 0.0, 50.0);
    // Agent 30m behind (west, opposite direction)
    assert!(!should_yield(-30.0, 0.0, &cone));
}

#[test]
fn agent_30m_perpendicular_should_not_yield() {
    let cone = compute_yield_cone(0.0, 0.0, 0.0, 50.0);
    // Agent 30m north (perpendicular) -- outside 45-degree half-angle cone
    assert!(!should_yield(0.0, 30.0, &cone));
}

#[test]
fn agent_exactly_at_50m_in_cone_should_yield() {
    let cone = compute_yield_cone(0.0, 0.0, 0.0, 50.0);
    // Agent exactly at 50m boundary, directly ahead
    assert!(should_yield(50.0, 0.0, &cone));
}

#[test]
fn agent_within_cone_angle_should_yield() {
    let cone = compute_yield_cone(0.0, 0.0, 0.0, 50.0);
    // Agent 30m ahead and 10m to the side -- within 45-degree half-angle
    // Angle = atan2(10, 30) ~ 18.4 degrees < 45 degrees
    assert!(should_yield(30.0, 10.0, &cone));
}

#[test]
fn agent_outside_cone_angle_should_not_yield() {
    let cone = compute_yield_cone(0.0, 0.0, 0.0, 50.0);
    // Agent 10m ahead and 30m to the side -- atan2(30, 10) ~ 71.6 degrees > 45 degrees
    assert!(!should_yield(10.0, 30.0, &cone));
}

#[test]
fn emergency_state_defaults() {
    let state = EmergencyState::default();
    assert!(!state.active);
    assert!((state.siren_range - 50.0).abs() < f64::EPSILON);
}

#[test]
fn yield_speed_target_is_1_4_mps() {
    let target = yield_speed_target();
    assert!((target - 1.4).abs() < f64::EPSILON, "yield speed should be 1.4 m/s (5 km/h)");
}

#[test]
fn cone_with_north_heading() {
    // Emergency heading north (PI/2 radians)
    let cone = compute_yield_cone(0.0, 0.0, PI / 2.0, 50.0);
    // Agent 30m north should yield
    assert!(should_yield(0.0, 30.0, &cone));
    // Agent 30m east should NOT yield (perpendicular)
    assert!(!should_yield(30.0, 0.0, &cone));
}

#[test]
fn cone_at_non_origin_position() {
    // Emergency at (100, 200) heading east
    let cone = compute_yield_cone(100.0, 200.0, 0.0, 50.0);
    // Agent 30m ahead at (130, 200)
    assert!(should_yield(130.0, 200.0, &cone));
    // Agent behind at (70, 200)
    assert!(!should_yield(70.0, 200.0, &cone));
}
