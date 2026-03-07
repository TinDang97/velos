//! GTFS import pipeline for HCMC bus routes.
//!
//! Parses standard GTFS CSV files (routes.txt, stops.txt, trips.txt, stop_times.txt)
//! into typed `BusRoute` and `BusSchedule` structs for simulation use.
//!
//! This uses a lightweight CSV parser (no external crate) since HCMC GTFS data
//! availability is uncertain and we may need to handle non-standard formats.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::error::DemandError;

/// A transit stop with geographic coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct GtfsStop {
    /// Unique stop identifier from GTFS.
    pub stop_id: String,
    /// Human-readable stop name.
    pub name: String,
    /// Latitude (WGS84).
    pub lat: f64,
    /// Longitude (WGS84).
    pub lon: f64,
}

/// A bus route with its ordered list of stops.
#[derive(Debug, Clone, PartialEq)]
pub struct BusRoute {
    /// Unique route identifier from GTFS.
    pub route_id: String,
    /// Route display name.
    pub route_name: String,
    /// Ordered stops along this route (derived from first trip's stop_times).
    pub stops: Vec<GtfsStop>,
}

/// A single stop time entry within a schedule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopTime {
    /// Stop identifier.
    pub stop_id: String,
    /// Arrival time as seconds from midnight.
    pub arrival_s: u32,
    /// Departure time as seconds from midnight.
    pub departure_s: u32,
    /// Position in the stop sequence (1-based).
    pub stop_sequence: u32,
}

/// A bus schedule representing one trip along a route.
#[derive(Debug, Clone, PartialEq)]
pub struct BusSchedule {
    /// Unique trip identifier.
    pub trip_id: String,
    /// Route this trip belongs to.
    pub route_id: String,
    /// Ordered stop times for this trip.
    pub stop_times: Vec<StopTime>,
}

/// Parse a time string "HH:MM:SS" into seconds from midnight.
///
/// Supports hours >= 24 (GTFS convention for trips crossing midnight).
fn parse_time(s: &str) -> Option<u32> {
    let parts: Vec<&str> = s.trim().split(':').collect();
    if parts.len() != 3 {
        return None;
    }
    let h: u32 = parts[0].parse().ok()?;
    let m: u32 = parts[1].parse().ok()?;
    let s: u32 = parts[2].parse().ok()?;
    Some(h * 3600 + m * 60 + s)
}

/// Parsed CSV: header names and rows as string-keyed maps.
type CsvData = (Vec<String>, Vec<HashMap<String, String>>);

/// Parse a CSV file into rows of (header_index -> value) maps.
///
/// Returns the header names and a vec of rows. Each row is a `HashMap<&str, &str>`.
/// Skips empty lines. Logs warnings for malformed rows but continues parsing.
fn parse_csv(content: &str) -> Option<CsvData> {
    let mut lines = content.lines();
    let header_line = lines.next()?;
    let headers: Vec<String> = header_line.split(',').map(|h| h.trim().to_string()).collect();

    let mut rows = Vec::new();
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split(',').collect();
        if fields.len() < headers.len() {
            log::warn!("skipping malformed CSV row: {line}");
            continue;
        }
        let mut row = HashMap::new();
        for (i, hdr) in headers.iter().enumerate() {
            row.insert(hdr.clone(), fields[i].trim().to_string());
        }
        rows.push(row);
    }
    Some((headers, rows))
}

/// Load GTFS data from CSV files in a directory.
///
/// Reads `routes.txt`, `stops.txt`, `trips.txt`, and `stop_times.txt`.
/// Returns `(Vec<BusRoute>, Vec<BusSchedule>)`.
///
/// # Errors
///
/// Returns `DemandError::MissingFile` if a required file is missing.
/// Returns `DemandError::Io` on read failures.
/// Malformed rows produce log warnings but do not fail the parse.
pub fn load_gtfs_csv(path: &Path) -> Result<(Vec<BusRoute>, Vec<BusSchedule>), DemandError> {
    // Read all required files
    let routes_path = path.join("routes.txt");
    let stops_path = path.join("stops.txt");
    let trips_path = path.join("trips.txt");
    let stop_times_path = path.join("stop_times.txt");

    let routes_csv = read_required(&routes_path)?;
    let stops_csv = read_required(&stops_path)?;
    let trips_csv = read_required(&trips_path)?;
    let stop_times_csv = read_required(&stop_times_path)?;

    // Parse stops into a lookup map
    let (_, stop_rows) = parse_csv(&stops_csv).ok_or_else(|| DemandError::Parse {
        file: "stops.txt".to_string(),
        line: 0,
        reason: "empty or invalid CSV".to_string(),
    })?;

    let mut stop_map: HashMap<String, GtfsStop> = HashMap::new();
    for (i, row) in stop_rows.iter().enumerate() {
        let Some(stop_id) = row.get("stop_id") else {
            log::warn!("stops.txt line {}: missing stop_id", i + 2);
            continue;
        };
        let name = row.get("stop_name").cloned().unwrap_or_default();
        let lat: f64 = row
            .get("stop_lat")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0);
        let lon: f64 = row
            .get("stop_lon")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0);

        stop_map.insert(
            stop_id.clone(),
            GtfsStop {
                stop_id: stop_id.clone(),
                name,
                lat,
                lon,
            },
        );
    }

    // Parse routes into a lookup map (route_id -> route_name)
    let (_, route_rows) = parse_csv(&routes_csv).ok_or_else(|| DemandError::Parse {
        file: "routes.txt".to_string(),
        line: 0,
        reason: "empty or invalid CSV".to_string(),
    })?;

    let mut route_names: HashMap<String, String> = HashMap::new();
    for row in &route_rows {
        if let Some(route_id) = row.get("route_id") {
            let name = row.get("route_long_name").cloned().unwrap_or_default();
            route_names.insert(route_id.clone(), name);
        }
    }

    // Parse trips (trip_id -> route_id)
    let (_, trip_rows) = parse_csv(&trips_csv).ok_or_else(|| DemandError::Parse {
        file: "trips.txt".to_string(),
        line: 0,
        reason: "empty or invalid CSV".to_string(),
    })?;

    let mut trip_route: HashMap<String, String> = HashMap::new();
    for row in &trip_rows {
        if let (Some(trip_id), Some(route_id)) = (row.get("trip_id"), row.get("route_id")) {
            trip_route.insert(trip_id.clone(), route_id.clone());
        }
    }

    // Parse stop_times grouped by trip
    let (_, st_rows) = parse_csv(&stop_times_csv).ok_or_else(|| DemandError::Parse {
        file: "stop_times.txt".to_string(),
        line: 0,
        reason: "empty or invalid CSV".to_string(),
    })?;

    let mut trip_stop_times: HashMap<String, Vec<StopTime>> = HashMap::new();
    for (i, row) in st_rows.iter().enumerate() {
        let Some(trip_id) = row.get("trip_id") else {
            log::warn!("stop_times.txt line {}: missing trip_id", i + 2);
            continue;
        };
        let Some(stop_id) = row.get("stop_id") else {
            log::warn!("stop_times.txt line {}: missing stop_id", i + 2);
            continue;
        };
        let arrival_s = row
            .get("arrival_time")
            .and_then(|v| parse_time(v))
            .unwrap_or(0);
        let departure_s = row
            .get("departure_time")
            .and_then(|v| parse_time(v))
            .unwrap_or(0);
        let stop_sequence: u32 = row
            .get("stop_sequence")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);

        trip_stop_times
            .entry(trip_id.clone())
            .or_default()
            .push(StopTime {
                stop_id: stop_id.clone(),
                arrival_s,
                departure_s,
                stop_sequence,
            });
    }

    // Sort stop_times by sequence within each trip
    for times in trip_stop_times.values_mut() {
        times.sort_by_key(|st| st.stop_sequence);
    }

    // Build BusSchedule list
    let mut schedules: Vec<BusSchedule> = Vec::new();
    for (trip_id, stop_times) in &trip_stop_times {
        let route_id = trip_route
            .get(trip_id)
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        schedules.push(BusSchedule {
            trip_id: trip_id.clone(),
            route_id,
            stop_times: stop_times.clone(),
        });
    }
    schedules.sort_by(|a, b| a.trip_id.cmp(&b.trip_id));

    // Build BusRoute list -- derive stops from the first trip per route
    let mut route_stops: HashMap<String, Vec<GtfsStop>> = HashMap::new();
    for sched in &schedules {
        route_stops.entry(sched.route_id.clone()).or_insert_with(|| {
            sched
                .stop_times
                .iter()
                .filter_map(|st| stop_map.get(&st.stop_id).cloned())
                .collect()
        });
    }

    let mut routes: Vec<BusRoute> = Vec::new();
    for (route_id, name) in &route_names {
        let stops = route_stops.remove(route_id).unwrap_or_default();
        routes.push(BusRoute {
            route_id: route_id.clone(),
            route_name: name.clone(),
            stops,
        });
    }
    routes.sort_by(|a, b| a.route_id.cmp(&b.route_id));

    Ok((routes, schedules))
}

/// Read a required file, returning a clear error if missing.
fn read_required(path: &Path) -> Result<String, DemandError> {
    if !path.exists() {
        return Err(DemandError::MissingFile {
            path: path.display().to_string(),
        });
    }
    Ok(fs::read_to_string(path)?)
}
