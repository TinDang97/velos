//! Simulation state and tick logic for wiring subsystems together.
//!
//! Extracted from app.rs to keep files under 700 lines.
//! Owns the ECS world, road graph, spawner, signal controllers,
//! gridlock detector, and all per-frame simulation stepping.

use std::collections::HashMap;

use hecs::{Entity, World};
use petgraph::graph::{EdgeIndex, NodeIndex};
use petgraph::visit::EdgeRef;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use velos_core::components::{Kinematics, Position, RoadPosition, Route, VehicleType, WaitState};
use velos_demand::{OdMatrix, SpawnVehicleType, Spawner, TodProfile, Zone};
use velos_net::RoadGraph;
use velos_signal::controller::FixedTimeController;
use velos_signal::plan::{PhaseState, SignalPhase, SignalPlan};
use velos_vehicle::gridlock::detect_cycles;
use velos_vehicle::idm::{idm_acceleration, integrate_with_stopping_guard, IdmParams};
use velos_vehicle::types::default_idm_params;

use crate::renderer::AgentInstance;

/// Simulation run state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimState {
    Stopped,
    Running,
    Paused,
}

impl SimState {
    pub fn is_running(self) -> bool {
        self == SimState::Running
    }
}

/// Live simulation metrics.
#[derive(Debug, Clone, Copy, Default)]
pub struct SimMetrics {
    pub frame_time_ms: f64,
    pub agent_count: u32,
    pub motorbike_count: u32,
    pub car_count: u32,
    pub ped_count: u32,
    pub sim_time: f64,
}

/// Zone centroid positions in projected metres (approximate District 1 landmarks).
fn zone_centroid(zone: Zone) -> [f64; 2] {
    match zone {
        Zone::BenThanh => [0.0, 0.0],
        Zone::NguyenHue => [200.0, 300.0],
        Zone::Bitexco => [150.0, -100.0],
        Zone::BuiVien => [-400.0, -200.0],
        Zone::Waterfront => [500.0, -400.0],
    }
}

/// Holds all simulation subsystems.
pub struct SimWorld {
    pub world: World,
    pub road_graph: RoadGraph,
    pub spawner: Spawner,
    pub signal_controllers: Vec<(NodeIndex, FixedTimeController)>,
    pub gridlock_timeout: f64,
    pub sim_time: f64,
    pub sim_state: SimState,
    pub speed_mult: f32,
    pub metrics: SimMetrics,
    rng: StdRng,
    signalized_nodes: HashMap<u32, Vec<EdgeIndex>>,
}

impl SimWorld {
    /// Initialize simulation with road graph loaded from PBF.
    pub fn new(road_graph: RoadGraph) -> Self {
        let spawner = Spawner::new(OdMatrix::district1_poc(), TodProfile::hcmc_weekday(), 42);

        let mut signal_controllers = Vec::new();
        let mut signalized_nodes = HashMap::new();
        let g = road_graph.inner();
        for node_idx in g.node_indices() {
            let degree = g.edges(node_idx).count();
            if degree >= 4 {
                let approaches: Vec<usize> = (0..degree).collect();
                let half = degree / 2;
                let phase_a = SignalPhase {
                    green_duration: 30.0,
                    amber_duration: 3.0,
                    approaches: approaches[..half].to_vec(),
                };
                let phase_b = SignalPhase {
                    green_duration: 30.0,
                    amber_duration: 3.0,
                    approaches: approaches[half..].to_vec(),
                };
                let plan = SignalPlan::new(vec![phase_a, phase_b]);
                let controller = FixedTimeController::new(plan, degree);
                signal_controllers.push((node_idx, controller));

                let edges: Vec<EdgeIndex> = g.edges(node_idx).map(|e| e.id()).collect();
                signalized_nodes.insert(node_idx.index() as u32, edges);
            }
        }

        log::info!(
            "Simulation initialized: {} signal controllers",
            signal_controllers.len()
        );

        Self {
            world: World::new(),
            road_graph,
            spawner,
            signal_controllers,
            gridlock_timeout: 300.0,
            sim_time: 0.0,
            sim_state: SimState::Stopped,
            speed_mult: 1.0,
            metrics: SimMetrics::default(),
            rng: StdRng::seed_from_u64(123),
            signalized_nodes,
        }
    }

    /// Reset the simulation to initial state.
    pub fn reset(&mut self) {
        self.world.clear();
        self.sim_time = 0.0;
        self.sim_state = SimState::Stopped;
        self.metrics = SimMetrics::default();
        self.rng = StdRng::seed_from_u64(123);
        self.spawner = Spawner::new(OdMatrix::district1_poc(), TodProfile::hcmc_weekday(), 42);
        for (_, ctrl) in &mut self.signal_controllers {
            ctrl.reset();
        }
    }

    /// Run one simulation tick. Returns per-type instance arrays for rendering.
    pub fn tick(
        &mut self,
        base_dt: f64,
    ) -> (Vec<AgentInstance>, Vec<AgentInstance>, Vec<AgentInstance>) {
        if !self.sim_state.is_running() {
            return self.build_instances();
        }

        let dt = base_dt * self.speed_mult as f64;
        self.sim_time += dt;

        self.spawn_agents(dt);
        self.step_signals(dt);
        self.step_vehicles(dt);
        self.step_pedestrians(dt);
        self.detect_gridlock();
        self.remove_finished_agents();
        self.update_metrics();

        self.build_instances()
    }

    fn spawn_agents(&mut self, dt: f64) {
        let sim_hour = self.sim_time / 3600.0;
        let requests = self.spawner.generate_spawns(sim_hour, dt);
        for req in requests {
            self.spawn_single_agent(&req);
        }
    }

    fn spawn_single_agent(&mut self, req: &velos_demand::SpawnRequest) {
        let origin_pos = zone_centroid(req.origin);
        let dest_pos = zone_centroid(req.destination);

        let from_node = self.nearest_node(origin_pos);
        let to_node = self.nearest_node(dest_pos);

        if from_node == to_node {
            return;
        }

        let route_result = velos_net::find_route(&self.road_graph, from_node, to_node);
        let path = match route_result {
            Ok((path, _cost)) => path,
            Err(_) => return,
        };

        if path.len() < 2 {
            return;
        }

        let vtype = match req.vehicle_type {
            SpawnVehicleType::Motorbike => VehicleType::Motorbike,
            SpawnVehicleType::Car => VehicleType::Car,
            SpawnVehicleType::Pedestrian => VehicleType::Pedestrian,
        };

        let g = self.road_graph.inner();
        let edge_idx = g
            .find_edge(path[0], path[1])
            .map(|e| e.index() as u32)
            .unwrap_or(0);

        let start_pos = g[path[0]].pos;
        let next_pos = g[path[1]].pos;
        let heading = (next_pos[1] - start_pos[1]).atan2(next_pos[0] - start_pos[0]);

        let vehicle_type_for_params = match vtype {
            VehicleType::Motorbike => velos_vehicle::types::VehicleType::Motorbike,
            VehicleType::Car => velos_vehicle::types::VehicleType::Car,
            VehicleType::Pedestrian => velos_vehicle::types::VehicleType::Pedestrian,
        };
        let idm_params = default_idm_params(vehicle_type_for_params);

        let jitter_x = self.rng.gen_range(-5.0..5.0);
        let jitter_y = self.rng.gen_range(-5.0..5.0);

        let path_u32: Vec<u32> = path.iter().map(|n| n.index() as u32).collect();

        self.world.spawn((
            Position {
                x: start_pos[0] + jitter_x,
                y: start_pos[1] + jitter_y,
            },
            Kinematics {
                vx: heading.cos() * 0.1,
                vy: heading.sin() * 0.1,
                speed: 0.1,
                heading,
            },
            vtype,
            RoadPosition {
                edge_index: edge_idx,
                lane: 0,
                offset_m: 0.0,
            },
            Route {
                path: path_u32,
                current_step: 1,
            },
            WaitState {
                stopped_since: -1.0,
                at_red_signal: false,
            },
            idm_params,
        ));
    }

    fn nearest_node(&self, pos: [f64; 2]) -> NodeIndex {
        let g = self.road_graph.inner();
        let mut best = NodeIndex::new(0);
        let mut best_dist = f64::MAX;
        for node in g.node_indices() {
            let np = g[node].pos;
            let dx = np[0] - pos[0];
            let dy = np[1] - pos[1];
            let dist = dx * dx + dy * dy;
            if dist < best_dist {
                best_dist = dist;
                best = node;
            }
        }
        best
    }

    fn step_signals(&mut self, dt: f64) {
        for (_, ctrl) in &mut self.signal_controllers {
            ctrl.tick(dt);
        }
    }

    fn step_vehicles(&mut self, dt: f64) {
        // Collect all vehicle agent state for processing.
        let agents: Vec<(Entity, RoadPosition, f64, IdmParams, VehicleType)> = self
            .world
            .query_mut::<(Entity, &RoadPosition, &Kinematics, &IdmParams, &VehicleType)>()
            .into_iter()
            .filter(|(_, _, _, _, vt)| **vt != VehicleType::Pedestrian)
            .map(|(e, rp, kin, idm, vt)| (e, *rp, kin.speed, *idm, *vt))
            .collect();

        // Group agents by edge for leader detection.
        let mut edge_agents: HashMap<u32, Vec<(Entity, f64)>> = HashMap::new();
        for (entity, rp, _, _, _) in &agents {
            edge_agents
                .entry(rp.edge_index)
                .or_default()
                .push((*entity, rp.offset_m));
        }
        for agents_on_edge in edge_agents.values_mut() {
            agents_on_edge.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        }

        // Collect speed info for leader lookups.
        let speed_map: HashMap<Entity, f64> = agents
            .iter()
            .map(|(e, _, speed, _, _)| (*e, *speed))
            .collect();

        // Process each vehicle.
        let mut updates: Vec<(Entity, f64, f64, bool)> = Vec::with_capacity(agents.len());

        for (entity, rp, speed, idm_params, _) in &agents {
            let at_red = self.check_signal_red(rp);

            let (gap, delta_v) = if at_red {
                (2.0, *speed)
            } else {
                Self::find_leader_static(*entity, rp, &edge_agents, &speed_map, *speed)
            };

            let accel = idm_acceleration(idm_params, *speed, gap, delta_v);
            let (v_new, dx) = integrate_with_stopping_guard(*speed, accel, dt);

            updates.push((*entity, v_new, rp.offset_m + dx, at_red));
        }

        // Apply updates.
        for (entity, v_new, new_offset, at_red) in updates {
            let edge_idx_val = {
                let mut rp = self.world.query_one::<&RoadPosition>(entity);
                let Ok(rp) = rp.get() else { continue };
                rp.edge_index
            };

            let edge_idx = EdgeIndex::new(edge_idx_val as usize);
            let edge_length = self
                .road_graph
                .inner()
                .edge_weight(edge_idx)
                .map(|e| e.length_m)
                .unwrap_or(100.0);

            if new_offset >= edge_length {
                self.advance_to_next_edge(entity, new_offset - edge_length);
            } else {
                let rp_q = self.world.query_one_mut::<&mut RoadPosition>(entity).unwrap();
                rp_q.offset_m = new_offset;
            }

            self.update_agent_state(entity, v_new);
            self.update_wait_state(entity, v_new, at_red);
        }
    }

    fn check_signal_red(&self, rp: &RoadPosition) -> bool {
        let edge_idx = EdgeIndex::new(rp.edge_index as usize);
        let g = self.road_graph.inner();

        let Some(edge_endpoints) = g.edge_endpoints(edge_idx) else {
            return false;
        };
        let target_node = edge_endpoints.1;

        let edge_length = g
            .edge_weight(edge_idx)
            .map(|e| e.length_m)
            .unwrap_or(100.0);

        if rp.offset_m < edge_length - 10.0 {
            return false;
        }

        let target_node_u32 = target_node.index() as u32;
        if !self.signalized_nodes.contains_key(&target_node_u32) {
            return false;
        }

        for (ctrl_node, ctrl) in &self.signal_controllers {
            if *ctrl_node == target_node {
                let edges_at_node: Vec<_> = g.edges(target_node).collect();
                for (approach_idx, edge_ref) in edges_at_node.iter().enumerate() {
                    if edge_ref.id() == edge_idx {
                        return ctrl.get_phase_state(approach_idx) == PhaseState::Red;
                    }
                }
                return true;
            }
        }
        false
    }

    fn find_leader_static(
        entity: Entity,
        rp: &RoadPosition,
        edge_agents: &HashMap<u32, Vec<(Entity, f64)>>,
        speed_map: &HashMap<Entity, f64>,
        own_speed: f64,
    ) -> (f64, f64) {
        let Some(agents_on_edge) = edge_agents.get(&rp.edge_index) else {
            return (1000.0, 0.0);
        };

        let own_offset = rp.offset_m;
        let mut closest_gap = 1000.0_f64;
        let mut closest_delta_v = 0.0_f64;

        for (other_entity, other_offset) in agents_on_edge {
            if *other_entity == entity {
                continue;
            }
            let gap = other_offset - own_offset;
            if gap > 0.0 && gap < closest_gap {
                let leader_speed = speed_map.get(other_entity).copied().unwrap_or(0.0);
                closest_gap = gap;
                closest_delta_v = own_speed - leader_speed;
            }
        }

        (closest_gap, closest_delta_v)
    }

    fn advance_to_next_edge(&mut self, entity: Entity, overflow: f64) {
        let next_info = {
            let mut q = self.world.query_one::<(&Route, &RoadPosition)>(entity);
            let Ok((route, _rp)) = q.get() else { return };
            if route.current_step + 1 >= route.path.len() {
                None
            } else {
                let from = NodeIndex::new(route.path[route.current_step] as usize);
                let to = NodeIndex::new(route.path[route.current_step + 1] as usize);
                let edge = self.road_graph.inner().find_edge(from, to).map(|e| e.index() as u32);
                Some((edge, route.current_step + 1))
            }
        };

        match next_info {
            Some((Some(edge_idx), new_step)) => {
                let (route, rp) = self
                    .world
                    .query_one_mut::<(&mut Route, &mut RoadPosition)>(entity)
                    .unwrap();
                route.current_step = new_step;
                rp.edge_index = edge_idx;
                rp.offset_m = overflow;
            }
            _ => {
                let route = self.world.query_one_mut::<&mut Route>(entity).unwrap();
                route.current_step = route.path.len();
            }
        }
    }

    fn update_agent_state(&mut self, entity: Entity, new_speed: f64) {
        let edge_info = {
            let mut q = self.world.query_one::<&RoadPosition>(entity);
            let Ok(rp) = q.get() else { return };
            let edge_idx = EdgeIndex::new(rp.edge_index as usize);
            let g = self.road_graph.inner();
            g.edge_weight(edge_idx).map(|e| {
                let geom = &e.geometry;
                let frac = (rp.offset_m / e.length_m).clamp(0.0, 1.0);
                let start = geom[0];
                let end = geom[geom.len() - 1];
                let x = start[0] + (end[0] - start[0]) * frac;
                let y = start[1] + (end[1] - start[1]) * frac;
                let heading = (end[1] - start[1]).atan2(end[0] - start[0]);
                (x, y, heading)
            })
        };

        if let Some((x, y, heading)) = edge_info {
            let (pos, kin) = self
                .world
                .query_one_mut::<(&mut Position, &mut Kinematics)>(entity)
                .unwrap();
            pos.x = x;
            pos.y = y;
            kin.speed = new_speed;
            kin.heading = heading;
            kin.vx = new_speed * heading.cos();
            kin.vy = new_speed * heading.sin();
        }
    }

    fn update_wait_state(&mut self, entity: Entity, speed: f64, at_red: bool) {
        let ws = self.world.query_one_mut::<&mut WaitState>(entity).unwrap();
        if speed < 0.1 {
            if ws.stopped_since < 0.0 {
                ws.stopped_since = self.sim_time;
            }
            ws.at_red_signal = at_red;
        } else {
            ws.stopped_since = -1.0;
            ws.at_red_signal = false;
        }
    }

    fn step_pedestrians(&mut self, dt: f64) {
        let peds: Vec<(Entity, Vec<u32>, usize)> = self
            .world
            .query_mut::<(Entity, &VehicleType, &Route)>()
            .into_iter()
            .filter(|(_, vt, _)| **vt == VehicleType::Pedestrian)
            .map(|(e, _, r)| (e, r.path.clone(), r.current_step))
            .collect();

        for (entity, path, current_step) in peds {
            if current_step >= path.len() {
                continue;
            }

            let target_node = NodeIndex::new(path[current_step] as usize);
            let target_pos = self.road_graph.inner()[target_node].pos;

            let (pos, kin, route) = self
                .world
                .query_one_mut::<(&mut Position, &mut Kinematics, &mut Route)>(entity)
                .unwrap();

            let dx = target_pos[0] - pos.x;
            let dy = target_pos[1] - pos.y;
            let dist = (dx * dx + dy * dy).sqrt();
            let ped_speed = 1.4;

            if dist < ped_speed * dt {
                pos.x = target_pos[0];
                pos.y = target_pos[1];
                route.current_step += 1;
                kin.speed = 0.0;
            } else {
                let heading = dy.atan2(dx);
                pos.x += ped_speed * heading.cos() * dt;
                pos.y += ped_speed * heading.sin() * dt;
                kin.speed = ped_speed;
                kin.heading = heading;
                kin.vx = ped_speed * heading.cos();
                kin.vy = ped_speed * heading.sin();
            }
        }
    }

    fn detect_gridlock(&mut self) {
        let stopped: Vec<(Entity, RoadPosition)> = self
            .world
            .query_mut::<(Entity, &RoadPosition, &WaitState, &VehicleType)>()
            .into_iter()
            .filter(|(_, _, ws, vt)| {
                **vt != VehicleType::Pedestrian
                    && ws.stopped_since > 0.0
                    && (self.sim_time - ws.stopped_since) > self.gridlock_timeout
                    && !ws.at_red_signal
            })
            .map(|(e, rp, _, _)| (e, *rp))
            .collect();

        if stopped.is_empty() {
            return;
        }

        let mut edge_stopped: HashMap<u32, Vec<(Entity, f64)>> = HashMap::new();
        for (entity, rp) in &stopped {
            edge_stopped
                .entry(rp.edge_index)
                .or_default()
                .push((*entity, rp.offset_m));
        }

        let mut waiting_graph: HashMap<u32, u32> = HashMap::new();
        for (entity, rp) in &stopped {
            let eid = entity.id();
            if let Some(agents_on_edge) = edge_stopped.get(&rp.edge_index) {
                let mut closest_ahead: Option<u32> = None;
                let mut closest_gap = f64::MAX;
                for (other, other_offset) in agents_on_edge {
                    if *other == *entity {
                        continue;
                    }
                    let gap = other_offset - rp.offset_m;
                    if gap > 0.0 && gap < closest_gap {
                        closest_gap = gap;
                        closest_ahead = Some(other.id());
                    }
                }
                if let Some(blocker) = closest_ahead {
                    waiting_graph.insert(eid, blocker);
                }
            }
        }

        let cycles = detect_cycles(&waiting_graph);
        for cycle in &cycles {
            if let Some(&agent_id) = cycle.first() {
                self.teleport_agent_forward(agent_id);
            }
        }
    }

    fn teleport_agent_forward(&mut self, agent_id: u32) {
        let entity: Option<Entity> = self
            .world
            .query_mut::<(Entity, &Route)>()
            .into_iter()
            .find(|(e, _)| e.id() == agent_id)
            .map(|(e, _)| e);

        let Some(entity) = entity else { return };

        let next_pos = {
            let mut q = self.world.query_one::<&Route>(entity);
            let Ok(route) = q.get() else { return };
            if route.current_step + 1 < route.path.len() {
                let next_node = NodeIndex::new(route.path[route.current_step + 1] as usize);
                Some(self.road_graph.inner()[next_node].pos)
            } else {
                None
            }
        };

        if let Some(next_pos) = next_pos {
            let (pos, route, rp, ws) = self
                .world
                .query_one_mut::<(&mut Position, &mut Route, &mut RoadPosition, &mut WaitState)>(
                    entity,
                )
                .unwrap();
            pos.x = next_pos[0];
            pos.y = next_pos[1];
            route.current_step += 1;
            rp.offset_m = 0.0;
            ws.stopped_since = -1.0;

            if route.current_step + 1 < route.path.len() {
                let from = NodeIndex::new(route.path[route.current_step] as usize);
                let to = NodeIndex::new(route.path[route.current_step + 1] as usize);
                if let Some(edge) = self.road_graph.inner().find_edge(from, to) {
                    rp.edge_index = edge.index() as u32;
                }
            }
        }
    }

    fn remove_finished_agents(&mut self) {
        let finished: Vec<Entity> = self
            .world
            .query_mut::<(Entity, &Route)>()
            .into_iter()
            .filter(|(_, route)| route.current_step >= route.path.len())
            .map(|(e, _)| e)
            .collect();

        for entity in finished {
            let _ = self.world.despawn(entity);
        }
    }

    fn update_metrics(&mut self) {
        let mut motorbike_count = 0u32;
        let mut car_count = 0u32;
        let mut ped_count = 0u32;

        for vtype in self.world.query_mut::<&VehicleType>().into_iter() {
            match *vtype {
                VehicleType::Motorbike => motorbike_count += 1,
                VehicleType::Car => car_count += 1,
                VehicleType::Pedestrian => ped_count += 1,
            }
        }

        self.metrics.agent_count = motorbike_count + car_count + ped_count;
        self.metrics.motorbike_count = motorbike_count;
        self.metrics.car_count = car_count;
        self.metrics.ped_count = ped_count;
        self.metrics.sim_time = self.sim_time;
    }

    /// Build per-type instance arrays for rendering.
    pub fn build_instances(
        &self,
    ) -> (Vec<AgentInstance>, Vec<AgentInstance>, Vec<AgentInstance>) {
        let mut motorbikes = Vec::new();
        let mut cars = Vec::new();
        let mut pedestrians = Vec::new();

        for (pos, kin, vtype) in self
            .world
            .query::<(&Position, &Kinematics, &VehicleType)>()
            .iter()
        {
            let instance = AgentInstance {
                position: [pos.x as f32, pos.y as f32],
                heading: kin.heading as f32,
                _pad: 0.0,
                color: match *vtype {
                    VehicleType::Motorbike => [0.2, 0.8, 0.4, 1.0],
                    VehicleType::Car => [0.2, 0.4, 0.9, 1.0],
                    VehicleType::Pedestrian => [0.9, 0.9, 0.9, 1.0],
                },
            };

            match *vtype {
                VehicleType::Motorbike => motorbikes.push(instance),
                VehicleType::Car => cars.push(instance),
                VehicleType::Pedestrian => pedestrians.push(instance),
            }
        }

        (motorbikes, cars, pedestrians)
    }

    /// Compute bounding box center of the road network for initial camera.
    pub fn network_center(&self) -> (f32, f32) {
        let g = self.road_graph.inner();
        if g.node_count() == 0 {
            return (0.0, 0.0);
        }
        let mut min_x = f64::MAX;
        let mut max_x = f64::MIN;
        let mut min_y = f64::MAX;
        let mut max_y = f64::MIN;
        for node in g.node_indices() {
            let pos = g[node].pos;
            min_x = min_x.min(pos[0]);
            max_x = max_x.max(pos[0]);
            min_y = min_y.min(pos[1]);
            max_y = max_y.max(pos[1]);
        }
        (
            ((min_x + max_x) / 2.0) as f32,
            ((min_y + max_y) / 2.0) as f32,
        )
    }
}
