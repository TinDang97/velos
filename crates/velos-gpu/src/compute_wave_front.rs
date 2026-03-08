//! Wave-front upload, dispatch, readback, and helper functions for ComputeDispatcher.
//!
//! Extracted from compute.rs to keep files under 700 lines.
//! Contains the wave-front pipeline data management (upload, dispatch, readback)
//! and utility functions (sort_agents_by_lane, compute_agent_flags, bgl_entry).

use std::collections::HashMap;

use velos_core::components::GpuAgentState;

use crate::compute::{ComputeDispatcher, WaveFrontParams};

impl ComputeDispatcher {
    /// Upload agent states and lane sorting data to GPU for wave-front dispatch.
    pub fn upload_wave_front_data(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        agents: &[GpuAgentState],
        lane_offsets: &[u32],
        lane_counts: &[u32],
        lane_agents: &[u32],
    ) {
        let agent_bytes = std::mem::size_of_val(agents) as u64;
        let offsets_bytes = std::mem::size_of_val(lane_offsets) as u64;
        let counts_bytes = std::mem::size_of_val(lane_counts) as u64;
        let agents_idx_bytes = std::mem::size_of_val(lane_agents) as u64;

        let needs_recreate = self.agent_buffer.as_ref().is_none_or(|b| b.size() < agent_bytes)
            || self.lane_offsets_buffer.as_ref().is_none_or(|b| b.size() < offsets_bytes)
            || self.lane_counts_buffer.as_ref().is_none_or(|b| b.size() < counts_bytes)
            || self.lane_agents_buffer.as_ref().is_none_or(|b| b.size() < agents_idx_bytes);

        if needs_recreate {
            let storage_rw = wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC;
            let storage_r = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST;

            self.agent_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("wf_agents"),
                size: agent_bytes.max(32),
                usage: storage_rw,
                mapped_at_creation: false,
            }));
            self.lane_offsets_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("wf_lane_offsets"),
                size: offsets_bytes.max(4),
                usage: storage_r,
                mapped_at_creation: false,
            }));
            self.lane_counts_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("wf_lane_counts"),
                size: counts_bytes.max(4),
                usage: storage_r,
                mapped_at_creation: false,
            }));
            self.lane_agents_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("wf_lane_agents"),
                size: agents_idx_bytes.max(4),
                usage: storage_r,
                mapped_at_creation: false,
            }));
            self.staging_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("wf_staging"),
                size: agent_bytes.max(32),
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }

        if !agents.is_empty() {
            queue.write_buffer(
                self.agent_buffer.as_ref().unwrap(),
                0,
                bytemuck::cast_slice(agents),
            );
        }
        if !lane_offsets.is_empty() {
            queue.write_buffer(
                self.lane_offsets_buffer.as_ref().unwrap(),
                0,
                bytemuck::cast_slice(lane_offsets),
            );
        }
        if !lane_counts.is_empty() {
            queue.write_buffer(
                self.lane_counts_buffer.as_ref().unwrap(),
                0,
                bytemuck::cast_slice(lane_counts),
            );
        }
        if !lane_agents.is_empty() {
            queue.write_buffer(
                self.lane_agents_buffer.as_ref().unwrap(),
                0,
                bytemuck::cast_slice(lane_agents),
            );
        }

        self.wave_front_agent_count = agents.len() as u32;
        self.wave_front_lane_count = lane_counts.len() as u32;
    }

    /// Encode a wave-front compute dispatch. One workgroup per lane.
    pub fn dispatch_wave_front(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        dt: f32,
    ) {
        if self.wave_front_lane_count == 0 {
            return;
        }

        let params = WaveFrontParams {
            agent_count: self.wave_front_agent_count,
            dt,
            step_counter: self.step_counter,
            emergency_count: self.emergency_count,
            sign_count: self.sign_count,
            sim_time: self.sim_time,
            _pad0: 0,
            _pad1: 0,
        };
        queue.write_buffer(&self.wf_params_buffer, 0, bytemuck::bytes_of(&params));

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("wave_front_bg"),
            layout: &self.wf_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.wf_params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.agent_buffer.as_ref().unwrap().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.lane_offsets_buffer.as_ref().unwrap().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.lane_counts_buffer.as_ref().unwrap().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.lane_agents_buffer.as_ref().unwrap().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: self.emergency_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: self.sign_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: self.vehicle_params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: self.perception_result_buffer.as_entire_binding(),
                },
            ],
        });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("wave_front_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.wf_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            const MAX_WG: u32 = 65535;
            let x = self.wave_front_lane_count.min(MAX_WG);
            let y = self.wave_front_lane_count.div_ceil(MAX_WG);
            pass.dispatch_workgroups(x, y, 1);
        }

        self.step_counter += 1;
    }

    /// Read back updated agent states from GPU after wave-front dispatch.
    pub fn readback_wave_front_agents(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Vec<GpuAgentState> {
        let count = self.wave_front_agent_count as usize;
        if count == 0 {
            return Vec::new();
        }

        let byte_size = (count * std::mem::size_of::<GpuAgentState>()) as u64;
        let staging = self.staging_buffer.as_ref().unwrap();

        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(
            self.agent_buffer.as_ref().unwrap(),
            0,
            staging,
            0,
            byte_size,
        );
        queue.submit(std::iter::once(encoder.finish()));

        let slice = staging.slice(..byte_size);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = device.poll(wgpu::PollType::wait_indefinitely());

        let data = slice.get_mapped_range();
        let agents: Vec<GpuAgentState> = bytemuck::cast_slice(&data).to_vec();
        drop(data);
        staging.unmap();

        agents
    }
}

/// Compute agent flags bitfield from vehicle state.
///
/// Combines FLAG_BUS_DWELLING (bit 0), FLAG_EMERGENCY_ACTIVE (bit 1),
/// and agent profile ID (bits 4-7) via `encode_profile_in_flags`.
/// Used by `step_vehicles_gpu()` when building GpuAgentState.
pub fn compute_agent_flags(
    is_bus_dwelling: bool,
    is_emergency: bool,
    profile: velos_core::cost::AgentProfile,
) -> u32 {
    let mut f = 0u32;
    if is_bus_dwelling {
        f |= 1; // FLAG_BUS_DWELLING
    }
    if is_emergency {
        f |= 2; // FLAG_EMERGENCY_ACTIVE
    }
    velos_core::cost::encode_profile_in_flags(f, profile)
}

/// Helper to create a bind group layout entry.
pub(crate) fn bgl_entry(
    binding: u32,
    ty: wgpu::BufferBindingType,
    _has_dynamic_offset: bool,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

/// Sort agents by lane for wave-front dispatch.
pub fn sort_agents_by_lane(
    agents: &[GpuAgentState],
) -> (Vec<u32>, Vec<u32>, Vec<u32>) {
    if agents.is_empty() {
        return (vec![0], vec![0], Vec::new());
    }

    let mut lane_map: HashMap<(u32, u32), Vec<(u32, i32)>> = HashMap::new();
    for (idx, agent) in agents.iter().enumerate() {
        lane_map
            .entry((agent.edge_id, agent.lane_idx))
            .or_default()
            .push((idx as u32, agent.position));
    }

    let mut lane_keys: Vec<(u32, u32)> = lane_map.keys().copied().collect();
    lane_keys.sort();

    let num_lanes = lane_keys.len();
    let mut lane_offsets = Vec::with_capacity(num_lanes);
    let mut lane_counts = Vec::with_capacity(num_lanes);
    let mut lane_agent_indices = Vec::with_capacity(agents.len());

    for key in &lane_keys {
        let group = lane_map.get_mut(key).unwrap();
        group.sort_by(|a, b| b.1.cmp(&a.1));

        lane_offsets.push(lane_agent_indices.len() as u32);
        lane_counts.push(group.len() as u32);
        for &(agent_idx, _) in group.iter() {
            lane_agent_indices.push(agent_idx);
        }
    }

    (lane_offsets, lane_counts, lane_agent_indices)
}
