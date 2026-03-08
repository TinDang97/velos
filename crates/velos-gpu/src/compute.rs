//! ComputeDispatcher: WGSL shader pipelines for agent position update.
//!
//! Three pipeline families:
//! 1. Legacy `agent_update.wgsl`: simple parallel Euler integration (backward compat).
//! 2. Wave-front `wave_front.wgsl`: per-lane sequential dispatch with IDM+Krauss branching.
//! 3. Pedestrian adaptive `pedestrian_adaptive.wgsl`: density-adaptive spatial hash with
//!    prefix-sum compaction and social force model (6-dispatch pipeline, in `ped_adaptive` module).
//!
//! The wave-front pipeline is the production path for vehicles. The pedestrian adaptive
//! pipeline handles pedestrian social force with adaptive workgroup dispatch.

use velos_vehicle::config::VehicleConfig;

use crate::buffers::BufferPool;

/// Per-vehicle-type parameters for GPU shader uniform buffer.
///
/// Layout: 7 vehicle types x 12 f32 parameters = 336 bytes.
/// Each row: `[v0, s0, t_headway, a, b, krauss_accel, krauss_decel, krauss_sigma,
///             creep_max_speed, creep_distance_scale, creep_min_distance, gap_acceptance_ttc]`
///
/// Indexed by `vehicle_type` (u32): 0=Motorbike, 1=Car, 2=Bus, 3=Bicycle,
/// 4=Truck, 5=Emergency, 6=Pedestrian.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuVehicleParams {
    pub params: [[f32; 12]; 7],
}

impl GpuVehicleParams {
    /// Convert a [`VehicleConfig`] to GPU-ready parameter buffer.
    ///
    /// Maps each vehicle type's IDM + Krauss + creep/gap parameters to the 12-float row.
    /// Pedestrian uses `desired_speed` for v0, `personal_space` for s0,
    /// and zeroes for car-following params (pedestrians use social force).
    pub fn from_config(config: &VehicleConfig) -> Self {
        let vehicle_types = [
            &config.motorbike,
            &config.car,
            &config.bus,
            &config.bicycle,
            &config.truck,
            &config.emergency,
        ];

        let mut params = [[0.0_f32; 12]; 7];

        for (i, vt) in vehicle_types.iter().enumerate() {
            params[i] = [
                vt.v0 as f32,
                vt.s0 as f32,
                vt.t_headway as f32,
                vt.a as f32,
                vt.b as f32,
                vt.krauss_accel as f32,
                vt.krauss_decel as f32,
                vt.krauss_sigma as f32,
                vt.creep_max_speed as f32,
                vt.creep_distance_scale as f32,
                0.5_f32, // creep_min_distance (constant across types)
                vt.gap_acceptance_ttc as f32,
            ];
        }

        // Index 6: Pedestrian (social force model, not car-following)
        let ped = &config.pedestrian;
        params[6] = [
            ped.desired_speed as f32, // v0 = desired walking speed
            ped.personal_space as f32, // s0 = personal space radius
            1.0,                       // t_headway (not used, sensible default)
            1.0,                       // a (not used)
            2.0,                       // b (not used)
            1.0,                       // krauss_accel (not used)
            3.0,                       // krauss_decel (not used)
            0.0,                       // krauss_sigma (not used)
            0.0,                       // creep_max_speed (pedestrians don't creep)
            5.0,                       // creep_distance_scale (default)
            0.5,                       // creep_min_distance (constant)
            ped.gap_acceptance_ttc as f32,
        ];

        Self { params }
    }
}

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
pub(crate) struct WaveFrontParams {
    pub(crate) agent_count: u32,
    pub(crate) dt: f32,
    pub(crate) step_counter: u32,
    pub(crate) emergency_count: u32,
    pub(crate) sign_count: u32,
    pub(crate) sim_time: f32,
    pub(crate) _pad0: u32,
    pub(crate) _pad1: u32,
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

const WORKGROUP_SIZE: u32 = 256;

/// Owns the compute pipelines and bind group layouts for agent updates.
pub struct ComputeDispatcher {
    // Legacy pipeline (agent_update.wgsl)
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    params_buffer: wgpu::Buffer,

    // Wave-front pipeline (wave_front.wgsl)
    pub(crate) wf_pipeline: wgpu::ComputePipeline,
    pub(crate) wf_bind_group_layout: wgpu::BindGroupLayout,
    pub(crate) wf_params_buffer: wgpu::Buffer,

    // Vehicle params uniform buffer (binding 7)
    pub(crate) vehicle_params_buffer: wgpu::Buffer,

    // Wave-front GPU buffers for lane data + agent state
    pub(crate) agent_buffer: Option<wgpu::Buffer>,
    pub(crate) lane_offsets_buffer: Option<wgpu::Buffer>,
    pub(crate) lane_counts_buffer: Option<wgpu::Buffer>,
    pub(crate) lane_agents_buffer: Option<wgpu::Buffer>,
    pub(crate) staging_buffer: Option<wgpu::Buffer>,
    pub(crate) emergency_buffer: wgpu::Buffer,
    pub(crate) sign_buffer: wgpu::Buffer,

    // Perception results buffer (binding 8) -- zeroed placeholder until PerceptionPipeline wired.
    // 32 bytes per agent (PerceptionResult), pre-allocated for 300K agents.
    pub(crate) perception_result_buffer: wgpu::Buffer,

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
                    bgl_entry(0, wgpu::BufferBindingType::Uniform, false),
                    bgl_entry(1, wgpu::BufferBindingType::Storage { read_only: false }, false),
                    bgl_entry(2, wgpu::BufferBindingType::Storage { read_only: true }, false),
                    bgl_entry(3, wgpu::BufferBindingType::Storage { read_only: true }, false),
                    bgl_entry(4, wgpu::BufferBindingType::Storage { read_only: true }, false),
                    bgl_entry(5, wgpu::BufferBindingType::Storage { read_only: true }, false),
                    bgl_entry(6, wgpu::BufferBindingType::Storage { read_only: true }, false),
                    bgl_entry(7, wgpu::BufferBindingType::Uniform, false),
                    bgl_entry(8, wgpu::BufferBindingType::Storage { read_only: true }, false),
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

        let emergency_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wf_emergency_vehicles"),
            size: (16 * std::mem::size_of::<GpuEmergencyVehicle>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Sign buffer: 16 bytes per GpuSign (sign_type u32 + value f32 + edge_id u32 + offset_m f32).
        // Pre-allocate for 256 signs; zero-length storage buffers are invalid in wgpu.
        let sign_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wf_signs"),
            size: (256 * 16) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Vehicle params uniform buffer: 7 types * 12 f32 = 336 bytes.
        // Must be populated via upload_vehicle_params() before first dispatch.
        let vehicle_params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wf_vehicle_params"),
            size: std::mem::size_of::<GpuVehicleParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Perception results placeholder buffer (binding 8): zeroed, 32 bytes per agent.
        // Pre-allocate for 300K agents (9.6 MB). Replaced at runtime via
        // set_perception_result_buffer() once PerceptionPipeline is wired.
        let perception_result_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wf_perception_results"),
            size: 300_000 * 32,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
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
            vehicle_params_buffer,
            emergency_buffer,
            sign_buffer,
            perception_result_buffer,
            wave_front_agent_count: 0,
            wave_front_lane_count: 0,
            step_counter: 0,
            emergency_count: 0,
            sign_count: 0,
            sim_time: 0.0,
        }
    }

    /// Returns a reference to the agent buffer for use by other pipelines (e.g., perception).
    ///
    /// Returns `None` if no agent data has been uploaded yet.
    pub fn agent_buffer(&self) -> Option<&wgpu::Buffer> {
        self.agent_buffer.as_ref()
    }

    /// Returns a reference to the lane_agents buffer for use by other pipelines.
    ///
    /// Returns `None` if no lane data has been uploaded yet.
    pub fn lane_agents_buffer(&self) -> Option<&wgpu::Buffer> {
        self.lane_agents_buffer.as_ref()
    }

    /// Upload active emergency vehicle positions for yield cone detection.
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

    /// Replace the placeholder perception result buffer with the PerceptionPipeline's
    /// result buffer. Called once at sim startup after PerceptionPipeline is created.
    pub fn set_perception_result_buffer(&mut self, buffer: wgpu::Buffer) {
        self.perception_result_buffer = buffer;
    }

    /// Returns a reference to the perception result buffer (binding 8).
    pub fn perception_result_buffer(&self) -> &wgpu::Buffer {
        &self.perception_result_buffer
    }

    /// Returns a reference to the sign buffer for use by other pipelines (e.g., perception).
    pub fn sign_buffer(&self) -> &wgpu::Buffer {
        &self.sign_buffer
    }

    /// Upload traffic sign data to the GPU sign buffer (binding 6).
    ///
    /// Writes `signs` to the pre-allocated sign buffer and updates `sign_count`.
    /// If `signs` is empty, sets `sign_count` to 0 (valid -- shader early-exits).
    /// Truncates to 256 signs if more are provided (buffer capacity).
    pub fn upload_signs(&mut self, queue: &wgpu::Queue, signs: &[velos_signal::signs::GpuSign]) {
        let count = signs.len().min(256);
        self.sign_count = count as u32;
        if count > 0 {
            let bytes = bytemuck::cast_slice(&signs[..count]);
            queue.write_buffer(&self.sign_buffer, 0, bytes);
        }
    }

    /// Upload per-vehicle-type parameters to the GPU uniform buffer (binding 7).
    ///
    /// Call this once at startup (with `GpuVehicleParams::from_config`) and again
    /// whenever vehicle configuration changes at runtime.
    pub fn upload_vehicle_params(&self, queue: &wgpu::Queue, params: &GpuVehicleParams) {
        queue.write_buffer(&self.vehicle_params_buffer, 0, bytemuck::bytes_of(params));
    }

    /// Encode a legacy compute dispatch into the given encoder.
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

// Re-export wave-front functions that are used across the crate.
pub use crate::compute_wave_front::{compute_agent_flags, sort_agents_by_lane};
pub(crate) use crate::compute_wave_front::bgl_entry;

#[cfg(test)]
#[path = "compute_tests.rs"]
mod tests;
