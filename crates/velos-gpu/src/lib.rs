//! velos-gpu: GPU device management, compute dispatch, and rendering.
//! Exposes high-level API only -- no raw wgpu types in public API.

pub mod buffers;
pub mod camera;
pub mod compute;
pub mod device;
pub mod error;
pub mod renderer;

pub use buffers::{BufferPool, GpuKinematics, GpuPosition};
pub use camera::Camera2D;
pub use compute::ComputeDispatcher;
pub use device::GpuContext;
pub use error::GpuError;
pub use renderer::{AgentInstance, Renderer};
