//! Integration tests for velos-demand crate.

use velos_demand::{OdMatrix, SpawnVehicleType, Spawner, TodProfile, Zone};

// ── OD Matrix tests ──────────────────────────────────────────────────

#[test]
fn od_matrix_get_set_trips() {
    let mut od = OdMatrix::new();
    od.set_trips(Zone::BenThanh, Zone::NguyenHue, 100);
    assert_eq!(od.get_trips(Zone::BenThanh, Zone::NguyenHue), 100);
}

#[test]
fn od_matrix_unconfigured_pair_returns_zero() {
    let od = OdMatrix::new();
    assert_eq!(od.get_trips(Zone::BenThanh, Zone::Bitexco), 0);
}

#[test]
fn od_matrix_total_trips() {
    let mut od = OdMatrix::new();
    od.set_trips(Zone::BenThanh, Zone::NguyenHue, 50);
    od.set_trips(Zone::Bitexco, Zone::BuiVien, 30);
    assert_eq!(od.total_trips(), 80);
}

#[test]
fn od_matrix_zone_pairs_iterates_nonzero() {
    let mut od = OdMatrix::new();
    od.set_trips(Zone::BenThanh, Zone::NguyenHue, 50);
    od.set_trips(Zone::Bitexco, Zone::BuiVien, 30);
    let pairs: Vec<_> = od.zone_pairs().collect();
    assert_eq!(pairs.len(), 2);
}

#[test]
fn od_matrix_district1_poc_has_nonzero_trips() {
    let od = OdMatrix::district1_poc();
    let total = od.total_trips();
    assert!(total >= 400, "POC should have >= 400 trips/hr, got {total}");
    assert!(total <= 600, "POC should have <= 600 trips/hr, got {total}");
}

#[test]
fn od_matrix_district1_poc_pairs_count() {
    let od = OdMatrix::district1_poc();
    let pairs: Vec<_> = od.zone_pairs().collect();
    assert!(pairs.len() >= 6, "POC should have at least 6 zone pairs");
}

// ── ToD Profile tests ────────────────────────────────────────────────

#[test]
fn tod_am_peak_factor_is_one() {
    let tod = TodProfile::hcmc_weekday();
    let factor = tod.factor_at(7.5);
    assert!(
        (factor - 1.0).abs() < 0.01,
        "AM peak (7.5h) should be ~1.0, got {factor}"
    );
}

#[test]
fn tod_pm_peak_factor_is_one() {
    let tod = TodProfile::hcmc_weekday();
    let factor = tod.factor_at(17.5);
    assert!(
        (factor - 1.0).abs() < 0.01,
        "PM peak (17.5h) should be ~1.0, got {factor}"
    );
}

#[test]
fn tod_off_peak_early_morning() {
    let tod = TodProfile::hcmc_weekday();
    let factor = tod.factor_at(3.0);
    assert!(
        factor < 0.10,
        "Off-peak (3h) should be < 0.10, got {factor}"
    );
}

#[test]
fn tod_midday_factor() {
    let tod = TodProfile::hcmc_weekday();
    let factor = tod.factor_at(12.0);
    assert!(
        (factor - 0.70).abs() < 0.01,
        "Midday (12h) should be ~0.70, got {factor}"
    );
}

#[test]
fn tod_before_first_point_uses_first_factor() {
    let tod = TodProfile::new(vec![(5.0, 0.3), (10.0, 1.0)]);
    let factor = tod.factor_at(2.0);
    assert!(
        (factor - 0.3).abs() < 0.001,
        "Before first point should use first factor, got {factor}"
    );
}

#[test]
fn tod_after_last_point_uses_last_factor() {
    let tod = TodProfile::new(vec![(5.0, 0.3), (10.0, 1.0)]);
    let factor = tod.factor_at(15.0);
    assert!(
        (factor - 1.0).abs() < 0.001,
        "After last point should use last factor, got {factor}"
    );
}

#[test]
fn tod_linear_interpolation_midpoint() {
    let tod = TodProfile::new(vec![(0.0, 0.0), (10.0, 1.0)]);
    let factor = tod.factor_at(5.0);
    assert!(
        (factor - 0.5).abs() < 0.001,
        "Midpoint interpolation should be 0.5, got {factor}"
    );
}

// ── Spawner tests ────────────────────────────────────────────────────

#[test]
fn spawner_peak_hour_count_matches_od_total() {
    let od = OdMatrix::district1_poc();
    let total_trips = od.total_trips();
    // Use a flat profile at factor 1.0 for a full hour
    let tod = TodProfile::new(vec![(0.0, 1.0), (24.0, 1.0)]);
    let mut spawner = Spawner::new(od, tod, 42);

    // Generate spawns for 1 full hour (dt = 3600s) at hour 12
    let spawns = spawner.generate_spawns(12.0, 3600.0);
    let count = spawns.len() as f64;

    // Should be approximately total_trips (560), allow 20% tolerance for stochastic
    let lower = total_trips as f64 * 0.80;
    let upper = total_trips as f64 * 1.20;
    assert!(
        count >= lower && count <= upper,
        "Expected ~{total_trips} spawns at peak, got {count}"
    );
}

#[test]
fn spawner_off_peak_produces_fewer() {
    let od = OdMatrix::district1_poc();
    let tod = TodProfile::new(vec![(0.0, 0.1), (24.0, 0.1)]);
    let mut spawner = Spawner::new(od.clone(), tod, 42);
    let off_peak_spawns = spawner.generate_spawns(3.0, 3600.0);

    let peak_tod = TodProfile::new(vec![(0.0, 1.0), (24.0, 1.0)]);
    let mut peak_spawner = Spawner::new(od, peak_tod, 42);
    let peak_spawns = peak_spawner.generate_spawns(7.0, 3600.0);

    assert!(
        off_peak_spawns.len() < peak_spawns.len(),
        "Off-peak ({}) should produce fewer spawns than peak ({})",
        off_peak_spawns.len(),
        peak_spawns.len()
    );
}

#[test]
fn spawner_vehicle_type_distribution() {
    let od = OdMatrix::district1_poc();
    let tod = TodProfile::new(vec![(0.0, 1.0), (24.0, 1.0)]);
    let mut spawner = Spawner::new(od, tod, 123);

    // Generate many spawns across multiple hours to get statistical convergence
    let mut motorbike_count = 0usize;
    let mut car_count = 0usize;
    let mut ped_count = 0usize;

    for hour in 0..20 {
        let spawns = spawner.generate_spawns(hour as f64, 3600.0);
        for s in &spawns {
            match s.vehicle_type {
                SpawnVehicleType::Motorbike | SpawnVehicleType::Bicycle => motorbike_count += 1,
                SpawnVehicleType::Car | SpawnVehicleType::Bus | SpawnVehicleType::Truck | SpawnVehicleType::Emergency => car_count += 1,
                SpawnVehicleType::Pedestrian => ped_count += 1,
            }
        }
    }

    let total = (motorbike_count + car_count + ped_count) as f64;
    assert!(total > 1000.0, "Need at least 1000 spawns, got {total}");

    let moto_pct = motorbike_count as f64 / total;
    let car_pct = car_count as f64 / total;
    let ped_pct = ped_count as f64 / total;

    assert!(
        (moto_pct - 0.80).abs() < 0.05,
        "Motorbike should be ~80%, got {:.1}%",
        moto_pct * 100.0
    );
    assert!(
        (car_pct - 0.15).abs() < 0.05,
        "Car should be ~15%, got {:.1}%",
        car_pct * 100.0
    );
    assert!(
        (ped_pct - 0.05).abs() < 0.05,
        "Pedestrian should be ~5%, got {:.1}%",
        ped_pct * 100.0
    );
}

#[test]
fn spawner_empty_od_produces_no_spawns() {
    let od = OdMatrix::new();
    let tod = TodProfile::hcmc_weekday();
    let mut spawner = Spawner::new(od, tod, 42);
    let spawns = spawner.generate_spawns(7.5, 3600.0);
    assert!(spawns.is_empty(), "Empty OD should produce no spawns");
}

#[test]
fn spawner_seeded_rng_is_deterministic() {
    let od = OdMatrix::district1_poc();
    let tod = TodProfile::hcmc_weekday();

    let mut s1 = Spawner::new(od.clone(), tod.clone(), 999);
    let mut s2 = Spawner::new(od, tod, 999);

    let spawns1 = s1.generate_spawns(7.5, 60.0);
    let spawns2 = s2.generate_spawns(7.5, 60.0);

    assert_eq!(spawns1.len(), spawns2.len(), "Same seed should give same count");
    for (a, b) in spawns1.iter().zip(spawns2.iter()) {
        assert_eq!(a.origin, b.origin);
        assert_eq!(a.destination, b.destination);
        assert_eq!(a.vehicle_type, b.vehicle_type);
    }
}

#[test]
fn spawner_origin_differs_from_destination() {
    let od = OdMatrix::district1_poc();
    let tod = TodProfile::new(vec![(0.0, 1.0), (24.0, 1.0)]);
    let mut spawner = Spawner::new(od, tod, 42);
    let spawns = spawner.generate_spawns(12.0, 3600.0);

    for s in &spawns {
        assert_ne!(
            s.origin, s.destination,
            "Origin and destination should differ"
        );
    }
}
