# Architecture Research: v1.0 to v2 Migration

**Domain:** GPU-accelerated traffic microsimulation platform migration (desktop to web + multi-GPU)
**Researched:** 2026-03-07
**Confidence:** HIGH -- based on existing v1.0 codebase analysis + authoritative v2 architecture documents

## Executive Summary

The v1.0 VELOS system is a 6-crate desktop application (7,802 LOC) with CPU-side physics, A* routing, and winit+egui rendering. The v2 target is a 14-crate multi-service platform with GPU-accelerated physics, CCH routing, web visualization, gRPC/REST API, and Docker deployment. This document maps the precise migration path: which existing crates split, which extend, what new crates are needed, the dependency chains that determine build order, and the data flow changes required.

The key architectural shift is from **monolithic desktop binary** (velos-gpu owns everything including app shell, simulation loop, rendering) to **separation of concerns** (simulation engine as headless service, API layer, web frontend as separate deployable).

## Current Architecture (v1.0 -- 6 Crates)

```
                    velos-gpu (BINARY: main.rs)
                    ┌─────────────────────────────┐
                    │  VelosApp (winit + egui)     │
                    │  Renderer (wgpu 2D instanced)│
                    │  Simulation loop             │
                    │  ComputeDispatcher           │
                    │  BufferPool                  │
                    │  Camera2D                    │
                    └──┬────┬───┬───┬───┬─────────┘
                       │    │   │   │   │
         ┌─────────────┘    │   │   │   └──────────────┐
         │       ┌──────────┘   │   └─────────┐        │
         v       v              v             v        v
    velos-core  velos-net  velos-vehicle  velos-signal  velos-demand
    ┌────────┐  ┌────────┐  ┌──────────┐  ┌─────────┐  ┌──────────┐
    │Position│  │ graph  │  │ idm      │  │ control │  │od_matrix │
    │Kinemat │  │osm_imp │  │ mobil    │  │ plan    │  │tod_prof  │
    │Route   │  │routing │  │social_frc│  │         │  │ spawner  │
    │cfl     │  │spatial │  │ sublane  │  │         │  │          │
    │VhclType│  │project │  │ gridlock │  │         │  │          │
    └────────┘  └────────┘  │ types    │  └─────────┘  └──────────┘
                            └──────────┘
```

### Current Dependency Graph

```
velos-core      → (no deps -- leaf crate)
velos-net       → (no deps -- leaf crate)
velos-vehicle   → (no deps -- leaf crate)
velos-signal    → (no deps -- leaf crate)
velos-demand    → (no deps -- leaf crate)
velos-gpu       → velos-core, velos-net, velos-vehicle, velos-demand, velos-signal
```

**Critical observation:** `velos-gpu` is the God Crate. It contains:
1. wgpu device management
2. GPU compute dispatch
3. GPU rendering (2D instanced)
4. Application shell (winit window, egui UI)
5. Simulation loop orchestration
6. Camera control
7. Buffer management

This conflation is the primary architectural debt that must be resolved.

### Current Component Model (velos-core)

```rust
// CPU f64 positions, no fixed-point
Position { x: f64, y: f64 }
Kinematics { vx: f64, vy: f64, speed: f64, heading: f64 }
RoadPosition { edge_index: u32, lane: u8, offset_m: f64 }
Route { path: Vec<u32>, current_step: usize }
VehicleType { Motorbike, Car, Pedestrian }  // no Bus, no Bicycle
LateralOffset { lateral_offset: f64, desired_lateral: f64 }
WaitState { stopped_since: f64, at_red_signal: bool }
LaneChangeState { target_lane: u8, time_remaining: f64, started_at: f64 }
```

## Target Architecture (v2 -- 14 Crates + Dashboard)

```
┌─────────────────────────────────────────────────────────────────────┐
│                     velos-viz (TypeScript/React)                     │
│                     deck.gl + MapLibre + CesiumJS                   │
│                     Standalone web app                               │
└─────────────────────────────────┬───────────────────────────────────┘
                                  │ WebSocket + REST
┌─────────────────────────────────▼───────────────────────────────────┐
│                     velos-api (Rust binary)                          │
│                     tonic gRPC + axum REST/WS + Redis pub/sub        │
└──────┬──────────────────────────┬───────────────────────────────────┘
       │ gRPC                     │ Redis
┌──────▼──────────────────────────▼───────────────────────────────────┐
│                     velos-sim (Rust binary -- headless)              │
│                     Simulation orchestrator (scheduler + frame loop) │
│                     Depends on: core, gpu, net, vehicle, pedestrian, │
│                       signal, meso, predict, demand, calibrate,      │
│                       output, scene                                  │
└─────────────────────────────────────────────────────────────────────┘
│                                                                      │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐            │
│  │velos-core│  │velos-gpu │  │velos-net │  │velos-veh │            │
│  │ECS+sched │  │Device mgr│  │Graph+CCH │  │IDM+MOBIL │            │
│  │Checkpoint│  │Multi-GPU │  │Spatial   │  │Sublane   │            │
│  │Time ctrl │  │ShaderReg │  │OSM import│  │          │            │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘            │
│                                                                      │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐            │
│  │velos-ped │  │velos-sig │  │velos-meso│  │velos-pred│            │
│  │SocialFrc │  │Fixed-time│  │Queue mdl │  │BPR+ETS   │            │
│  │Adapt WG  │  │Actuated  │  │Buffer zn │  │Ensemble  │            │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘            │
│                                                                      │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐                          │
│  │velos-dem │  │velos-cal │  │velos-out │                          │
│  │OD matrix │  │GEH/RMSE │  │FCD/Parq  │                          │
│  │ToD prof  │  │Bayesian  │  │Emissions │                          │
│  └──────────┘  └──────────┘  └──────────┘                          │
│                                                                      │
│  ┌──────────┐                                                       │
│  │velos-scn │                                                       │
│  │ScenarioDSL│                                                      │
│  │Batch run │                                                       │
│  └──────────┘                                                       │
└─────────────────────────────────────────────────────────────────────┘
```

## Crate Migration Map

### Crates That SPLIT

| v1.0 Crate | Splits Into | Rationale |
|------------|-------------|-----------|
| **velos-gpu** | **velos-gpu** (library: device, buffers, multi-GPU, shader registry) + **velos-sim** (binary: simulation loop, frame pipeline orchestrator) + **velos-api** (binary: gRPC/REST/WS server) | The God Crate. Rendering moves to web (velos-viz). App shell (winit/egui) is replaced by headless server mode. Compute stays in velos-gpu. |
| **velos-vehicle** | **velos-vehicle** (IDM, MOBIL, sublane, types) + **velos-pedestrian** (social force, adaptive workgroups) | Pedestrian social_force.rs currently lives in velos-vehicle but has fundamentally different GPU dispatch patterns (spatial hash + prefix-sum compaction vs. per-lane wave-front). Separate crate enables independent iteration. |

### Crates That EXTEND (In-Place Modification)

| v1.0 Crate | What Changes | LOC Impact |
|------------|-------------|------------|
| **velos-core** | Add: world.rs (hecs wrapper), scheduler.rs (frame pipeline), time.rs (clock control), checkpoint.rs (Parquet save/restore). Modify: components.rs (add Bus, Bicycle types; add fixed-point fields alongside f64). Modify: Position to include Q16.16 offset, Q8.8 lateral. | ~2x current size |
| **velos-gpu** | Remove: app.rs, main.rs, camera.rs, renderer.rs, sim.rs, sim_*.rs (all app/render code). Add: partition.rs (METIS multi-GPU), pipeline.rs (registry), sync.rs (frame fences). Modify: device.rs (multi-adapter enumeration), buffers.rs (staging belt, double-buffer). | Net ~same LOC (remove render, add multi-GPU) |
| **velos-net** | Replace: routing.rs (A* on petgraph) with CCH implementation. Add: cch.rs (CCH ordering + customization). Modify: osm_import.rs (5-district coverage, signal inference). Modify: graph.rs (edge capacity, lane structure enrichment). | ~2x current size |
| **velos-signal** | Add: actuated.rs (vehicle-actuated controllers). Modify: controller.rs (signal state buffer for GPU consumption). | ~1.5x current size |
| **velos-demand** | Modify: od_matrix.rs (larger matrices, 5 districts). Modify: tod_profile.rs (HCMC 9-band profile). Add: sensor_calib.rs (integration with calibration data). | ~1.3x current size |

### New Crates to Create

| New Crate | Responsibility | Depends On | Build Priority |
|-----------|---------------|------------|----------------|
| **velos-sim** | Headless simulation binary. Frame pipeline orchestrator. Replaces velos-gpu/main.rs and sim.rs. Publishes frame data to Redis. | core, gpu, net, vehicle, pedestrian, signal, meso, predict, demand, output, scene | After core+gpu+net+vehicle stabilize |
| **velos-api** | gRPC server (tonic), REST gateway (axum), WebSocket relay with Redis pub/sub. | sim (via gRPC), Redis | After velos-sim produces frames |
| **velos-viz** | TypeScript/React dashboard. deck.gl layers, MapLibre base map, CesiumJS 3D optional. pnpm workspace, not a Rust crate. | velos-api (via WebSocket/REST) | Can start early (mock data) |
| **velos-pedestrian** | Social force model, adaptive workgroup spatial hashing, prefix-sum compaction. Extracted from velos-vehicle. | core (components), gpu (pipelines) | After vehicle model stabilizes |
| **velos-meso** | Mesoscopic queue model, graduated buffer zone, meso-micro velocity matching. | core, net, vehicle | Late -- after micro sim is proven |
| **velos-predict** | BPR+ETS+historical ensemble, ArcSwap overlay. | net (edge weights) | After CCH is working |
| **velos-calibrate** | GEH statistic, Bayesian optimization (argmin), RMSE validation, parameter tuning. | core, net, demand, output | After output produces data |
| **velos-output** | FCD recording, edge stats, emissions (HBEFA), Parquet/CSV/GeoJSON export. | core (ECS queries) | After sim loop is stable |
| **velos-scene** | Scenario DSL parser, batch runner, MOE comparison. | sim, output, calibrate | Latest priority |

## Component Responsibilities (v2)

| Component | Responsibility | Communication Pattern |
|-----------|----------------|----------------------|
| **velos-core** | ECS world (hecs), frame scheduler, time control, checkpoint save/restore to Parquet. Defines all shared component types. | Direct function calls. Owns the `World`. All crates depend on core for component types. |
| **velos-gpu** | wgpu device/queue management, multi-GPU partition manager, compute pipeline registry, buffer pool, shader registry. No rendering. | Called by velos-sim during frame pipeline. Returns updated buffers. |
| **velos-net** | Road graph (petgraph), OSM import, CCH pathfinding (replaces A*), rstar spatial index, edge weight management. | Called by velos-sim for routing. Reads PredictionOverlay via ArcSwap. |
| **velos-vehicle** | IDM car-following, MOBIL lane-change, motorbike sublane filtering, bicycle model. WGSL shader definitions. | Invoked during GPU compute pass. Reads ECS components, writes updated kinematics. |
| **velos-pedestrian** | Social force model, adaptive workgroup spatial hashing, jaywalking, gap acceptance. | Separate GPU compute pass with prefix-sum compaction dispatch. |
| **velos-signal** | Fixed-time and actuated signal controllers, phase cycling, signal state buffer. | CPU-side state machine. Writes signal buffer consumed by GPU shaders. |
| **velos-meso** | Mesoscopic queue model (BPR travel time), graduated buffer zone, meso-micro velocity matching. | Manages far-field agents cheaply. Hands off to micro sim at buffer zone boundary. |
| **velos-predict** | BPR+ETS+historical ensemble prediction, ArcSwap overlay for lock-free reads. | Background async task. Publishes PredictionOverlay consumed by velos-net. |
| **velos-demand** | OD matrices, time-of-day profiles, agent spawning and despawning. | Spawns entities into ECS world at configured rates. |
| **velos-calibrate** | GEH/RMSE statistics, Bayesian optimization via argmin, parameter sensitivity analysis. | Reads output data, adjusts IDM/demand parameters, triggers re-simulation. |
| **velos-output** | FCD recording, edge aggregation, emissions (HBEFA), Parquet/CSV/GeoJSON export. | Queries ECS each frame, writes to output buffers, flushes periodically. |
| **velos-scene** | Scenario DSL parser, batch runner, MOE metric comparison. | Drives velos-sim with different configs, collects output for comparison. |
| **velos-sim** | Headless simulation binary. Frame pipeline: upload, dispatch, readback, advance, publish. | Entry point binary. Owns simulation loop. Publishes to Redis. Exposes gRPC for control. |
| **velos-api** | gRPC (tonic) + REST (axum) + WebSocket relay with Redis pub/sub spatial tiling. | Connects to velos-sim via gRPC. Serves web clients. |
| **velos-viz** | deck.gl 2D dashboard, MapLibre basemap, CesiumJS 3D optional. TypeScript/React. | Consumes WebSocket binary frames + REST queries from velos-api. |

## Data Flow Changes (v1.0 vs v2)

### v1.0: In-Process Monolith

```
ECS World ──→ GPU Buffers ──→ Compute ──→ Readback ──→ ECS World
    │                                                       │
    └──→ egui UI (same process, same thread) ◄──────────────┘
```

### v2: Multi-Service Architecture

```
ECS World ──→ GPU Buffers ──→ Compute ──→ Readback ──→ ECS World
    │                                                       │
    │              velos-sim (headless binary)               │
    │                        │                               │
    │                        │ Redis pub/sub                 │
    │                        │ (tile:x:y channels)           │
    │                        ▼                               │
    │                  velos-api                              │
    │              (gRPC + REST + WS)                         │
    │                   │         │                           │
    │              WebSocket   REST                           │
    │                   │         │                           │
    │                   ▼         ▼                           │
    │               velos-viz (browser)                       │
    │            deck.gl + MapLibre                           │
    │                                                        │
    └──→ velos-output ──→ Parquet files                      │
    └──→ velos-predict ──→ ArcSwap ──→ velos-net (CCH)  ────┘
```

### Key Data Flow Differences

| Flow | v1.0 | v2 |
|------|------|-----|
| **Agent state cycle** | ECS → GPU → ECS (in-process, ~3ms) | Same, but within velos-sim headless binary |
| **UI updates** | Direct egui calls in same thread | Redis pub/sub → WebSocket → browser (adds ~5ms latency) |
| **User commands** | egui button → function call | Browser → WebSocket → velos-api → gRPC → velos-sim |
| **Pathfinding** | A* on petgraph (~0.5ms/query) | CCH (~0.02ms/query), 25x faster, dynamic weights |
| **Rendering** | wgpu 2D instanced (in-process) | deck.gl WebGL in browser (separate process) |
| **Checkpoint** | Not implemented | ECS → Parquet (velos-core), ~200ms for 280K agents |
| **Metrics** | egui dashboard (in-process) | Prometheus export → Grafana |

## Detailed Migration Steps

### Step 1: Decompose velos-gpu (The God Crate Split)

This is the highest-priority and highest-risk migration step. The current `velos-gpu` contains 14 source files serving 3 distinct roles.

**Files to REMOVE from velos-gpu (move to velos-sim):**
- `main.rs` -- application entry point, becomes velos-sim binary
- `app.rs` -- VelosApp (winit+egui app shell), replaced by headless server
- `sim.rs` -- simulation loop orchestration, moves to velos-sim
- `sim_helpers.rs` -- simulation helper functions, moves to velos-sim
- `sim_lifecycle.rs` -- init/reset/step logic, moves to velos-sim
- `sim_mobil.rs` -- MOBIL integration glue, moves to velos-sim
- `sim_render.rs` -- render frame logic, removed (web replaces)
- `sim_snapshot.rs` -- snapshot logic, migrates to velos-core checkpoint
- `camera.rs` -- Camera2D, removed (deck.gl handles camera)
- `renderer.rs` -- GPU-instanced 2D rendering, removed (deck.gl replaces)

**Files that STAY in velos-gpu (library crate):**
- `lib.rs` -- public API (simplified)
- `device.rs` -- wgpu device management (extended for multi-adapter)
- `buffers.rs` -- buffer pool (extended for staging belt, double-buffer)
- `compute.rs` -- compute dispatcher (extended for wave-front)
- `error.rs` -- error types

**New files in velos-gpu:**
- `partition.rs` -- METIS graph partitioning, GpuPartition struct
- `pipeline.rs` -- PipelineRegistry (pre-created compute pipelines)
- `sync.rs` -- frame fences, submission index tracking
- `transfer.rs` -- boundary agent inbox/outbox protocol

After this split, `velos-gpu` becomes a **library crate** (no binary). It provides GPU abstraction consumed by `velos-sim`.

### Step 2: Extract velos-pedestrian from velos-vehicle

**File to MOVE:** `velos-vehicle/src/social_force.rs` becomes `velos-pedestrian/src/social_force.rs`

**New files in velos-pedestrian:**
- `spatial_hash.rs` -- density-aware spatial hashing
- `prefix_sum.rs` -- GPU prefix-sum compaction
- `adaptive_workgroup.rs` -- dynamic workgroup sizing
- `crossing.rs` -- jaywalking logic, gap acceptance
- `params.rs` -- HCMC pedestrian parameters

**velos-vehicle retains:**
- `idm.rs`, `mobil.rs`, `sublane.rs`, `gridlock.rs`, `types.rs`

**Add to velos-vehicle:**
- `bicycle.rs` -- bicycle sublane model (rightmost, no filtering)
- `bus.rs` -- bus dwell time model, stop logic

### Step 3: Upgrade velos-core Components

The current component model uses f64 and simple types. The v2 model needs:

```rust
// v1.0 (current)
pub struct Position { pub x: f64, pub y: f64 }

// v2 (target) -- must coexist during migration
pub struct Position {
    pub edge_id: u32,
    pub lane_idx: u8,
    pub offset: FixedQ16_16,    // NEW: fixed-point
    pub lateral: FixedQ8_8,     // NEW: fixed-point sublane
}
```

**Migration strategy:** Add v2 component types alongside v1 types. Use a feature flag `v2-components` to switch. This allows incremental migration without breaking existing tests.

**New modules in velos-core:**
- `world.rs` -- hecs World wrapper with GPU index mapping
- `scheduler.rs` -- frame pipeline orchestrator (sequence of system passes)
- `time.rs` -- simulation clock with speed control (1x, 5x, 20x)
- `checkpoint.rs` -- ECS snapshot to/from Parquet (arrow-rs)
- `fixed_point.rs` -- Q16.16, Q12.20, Q8.8 types with arithmetic

### Step 4: Upgrade velos-net Routing

Replace A* with CCH. The petgraph graph structure stays but routing.rs is rewritten.

**Current:** `routing.rs` -- A* via `petgraph::algo::astar`
**Target:** `cch.rs` -- Customizable Contraction Hierarchies

**Migration:** Keep the old `routing.rs` as `routing_astar.rs` behind a feature flag. New default is CCH. This enables fallback if CCH spike fails.

### Step 5: Create Service Layer (velos-sim, velos-api, velos-viz)

This is where the desktop-to-web migration happens.

**velos-sim (Rust binary -- headless):**
- Absorbs simulation loop from velos-gpu
- No windowing, no rendering, no UI
- Publishes frame data to Redis
- Exposes gRPC service for control (start/stop/step/checkpoint)
- Can run standalone for benchmarking

**velos-api (Rust binary -- server):**
- tonic gRPC server connecting to velos-sim
- axum REST gateway for convenience queries
- WebSocket relay consuming Redis pub/sub tile channels
- Stateless, horizontally scalable

**velos-viz (TypeScript/React -- pnpm workspace):**
- deck.gl ScatterplotLayer for 280K agent positions
- MapLibre basemap with PMTiles
- KPI dashboard with charts
- WebSocket client consuming binary tile frames
- CesiumJS 3D (optional, stretch goal)

## v2 Dependency Graph

```
Level 0 (leaf crates, no workspace deps):
    velos-core         (components, checkpoint, time, CFL)

Level 1 (depend on core only):
    velos-net          (core: component types for edge/junction)
    velos-vehicle      (core: component types for agents)
    velos-signal       (core: component types for signal state)
    velos-demand       (core: component types for spawning)

Level 2 (depend on core + one Level 1):
    velos-gpu          (core: component types for buffer layout)
    velos-pedestrian   (core: component types)
    velos-predict      (core, net: edge weights)
    velos-meso         (core, net, vehicle: agent transition)
    velos-output       (core: ECS queries for recording)
    velos-calibrate    (core, net, demand, output: parameter tuning)

Level 3 (integration crates):
    velos-scene        (core, net, demand, output, calibrate: scenario orchestration)

Level 4 (binary crates):
    velos-sim          (ALL library crates: simulation binary)
    velos-api          (depends on velos-sim via gRPC, not Cargo dep)

Separate workspace:
    velos-viz          (TypeScript, connects via network to velos-api)
```

## Suggested Build Order (Dependency-Driven)

The build order must respect dependency chains while maximizing parallelism. Below is ordered by phases with explicit dependencies.

### Phase A: Foundation Refactoring (Weeks 1-4)

These steps unblock everything else and can be partially parallelized.

| Order | Task | Depends On | Unblocks |
|-------|------|------------|----------|
| A.1 | **Split velos-gpu**: extract sim loop, renderer, app shell into separate module boundaries. Keep compiling as single crate initially, but isolate the code. | Nothing | A.3, A.4 |
| A.2 | **Extend velos-core**: add world.rs, scheduler.rs, time.rs, fixed_point.rs. Add v2 component types with feature flag. | Nothing | A.3, B.1, B.2, B.3 |
| A.3 | **Create velos-sim binary**: move extracted sim loop code from velos-gpu into velos-sim. Make velos-gpu a pure library crate. At this point, the old winit/egui app stops working -- this is intentional. | A.1, A.2 | B.4, C.1 |
| A.4 | **velos-viz scaffold**: React+Vite project, deck.gl + MapLibre rendering HCMC basemap from PMTiles. Mock WebSocket data for agent dots. | Nothing (independent) | C.2 |

**Parallel tracks:** A.1+A.2 can run simultaneously. A.4 is fully independent. A.3 blocks on A.1+A.2.

### Phase B: Core Engine Upgrades (Weeks 5-12)

Upgrade the simulation models to v2 specifications.

| Order | Task | Depends On | Unblocks |
|-------|------|------------|----------|
| B.1 | **velos-net: CCH routing**: Replace A* with CCH. Keep A* as fallback. Spike S3 first. | A.2 (component types) | B.5 |
| B.2 | **velos-gpu: multi-GPU**: Multi-adapter enumeration, METIS partition, boundary protocol. Spike S2 first. | A.2, A.3 | B.6 |
| B.3 | **velos-gpu: wave-front dispatch**: Per-lane Gauss-Seidel WGSL shaders. Spike S1 first. | A.2, A.3 | B.6 |
| B.4 | **Extract velos-pedestrian**: Move social_force.rs out of velos-vehicle. Add adaptive workgroup dispatch. | A.2, A.3 | B.6 |
| B.5 | **velos-predict**: BPR+ETS+historical ensemble, ArcSwap overlay. | B.1 (CCH for weight customization) | B.6 |
| B.6 | **velos-sim integration**: Wire all upgraded crates into the frame pipeline. 280K agent benchmark. | B.1-B.5 | C.1 |

**Parallel tracks:** B.1, B.2, B.3, B.4 can all proceed in parallel after Phase A completes. B.5 needs B.1. B.6 is the integration point.

### Phase C: Service Layer (Weeks 10-16, overlaps Phase B)

Build the web platform while engine upgrades proceed.

| Order | Task | Depends On | Unblocks |
|-------|------|------------|----------|
| C.1 | **velos-sim Redis publishing**: Add Redis pub/sub output to velos-sim. Spatial tiling for frame data. | A.3 (velos-sim exists) | C.2 |
| C.2 | **velos-api**: gRPC server, WebSocket relay consuming Redis, REST gateway. | C.1 | C.3 |
| C.3 | **velos-viz live integration**: Replace mock WebSocket data with real frames from velos-api. | C.2, A.4 | D.1 |
| C.4 | **velos-core: checkpoint**: Parquet save/restore via arrow-rs. | A.2 | D.2 |

### Phase D: Analytics and Calibration (Weeks 14-24)

| Order | Task | Depends On | Unblocks |
|-------|------|------------|----------|
| D.1 | **velos-output**: FCD recording, edge stats, Parquet export. | B.6 (sim produces data) | D.2 |
| D.2 | **velos-calibrate**: GEH/RMSE framework, argmin integration. | D.1, C.4 | D.4 |
| D.3 | **velos-meso**: Queue model, graduated buffer zone. | B.6 | D.4 |
| D.4 | **velos-scene**: Scenario DSL, batch runner, MOE comparison. | D.1, D.2 | Done |

### Phase E: Hardening (Weeks 20-28)

| Order | Task | Depends On |
|-------|------|------------|
| E.1 | Fixed-point arithmetic (optional, based on spike results) | B.3 |
| E.2 | CesiumJS 3D (stretch) | C.3 |
| E.3 | Load testing (100 WebSocket viewers) | C.2 |
| E.4 | Docker Compose deployment | All services exist |
| E.5 | Prometheus/Grafana monitoring | E.4 |

## Critical Dependency Chain (Longest Path)

```
A.1 (split velos-gpu)
  → A.3 (create velos-sim)
    → B.3 (wave-front dispatch)
      → B.6 (integration, 280K benchmark)
        → D.1 (output)
          → D.2 (calibration)
            → D.4 (scenarios)
```

**Bottleneck:** The GPU engine work (A.1 → A.3 → B.2/B.3 → B.6) is the critical path. The web platform (A.4 → C.1 → C.2 → C.3) runs in parallel and is not on the critical path.

## Architectural Patterns

### Pattern 1: Headless Simulation Server

**What:** velos-sim runs as a headless binary with no windowing or rendering. It produces simulation frames consumed by other services via Redis and gRPC.

**Why:** Decouples simulation performance from visualization. Simulation can run faster-than-real-time (20x) without rendering overhead. Multiple visualization clients can connect/disconnect without affecting the simulation.

**Implementation:**
```rust
// velos-sim/src/main.rs
#[tokio::main]
async fn main() {
    let config = SimConfig::from_args();
    let world = World::new();
    let gpu = GpuContext::new_multi_adapter(config.gpu_count).await;
    let net = RoadNetwork::from_osm(&config.osm_path);
    let redis = RedisClient::connect(&config.redis_url).await;

    // gRPC control server on background task
    let (ctrl_tx, ctrl_rx) = mpsc::channel(64);
    tokio::spawn(grpc_server(ctrl_tx, config.grpc_port));

    // Simulation loop (runs on dedicated thread, not tokio)
    std::thread::spawn(move || {
        loop {
            simulation_step(&mut world, &gpu, &net);
            publish_frame(&redis, &world);  // Redis pub/sub
            handle_commands(&ctrl_rx);       // gRPC commands
        }
    });
}
```

### Pattern 2: Redis Spatial Tiling for Frame Distribution

**What:** Simulation publishes frame data to Redis channels keyed by spatial tile. WebSocket relay pods subscribe only to tiles their clients are viewing.

**Why:** Avoids broadcasting 280K agent positions to all clients. Each client receives only visible agents (~1K per tile). Enables horizontal scaling of WebSocket relay pods.

**Tile schema:** `tile:{x}:{y}` channels, 500m x 500m tiles, 256 tiles for HCMC POC area.

### Pattern 3: Feature-Flagged Component Migration

**What:** v2 component types coexist with v1 types during migration via Cargo feature flags.

**Why:** Enables incremental migration. Existing 185 tests continue passing during transition. Each subsystem can migrate independently.

```toml
# velos-core/Cargo.toml
[features]
default = ["v1-components"]
v1-components = []
v2-components = ["arrow-rs"]  # fixed-point, Parquet checkpoint
```

## Anti-Patterns to Avoid

### Anti-Pattern 1: Big-Bang Migration

**What people do:** Rewrite all 6 crates simultaneously to match v2 architecture.
**Why it's wrong:** 7,800 LOC of working code becomes untestable for weeks. Regression risk is enormous. If multi-GPU doesn't work, you've also broken the single-GPU path.
**Do this instead:** Incremental split. Keep v1.0 desktop app working as long as possible. Only break it at A.3 (velos-sim extraction) which is a deliberate, planned cut.

### Anti-Pattern 2: Premature Service Separation

**What people do:** Deploy velos-sim, velos-api, and velos-viz as separate Docker containers from day one.
**Why it's wrong:** Development velocity requires fast iteration cycles. Docker rebuild + restart adds 30-60s per change. Debugging across containers is harder.
**Do this instead:** Develop velos-sim and velos-api as separate Rust binaries in the same workspace, but run them locally without Docker during development. Docker Compose is for deployment testing and CI, not daily development.

### Anti-Pattern 3: Keeping the Desktop App Alive

**What people do:** Maintain both the winit+egui desktop app AND the web dashboard, thinking "we might need both."
**Why it's wrong:** Double the rendering code to maintain. The v2 architecture is a web platform; the desktop app adds no value once deck.gl visualization works.
**Do this instead:** Accept the desktop app dies at Phase A.3. All visualization moves to web. If local development needs quick visual verification, use the web dashboard on localhost.

### Anti-Pattern 4: Over-Abstracting the GPU Layer

**What people do:** Create a "GPU abstraction" that works for both compute and rendering, single and multi-GPU, with trait-based dispatch.
**Why it's wrong:** Compute and rendering have fundamentally different dispatch patterns. Multi-GPU adds partition management that single-GPU doesn't need. The abstraction leaks everywhere.
**Do this instead:** velos-gpu is specifically a compute library. It manages devices, pipelines, and buffers for simulation compute. Rendering is entirely web-side. Multi-GPU is a concrete implementation, not an abstract capability.

## Integration Points

### Internal Boundaries

| Boundary | Communication | Migration Impact |
|----------|---------------|------------------|
| velos-sim ↔ velos-gpu | Direct function calls (same process) | No change from v1 pattern, just relocated |
| velos-sim ↔ velos-core | Direct function calls (same process) | Scheduler orchestration is new |
| velos-sim ↔ Redis | Async publish via tokio | NEW: requires Redis client (fred or redis-rs) |
| velos-api ↔ velos-sim | gRPC (tonic) | NEW: requires protobuf definitions |
| velos-api ↔ Redis | Async subscribe via tokio | NEW: requires Redis client |
| velos-api ↔ velos-viz | WebSocket (binary) + REST (JSON) | NEW: FlatBuffers for binary frames |
| velos-predict ↔ velos-net | ArcSwap<PredictionOverlay> | NEW: lock-free overlay pattern |

### External Services

| Service | Purpose | Integration |
|---------|---------|-------------|
| Redis | Frame pub/sub, job queue | fred or redis-rs crate, localhost in dev, Docker in prod |
| Nginx | PMTiles serving, dashboard static files | Static config, no code integration |
| Prometheus | Metrics scraping | prometheus-client crate, /metrics endpoint on velos-sim and velos-api |
| Grafana | Dashboard visualization | Config-as-code, provisioned via Docker volume |

## New Workspace Dependencies

```toml
# Additions to workspace Cargo.toml [workspace.dependencies]
tonic = "0.12"           # gRPC server
prost = "0.13"           # Protobuf codegen
axum = "0.8"             # REST/WebSocket server
tokio = { version = "1", features = ["full"] }
fred = "9"               # Redis client (or redis = "0.27")
arrow = "54"             # Parquet read/write
parquet = "54"           # Parquet file format
flatbuffers = "24"       # WebSocket binary protocol
argmin = "0.10"          # Bayesian optimization
arc-swap = "1"           # Lock-free overlay pattern
prometheus-client = "0.22"  # Metrics
tracing = "0.1"          # Structured logging
tracing-subscriber = "0.3"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

## Scaling Considerations

| Scale | Architecture Adjustments |
|-------|--------------------------|
| 1.5K agents (current v1.0) | Single GPU, CPU physics, no Redis/API needed. Desktop app sufficient. |
| 50K agents | Single GPU, GPU physics via wave-front. Redis+API optional. Web dashboard useful. |
| 280K agents (v2 target) | Multi-GPU (2x), wave-front dispatch, Redis pub/sub required for visualization scale. Full service architecture. |
| 1M+ agents (v3 future) | Multi-node, 8-16 GPUs. gRPC-based distributed simulation. K8s deployment. |

### Scaling Priorities

1. **First bottleneck: velos-gpu God Crate split.** Cannot add multi-GPU, cannot run headless, cannot serve API until the monolith is decomposed. This is the gating factor for all v2 work.

2. **Second bottleneck: CCH routing.** A* at 0.5ms/query cannot support 500 reroutes/step at 10 Hz. CCH at 0.02ms/query enables dynamic rerouting. Block on Spike S3 result.

3. **Third bottleneck: Redis frame throughput.** At 280K agents, 10 Hz, 256 tiles: ~2MB/frame through Redis. Redis handles this easily. The bottleneck is the WebSocket relay fan-out to 100+ clients. Solution: stateless relay pods, scale horizontally.

## Sources

- VELOS v1.0 codebase analysis (6 crates, Cargo.toml dependency graph) -- HIGH confidence
- `docs/architect/00-architecture-overview.md` -- v2 component diagram, tech stack -- HIGH confidence (project-internal, authoritative)
- `docs/architect/01-simulation-engine.md` -- multi-GPU, wave-front, fixed-point -- HIGH confidence
- `docs/architect/02-agent-models.md` -- vehicle types, pedestrian adaptive workgroups -- HIGH confidence
- `docs/architect/05-visualization-api.md` -- WebSocket scaling, gRPC contracts, deck.gl layers -- HIGH confidence
- `docs/architect/06-infrastructure.md` -- Docker Compose, checkpoint, PMTiles -- HIGH confidence
- `docs/architect/07-timeline-risks.md` -- phase plan, dependency DAG, go/no-go gates -- HIGH confidence

---
*Architecture research for: VELOS v1.0 to v2 migration path*
*Researched: 2026-03-07*
