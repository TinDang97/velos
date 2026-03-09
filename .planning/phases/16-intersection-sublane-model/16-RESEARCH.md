# Phase 16: Intersection Sublane Model - Research

**Researched:** 2026-03-09
**Domain:** Junction geometry, sublane conflict resolution, PMTiles map rendering, wgpu 2D tile rendering
**Confidence:** HIGH

## Summary

Phase 16 adds three major capabilities: (1) continuous sublane positioning through intersections using precomputed quadratic Bezier curves, (2) conflict detection at junction crossing points with IDM-based yielding, and (3) self-hosted 2D vector map tile rendering from PMTiles. The simulation side (Bezier junction traversal, conflict resolution) is pure geometry and well-understood math with no external dependencies beyond what exists. The map tile rendering requires three new crates: `pmtiles2` for sync PMTiles reading, `mvt-reader` for MVT protobuf decoding, and `earcutr` for polygon triangulation. All are lightweight, well-maintained Rust crates.

The critical integration point is the edge transition logic in `sim_helpers.rs::advance_to_next_edge()`. Currently, when an agent reaches the end of an edge, it immediately teleports to the next edge. Phase 16 must intercept this transition to insert a junction traversal phase where the agent follows a Bezier curve through the junction before appearing on the exit edge. This requires a new ECS component (`JunctionTraversal`) and a new simulation step between edge physics and edge transitions.

**Primary recommendation:** Implement junction Bezier geometry and conflict detection in `velos-net` (data) and `velos-vehicle` (logic), junction traversal ECS state in `velos-core`, frame pipeline integration in `velos-gpu`, and map tile rendering as a new module within `velos-gpu`.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Quadratic Bezier curves for vehicle turn paths through intersections
- Control points: P0 = entry edge endpoint, P1 = junction centroid, P2 = exit edge startpoint
- Precompute one Bezier per (entry_edge, exit_edge) pair at network load (~15K junctions x ~4 turns avg = ~60K curves stored)
- Lateral offset maps onto curves by shifting perpendicular to the Bezier tangent -- motorbike on left side traces tighter inner arc
- Lateral offset is locked when agent enters junction -- no dynamic lateral filtering inside junction areas
- Motorbikes still filter freely on approach and departure edges
- Precompute Bezier crossing points per (turn_A, turn_B) pair at network load -- store ConflictPoint(turn_A, turn_B, t_A, t_B)
- Runtime: for agents in same junction, look up ConflictPoint and check if both agents' t-parameters are near crossing t-values
- Priority: agent closer to crossing point (lower |t - t_cross|) has priority -- they clear the conflict zone first
- Tie-breaking: use existing size_factor from intersection.rs (Emergency > Truck/Bus > Car > Motorbike)
- Yielding: treat conflict crossing point as virtual leader -- apply IDM car-following for smooth deceleration, stop before crossing point, resume when priority agent clears
- Approach-phase check: agents approaching junction entry check for foe agents already inside, using existing intersection_gap_acceptance() TTC logic
- PMTiles generated from OSM offline (tilemaker), decoded to wgpu geometry at runtime
- Full OSM detail: roads, water, buildings, POIs, labels, amenities, land use
- Decode MVT protobuf per tile, triangulate polygons (earcut), upload to wgpu vertex buffers, render as colored geometry
- Viewport-based dynamic tile loading -- only decode and upload tiles visible in current camera view
- LRU tile cache for decoded tiles, decode + triangulate on background thread
- No external tile server, no HTTP -- pure Rust, local file I/O
- Dashed Bezier guide lines rendered through junctions showing turn paths (toggleable via egui checkbox)
- Agent shapes rotate to follow Bezier curve tangent at current t-parameter
- Tangent computed as B'(t) = 2(1-t)(P1-P0) + 2t(P2-P1)
- Color by vehicle type: motorbike = orange, car = blue, bus = green, truck = red
- Size by vehicle type: motorbike = small, car = medium, bus/truck = large
- Toggleable debug overlay via egui: conflict crossing points (red dots), active conflict pair lines, agent t-parameters

### Claude's Discretion
- Exact tile cache size and eviction policy
- PMTiles zoom level selection strategy per camera zoom
- Bezier evaluation precision and sub-step count for long junction traversals
- Exact colors/opacity for map tile feature layers
- Label rendering approach for POIs/street names (text atlas vs simplified)

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| ISL-01 | Vehicles maintain continuous lateral position through junction internal edges | Bezier curve geometry with perpendicular lateral offset shift; JunctionTraversal ECS component tracks t-parameter; lateral_offset locked on entry |
| ISL-02 | Motorbikes filter and weave through intersection areas using probe-based gap scanning | Existing sublane.rs probe logic reused on approach/departure edges; lateral locked inside junction per decision |
| ISL-03 | Turn geometry supports sublane positioning (curved paths with lateral offset) | Quadratic Bezier precomputed per (entry, exit) pair; lateral offset shifts perpendicular to tangent B'(t) |
| ISL-04 | Conflict detection at crossing points within junctions resolves priority | Precomputed ConflictPoint(turn_A, turn_B, t_A, t_B); runtime t-proximity check; IDM virtual leader for yielding |
| MAP-01 | Self-hosted 2D vector map tiles from OSM render as background layer | pmtiles2 sync read + mvt-reader decode + earcutr triangulation + wgpu vertex buffers; viewport-based tile loading |
| MAP-02 | Sublane positions visually rendered in 2D with lane marking context | Agent heading from Bezier tangent; vehicle-type coloring; guide line overlay; map tiles provide lane context |
</phase_requirements>

## Standard Stack

### Core (existing workspace dependencies -- no new crate additions)
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| petgraph | 0.6 | Road graph -- extend RoadNode/RoadEdge with Bezier data | Already used for road network |
| hecs | 0.11 | ECS -- add JunctionTraversal component | Already the ECS framework |
| wgpu | 27 | GPU rendering -- new tile render pass, guide line pipeline | Already the render backend |
| glam | 0.29 | Vec2 math for Bezier evaluation | Already in workspace |
| bytemuck | 1 | Pod/Zeroable for GPU vertex types | Already in workspace |
| rayon | 1 | Background tile decode thread pool | Already in workspace |

### New Dependencies
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| pmtiles2 | 0.3.1 | Sync PMTiles v3 file reader | Read tile data by z/x/y from local .pmtiles file |
| mvt-reader | 2.3.0 | Decode MVT protobuf tile bytes into layers/features/geometry | Parse each tile after PMTiles extraction |
| earcutr | 0.5.0 | Earcut polygon triangulation | Triangulate building/water/landuse polygons for wgpu rendering |
| flate2 | 1 | gzip decompression | Decompress PMTiles tile data (tiles typically gzip-compressed) |
| lru | 0.12 | LRU cache for decoded tiles | Evict old tiles when cache exceeds limit |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| pmtiles2 (sync) | pmtiles (async, v0.20) | pmtiles is async-only, requires tokio runtime -- VELOS is sync wgpu app |
| earcutr | earcut (0.4.4) | earcut is newer rewrite but less battle-tested; earcutr is direct mapbox port |
| mvt-reader | geozero | geozero is heavier, more features than needed |
| lru | custom HashMap+VecDeque | LRU cache is deceptively complex; lru crate is tiny and correct |

**Installation:**
```bash
cargo add pmtiles2@0.3 mvt-reader@2.3 earcutr@0.5 flate2@1 lru@0.12 -p velos-gpu
```

## Architecture Patterns

### Recommended Structure (new/modified files)
```
crates/
  velos-net/src/
    junction.rs          # NEW: BezierTurn, ConflictPoint, precompute at graph load
    graph.rs             # MODIFY: extend RoadNode with junction_turns, conflict_points
  velos-core/src/
    components.rs        # MODIFY: add JunctionTraversal component
  velos-vehicle/src/
    junction_traversal.rs # NEW: advance_on_bezier(), conflict_check(), yield_deceleration()
  velos-gpu/src/
    map_tiles.rs         # NEW: PMTiles loader, MVT decoder, tile cache, wgpu tile renderer
    sim_junction.rs      # NEW: step_junction_traversal() frame pipeline step
    sim_render.rs        # MODIFY: vehicle-type coloring, Bezier tangent heading
    sim_helpers.rs       # MODIFY: intercept advance_to_next_edge for junction entry
    renderer.rs          # MODIFY: add tile render pass, guide line pipeline
  velos-gpu/shaders/
    map_tile.wgsl        # NEW: simple colored polygon shader for map tiles
    guide_line.wgsl      # NEW: dashed line shader for Bezier guide lines (or reuse road_line.wgsl)
```

### Pattern 1: Quadratic Bezier Junction Path
**What:** Each (entry_edge, exit_edge) pair at a junction gets a precomputed quadratic Bezier curve.
**When to use:** During graph load (once), stored on RoadNode or in a junction lookup table.
**Example:**
```rust
/// A precomputed turn path through a junction.
#[derive(Debug, Clone)]
pub struct BezierTurn {
    /// Entry edge index.
    pub entry_edge: u32,
    /// Exit edge index.
    pub exit_edge: u32,
    /// Bezier control points: P0=entry endpoint, P1=junction centroid, P2=exit startpoint.
    pub p0: [f64; 2],
    pub p1: [f64; 2],
    pub p2: [f64; 2],
    /// Approximate arc length for speed/time calculations.
    pub arc_length: f64,
}

impl BezierTurn {
    /// Evaluate position at parameter t in [0, 1].
    pub fn position(&self, t: f64) -> [f64; 2] {
        let u = 1.0 - t;
        [
            u * u * self.p0[0] + 2.0 * u * t * self.p1[0] + t * t * self.p2[0],
            u * u * self.p0[1] + 2.0 * u * t * self.p1[1] + t * t * self.p2[1],
        ]
    }

    /// Evaluate tangent (unnormalized) at parameter t.
    pub fn tangent(&self, t: f64) -> [f64; 2] {
        let u = 1.0 - t;
        [
            2.0 * u * (self.p1[0] - self.p0[0]) + 2.0 * t * (self.p2[0] - self.p1[0]),
            2.0 * u * (self.p1[1] - self.p0[1]) + 2.0 * t * (self.p2[1] - self.p1[1]),
        ]
    }

    /// Offset position perpendicular to tangent by lateral_offset metres.
    pub fn offset_position(&self, t: f64, lateral_offset: f64, road_half_width: f64) -> [f64; 2] {
        let pos = self.position(t);
        let tan = self.tangent(t);
        let len = (tan[0] * tan[0] + tan[1] * tan[1]).sqrt().max(1e-6);
        let nx = -tan[1] / len; // perpendicular (left-pointing)
        let ny = tan[0] / len;
        let offset_from_center = lateral_offset - road_half_width;
        [pos[0] + offset_from_center * nx, pos[1] + offset_from_center * ny]
    }
}
```

### Pattern 2: Conflict Point Precomputation
**What:** For each junction, find where turn paths cross and store the Bezier t-parameters.
**When to use:** During graph load, for every pair of turns at the same junction.
**Example:**
```rust
/// A precomputed crossing point between two turn paths.
#[derive(Debug, Clone, Copy)]
pub struct ConflictPoint {
    pub turn_a_idx: u16,  // index into junction's turns array
    pub turn_b_idx: u16,
    pub t_a: f32,         // Bezier t-parameter on turn A at crossing
    pub t_b: f32,         // Bezier t-parameter on turn B at crossing
}

/// Find crossing point by sampling both curves and finding minimum distance.
/// Returns None if curves don't intersect within tolerance.
pub fn find_conflict_point(a: &BezierTurn, b: &BezierTurn, steps: usize) -> Option<(f32, f32)> {
    let mut best_dist = f64::MAX;
    let mut best_ta = 0.0;
    let mut best_tb = 0.0;
    let inv = 1.0 / steps as f64;
    for i in 0..=steps {
        let ta = i as f64 * inv;
        let pa = a.position(ta);
        for j in 0..=steps {
            let tb = j as f64 * inv;
            let pb = b.position(tb);
            let dx = pa[0] - pb[0];
            let dy = pa[1] - pb[1];
            let dist = dx * dx + dy * dy;
            if dist < best_dist {
                best_dist = dist;
                best_ta = ta;
                best_tb = tb;
            }
        }
    }
    // Threshold: within 2m means curves cross (vehicle widths overlap)
    if best_dist.sqrt() < 2.0 {
        Some((best_ta as f32, best_tb as f32))
    } else {
        None
    }
}
```

### Pattern 3: Junction Traversal ECS Component
**What:** Tracks an agent currently traversing a junction on a Bezier curve.
**When to use:** Attached when agent enters junction, removed when agent exits to next edge.
**Example:**
```rust
/// ECS component for an agent traversing a junction on a Bezier curve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JunctionTraversal {
    /// Index of the junction node in the road graph.
    pub junction_node: u32,
    /// Index of the BezierTurn in the junction's turns array.
    pub turn_index: u16,
    /// Current parameter along the Bezier curve [0.0, 1.0].
    pub t: f64,
    /// Locked lateral offset (from when agent entered junction).
    pub lateral_offset: f64,
    /// Speed along the curve (m/s).
    pub speed: f64,
}
```

### Pattern 4: Viewport Tile Loading
**What:** Determine visible tile coordinates from camera bounds, load/decode on background thread.
**When to use:** Every frame (check), decode only when new tiles needed.
**Example:**
```rust
/// Convert camera viewport to tile x/y range at a given zoom level.
fn visible_tiles(camera: &Camera2D, zoom: u8) -> Vec<(u64, u64, u8)> {
    let half_w = camera.viewport.x / (2.0 * camera.zoom);
    let half_h = camera.viewport.y / (2.0 * camera.zoom);
    let min_x = camera.center.x - half_w;
    let max_x = camera.center.x + half_w;
    let min_y = camera.center.y - half_h;
    let max_y = camera.center.y + half_h;
    // Convert world coords (metres) to tile coords at zoom level
    // ... project back to lon/lat, then to tile x/y
    todo!()
}
```

### Anti-Patterns to Avoid
- **Bezier re-evaluation every frame for all agents:** Precompute arc length at graph load; only evaluate position/tangent for agents currently in a junction (not all 280K agents).
- **Blocking tile decode on main thread:** MVT decode + earcut triangulation for a tile can take 1-5ms. Must happen on a background thread with results uploaded to GPU when ready.
- **Dynamic conflict point computation:** Computing Bezier crossings at runtime is O(steps^2) per pair -- far too expensive. Precompute all at graph load.
- **Synchronous PMTiles read on render thread:** File I/O blocks rendering. Use a dedicated background thread for tile reads + decode.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| PMTiles v3 format parsing | Custom binary parser for header/directory/tile offsets | `pmtiles2` crate | Format has hilbert-curve tile ID encoding, internal compression, directory structure -- complex to parse correctly |
| MVT protobuf decoding | Custom protobuf parser for vector_tile.proto | `mvt-reader` crate | Zig-zag varint encoding, command-based geometry decoding, layer/feature/tag structure |
| Polygon triangulation | Custom ear-clipping algorithm | `earcutr` crate | Handles holes, degenerate polygons, collinear points -- many edge cases |
| LRU cache | HashMap + linked list | `lru` crate | Correct O(1) eviction with proper ordering is surprisingly tricky |
| Gzip decompression | Custom deflate | `flate2` crate | PMTiles tiles are gzip-compressed by default |

**Key insight:** The tile rendering pipeline (PMTiles -> MVT -> triangulation -> GPU) has four distinct decoding stages, each with non-trivial format complexity. Using dedicated crates for each stage avoids weeks of format debugging.

## Common Pitfalls

### Pitfall 1: Junction Entry/Exit Coordinate Mismatch
**What goes wrong:** Agent position jumps when entering/exiting junction because Bezier P0 doesn't exactly match the edge endpoint world position.
**Why it happens:** Edge geometry uses a polyline, and the last point may not exactly equal the node position in the graph.
**How to avoid:** Use `RoadNode.pos` (the actual node position) for P0/P2, NOT the last/first point of the edge geometry. Verify: `graph[edge_endpoints.1].pos == P0` for the entry edge.
**Warning signs:** Vehicles "teleporting" a few metres when entering/exiting junctions.

### Pitfall 2: Bezier Arc Length vs Parameter t
**What goes wrong:** Agents move at non-uniform speed through the junction because Bezier parameter t is not proportional to arc length.
**Why it happens:** Quadratic Beziers have non-uniform parameterization -- dt of 0.1 near the control point covers more distance than 0.1 near the endpoints.
**How to avoid:** Precompute approximate arc length for each BezierTurn (sample N points, sum segment lengths). At runtime, advance t by `dt * speed / arc_length`, which gives approximately uniform speed.
**Warning signs:** Agents appearing to slow down mid-turn then speed up, or vice versa.

### Pitfall 3: PMTiles Coordinate System Mismatch
**What goes wrong:** Map tiles render at wrong position or scale relative to simulation agents.
**Why it happens:** PMTiles/MVT use Web Mercator (EPSG:3857) coordinates. VELOS uses local metres projected from WGS84 via the custom `projection.rs` module.
**How to avoid:** When decoding MVT geometry, reproject each coordinate from Web Mercator to VELOS local metres using the same projection parameters as `velos_net::projection`. The projection origin is `(10.7756, 106.7019)` (HCMC center).
**Warning signs:** Map tiles appearing as a tiny dot or massive blob, or offset by kilometres.

### Pitfall 4: Tile Decode Thread Synchronization
**What goes wrong:** Decoded tile vertex buffers are uploaded to GPU from the wrong thread, causing wgpu validation errors.
**Why it happens:** wgpu device/queue are not Send+Sync in all backends. Buffer creation must happen on the main thread.
**How to avoid:** Background thread produces `Vec<TileVertex>` (CPU data). Main thread creates `wgpu::Buffer` from that data using `device.create_buffer_init()`. Use a channel (`std::sync::mpsc`) to send decoded tile data from background to main.
**Warning signs:** wgpu validation errors, crashes on buffer creation.

### Pitfall 5: Conflict Detection False Positives
**What goes wrong:** Agents yield to non-threatening vehicles that are on non-crossing paths.
**Why it happens:** Precomputed conflict points exist for all turn pairs, but at runtime the t-proximity threshold is too loose, triggering conflict for agents that won't actually collide.
**How to avoid:** Use a tight t-proximity threshold (e.g., |agent_t - conflict_t| < 0.15 for a typical 20m junction curve). Also check that the foe agent hasn't already passed the conflict point (foe_t > conflict_t + margin).
**Warning signs:** Vehicles stopping unnecessarily in the middle of intersections.

### Pitfall 6: Large Tile Vertex Buffer Overhead
**What goes wrong:** Memory usage spikes or frame rate drops when many tiles are loaded.
**Why it happens:** Each decoded tile produces thousands of triangulated vertices. At zoom 16, a tile with buildings can produce 50K+ vertices.
**How to avoid:** Limit the LRU cache to ~128 decoded tiles. At zoom 16, the viewport typically shows 4-16 tiles. With 128-tile cache, overhead is manageable (~50MB for vertices). Evict least-recently-used tiles including their GPU buffers.
**Warning signs:** Monotonically increasing memory, frame time spikes when panning.

## Code Examples

### Junction Precomputation (at graph load)
```rust
// In velos-net/src/junction.rs
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::Direction;
use crate::graph::{RoadNode, RoadEdge};

/// Precompute all BezierTurns and ConflictPoints for a junction node.
pub fn precompute_junction(
    graph: &DiGraph<RoadNode, RoadEdge>,
    node: NodeIndex,
) -> (Vec<BezierTurn>, Vec<ConflictPoint>) {
    let centroid = graph[node].pos;
    let incoming: Vec<_> = graph.edges_directed(node, Direction::Incoming).collect();
    let outgoing: Vec<_> = graph.edges_directed(node, Direction::Outgoing).collect();

    let mut turns = Vec::new();
    for inc in &incoming {
        let p0 = graph[inc.source()].pos; // entry endpoint
        // Use node position as control point (junction centroid)
        for out in &outgoing {
            if inc.source() == out.target() { continue; } // skip U-turns
            let p2 = graph[out.target()].pos; // exit startpoint
            let arc_length = estimate_arc_length(&p0, &centroid, &p2, 20);
            turns.push(BezierTurn {
                entry_edge: inc.id().index() as u32,
                exit_edge: out.id().index() as u32,
                p0, p1: centroid, p2,
                arc_length,
            });
        }
    }

    // Precompute conflict points between all turn pairs
    let mut conflicts = Vec::new();
    for i in 0..turns.len() {
        for j in (i + 1)..turns.len() {
            if let Some((ta, tb)) = find_conflict_point(&turns[i], &turns[j], 30) {
                conflicts.push(ConflictPoint {
                    turn_a_idx: i as u16,
                    turn_b_idx: j as u16,
                    t_a: ta,
                    t_b: tb,
                });
            }
        }
    }

    (turns, conflicts)
}

fn estimate_arc_length(p0: &[f64; 2], p1: &[f64; 2], p2: &[f64; 2], steps: usize) -> f64 {
    let mut length = 0.0;
    let inv = 1.0 / steps as f64;
    let mut prev = *p0;
    for i in 1..=steps {
        let t = i as f64 * inv;
        let u = 1.0 - t;
        let x = u * u * p0[0] + 2.0 * u * t * p1[0] + t * t * p2[0];
        let y = u * u * p0[1] + 2.0 * u * t * p1[1] + t * t * p2[1];
        let dx = x - prev[0];
        let dy = y - prev[1];
        length += (dx * dx + dy * dy).sqrt();
        prev = [x, y];
    }
    length
}
```

### Junction Traversal Step (frame pipeline)
```rust
// In velos-gpu/src/sim_junction.rs
impl SimWorld {
    /// Step 6.8: Advance agents currently traversing junctions.
    /// Runs after lane changes, before GPU vehicle physics.
    pub(crate) fn step_junction_traversal(&mut self, dt: f64) {
        // Collect updates to avoid borrow conflicts
        struct JunctionUpdate {
            entity: hecs::Entity,
            new_t: f64,
            position: [f64; 2],
            heading: f64,
            speed: f64,
            finished: bool,
        }
        let mut updates = Vec::new();

        for (entity, jt) in self.world.query_mut::<&JunctionTraversal>() {
            let junction = &self.junction_data[&jt.junction_node];
            let turn = &junction.turns[jt.turn_index as usize];

            // Advance t based on speed and arc length
            let dt_param = dt * jt.speed / turn.arc_length.max(1.0);
            let new_t = (jt.t + dt_param).min(1.0);

            // Check conflict: may need to decelerate
            let effective_speed = jt.speed; // conflict check may reduce this

            let road_half_width = 3.5; // TODO: from edge lane_count
            let pos = turn.offset_position(new_t, jt.lateral_offset, road_half_width);
            let tan = turn.tangent(new_t);
            let heading = tan[1].atan2(tan[0]);

            updates.push(JunctionUpdate {
                entity,
                new_t,
                position: pos,
                heading,
                speed: effective_speed,
                finished: new_t >= 1.0,
            });
        }

        for upd in updates {
            // Update Position and Kinematics
            if let Ok((pos, kin)) = self.world.query_one_mut::<(
                &mut Position, &mut Kinematics,
            )>(upd.entity) {
                pos.x = upd.position[0];
                pos.y = upd.position[1];
                kin.heading = upd.heading;
                kin.speed = upd.speed;
                kin.vx = upd.speed * upd.heading.cos();
                kin.vy = upd.speed * upd.heading.sin();
            }

            if upd.finished {
                // Transition to exit edge
                // Remove JunctionTraversal, set RoadPosition to exit edge offset=0
                let _ = self.world.remove_one::<JunctionTraversal>(upd.entity);
                // advance_to_next_edge handles the rest
            } else {
                if let Ok(jt) = self.world.query_one_mut::<&mut JunctionTraversal>(upd.entity) {
                    jt.t = upd.new_t;
                }
            }
        }
    }
}
```

### Map Tile Rendering (PMTiles + MVT + wgpu)
```rust
// In velos-gpu/src/map_tiles.rs -- conceptual structure
use std::collections::HashMap;
use std::sync::mpsc;
use lru::LruCache;

/// Vertex for map tile geometry (position + color).
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TileVertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
}

/// A decoded tile ready for GPU upload.
pub struct DecodedTile {
    pub x: u64,
    pub y: u64,
    pub z: u8,
    pub vertices: Vec<TileVertex>,
}

/// Map tile manager: handles loading, decoding, caching, and GPU rendering.
pub struct MapTileRenderer {
    /// PMTiles file reader (sync, local).
    pmtiles: Option<pmtiles2::PMTiles<std::io::BufReader<std::fs::File>>>,
    /// LRU cache of decoded tiles (key = (z, x, y)).
    tile_cache: LruCache<(u8, u64, u64), GpuTile>,
    /// Channel to receive decoded tiles from background thread.
    decoded_rx: mpsc::Receiver<DecodedTile>,
    /// Channel to request tile decoding.
    request_tx: mpsc::Sender<(u64, u64, u8)>,
    /// Render pipeline for tile geometry.
    pipeline: wgpu::RenderPipeline,
    /// Bind group for camera uniform (shared with agent renderer).
    camera_bind_group: wgpu::BindGroup,
}

struct GpuTile {
    vertex_buffer: wgpu::Buffer,
    vertex_count: u32,
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| SUMO: discrete internal lanes | VELOS: continuous Bezier + lateral offset | This phase | Motorbikes trace realistic paths through junctions |
| HTTP tile servers (Martin, etc.) | Local PMTiles file, pure Rust decode | PMTiles v3 (2023+) | Zero-ops, no server process, no network dependency |
| Browser-based deck.gl tiles | wgpu native tile rendering | This phase (v1.2 decision) | Native performance, no browser overhead |

**Deprecated/outdated:**
- The `pmtiles` crate (stadiamaps, v0.20) is async-only and requires tokio. Use `pmtiles2` for sync local file access.
- The `mvt` crate is for encoding only, not decoding. Use `mvt-reader` for decoding.

## Open Questions

1. **PMTiles zoom level mapping to camera zoom**
   - What we know: Camera2D has `zoom` (pixels per world unit). PMTiles have discrete zoom levels 0-20.
   - What's unclear: Exact mapping function from camera zoom to tile zoom level.
   - Recommendation: Use `floor(log2(camera.zoom * 256 / tile_size_pixels))` clamped to available zoom range. Start with zoom 14-16 for the HCMC POC area. Tune based on visual quality vs decode cost.

2. **Label rendering for POIs/street names**
   - What we know: MVT tiles contain label text as feature properties. wgpu has no built-in text rendering.
   - What's unclear: Whether to implement text atlas rendering or skip labels entirely for v1.2.
   - Recommendation: Skip label rendering for this phase. Map tile polygons (buildings, water, roads) provide sufficient spatial context. Labels can be added in a future phase using a glyph atlas approach. The egui overlay can show selected POI names on hover if needed.

3. **Conflict detection performance at scale**
   - What we know: ~15K junctions, ~4 turns each, ~60K curves. Conflict check is per-pair within each junction.
   - What's unclear: How many agents will be in junctions simultaneously (worst case at morning rush).
   - Recommendation: At 280K agents, expect ~5-10% in junctions at any time (14K-28K). Each agent checks conflicts only with other agents in the SAME junction (typically 2-8). Total conflict checks per frame: ~50K-100K, which is trivially fast at ~10ns each. No performance concern.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[cfg(test)]` + `cargo test` |
| Config file | Workspace Cargo.toml (already configured) |
| Quick run command | `cargo test -p velos-net --lib junction && cargo test -p velos-vehicle --lib junction_traversal` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| ISL-01 | Lateral offset preserved through junction | unit | `cargo test -p velos-vehicle --lib junction_traversal::tests::lateral_offset_preserved -x` | Wave 0 |
| ISL-02 | Probe gap scanning on approach/departure (existing) | unit | `cargo test -p velos-vehicle --lib sublane::tests -x` | Exists |
| ISL-03 | Bezier curve with lateral offset shift | unit | `cargo test -p velos-net --lib junction::tests::bezier_lateral_offset -x` | Wave 0 |
| ISL-04 | Conflict detection resolves priority | unit | `cargo test -p velos-vehicle --lib junction_traversal::tests::conflict_priority -x` | Wave 0 |
| MAP-01 | Tile decode + triangulate produces vertices | unit | `cargo test -p velos-gpu --lib map_tiles::tests::decode_tile -x` | Wave 0 |
| MAP-02 | Agent heading follows Bezier tangent | unit | `cargo test -p velos-vehicle --lib junction_traversal::tests::heading_follows_tangent -x` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p velos-net -p velos-vehicle -p velos-gpu --lib`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `crates/velos-net/src/junction.rs` -- BezierTurn, ConflictPoint, precompute (ISL-01, ISL-03, ISL-04)
- [ ] `crates/velos-vehicle/src/junction_traversal.rs` -- advance_on_bezier, conflict_check (ISL-01, ISL-04, MAP-02)
- [ ] `crates/velos-gpu/src/map_tiles.rs` -- tile decode + cache tests (MAP-01)
- [ ] New workspace dependencies: pmtiles2, mvt-reader, earcutr, flate2, lru

## Discretion Recommendations

### Tile Cache Size and Eviction
**Recommendation:** 128 tiles in LRU cache. At zoom 16, viewport shows ~4-16 tiles depending on camera zoom. 128 gives generous buffer for panning without re-decode. Each tile ~200KB-500KB decoded vertices, so ~25-64MB total cache. Use `lru::LruCache` with `NonZeroUsize::new(128)`.

### PMTiles Zoom Level Strategy
**Recommendation:** Map camera zoom ranges to tile zoom levels:
- Camera zoom < 0.5: tile zoom 14 (district overview)
- Camera zoom 0.5-2.0: tile zoom 15 (neighborhood)
- Camera zoom 2.0-8.0: tile zoom 16 (street level, default simulation view)
- Camera zoom > 8.0: tile zoom 16 (max detail, no point loading z17+ for HCMC POC)

### Bezier Evaluation Precision
**Recommendation:** Use 20 sample points for arc length estimation (already in example). For runtime t-advancement, the simple `dt * speed / arc_length` formula gives acceptable uniformity for typical junction sizes (10-30m curves). No sub-stepping needed.

### Map Feature Colors
**Recommendation:**
- Buildings: `[0.20, 0.18, 0.22, 0.8]` (dark grey, semi-transparent)
- Water: `[0.15, 0.25, 0.40, 0.9]` (dark blue)
- Roads (map layer): `[0.25, 0.25, 0.30, 0.6]` (grey, semi-transparent -- simulation road lines render on top)
- Parks/green: `[0.15, 0.25, 0.15, 0.7]` (dark green)
- Land use (residential): `[0.18, 0.17, 0.20, 0.5]` (subtle dark)

### Label Rendering
**Recommendation:** Skip for this phase. Map polygons provide sufficient context. Street names are visible from the road line overlay. POI labels would require a glyph atlas + text shaping pipeline -- significant work for marginal value in a simulation tool.

## Sources

### Primary (HIGH confidence)
- Existing codebase: `velos-vehicle/src/sublane.rs`, `intersection.rs`, `velos-gpu/src/renderer.rs`, `sim_helpers.rs`, `sim.rs` -- read directly
- Existing codebase: `velos-net/src/graph.rs`, `velos-core/src/components.rs` -- read directly
- [pmtiles2 docs](https://docs.rs/pmtiles2/latest/pmtiles2/struct.PMTiles.html) -- sync API confirmed: `get_tile(x, y, z)` returns `Result<Option<Vec<u8>>>`
- [mvt-reader docs](https://docs.rs/mvt-reader) -- v2.3.0, `Reader::new(data)`, `get_layer_names()`, `get_features(layer_index)`
- [earcutr docs](https://docs.rs/crate/earcutr/latest) -- v0.5.0, `earcut(&vertices, &holes, 2)` returns triangle indices

### Secondary (MEDIUM confidence)
- [tilemaker.org](https://tilemaker.org/) -- OSM to PMTiles generation tool, Lua scripting for tag selection
- [pmtiles GitHub](https://github.com/stadiamaps/pmtiles-rs) -- main crate is async-only, confirmed by docs showing tokio dependency
- Quadratic Bezier math -- standard computational geometry, well-known formulas

### Tertiary (LOW confidence)
- Tile vertex count estimates (~50K vertices per tile at zoom 16) -- extrapolated from general MVT tile sizes, not measured on HCMC data. Validate during implementation.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all crates verified via docs.rs, APIs confirmed
- Architecture: HIGH -- patterns directly extend existing codebase patterns (ECS components, render pipelines, frame pipeline steps)
- Pitfalls: HIGH -- based on direct code reading (coordinate systems, edge transitions, wgpu threading)
- Map tile pipeline: MEDIUM -- pmtiles2 sync API confirmed but not tested with actual HCMC PMTiles data

**Research date:** 2026-03-09
**Valid until:** 2026-04-09 (stable domain -- geometry math and tile formats don't change)
