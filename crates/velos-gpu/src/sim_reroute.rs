//! Reroute evaluation integration for SimWorld.
//!
//! Wires the CPU-side reroute scheduler into the simulation frame loop.
//! After GPU wave-front dispatch and perception readback, evaluates a
//! batch of agents for rerouting using CCH alternative paths.

use velos_core::cost::{decode_profile_from_flags, EdgeAttributes, PROFILE_WEIGHTS};
use velos_core::reroute::{
    evaluate_reroute, PerceptionSnapshot, RerouteConfig, RerouteResult, RerouteScheduler,
    RouteEvalContext,
};
use velos_net::cch::{CCHRouter, EdgeNodeMap};
use velos_predict::PredictionService;

use crate::perception::PerceptionResult;
use crate::sim::SimWorld;

/// Reroute subsystem state, owned by SimWorld.
///
/// Extracted to a struct to keep SimWorld field count manageable.
pub(crate) struct RerouteState {
    /// Staggered scheduler processing 1K agents/step.
    pub scheduler: RerouteScheduler,
    /// CCH router for alternative path queries.
    pub cch_router: Option<CCHRouter>,
    /// Edge-to-node lookup for mapping agent positions to CCH queries.
    pub edge_node_map: Option<EdgeNodeMap>,
    /// Prediction service for overlay travel times.
    pub prediction_service: Option<PredictionService>,
    /// Per-edge attributes for cost function.
    pub edge_attrs: Vec<EdgeAttributes>,
}

impl RerouteState {
    /// Create a new reroute state with default configuration.
    pub fn new() -> Self {
        Self {
            scheduler: RerouteScheduler::new(RerouteConfig::default()),
            cch_router: None,
            edge_node_map: None,
            prediction_service: None,
            edge_attrs: Vec::new(),
        }
    }
}

impl SimWorld {
    /// Initialize the reroute subsystem with CCH router and prediction service.
    ///
    /// Called at simulation startup after the road graph is loaded.
    /// Builds CCH from graph (with disk cache), initializes prediction service,
    /// and computes edge attributes from road class heuristics.
    pub fn init_reroute(&mut self) {
        use std::path::PathBuf;
        use velos_core::cost::{default_edge_attributes, RoadClass as CostRoadClass};
        use velos_net::graph::RoadClass;

        let graph = &self.road_graph;
        let g = graph.inner();

        // Build edge attributes from road class heuristics
        let edge_count = graph.edge_count();
        let mut edge_attrs = Vec::with_capacity(edge_count);
        let mut free_flow = Vec::with_capacity(edge_count);

        for edge_idx in g.edge_indices() {
            let edge = &g[edge_idx];
            let cost_rc = match edge.road_class {
                RoadClass::Motorway => CostRoadClass::Motorway,
                RoadClass::Trunk => CostRoadClass::Trunk,
                RoadClass::Primary => CostRoadClass::Primary,
                RoadClass::Secondary => CostRoadClass::Secondary,
                RoadClass::Tertiary => CostRoadClass::Tertiary,
                RoadClass::Residential => CostRoadClass::Residential,
                RoadClass::Service => CostRoadClass::Service,
            };
            let has_signal = false; // Approximation; refined later from signal controller data
            let speed_kmh = edge.speed_limit_mps * 3.6;
            let attr = default_edge_attributes(cost_rc, edge.length_m as f32, speed_kmh as f32, has_signal);
            free_flow.push(attr.current_travel_time);
            edge_attrs.push(attr);
        }

        // Build edge-to-node lookup
        let edge_node_map = EdgeNodeMap::from_graph(graph);

        // Build CCH router with disk cache
        let cache_path = PathBuf::from("data/cch_cache.bin");
        let cch_router = match CCHRouter::from_graph_cached(graph, &cache_path) {
            Ok(router) => {
                log::info!("CCH router initialized ({} nodes)", router.node_count);
                Some(router)
            }
            Err(e) => {
                log::warn!("Failed to build CCH router: {e}");
                None
            }
        };

        // Customize CCH with free-flow weights
        if let Some(mut router) = cch_router {
            router.customize(&free_flow);
            self.reroute.cch_router = Some(router);
        }

        // Initialize prediction service
        let prediction_service = PredictionService::new(edge_count, &free_flow);

        self.reroute.edge_node_map = Some(edge_node_map);
        self.reroute.prediction_service = Some(prediction_service);
        self.reroute.edge_attrs = edge_attrs;

        log::info!("Reroute subsystem initialized: {} edges", edge_count);
    }

    /// Run one reroute evaluation step: process a batch of agents.
    ///
    /// Called after GPU dispatch and perception readback in the frame loop.
    /// Scans perception results for immediate triggers and evaluates the
    /// scheduled batch for potential rerouting.
    pub fn step_reroute(&mut self, perception_results: &[PerceptionResult]) {
        use velos_core::components::Route;

        let sim_time = self.sim_time;

        // Update population count
        let agent_count = self.world.len() as usize;
        self.reroute.scheduler.set_population(agent_count);

        // Scan perception results for immediate triggers (route_blocked, emergency)
        // Agent indices in perception_results correspond to entity ordering
        for (i, pr) in perception_results.iter().enumerate() {
            if pr.flags & 0b11 != 0 {
                // Route blocked or emergency nearby
                self.reroute.scheduler.add_immediate_trigger(i as u32);
            }
        }

        // Get CCH router and overlay, bail if not initialized
        let (cch, edge_node_map, prediction_service) = match (
            &self.reroute.cch_router,
            &self.reroute.edge_node_map,
            &self.reroute.prediction_service,
        ) {
            (Some(c), Some(m), Some(p)) => (c, m, p),
            _ => return,
        };

        let overlay = prediction_service.store().current();

        // Get next batch of agents to evaluate
        let batch = self.reroute.scheduler.next_batch(sim_time);
        if batch.is_empty() {
            return;
        }

        let edge_attrs = &self.reroute.edge_attrs;
        let config = self.reroute.scheduler.config().clone();

        // Collect reroute decisions (avoid borrow conflict with self)
        let mut reroute_decisions: Vec<(u32, Vec<u32>)> = Vec::new();

        for &agent_id in &batch {
            // Build perception snapshot from readback buffer
            let perception = if (agent_id as usize) < perception_results.len() {
                let pr = &perception_results[agent_id as usize];
                PerceptionSnapshot {
                    congestion_own_route: pr.congestion_own_route,
                    congestion_area: pr.congestion_area,
                    flags: pr.flags,
                }
            } else {
                PerceptionSnapshot::default()
            };

            // Get agent's current route (by iterating -- O(n) but for 1K batch this is fine)
            // In production, a direct entity lookup by agent_id would be more efficient
            let route_data: Option<(Vec<u32>, usize, u32)> = {
                let mut found: Option<(Vec<u32>, usize, u32)> = None;
                for (entity, route, gpu_state) in
                    self.world.query_mut::<(hecs::Entity, &Route, &velos_core::GpuAgentState)>()
                {
                    if entity.id() == agent_id {
                        found = Some((route.path.clone(), route.current_step, gpu_state.flags));
                        break;
                    }
                }
                found
            };

            let (path, current_step, flags) = match route_data {
                Some(d) => d,
                None => continue,
            };

            if current_step >= path.len() {
                continue; // Agent has reached destination
            }

            // Get the current node (the one the agent is heading toward)
            let current_node = path[current_step];

            // Get destination node
            let destination = match path.last() {
                Some(&dest) => dest,
                None => continue,
            };

            // Determine remaining edges by looking up node pairs
            // This is approximate -- we use CCH cost comparison
            let remaining_nodes = &path[current_step..];
            let remaining_edges: Vec<u32> = remaining_nodes
                .windows(2)
                .filter_map(|w| {
                    // Find edge between consecutive nodes
                    // For now, use node indices as approximate edge lookup
                    find_edge_between(
                        &self.road_graph,
                        w[0],
                        w[1],
                    )
                })
                .collect();

            if remaining_edges.is_empty() {
                continue;
            }

            // Get agent profile weights
            let profile = decode_profile_from_flags(flags);
            let weights = &PROFILE_WEIGHTS[profile as usize];

            // Query CCH for alternative route
            let alt_route = cch
                .query_with_path(current_node, destination)
                .and_then(|(_, node_path)| {
                    // Convert node path to edge sequence
                    let edges: Vec<u32> = node_path
                        .windows(2)
                        .filter_map(|w| find_edge_between(&self.road_graph, w[0], w[1]))
                        .collect();
                    if edges.is_empty() { None } else { Some(edges) }
                });

            let ctx = RouteEvalContext {
                remaining_edges: &remaining_edges,
                perception: &perception,
                profile_weights: weights,
                edge_attrs,
                overlay_travel_times: &overlay.edge_travel_times,
                overlay_confidence: &overlay.edge_confidence,
                alternative_route: alt_route,
                cost_delta_threshold: config.cost_delta_threshold,
            };

            if let RerouteResult::ShouldReroute { new_route, .. } = evaluate_reroute(&ctx) {
                reroute_decisions.push((agent_id, new_route));
            }
        }

        // Apply reroute decisions
        for (agent_id, new_edges) in &reroute_decisions {
            // Convert edge sequence back to node path for Route component
            let mut new_path: Vec<u32> = Vec::with_capacity(new_edges.len() + 1);
            for (i, &edge_id) in new_edges.iter().enumerate() {
                if let Some((src, tgt)) = edge_node_map.get(edge_id) {
                    if i == 0 {
                        new_path.push(src);
                    }
                    new_path.push(tgt);
                }
            }

            // Find entity and update route
            for (entity, route) in
                self.world.query_mut::<(hecs::Entity, &mut Route)>()
            {
                if entity.id() == *agent_id {
                    route.path = new_path.clone();
                    route.current_step = 0;
                    break;
                }
            }

            self.reroute.scheduler.record_reroute(*agent_id, sim_time);
        }

        if !reroute_decisions.is_empty() {
            log::debug!(
                "Rerouted {}/{} agents at t={:.1}s",
                reroute_decisions.len(),
                batch.len(),
                sim_time
            );
        }
    }
}

/// Find the edge index between two nodes in the road graph.
fn find_edge_between(
    graph: &velos_net::RoadGraph,
    source: u32,
    target: u32,
) -> Option<u32> {
    use petgraph::graph::NodeIndex;
    use petgraph::visit::EdgeRef;

    let g = graph.inner();
    let src = NodeIndex::new(source as usize);
    let tgt = NodeIndex::new(target as usize);

    g.edges_connecting(src, tgt)
        .next()
        .map(|e| e.id().index() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_prediction_skips_when_not_time() {
        use petgraph::graph::DiGraph;
        use velos_net::graph::{RoadClass, RoadEdge, RoadGraph, RoadNode};

        let mut g = DiGraph::new();
        let a = g.add_node(RoadNode { pos: [0.0, 0.0] });
        let b = g.add_node(RoadNode { pos: [100.0, 0.0] });
        g.add_edge(
            a,
            b,
            RoadEdge {
                length_m: 100.0,
                speed_limit_mps: 13.9,
                lane_count: 2,
                oneway: true,
                road_class: RoadClass::Primary,
                geometry: vec![[0.0, 0.0], [100.0, 0.0]],
                motorbike_only: false,
                time_windows: None,
            },
        );
        let graph = RoadGraph::new(g);
        let mut sim = crate::sim::SimWorld::new_cpu_only(graph);

        // Initialize prediction service manually
        let edge_count = sim.road_graph.edge_count();
        let free_flow = vec![7.2_f32; edge_count];
        sim.reroute.prediction_service =
            Some(PredictionService::new(edge_count, &free_flow));

        // sim_time < 60s (default is morning rush ~ 25200s, but last_update starts at 0)
        // PredictionService.last_update_sim_seconds starts at 0.0, sim_time starts at 25200.
        // So should_update will be true. Let's test with a fresh service and sim_time = 10.
        sim.sim_time = 10.0;
        sim.reroute.prediction_service =
            Some(PredictionService::new(edge_count, &free_flow));

        // At sim_time=10, should_update should be false (10 - 0 < 60)
        // Actually: 10.0 - 0.0 = 10.0 < 60.0, so should_update is false
        // step_prediction should be a no-op
        sim.step_prediction();
        // No crash = success; overlay timestamp should still be 0
        let overlay = sim.reroute.prediction_service.as_ref().unwrap().store().current();
        assert!(
            overlay.timestamp_sim_seconds < 1.0,
            "overlay should not have been updated at t=10s"
        );
    }

    #[test]
    fn step_prediction_updates_when_time_elapsed() {
        use petgraph::graph::DiGraph;
        use velos_net::graph::{RoadClass, RoadEdge, RoadGraph, RoadNode};

        let mut g = DiGraph::new();
        let a = g.add_node(RoadNode { pos: [0.0, 0.0] });
        let b = g.add_node(RoadNode { pos: [100.0, 0.0] });
        g.add_edge(
            a,
            b,
            RoadEdge {
                length_m: 100.0,
                speed_limit_mps: 13.9,
                lane_count: 2,
                oneway: true,
                road_class: RoadClass::Primary,
                geometry: vec![[0.0, 0.0], [100.0, 0.0]],
                motorbike_only: false,
                time_windows: None,
            },
        );
        let graph = RoadGraph::new(g);
        let mut sim = crate::sim::SimWorld::new_cpu_only(graph);

        let edge_count = sim.road_graph.edge_count();
        let free_flow = vec![7.2_f32; edge_count];
        sim.reroute.prediction_service =
            Some(PredictionService::new(edge_count, &free_flow));

        // Set sim_time past 60s threshold
        sim.sim_time = 65.0;

        sim.step_prediction();

        let overlay = sim.reroute.prediction_service.as_ref().unwrap().store().current();
        assert!(
            (overlay.timestamp_sim_seconds - 65.0).abs() < 0.1,
            "overlay should have been updated at t=65s, got t={}",
            overlay.timestamp_sim_seconds,
        );
    }

    #[test]
    fn reroute_state_default_config() {
        let state = RerouteState::new();
        assert_eq!(state.scheduler.config().batch_size, 1000);
        assert!((state.scheduler.config().cooldown_seconds - 30.0).abs() < f64::EPSILON);
        assert!((state.scheduler.config().cost_delta_threshold - 0.30).abs() < f32::EPSILON);
    }

    #[test]
    fn find_edge_between_nonexistent_returns_none() {
        // Build a minimal graph with no edges
        use petgraph::graph::DiGraph;
        use velos_net::graph::{RoadGraph, RoadNode};

        let mut g = DiGraph::new();
        g.add_node(RoadNode { pos: [0.0, 0.0] });
        g.add_node(RoadNode { pos: [1.0, 0.0] });
        let graph = RoadGraph::new(g);

        assert_eq!(find_edge_between(&graph, 0, 1), None);
    }

    #[test]
    fn find_edge_between_existing_returns_index() {
        use petgraph::graph::DiGraph;
        use velos_net::graph::{RoadClass, RoadEdge, RoadGraph, RoadNode};

        let mut g = DiGraph::new();
        let a = g.add_node(RoadNode { pos: [0.0, 0.0] });
        let b = g.add_node(RoadNode { pos: [100.0, 0.0] });
        g.add_edge(
            a,
            b,
            RoadEdge {
                length_m: 100.0,
                speed_limit_mps: 13.9,
                lane_count: 2,
                oneway: true,
                road_class: RoadClass::Primary,
                geometry: vec![[0.0, 0.0], [100.0, 0.0]],
                motorbike_only: false,
                time_windows: None,
            },
        );
        let graph = RoadGraph::new(g);

        assert_eq!(find_edge_between(&graph, 0, 1), Some(0));
        assert_eq!(find_edge_between(&graph, 1, 0), None); // Wrong direction
    }
}
