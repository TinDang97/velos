//! CPU-side reroute evaluation with staggered scheduling.
//!
//! The [`RerouteScheduler`] processes 1K agents per simulation step in
//! round-robin fashion, with immediate trigger priority and per-agent
//! cooldown to prevent oscillation.
//!
//! [`evaluate_reroute`] compares current route cost vs CCH alternative,
//! returning [`RerouteResult::ShouldReroute`] when cost_delta exceeds
//! the configurable threshold (default 30%).

use std::collections::{HashMap, VecDeque};

use crate::cost::{route_cost, CostWeights, EdgeAttributes};

/// Configuration for reroute evaluation behavior.
#[derive(Debug, Clone)]
pub struct RerouteConfig {
    /// Number of agents evaluated per simulation step.
    pub batch_size: usize,
    /// Minimum seconds between reroutes for the same agent.
    pub cooldown_seconds: f64,
    /// Minimum cost improvement fraction to trigger reroute.
    pub cost_delta_threshold: f32,
}

impl Default for RerouteConfig {
    fn default() -> Self {
        Self {
            batch_size: 1000,
            cooldown_seconds: 30.0,
            cost_delta_threshold: 0.30,
        }
    }
}

/// Result of evaluating whether an agent should reroute.
#[derive(Debug, Clone, PartialEq)]
pub enum RerouteResult {
    /// Agent should keep current route.
    NoReroute,
    /// Agent should switch to a better route.
    ShouldReroute {
        /// New route as edge IDs.
        new_route: Vec<u32>,
        /// Cost improvement fraction (positive = cheaper alternative).
        cost_delta: f32,
    },
}

/// Perception data from GPU readback, used for reroute trigger detection.
///
/// Mirrors the GPU PerceptionResult struct. Defined here to avoid
/// a dependency from velos-core to velos-gpu.
#[derive(Debug, Clone, Copy, Default)]
pub struct PerceptionSnapshot {
    /// Congestion level on the agent's current route (0.0-1.0).
    pub congestion_own_route: f32,
    /// Area-wide congestion level (0.0-1.0).
    pub congestion_area: f32,
    /// Bitfield: bit0=route_blocked, bit1=emergency_nearby.
    pub flags: u32,
}

impl PerceptionSnapshot {
    /// Check if the agent's route is blocked (bit0 of flags).
    #[inline]
    pub fn is_route_blocked(&self) -> bool {
        self.flags & 1 != 0
    }

    /// Check if an emergency vehicle is nearby (bit1 of flags).
    #[inline]
    pub fn is_emergency_nearby(&self) -> bool {
        self.flags & 2 != 0
    }
}

/// Staggered reroute scheduler that processes a fixed batch of agents
/// per simulation step in round-robin fashion.
///
/// Immediate triggers (blocked edges, emergency) are front-loaded into
/// the next batch but still respect the per-step budget.
#[derive(Debug)]
pub struct RerouteScheduler {
    config: RerouteConfig,
    /// Current position in the round-robin ring (agent index).
    ring_index: usize,
    /// Total number of agents in the simulation.
    total_agents: usize,
    /// Priority queue for agents needing immediate evaluation.
    immediate_queue: VecDeque<u32>,
    /// Tracks last reroute time per agent for cooldown enforcement.
    last_reroute: HashMap<u32, f64>,
}

impl RerouteScheduler {
    /// Create a new scheduler with the given configuration.
    pub fn new(config: RerouteConfig) -> Self {
        Self {
            config,
            ring_index: 0,
            total_agents: 0,
            immediate_queue: VecDeque::new(),
            last_reroute: HashMap::new(),
        }
    }

    /// Update the total agent population count.
    ///
    /// Called when agents are spawned or removed.
    pub fn set_population(&mut self, count: usize) {
        self.total_agents = count;
        if self.ring_index >= count && count > 0 {
            self.ring_index %= count;
        }
    }

    /// Push an agent to the immediate evaluation queue.
    ///
    /// The agent will be included in the next batch, displacing
    /// regular round-robin agents (not added on top of the budget).
    pub fn add_immediate_trigger(&mut self, agent_id: u32) {
        if !self.immediate_queue.contains(&agent_id) {
            self.immediate_queue.push_back(agent_id);
        }
    }

    /// Get the next batch of agent IDs to evaluate for rerouting.
    ///
    /// 1. Pulls from immediate_queue first (skipping agents on cooldown)
    /// 2. Fills remainder from round-robin ring (skipping cooldown)
    /// 3. Advances ring_index past all considered agents
    ///
    /// Returns up to `batch_size` agent IDs.
    pub fn next_batch(&mut self, sim_time: f64) -> Vec<u32> {
        if self.total_agents == 0 {
            return Vec::new();
        }

        let mut batch = Vec::with_capacity(self.config.batch_size);

        // 1. Take from immediate queue (filter out cooldown)
        let mut remaining_immediate = VecDeque::new();
        while batch.len() < self.config.batch_size {
            match self.immediate_queue.pop_front() {
                Some(id) => {
                    if !self.is_on_cooldown(id, sim_time) {
                        batch.push(id);
                    } else {
                        remaining_immediate.push_back(id);
                    }
                }
                None => break,
            }
        }
        // Put back any that didn't fit
        while let Some(id) = self.immediate_queue.pop_front() {
            remaining_immediate.push_back(id);
        }
        self.immediate_queue = remaining_immediate;

        // 2. Fill from round-robin ring
        let mut considered = 0;
        while batch.len() < self.config.batch_size && considered < self.total_agents {
            let agent_id = self.ring_index as u32;
            self.ring_index = (self.ring_index + 1) % self.total_agents;
            considered += 1;

            if batch.contains(&agent_id) {
                continue; // Already in batch from immediate queue
            }
            if self.is_on_cooldown(agent_id, sim_time) {
                continue;
            }
            batch.push(agent_id);
        }

        batch
    }

    /// Record that an agent was rerouted at the given simulation time.
    pub fn record_reroute(&mut self, agent_id: u32, sim_time: f64) {
        self.last_reroute.insert(agent_id, sim_time);
    }

    /// Check if an agent is still within the cooldown period.
    pub fn is_on_cooldown(&self, agent_id: u32, sim_time: f64) -> bool {
        match self.last_reroute.get(&agent_id) {
            Some(&last_time) => (sim_time - last_time) < self.config.cooldown_seconds,
            None => false,
        }
    }

    /// Access the current configuration.
    pub fn config(&self) -> &RerouteConfig {
        &self.config
    }
}

/// Context for route cost evaluation, abstracting over CCH query results.
///
/// Allows testing without a real CCH router by providing the alternative
/// route and cost directly.
pub struct RouteEvalContext<'a> {
    /// Remaining edges of the current route (from current position onward).
    pub remaining_edges: &'a [u32],
    /// Perception snapshot from GPU readback.
    pub perception: &'a PerceptionSnapshot,
    /// Agent's cost weight profile.
    pub profile_weights: &'a CostWeights,
    /// Edge attribute data for cost computation.
    pub edge_attrs: &'a [EdgeAttributes],
    /// Predicted travel times per edge.
    pub overlay_travel_times: &'a [f32],
    /// Prediction confidence per edge.
    pub overlay_confidence: &'a [f32],
    /// Alternative route from CCH query (None if no path found).
    pub alternative_route: Option<Vec<u32>>,
    /// Cost delta threshold from config.
    pub cost_delta_threshold: f32,
}

/// Evaluate whether an agent should reroute based on current vs alternative
/// route cost.
///
/// Algorithm:
/// 1. If route_blocked flag is set, immediately flag for reroute
/// 2. Compute current cost on remaining edges
/// 3. If no alternative route available, return NoReroute
/// 4. Compute alternative cost
/// 5. If cost_delta = (current - alt) / current > threshold, reroute
pub fn evaluate_reroute(ctx: &RouteEvalContext<'_>) -> RerouteResult {
    // If route is blocked, any alternative is worth taking
    let route_blocked = ctx.perception.is_route_blocked();

    let current_cost = route_cost(
        ctx.remaining_edges,
        ctx.overlay_travel_times,
        ctx.overlay_confidence,
        ctx.profile_weights,
        ctx.edge_attrs,
    );

    let alt_route = match &ctx.alternative_route {
        Some(route) if !route.is_empty() => route,
        _ => {
            if route_blocked {
                // Route is blocked but no alternative found
                return RerouteResult::NoReroute;
            }
            return RerouteResult::NoReroute;
        }
    };

    let alt_cost = route_cost(
        alt_route,
        ctx.overlay_travel_times,
        ctx.overlay_confidence,
        ctx.profile_weights,
        ctx.edge_attrs,
    );

    // For blocked routes, accept any finite alternative
    if route_blocked && alt_cost.is_finite() {
        let cost_delta = if current_cost > 0.0 {
            (current_cost - alt_cost) / current_cost
        } else {
            1.0
        };
        return RerouteResult::ShouldReroute {
            new_route: alt_route.clone(),
            cost_delta,
        };
    }

    // Normal cost comparison
    if current_cost <= 0.0 {
        return RerouteResult::NoReroute;
    }

    let cost_delta = (current_cost - alt_cost) / current_cost;
    if cost_delta > ctx.cost_delta_threshold {
        RerouteResult::ShouldReroute {
            new_route: alt_route.clone(),
            cost_delta,
        }
    } else {
        RerouteResult::NoReroute
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(batch_size: usize) -> RerouteConfig {
        RerouteConfig {
            batch_size,
            cooldown_seconds: 30.0,
            cost_delta_threshold: 0.30,
        }
    }

    fn make_edge_attrs(count: usize) -> Vec<EdgeAttributes> {
        (0..count)
            .map(|_| EdgeAttributes {
                current_travel_time: 10.0,
                distance_m: 500.0,
                comfort_penalty: 0.2,
                safety_score: 0.3,
                fuel_rate: 0.00008,
                signal_delay: 0.0,
            })
            .collect()
    }

    fn time_only_weights() -> CostWeights {
        CostWeights {
            time: 1.0,
            comfort: 0.0,
            safety: 0.0,
            fuel: 0.0,
            signal_delay: 0.0,
            prediction_penalty: 0.0,
        }
    }

    // --- Scheduler tests ---

    #[test]
    fn scheduler_returns_exactly_batch_size_per_step() {
        let mut sched = RerouteScheduler::new(make_config(1000));
        sched.set_population(5000);

        let batch = sched.next_batch(0.0);
        assert_eq!(batch.len(), 1000);
    }

    #[test]
    fn scheduler_round_robin_covers_all_agents_in_5_steps() {
        let mut sched = RerouteScheduler::new(make_config(1000));
        sched.set_population(5000);

        let mut all_ids: Vec<u32> = Vec::new();
        for _ in 0..5 {
            let batch = sched.next_batch(0.0);
            assert_eq!(batch.len(), 1000);
            all_ids.extend(batch);
        }

        all_ids.sort();
        all_ids.dedup();
        assert_eq!(all_ids.len(), 5000, "All 5000 agents should be scheduled exactly once");

        let expected: Vec<u32> = (0..5000).collect();
        assert_eq!(all_ids, expected);
    }

    #[test]
    fn scheduler_immediate_trigger_front_loaded() {
        let mut sched = RerouteScheduler::new(make_config(5));
        sched.set_population(100);

        sched.add_immediate_trigger(99);
        sched.add_immediate_trigger(50);

        let batch = sched.next_batch(0.0);
        assert_eq!(batch.len(), 5);
        // Immediate triggers should be first
        assert_eq!(batch[0], 99);
        assert_eq!(batch[1], 50);
        // Remaining filled from round-robin
    }

    #[test]
    fn scheduler_immediate_triggers_respect_budget() {
        let mut sched = RerouteScheduler::new(make_config(3));
        sched.set_population(100);

        // Add 5 immediate triggers but budget is only 3
        for id in [10, 20, 30, 40, 50] {
            sched.add_immediate_trigger(id);
        }

        let batch = sched.next_batch(0.0);
        assert_eq!(batch.len(), 3, "Must respect batch_size budget");
        assert_eq!(batch, vec![10, 20, 30]);

        // Remaining should be available next step
        let batch2 = sched.next_batch(0.0);
        assert!(batch2.contains(&40));
        assert!(batch2.contains(&50));
    }

    #[test]
    fn scheduler_cooldown_skips_recently_rerouted() {
        let mut sched = RerouteScheduler::new(make_config(1000));
        sched.set_population(5);

        // Reroute agents 0, 1, 2 at time 0
        sched.record_reroute(0, 0.0);
        sched.record_reroute(1, 0.0);
        sched.record_reroute(2, 0.0);

        // At time 10s (< 30s cooldown), only agents 3, 4 should be eligible
        let batch = sched.next_batch(10.0);
        assert_eq!(batch.len(), 2);
        assert!(batch.contains(&3));
        assert!(batch.contains(&4));
    }

    #[test]
    fn scheduler_cooldown_expires_after_30s() {
        let mut sched = RerouteScheduler::new(make_config(1000));
        sched.set_population(3);

        sched.record_reroute(0, 0.0);
        sched.record_reroute(1, 0.0);
        sched.record_reroute(2, 0.0);

        // At 29.9s: still on cooldown
        assert!(sched.is_on_cooldown(0, 29.9));

        // At 30.0s: cooldown expired (>= 30s threshold uses strict <)
        assert!(!sched.is_on_cooldown(0, 30.0));

        // All agents should be eligible at 31s
        let batch = sched.next_batch(31.0);
        assert_eq!(batch.len(), 3);
    }

    #[test]
    fn scheduler_config_customizable() {
        let config = RerouteConfig {
            batch_size: 500,
            cooldown_seconds: 60.0,
            cost_delta_threshold: 0.20,
        };
        let mut sched = RerouteScheduler::new(config);
        sched.set_population(1000);

        let batch = sched.next_batch(0.0);
        assert_eq!(batch.len(), 500);

        sched.record_reroute(0, 0.0);
        // Still on cooldown at 59s with 60s threshold
        assert!(sched.is_on_cooldown(0, 59.0));
        assert!(!sched.is_on_cooldown(0, 60.0));
    }

    // --- evaluate_reroute tests ---

    #[test]
    fn evaluate_reroute_no_reroute_when_low_cost_delta() {
        let edge_attrs = make_edge_attrs(4);
        let overlay_times = vec![10.0; 4];
        let overlay_conf = vec![1.0; 4];
        let weights = time_only_weights();

        let ctx = RouteEvalContext {
            remaining_edges: &[0, 1],
            perception: &PerceptionSnapshot::default(),
            profile_weights: &weights,
            edge_attrs: &edge_attrs,
            overlay_travel_times: &overlay_times,
            overlay_confidence: &overlay_conf,
            alternative_route: Some(vec![2, 3]), // Same cost edges
            cost_delta_threshold: 0.30,
        };

        let result = evaluate_reroute(&ctx);
        assert_eq!(result, RerouteResult::NoReroute);
    }

    #[test]
    fn evaluate_reroute_should_reroute_when_high_cost_delta() {
        // Current route: 2 expensive edges (travel_time=100 each)
        // Alternative: 2 cheap edges (travel_time=10 each)
        let mut edge_attrs = make_edge_attrs(4);
        edge_attrs[0].current_travel_time = 100.0;
        edge_attrs[1].current_travel_time = 100.0;
        edge_attrs[2].current_travel_time = 10.0;
        edge_attrs[3].current_travel_time = 10.0;

        let overlay_times = vec![100.0, 100.0, 10.0, 10.0];
        let overlay_conf = vec![1.0; 4];
        let weights = time_only_weights();

        let ctx = RouteEvalContext {
            remaining_edges: &[0, 1],
            perception: &PerceptionSnapshot::default(),
            profile_weights: &weights,
            edge_attrs: &edge_attrs,
            overlay_travel_times: &overlay_times,
            overlay_confidence: &overlay_conf,
            alternative_route: Some(vec![2, 3]),
            cost_delta_threshold: 0.30,
        };

        let result = evaluate_reroute(&ctx);
        match result {
            RerouteResult::ShouldReroute { new_route, cost_delta } => {
                assert_eq!(new_route, vec![2, 3]);
                // (200 - 20) / 200 = 0.9 > 0.30
                assert!(cost_delta > 0.30, "cost_delta={cost_delta}");
            }
            RerouteResult::NoReroute => panic!("Expected ShouldReroute"),
        }
    }

    #[test]
    fn evaluate_reroute_no_reroute_for_unblocked_low_congestion() {
        let edge_attrs = make_edge_attrs(4);
        let overlay_times = vec![10.0; 4];
        let overlay_conf = vec![1.0; 4];
        let weights = time_only_weights();

        let perception = PerceptionSnapshot {
            congestion_own_route: 0.1, // Low congestion
            congestion_area: 0.1,
            flags: 0, // Not blocked
        };

        let ctx = RouteEvalContext {
            remaining_edges: &[0, 1],
            perception: &perception,
            profile_weights: &weights,
            edge_attrs: &edge_attrs,
            overlay_travel_times: &overlay_times,
            overlay_confidence: &overlay_conf,
            alternative_route: Some(vec![2, 3]), // Same cost
            cost_delta_threshold: 0.30,
        };

        assert_eq!(evaluate_reroute(&ctx), RerouteResult::NoReroute);
    }

    #[test]
    fn evaluate_reroute_blocked_route_accepts_any_alternative() {
        let mut edge_attrs = make_edge_attrs(4);
        // Even if alternative is slightly more expensive (10 vs 8), blocked route accepts it
        edge_attrs[0].current_travel_time = 8.0;
        edge_attrs[1].current_travel_time = 8.0;
        edge_attrs[2].current_travel_time = 10.0;
        edge_attrs[3].current_travel_time = 10.0;

        let overlay_times = vec![8.0, 8.0, 10.0, 10.0];
        let overlay_conf = vec![1.0; 4];
        let weights = time_only_weights();

        let perception = PerceptionSnapshot {
            congestion_own_route: 1.0,
            congestion_area: 0.5,
            flags: 1, // bit0 = route_blocked
        };

        let ctx = RouteEvalContext {
            remaining_edges: &[0, 1],
            perception: &perception,
            profile_weights: &weights,
            edge_attrs: &edge_attrs,
            overlay_travel_times: &overlay_times,
            overlay_confidence: &overlay_conf,
            alternative_route: Some(vec![2, 3]),
            cost_delta_threshold: 0.30,
        };

        match evaluate_reroute(&ctx) {
            RerouteResult::ShouldReroute { new_route, .. } => {
                assert_eq!(new_route, vec![2, 3]);
            }
            RerouteResult::NoReroute => panic!("Blocked route should accept any alternative"),
        }
    }

    #[test]
    fn evaluate_reroute_no_alternative_returns_no_reroute() {
        let edge_attrs = make_edge_attrs(2);
        let overlay_times = vec![10.0; 2];
        let overlay_conf = vec![1.0; 2];
        let weights = time_only_weights();

        let ctx = RouteEvalContext {
            remaining_edges: &[0, 1],
            perception: &PerceptionSnapshot::default(),
            profile_weights: &weights,
            edge_attrs: &edge_attrs,
            overlay_travel_times: &overlay_times,
            overlay_confidence: &overlay_conf,
            alternative_route: None,
            cost_delta_threshold: 0.30,
        };

        assert_eq!(evaluate_reroute(&ctx), RerouteResult::NoReroute);
    }

    #[test]
    fn scheduler_empty_population_returns_empty_batch() {
        let mut sched = RerouteScheduler::new(make_config(1000));
        let batch = sched.next_batch(0.0);
        assert!(batch.is_empty());
    }

    #[test]
    fn scheduler_population_less_than_batch_returns_all() {
        let mut sched = RerouteScheduler::new(make_config(1000));
        sched.set_population(50);

        let batch = sched.next_batch(0.0);
        assert_eq!(batch.len(), 50);
    }

    #[test]
    fn perception_snapshot_flag_decoding() {
        let p = PerceptionSnapshot { flags: 0b11, ..Default::default() };
        assert!(p.is_route_blocked());
        assert!(p.is_emergency_nearby());

        let p2 = PerceptionSnapshot { flags: 0b01, ..Default::default() };
        assert!(p2.is_route_blocked());
        assert!(!p2.is_emergency_nearby());

        let p3 = PerceptionSnapshot { flags: 0b10, ..Default::default() };
        assert!(!p3.is_route_blocked());
        assert!(p3.is_emergency_nearby());
    }

    #[test]
    fn scheduler_immediate_trigger_skips_cooldown_agents() {
        let mut sched = RerouteScheduler::new(make_config(5));
        sched.set_population(10);

        sched.record_reroute(5, 0.0);
        sched.add_immediate_trigger(5); // On cooldown

        let batch = sched.next_batch(10.0); // 10s < 30s cooldown
        assert!(!batch.contains(&5), "Agent 5 should be skipped due to cooldown");
    }

    #[test]
    fn evaluate_reroute_cost_delta_at_boundary() {
        // Create scenario where cost_delta is exactly at threshold
        let mut edge_attrs = make_edge_attrs(4);
        edge_attrs[0].current_travel_time = 100.0;
        edge_attrs[1].current_travel_time = 100.0;
        // Alternative: 70% of current cost (delta = 0.30)
        edge_attrs[2].current_travel_time = 70.0;
        edge_attrs[3].current_travel_time = 70.0;

        let overlay_times = vec![100.0, 100.0, 70.0, 70.0];
        let overlay_conf = vec![1.0; 4];
        let weights = time_only_weights();

        let ctx = RouteEvalContext {
            remaining_edges: &[0, 1],
            perception: &PerceptionSnapshot::default(),
            profile_weights: &weights,
            edge_attrs: &edge_attrs,
            overlay_travel_times: &overlay_times,
            overlay_confidence: &overlay_conf,
            alternative_route: Some(vec![2, 3]),
            cost_delta_threshold: 0.30,
        };

        // At exactly 0.30, should NOT reroute (threshold is strict >)
        let result = evaluate_reroute(&ctx);
        assert_eq!(result, RerouteResult::NoReroute);
    }
}
