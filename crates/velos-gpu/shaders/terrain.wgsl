// terrain.wgsl -- Terrain rendering with camera-only uniform.
//
// Renders terrain mesh with elevation-based geometry and flat muted green color.
// Uses the same vertex layout as ground_plane.wgsl (position vec3 + color vec4).
//
// Bind groups:
//   @group(0) @binding(0): Camera uniform (view_proj mat4x4<f32>)

struct CameraUniform {
    view_proj: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) frag_color: vec4<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.frag_color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.frag_color;
}
