// Agent position update: simple parallel Euler integration.
// Every agent updated independently (no dependencies between agents).
// Workgroup size 256 -- dispatch ceil(agent_count / 256) workgroups.

struct Params {
    agent_count: u32,
    dt: f32,
    _pad0: u32,
    _pad1: u32,
}

@group(0) @binding(0) var<uniform> params: Params;
// pos_in: packed vec2<f32> per agent (x, y)
@group(0) @binding(1) var<storage, read> pos_in: array<vec2<f32>>;
// kin_in: packed vec4<f32> per agent (vx, vy, speed, heading)
@group(0) @binding(2) var<storage, read> kin_in: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> pos_out: array<vec2<f32>>;
@group(0) @binding(4) var<storage, read_write> kin_out: array<vec4<f32>>;

@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    if idx >= params.agent_count {
        return;
    }

    let pos = pos_in[idx];
    let kin = kin_in[idx];

    // Euler integration: new_pos = pos + vel * dt
    let vel = vec2<f32>(kin.x, kin.y);
    pos_out[idx] = pos + vel * params.dt;

    // Velocity unchanged in Phase 1 (no forces)
    kin_out[idx] = kin;
}
