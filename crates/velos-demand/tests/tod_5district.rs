//! Tests for 5-zone time-of-day demand profiles.

use velos_demand::tod_profile::TodProfile;
use velos_demand::od_matrix::{OdMatrix, Zone};

#[test]
fn hcmc_5district_weekday_returns_5_zones() {
    let profiles = TodProfile::hcmc_5district_weekday();
    assert_eq!(profiles.len(), 5);
}

#[test]
fn hcmc_5district_weekend_returns_5_zones() {
    let profiles = TodProfile::hcmc_5district_weekend();
    assert_eq!(profiles.len(), 5);
}

#[test]
fn district_names_are_correct() {
    let profiles = TodProfile::hcmc_5district_weekday();
    let names: Vec<&str> = profiles.iter().map(|(z, _)| z.name.as_str()).collect();
    assert!(names.contains(&"District 1"));
    assert!(names.contains(&"District 3"));
    assert!(names.contains(&"District 5"));
    assert!(names.contains(&"District 10"));
    assert!(names.contains(&"Binh Thanh"));
}

#[test]
fn am_peak_factor_higher_than_offpeak() {
    let profiles = TodProfile::hcmc_5district_weekday();
    for (_zone, tod) in &profiles {
        let am_peak = tod.factor_at(7.5);
        let off_peak = tod.factor_at(13.0);
        assert!(
            am_peak > off_peak,
            "AM peak ({am_peak}) should be higher than off-peak ({off_peak})"
        );
    }
}

#[test]
fn pm_peak_factor_higher_than_offpeak() {
    let profiles = TodProfile::hcmc_5district_weekday();
    for (_zone, tod) in &profiles {
        let pm_peak = tod.factor_at(17.5);
        let off_peak = tod.factor_at(13.0);
        assert!(
            pm_peak > off_peak,
            "PM peak ({pm_peak}) should be higher than off-peak ({off_peak})"
        );
    }
}

#[test]
fn am_peak_factor_in_expected_range() {
    let profiles = TodProfile::hcmc_5district_weekday();
    for (zone, tod) in &profiles {
        let am = tod.factor_at(7.5);
        assert!(
            (1.0..=2.5).contains(&am),
            "{}: AM peak factor {am} outside [1.0, 2.5]",
            zone.name
        );
    }
}

#[test]
fn off_peak_factor_in_expected_range() {
    let profiles = TodProfile::hcmc_5district_weekday();
    for (zone, tod) in &profiles {
        let off = tod.factor_at(13.0);
        assert!(
            (0.3..=0.8).contains(&off),
            "{}: off-peak factor {off} outside [0.3, 0.8]",
            zone.name
        );
    }
}

#[test]
fn weekend_lower_than_weekday_at_peak() {
    let weekday = TodProfile::hcmc_5district_weekday();
    let weekend = TodProfile::hcmc_5district_weekend();

    for ((wd_zone, wd_tod), (we_zone, we_tod)) in weekday.iter().zip(weekend.iter()) {
        assert_eq!(wd_zone.name, we_zone.name);
        let wd_am = wd_tod.factor_at(7.5);
        let we_am = we_tod.factor_at(7.5);
        assert!(
            we_am < wd_am,
            "{}: weekend AM ({we_am}) should be lower than weekday AM ({wd_am})",
            wd_zone.name
        );
    }
}

#[test]
fn interpolation_is_smooth() {
    let profiles = TodProfile::hcmc_5district_weekday();
    for (_zone, tod) in &profiles {
        // Check that adjacent hours don't jump more than 0.5 factor units.
        for hour_x10 in 0..230 {
            let h1 = hour_x10 as f64 / 10.0;
            let h2 = (hour_x10 + 1) as f64 / 10.0;
            let f1 = tod.factor_at(h1);
            let f2 = tod.factor_at(h2);
            let diff = (f2 - f1).abs();
            assert!(
                diff < 0.5,
                "Jump of {diff} between {h1} and {h2} is too steep"
            );
        }
    }
}

#[test]
fn od_matrix_5district_has_25_pairs() {
    let od = OdMatrix::hcmc_5district();
    // 5 zones => up to 25 OD pairs (including intra-zone).
    // We expect at least 20 non-zero pairs (all cross-district flows).
    let pair_count = od.zone_pairs().count();
    assert!(
        pair_count >= 20,
        "Expected >= 20 OD pairs, got {pair_count}"
    );
}

#[test]
fn od_matrix_total_demand_near_280k_at_peak() {
    let od = OdMatrix::hcmc_5district();
    let profiles = TodProfile::hcmc_5district_weekday();

    // Calculate total demand at AM peak (hour 7.5).
    let mut total: f64 = 0.0;
    for (zone, tod) in &profiles {
        let zone_enum = zone_name_to_enum(&zone.name);
        let factor = tod.factor_at(7.5);
        // Sum all outbound trips from this zone.
        let outbound: u32 = od
            .zone_pairs()
            .filter(|(from, _, _)| *from == zone_enum)
            .map(|(_, _, count)| count)
            .sum();
        total += outbound as f64 * factor;
    }

    // Should be approximately 280K (within 30% tolerance for profile shape).
    assert!(
        total > 180_000.0 && total < 380_000.0,
        "Total demand at AM peak: {total}, expected ~280K"
    );
}

/// Map zone name string to Zone enum for OD matrix lookup.
fn zone_name_to_enum(name: &str) -> Zone {
    match name {
        "District 1" => Zone::District1,
        "District 3" => Zone::District3,
        "District 5" => Zone::District5,
        "District 10" => Zone::District10,
        "Binh Thanh" => Zone::BinhThanh,
        _ => panic!("Unknown zone: {name}"),
    }
}

#[test]
fn district1_cbd_has_sharpest_peak() {
    let profiles = TodProfile::hcmc_5district_weekday();
    let d1 = profiles
        .iter()
        .find(|(z, _)| z.name == "District 1")
        .unwrap();
    let d10 = profiles
        .iter()
        .find(|(z, _)| z.name == "District 10")
        .unwrap();

    // District 1 (CBD) should have higher peak factor than residential District 10.
    let d1_peak = d1.1.factor_at(7.5);
    let d10_peak = d10.1.factor_at(7.5);
    assert!(
        d1_peak > d10_peak,
        "D1 peak ({d1_peak}) should exceed D10 peak ({d10_peak})"
    );
}
