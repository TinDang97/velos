// Perception gather kernel: per-agent awareness data in a single compute pass.
//
// Reads agent state, signal state, sign data, congestion grid, and edge travel
// ratios. Produces one PerceptionResult per agent for CPU readback.
//
// Runs AFTER wave_front update so positions are current.
// Uses SEPARATE bind group from wave_front (avoids binding conflicts).

// ============================================================
// Fixed-point helpers (same as wave_front.wgsl)
// ============================================================

const POS_SCALE: f32 = 65536.0;
const SPD_SCALE: f32 = 1048576.0;

fn fixpos_to_f32(v: i32) -> f32 {
    return f32(v) / POS_SCALE;
}

fn fixspd_to_f32(v: i32) -> f32 {
    return f32(v) / SPD_SCALE;
}

// ============================================================
// Struct definitions
// ============================================================

struct PerceptionParams {
    agent_count: u32,
    grid_width: u32,
    grid_height: u32,
    grid_cell_size: f32,
}

// Must match Rust PerceptionResult (32 bytes)
struct PerceptionResult {
    leader_speed: f32,
    leader_gap: f32,
    signal_state: u32,
    signal_distance: f32,
    congestion_own_route: f32,
    congestion_area: f32,
    sign_speed_limit: f32,
    flags: u32,
}

// Must match GpuAgentState in wave_front.wgsl (40 bytes)
struct AgentState {
    edge_id: u32,
    lane_idx: u32,
    position: i32,
    lateral: i32,
    speed: i32,
    acceleration: i32,
    cf_model: u32,
    rng_state: u32,
    vehicle_type: u32,
    flags: u32,
}

// Signal state per edge (simplified: one signal per edge)
struct SignalState {
    state: u32,         // 0=green, 1=amber, 2=red, 3=none
    offset_m: f32,      // position along edge (metres)
    _pad0: u32,
    _pad1: u32,
}

// Traffic sign (matches GpuSign in wave_front.wgsl, 16 bytes)
struct GpuSign {
    sign_type: u32,
    value: f32,
    edge_id: u32,
    offset_m: f32,
}

// Sign type constants
const SIGN_SPEED_LIMIT: u32 = 0u;

// Flag constants for PerceptionResult
const FLAG_ROUTE_BLOCKED: u32 = 1u;
const FLAG_EMERGENCY_NEARBY: u32 = 2u;

// Agent flag constants (from wave_front)
const FLAG_EMERGENCY_ACTIVE: u32 = 2u;

// Vehicle type constants
const VT_EMERGENCY: u32 = 5u;

// ============================================================
// Bindings (SEPARATE bind group from wave_front)
// ============================================================

@group(0) @binding(0) var<uniform> params: PerceptionParams;
@group(0) @binding(1) var<storage, read> agents: array<AgentState>;
@group(0) @binding(2) var<storage, read> lane_agents: array<u32>;
@group(0) @binding(3) var<storage, read> signals: array<SignalState>;
@group(0) @binding(4) var<storage, read> signs: array<GpuSign>;
@group(0) @binding(5) var<storage, read> congestion_grid: array<f32>;
@group(0) @binding(6) var<storage, read> edge_travel_ratios: array<f32>;
@group(0) @binding(7) var<storage, read_write> results: array<PerceptionResult>;

// ============================================================
// Perception gather kernel
// ============================================================

@compute @workgroup_size(256)
fn perception_gather(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    if idx >= params.agent_count {
        return;
    }

    let agent = agents[idx];
    let own_pos_f32 = fixpos_to_f32(agent.position);
    let own_speed_f32 = fixspd_to_f32(agent.speed);
    let own_edge = agent.edge_id;
    let own_lane = agent.lane_idx;

    // ---- Leader detection ----
    // Scan agents on same edge+lane with higher position. Find closest leader.
    var best_leader_gap: f32 = 9999.0;
    var best_leader_speed: f32 = 0.0;

    let total_agents = params.agent_count;
    for (var i = 0u; i < total_agents; i = i + 1u) {
        if i == idx {
            continue;
        }
        let other = agents[i];
        if other.edge_id != own_edge || other.lane_idx != own_lane {
            continue;
        }
        let other_pos = fixpos_to_f32(other.position);
        let gap = other_pos - own_pos_f32;
        if gap > 0.0 && gap < best_leader_gap {
            best_leader_gap = gap;
            best_leader_speed = fixspd_to_f32(other.speed);
        }
    }

    // ---- Signal state ----
    // Read from signals array indexed by edge_id (simplified: assumes one signal per edge).
    var sig_state: u32 = 3u; // 3 = none
    var sig_distance: f32 = 9999.0;
    let signal_count = arrayLength(&signals);
    if own_edge < signal_count {
        let sig = signals[own_edge];
        sig_state = sig.state;
        sig_distance = max(sig.offset_m - own_pos_f32, 0.0);
    }

    // ---- Congestion (own route) ----
    // Read edge_travel_ratios[edge_id]: current/free_flow travel time ratio.
    var cong_own: f32 = 1.0;
    let ratio_count = arrayLength(&edge_travel_ratios);
    if own_edge < ratio_count {
        cong_own = edge_travel_ratios[own_edge];
    }

    // ---- Congestion (area) ----
    // Convert agent position to grid cell and read heatmap value.
    var cong_area: f32 = 0.0;
    if params.grid_cell_size > 0.0 {
        let cell_x = u32(max(own_pos_f32 / params.grid_cell_size, 0.0));
        let lateral_f32 = f32(agent.lateral) / 256.0; // Q8.8
        let cell_y = u32(max(lateral_f32 / params.grid_cell_size, 0.0));
        let clamped_x = min(cell_x, params.grid_width - 1u);
        let clamped_y = min(cell_y, params.grid_height - 1u);
        let cell_idx = clamped_y * params.grid_width + clamped_x;
        let grid_len = arrayLength(&congestion_grid);
        if cell_idx < grid_len {
            cong_area = congestion_grid[cell_idx];
        }
    }

    // ---- Signs (speed limit) ----
    // Scan sign buffer for speed limit signs on agent's edge.
    var speed_limit: f32 = 0.0;
    let s_count = arrayLength(&signs);
    for (var s = 0u; s < s_count; s = s + 1u) {
        let sign = signs[s];
        if sign.edge_id == own_edge && sign.sign_type == SIGN_SPEED_LIMIT {
            speed_limit = sign.value;
            break; // take first matching speed limit
        }
    }

    // ---- Flags ----
    var result_flags: u32 = 0u;

    // bit0: route_blocked if congestion > 5.0 (severely congested)
    if cong_own > 5.0 {
        result_flags = result_flags | FLAG_ROUTE_BLOCKED;
    }

    // bit1: emergency_nearby if any emergency vehicle with active flag on same edge
    for (var e = 0u; e < total_agents; e = e + 1u) {
        if e == idx {
            continue;
        }
        let other = agents[e];
        if other.vehicle_type == VT_EMERGENCY
            && (other.flags & FLAG_EMERGENCY_ACTIVE) != 0u
            && other.edge_id == own_edge {
            result_flags = result_flags | FLAG_EMERGENCY_NEARBY;
            break;
        }
    }

    // ---- Write result ----
    results[idx] = PerceptionResult(
        best_leader_speed,
        best_leader_gap,
        sig_state,
        sig_distance,
        cong_own,
        cong_area,
        speed_limit,
        result_flags,
    );
}
