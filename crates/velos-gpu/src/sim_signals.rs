//! Signal-related SimWorld methods: detector updates, signal stepping, priority.
//!
//! Extracted from sim.rs to keep files under 700 lines.

use hecs::Entity;
use petgraph::graph::{EdgeIndex, NodeIndex};

use velos_core::components::{RoadPosition, VehicleType};
use velos_signal::detector::DetectorReading;
use velos_signal::priority::{PriorityLevel, PriorityRequest};

use crate::sim::SimWorld;
use velos_core::components::Kinematics;

impl SimWorld {
    /// Advance signal controllers with detector readings from loop detectors.
    ///
    /// Each controller receives only the readings from its own intersection's
    /// detectors. Fixed-time controllers ignore readings; actuated controllers
    /// use them for gap-out decisions.
    pub(crate) fn step_signals_with_detectors(
        &mut self,
        dt: f64,
        detector_readings: &[(NodeIndex, Vec<DetectorReading>)],
    ) {
        for (node, ctrl) in &mut self.signal_controllers {
            let old_phase = ctrl.get_phase_state(0);
            let readings = detector_readings
                .iter()
                .find(|(n, _)| n == node)
                .map_or(&[][..], |(_, r)| r.as_slice());
            ctrl.tick(dt, readings);
            let new_phase = ctrl.get_phase_state(0);
            if old_phase != new_phase {
                self.signal_dirty = true;
            }
        }
    }

    /// Check loop detectors for agent crossings.
    ///
    /// For each detector, scans agents on the same edge and checks if any
    /// agent's offset crossed the detector point this frame. Uses current
    /// ECS positions (RoadPosition.offset_m) compared against the previous
    /// frame's position stored in Kinematics.speed * dt approximation.
    pub(crate) fn update_loop_detectors(&self) -> Vec<(NodeIndex, Vec<DetectorReading>)> {
        let mut results = Vec::with_capacity(self.loop_detectors.len());

        for (node, detectors) in &self.loop_detectors {
            let mut readings = Vec::with_capacity(detectors.len());

            for (det_idx, detector) in detectors.iter().enumerate() {
                let mut triggered = false;

                // Scan agents on this detector's edge
                for (rp, kin) in self
                    .world
                    .query::<(&RoadPosition, &Kinematics)>()
                    .iter()
                {
                    if rp.edge_index != detector.edge_id {
                        continue;
                    }

                    // Approximate previous position: current offset minus distance
                    // traveled this frame. For forward-only detection this is
                    // sufficient (LoopDetector::check uses prev < offset <= cur).
                    let cur_pos = rp.offset_m;
                    // Use a small dt estimate; the exact dt doesn't matter much
                    // since we only need to know if the agent crossed the point.
                    // Speed * 1 tick at base dt gives a conservative estimate.
                    let prev_pos = (cur_pos - kin.speed.abs() * 0.1).max(0.0);

                    if detector.check(prev_pos, cur_pos) {
                        triggered = true;
                        break; // One trigger per detector per frame is sufficient
                    }
                }

                readings.push(DetectorReading {
                    detector_index: det_idx,
                    triggered,
                });
            }

            results.push((*node, readings));
        }

        results
    }

    /// Process signal priority requests from bus and emergency vehicles.
    ///
    /// Scans vehicles near signalized intersections (within 100m of the
    /// intersection node) and submits priority requests for bus and
    /// emergency vehicle types.
    pub(crate) fn step_signal_priority(&mut self) {
        // Collect priority requests (avoid borrow conflict with self)
        let mut requests: Vec<(NodeIndex, PriorityRequest)> = Vec::new();

        let g = self.road_graph.inner();

        for (entity, rp, vtype) in self
            .world
            .query::<(Entity, &RoadPosition, &VehicleType)>()
            .iter()
        {
            let level = match *vtype {
                VehicleType::Bus => PriorityLevel::Bus,
                VehicleType::Emergency => PriorityLevel::Emergency,
                _ => continue,
            };

            // Check if agent's edge connects to a signalized node
            let edge_idx = EdgeIndex::new(rp.edge_index as usize);
            let Some(endpoints) = g.edge_endpoints(edge_idx) else {
                continue;
            };
            let target_node = endpoints.1;
            let target_id = target_node.index() as u32;

            if !self.signalized_nodes.contains_key(&target_id) {
                continue;
            }

            // Check proximity: agent must be within 100m of intersection
            let edge_length = g
                .edge_weight(edge_idx)
                .map(|e| e.length_m)
                .unwrap_or(100.0);
            let distance_to_intersection = edge_length - rp.offset_m;
            if distance_to_intersection > 100.0 {
                continue;
            }

            // Determine approach index for this edge
            let incoming: Vec<_> = g
                .edges_directed(target_node, petgraph::Direction::Incoming)
                .collect();
            let approach_index = incoming
                .iter()
                .position(|e| {
                    use petgraph::visit::EdgeRef;
                    e.id() == edge_idx
                })
                .unwrap_or(0);

            requests.push((
                target_node,
                PriorityRequest {
                    approach_index,
                    level,
                    vehicle_id: entity.id(),
                },
            ));
        }

        // Submit requests to the matching signal controllers
        for (target_node, request) in &requests {
            for (ctrl_node, ctrl) in &mut self.signal_controllers {
                if ctrl_node == target_node {
                    ctrl.request_priority(request);
                    break;
                }
            }
        }
    }
}
