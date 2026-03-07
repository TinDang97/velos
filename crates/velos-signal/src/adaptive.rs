//! Adaptive signal controller with queue-proportional green redistribution.
//!
//! An `AdaptiveController` adjusts green durations at the end of each cycle
//! based on queue lengths observed on each approach. Phases serving approaches
//! with longer queues receive proportionally more green time.
//!
//! During a cycle, the controller behaves like a fixed-time controller --
//! phase transitions occur at elapsed time boundaries. The adaptation
//! happens only at cycle boundaries.

use crate::detector::DetectorReading;
use crate::plan::{PhaseState, SignalPlan};
use crate::priority::{PriorityRequest, MAX_GREEN_EXTENSION, MAX_RED_SHORTENING};
use crate::spat::SpatBroadcast;
use crate::SignalController;

/// Adaptive traffic signal controller.
///
/// Redistributes green time proportional to queue lengths at each cycle end.
/// Within a cycle, phases transition at fixed elapsed-time boundaries
/// (same as `FixedTimeController`).
#[derive(Debug, Clone)]
pub struct AdaptiveController {
    /// The signal timing plan (green durations are mutated at cycle end).
    plan: SignalPlan,
    /// Elapsed time within the current cycle (seconds).
    elapsed: f64,
    /// Total number of approaches at the intersection.
    num_approaches: usize,
    /// Minimum green time per phase (seconds).
    min_green_per_phase: f64,
    /// Queue lengths per approach, updated externally each cycle.
    queue_lengths: Vec<u32>,
}

impl AdaptiveController {
    /// Create a new adaptive controller with default min_green = 7s.
    pub fn new(plan: SignalPlan, num_approaches: usize) -> Self {
        let queue_lengths = vec![0; num_approaches];
        Self {
            plan,
            elapsed: 0.0,
            num_approaches,
            min_green_per_phase: 7.0,
            queue_lengths,
        }
    }

    /// Update the queue lengths for each approach.
    ///
    /// Called externally (e.g., by the simulation engine) once per cycle.
    /// The redistribution occurs at the next cycle boundary.
    pub fn update_queue_lengths(&mut self, queue_lengths: &[u32]) {
        let len = queue_lengths.len().min(self.num_approaches);
        self.queue_lengths[..len].copy_from_slice(&queue_lengths[..len]);
    }

    /// Redistribute green time proportional to queue lengths.
    ///
    /// Called internally at cycle end. If all queues are zero, timing
    /// is left unchanged to avoid division by zero.
    fn redistribute_green(&mut self) {
        let phase_count = self.plan.phases.len();
        if phase_count == 0 {
            return;
        }

        // Compute queue sum per phase (sum of queues on its approaches).
        let phase_queues: Vec<f64> = self
            .plan
            .phases
            .iter()
            .map(|phase| {
                phase
                    .approaches
                    .iter()
                    .filter_map(|&a| self.queue_lengths.get(a))
                    .map(|&q| f64::from(q))
                    .sum::<f64>()
            })
            .collect();

        let total_queue: f64 = phase_queues.iter().sum();

        // If all queues are zero, keep previous timing unchanged.
        if total_queue <= 0.0 {
            return;
        }

        // Total available green time (sum of all phase green durations).
        let total_green: f64 = self.plan.phases.iter().map(|p| p.green_duration).sum();

        // First pass: compute proportional green, enforce minimum.
        let mut new_greens: Vec<f64> = phase_queues
            .iter()
            .map(|&q| (q / total_queue) * total_green)
            .collect();

        // Enforce min_green per phase.
        let mut deficit = 0.0_f64;
        let mut above_min_total = 0.0_f64;

        for green in &mut new_greens {
            if *green < self.min_green_per_phase {
                deficit += self.min_green_per_phase - *green;
                *green = self.min_green_per_phase;
            } else {
                above_min_total += *green;
            }
        }

        // Second pass: distribute deficit among phases above minimum.
        if deficit > 0.0 && above_min_total > 0.0 {
            for green in &mut new_greens {
                if *green > self.min_green_per_phase {
                    let share = (*green / above_min_total) * deficit;
                    *green -= share;
                    // Ensure we never go below min after redistribution
                    if *green < self.min_green_per_phase {
                        *green = self.min_green_per_phase;
                    }
                }
            }
        }

        // Apply new green durations.
        for (phase, &new_green) in self.plan.phases.iter_mut().zip(new_greens.iter()) {
            phase.green_duration = new_green;
        }

        // Recompute cycle time.
        self.plan.cycle_time = self.plan.phases.iter().map(|p| p.duration()).sum();
    }

    /// Get the current phase index and time-within-phase for the elapsed time.
    fn current_phase_info(&self) -> (usize, f64) {
        let mut time_remaining = self.elapsed;
        for (idx, phase) in self.plan.phases.iter().enumerate() {
            let phase_dur = phase.duration();
            if time_remaining < phase_dur {
                return (idx, time_remaining);
            }
            time_remaining -= phase_dur;
        }
        // Fallback: last phase
        let last = self.plan.phases.len().saturating_sub(1);
        (last, 0.0)
    }
}

impl SignalController for AdaptiveController {
    fn tick(&mut self, dt: f64, _detectors: &[DetectorReading]) {
        if self.plan.phases.is_empty() || self.plan.cycle_time <= 0.0 {
            return;
        }

        self.elapsed += dt;

        // Check for cycle wrap -- redistribute at cycle end.
        if self.elapsed >= self.plan.cycle_time {
            self.elapsed %= self.plan.cycle_time;
            self.redistribute_green();
        }
    }

    fn get_phase_state(&self, approach_index: usize) -> PhaseState {
        if approach_index >= self.num_approaches {
            return PhaseState::Red;
        }

        let (phase_idx, time_in_phase) = self.current_phase_info();
        let phase = &self.plan.phases[phase_idx];

        if phase.approaches.contains(&approach_index) {
            if time_in_phase < phase.green_duration {
                PhaseState::Green
            } else {
                PhaseState::Amber
            }
        } else {
            PhaseState::Red
        }
    }

    fn reset(&mut self) {
        self.elapsed = 0.0;
    }

    fn spat_data(&self, num_approaches: usize) -> SpatBroadcast {
        let approach_states = (0..num_approaches)
            .map(|i| self.get_phase_state(i))
            .collect();

        // Compute time to next phase change
        let (phase_idx, time_in_phase) = self.current_phase_info();
        let phase = &self.plan.phases[phase_idx];
        let time_to_next = (phase.duration() - time_in_phase).max(0.0);

        SpatBroadcast {
            approach_states,
            time_to_next_change: time_to_next,
            cycle_time: self.plan.cycle_time,
        }
    }

    fn request_priority(&mut self, request: &PriorityRequest) {
        let (phase_idx, _) = self.current_phase_info();
        let phase = &self.plan.phases[phase_idx];

        if phase.approaches.contains(&request.approach_index) {
            // Approach is green -- extend by up to MAX_GREEN_EXTENSION
            self.plan.phases[phase_idx].green_duration += MAX_GREEN_EXTENSION;
            self.plan.cycle_time = self.plan.phases.iter().map(|p| p.duration()).sum();
        } else {
            // Approach is red -- shorten current green by up to MAX_RED_SHORTENING
            let current_green = self.plan.phases[phase_idx].green_duration;
            let shortened = (current_green - MAX_RED_SHORTENING).max(self.min_green_per_phase);
            self.plan.phases[phase_idx].green_duration = shortened;
            self.plan.cycle_time = self.plan.phases.iter().map(|p| p.duration()).sum();
        }
    }
}
