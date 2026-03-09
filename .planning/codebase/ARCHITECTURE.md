# Architecture

**Analysis Date:** 2026-03-09

## Pattern Overview

**Overall:** GPU-accelerated ECS microsimulation with Cargo workspace monorepo (8 implemented crates), winit/wgpu native application with egui UI

**Key Characteristics:**
- Active development: 8 of 14 planned crates implemented with working GPU simulation pipeline
- Single binary entry point via `velos-gpu` crate (winit application with egui control panel)
- ECS (hecs) with SoA layout for CPU-side agent state, GPU buffers use fixed-point `GpuAgentState` (40 bytes/agent)
- Wave-front (Gauss-Seidel) per-lane GPU dispatch for vehicle car-following physics
- Dual physics path: GPU (`tick_gpu`) for production, CPU (`tick`) for tests without GPU
- Fixed-point integer arithmetic: Q16.16 position, Q12.20 speed, Q8.8 lateral offset for determinism
- In-process prediction ensemble (BPR + ETS + historical) with ArcSwap lock-free overlay
- CCH pathfinding implemented in `velos-net/src/cch/` with dynamic weight customization

## Layers

**Foundation Layer (velos-core):**
- Purpose: Shared ECS components, fixed-point types, CFL stability checks, cost functions, reroute evaluation
- Location: `crates/velos-core/src/`
- Contains: `Position`, `Kinematics`, `VehicleType`, `RoadPosition`, `Route`, `GpuAgentState`, `LateralOffset`, `WaitState`, `LaneChangeState`, `CarFollowingModel` (IDM/Krauss), `FixLat`/`FixPos`/`FixSpd` fixed-point types, `CostWeights` with 8 `AgentProfile` variants, `RerouteScheduler`
- Depends on: bytemuck, thiserror
- Used by: Every other velos crate

**GPU Orchestration Layer (velos-gpu):**
- Purpose: Application entry point, GPU compute dispatch, rendering, simulation orchestration
- Location: `crates/velos-gpu/src/`
- Contains: `VelosApp` (winit ApplicationHandler), `GpuState` (device/queue/surface), `SimWorld` (owns hecs World + all subsystems), `ComputeDispatcher` (WGSL pipelines), `Renderer` (agent + road rendering), `Camera2D`, `BufferPool`, `PerceptionPipeline`, `PedestrianAdaptivePipeline`, `MultiGpuScheduler`, `partition_network()`
- Depends on: wgpu, hecs, winit, egui, velos-core, velos-net, velos-vehicle, velos-demand, velos-signal, velos-predict, velos-meso
- Used by: Binary entry point (`main.rs`)
- **Note:** This crate is the "god crate" -- it owns simulation state, frame pipeline orchestration, and rendering. Split across ~20 source files to stay under 700 lines each.

**Network Layer (velos-net):**
- Purpose: Road graph, OSM/SUMO import, spatial indexing, routing, CCH pathfinding
- Location: `crates/velos-net/src/`
- Contains: `RoadGraph` (petgraph DiGraph wrapper), `RoadEdge` (length, speed limit, lane count, geometry, motorbike-only, time windows), `RoadNode` (position), `import_osm()`, `SpatialIndex` (rstar R-tree), `find_route()`, CCH module (`cch/` with topology, ordering, customization, query, cache), `EquirectangularProjection`, `clean_network()`, SUMO import (`sumo_import.rs`, `sumo_demand.rs`), `snap_to_nearest_edge()`
- Depends on: osmpbf, petgraph, rstar, velos-signal, velos-demand, velos-vehicle
- Used by: velos-gpu

**Vehicle Behavior Layer (velos-vehicle):**
- Purpose: Vehicle physics models (car-following, lane-change, sublane, social force)
- Location: `crates/velos-vehicle/src/`
- Contains: `IdmParams` + `idm_acceleration()`, `MobilParams` + `mobil_evaluate()`, `SublaneParams` + sublane filtering, `SocialForceParams`, `BusDwellModel` + `BusStop`, `EmergencyVehicle`, `GridlockDetector`, `KraussParams`, `IntersectionModel`, `VehicleConfig` (TOML-loadable per-type params)
- Depends on: log, rand, serde, toml, thiserror
- Used by: velos-gpu, velos-net, velos-meso

**Signal Control Layer (velos-signal):**
- Purpose: Traffic signal controllers (fixed-time, actuated, adaptive), detectors, signs
- Location: `crates/velos-signal/src/`
- Contains: `SignalController` trait, `FixedTimeController`, `ActuatedController`, `AdaptiveController`, `LoopDetector`, `SignalPlan`/`SignalPhase`, `SpatBroadcast`, `PriorityQueue` (bus/emergency priority), `TrafficSign`/`GpuSign`, `SignalConfig` (TOML), `IntersectionConfig`
- Depends on: thiserror, log, bytemuck, serde, toml
- Used by: velos-gpu, velos-net

**Demand Layer (velos-demand):**
- Purpose: Trip generation, OD matrices, time-of-day profiles, agent spawning
- Location: `crates/velos-demand/src/`
- Contains: `OdMatrix` (with `hcmc_5district()`), `Zone` enum (BenThanh, NguyenHue, Bitexco, BuiVien, Waterfront, District1/3/5/10, BinhThanh), `TodProfile` (with `hcmc_weekday()`), `Spawner`, `BusSpawner`, GTFS loader (`load_gtfs_csv()`), `SpawnRequest`, `ProfileDistribution`
- Depends on: thiserror, log, rand, velos-core
- Used by: velos-gpu, velos-net

**Mesoscopic Layer (velos-meso):**
- Purpose: Queue-based mesoscopic simulation for peripheral zones with buffer zone transitions
- Location: `crates/velos-meso/src/`
- Contains: `SpatialQueue` (BPR-based travel time), `BufferZone` (100m graduated C1-continuous IDM interpolation), `ZoneConfig` (edge-to-zone mapping: Micro/Meso/Buffer)
- Depends on: serde, thiserror, toml, velos-vehicle
- Used by: velos-gpu

**Prediction Layer (velos-predict):**
- Purpose: Edge travel time prediction for dynamic routing
- Location: `crates/velos-predict/src/`
- Contains: `PredictionService` (owns ensemble + overlay store, updates every 60 sim-seconds), `PredictionEnsemble` (BPR + ETS + Historical blended by `AdaptiveWeights`), `PredictionStore` (ArcSwap lock-free overlay), `PredictionOverlay` (per-edge travel times + confidence)
- Depends on: arc-swap, serde, log
- Used by: velos-gpu

## Data Flow

**Simulation Frame Pipeline (`SimWorld::tick_gpu`, 10 steps):**

1. `spawn_agents` -- create new agents from OD matrix + TodProfile demand
2. `update_loop_detectors` -- feed detector readings to actuated signals
3. `step_signals_with_detectors` -- advance signal controllers with detector data
4. `step_signal_priority` -- process bus/emergency signal priority requests
5. `step_glosa` -- GLOSA advisory speed reduction near non-green signals
6. `step_perception` -- GPU perception gather + readback (edge awareness, signal state)
7. `step_reroute` -- evaluate CCH rerouting from perception results
8. `step_meso` -- mesoscopic queue tick + buffer zone insertion
9. `step_lane_changes` -- MOBIL evaluation + lateral drift (CPU, cars)
10. `step_vehicles_gpu` -- GPU wave-front car-following physics (IDM/Krauss per agent)
11. `step_motorbikes_sublane` -- CPU lateral filtering for motorbikes
12. `step_bus_dwell` -- bus dwell lifecycle at stops
13. `step_prediction` -- prediction overlay refresh (every 60 sim-seconds)
14. `step_pedestrians_gpu` -- GPU adaptive social force model
15. `detect_gridlock` + `remove_finished_agents` + `update_metrics`

**CPU Fallback Pipeline (`SimWorld::tick`):**
- Same ordering but skips GPU perception/reroute
- Uses `cpu_reference::step_vehicles()` and `cpu_reference::step_motorbikes_sublane()` instead of GPU dispatch
- Used by integration tests that don't require a GPU device

**Application Frame Loop (`GpuState::update` + `render`):**
1. Compute frame dt from wall clock
2. Call `SimWorld::tick_gpu()` with base_dt=0.016s (60 FPS target)
3. Build agent instances (motorbikes, cars, pedestrians) + signal indicators
4. Upload instances to GPU render buffers
5. Render road lines + agents + egui UI overlay
6. Present surface

**Data Ingestion Flow:**
1. OSM PBF -> `velos_net::import_osm()` with HCMC center coords (10.7756, 106.7019) -> `RoadGraph`
2. Network cleaning -> `clean_network()` with optional override TOML
3. Signal config -> `data/hcmc/signal_config.toml` -> polymorphic controllers (Fixed/Actuated/Adaptive)
4. Vehicle params -> `data/hcmc/vehicle_params.toml` -> `VehicleConfig` -> GPU uniform buffer
5. GTFS CSV -> `load_gtfs_csv()` -> bus stops snapped to network + `BusSpawner`
6. OD matrix -> `OdMatrix::hcmc_5district()` hardcoded -> `Spawner` with `TodProfile::hcmc_weekday()`

**State Management:**
- ECS (hecs): All agent state as components in `SimWorld.world`
- GPU buffers: `BufferPool` with front/back double-buffering for position/kinematics
- Wave-front dispatch: `ComputeDispatcher` owns agent, lane offset, lane count, lane agent index buffers
- Perception: Shared `perception_result_buffer` between PerceptionPipeline (write) and wave_front.wgsl (read)
- Prediction: `PredictionStore` with ArcSwap for lock-free overlay reads

## Key Abstractions

**SimWorld:**
- Purpose: Central simulation state container -- owns ECS world, road graph, spawner, signal controllers, and all subsystems
- Examples: `crates/velos-gpu/src/sim.rs`
- Pattern: Monolithic struct with subsystem methods split across `sim_*.rs` files (sim_lifecycle, sim_vehicles, sim_signals, sim_pedestrians, sim_bus, sim_meso, sim_reroute, sim_perception, sim_render, sim_helpers, sim_startup, sim_snapshot)

**ComputeDispatcher:**
- Purpose: Owns WGSL compute pipelines and GPU buffer management for agent physics
- Examples: `crates/velos-gpu/src/compute.rs`, `crates/velos-gpu/src/compute_wave_front.rs`
- Pattern: Three pipeline families -- legacy `agent_update.wgsl` (simple Euler), wave-front `wave_front.wgsl` (production IDM+Krauss), pedestrian adaptive `pedestrian_adaptive.wgsl` (social force)

**GpuAgentState:**
- Purpose: 40-byte fixed-point GPU representation of agent state
- Examples: `crates/velos-core/src/components.rs`
- Pattern: `#[repr(C)]` Pod struct with fixed-point fields (edge_id, lane_idx, position Q16.16, lateral Q8.8, speed Q12.20, acceleration Q12.20, cf_model, rng_state, vehicle_type, flags bitfield)

**RoadGraph:**
- Purpose: Directed graph of road network backed by petgraph DiGraph
- Examples: `crates/velos-net/src/graph.rs`
- Pattern: Wrapper around `DiGraph<RoadNode, RoadEdge>` with convenience methods. Supports serialization via postcard for caching.

**PredictionService:**
- Purpose: Manages prediction ensemble and lock-free overlay store
- Examples: `crates/velos-predict/src/lib.rs`
- Pattern: Owns `PredictionEnsemble` + `PredictionStore`. Checks `should_update()` every tick, recomputes BPR/ETS/Historical blend, atomically swaps `PredictionOverlay` via ArcSwap.

**SignalController trait:**
- Purpose: Polymorphic interface for all signal controller types
- Examples: `crates/velos-signal/src/lib.rs`
- Pattern: Trait with `tick()`, `get_phase_state()`, `reset()`, `spat_data()`, `request_priority()`. Implemented by `FixedTimeController`, `ActuatedController`, `AdaptiveController`.

**CCH (Customizable Contraction Hierarchies):**
- Purpose: Fast dynamic-weight shortest path queries
- Examples: `crates/velos-net/src/cch/` (mod.rs, topology.rs, ordering.rs, customization.rs, query.rs, cache.rs)
- Pattern: Build node ordering + shortcut topology once. Call `customize()` with new edge weights (~3ms). Bidirectional Dijkstra queries on contracted graph.

## Entry Points

**Binary Application (`velos-gpu`):**
- Location: `crates/velos-gpu/src/main.rs`
- Triggers: `cargo run -p velos-gpu`
- Responsibilities: Creates winit EventLoop, instantiates `VelosApp`, runs event loop. On `resumed()`, creates window + GPU device + SimWorld. Frame loop: update sim -> render agents -> render egui -> present.

**CPU-only Test Path:**
- Location: `SimWorld::new_cpu_only()` in `crates/velos-gpu/src/sim.rs`
- Triggers: Integration tests that don't need GPU
- Responsibilities: Creates SimWorld without GPU pipelines (perception=None, ped_adaptive=None). Uses `tick()` instead of `tick_gpu()`.

## Error Handling

**Strategy:** Per-crate error enums using `thiserror` derive macros

**Patterns:**
- Each crate defines its own error type: `CoreError`, `GpuError`, `NetError`, `DemandError`, `MesoError`, `SignalError` (via `controller::ControllerError`), `VehicleError`
- Fallback with logging: config loading falls back to defaults with `log::warn!()` (e.g., vehicle config, signal config, zone config)
- GPU operations use `Option<T>` for pipelines that may not exist in CPU-only mode (perception, ped_adaptive)
- No panic-on-error in production paths -- graceful degradation with warnings

## Cross-Cutting Concerns

**Logging:** `log` crate with `env_logger`. Default filter: `warn`. Key log points: OSM import stats, signal controller counts, OD trip rates, zone config loading, spawn caps.

**Validation:** Zone centroids validated against graph node proximity at startup (warns if no nodes within 2km). CFL checks via `cfl_check()` in velos-core for numerical stability.

**Configuration:** TOML files in `data/hcmc/` for vehicle params, signal config, network overrides. Environment variable override: `VELOS_VEHICLE_CONFIG` for vehicle params path. Zone config loaded from `data/hcmc/zone_config.toml` (or defaults if missing).

**Determinism:** Fixed-point arithmetic in GPU shaders (`fixed_point.wgsl`). CPU-side uses f64 for precision. PCG hash RNG state per agent in `GpuAgentState.rng_state` for Krauss stochastic component. Simulation seeded with `StdRng::seed_from_u64(123)`.

**Performance:** Workgroup size 256 for GPU compute. Pre-allocated buffers for 300K agents. Spawn cap of 50 agents/tick to control growth. Base OD matrix produces ~140K trips/hr (previous 3x multiplier removed due to performance regression).

---

*Architecture analysis: 2026-03-09*
