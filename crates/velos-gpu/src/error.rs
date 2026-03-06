//! Error types for velos-gpu.

#[derive(Debug, thiserror::Error)]
pub enum GpuError {
    #[error("No GPU adapter available on this system")]
    NoAdapter,

    #[error("GPU device request failed: {0}")]
    DeviceRequest(#[from] wgpu::RequestDeviceError),

    #[error("Surface error: {0}")]
    Surface(#[from] wgpu::SurfaceError),

    #[error("Buffer capacity {requested} exceeds maximum {max}")]
    BufferCapacityExceeded { requested: u32, max: u32 },
}
