//! Integration tests for velos-demand crate.

use velos_demand::{OdMatrix, TodProfile, Zone};

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
