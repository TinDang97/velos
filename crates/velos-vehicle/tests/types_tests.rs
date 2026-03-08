//! Tests for VehicleType enum and default IDM parameters.
//!
//! Updated for HCMC-calibrated defaults (Phase 08-01).

use velos_vehicle::config::VehicleConfig;
use velos_vehicle::types::{
    default_idm_params, default_idm_params_from_config, default_mobil_params,
    default_mobil_params_for_type, VehicleType,
};

#[test]
fn vehicle_type_has_7_variants() {
    let variants = [
        VehicleType::Motorbike,
        VehicleType::Car,
        VehicleType::Bus,
        VehicleType::Bicycle,
        VehicleType::Truck,
        VehicleType::Emergency,
        VehicleType::Pedestrian,
    ];
    assert_eq!(variants.len(), 7);
}

#[test]
fn idm_params_motorbike() {
    let p = default_idm_params(VehicleType::Motorbike);
    assert!((p.v0 - 11.1).abs() < 0.01);
    assert!((p.s0 - 1.0).abs() < 0.01);
    // HCMC: t_headway=0.8 (more aggressive than literature 1.0)
    assert!((p.t_headway - 0.8).abs() < 0.01);
    assert!((p.a - 2.0).abs() < 0.01);
    assert!((p.b - 3.0).abs() < 0.01);
    assert!((p.delta - 4.0).abs() < 0.01);
}

#[test]
fn idm_params_car() {
    let p = default_idm_params(VehicleType::Car);
    // HCMC: v0=9.7 (35 km/h), NOT 13.9 (50 km/h)
    assert!((p.v0 - 9.7).abs() < 0.01, "car v0 should be 9.7, got {}", p.v0);
    assert!((p.s0 - 2.0).abs() < 0.01);
    assert!((p.t_headway - 1.5).abs() < 0.01);
    assert!((p.a - 1.0).abs() < 0.01);
    assert!((p.b - 2.0).abs() < 0.01);
}

#[test]
fn idm_params_bus() {
    let p = default_idm_params(VehicleType::Bus);
    // HCMC: v0=8.3 (30 km/h), NOT 11.1 (40 km/h)
    assert!((p.v0 - 8.3).abs() < 0.01, "Bus v0 should be 8.3, got {}", p.v0);
    assert!((p.s0 - 3.0).abs() < 0.01, "Bus s0 should be 3.0, got {}", p.s0);
    assert!((p.t_headway - 1.5).abs() < 0.01, "Bus t_headway should be 1.5, got {}", p.t_headway);
    assert!((p.a - 1.0).abs() < 0.01, "Bus a should be 1.0, got {}", p.a);
    assert!((p.b - 2.5).abs() < 0.01, "Bus b should be 2.5, got {}", p.b);
    assert!((p.delta - 4.0).abs() < 0.01);
}

#[test]
fn idm_params_bicycle() {
    let p = default_idm_params(VehicleType::Bicycle);
    assert!((p.v0 - 4.17).abs() < 0.01, "Bicycle v0 should be 4.17, got {}", p.v0);
    assert!((p.s0 - 1.5).abs() < 0.01, "Bicycle s0 should be 1.5, got {}", p.s0);
    assert!((p.t_headway - 1.0).abs() < 0.01, "Bicycle t_headway should be 1.0, got {}", p.t_headway);
    assert!((p.a - 1.0).abs() < 0.01, "Bicycle a should be 1.0, got {}", p.a);
    assert!((p.b - 3.0).abs() < 0.01, "Bicycle b should be 3.0, got {}", p.b);
    assert!((p.delta - 4.0).abs() < 0.01);
}

#[test]
fn idm_params_truck() {
    let p = default_idm_params(VehicleType::Truck);
    // HCMC: v0=11.1 (40 km/h), matching motorbike speed for urban flow
    assert!((p.v0 - 11.1).abs() < 0.01, "Truck v0 should be 11.1 (40km/h), got {}", p.v0);
    assert!((p.s0 - 3.0).abs() < 0.01, "Truck s0 should be 3.0, got {}", p.s0);
    assert!((p.t_headway - 1.5).abs() < 0.01, "Truck t_headway should be 1.5, got {}", p.t_headway);
    assert!((p.a - 1.5).abs() < 0.01, "Truck a should be 1.5, got {}", p.a);
    assert!((p.b - 2.5).abs() < 0.01, "Truck b should be 2.5, got {}", p.b);
    assert!((p.delta - 4.0).abs() < 0.01);
}

#[test]
fn idm_params_emergency() {
    let p = default_idm_params(VehicleType::Emergency);
    assert!((p.v0 - 16.7).abs() < 0.01, "Emergency v0 should be 16.7, got {}", p.v0);
    assert!((p.s0 - 2.0).abs() < 0.01, "Emergency s0 should be 2.0, got {}", p.s0);
    assert!((p.t_headway - 1.2).abs() < 0.01, "Emergency t_headway should be 1.2, got {}", p.t_headway);
    assert!((p.a - 2.0).abs() < 0.01, "Emergency a should be 2.0, got {}", p.a);
    assert!((p.b - 3.5).abs() < 0.01, "Emergency b should be 3.5, got {}", p.b);
    assert!((p.delta - 4.0).abs() < 0.01);
}

#[test]
fn idm_params_pedestrian() {
    let p = default_idm_params(VehicleType::Pedestrian);
    assert!((p.v0 - 1.4).abs() < 0.01);
    assert!((p.s0 - 0.5).abs() < 0.01);
}

// ---------------------------------------------------------------------------
// Config-backed factory functions
// ---------------------------------------------------------------------------

#[test]
fn idm_params_from_config_matches_default() {
    let config = VehicleConfig::default();
    for vt in [
        VehicleType::Motorbike,
        VehicleType::Car,
        VehicleType::Bus,
        VehicleType::Bicycle,
        VehicleType::Truck,
        VehicleType::Emergency,
        VehicleType::Pedestrian,
    ] {
        let from_fn = default_idm_params(vt);
        let from_cfg = default_idm_params_from_config(vt, &config);
        assert!(
            (from_fn.v0 - from_cfg.v0).abs() < 0.01,
            "{vt:?}: v0 mismatch {:.2} vs {:.2}",
            from_fn.v0,
            from_cfg.v0
        );
    }
}

#[test]
fn mobil_params_default_matches_car_config() {
    let params = default_mobil_params();
    let config = VehicleConfig::default();
    let from_cfg = default_mobil_params_for_type(VehicleType::Car, &config);
    assert!((params.politeness - from_cfg.politeness).abs() < 0.01);
    assert!((params.threshold - from_cfg.threshold).abs() < 0.01);
}

#[test]
fn mobil_params_per_type_varies() {
    let config = VehicleConfig::default();
    let moto = default_mobil_params_for_type(VehicleType::Motorbike, &config);
    let car = default_mobil_params_for_type(VehicleType::Car, &config);
    // Motorbike politeness (0.1) < car politeness (0.3)
    assert!(
        moto.politeness < car.politeness,
        "motorbike should be less polite: {} vs {}",
        moto.politeness,
        car.politeness
    );
}
