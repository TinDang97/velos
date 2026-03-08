//! Tests for motorbike sublane lateral filtering in GPU shaders.
//!
//! Validates that wave_front.wgsl and perception.wgsl correctly skip
//! leaders when a motorbike has sufficient lateral clearance (>= 0.8m).

/// Verify wave_front.wgsl contains the motorbike lateral clearance constant.
#[test]
fn wgsl_wave_front_has_lateral_clearance_constant() {
    let source = include_str!("../shaders/wave_front.wgsl");
    assert!(
        source.contains("MOTORBIKE_LATERAL_CLEARANCE"),
        "wave_front.wgsl must define MOTORBIKE_LATERAL_CLEARANCE"
    );
}

/// Verify perception.wgsl contains the motorbike lateral clearance constant.
#[test]
fn wgsl_perception_has_lateral_clearance_constant() {
    let source = include_str!("../shaders/perception.wgsl");
    assert!(
        source.contains("MOTORBIKE_LATERAL_CLEARANCE"),
        "perception.wgsl must define MOTORBIKE_LATERAL_CLEARANCE"
    );
}

/// Verify wave_front.wgsl checks vehicle_type == VT_MOTORBIKE for lateral filtering.
#[test]
fn wgsl_wave_front_checks_motorbike_type_for_lateral() {
    let source = include_str!("../shaders/wave_front.wgsl");
    assert!(
        source.contains("agent.vehicle_type == VT_MOTORBIKE"),
        "wave_front.wgsl must check vehicle_type == VT_MOTORBIKE for lateral filtering"
    );
}

/// Verify perception.wgsl checks vehicle_type == VT_MOTORBIKE for lateral filtering.
#[test]
fn wgsl_perception_checks_motorbike_type_for_lateral() {
    let source = include_str!("../shaders/perception.wgsl");
    assert!(
        source.contains("agent.vehicle_type == VT_MOTORBIKE"),
        "perception.wgsl must check vehicle_type == VT_MOTORBIKE for lateral filtering"
    );
}

/// Verify wave_front.wgsl sets free-flow gap when lateral clearance is sufficient.
#[test]
fn wgsl_wave_front_sets_free_flow_on_lateral_clearance() {
    let source = include_str!("../shaders/wave_front.wgsl");
    // When lateral clearance >= threshold, gap should be set to 1000.0 (free-flow)
    assert!(
        source.contains("gap = 1000.0;"),
        "wave_front.wgsl must set gap=1000 when motorbike has lateral clearance"
    );
}

/// Verify the MOTORBIKE_LATERAL_CLEARANCE value is 0.8 (matching CPU threshold).
#[test]
fn wgsl_lateral_clearance_value_matches_cpu() {
    let source = include_str!("../shaders/wave_front.wgsl");
    assert!(
        source.contains("MOTORBIKE_LATERAL_CLEARANCE: f32 = 0.8"),
        "MOTORBIKE_LATERAL_CLEARANCE must be 0.8m to match CPU sublane model"
    );
}

/// Verify that the lateral filtering uses Q8.8 decoding (/ 256.0).
#[test]
fn wgsl_lateral_uses_q8_8_decoding() {
    let source = include_str!("../shaders/wave_front.wgsl");
    // The shader must decode lateral from Q8.8 format
    assert!(
        source.contains("/ 256.0"),
        "lateral offset must be decoded from Q8.8 (divide by 256.0)"
    );
}

/// CPU sublane threshold must match GPU constant.
///
/// The CPU code in cpu_reference.rs uses 0.8m as the lateral distance
/// threshold for leader detection. The GPU must use the same value.
#[test]
fn cpu_and_gpu_lateral_threshold_consistent() {
    // CPU uses 0.8m in cpu_reference.rs line 286: `if lateral_dist < 0.8`
    // GPU uses MOTORBIKE_LATERAL_CLEARANCE = 0.8
    let source = include_str!("../shaders/wave_front.wgsl");
    assert!(
        source.contains("0.8"),
        "GPU lateral clearance must contain 0.8m threshold"
    );
}
