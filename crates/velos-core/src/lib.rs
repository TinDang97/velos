//! velos-core: ECS components and numerical utilities shared by all crates.

pub mod cfl;
pub mod components;
pub mod error;

pub use cfl::cfl_check;
pub use components::{Kinematics, Position, RoadPosition, Route, VehicleType, WaitState};
pub use error::CoreError;
