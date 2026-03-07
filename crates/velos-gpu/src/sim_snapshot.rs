//! Per-frame agent snapshot for spatial queries and cross-type interactions.
//!
//! Built once per tick from ECS world, shared across step functions to avoid
//! redundant queries and enable cross-type collision avoidance.

use std::collections::HashMap;

use hecs::World;

use velos_core::components::{Kinematics, LateralOffset, Position, RoadPosition, VehicleType};

/// Frame snapshot of all agent state needed for spatial queries.
///
/// Parallel vectors indexed by position in `ids`. Built once per frame
/// by `collect_all_agents()`, consumed by step functions and spatial index.
pub struct AgentSnapshot {
    /// Entity IDs as u32 for spatial index compatibility.
    pub ids: Vec<u32>,
    /// World positions [x, y] in metres.
    pub positions: Vec<[f64; 2]>,
    /// Vehicle type per agent.
    pub vehicle_types: Vec<VehicleType>,
    /// Scalar speed per agent (m/s).
    pub speeds: Vec<f64>,
    /// Heading in radians per agent.
    pub headings: Vec<f64>,
    /// Lateral offset if agent is a motorbike (None for cars/pedestrians).
    pub lateral_offsets: Vec<Option<f64>>,
    /// Road position edge index per agent.
    pub edge_indices: Vec<u32>,
    /// Map from entity id (u32) to index in the parallel vecs.
    pub id_to_index: HashMap<u32, usize>,
}

impl AgentSnapshot {
    /// Build a snapshot of all agents from the ECS world.
    ///
    /// Queries all entities with (Position, Kinematics, VehicleType, RoadPosition)
    /// and optionally LateralOffset. Returns parallel vectors for spatial index
    /// construction and neighbor lookups.
    pub fn collect(world: &World) -> Self {
        let mut ids = Vec::new();
        let mut positions = Vec::new();
        let mut vehicle_types = Vec::new();
        let mut speeds = Vec::new();
        let mut headings = Vec::new();
        let mut lateral_offsets = Vec::new();
        let mut edge_indices = Vec::new();
        let mut id_to_index = HashMap::new();

        for (pos, kin, vtype, rp, lat) in world
            .query::<(
                &Position,
                &Kinematics,
                &VehicleType,
                &RoadPosition,
                Option<&LateralOffset>,
            )>()
            .iter()
        {
            let idx = ids.len();
            let eid = idx as u32;
            ids.push(eid);
            positions.push([pos.x, pos.y]);
            vehicle_types.push(*vtype);
            speeds.push(kin.speed);
            headings.push(kin.heading);
            lateral_offsets.push(lat.map(|l: &LateralOffset| l.lateral_offset));
            edge_indices.push(rp.edge_index);
            id_to_index.insert(eid, idx);
        }

        Self {
            ids,
            positions,
            vehicle_types,
            speeds,
            headings,
            lateral_offsets,
            edge_indices,
            id_to_index,
        }
    }

    /// Look up vehicle type by entity id.
    pub fn vehicle_type(&self, id: u32) -> Option<VehicleType> {
        self.id_to_index.get(&id).map(|&i| self.vehicle_types[i])
    }

    /// Look up speed by entity id.
    pub fn speed(&self, id: u32) -> Option<f64> {
        self.id_to_index.get(&id).map(|&i| self.speeds[i])
    }

    /// Look up heading by entity id.
    pub fn heading(&self, id: u32) -> Option<f64> {
        self.id_to_index.get(&id).map(|&i| self.headings[i])
    }

    /// Look up lateral offset by entity id (None if not a motorbike).
    pub fn lateral_offset(&self, id: u32) -> Option<f64> {
        self.id_to_index
            .get(&id)
            .and_then(|&i| self.lateral_offsets[i])
    }

    /// Half-width by vehicle type for collision avoidance.
    pub fn half_width_for_type(vtype: VehicleType) -> f64 {
        match vtype {
            VehicleType::Motorbike => 0.25,
            VehicleType::Car => 0.9,
            VehicleType::Pedestrian => 0.3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use velos_core::components::{Kinematics, Position, RoadPosition, VehicleType};

    #[test]
    fn collect_empty_world() {
        let world = World::new();
        let snap = AgentSnapshot::collect(&world);
        assert!(snap.ids.is_empty());
        assert!(snap.positions.is_empty());
    }

    #[test]
    fn collect_mixed_agents() {
        let mut world = World::new();

        // Spawn a car (no LateralOffset)
        world.spawn((
            Position { x: 10.0, y: 20.0 },
            Kinematics {
                vx: 1.0,
                vy: 0.0,
                speed: 1.0,
                heading: 0.0,
            },
            VehicleType::Car,
            RoadPosition {
                edge_index: 0,
                lane: 0,
                offset_m: 5.0,
            },
        ));

        // Spawn a motorbike with LateralOffset
        world.spawn((
            Position { x: 15.0, y: 25.0 },
            Kinematics {
                vx: 2.0,
                vy: 0.0,
                speed: 2.0,
                heading: 0.0,
            },
            VehicleType::Motorbike,
            RoadPosition {
                edge_index: 1,
                lane: 0,
                offset_m: 3.0,
            },
            LateralOffset {
                lateral_offset: 1.5,
                desired_lateral: 1.5,
            },
        ));

        let snap = AgentSnapshot::collect(&world);
        assert_eq!(snap.ids.len(), 2);
        assert_eq!(snap.positions.len(), 2);

        // Check that motorbike has lateral offset and car doesn't
        let mut has_lateral = false;
        let mut has_none = false;
        for lo in &snap.lateral_offsets {
            match lo {
                Some(_) => has_lateral = true,
                None => has_none = true,
            }
        }
        assert!(has_lateral, "motorbike should have lateral offset");
        assert!(has_none, "car should not have lateral offset");
    }

    #[test]
    fn half_width_values() {
        assert!((AgentSnapshot::half_width_for_type(VehicleType::Motorbike) - 0.25).abs() < 1e-10);
        assert!((AgentSnapshot::half_width_for_type(VehicleType::Car) - 0.9).abs() < 1e-10);
        assert!(
            (AgentSnapshot::half_width_for_type(VehicleType::Pedestrian) - 0.3).abs() < 1e-10
        );
    }
}
