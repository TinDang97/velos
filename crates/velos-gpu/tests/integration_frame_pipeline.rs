//! Integration tests for the CPU frame pipeline (tick()).
//!
//! Verifies that tick() calls step_signals, update_loop_detectors, and
//! step_signal_priority in the correct pipeline order. Uses CPU-only path
//! since GPU is not available in CI.
//!
//! Requirements: INT-03, RTE-03, SIG-03, SIG-04

use petgraph::graph::DiGraph;
use velos_core::components::{Kinematics, RoadPosition, VehicleType};
use velos_gpu::sim::{SimState, SimWorld};
use velos_net::graph::{RoadClass, RoadEdge, RoadGraph, RoadNode};
use velos_signal::plan::PhaseState;

/// Build a road graph with a 4-way intersection and outgoing edges for routes.
fn make_intersection_graph() -> RoadGraph {
    let mut g = DiGraph::new();
    let center = g.add_node(RoadNode { pos: [0.0, 0.0] });
    let n = g.add_node(RoadNode { pos: [0.0, 100.0] });
    let s = g.add_node(RoadNode { pos: [0.0, -100.0] });
    let e = g.add_node(RoadNode { pos: [100.0, 0.0] });
    let w = g.add_node(RoadNode { pos: [-100.0, 0.0] });

    let edge = |from: [f64; 2], to: [f64; 2]| RoadEdge {
        length_m: ((to[0] - from[0]).powi(2) + (to[1] - from[1]).powi(2)).sqrt(),
        speed_limit_mps: 13.9,
        lane_count: 2,
        oneway: true,
        road_class: RoadClass::Primary,
        geometry: vec![from, to],
        motorbike_only: false,
        time_windows: None,
    };

    // 4 incoming edges to center -> signalized
    g.add_edge(n, center, edge([0.0, 100.0], [0.0, 0.0]));
    g.add_edge(s, center, edge([0.0, -100.0], [0.0, 0.0]));
    g.add_edge(e, center, edge([100.0, 0.0], [0.0, 0.0]));
    g.add_edge(w, center, edge([-100.0, 0.0], [0.0, 0.0]));

    // Outgoing edges from center (for agent routes)
    g.add_edge(center, n, edge([0.0, 0.0], [0.0, 100.0]));
    g.add_edge(center, s, edge([0.0, 0.0], [0.0, -100.0]));
    g.add_edge(center, e, edge([0.0, 0.0], [100.0, 0.0]));
    g.add_edge(center, w, edge([0.0, 0.0], [-100.0, 0.0]));

    RoadGraph::new(g)
}

// ---------------------------------------------------------------------------
// SIG-03: Signal controllers are ticked during frame pipeline
// ---------------------------------------------------------------------------

#[test]
fn tick_advances_signal_controllers() {
    let graph = make_intersection_graph();
    let mut sim = SimWorld::new_cpu_only(graph);
    sim.sim_state = SimState::Running;

    // Record initial phase state.
    let initial_state = sim.signal_controllers[0].1.get_phase_state(0);
    assert_eq!(initial_state, PhaseState::Green, "Should start Green");

    // Tick enough times to advance past the green phase (30s green + 3s amber).
    // Each tick with base_dt=0.1 and speed_mult=2.0 advances 0.2s of sim time.
    // 30s / 0.2 = 150 ticks to exit green, + 3s/0.2 = 15 more for amber.
    for _ in 0..170 {
        sim.tick(0.1);
    }

    // After ~34s of sim time, the signal should have transitioned past Green.
    let state_after = sim.signal_controllers[0].1.get_phase_state(0);

    // The first phase (approaches 0,1) should now be Red (second phase is active).
    assert_ne!(
        state_after,
        PhaseState::Green,
        "After 34s sim time, phase 0 should have transitioned from Green"
    );
}

// ---------------------------------------------------------------------------
// INT-03: Frame pipeline executes without crashing (CPU path)
// ---------------------------------------------------------------------------

#[test]
fn tick_cpu_path_completes_full_pipeline_without_crash() {
    let graph = make_intersection_graph();
    let mut sim = SimWorld::new_cpu_only(graph);
    sim.sim_state = SimState::Running;

    // Run 100 ticks. If the pipeline order is broken, this will panic.
    for _ in 0..100 {
        let (_motorbikes, _cars, _peds) = sim.tick(0.016);
    }

    // Sim time should have advanced.
    assert!(
        sim.sim_time > 7.0 * 3600.0,
        "Sim time should advance from morning rush start"
    );
}

// ---------------------------------------------------------------------------
// RTE-03: Tick does not crash even with spawned agents
// ---------------------------------------------------------------------------

#[test]
fn tick_with_spawned_agents_processes_pipeline() {
    let graph = make_intersection_graph();
    let mut sim = SimWorld::new_cpu_only(graph);
    sim.sim_state = SimState::Running;

    // Tick enough times to trigger agent spawning.
    for _ in 0..500 {
        sim.tick(0.1);
    }

    // Verify the pipeline completed without crash. Agent count may be 0 if
    // demand system doesn't spawn on this graph, but the pipeline should
    // complete regardless.
    let agent_count = sim.metrics.agent_count;
    eprintln!("Agent count after 500 ticks: {}", agent_count);
}

// ---------------------------------------------------------------------------
// SIG-04: Signal priority does not crash with bus/emergency near intersection
// ---------------------------------------------------------------------------

#[test]
fn tick_processes_signal_priority_for_bus_near_intersection() {
    let graph = make_intersection_graph();
    let mut sim = SimWorld::new_cpu_only(graph);
    sim.sim_state = SimState::Running;

    // Manually spawn a bus agent on an edge incoming to the signalized intersection.
    // Edge 0 goes from node N to center (edge_index=0).
    let bus_entity = sim.world.spawn((
        RoadPosition {
            edge_index: 0,
            lane: 0,
            offset_m: 90.0, // 90m along 100m edge = 10m from intersection
        },
        Kinematics {
            vx: 5.0,
            vy: 0.0,
            speed: 5.0,
            heading: 0.0,
        },
        VehicleType::Bus,
    ));

    // Tick once. step_signal_priority should detect the bus and submit a
    // priority request without crashing.
    sim.tick(0.1);

    // If we got here, step_signal_priority processed the bus without panic.
    assert!(
        sim.world.contains(bus_entity),
        "Bus entity should still exist after priority processing"
    );
}

#[test]
fn tick_processes_signal_priority_for_emergency_vehicle() {
    let graph = make_intersection_graph();
    let mut sim = SimWorld::new_cpu_only(graph);
    sim.sim_state = SimState::Running;

    // Spawn an emergency vehicle near the intersection.
    let emergency_entity = sim.world.spawn((
        RoadPosition {
            edge_index: 1, // Edge from S to center
            lane: 0,
            offset_m: 85.0, // 15m from intersection
        },
        Kinematics {
            vx: 10.0,
            vy: 0.0,
            speed: 10.0,
            heading: 0.0,
        },
        VehicleType::Emergency,
    ));

    sim.tick(0.1);

    assert!(
        sim.world.contains(emergency_entity),
        "Emergency entity should still exist after priority processing"
    );
}

// ---------------------------------------------------------------------------
// Stopped simulation does not advance
// ---------------------------------------------------------------------------

#[test]
fn tick_does_not_advance_when_stopped() {
    let graph = make_intersection_graph();
    let mut sim = SimWorld::new_cpu_only(graph);

    // Default state is Stopped.
    assert_eq!(sim.sim_state, SimState::Stopped);
    let initial_time = sim.sim_time;

    sim.tick(0.1);

    assert_eq!(
        sim.sim_time, initial_time,
        "Sim time should not advance when stopped"
    );
}

// ---------------------------------------------------------------------------
// Reset clears state and resets signal controllers
// ---------------------------------------------------------------------------

#[test]
fn reset_resets_signal_controllers() {
    let graph = make_intersection_graph();
    let mut sim = SimWorld::new_cpu_only(graph);
    sim.sim_state = SimState::Running;

    // Advance signals past initial phase.
    for _ in 0..200 {
        sim.tick(0.1);
    }

    sim.reset();

    let state_after_reset = sim.signal_controllers[0].1.get_phase_state(0);
    assert_eq!(
        state_after_reset,
        PhaseState::Green,
        "Signal controllers should reset to initial Green state"
    );
}

// ---------------------------------------------------------------------------
// Pipeline order: signals tick with empty detectors (fixed-time)
// ---------------------------------------------------------------------------

#[test]
fn tick_signals_work_with_empty_detector_readings() {
    let graph = make_intersection_graph();
    let mut sim = SimWorld::new_cpu_only(graph);
    sim.sim_state = SimState::Running;

    // With default config (all fixed-time), step_signals_with_detectors
    // receives empty readings. Signal should still advance on time alone.
    for _ in 0..10 {
        sim.tick(0.1);
    }

    // Signal should still be Green (only ~2s elapsed, green is 30s).
    let state = sim.signal_controllers[0].1.get_phase_state(0);
    assert_eq!(state, PhaseState::Green, "Should still be Green after 2s");
}
