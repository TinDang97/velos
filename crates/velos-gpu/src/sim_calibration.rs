//! API command processing and calibration overlay computation for SimWorld.
//!
//! Drains gRPC commands via try_recv each frame, computes calibration
//! ratios from observed vs simulated counts, and swaps the overlay.

use std::collections::HashMap;

use velos_api::bridge::ApiCommand;
use velos_api::calibration::compute_calibration_factors;
use velos_api::proto::velos::v2::RegisterCameraResponse;
use velos_core::components::RoadPosition;
use velos_demand::Zone;
use velos_net::RoadGraph;

use crate::sim::SimWorld;

/// Build a mapping from edge ID to the nearest zone.
///
/// For each edge, find the zone whose centroid is closest to the edge midpoint.
/// Used by calibration to map camera-covered edges to OD pair zones.
pub(crate) fn build_edge_to_zone(
    graph: &RoadGraph,
    centroids: &HashMap<Zone, [f64; 2]>,
) -> HashMap<u32, Zone> {
    let g = graph.inner();
    let mut mapping = HashMap::new();

    if centroids.is_empty() {
        return mapping;
    }

    for edge_idx in g.edge_indices() {
        let edge_id = edge_idx.index() as u32;
        let (src, tgt) = g.edge_endpoints(edge_idx).unwrap();
        let sp = g[src].pos;
        let tp = g[tgt].pos;
        let mid = [(sp[0] + tp[0]) / 2.0, (sp[1] + tp[1]) / 2.0];

        let mut best_zone = None;
        let mut best_dist = f64::MAX;
        for (&zone, &pos) in centroids {
            let dx = mid[0] - pos[0];
            let dy = mid[1] - pos[1];
            let dist = dx * dx + dy * dy;
            if dist < best_dist {
                best_dist = dist;
                best_zone = Some(zone);
            }
        }
        if let Some(zone) = best_zone {
            mapping.insert(edge_id, zone);
        }
    }

    mapping
}

/// Calibration recomputation interval in sim-seconds (5 minutes).
const CALIBRATION_INTERVAL_SECS: f64 = 300.0;

/// Maximum API commands to process per frame to prevent frame spikes.
const MAX_COMMANDS_PER_FRAME: usize = 64;

impl SimWorld {
    /// Drain pending API commands from the gRPC bridge (non-blocking).
    ///
    /// Processes up to [`MAX_COMMANDS_PER_FRAME`] commands per frame.
    /// RegisterCamera commands are forwarded to the camera registry and
    /// replied via oneshot. DetectionBatch commands are ingested into
    /// the aggregator.
    pub(crate) fn step_api_commands(&mut self) {
        let bridge = match &mut self.api_bridge {
            Some(b) => b,
            None => return,
        };

        let commands = bridge.drain(MAX_COMMANDS_PER_FRAME);
        for cmd in commands {
            match cmd {
                ApiCommand::RegisterCamera { request, reply } => {
                    // Camera is already registered locally in the gRPC handler.
                    // Here we just acknowledge via the oneshot channel.
                    let camera = {
                        let reg = self.camera_registry.lock().unwrap();
                        reg.get(reg.list().last().map(|c| c.id).unwrap_or(0))
                            .cloned()
                    };

                    let response = if let Some(cam) = camera {
                        RegisterCameraResponse {
                            camera_id: cam.id,
                            covered_edge_ids: cam.covered_edges.clone(),
                        }
                    } else {
                        // Fallback: use request name as camera wasn't found
                        RegisterCameraResponse {
                            camera_id: 0,
                            covered_edge_ids: vec![],
                        }
                    };

                    let _ = reply.send(response);
                    log::info!(
                        "Registered camera '{}' via SimWorld bridge",
                        request.name
                    );
                }
                ApiCommand::DetectionBatch { batch } => {
                    // Detection batches are already ingested by the gRPC handler
                    // into the shared aggregator. This is a notification that
                    // the simulation can use for bookkeeping if needed.
                    log::trace!(
                        "SimWorld received detection batch {}, {} events",
                        batch.batch_id,
                        batch.events.len()
                    );
                }
            }
        }
    }

    /// Recompute calibration factors if enough sim-time has elapsed.
    ///
    /// Collects simulated agent counts per camera by querying the ECS
    /// for agents on edges covered by each camera. Then computes
    /// observed/simulated ratios with EMA smoothing and swaps the
    /// calibration overlay.
    pub(crate) fn step_calibration(&mut self) {
        // Check if calibration interval has elapsed
        if self.sim_time - self.last_calibration_time < CALIBRATION_INTERVAL_SECS {
            return;
        }
        self.last_calibration_time = self.sim_time;

        // Collect simulated counts: for each camera, count agents on covered edges
        let registry = self.camera_registry.lock().unwrap();
        let cameras = registry.list();

        if cameras.is_empty() {
            return;
        }

        // Build edge -> camera mapping for fast lookup
        let mut edge_to_cameras: HashMap<u32, Vec<u32>> = HashMap::new();
        for cam in &cameras {
            for &edge_id in &cam.covered_edges {
                edge_to_cameras
                    .entry(edge_id)
                    .or_default()
                    .push(cam.id);
            }
        }
        drop(registry); // release lock before ECS query

        // Count agents per camera via ECS query
        self.simulated_counts.clear();
        for rp in self.world.query_mut::<&RoadPosition>().into_iter() {
            if let Some(cam_ids) = edge_to_cameras.get(&rp.edge_index) {
                for &cam_id in cam_ids {
                    *self.simulated_counts.entry(cam_id).or_insert(0) += 1;
                }
            }
        }

        // Compute calibration factors
        let registry = self.camera_registry.lock().unwrap();
        let aggregator = self.aggregator.lock().unwrap();

        let overlay = compute_calibration_factors(
            &registry,
            &aggregator,
            &self.simulated_counts,
            &mut self.calibration_states,
            &self.edge_to_zone,
            self.sim_time,
        );

        let factor_count = overlay.factors.len();
        self.calibration_store.swap(overlay);

        if factor_count > 0 {
            log::debug!(
                "Calibration overlay updated: {} OD pair factors at sim_time={:.0}",
                factor_count,
                self.sim_time
            );
        }
    }
}
