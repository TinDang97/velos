//! Tests for traffic sign ECS components and GPU buffer types.

use velos_signal::signs::{
    school_zone_active, speed_limit_effect, stop_sign_should_stop, yield_sign_should_stop,
    GpuSign, SignType, TrafficSign,
};

#[test]
fn gpu_sign_is_16_bytes() {
    assert_eq!(std::mem::size_of::<GpuSign>(), 16);
}

#[test]
fn sign_type_enum_values() {
    assert_eq!(SignType::SpeedLimit as u32, 0);
    assert_eq!(SignType::Stop as u32, 1);
    assert_eq!(SignType::Yield as u32, 2);
    assert_eq!(SignType::NoTurn as u32, 3);
    assert_eq!(SignType::SchoolZone as u32, 4);
}

#[test]
fn traffic_sign_construction() {
    let sign = TrafficSign {
        sign_type: SignType::SpeedLimit,
        value: 11.11, // 40 km/h
        edge_id: 42,
        offset_m: 100.0,
        time_window: None,
    };
    assert_eq!(sign.edge_id, 42);
    assert!((sign.value - 11.11).abs() < f64::EPSILON);
}

#[test]
fn traffic_sign_to_gpu() {
    let sign = TrafficSign {
        sign_type: SignType::SpeedLimit,
        value: 11.11,
        edge_id: 42,
        offset_m: 100.0,
        time_window: None,
    };
    let gpu = sign.to_gpu();
    assert_eq!(gpu.sign_type, 0); // SpeedLimit
    assert!((gpu.value - 11.11_f32).abs() < 0.01);
    assert_eq!(gpu.edge_id, 42);
    assert!((gpu.offset_m - 100.0_f32).abs() < 0.01);
}

#[test]
fn speed_limit_reduces_v0_within_50m() {
    let result = speed_limit_effect(13.89, 11.11, 30.0);
    assert!((result - 11.11).abs() < f64::EPSILON);
}

#[test]
fn speed_limit_unchanged_beyond_50m() {
    let result = speed_limit_effect(13.89, 11.11, 80.0);
    assert!((result - 13.89).abs() < f64::EPSILON);
}

#[test]
fn speed_limit_at_boundary_50m() {
    let result = speed_limit_effect(13.89, 11.11, 50.0);
    assert!((result - 11.11).abs() < f64::EPSILON);
}

#[test]
fn speed_limit_does_not_increase_speed() {
    // Current speed lower than limit -- stays at current
    let result = speed_limit_effect(8.0, 11.11, 30.0);
    assert!((result - 8.0).abs() < f64::EPSILON);
}

#[test]
fn stop_sign_should_stop_close_range() {
    assert!(stop_sign_should_stop(1.5, 5.0));
}

#[test]
fn stop_sign_should_not_stop_far_away() {
    assert!(!stop_sign_should_stop(10.0, 5.0));
}

#[test]
fn stop_sign_should_not_stop_when_already_stopped() {
    assert!(!stop_sign_should_stop(1.0, 0.05));
}

#[test]
fn school_zone_active_within_window() {
    // 7:00 to 8:00 (in hours), sim_time = 7.5h = 27000s
    assert!(school_zone_active(27000.0, 7.0, 8.0));
}

#[test]
fn school_zone_inactive_outside_window() {
    // 7:00 to 8:00 (in hours), sim_time = 10h = 36000s
    assert!(!school_zone_active(36000.0, 7.0, 8.0));
}

#[test]
fn school_zone_active_at_boundary_start() {
    // Exactly at start = 7:00 = 25200s
    assert!(school_zone_active(25200.0, 7.0, 8.0));
}

#[test]
fn yield_sign_should_stop_with_conflicting_traffic() {
    assert!(yield_sign_should_stop(1.5, true));
}

#[test]
fn yield_sign_should_not_stop_without_conflicting_traffic() {
    assert!(!yield_sign_should_stop(1.5, false));
}

#[test]
fn yield_sign_should_not_stop_far_away() {
    assert!(!yield_sign_should_stop(10.0, true));
}
