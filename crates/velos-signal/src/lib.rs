//! Traffic signal controllers for VELOS microsimulation.
//!
//! This crate provides:
//! - **SignalPlan** and **SignalPhase** for defining signal timing
//! - **FixedTimeController** for cycling through green/amber/red phases
//! - **PhaseState** enum for querying current signal indication

pub mod controller;
pub mod error;
pub mod plan;
