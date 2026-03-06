//! Tests for fixed-time signal controller.

use velos_signal::controller::FixedTimeController;
use velos_signal::plan::{PhaseState, SignalPhase, SignalPlan};

/// Create a standard 2-phase signal plan for testing.
///
/// Phase 0: NS approaches (indices 0,1) green 30s + amber 3s = 33s
/// Phase 1: EW approaches (indices 2,3) green 25s + amber 3s = 28s
/// Total cycle: 61s
fn two_phase_plan() -> (SignalPlan, FixedTimeController) {
    let phases = vec![
        SignalPhase {
            green_duration: 30.0,
            amber_duration: 3.0,
            approaches: vec![0, 1], // NS
        },
        SignalPhase {
            green_duration: 25.0,
            amber_duration: 3.0,
            approaches: vec![2, 3], // EW
        },
    ];
    let plan = SignalPlan::new(phases);
    assert!((plan.cycle_time - 61.0).abs() < 1e-10, "cycle_time should be 61s");
    let ctrl = FixedTimeController::new(plan.clone(), 4);
    (plan, ctrl)
}

#[test]
fn cycle_time_computed_correctly() {
    let (plan, _) = two_phase_plan();
    assert!((plan.cycle_time - 61.0).abs() < 1e-10);
}

#[test]
fn at_t0_ns_is_green_ew_is_red() {
    let (_, ctrl) = two_phase_plan();
    // t=0: NS phase active, green
    assert_eq!(ctrl.get_phase_state(0), PhaseState::Green, "NS approach 0 at t=0");
    assert_eq!(ctrl.get_phase_state(1), PhaseState::Green, "NS approach 1 at t=0");
    assert_eq!(ctrl.get_phase_state(2), PhaseState::Red, "EW approach 2 at t=0");
    assert_eq!(ctrl.get_phase_state(3), PhaseState::Red, "EW approach 3 at t=0");
}

#[test]
fn at_t30_ns_is_amber() {
    let (_, mut ctrl) = two_phase_plan();
    ctrl.tick(30.0);
    // t=30: NS phase, green ended, amber started
    assert_eq!(ctrl.get_phase_state(0), PhaseState::Amber, "NS at t=30 should be amber");
    assert_eq!(ctrl.get_phase_state(2), PhaseState::Red, "EW at t=30 still red");
}

#[test]
fn at_t33_ew_is_green() {
    let (_, mut ctrl) = two_phase_plan();
    ctrl.tick(33.0);
    // t=33: NS phase ended (30+3), EW phase starts green
    assert_eq!(ctrl.get_phase_state(0), PhaseState::Red, "NS at t=33 should be red");
    assert_eq!(ctrl.get_phase_state(2), PhaseState::Green, "EW at t=33 should be green");
    assert_eq!(ctrl.get_phase_state(3), PhaseState::Green, "EW approach 3 at t=33");
}

#[test]
fn at_t58_ew_is_amber() {
    let (_, mut ctrl) = two_phase_plan();
    ctrl.tick(58.0);
    // t=58: EW phase, elapsed within EW = 58-33 = 25s, green=25s so amber starts
    assert_eq!(ctrl.get_phase_state(0), PhaseState::Red, "NS at t=58");
    assert_eq!(ctrl.get_phase_state(2), PhaseState::Amber, "EW at t=58 should be amber");
}

#[test]
fn cycle_wraps_around() {
    let (_, mut ctrl) = two_phase_plan();
    ctrl.tick(61.0);
    // t=61: wraps to t=0, NS green again
    // Due to float modulo, elapsed should be ~0.0
    assert_eq!(ctrl.get_phase_state(0), PhaseState::Green, "NS at t=61 (wrap) should be green");
    assert_eq!(ctrl.get_phase_state(2), PhaseState::Red, "EW at t=61 (wrap) should be red");
}

#[test]
fn multiple_cycles() {
    let (_, mut ctrl) = two_phase_plan();
    // After 2 full cycles + 35s = 122 + 35 = 157s
    ctrl.tick(157.0);
    // 157 % 61 = 157 - 2*61 = 35, so elapsed=35
    // Phase 0 (NS) = 33s, so 35-33=2s into phase 1 (EW) -> EW green
    assert_eq!(ctrl.get_phase_state(0), PhaseState::Red, "NS at t=157");
    assert_eq!(ctrl.get_phase_state(2), PhaseState::Green, "EW at t=157 should be green");
}

#[test]
fn reset_returns_to_initial_state() {
    let (_, mut ctrl) = two_phase_plan();
    ctrl.tick(45.0);
    assert_eq!(ctrl.get_phase_state(0), PhaseState::Red, "NS should be red at t=45");
    ctrl.reset();
    assert_eq!(ctrl.get_phase_state(0), PhaseState::Green, "NS green after reset");
    assert!((ctrl.elapsed() - 0.0).abs() < 1e-10, "elapsed should be 0 after reset");
}

#[test]
fn incremental_ticks_match_single_tick() {
    let (_, mut ctrl_single) = two_phase_plan();
    let (_, mut ctrl_inc) = two_phase_plan();

    ctrl_single.tick(45.0);

    // 450 ticks of 0.1s each
    for _ in 0..450 {
        ctrl_inc.tick(0.1);
    }

    // Both should show same state
    for approach in 0..4 {
        assert_eq!(
            ctrl_single.get_phase_state(approach),
            ctrl_inc.get_phase_state(approach),
            "approach {approach}: single tick vs incremental should match"
        );
    }
}

#[test]
fn out_of_range_approach_returns_red() {
    let (_, ctrl) = two_phase_plan();
    assert_eq!(ctrl.get_phase_state(10), PhaseState::Red, "invalid approach should be Red");
}

#[test]
fn single_phase_plan() {
    let phases = vec![SignalPhase {
        green_duration: 50.0,
        amber_duration: 5.0,
        approaches: vec![0],
    }];
    let plan = SignalPlan::new(phases);
    let mut ctrl = FixedTimeController::new(plan, 2);

    assert_eq!(ctrl.get_phase_state(0), PhaseState::Green);
    assert_eq!(ctrl.get_phase_state(1), PhaseState::Red);

    ctrl.tick(50.0);
    assert_eq!(ctrl.get_phase_state(0), PhaseState::Amber);

    ctrl.tick(5.0);
    // Wraps around: 55 % 55 = 0 -> green again
    assert_eq!(ctrl.get_phase_state(0), PhaseState::Green);
}
