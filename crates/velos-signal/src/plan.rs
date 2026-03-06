//! Signal plan and phase definitions.
//!
//! A `SignalPlan` consists of ordered phases. Each phase grants green
//! to a set of intersection approaches, followed by amber, then red
//! (while the next phase begins its green).

/// Current state of a signal phase for a given approach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseState {
    /// Approach has right of way.
    Green,
    /// Transition warning; vehicles should stop if safe.
    Amber,
    /// Approach must stop.
    Red,
}

/// A single phase in a signal plan.
///
/// During this phase, the listed approaches receive green for
/// `green_duration` seconds, then amber for `amber_duration` seconds.
/// All other approaches remain red.
#[derive(Debug, Clone)]
pub struct SignalPhase {
    /// Duration of green indication (seconds).
    pub green_duration: f64,
    /// Duration of amber/yellow indication (seconds).
    pub amber_duration: f64,
    /// Approach indices that receive green during this phase.
    pub approaches: Vec<usize>,
}

impl SignalPhase {
    /// Total duration of this phase (green + amber).
    pub fn duration(&self) -> f64 {
        self.green_duration + self.amber_duration
    }
}

/// A fixed-time signal timing plan.
///
/// Phases are executed in order; after the last phase completes
/// the cycle repeats from the first phase.
#[derive(Debug, Clone)]
pub struct SignalPlan {
    /// Ordered list of signal phases.
    pub phases: Vec<SignalPhase>,
    /// Total cycle time (sum of all phase durations).
    pub cycle_time: f64,
}

impl SignalPlan {
    /// Create a new signal plan from the given phases.
    ///
    /// Computes `cycle_time` as the sum of all phase durations.
    pub fn new(phases: Vec<SignalPhase>) -> Self {
        let cycle_time: f64 = phases.iter().map(|p| p.duration()).sum();
        Self {
            phases,
            cycle_time,
        }
    }
}
