//! Integration tests for MOBIL lane-change, motorbike sublane filtering,
//! and prediction overlay update in the GPU tick loop.
//!
//! All tests use CPU-only path (no GPU device needed) since step_lane_changes,
//! step_motorbikes_sublane, and step_prediction are CPU functions.

use petgraph::graph::DiGraph;
use velos_core::components::{
    Kinematics, LaneChangeState, LateralOffset, Position, RoadPosition, VehicleType, WaitState,
};
use velos_gpu::sim::{SimState, SimWorld};
use velos_net::graph::{RoadClass, RoadEdge, RoadGraph, RoadNode};
use velos_predict::PredictionService;
use velos_vehicle::idm::IdmParams;

/// Build a road graph with a single 2-lane edge (200m).
fn make_2lane_graph() -> RoadGraph {
    let mut g = DiGraph::new();
    let a = g.add_node(RoadNode { pos: [0.0, 0.0] });
    let b = g.add_node(RoadNode { pos: [200.0, 0.0] });
    g.add_edge(
        a,
        b,
        RoadEdge {
            length_m: 200.0,
            speed_limit_mps: 13.9,
            lane_count: 2,
            oneway: true,
            road_class: RoadClass::Primary,
            geometry: vec![[0.0, 0.0], [200.0, 0.0]],
            motorbike_only: false,
            time_windows: None,
        },
    );
    RoadGraph::new(g)
}

/// Build a road graph with a single 1-lane edge (200m).
fn make_1lane_graph() -> RoadGraph {
    let mut g = DiGraph::new();
    let a = g.add_node(RoadNode { pos: [0.0, 0.0] });
    let b = g.add_node(RoadNode { pos: [200.0, 0.0] });
    g.add_edge(
        a,
        b,
        RoadEdge {
            length_m: 200.0,
            speed_limit_mps: 13.9,
            lane_count: 1,
            oneway: true,
            road_class: RoadClass::Primary,
            geometry: vec![[0.0, 0.0], [200.0, 0.0]],
            motorbike_only: false,
            time_windows: None,
        },
    );
    RoadGraph::new(g)
}

fn default_idm() -> IdmParams {
    IdmParams {
        v0: 13.89,
        s0: 2.0,
        t_headway: 1.6,
        a: 1.0,
        b: 2.0,
        delta: 4.0,
    }
}

fn spawn_car(
    sim: &mut SimWorld,
    edge: u32,
    lane: u8,
    offset: f64,
    speed: f64,
) -> hecs::Entity {
    sim.world.spawn((
        RoadPosition {
            edge_index: edge,
            lane,
            offset_m: offset,
        },
        Kinematics {
            vx: speed,
            vy: 0.0,
            speed,
            heading: 0.0,
        },
        default_idm(),
        VehicleType::Car,
        Position {
            x: offset,
            y: (lane as f64) * 3.5 + 1.75,
        },
        LateralOffset {
            lateral_offset: (lane as f64 + 0.5) * 3.5,
            desired_lateral: (lane as f64 + 0.5) * 3.5,
        },
        WaitState {
            stopped_since: -1.0,
            at_red_signal: false,
        },
    ))
}

fn spawn_motorbike(
    sim: &mut SimWorld,
    edge: u32,
    lane: u8,
    offset: f64,
    speed: f64,
    lateral: f64,
) -> hecs::Entity {
    sim.world.spawn((
        RoadPosition {
            edge_index: edge,
            lane,
            offset_m: offset,
        },
        Kinematics {
            vx: speed,
            vy: 0.0,
            speed,
            heading: 0.0,
        },
        IdmParams {
            v0: 11.0,
            s0: 1.0,
            t_headway: 0.8,
            a: 2.0,
            b: 3.0,
            delta: 4.0,
        },
        VehicleType::Motorbike,
        Position {
            x: offset,
            y: lateral,
        },
        LateralOffset {
            lateral_offset: lateral,
            desired_lateral: lateral,
        },
        WaitState {
            stopped_since: -1.0,
            at_red_signal: false,
        },
    ))
}

/// Test: car behind slow leader on multi-lane edge triggers MOBIL and gets LaneChangeState.
#[test]
fn test_mobil_triggers_lane_change() {
    let graph = make_2lane_graph();
    let mut sim = SimWorld::new_cpu_only(graph);
    sim.sim_state = SimState::Running;

    // Slow car ahead at offset 100m, speed 2.0 m/s
    spawn_car(&mut sim, 0, 0, 100.0, 2.0);
    // Fast car behind at offset 80m, speed 10.0 m/s
    let fast = spawn_car(&mut sim, 0, 0, 80.0, 10.0);

    sim.step_lane_changes(0.1);

    let has_lcs = sim
        .world
        .query_one_mut::<&LaneChangeState>(fast)
        .is_ok();
    assert!(
        has_lcs,
        "fast car behind slow leader should trigger MOBIL and get LaneChangeState"
    );

    // Verify target lane is 1 (the other lane)
    let lcs = sim.world.query_one_mut::<&LaneChangeState>(fast).unwrap();
    assert_eq!(lcs.target_lane, 1, "should change to lane 1");
}

/// Test: after 2 seconds of ticks (20 ticks at 0.1s), LaneChangeState is removed
/// and the car's lane is updated to the target lane.
#[test]
fn test_lane_change_completes_after_drift() {
    let graph = make_2lane_graph();
    let mut sim = SimWorld::new_cpu_only(graph);
    sim.sim_state = SimState::Running;

    // Single car, manually start lane change
    let car = spawn_car(&mut sim, 0, 0, 50.0, 10.0);
    sim.start_lane_change(car, 1, sim.sim_time);

    // Tick 25 times (2.5s > 2.0s drift duration)
    for _ in 0..25 {
        sim.step_lane_changes(0.1);
    }

    // LaneChangeState should be removed
    let has_lcs = sim
        .world
        .query_one_mut::<&LaneChangeState>(car)
        .is_ok();
    assert!(
        !has_lcs,
        "LaneChangeState should be removed after drift completes"
    );

    // Lane should be updated to target (1)
    let rp = sim.world.query_one_mut::<&RoadPosition>(car).unwrap();
    assert_eq!(rp.lane, 1, "car should now be in lane 1 after drift");
}

/// Test: single-lane edge does not trigger MOBIL (no adjacent lanes).
#[test]
fn test_single_lane_no_mobil() {
    let graph = make_1lane_graph();
    let mut sim = SimWorld::new_cpu_only(graph);
    sim.sim_state = SimState::Running;

    // Slow car ahead
    spawn_car(&mut sim, 0, 0, 100.0, 2.0);
    // Fast car behind
    let fast = spawn_car(&mut sim, 0, 0, 80.0, 10.0);

    sim.step_lane_changes(0.1);

    let has_lcs = sim
        .world
        .query_one_mut::<&LaneChangeState>(fast)
        .is_ok();
    assert!(
        !has_lcs,
        "single-lane road should not trigger MOBIL lane change"
    );
}

/// Test: motorbike on multi-lane edge has lateral offset adjusted by sublane filtering.
#[test]
fn test_motorbike_sublane_adjusts_lateral() {
    use velos_net::SpatialIndex;
    use velos_gpu::sim_snapshot::AgentSnapshot;
    use velos_gpu::cpu_reference;

    let graph = make_2lane_graph();
    let mut sim = SimWorld::new_cpu_only(graph);
    sim.sim_state = SimState::Running;

    // Place a motorbike with initial lateral offset of 1.0 (not centered)
    let bike = spawn_motorbike(&mut sim, 0, 0, 50.0, 5.0, 1.0);

    // Also spawn a neighbor motorbike to create interaction
    spawn_motorbike(&mut sim, 0, 0, 55.0, 3.0, 1.5);

    let snapshot = AgentSnapshot::collect(&sim.world);
    let spatial = SpatialIndex::from_positions(&snapshot.ids, &snapshot.positions);

    cpu_reference::step_motorbikes_sublane(&mut sim, 0.1, &spatial, &snapshot);

    let lat = sim
        .world
        .query_one_mut::<&LateralOffset>(bike)
        .unwrap();

    // Lateral offset should have changed from 1.0 (sublane filtering adjusts it)
    assert!(
        (lat.lateral_offset - 1.0).abs() > 1e-6,
        "sublane filtering should adjust lateral offset, got {}",
        lat.lateral_offset,
    );
}

/// Test: step_prediction updates overlay timestamp after 60 sim-seconds.
#[test]
fn test_prediction_overlay_updates() {
    let graph = make_2lane_graph();
    let mut sim = SimWorld::new_cpu_only(graph);
    sim.sim_state = SimState::Running;

    // Manually initialize prediction service
    let edge_count = sim.road_graph.edge_count();
    let free_flow = vec![7.2_f32; edge_count];
    sim.reroute.prediction_service =
        Some(PredictionService::new(edge_count, &free_flow));

    // Before 60s: overlay should not update
    sim.sim_time = 30.0;
    sim.step_prediction();
    let overlay = sim
        .reroute
        .prediction_service
        .as_ref()
        .unwrap()
        .store()
        .current();
    assert!(
        overlay.timestamp_sim_seconds < 1.0,
        "overlay should not update before 60s, got t={}",
        overlay.timestamp_sim_seconds,
    );

    // After 60s: overlay should update
    sim.sim_time = 65.0;
    sim.step_prediction();
    let overlay = sim
        .reroute
        .prediction_service
        .as_ref()
        .unwrap()
        .store()
        .current();
    assert!(
        (overlay.timestamp_sim_seconds - 65.0).abs() < 0.1,
        "overlay should update at t=65s, got t={}",
        overlay.timestamp_sim_seconds,
    );
}
