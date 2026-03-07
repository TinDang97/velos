//! Tests for AdaptiveController with queue-proportional green redistribution.

use velos_signal::adaptive::AdaptiveController;
use velos_signal::detector::DetectorReading;
use velos_signal::plan::{PhaseState, SignalPhase, SignalPlan};
use velos_signal::SignalController;

/// Create a 4-phase plan where each phase serves one approach.
///
/// Phase 0: approach 0, green 20s + amber 3s
/// Phase 1: approach 1, green 20s + amber 3s
/// Phase 2: approach 2, green 20s + amber 3s
/// Phase 3: approach 3, green 20s + amber 3s
/// Total cycle: 92s (80s green + 12s amber)
fn four_phase_plan() -> SignalPlan {
    SignalPlan::new(vec![
        SignalPhase {
            green_duration: 20.0,
            amber_duration: 3.0,
            approaches: vec![0],
        },
        SignalPhase {
            green_duration: 20.0,
            amber_duration: 3.0,
            approaches: vec![1],
        },
        SignalPhase {
            green_duration: 20.0,
            amber_duration: 3.0,
            approaches: vec![2],
        },
        SignalPhase {
            green_duration: 20.0,
            amber_duration: 3.0,
            approaches: vec![3],
        },
    ])
}

fn no_detectors() -> Vec<DetectorReading> {
    vec![]
}

#[test]
fn proportional_redistribution_with_known_queues() {
    let plan = four_phase_plan();
    let mut ctrl = AdaptiveController::new(plan, 4);

    // Queue lengths: [10, 30, 20, 40] total=100
    // Total green = 80s
    // Proportional: [8, 24, 16, 32]s
    ctrl.update_queue_lengths(&[10, 30, 20, 40]);

    // Tick through entire cycle to trigger redistribution
    // Use a single tick to avoid floating-point accumulation
    ctrl.tick(92.0, &no_detectors());

    // After cycle completes, redistribution happens.
    // Now verify the new green durations by observing phase transitions.
    ctrl.reset();

    // Phase 0 should get 8s green
    ctrl.tick(7.5, &no_detectors());
    assert_eq!(ctrl.get_phase_state(0), PhaseState::Green, "phase 0 green at 7.5s");
    // At 8.5s should be in amber (green=8s, so 8.5 > 8.0)
    ctrl.tick(1.0, &no_detectors());
    assert_eq!(ctrl.get_phase_state(0), PhaseState::Amber, "phase 0 amber at 8.5s");
}

#[test]
fn minimum_green_enforcement() {
    let plan = four_phase_plan();
    let mut ctrl = AdaptiveController::new(plan, 4);

    // Queue lengths: [1, 1, 1, 97] total=100
    // Proportional: [0.8, 0.8, 0.8, 77.6]s -- but min_green=7 enforced
    // After min enforcement: phases 0-2 get 7s each = 21s
    // Remaining = 80 - 21 = 59s for phase 3
    ctrl.update_queue_lengths(&[1, 1, 1, 97]);

    // Complete a full cycle to trigger redistribution
    ctrl.tick(92.0, &no_detectors());

    ctrl.reset();

    // Phase 0 should get min_green = 7s, not 0.8s
    ctrl.tick(6.9, &no_detectors());
    assert_eq!(ctrl.get_phase_state(0), PhaseState::Green, "phase 0 should be green at 6.9s (min_green)");
}

#[test]
fn equal_queues_equal_distribution() {
    let plan = four_phase_plan();
    let mut ctrl = AdaptiveController::new(plan, 4);

    // Equal queues: [25, 25, 25, 25]
    // Proportional: [20, 20, 20, 20]s -- same as original
    ctrl.update_queue_lengths(&[25, 25, 25, 25]);

    // Complete a cycle
    ctrl.tick(92.0, &no_detectors());

    ctrl.reset();

    // Phase 0 should still get 20s green
    ctrl.tick(19.9, &no_detectors());
    assert_eq!(ctrl.get_phase_state(0), PhaseState::Green, "phase 0 green at 19.9s with equal queues");
    ctrl.tick(0.2, &no_detectors());
    assert_eq!(ctrl.get_phase_state(0), PhaseState::Amber, "phase 0 amber at 20.1s with equal queues");
}

#[test]
fn zero_queues_unchanged_timing() {
    let plan = four_phase_plan();
    let mut ctrl = AdaptiveController::new(plan, 4);

    // All queues zero: keep previous timing
    ctrl.update_queue_lengths(&[0, 0, 0, 0]);

    // Complete a cycle
    ctrl.tick(92.0, &no_detectors());

    ctrl.reset();

    // Phase 0 should still have original 20s green
    ctrl.tick(19.9, &no_detectors());
    assert_eq!(ctrl.get_phase_state(0), PhaseState::Green, "phase 0 green at 19.9s with zero queues");
    ctrl.tick(0.2, &no_detectors());
    assert_eq!(ctrl.get_phase_state(0), PhaseState::Amber, "phase 0 amber at 20.1s with zero queues");
}

#[test]
fn full_cycle_phase_transitions() {
    let plan = four_phase_plan();
    let mut ctrl = AdaptiveController::new(plan, 4);

    // At t=0: phase 0 green
    assert_eq!(ctrl.get_phase_state(0), PhaseState::Green);
    assert_eq!(ctrl.get_phase_state(1), PhaseState::Red);

    // At t=20: phase 0 amber
    ctrl.tick(20.0, &no_detectors());
    assert_eq!(ctrl.get_phase_state(0), PhaseState::Amber);

    // At t=23: phase 1 green
    ctrl.tick(3.0, &no_detectors());
    assert_eq!(ctrl.get_phase_state(0), PhaseState::Red);
    assert_eq!(ctrl.get_phase_state(1), PhaseState::Green);

    // At t=43: phase 1 amber
    ctrl.tick(20.0, &no_detectors());
    assert_eq!(ctrl.get_phase_state(1), PhaseState::Amber);

    // At t=46: phase 2 green
    ctrl.tick(3.0, &no_detectors());
    assert_eq!(ctrl.get_phase_state(2), PhaseState::Green);
}

#[test]
fn implements_signal_controller_trait() {
    let plan = four_phase_plan();
    let mut ctrl = AdaptiveController::new(plan, 4);

    let _state = ctrl.get_phase_state(0);
    ctrl.tick(1.0, &no_detectors());
    ctrl.reset();
    assert_eq!(ctrl.get_phase_state(0), PhaseState::Green, "green after reset");
}

#[test]
fn two_phase_proportional_redistribution() {
    // Simpler 2-phase plan to verify math
    let plan = SignalPlan::new(vec![
        SignalPhase {
            green_duration: 30.0,
            amber_duration: 3.0,
            approaches: vec![0, 1],
        },
        SignalPhase {
            green_duration: 30.0,
            amber_duration: 3.0,
            approaches: vec![2, 3],
        },
    ]);
    let mut ctrl = AdaptiveController::new(plan, 4);

    // Approaches 0,1 have queue 10 each -> phase 0 queue sum = 20
    // Approaches 2,3 have queue 40 each -> phase 1 queue sum = 80
    // Total queue = 100, total green = 60s
    // Phase 0: 20/100 * 60 = 12s (above min_green=7)
    // Phase 1: 80/100 * 60 = 48s
    ctrl.update_queue_lengths(&[10, 10, 40, 40]);

    // Complete cycle (66s = 60s green + 6s amber)
    ctrl.tick(66.0, &no_detectors());

    ctrl.reset();

    // Phase 0 should get 12s green
    ctrl.tick(11.9, &no_detectors());
    assert_eq!(ctrl.get_phase_state(0), PhaseState::Green, "phase 0 green at 11.9s");
    ctrl.tick(0.2, &no_detectors());
    assert_eq!(ctrl.get_phase_state(0), PhaseState::Amber, "phase 0 amber at 12.1s");
}
