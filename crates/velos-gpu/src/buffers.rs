//! Double-buffered SoA GPU buffers for agent position and kinematics.
//!
//! Front buffers: STORAGE | COPY_SRC (read by compute shader, copied out for readback).
//! Back buffers: STORAGE | COPY_DST (written by CPU upload and by compute output).
//!
//! Frame loop:
//!   1. upload_from_ecs -> writes f32 SoA data to back buffers
//!   2. compute pass reads front, writes back (back is output)
//!   3. swap() -- back becomes front for next frame's compute input
//!
//! Note: compute shader reads pos_front/kin_front (binding 1/2) and writes
//! to pos_back/kin_back (binding 3/4). After swap, the written data is in front.

use bytemuck::{Pod, Zeroable};
use velos_core::components::{Kinematics, Position};
use hecs::Entity;

/// GPU-side position: f32 vec2 (x, y). Matches WGSL `vec2<f32>`.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct GpuPosition {
    pub x: f32,
    pub y: f32,
}

/// GPU-side kinematics: f32 vec4 (vx, vy, speed, heading). Matches WGSL `vec4<f32>`.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct GpuKinematics {
    pub vx: f32,
    pub vy: f32,
    pub speed: f32,
    pub heading: f32,
}

/// Double-buffered SoA GPU buffers for agent state.
pub struct BufferPool {
    /// Position front buffer: STORAGE | COPY_SRC.
    pub pos_front: wgpu::Buffer,
    /// Position back buffer: STORAGE | COPY_DST.
    pub pos_back: wgpu::Buffer,
    /// Kinematics front buffer: STORAGE | COPY_SRC.
    pub kin_front: wgpu::Buffer,
    /// Kinematics back buffer: STORAGE | COPY_DST.
    pub kin_back: wgpu::Buffer,
    /// Index map: GPU slot index -> ECS Entity. Rebuilt on upload.
    pub index_map: Vec<hecs::Entity>,
    /// Current number of agents in the buffers.
    pub agent_count: u32,
    /// Allocated capacity (slots, not bytes).
    pub capacity: u32,
}

impl BufferPool {
    /// Create a new buffer pool with `capacity` slots.
    pub fn new(device: &wgpu::Device, capacity: u32) -> Self {
        let pos_bytes = (capacity as usize) * std::mem::size_of::<GpuPosition>();
        let kin_bytes = (capacity as usize) * std::mem::size_of::<GpuKinematics>();

        let front_usage =
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST;
        let back_usage =
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC;

        let pos_front = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pos_front"),
            size: pos_bytes as u64,
            usage: front_usage,
            mapped_at_creation: false,
        });
        let pos_back = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pos_back"),
            size: pos_bytes as u64,
            usage: back_usage,
            mapped_at_creation: false,
        });
        let kin_front = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("kin_front"),
            size: kin_bytes as u64,
            usage: front_usage,
            mapped_at_creation: false,
        });
        let kin_back = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("kin_back"),
            size: kin_bytes as u64,
            usage: back_usage,
            mapped_at_creation: false,
        });

        Self {
            pos_front,
            pos_back,
            kin_front,
            kin_back,
            index_map: Vec::with_capacity(capacity as usize),
            agent_count: 0,
            capacity,
        }
    }

    /// Upload ECS agent state to the back GPU buffers.
    /// Queries the hecs World for all (Position, Kinematics) entities.
    /// Rebuilds the index_map on each call.
    pub fn upload_from_ecs(&mut self, world: &hecs::World, queue: &wgpu::Queue) {
        let mut positions: Vec<GpuPosition> = Vec::with_capacity(self.capacity as usize);
        let mut kinematics: Vec<GpuKinematics> = Vec::with_capacity(self.capacity as usize);
        self.index_map.clear();

        for (entity, pos, kin) in world.query::<(Entity, &Position, &Kinematics)>().iter() {
            if positions.len() >= self.capacity as usize {
                log::warn!("BufferPool capacity {} exceeded, truncating", self.capacity);
                break;
            }
            self.index_map.push(entity);
            positions.push(GpuPosition {
                x: pos.x as f32,
                y: pos.y as f32,
            });
            kinematics.push(GpuKinematics {
                vx: kin.vx as f32,
                vy: kin.vy as f32,
                speed: kin.speed as f32,
                heading: kin.heading as f32,
            });
        }

        self.agent_count = positions.len() as u32;

        if !positions.is_empty() {
            queue.write_buffer(&self.pos_back, 0, bytemuck::cast_slice(&positions));
            queue.write_buffer(&self.kin_back, 0, bytemuck::cast_slice(&kinematics));
        }
    }

    /// Swap front and back buffers.
    /// Call after submitting the compute command encoder.
    pub fn swap(&mut self) {
        std::mem::swap(&mut self.pos_front, &mut self.pos_back);
        std::mem::swap(&mut self.kin_front, &mut self.kin_back);
    }
}
