//! ComputeDispatcher: WGSL shader pipelines for agent position update.
//!
//! Three pipeline families:
//! 1. Legacy `agent_update.wgsl`: simple parallel Euler integration (backward compat).
//! 2. Wave-front `wave_front.wgsl`: per-lane sequential dispatch with IDM+Krauss branching.
//! 3. Pedestrian adaptive `pedestrian_adaptive.wgsl`: density-adaptive spatial hash with
//!    prefix-sum compaction and social force model (4-pass pipeline).
//!
//! The wave-front pipeline is the production path for vehicles. The pedestrian adaptive
//! pipeline handles pedestrian social force with adaptive workgroup dispatch.

use std::collections::HashMap;

use velos_core::components::GpuAgentState;

use crate::buffers::BufferPool;

/// Uniform params buffer layout. Must match WGSL `struct Params` in both shaders.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct DispatchParams {
    agent_count: u32,
    dt: f32,
    _pad0: u32,
    _pad1: u32,
}

/// Wave-front params: matches WGSL `struct Params` in wave_front.wgsl.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct WaveFrontParams {
    agent_count: u32,
    dt: f32,
    step_counter: u32,
    emergency_count: u32,
    sign_count: u32,
    sim_time: f32,
    _pad0: u32,
    _pad1: u32,
}

/// GPU-side emergency vehicle data for yield cone detection.
/// Matches WGSL `struct EmergencyVehicle` in wave_front.wgsl.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuEmergencyVehicle {
    pub pos_x: f32,
    pub pos_y: f32,
    pub heading: f32,
    pub _pad: f32,
}

/// GPU-side pedestrian state for adaptive social force dispatch.
/// Matches WGSL `struct Pedestrian` in pedestrian_adaptive.wgsl.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuPedestrian {
    pub pos_x: f32,
    pub pos_y: f32,
    pub vel_x: f32,
    pub vel_y: f32,
    pub dest_x: f32,
    pub dest_y: f32,
    pub radius: f32,
    pub _pad: f32,
}

/// Uniform params for pedestrian adaptive dispatch.
/// Matches WGSL `struct PedestrianParams` in pedestrian_adaptive.wgsl.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PedestrianAdaptiveParams {
    pub ped_count: u32,
    pub cell_count: u32,
    pub grid_w: u32,
    pub grid_h: u32,
    pub cell_size: f32,
    pub dt: f32,
    pub a_social: f32,
    pub b_social: f32,
    pub tau: f32,
    pub desired_speed: f32,
    pub lambda: f32,
    pub max_force: f32,
    pub max_speed: f32,
    pub radius: f32,
    pub workgroup_count: u32,
    pub _pad: u32,
}

impl Default for PedestrianAdaptiveParams {
    fn default() -> Self {
        Self {
            ped_count: 0,
            cell_count: 0,
            grid_w: 0,
            grid_h: 0,
            cell_size: 5.0,
            dt: 0.1,
            a_social: 2000.0,
            b_social: 0.08,
            tau: 0.5,
            desired_speed: 1.2,
            lambda: 0.5,
            max_force: 50.0,
            max_speed: 2.0,
            radius: 0.3,
            workgroup_count: 0,
            _pad: 0,
        }
    }
}

const WORKGROUP_SIZE: u32 = 256;
const PED_SF_WORKGROUP_SIZE: u32 = 64;

/// Owns the compute pipelines and bind group layouts for agent updates.
pub struct ComputeDispatcher {
    // Legacy pipeline (agent_update.wgsl)
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    params_buffer: wgpu::Buffer,

    // Wave-front pipeline (wave_front.wgsl)
    wf_pipeline: wgpu::ComputePipeline,
    wf_bind_group_layout: wgpu::BindGroupLayout,
    wf_params_buffer: wgpu::Buffer,

    // Wave-front GPU buffers for lane data + agent state
    agent_buffer: Option<wgpu::Buffer>,
    lane_offsets_buffer: Option<wgpu::Buffer>,
    lane_counts_buffer: Option<wgpu::Buffer>,
    lane_agents_buffer: Option<wgpu::Buffer>,
    staging_buffer: Option<wgpu::Buffer>,
    emergency_buffer: wgpu::Buffer,

    /// Current agent count in GPU buffers.
    pub wave_front_agent_count: u32,
    /// Current lane count for dispatch.
    pub wave_front_lane_count: u32,
    /// Current step counter for RNG seeding.
    pub step_counter: u32,
    /// Number of active emergency vehicles (0 = early-exit in shader).
    pub emergency_count: u32,
    /// Number of traffic signs in the sign buffer.
    pub sign_count: u32,
    /// Current simulation time in seconds (for school zone time windows).
    pub sim_time: f32,

    // --- Pedestrian adaptive pipeline ---
    ped_count_pipeline: wgpu::ComputePipeline,
    ped_prefix_local_pipeline: wgpu::ComputePipeline,
    ped_prefix_wg_sums_pipeline: wgpu::ComputePipeline,
    ped_prefix_propagate_pipeline: wgpu::ComputePipeline,
    ped_scatter_pipeline: wgpu::ComputePipeline,
    ped_social_force_pipeline: wgpu::ComputePipeline,
    ped_bind_group_layout: wgpu::BindGroupLayout,
    ped_params_buffer: wgpu::Buffer,
    ped_buffer: Option<wgpu::Buffer>,
    ped_cell_counts_buffer: Option<wgpu::Buffer>,
    ped_cell_offsets_buffer: Option<wgpu::Buffer>,
    ped_compacted_indices_buffer: Option<wgpu::Buffer>,
    ped_cell_map_buffer: Option<wgpu::Buffer>,
    ped_scatter_counters_buffer: Option<wgpu::Buffer>,
    ped_workgroup_sums_buffer: Option<wgpu::Buffer>,
    ped_staging_buffer: Option<wgpu::Buffer>,
    /// Current pedestrian count in GPU buffers.
    pub ped_adaptive_count: u32,
}

impl ComputeDispatcher {
    /// Create both compute pipelines from embedded WGSL shaders.
    pub fn new(device: &wgpu::Device) -> Self {
        // --- Legacy pipeline ---
        let shader =
            device.create_shader_module(wgpu::include_wgsl!("../shaders/agent_update.wgsl"));

        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("compute_bgl"),
                entries: &[
                    bgl_entry(0, wgpu::BufferBindingType::Uniform, false),
                    bgl_entry(1, wgpu::BufferBindingType::Storage { read_only: true }, false),
                    bgl_entry(2, wgpu::BufferBindingType::Storage { read_only: true }, false),
                    bgl_entry(3, wgpu::BufferBindingType::Storage { read_only: false }, false),
                    bgl_entry(4, wgpu::BufferBindingType::Storage { read_only: false }, false),
                ],
            });

        let pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("compute_pipeline_layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("agent_update_pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dispatch_params"),
            size: std::mem::size_of::<DispatchParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // --- Wave-front pipeline ---
        let wf_shader =
            device.create_shader_module(wgpu::include_wgsl!("../shaders/wave_front.wgsl"));

        let wf_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("wave_front_bgl"),
                entries: &[
                    // binding 0: Params uniform
                    bgl_entry(0, wgpu::BufferBindingType::Uniform, false),
                    // binding 1: agents (read_write storage)
                    bgl_entry(1, wgpu::BufferBindingType::Storage { read_only: false }, false),
                    // binding 2: lane_offsets (read-only storage)
                    bgl_entry(2, wgpu::BufferBindingType::Storage { read_only: true }, false),
                    // binding 3: lane_counts (read-only storage)
                    bgl_entry(3, wgpu::BufferBindingType::Storage { read_only: true }, false),
                    // binding 4: lane_agents (read-only storage)
                    bgl_entry(4, wgpu::BufferBindingType::Storage { read_only: true }, false),
                    // binding 5: emergency_vehicles (read-only storage)
                    bgl_entry(5, wgpu::BufferBindingType::Storage { read_only: true }, false),
                ],
            });

        let wf_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("wave_front_pipeline_layout"),
                bind_group_layouts: &[&wf_bind_group_layout],
                push_constant_ranges: &[],
            });

        let wf_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("wave_front_pipeline"),
            layout: Some(&wf_pipeline_layout),
            module: &wf_shader,
            entry_point: Some("wave_front_update"),
            compilation_options: Default::default(),
            cache: None,
        });

        let wf_params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wave_front_params"),
            size: std::mem::size_of::<WaveFrontParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Emergency vehicles buffer: max 16 entries, initially empty.
        // Pre-allocated so the bind group always has a valid buffer.
        let emergency_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wf_emergency_vehicles"),
            size: (16 * std::mem::size_of::<GpuEmergencyVehicle>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // --- Pedestrian adaptive pipeline ---
        let ped_shader = device.create_shader_module(wgpu::include_wgsl!(
            "../shaders/pedestrian_adaptive.wgsl"
        ));

        let ped_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("ped_adaptive_bgl"),
                entries: &[
                    // binding 0: PedestrianParams uniform
                    bgl_entry(0, wgpu::BufferBindingType::Uniform, false),
                    // binding 1: pedestrians (read_write storage)
                    bgl_entry(1, wgpu::BufferBindingType::Storage { read_only: false }, false),
                    // binding 2: cell_counts (read_write storage, atomic)
                    bgl_entry(2, wgpu::BufferBindingType::Storage { read_only: false }, false),
                    // binding 3: cell_offsets (read_write storage)
                    bgl_entry(3, wgpu::BufferBindingType::Storage { read_only: false }, false),
                    // binding 4: compacted_indices (read_write storage)
                    bgl_entry(4, wgpu::BufferBindingType::Storage { read_only: false }, false),
                    // binding 5: ped_cell_map (read_write storage)
                    bgl_entry(5, wgpu::BufferBindingType::Storage { read_only: false }, false),
                    // binding 6: scatter_counters (read_write storage, atomic)
                    bgl_entry(6, wgpu::BufferBindingType::Storage { read_only: false }, false),
                    // binding 7: workgroup_sums (read_write storage)
                    bgl_entry(7, wgpu::BufferBindingType::Storage { read_only: false }, false),
                ],
            });

        let ped_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("ped_adaptive_pipeline_layout"),
                bind_group_layouts: &[&ped_bind_group_layout],
                push_constant_ranges: &[],
            });

        let create_ped_pipeline = |entry: &str, label: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(label),
                layout: Some(&ped_pipeline_layout),
                module: &ped_shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };

        let ped_count_pipeline = create_ped_pipeline("count_per_cell", "ped_count_pipeline");
        let ped_prefix_local_pipeline =
            create_ped_pipeline("prefix_sum_local", "ped_prefix_local_pipeline");
        let ped_prefix_wg_sums_pipeline =
            create_ped_pipeline("prefix_sum_workgroup_sums", "ped_prefix_wg_sums_pipeline");
        let ped_prefix_propagate_pipeline =
            create_ped_pipeline("prefix_sum_propagate", "ped_prefix_propagate_pipeline");
        let ped_scatter_pipeline = create_ped_pipeline("scatter", "ped_scatter_pipeline");
        let ped_social_force_pipeline =
            create_ped_pipeline("social_force_adaptive", "ped_social_force_pipeline");

        let ped_params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ped_adaptive_params"),
            size: std::mem::size_of::<PedestrianAdaptiveParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            bind_group_layout,
            params_buffer,
            wf_pipeline,
            wf_bind_group_layout,
            wf_params_buffer,
            agent_buffer: None,
            lane_offsets_buffer: None,
            lane_counts_buffer: None,
            lane_agents_buffer: None,
            staging_buffer: None,
            emergency_buffer,
            wave_front_agent_count: 0,
            wave_front_lane_count: 0,
            step_counter: 0,
            emergency_count: 0,
            sign_count: 0,
            sim_time: 0.0,

            ped_count_pipeline,
            ped_prefix_local_pipeline,
            ped_prefix_wg_sums_pipeline,
            ped_prefix_propagate_pipeline,
            ped_scatter_pipeline,
            ped_social_force_pipeline,
            ped_bind_group_layout,
            ped_params_buffer,
            ped_buffer: None,
            ped_cell_counts_buffer: None,
            ped_cell_offsets_buffer: None,
            ped_compacted_indices_buffer: None,
            ped_cell_map_buffer: None,
            ped_scatter_counters_buffer: None,
            ped_workgroup_sums_buffer: None,
            ped_staging_buffer: None,
            ped_adaptive_count: 0,
        }
    }

    /// Upload agent states and lane sorting data to GPU for wave-front dispatch.
    ///
    /// `agents` is the full agent state array (indexed by agent slot).
    /// `lane_offsets`, `lane_counts`, `lane_agents` describe the per-lane
    /// front-to-back ordering produced by `sort_agents_by_lane`.
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

        // Recreate buffers if capacity is insufficient.
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

    /// Upload active emergency vehicle positions for yield cone detection.
    ///
    /// Maximum 16 emergency vehicles. If more are provided, only the first 16 are used.
    /// Set to empty slice when no emergency vehicles are active (shader early-exits).
    pub fn upload_emergency_vehicles(
        &mut self,
        queue: &wgpu::Queue,
        vehicles: &[GpuEmergencyVehicle],
    ) {
        let count = vehicles.len().min(16);
        self.emergency_count = count as u32;
        if count > 0 {
            let bytes = bytemuck::cast_slice(&vehicles[..count]);
            queue.write_buffer(&self.emergency_buffer, 0, bytes);
        }
    }

    /// Encode a wave-front compute dispatch. One workgroup per lane.
    /// After submission, call `readback_wave_front_agents` to get updated state.
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
            ],
        });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("wave_front_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.wf_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            // Use 2D dispatch for > 65535 lanes (wgpu limit per dimension).
            const MAX_WG: u32 = 65535;
            let x = self.wave_front_lane_count.min(MAX_WG);
            let y = self.wave_front_lane_count.div_ceil(MAX_WG);
            pass.dispatch_workgroups(x, y, 1);
        }

        self.step_counter += 1;
    }

    /// Read back updated agent states from GPU after wave-front dispatch.
    /// Blocks until GPU completes. Only use in simulation loop, not render loop.
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

    /// Upload pedestrian data for adaptive social force dispatch.
    ///
    /// `pedestrians` contains position, velocity, destination, and radius for each pedestrian.
    /// `grid_w` and `grid_h` define the spatial hash grid dimensions.
    /// `cell_size` is the spatial hash cell size in metres (2.0, 5.0, or 10.0).
    pub fn upload_pedestrian_data(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pedestrians: &[GpuPedestrian],
        grid_w: u32,
        grid_h: u32,
        cell_size: f32,
    ) {
        let ped_count = pedestrians.len() as u32;
        let cell_count = grid_w * grid_h;
        let prefix_wg_count = cell_count.div_ceil(WORKGROUP_SIZE);

        let ped_bytes = (pedestrians.len() * std::mem::size_of::<GpuPedestrian>()) as u64;
        let cell_u32_bytes = (cell_count as u64) * 4;
        let ped_u32_bytes = (ped_count as u64) * 4;
        let wg_sums_bytes = (prefix_wg_count as u64) * 4;

        let storage_rw = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST;
        let storage_rw_src =
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC;

        let needs_recreate = self.ped_buffer.as_ref().is_none_or(|b| b.size() < ped_bytes.max(32));

        if needs_recreate {
            self.ped_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ped_adaptive_peds"),
                size: ped_bytes.max(32),
                usage: storage_rw_src,
                mapped_at_creation: false,
            }));
            self.ped_cell_counts_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ped_cell_counts"),
                size: cell_u32_bytes.max(4),
                usage: storage_rw,
                mapped_at_creation: false,
            }));
            self.ped_cell_offsets_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ped_cell_offsets"),
                size: cell_u32_bytes.max(4),
                usage: storage_rw,
                mapped_at_creation: false,
            }));
            self.ped_compacted_indices_buffer =
                Some(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("ped_compacted_indices"),
                    size: ped_u32_bytes.max(4),
                    usage: storage_rw,
                    mapped_at_creation: false,
                }));
            self.ped_cell_map_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ped_cell_map"),
                size: ped_u32_bytes.max(4),
                usage: storage_rw,
                mapped_at_creation: false,
            }));
            self.ped_scatter_counters_buffer =
                Some(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("ped_scatter_counters"),
                    size: cell_u32_bytes.max(4),
                    usage: storage_rw,
                    mapped_at_creation: false,
                }));
            self.ped_workgroup_sums_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ped_workgroup_sums"),
                size: wg_sums_bytes.max(4),
                usage: storage_rw,
                mapped_at_creation: false,
            }));
            self.ped_staging_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ped_staging"),
                size: ped_bytes.max(32),
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }

        if !pedestrians.is_empty() {
            queue.write_buffer(
                self.ped_buffer.as_ref().unwrap(),
                0,
                bytemuck::cast_slice(pedestrians),
            );
        }

        // Zero out cell_counts and scatter_counters (they use atomics)
        let zeros = vec![0u8; cell_u32_bytes as usize];
        queue.write_buffer(self.ped_cell_counts_buffer.as_ref().unwrap(), 0, &zeros);
        queue.write_buffer(self.ped_scatter_counters_buffer.as_ref().unwrap(), 0, &zeros);

        self.ped_adaptive_count = ped_count;
    }

    /// Encode a 6-dispatch pedestrian adaptive pipeline.
    ///
    /// Dispatches: count_per_cell, prefix_sum_local, prefix_sum_workgroup_sums,
    /// prefix_sum_propagate, scatter, social_force_adaptive.
    ///
    /// After submission, call `readback_pedestrians` to get updated state.
    pub fn dispatch_pedestrian_adaptive(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        grid_w: u32,
        grid_h: u32,
        cell_size: f32,
        dt: f32,
        social_params: &PedestrianAdaptiveParams,
    ) {
        if self.ped_adaptive_count == 0 {
            return;
        }

        let cell_count = grid_w * grid_h;
        let prefix_wg_count = cell_count.div_ceil(WORKGROUP_SIZE);

        let params = PedestrianAdaptiveParams {
            ped_count: self.ped_adaptive_count,
            cell_count,
            grid_w,
            grid_h,
            cell_size,
            dt,
            workgroup_count: prefix_wg_count,
            ..*social_params
        };
        queue.write_buffer(&self.ped_params_buffer, 0, bytemuck::bytes_of(&params));

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ped_adaptive_bg"),
            layout: &self.ped_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.ped_params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.ped_buffer.as_ref().unwrap().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.ped_cell_counts_buffer.as_ref().unwrap().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.ped_cell_offsets_buffer.as_ref().unwrap().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self
                        .ped_compacted_indices_buffer
                        .as_ref()
                        .unwrap()
                        .as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: self.ped_cell_map_buffer.as_ref().unwrap().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: self
                        .ped_scatter_counters_buffer
                        .as_ref()
                        .unwrap()
                        .as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: self
                        .ped_workgroup_sums_buffer
                        .as_ref()
                        .unwrap()
                        .as_entire_binding(),
                },
            ],
        });

        let ped_wg_count = self.ped_adaptive_count.div_ceil(WORKGROUP_SIZE);

        // Pass 1: Count pedestrians per cell
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("ped_count_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.ped_count_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(ped_wg_count, 1, 1);
        }

        // Pass 2a: Per-workgroup prefix sum
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("ped_prefix_local_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.ped_prefix_local_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(prefix_wg_count, 1, 1);
        }

        // Pass 2b: Scan workgroup sums (single workgroup)
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("ped_prefix_wg_sums_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.ped_prefix_wg_sums_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }

        // Pass 2c: Propagate scanned totals back
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("ped_prefix_propagate_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.ped_prefix_propagate_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(prefix_wg_count, 1, 1);
        }

        // Pass 3: Scatter
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("ped_scatter_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.ped_scatter_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(ped_wg_count, 1, 1);
        }

        // Pass 4: Social force (one workgroup per cell)
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("ped_social_force_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.ped_social_force_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(cell_count, 1, 1);
        }
    }

    /// Read back updated pedestrian states from GPU after adaptive dispatch.
    /// Blocks until GPU completes. Only use in simulation loop, not render loop.
    pub fn readback_pedestrians(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Vec<GpuPedestrian> {
        let count = self.ped_adaptive_count as usize;
        if count == 0 {
            return Vec::new();
        }

        let byte_size = (count * std::mem::size_of::<GpuPedestrian>()) as u64;
        let staging = self.ped_staging_buffer.as_ref().unwrap();

        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(self.ped_buffer.as_ref().unwrap(), 0, staging, 0, byte_size);
        queue.submit(std::iter::once(encoder.finish()));

        let slice = staging.slice(..byte_size);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = device.poll(wgpu::PollType::wait_indefinitely());

        let data = slice.get_mapped_range();
        let peds: Vec<GpuPedestrian> = bytemuck::cast_slice(&data).to_vec();
        drop(data);
        staging.unmap();

        peds
    }

    /// Classify pedestrian density and return appropriate cell size.
    ///
    /// - Dense (>100 peds/hectare): 2.0m cells
    /// - Medium (10-100 peds/hectare): 5.0m cells
    /// - Sparse (<10 peds/hectare): 10.0m cells
    pub fn classify_density(ped_count: u32, area_sq_m: f32) -> f32 {
        if area_sq_m < 1.0 {
            return 2.0;
        }
        let density_per_hectare = (ped_count as f32) / (area_sq_m / 10_000.0);
        if density_per_hectare > 100.0 {
            2.0
        } else if density_per_hectare > 10.0 {
            5.0
        } else {
            10.0
        }
    }

    /// Encode a legacy compute dispatch into the given encoder.
    /// Reads from `pool.pos_front`/`kin_front`, writes to `pool.pos_back`/`kin_back`.
    /// Call `pool.swap()` after submitting the encoder.
    pub fn dispatch(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        pool: &BufferPool,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        dt: f32,
    ) {
        let params = DispatchParams {
            agent_count: pool.agent_count,
            dt,
            _pad0: 0,
            _pad1: 0,
        };
        queue.write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&params));

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("compute_bg"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: pool.pos_front.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: pool.kin_front.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: pool.pos_back.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: pool.kin_back.as_entire_binding(),
                },
            ],
        });

        let workgroups = pool.agent_count.div_ceil(WORKGROUP_SIZE);
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("agent_update_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(workgroups, 1, 1);
        }
    }

    /// Copy output buffer to a staging buffer and read back positions.
    /// Only use in tests and benchmarks -- not in the render loop.
    pub fn readback_positions(
        pool: &BufferPool,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Vec<[f32; 2]> {
        let agent_count = pool.agent_count as usize;
        let byte_size = (agent_count * std::mem::size_of::<[f32; 2]>()) as u64;

        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pos_staging"),
            size: byte_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(&pool.pos_front, 0, &staging, 0, byte_size);
        queue.submit(std::iter::once(encoder.finish()));

        let slice = staging.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = device.poll(wgpu::PollType::wait_indefinitely());

        let data = slice.get_mapped_range();
        let positions: Vec<[f32; 2]> = bytemuck::cast_slice(&data).to_vec();
        drop(data);
        staging.unmap();

        positions
    }
}

/// Helper to create a bind group layout entry.
fn bgl_entry(
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
///
/// Groups agents by `lane_idx`, sorts each group by position descending
/// (leader = highest position = first in array), and returns the
/// lane indexing arrays for the GPU.
///
/// Returns `(lane_offsets, lane_counts, lane_agent_indices)` where:
/// - `lane_offsets[i]` is the start index in `lane_agent_indices` for lane `i`
/// - `lane_counts[i]` is the number of agents in lane `i`
/// - `lane_agent_indices` contains agent indices sorted by position descending per lane
pub fn sort_agents_by_lane(
    agents: &[GpuAgentState],
) -> (Vec<u32>, Vec<u32>, Vec<u32>) {
    if agents.is_empty() {
        return (vec![0], vec![0], Vec::new());
    }

    // Group agents by (edge_id, lane_idx) to form unique lanes.
    let mut lane_map: HashMap<(u32, u32), Vec<(u32, i32)>> = HashMap::new();
    for (idx, agent) in agents.iter().enumerate() {
        lane_map
            .entry((agent.edge_id, agent.lane_idx))
            .or_default()
            .push((idx as u32, agent.position));
    }

    // Sort lane keys for deterministic ordering.
    let mut lane_keys: Vec<(u32, u32)> = lane_map.keys().copied().collect();
    lane_keys.sort();

    let num_lanes = lane_keys.len();
    let mut lane_offsets = Vec::with_capacity(num_lanes);
    let mut lane_counts = Vec::with_capacity(num_lanes);
    let mut lane_agent_indices = Vec::with_capacity(agents.len());

    for key in &lane_keys {
        let group = lane_map.get_mut(key).unwrap();
        // Sort by position descending (leader first = highest position).
        group.sort_by(|a, b| b.1.cmp(&a.1));

        lane_offsets.push(lane_agent_indices.len() as u32);
        lane_counts.push(group.len() as u32);
        for &(agent_idx, _) in group.iter() {
            lane_agent_indices.push(agent_idx);
        }
    }

    (lane_offsets, lane_counts, lane_agent_indices)
}

#[cfg(test)]
mod tests {
    use super::*;
    use velos_core::components::GpuAgentState;

    #[test]
    fn sort_agents_empty() {
        let (offsets, counts, indices) = sort_agents_by_lane(&[]);
        assert_eq!(offsets, vec![0]);
        assert_eq!(counts, vec![0]);
        assert!(indices.is_empty());
    }

    #[test]
    fn sort_agents_single_lane() {
        let agents = vec![
            GpuAgentState {
                edge_id: 0, lane_idx: 0, position: 100, lateral: 0,
                speed: 50, acceleration: 0, cf_model: 0, rng_state: 0,
                vehicle_type: 0, flags: 0,
            },
            GpuAgentState {
                edge_id: 0, lane_idx: 0, position: 500, lateral: 0,
                speed: 50, acceleration: 0, cf_model: 0, rng_state: 0,
                vehicle_type: 0, flags: 0,
            },
            GpuAgentState {
                edge_id: 0, lane_idx: 0, position: 300, lateral: 0,
                speed: 50, acceleration: 0, cf_model: 0, rng_state: 0,
                vehicle_type: 0, flags: 0,
            },
        ];
        let (offsets, counts, indices) = sort_agents_by_lane(&agents);
        assert_eq!(offsets.len(), 1);
        assert_eq!(counts, vec![3]);
        // Sorted by position descending: agent 1 (500), agent 2 (300), agent 0 (100)
        assert_eq!(indices, vec![1, 2, 0]);
    }

    #[test]
    fn sort_agents_multiple_lanes() {
        let agents = vec![
            GpuAgentState {
                edge_id: 0, lane_idx: 0, position: 100, lateral: 0,
                speed: 50, acceleration: 0, cf_model: 0, rng_state: 0,
                vehicle_type: 0, flags: 0,
            },
            GpuAgentState {
                edge_id: 0, lane_idx: 1, position: 200, lateral: 0,
                speed: 50, acceleration: 0, cf_model: 1, rng_state: 42,
                vehicle_type: 0, flags: 0,
            },
            GpuAgentState {
                edge_id: 0, lane_idx: 0, position: 300, lateral: 0,
                speed: 50, acceleration: 0, cf_model: 0, rng_state: 0,
                vehicle_type: 0, flags: 0,
            },
        ];
        let (offsets, counts, indices) = sort_agents_by_lane(&agents);
        assert_eq!(counts.len(), 2);
        // Lane (0,0): agents 0 (pos=100) and 2 (pos=300) -> sorted desc: [2, 0]
        // Lane (0,1): agent 1 (pos=200) -> [1]
        assert_eq!(counts[0], 2);
        assert_eq!(counts[1], 1);
        assert_eq!(indices[offsets[0] as usize], 2); // leader of lane 0
        assert_eq!(indices[offsets[0] as usize + 1], 0);
        assert_eq!(indices[offsets[1] as usize], 1); // only agent in lane 1
    }
}
