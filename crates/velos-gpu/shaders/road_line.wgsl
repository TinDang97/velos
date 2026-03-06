// Simple line shader for road network edges.
// Uses the same camera uniform as agent_render.wgsl.

struct CameraUniform {
    view_proj: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) color: vec4<f32>,
}

@vertex
fn vs_main(vert: VertexInput) -> VertexOutput {
    let world = vec4<f32>(vert.position, 0.0, 1.0);
    var out: VertexOutput;
    out.clip_pos = camera.view_proj * world;
    out.color = vert.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
