// Instanced 2D agent renderer.
// Vertex buffer 0: shape vertices (local space, triangle for motorbike/car).
// Vertex buffer 1: per-instance data (world position, heading, color).
// Group 0, binding 0: camera uniform (orthographic view-projection matrix).

struct CameraUniform {
    view_proj: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) local_pos: vec2<f32>,
}

struct InstanceInput {
    @location(1) world_pos: vec2<f32>,
    @location(2) heading: f32,
    @location(3) _pad: f32,
    @location(4) color: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) color: vec4<f32>,
}

@vertex
fn vs_main(vert: VertexInput, inst: InstanceInput) -> VertexOutput {
    // Rotate local vertex around origin by heading (CCW)
    let c = cos(inst.heading);
    let s = sin(inst.heading);
    let rotated = vec2<f32>(
        vert.local_pos.x * c - vert.local_pos.y * s,
        vert.local_pos.x * s + vert.local_pos.y * c,
    );

    let world = vec4<f32>(rotated + inst.world_pos, 0.0, 1.0);

    var out: VertexOutput;
    out.clip_pos = camera.view_proj * world;
    out.color = inst.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
