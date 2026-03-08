//! Tests for GpuVehicleParams 12-float extension.
//!
//! Validates Rust struct layout, field mapping from VehicleConfig,
//! and alignment with WGSL VehicleTypeParams.

use velos_gpu::compute::GpuVehicleParams;
use velos_vehicle::config::VehicleConfig;

#[test]
fn gpu_vehicle_params_struct_size_is_336_bytes() {
    // 12 floats * 4 bytes * 7 vehicle types = 336 bytes
    assert_eq!(
        std::mem::size_of::<GpuVehicleParams>(),
        12 * 4 * 7,
        "GpuVehicleParams must be 12 floats x 7 types = 336 bytes"
    );
}

#[test]
fn motorbike_row_has_creep_fields() {
    let config = VehicleConfig::default();
    let gpu = GpuVehicleParams::from_config(&config);
    let row = &gpu.params[0]; // Motorbike

    // Indices 8-11: creep_max_speed, creep_distance_scale, creep_min_distance, gap_acceptance_ttc
    assert!(
        (row[8] - 0.3).abs() < 1e-6,
        "motorbike creep_max_speed should be 0.3, got {}",
        row[8]
    );
    assert!(
        (row[9] - 5.0).abs() < 1e-6,
        "motorbike creep_distance_scale should be 5.0, got {}",
        row[9]
    );
    assert!(
        (row[10] - 0.5).abs() < 1e-6,
        "motorbike creep_min_distance should be 0.5, got {}",
        row[10]
    );
    assert!(
        (row[11] - 1.0).abs() < 1e-6,
        "motorbike gap_acceptance_ttc should be 1.0, got {}",
        row[11]
    );
}

#[test]
fn car_row_has_zero_creep_and_correct_gap_ttc() {
    let config = VehicleConfig::default();
    let gpu = GpuVehicleParams::from_config(&config);
    let row = &gpu.params[1]; // Car

    assert!(
        (row[8] - 0.0).abs() < 1e-6,
        "car creep_max_speed should be 0.0, got {}",
        row[8]
    );
    assert!(
        (row[11] - 1.5).abs() < 1e-6,
        "car gap_acceptance_ttc should be 1.5, got {}",
        row[11]
    );
}

#[test]
fn bicycle_row_has_creep_fields() {
    let config = VehicleConfig::default();
    let gpu = GpuVehicleParams::from_config(&config);
    let row = &gpu.params[3]; // Bicycle

    assert!(
        (row[8] - 0.2).abs() < 1e-6,
        "bicycle creep_max_speed should be 0.2, got {}",
        row[8]
    );
    assert!(
        (row[11] - 1.2).abs() < 1e-6,
        "bicycle gap_acceptance_ttc should be 1.2, got {}",
        row[11]
    );
}

#[test]
fn pedestrian_row_has_gap_ttc_and_zero_creep() {
    let config = VehicleConfig::default();
    let gpu = GpuVehicleParams::from_config(&config);
    let row = &gpu.params[6]; // Pedestrian

    assert!(
        (row[8] - 0.0).abs() < 1e-6,
        "pedestrian creep_max_speed should be 0.0, got {}",
        row[8]
    );
    assert!(
        (row[11] - 2.0).abs() < 1e-6,
        "pedestrian gap_acceptance_ttc should be 2.0, got {}",
        row[11]
    );
}

#[test]
fn first_8_fields_unchanged() {
    let config = VehicleConfig::default();
    let gpu = GpuVehicleParams::from_config(&config);
    let row = &gpu.params[0]; // Motorbike

    // Existing 8 fields: v0, s0, t_headway, a, b, krauss_accel, krauss_decel, krauss_sigma
    assert!((row[0] - 11.1).abs() < 1e-5, "v0");
    assert!((row[1] - 1.0).abs() < 1e-5, "s0");
    assert!((row[2] - 0.8).abs() < 1e-5, "t_headway");
    assert!((row[3] - 2.0).abs() < 1e-5, "a");
    assert!((row[4] - 3.0).abs() < 1e-5, "b");
    assert!((row[5] - 2.0).abs() < 1e-5, "krauss_accel");
    assert!((row[6] - 3.0).abs() < 1e-5, "krauss_decel");
    assert!((row[7] - 0.3).abs() < 1e-5, "krauss_sigma");
}

#[test]
fn wgsl_has_12_field_vehicle_type_params() {
    let shader_source = include_str!("../shaders/wave_front.wgsl");
    for field in &[
        "creep_max_speed",
        "creep_distance_scale",
        "creep_min_distance",
        "gap_acceptance_ttc",
    ] {
        assert!(
            shader_source.contains(field),
            "WGSL VehicleTypeParams must contain field: {field}"
        );
    }
}

#[test]
fn wgsl_no_hardcoded_creep_gap_constants() {
    let shader_source = include_str!("../shaders/wave_front.wgsl");

    let forbidden = [
        "CREEP_MAX_SPEED",
        "CREEP_DISTANCE_SCALE",
        "CREEP_MIN_DISTANCE",
        "GAP_MAX_WAIT_TIME",
        "GAP_FORCED_ACCEPTANCE_FACTOR",
        "GAP_WAIT_REDUCTION_RATE",
    ];

    for constant in &forbidden {
        // Check for `const X:` declarations -- these should not exist
        let pattern = format!("const {constant}");
        assert!(
            !shader_source.contains(&pattern),
            "WGSL must NOT contain hardcoded constant: {constant}"
        );
    }
}

#[test]
fn wgsl_creep_reads_from_uniform_buffer() {
    let shader_source = include_str!("../shaders/wave_front.wgsl");

    // red_light_creep_speed should read from vehicle_params, not hardcoded constants
    assert!(
        shader_source.contains("vtp.creep_max_speed"),
        "red_light_creep_speed must read creep_max_speed from uniform buffer"
    );
    assert!(
        shader_source.contains("vtp.creep_distance_scale"),
        "red_light_creep_speed must read creep_distance_scale from uniform buffer"
    );
    assert!(
        shader_source.contains("vtp.creep_min_distance"),
        "red_light_creep_speed must read creep_min_distance from uniform buffer"
    );
}

#[test]
fn wgsl_gap_acceptance_reads_ttc_from_buffer() {
    let shader_source = include_str!("../shaders/wave_front.wgsl");
    // The gap acceptance call should use gap_acceptance_ttc from the vehicle params
    assert!(
        shader_source.contains("gap_acceptance_ttc"),
        "intersection_gap_acceptance must use gap_acceptance_ttc from uniform buffer"
    );
}

#[test]
fn all_vehicle_types_have_12_floats() {
    let config = VehicleConfig::default();
    let gpu = GpuVehicleParams::from_config(&config);

    for (i, row) in gpu.params.iter().enumerate() {
        assert_eq!(
            row.len(),
            12,
            "Vehicle type {i} must have 12 floats, got {}",
            row.len()
        );
    }
}
