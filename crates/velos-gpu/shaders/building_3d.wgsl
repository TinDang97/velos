// building_3d.wgsl -- Lit building shader with per-vertex normals and color.
//
// Renders extruded building geometry with Lambert diffuse + ambient shading.
// No instancing -- buildings use merged vertex/index buffers with unique
// geometry per building and per-vertex color baked in.
//
// Bind groups (same as mesh_3d.wgsl):
//   @group(0) @binding(0): Camera uniform (view_proj, eye_position)
//   @group(0) @binding(1): Lighting uniform (sun_direction, sun_color, ambient)

struct CameraUniform {
    view_proj: mat4x4<f32>,
    eye_position: vec3<f32>,
    _pad0: f32,
    camera_right: vec3<f32>,
    _pad1: f32,
    camera_up: vec3<f32>,
    _pad2: f32,
}

struct LightingUniform {
    sun_direction: vec3<f32>,
    _pad0: f32,
    sun_color: vec3<f32>,
    _pad1: f32,
    ambient_color: vec3<f32>,
    ambient_intensity: f32,
}

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> lighting: LightingUniform;

// Vertex attributes (from merged vertex buffer -- no instancing)
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) frag_color: vec4<f32>,
}

@vertex
fn vs_main(vertex: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(vertex.position, 1.0);
    out.world_normal = normalize(vertex.normal);
    out.frag_color = vertex.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let N = normalize(in.world_normal);
    // Sun direction points toward the sun; negate for light direction toward surface
    let L = normalize(-lighting.sun_direction);

    // Diffuse shading (Lambert)
    let n_dot_l = max(dot(N, L), 0.0);
    let diffuse = lighting.sun_color * n_dot_l;

    // Ambient
    let ambient = lighting.ambient_color * lighting.ambient_intensity;

    // Final color: (ambient + diffuse) * base color
    let lit_color = (ambient + diffuse) * in.frag_color.rgb;
    return vec4<f32>(lit_color, in.frag_color.a);
}
