//! Integration tests for SimWorld startup initialization (CPU-only path).
//!
//! Verifies that SimWorld::new_cpu_only() correctly initializes all subsystems:
//! polymorphic signal controllers, vehicle config, loop detectors, and reroute state.
//!
//! Requirements: SIG-01, SIG-02, TUN-02, SIG-05

use petgraph::graph::DiGraph;
use velos_gpu::sim::SimWorld;
use velos_net::graph::{RoadClass, RoadEdge, RoadGraph, RoadNode};
use velos_signal::plan::PhaseState;

/// Build a road graph with a single 4-way intersection (in_degree=4 at center).
fn make_signalized_graph() -> RoadGraph {
    let mut g = DiGraph::new();
    let center = g.add_node(RoadNode { pos: [0.0, 0.0] });
    let n = g.add_node(RoadNode { pos: [0.0, 100.0] });
    let s = g.add_node(RoadNode { pos: [0.0, -100.0] });
    let e = g.add_node(RoadNode { pos: [100.0, 0.0] });
    let w = g.add_node(RoadNode { pos: [-100.0, 0.0] });

    let edge = || RoadEdge {
        length_m: 100.0,
        speed_limit_mps: 13.9,
        lane_count: 2,
        oneway: true,
        road_class: RoadClass::Primary,
        geometry: vec![[0.0, 0.0], [100.0, 0.0]],
        motorbike_only: false,
        time_windows: None,
    };

    // 4 incoming edges to center -> qualifies as signalized intersection
    g.add_edge(n, center, edge());
    g.add_edge(s, center, edge());
    g.add_edge(e, center, edge());
    g.add_edge(w, center, edge());

    RoadGraph::new(g)
}

/// Build a simple linear graph (no signalized intersections).
fn make_linear_graph() -> RoadGraph {
    let mut g = DiGraph::new();
    let a = g.add_node(RoadNode { pos: [0.0, 0.0] });
    let b = g.add_node(RoadNode { pos: [100.0, 0.0] });
    let c = g.add_node(RoadNode { pos: [200.0, 0.0] });

    let edge = |from: [f64; 2], to: [f64; 2]| RoadEdge {
        length_m: ((to[0] - from[0]).powi(2) + (to[1] - from[1]).powi(2)).sqrt(),
        speed_limit_mps: 13.89,
        lane_count: 2,
        oneway: true,
        road_class: RoadClass::Secondary,
        geometry: vec![from, to],
        motorbike_only: false,
        time_windows: None,
    };

    g.add_edge(a, b, edge([0.0, 0.0], [100.0, 0.0]));
    g.add_edge(b, c, edge([100.0, 0.0], [200.0, 0.0]));

    RoadGraph::new(g)
}

// ---------------------------------------------------------------------------
// SIG-01: Signal controllers populated for signalized intersections
// ---------------------------------------------------------------------------

#[test]
fn startup_creates_signal_controllers_for_4way_intersections() {
    let graph = make_signalized_graph();
    let sim = SimWorld::new_cpu_only(graph);

    // A 4-way intersection (in_degree=4) should produce exactly 1 signal controller.
    assert_eq!(
        sim.signal_controllers.len(),
        1,
        "Should have 1 signal controller for the 4-way intersection"
    );
}

// ---------------------------------------------------------------------------
// SIG-02: Signal controllers are polymorphic Box<dyn SignalController>
// ---------------------------------------------------------------------------

#[test]
fn startup_signal_controllers_are_polymorphic_and_produce_phase_state() {
    let graph = make_signalized_graph();
    let sim = SimWorld::new_cpu_only(graph);

    // The controller should respond to get_phase_state via the trait.
    let (_, ctrl) = &sim.signal_controllers[0];
    let state = ctrl.get_phase_state(0);

    // Initial state should be Green (first phase starts green).
    assert_eq!(
        state,
        PhaseState::Green,
        "Initial phase state should be Green"
    );
}

#[test]
fn startup_signal_controllers_support_trait_methods() {
    let graph = make_signalized_graph();
    let sim = SimWorld::new_cpu_only(graph);

    let (_, ctrl) = &sim.signal_controllers[0];

    // Verify spat_data() works via trait dispatch (SIG-02 polymorphic requirement).
    let spat = ctrl.spat_data(4);
    assert_eq!(
        spat.approach_states.len(),
        4,
        "SPaT data should cover all 4 approaches"
    );
}

// ---------------------------------------------------------------------------
// TUN-02: Vehicle config loaded with valid values (verified indirectly)
// ---------------------------------------------------------------------------

#[test]
fn startup_loads_vehicle_config_verified_via_construction() {
    // vehicle_config is pub(crate), so we verify indirectly: the constructor
    // calls load_vehicle_config() and stores the result. If it loaded zeros
    // or failed, the sim would still construct but with VehicleConfig::default().
    // We verify the constructor doesn't panic and the sim is functional.
    let graph = make_linear_graph();
    let mut sim = SimWorld::new_cpu_only(graph);
    sim.sim_state = velos_gpu::sim::SimState::Running;

    // Tick once to verify vehicle config doesn't cause any runtime issues.
    // If vehicle_config had invalid (zero) params, physics would produce NaN/Inf.
    sim.tick(0.016);
    assert!(sim.sim_time > 0.0, "Sim time should advance after tick");
}

// ---------------------------------------------------------------------------
// SIG-05: Loop detectors only built for actuated intersections
// ---------------------------------------------------------------------------

#[test]
fn startup_with_default_config_has_no_detectors_verified_via_tick() {
    // Default config => all fixed-time => no loop detectors needed.
    // loop_detectors is pub(crate), but we can verify behavior:
    // tick() calls update_loop_detectors() which iterates loop_detectors.
    // With empty detectors, step_signals_with_detectors gets empty readings.
    // The signal should still advance purely on time (fixed-time behavior).
    let graph = make_signalized_graph();
    let mut sim = SimWorld::new_cpu_only(graph);
    sim.sim_state = velos_gpu::sim::SimState::Running;

    // Tick enough to advance past green phase (30s).
    // base_dt=0.1, speed_mult=2.0 => 0.2s per tick. 160 ticks = 32s.
    for _ in 0..170 {
        sim.tick(0.1);
    }

    // Signal should have transitioned (fixed-time, no detector dependence).
    let state = sim.signal_controllers[0].1.get_phase_state(0);
    assert_ne!(
        state,
        PhaseState::Green,
        "Fixed-time signal should transition without detectors"
    );
}

// ---------------------------------------------------------------------------
// Startup: Reroute state initialized (verified indirectly)
// ---------------------------------------------------------------------------

#[test]
fn startup_initializes_all_subsystems_without_panic() {
    // RerouteState, perception, loop_detectors are all pub(crate).
    // We verify the entire startup path completes without panicking,
    // which confirms all subsystems initialized successfully.
    let graph = make_signalized_graph();
    let sim = SimWorld::new_cpu_only(graph);

    assert_eq!(
        sim.sim_state,
        velos_gpu::sim::SimState::Stopped,
        "SimWorld should start in Stopped state"
    );
    assert!(
        !sim.signal_controllers.is_empty(),
        "Signal controllers should be populated"
    );
}

// ---------------------------------------------------------------------------
// Startup: No intersection => no controllers
// ---------------------------------------------------------------------------

#[test]
fn startup_no_controllers_for_graph_without_intersections() {
    let graph = make_linear_graph();
    let sim = SimWorld::new_cpu_only(graph);

    // Linear graph has no node with in_degree >= 4
    assert!(
        sim.signal_controllers.is_empty(),
        "Linear graph should have no signal controllers"
    );
}

// ---------------------------------------------------------------------------
// Startup: Missing config files degrade gracefully (no crash)
// ---------------------------------------------------------------------------

#[test]
fn startup_does_not_crash_with_missing_config_files() {
    // SimWorld::new_cpu_only should handle missing TOML files gracefully
    // by falling back to defaults. This test verifies no panic.
    let graph = make_linear_graph();
    let _sim = SimWorld::new_cpu_only(graph);
}

// ---------------------------------------------------------------------------
// Startup: Reset works correctly on initialized SimWorld
// ---------------------------------------------------------------------------

#[test]
fn startup_reset_restores_initial_state() {
    let graph = make_signalized_graph();
    let mut sim = SimWorld::new_cpu_only(graph);
    sim.sim_state = velos_gpu::sim::SimState::Running;

    // Advance signals.
    for _ in 0..200 {
        sim.tick(0.1);
    }

    sim.reset();

    // After reset, signal controllers should be back to initial Green.
    let state = sim.signal_controllers[0].1.get_phase_state(0);
    assert_eq!(
        state,
        PhaseState::Green,
        "Reset should restore signal controllers to initial state"
    );
    assert_eq!(
        sim.sim_state,
        velos_gpu::sim::SimState::Stopped,
        "Reset should set state to Stopped"
    );
}
