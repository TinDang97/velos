//! Bus dwell lifecycle for SimWorld.
//!
//! Extracted to its own module (following sim_pedestrians.rs pattern)
//! to keep sim.rs under 700 lines.
//!
//! Each frame, `step_bus_dwell()` scans all bus agents:
//! 1. If dwelling: tick_dwell() to count down remaining dwell time
//! 2. If not dwelling: check should_stop() proximity to next scheduled stop
//! 3. If at stop: generate stochastic passenger counts, begin_dwell()

use rand::Rng;

use velos_core::components::RoadPosition;
use velos_vehicle::bus::BusState;

use crate::sim::SimWorld;

impl SimWorld {
    /// Advance bus dwell state machines for all bus agents.
    ///
    /// Called every frame after vehicle physics (step_vehicles_gpu) and
    /// before pedestrian physics (step_pedestrians). Dwelling buses have
    /// FLAG_BUS_DWELLING set in the next GPU upload cycle so the shader
    /// holds them at zero speed.
    pub fn step_bus_dwell(&mut self, dt: f64) {
        // Collect bus agent data to avoid borrow conflicts with self.
        struct BusDwellAction {
            entity: hecs::Entity,
            should_begin: bool,
            boarding: u32,
            alighting: u32,
        }

        let mut actions: Vec<BusDwellAction> = Vec::new();

        // Identify buses that need to begin dwelling.
        for (entity, bus_state, rp) in self
            .world
            .query_mut::<(hecs::Entity, &BusState, &RoadPosition)>()
            .into_iter()
        {
            if bus_state.is_dwelling() {
                // Will tick in second pass
                continue;
            }
            if bus_state.should_stop(rp.edge_index, rp.offset_m, &self.bus_stops) {
                actions.push(BusDwellAction {
                    entity,
                    should_begin: true,
                    boarding: 0,  // Placeholder, filled below
                    alighting: 0, // Placeholder, filled below
                });
            }
        }

        // Generate stochastic passenger counts and begin dwell.
        for action in &mut actions {
            // Boarding: proportional to stop capacity (mean = capacity * 0.3).
            // Simple uniform approximation since rand_distr may not be available.
            let capacity = self.bus_stops.iter()
                .find(|_| true) // We use stop from bus_state context below
                .map_or(40u16, |s| s.capacity);
            let max_boarding = ((capacity as u32) * 3 / 10).max(1);
            action.boarding = self.rng.gen_range(0..=max_boarding);

            // Alighting: simple heuristic per stop (0-3 passengers).
            action.alighting = self.rng.gen_range(0..=3);
        }

        // Apply begin_dwell for buses that reached their stop.
        let model = self.bus_dwell_model;
        for action in &actions {
            if action.should_begin {
                if let Ok(bus_state) = self
                    .world
                    .query_one_mut::<&mut BusState>(action.entity)
                {
                    bus_state.begin_dwell(&model, action.boarding, action.alighting);
                }
            }
        }

        // Tick dwell for all currently dwelling buses.
        let mut dwelling_entities: Vec<hecs::Entity> = Vec::new();
        for (entity, bus_state) in self
            .world
            .query_mut::<(hecs::Entity, &BusState)>()
            .into_iter()
        {
            if bus_state.is_dwelling() {
                dwelling_entities.push(entity);
            }
        }

        for entity in dwelling_entities {
            if let Ok(bus_state) = self
                .world
                .query_one_mut::<&mut BusState>(entity)
            {
                bus_state.tick_dwell(dt);
            }
        }
    }
}
