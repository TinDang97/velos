//! Agent spawner combining OD matrices and ToD profiles to generate traffic demand.
//!
//! The spawner is the entry point for demand generation: it takes an OD matrix
//! (how many trips between zone pairs per hour) and a ToD profile (how demand
//! scales across the day), then stochastically generates individual spawn requests
//! with the correct vehicle type distribution (80% motorbike, 15% car, 5% ped).

use rand::distributions::WeightedIndex;
use rand::prelude::Distribution;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use velos_core::cost::AgentProfile;

use std::collections::HashMap;

use crate::od_matrix::{OdMatrix, Zone};
use crate::profile::{assign_profile, ProfileDistribution};
use crate::tod_profile::TodProfile;

/// Vehicle type for spawn requests. Kept local to avoid circular dependency
/// with velos-vehicle. The integration layer (02-04) maps to the real enum.
/// Vehicle type for spawn requests.
///
/// Order matches velos-core VehicleType for consistency.
/// The integration layer maps to the real VehicleType enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpawnVehicleType {
    Motorbike,
    Car,
    Bus,
    Bicycle,
    Truck,
    Emergency,
    Pedestrian,
}

/// HCMC vehicle type weights matching [`SpawnVehicleType`] variant order.
///
/// Mode shares calibrated for Ho Chi Minh City urban mixed traffic:
/// - Motorbike dominates (~76%) per HCMC transport surveys
/// - Bus weight is small here because [`BusSpawner`] handles GTFS-scheduled buses
/// - Emergency vehicles are rare (~0.5%)
const VEHICLE_WEIGHTS: [f64; 7] = [
    0.76,  // Motorbike
    0.12,  // Car
    0.02,  // Bus (supplementary to GTFS-scheduled BusSpawner)
    0.03,  // Bicycle
    0.03,  // Truck
    0.005, // Emergency
    0.035, // Pedestrian
];

/// A request to spawn an agent at a specific origin heading to a destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnRequest {
    /// Origin traffic analysis zone.
    pub origin: Zone,
    /// Destination traffic analysis zone.
    pub destination: Zone,
    /// Type of vehicle/agent to spawn.
    pub vehicle_type: SpawnVehicleType,
    /// Agent profile for cost-weighted route choice.
    pub profile: AgentProfile,
}

/// Combines an OD matrix and ToD profile to generate stochastic spawn requests.
///
/// Uses a seeded RNG for reproducibility across simulation runs.
pub struct Spawner {
    od: OdMatrix,
    tod: TodProfile,
    rng: StdRng,
    vehicle_dist: WeightedIndex<f64>,
    profile_dist: ProfileDistribution,
}

impl Spawner {
    /// Create a spawner with the given OD matrix, ToD profile, and RNG seed.
    pub fn new(od: OdMatrix, tod: TodProfile, seed: u64) -> Self {
        let vehicle_dist =
            WeightedIndex::new(VEHICLE_WEIGHTS).expect("vehicle weights are valid");
        Self {
            od,
            tod,
            rng: StdRng::seed_from_u64(seed),
            vehicle_dist,
            profile_dist: ProfileDistribution::default(),
        }
    }

    /// Access the profile distribution for external spawn logic.
    pub fn profile_dist(&self) -> &ProfileDistribution {
        &self.profile_dist
    }

    /// Create a spawner with a custom profile distribution.
    pub fn with_profile_distribution(mut self, dist: ProfileDistribution) -> Self {
        self.profile_dist = dist;
        self
    }

    /// Generate spawn requests for a given simulation hour and timestep (seconds).
    ///
    /// For each OD pair, expected spawns = `trips_per_hour * tod_factor * (dt / 3600)`.
    /// Integer part spawns deterministically; fractional part spawns with that probability.
    /// Each spawn gets a random vehicle type from the 80/15/5 distribution.
    pub fn generate_spawns(&mut self, sim_hour: f64, dt: f64) -> Vec<SpawnRequest> {
        let factor = self.tod.factor_at(sim_hour);
        let time_fraction = dt / 3600.0;

        let pairs: Vec<(Zone, Zone, u32)> = self.od.zone_pairs().collect();
        let mut spawns = Vec::new();

        for (from, to, trips) in pairs {
            let expected = trips as f64 * factor * time_fraction;

            // Deterministic integer part + stochastic fractional part
            let whole = expected.floor() as u32;
            let frac = expected - expected.floor();

            let mut count = whole;
            if frac > 0.0 && self.rng.gen_range(0.0..1.0) < frac {
                count += 1;
            }

            for _ in 0..count {
                let vtype = match self.vehicle_dist.sample(&mut self.rng) {
                    0 => SpawnVehicleType::Motorbike,
                    1 => SpawnVehicleType::Car,
                    2 => SpawnVehicleType::Bus,
                    3 => SpawnVehicleType::Bicycle,
                    4 => SpawnVehicleType::Truck,
                    5 => SpawnVehicleType::Emergency,
                    _ => SpawnVehicleType::Pedestrian,
                };
                let profile = assign_profile(vtype, &self.profile_dist, &mut self.rng);
                spawns.push(SpawnRequest {
                    origin: from,
                    destination: to,
                    vehicle_type: vtype,
                    profile,
                });
            }
        }

        spawns
    }

    /// Generate spawn requests with calibration scaling factors applied.
    ///
    /// Same logic as [`generate_spawns`] but multiplies each OD pair's trip
    /// count by the corresponding calibration factor. If no factor exists for
    /// an OD pair, 1.0 (no adjustment) is used.
    pub fn generate_spawns_calibrated(
        &mut self,
        sim_hour: f64,
        dt: f64,
        calibration_factors: &HashMap<(Zone, Zone), f32>,
    ) -> Vec<SpawnRequest> {
        let factor = self.tod.factor_at(sim_hour);
        let time_fraction = dt / 3600.0;

        let pairs: Vec<(Zone, Zone, u32)> = self.od.zone_pairs().collect();
        let mut spawns = Vec::new();

        for (from, to, trips) in pairs {
            let cal_factor = calibration_factors
                .get(&(from, to))
                .copied()
                .unwrap_or(1.0) as f64;
            let expected = trips as f64 * factor * cal_factor * time_fraction;

            let whole = expected.floor() as u32;
            let frac = expected - expected.floor();

            let mut count = whole;
            if frac > 0.0 && self.rng.gen_range(0.0..1.0) < frac {
                count += 1;
            }

            for _ in 0..count {
                let vtype = match self.vehicle_dist.sample(&mut self.rng) {
                    0 => SpawnVehicleType::Motorbike,
                    1 => SpawnVehicleType::Car,
                    2 => SpawnVehicleType::Bus,
                    3 => SpawnVehicleType::Bicycle,
                    4 => SpawnVehicleType::Truck,
                    5 => SpawnVehicleType::Emergency,
                    _ => SpawnVehicleType::Pedestrian,
                };
                let profile = assign_profile(vtype, &self.profile_dist, &mut self.rng);
                spawns.push(SpawnRequest {
                    origin: from,
                    destination: to,
                    vehicle_type: vtype,
                    profile,
                });
            }
        }

        spawns
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vehicle_weights_sum_to_one() {
        let sum: f64 = VEHICLE_WEIGHTS.iter().sum();
        assert!((sum - 1.0).abs() < 1e-10, "weights sum to {sum}, expected 1.0");
    }

    #[test]
    fn vehicle_weights_cover_all_types() {
        assert_eq!(
            VEHICLE_WEIGHTS.len(),
            7,
            "VEHICLE_WEIGHTS must have one entry per SpawnVehicleType variant"
        );
        for (i, &w) in VEHICLE_WEIGHTS.iter().enumerate() {
            assert!(w > 0.0, "weight index {i} must be positive");
        }
    }

    #[test]
    fn spawner_generates_all_seven_vehicle_types() {
        use std::collections::HashSet;

        let mut od = OdMatrix::new();
        // High trip volume so each generate_spawns call produces many agents,
        // ensuring rare types (Emergency 0.5%) appear within a few iterations.
        od.set_trips(Zone::BenThanh, Zone::NguyenHue, 36_000);
        let tod = TodProfile::new(vec![(0.0, 1.0), (24.0, 1.0)]);

        let mut seen = HashSet::new();
        for seed in 0..200 {
            let mut s = Spawner::new(od.clone(), tod.clone(), seed);
            for req in s.generate_spawns(12.0, 1.0) {
                seen.insert(req.vehicle_type);
            }
            if seen.len() == 7 {
                break;
            }
        }

        assert!(seen.contains(&SpawnVehicleType::Motorbike), "missing Motorbike");
        assert!(seen.contains(&SpawnVehicleType::Car), "missing Car");
        assert!(seen.contains(&SpawnVehicleType::Bus), "missing Bus");
        assert!(seen.contains(&SpawnVehicleType::Bicycle), "missing Bicycle");
        assert!(seen.contains(&SpawnVehicleType::Truck), "missing Truck");
        assert!(seen.contains(&SpawnVehicleType::Emergency), "missing Emergency");
        assert!(seen.contains(&SpawnVehicleType::Pedestrian), "missing Pedestrian");
    }

    #[test]
    fn spawner_calibrated_doubles_spawns_for_scaled_pair() {
        use std::collections::HashMap;

        let mut od = OdMatrix::new();
        od.set_trips(Zone::BenThanh, Zone::NguyenHue, 36_000);
        let tod = TodProfile::new(vec![(0.0, 1.0), (24.0, 1.0)]);

        // Run uncalibrated
        let mut total_uncal = 0usize;
        for seed in 0..100 {
            let mut s = Spawner::new(od.clone(), tod.clone(), seed);
            total_uncal += s.generate_spawns(12.0, 1.0).len();
        }

        // Run with 2.0 factor
        let mut factors = HashMap::new();
        factors.insert((Zone::BenThanh, Zone::NguyenHue), 2.0f32);

        let mut total_cal = 0usize;
        for seed in 0..100 {
            let mut s = Spawner::new(od.clone(), tod.clone(), seed);
            total_cal += s.generate_spawns_calibrated(12.0, 1.0, &factors).len();
        }

        // Calibrated should be roughly 2x uncalibrated (within tolerance)
        let ratio = total_cal as f64 / total_uncal as f64;
        assert!(
            (1.8..=2.2).contains(&ratio),
            "expected ratio ~2.0, got {ratio:.2} (uncal={total_uncal}, cal={total_cal})"
        );
    }

    #[test]
    fn spawner_small_dt_produces_fractional_spawns() {
        let mut od = OdMatrix::new();
        od.set_trips(Zone::BenThanh, Zone::NguyenHue, 10);
        let tod = TodProfile::new(vec![(0.0, 1.0), (24.0, 1.0)]);

        // dt=1s => expected = 10 * 1.0 * (1/3600) = 0.00278
        // Over 10000 iterations, we should see some spawns but not many
        let mut total = 0;
        for i in 0..10000 {
            let mut s = Spawner::new(od.clone(), tod.clone(), i);
            total += s.generate_spawns(12.0, 1.0).len();
        }
        // Expected ~27.8 spawns across 10000 runs
        assert!(total > 10 && total < 60, "Got {total} spawns");
    }
}
