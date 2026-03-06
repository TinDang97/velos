//! Instanced 2D agent renderer.
//!
//! One render pipeline serves all agent types. Each shape type (triangle, dot)
//! gets its own draw call (REN-04). Phase 1 uses only triangles for all agents.
//!
//! Frame usage:
//!   1. update_camera(queue, camera) -- upload projection matrix
//!   2. update_instances_from_cpu(queue, positions, headings) -- build AgentInstance array
//!   3. render_frame(encoder, view, instance_count) -- record draw call

use crate::camera::Camera2D;
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

/// Per-instance data uploaded to GPU for each agent.
/// Matches WGSL InstanceInput: location 1-4.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct AgentInstance {
    /// World-space position (metres).
    pub position: [f32; 2],
    /// Heading in radians (CCW from east).
    pub heading: f32,
    /// Padding to align color to 16 bytes.
    pub _pad: f32,
    /// RGBA color [0.0, 1.0].
    pub color: [f32; 4],
}

impl AgentInstance {
    /// Vertex buffer layout for the instance buffer (VertexStepMode::Instance).
    pub fn vertex_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<AgentInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    // location(1): world_pos
                    offset: 0,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    // location(2): heading
                    offset: 8,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32,
                },
                wgpu::VertexAttribute {
                    // location(3): _pad (consumed by WGSL)
                    offset: 12,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32,
                },
                wgpu::VertexAttribute {
                    // location(4): color
                    offset: 16,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

/// Camera uniform buffer layout. Must match WGSL CameraUniform.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct CameraUniform {
    view_proj: [f32; 16],
}

/// Vertex layout for shape mesh vertices (local space).
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct ShapeVertex {
    local_pos: [f32; 2],
}

/// Triangle vertices for a motorbike/agent shape in local space.
/// Points forward (east, +x direction). Scale: ~2 metres long, 1 metre wide.
const TRIANGLE_VERTICES: &[ShapeVertex] = &[
    ShapeVertex { local_pos: [2.0, 0.0] },   // nose (forward)
    ShapeVertex { local_pos: [-1.0, 0.8] },  // left rear
    ShapeVertex { local_pos: [-1.0, -0.8] }, // right rear
];

/// Instanced 2D renderer.
pub struct Renderer {
    render_pipeline: wgpu::RenderPipeline,
    camera_bind_group: wgpu::BindGroup,
    camera_uniform_buffer: wgpu::Buffer,
    shape_vertex_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    instance_capacity: u32,
    pub surface_format: wgpu::TextureFormat,
}

impl Renderer {
    /// Create the render pipeline for the given surface texture format.
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        let shader =
            device.create_shader_module(wgpu::include_wgsl!("../shaders/agent_render.wgsl"));

        let camera_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera_uniform"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("camera_bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera_bg"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("render_pipeline_layout"),
                bind_group_layouts: &[&camera_bind_group_layout],
                immediate_size: 0,
            });

        // Shape vertex buffer layout: location(0) local_pos vec2<f32>
        let shape_vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ShapeVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x2,
            }],
        };

        let render_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("agent_render_pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[shape_vertex_layout, AgentInstance::vertex_buffer_layout()],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: surface_format,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });

        let shape_vertex_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("shape_vertices"),
                contents: bytemuck::cast_slice(TRIANGLE_VERTICES),
                usage: wgpu::BufferUsages::VERTEX,
            });

        let instance_capacity = 2048_u32;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instance_buffer"),
            size: (instance_capacity as usize * std::mem::size_of::<AgentInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            render_pipeline,
            camera_bind_group,
            camera_uniform_buffer,
            shape_vertex_buffer,
            instance_buffer,
            instance_capacity,
            surface_format,
        }
    }

    /// Upload the camera view-projection matrix to the uniform buffer.
    pub fn update_camera(&self, queue: &wgpu::Queue, camera: &Camera2D) {
        let m = camera.view_proj_matrix();
        let uniform = CameraUniform {
            view_proj: m.to_cols_array(),
        };
        queue.write_buffer(
            &self.camera_uniform_buffer,
            0,
            bytemuck::bytes_of(&uniform),
        );
    }

    /// Rebuild the instance buffer from CPU-side position and heading arrays.
    /// Reads positions and headings directly -- no GPU readback needed.
    /// Phase 1: practical for 1K agents.
    pub fn update_instances_from_cpu(
        &self,
        queue: &wgpu::Queue,
        positions: &[[f32; 2]],
        headings: &[f32],
    ) {
        let count = positions.len().min(self.instance_capacity as usize);
        let instances: Vec<AgentInstance> = (0..count)
            .map(|i| AgentInstance {
                position: positions[i],
                heading: headings[i],
                _pad: 0.0,
                color: [0.2, 0.8, 0.4, 1.0], // green triangles for Phase 1
            })
            .collect();

        if !instances.is_empty() {
            queue.write_buffer(
                &self.instance_buffer,
                0,
                bytemuck::cast_slice(&instances),
            );
        }
    }

    /// Record the render pass into the encoder.
    /// Draws triangles shape type as one instanced draw call (REN-04).
    pub fn render_frame(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        instance_count: u32,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("agent_render_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.05,
                        g: 0.05,
                        b: 0.1,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });

        pass.set_pipeline(&self.render_pipeline);
        pass.set_bind_group(0, &self.camera_bind_group, &[]);
        pass.set_vertex_buffer(0, self.shape_vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        // REN-04: one draw call for triangles (motorbikes + cars in Phase 1)
        pass.draw(0..TRIANGLE_VERTICES.len() as u32, 0..instance_count);
    }
}
