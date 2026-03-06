//! Fixed-time signal controller.
//!
//! Advances elapsed simulation time and determines the current
//! `PhaseState` for each intersection approach based on the signal plan.

use crate::plan::{PhaseState, SignalPlan};

/// A fixed-time traffic signal controller.
///
/// Ticks simulation time and cycles through phases according to the plan.
/// One controller is instantiated per signalised intersection.
#[derive(Debug, Clone)]
pub struct FixedTimeController {
    /// The signal timing plan.
    plan: SignalPlan,
    /// Elapsed time within the current cycle (seconds).
    elapsed: f64,
    /// Total number of approaches at this intersection.
    num_approaches: usize,
}

impl FixedTimeController {
    /// Create a new controller for the given plan and approach count.
    pub fn new(plan: SignalPlan, num_approaches: usize) -> Self {
        Self {
            plan,
            elapsed: 0.0,
            num_approaches,
        }
    }

    /// Advance the controller by `dt` seconds.
    ///
    /// Wraps elapsed time around the cycle time automatically.
    pub fn tick(&mut self, dt: f64) {
        self.elapsed += dt;
        if self.plan.cycle_time > 0.0 {
            self.elapsed %= self.plan.cycle_time;
        }
    }

    /// Get the current phase state for the given approach index.
    ///
    /// Walks through the plan phases to find which phase is active
    /// at the current elapsed time, then checks whether the approach
    /// is served by that phase.
    pub fn get_phase_state(&self, approach_index: usize) -> PhaseState {
        if approach_index >= self.num_approaches {
            return PhaseState::Red;
        }

        let mut time_remaining = self.elapsed;

        for phase in &self.plan.phases {
            let phase_duration = phase.duration();
            if time_remaining < phase_duration {
                // We are within this phase
                if phase.approaches.contains(&approach_index) {
                    // This approach is served by the active phase
                    if time_remaining < phase.green_duration {
                        return PhaseState::Green;
                    } else {
                        return PhaseState::Amber;
                    }
                } else {
                    // Active phase does not serve this approach
                    return PhaseState::Red;
                }
            }
            time_remaining -= phase_duration;
        }

        // Should not reach here if elapsed is properly wrapped,
        // but default to Red for safety.
        PhaseState::Red
    }

    /// Reset the controller to the start of the cycle.
    pub fn reset(&mut self) {
        self.elapsed = 0.0;
    }

    /// Get the current elapsed time within the cycle.
    pub fn elapsed(&self) -> f64 {
        self.elapsed
    }

    /// Get a reference to the signal plan.
    pub fn plan(&self) -> &SignalPlan {
        &self.plan
    }
}
