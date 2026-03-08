//! Vehicle type definitions and default parameter sets.

use crate::config::VehicleConfig;
use crate::idm::IdmParams;
use crate::mobil::MobilParams;

/// Classification of simulated vehicle/agent types.
///
/// Order must match velos-core VehicleType and WGSL constants in wave_front.wgsl.
/// GPU mapping: 0=Motorbike, 1=Car, 2=Bus, 3=Bicycle, 4=Truck, 5=Emergency, 6=Pedestrian.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VehicleType {
    /// Two-wheeled motorbike (dominant in HCMC, ~80% of traffic). GPU=0.
    Motorbike,
    /// Four-wheeled car (~15% of traffic). GPU=1.
    Car,
    /// Public transit bus. GPU=2.
    Bus,
    /// Bicycle (pedal-powered, uses sublane model with IDM). GPU=3.
    Bicycle,
    /// Heavy goods vehicle / truck. GPU=4.
    Truck,
    /// Emergency vehicle (ambulance, fire truck). GPU=5.
    Emergency,
    /// Pedestrian agent (~5% of traffic). GPU=6.
    Pedestrian,
}

/// Return the default IDM parameters for a given vehicle type.
///
/// Values are HCMC-calibrated defaults matching `VehicleConfig::default()`.
/// For config-driven loading, use [`default_idm_params_from_config`] instead.
pub fn default_idm_params(vehicle_type: VehicleType) -> IdmParams {
    let config = VehicleConfig::default();
    default_idm_params_from_config(vehicle_type, &config)
}

/// Return IDM parameters for a vehicle type from a loaded config.
pub fn default_idm_params_from_config(vehicle_type: VehicleType, config: &VehicleConfig) -> IdmParams {
    match vehicle_type {
        VehicleType::Pedestrian => {
            // Pedestrian uses social force model primarily, but IDM is used for
            // longitudinal following in some contexts.
            IdmParams {
                v0: 1.4,
                s0: 0.5,
                t_headway: 0.5,
                a: 0.5,
                b: 1.0,
                delta: 4.0,
            }
        }
        other => config.for_vehicle_type(other).to_idm_params(),
    }
}

/// Return the default MOBIL lane-change parameters for HCMC traffic (car defaults).
///
/// For per-vehicle-type MOBIL params, use [`default_mobil_params_for_type`].
pub fn default_mobil_params() -> MobilParams {
    let config = VehicleConfig::default();
    config.car.to_mobil_params()
}

/// Return MOBIL parameters for a specific vehicle type from config.
pub fn default_mobil_params_for_type(
    vehicle_type: VehicleType,
    config: &VehicleConfig,
) -> MobilParams {
    match vehicle_type {
        VehicleType::Pedestrian => {
            // Pedestrians don't lane-change, but provide sensible defaults
            MobilParams {
                politeness: 0.5,
                threshold: 0.2,
                safe_decel: -4.0,
                right_bias: 0.0,
            }
        }
        other => config.for_vehicle_type(other).to_mobil_params(),
    }
}

/// Return per-vehicle-type MOBIL params using built-in HCMC defaults.
pub fn default_mobil_params_for_type_builtin(vehicle_type: VehicleType) -> MobilParams {
    let config = VehicleConfig::default();
    default_mobil_params_for_type(vehicle_type, &config)
}
