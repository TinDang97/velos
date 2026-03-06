//! Agent spawner combining OD matrices and ToD profiles to generate traffic demand.

use rand::rngs::StdRng;
use rand::SeedableRng;

use crate::od_matrix::{OdMatrix, Zone};
use crate::tod_profile::TodProfile;

/// Vehicle type for spawn requests. Kept local to avoid circular dependency
/// with velos-vehicle. The integration layer (02-04) maps to the real enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpawnVehicleType {
    Motorbike,
    Car,
    Pedestrian,
}

/// A request to spawn an agent at a specific origin heading to a destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnRequest {
    /// Origin traffic analysis zone.
    pub origin: Zone,
    /// Destination traffic analysis zone.
    pub destination: Zone,
    /// Type of vehicle/agent to spawn.
    pub vehicle_type: SpawnVehicleType,
}

/// Combines an OD matrix and ToD profile to generate stochastic spawn requests.
///
/// Uses a seeded RNG for reproducibility across simulation runs.
pub struct Spawner {
    od: OdMatrix,
    tod: TodProfile,
    rng: StdRng,
}

impl Spawner {
    /// Create a spawner with the given OD matrix, ToD profile, and RNG seed.
    pub fn new(od: OdMatrix, tod: TodProfile, seed: u64) -> Self {
        Self {
            od,
            tod,
            rng: StdRng::seed_from_u64(seed),
        }
    }

    /// Generate spawn requests for a given simulation hour and timestep (seconds).
    ///
    /// For each OD pair, expected spawns = trips_per_hour * tod_factor * (dt / 3600).
    /// Uses Bernoulli sampling for fractional expected counts.
    pub fn generate_spawns(&mut self, _sim_hour: f64, _dt: f64) -> Vec<SpawnRequest> {
        todo!("implement generate_spawns")
    }
}
