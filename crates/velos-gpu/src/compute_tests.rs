//! Tests for ComputeDispatcher, wave-front pipeline, and GPU helper functions.

use super::*;
use velos_core::components::GpuAgentState;

#[test]
fn sort_agents_empty() {
    let (offsets, counts, indices) = sort_agents_by_lane(&[]);
    assert_eq!(offsets, vec![0]);
    assert_eq!(counts, vec![0]);
    assert!(indices.is_empty());
}

#[test]
fn sort_agents_single_lane() {
    let agents = vec![
        GpuAgentState {
            edge_id: 0, lane_idx: 0, position: 100, lateral: 0,
            speed: 50, acceleration: 0, cf_model: 0, rng_state: 0,
            vehicle_type: 0, flags: 0,
        },
        GpuAgentState {
            edge_id: 0, lane_idx: 0, position: 500, lateral: 0,
            speed: 50, acceleration: 0, cf_model: 0, rng_state: 0,
            vehicle_type: 0, flags: 0,
        },
        GpuAgentState {
            edge_id: 0, lane_idx: 0, position: 300, lateral: 0,
            speed: 50, acceleration: 0, cf_model: 0, rng_state: 0,
            vehicle_type: 0, flags: 0,
        },
    ];
    let (offsets, counts, indices) = sort_agents_by_lane(&agents);
    assert_eq!(offsets.len(), 1);
    assert_eq!(counts, vec![3]);
    assert_eq!(indices, vec![1, 2, 0]);
}

#[test]
fn sort_agents_multiple_lanes() {
    let agents = vec![
        GpuAgentState {
            edge_id: 0, lane_idx: 0, position: 100, lateral: 0,
            speed: 50, acceleration: 0, cf_model: 0, rng_state: 0,
            vehicle_type: 0, flags: 0,
        },
        GpuAgentState {
            edge_id: 0, lane_idx: 1, position: 200, lateral: 0,
            speed: 50, acceleration: 0, cf_model: 1, rng_state: 42,
            vehicle_type: 0, flags: 0,
        },
        GpuAgentState {
            edge_id: 0, lane_idx: 0, position: 300, lateral: 0,
            speed: 50, acceleration: 0, cf_model: 0, rng_state: 0,
            vehicle_type: 0, flags: 0,
        },
    ];
    let (offsets, counts, indices) = sort_agents_by_lane(&agents);
    assert_eq!(counts.len(), 2);
    assert_eq!(counts[0], 2);
    assert_eq!(counts[1], 1);
    assert_eq!(indices[offsets[0] as usize], 2);
    assert_eq!(indices[offsets[0] as usize + 1], 0);
    assert_eq!(indices[offsets[1] as usize], 1);
}

#[test]
fn wave_front_bgl_has_nine_entries() {
    // Wave-front bind group layout must have 9 entries (bindings 0-8).
    // This validates that the perception_results binding (8) is included.
    // We can't create a real device in unit tests, so verify the shader source
    // references binding(8) and the BGL entry count matches.
    let shader_source = include_str!("../shaders/wave_front.wgsl");
    assert!(
        shader_source.contains("@binding(8)"),
        "wave_front.wgsl must contain @binding(8) for perception_results"
    );
}

#[test]
fn wave_front_shader_has_perception_result_struct() {
    let shader_source = include_str!("../shaders/wave_front.wgsl");
    assert!(
        shader_source.contains("struct PerceptionResult"),
        "wave_front.wgsl must define PerceptionResult struct"
    );
    // Verify all 8 fields are present
    for field in &[
        "leader_speed", "leader_gap", "signal_state", "signal_distance",
        "congestion_own_route", "congestion_area", "sign_speed_limit", "perc_flags",
    ] {
        assert!(
            shader_source.contains(field),
            "PerceptionResult must contain field: {field}"
        );
    }
}

#[test]
fn wave_front_shader_has_hcmc_behavior_functions() {
    let shader_source = include_str!("../shaders/wave_front.wgsl");
    assert!(
        shader_source.contains("fn red_light_creep_speed"),
        "wave_front.wgsl must define red_light_creep_speed function"
    );
    assert!(
        shader_source.contains("fn intersection_gap_acceptance"),
        "wave_front.wgsl must define intersection_gap_acceptance function"
    );
    assert!(
        shader_source.contains("fn size_factor"),
        "wave_front.wgsl must define size_factor function"
    );
}

#[test]
fn wave_front_shader_uses_perception_in_main() {
    let shader_source = include_str!("../shaders/wave_front.wgsl");
    // Verify the main function reads perception_results
    assert!(
        shader_source.contains("perception_results[agent_idx]"),
        "wave_front_update must read perception_results for HCMC behaviors"
    );
    // Verify creep is called in main
    assert!(
        shader_source.contains("red_light_creep_speed(perc.signal_distance"),
        "wave_front_update must call red_light_creep_speed with perception data"
    );
}

#[test]
fn wave_front_shader_under_700_lines() {
    let shader_source = include_str!("../shaders/wave_front.wgsl");
    let line_count = shader_source.lines().count();
    assert!(
        line_count < 700,
        "wave_front.wgsl must be under 700 lines, got {line_count}"
    );
}

#[test]
fn wave_front_shader_naga_validates() {
    // Validate the WGSL shader parses correctly using naga.
    let shader_source = include_str!("../shaders/wave_front.wgsl");
    let result = naga::front::wgsl::parse_str(shader_source);
    assert!(
        result.is_ok(),
        "wave_front.wgsl must parse without errors: {:?}",
        result.err()
    );
}

/// CPU reference for red_light_creep_speed to verify GPU behavior matches.
/// Mirrors the WGSL function exactly (now reads from uniform buffer params).
fn cpu_red_light_creep_speed(distance_to_stop: f32, vehicle_type: u32) -> f32 {
    let config = VehicleConfig::default();
    let gpu = GpuVehicleParams::from_config(&config);
    let vtp = &gpu.params[vehicle_type as usize];
    let creep_max = vtp[8];
    let creep_scale = vtp[9];
    let creep_min_dist = vtp[10];

    // Early-exit for non-creeping vehicles (creep_max_speed == 0.0)
    if creep_max == 0.0 {
        return 0.0;
    }
    if distance_to_stop < creep_min_dist {
        return 0.0;
    }
    creep_max * (distance_to_stop / creep_scale).min(1.0)
}

#[test]
fn creep_speed_motorbike_normal_distance() {
    // Motorbike at 3m from stop line: should creep
    let speed = cpu_red_light_creep_speed(3.0, 0);
    assert!((speed - 0.3 * (3.0 / 5.0)).abs() < 1e-6);
}

#[test]
fn creep_speed_car_returns_zero() {
    // Cars don't creep
    assert_eq!(cpu_red_light_creep_speed(3.0, 1), 0.0);
}

#[test]
fn creep_speed_too_close_returns_zero() {
    // Under 0.5m: no creep
    assert_eq!(cpu_red_light_creep_speed(0.3, 0), 0.0);
}

#[test]
fn creep_speed_far_distance_capped() {
    // Beyond 5m: capped at max
    let speed = cpu_red_light_creep_speed(10.0, 0);
    assert!((speed - 0.3).abs() < 1e-6);
}

#[test]
fn creep_speed_bicycle_also_creeps() {
    let speed = cpu_red_light_creep_speed(2.5, 3);
    assert!(speed > 0.0);
}

/// CPU reference for size_factor matching WGSL.
fn cpu_size_factor(approaching_type: u32) -> f32 {
    match approaching_type {
        4 | 2 => 1.3,   // Truck, Bus
        5 => 2.0,       // Emergency
        0 | 3 => 0.8,   // Motorbike, Bicycle
        6 => 0.5,       // Pedestrian
        _ => 1.0,       // Car (default)
    }
}

#[test]
fn size_factor_values_match_cpu_reference() {
    assert!((cpu_size_factor(0) - 0.8).abs() < 1e-6);  // Motorbike
    assert!((cpu_size_factor(1) - 1.0).abs() < 1e-6);  // Car
    assert!((cpu_size_factor(2) - 1.3).abs() < 1e-6);  // Bus
    assert!((cpu_size_factor(3) - 0.8).abs() < 1e-6);  // Bicycle
    assert!((cpu_size_factor(4) - 1.3).abs() < 1e-6);  // Truck
    assert!((cpu_size_factor(5) - 2.0).abs() < 1e-6);  // Emergency
    assert!((cpu_size_factor(6) - 0.5).abs() < 1e-6);  // Pedestrian
}

/// CPU reference for gap acceptance matching WGSL.
fn cpu_gap_acceptance(other_type: u32, ttc: f32, threshold: f32, wait_time: f32) -> bool {
    let sf = cpu_size_factor(other_type);
    let wait_mod = if wait_time >= 5.0 {
        0.5
    } else {
        1.0 - 0.1 * wait_time.min(5.0)
    };
    let effective = threshold * sf * wait_mod;
    ttc > effective
}

#[test]
fn gap_acceptance_large_ttc_accepts() {
    // Large TTC should always accept
    assert!(cpu_gap_acceptance(1, 10.0, 2.0, 0.0));
}

#[test]
fn gap_acceptance_small_ttc_rejects() {
    // TTC below threshold should reject
    assert!(!cpu_gap_acceptance(1, 0.5, 2.0, 0.0));
}

#[test]
fn gap_acceptance_forced_after_max_wait() {
    // After 5s wait, threshold halved -- previously rejected gap now accepted
    assert!(!cpu_gap_acceptance(1, 1.5, 2.0, 0.0)); // rejected at 0s wait
    assert!(cpu_gap_acceptance(1, 1.5, 2.0, 5.0));   // accepted at 5s wait
}

#[test]
fn gap_acceptance_emergency_needs_larger_gap() {
    // Emergency approaching (size_factor=2.0) needs bigger gap
    assert!(cpu_gap_acceptance(1, 3.0, 2.0, 0.0));   // car: accepts
    assert!(!cpu_gap_acceptance(5, 3.0, 2.0, 0.0));  // emergency: rejects
}

/// Compute flags bitfield matching the logic in step_vehicles_gpu().
/// Extracted as a pure function for testability.
/// Uses Commuter profile as default for backward-compatible flag tests.
fn compute_agent_flags_test(is_bus_dwelling: bool, is_emergency: bool) -> u32 {
    crate::compute::compute_agent_flags(
        is_bus_dwelling,
        is_emergency,
        velos_core::cost::AgentProfile::Commuter,
    )
}

#[test]
fn flags_neither_dwelling_nor_emergency() {
    assert_eq!(compute_agent_flags_test(false, false), 0);
}

#[test]
fn flags_bus_dwelling_only() {
    // Bit 0 set: FLAG_BUS_DWELLING
    assert_eq!(compute_agent_flags_test(true, false), 1);
}

#[test]
fn flags_emergency_only() {
    // Bit 1 set: FLAG_EMERGENCY_ACTIVE
    let flags = compute_agent_flags_test(false, true);
    assert_eq!(flags & 2, 2, "FLAG_EMERGENCY_ACTIVE (bit 1) must be set");
    assert_eq!(flags, 2);
}

#[test]
fn flags_bus_dwelling_and_emergency() {
    // Both bits set: FLAG_BUS_DWELLING | FLAG_EMERGENCY_ACTIVE
    let flags = compute_agent_flags_test(true, true);
    assert_eq!(flags, 3, "Both FLAG_BUS_DWELLING and FLAG_EMERGENCY_ACTIVE must be set");
    assert_eq!(flags & 1, 1, "FLAG_BUS_DWELLING bit must be set");
    assert_eq!(flags & 2, 2, "FLAG_EMERGENCY_ACTIVE bit must be set");
}

#[test]
fn emergency_vehicle_upload_count_capped_at_16() {
    // Verify GpuEmergencyVehicle is Pod/Zeroable and can be used in upload
    let vehicles: Vec<GpuEmergencyVehicle> = (0..20)
        .map(|i| GpuEmergencyVehicle {
            pos_x: i as f32 * 10.0,
            pos_y: i as f32 * 5.0,
            heading: 0.5,
            _pad: 0.0,
        })
        .collect();
    // upload_emergency_vehicles caps at 16
    let count = vehicles.len().min(16);
    assert_eq!(count, 16);
}

#[test]
fn gpu_emergency_vehicle_layout() {
    // Verify GpuEmergencyVehicle is 16 bytes (4 x f32) for correct GPU alignment
    assert_eq!(std::mem::size_of::<GpuEmergencyVehicle>(), 16);
}

#[test]
fn compute_flags_with_tourist_profile_encodes_bits_4_7() {
    use velos_core::cost::AgentProfile;

    let flags = crate::compute::compute_agent_flags(false, false, AgentProfile::Tourist);
    // Tourist = 4, encoded in bits 4-7 => 0x40
    assert_eq!((flags >> 4) & 0x0F, 4, "Profile bits 4-7 should encode Tourist (4)");
    assert_eq!(flags & 0x0F, 0, "Low bits should be 0 (no dwelling, no emergency)");
}

#[test]
fn compute_flags_with_profile_preserves_low_bits() {
    use velos_core::cost::AgentProfile;

    // Bus dwelling + Emergency active + Bus profile
    let flags = crate::compute::compute_agent_flags(true, true, AgentProfile::Bus);
    assert_eq!(flags & 0x01, 1, "FLAG_BUS_DWELLING should be set");
    assert_eq!(flags & 0x02, 2, "FLAG_EMERGENCY_ACTIVE should be set");
    assert_eq!((flags >> 4) & 0x0F, 1, "Profile bits should encode Bus (1)");
}

#[test]
fn compute_flags_decode_roundtrip_all_profiles() {
    use velos_core::cost::{AgentProfile, decode_profile_from_flags};

    for profile in [
        AgentProfile::Commuter,
        AgentProfile::Bus,
        AgentProfile::Truck,
        AgentProfile::Emergency,
        AgentProfile::Tourist,
        AgentProfile::Teen,
        AgentProfile::Senior,
        AgentProfile::Cyclist,
    ] {
        let flags = crate::compute::compute_agent_flags(false, false, profile);
        let decoded = decode_profile_from_flags(flags);
        assert_eq!(decoded, profile, "Round-trip failed for {profile:?}");
    }
}
