//! Tests for GTFS import pipeline.

use std::path::Path;
use velos_demand::gtfs::load_gtfs_csv;

const FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/gtfs/test_fixture");

#[test]
fn load_csv_parses_routes() {
    let (routes, _schedules) = load_gtfs_csv(Path::new(FIXTURE_DIR)).unwrap();
    assert_eq!(routes.len(), 2, "expected 2 routes, got {}", routes.len());

    let r1 = routes.iter().find(|r| r.route_id == "route_01").unwrap();
    assert_eq!(r1.route_name, "Ben Thanh - Cho Lon");
    assert_eq!(r1.stops.len(), 4, "route_01 should have 4 stops from stop_times");
}

#[test]
fn load_csv_parses_stops_with_coordinates() {
    let (routes, _) = load_gtfs_csv(Path::new(FIXTURE_DIR)).unwrap();
    let r1 = routes.iter().find(|r| r.route_id == "route_01").unwrap();

    let ben_thanh = &r1.stops[0];
    assert_eq!(ben_thanh.stop_id, "stop_01");
    assert_eq!(ben_thanh.name, "Ben Thanh Market");
    assert!((ben_thanh.lat - 10.7725).abs() < 1e-4);
    assert!((ben_thanh.lon - 106.6980).abs() < 1e-4);
}

#[test]
fn load_csv_parses_schedules() {
    let (_, schedules) = load_gtfs_csv(Path::new(FIXTURE_DIR)).unwrap();
    assert_eq!(schedules.len(), 2, "expected 2 schedules");

    let s1 = schedules.iter().find(|s| s.trip_id == "trip_01").unwrap();
    assert_eq!(s1.route_id, "route_01");
    assert_eq!(s1.stop_times.len(), 4);

    // First stop: 06:00:00 = 21600s
    assert_eq!(s1.stop_times[0].arrival_s, 21600);
    assert_eq!(s1.stop_times[0].departure_s, 21660); // 06:01:00
    assert_eq!(s1.stop_times[0].stop_sequence, 1);
}

#[test]
fn load_csv_stop_times_ordered_by_sequence() {
    let (_, schedules) = load_gtfs_csv(Path::new(FIXTURE_DIR)).unwrap();
    for sched in &schedules {
        let seqs: Vec<u32> = sched.stop_times.iter().map(|st| st.stop_sequence).collect();
        let mut sorted = seqs.clone();
        sorted.sort();
        assert_eq!(seqs, sorted, "stop_times for {} not ordered", sched.trip_id);
    }
}

#[test]
fn load_csv_route2_has_correct_stops() {
    let (routes, _) = load_gtfs_csv(Path::new(FIXTURE_DIR)).unwrap();
    let r2 = routes.iter().find(|r| r.route_id == "route_02").unwrap();
    assert_eq!(r2.stops.len(), 3);
    assert_eq!(r2.stops[0].stop_id, "stop_02"); // sequence 1
    assert_eq!(r2.stops[1].stop_id, "stop_05"); // sequence 2
    assert_eq!(r2.stops[2].stop_id, "stop_01"); // sequence 3
}

#[test]
fn load_csv_missing_directory_returns_error() {
    let result = load_gtfs_csv(Path::new("/nonexistent/path"));
    assert!(result.is_err());
}
