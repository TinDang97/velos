//! GPU context: device, queue, and optional surface.
//! `GpuContext` is the root resource -- all other GPU objects require it.

/// Maximum agents supported in a single BufferPool allocation.
pub const MAX_AGENTS: u32 = 65_536;

/// Headless GPU context for compute-only workloads (tests, benches).
pub struct GpuContext {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub adapter: wgpu::Adapter,
}

impl GpuContext {
    /// Create a headless context using the default adapter.
    /// Returns `None` if no GPU adapter is available (CI without GPU).
    pub fn new_headless() -> Option<Self> {
        let instance = wgpu::Instance::default();
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            }))
            .ok()?;

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("velos-gpu headless"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            },
        ))
        .ok()?;

        Some(Self {
            device,
            queue,
            adapter,
        })
    }
}
