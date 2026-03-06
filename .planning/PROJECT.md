# VELOS -- GPU-Accelerated Traffic Microsimulation

## What This Is

A native macOS desktop application that simulates mixed urban traffic (motorbikes, cars, pedestrians) in real-time using GPU compute. The first slice targets ~1K agents in a small Ho Chi Minh City area, rendered natively via wgpu on Apple Silicon (Metal backend). Built as a pure Rust application using winit for windowing and egui for the dashboard UI.

## Core Value

Motorbikes move realistically through traffic using continuous sublane positioning -- not forced into discrete lanes like Western traffic models. If everything else is rough, this must look right.

## Requirements

### Validated

(None yet -- ship to validate)

### Active

- [ ] GPU compute pipeline dispatching agent updates via wgpu/Metal
- [ ] f64 CPU / f32 GPU arithmetic (no fixed-point for POC)
- [ ] hecs ECS managing agent state with GPU buffer mapping
- [ ] CFL numerical stability checks on simulation timestep
- [ ] Motorbike sublane model with continuous lateral position and filtering behavior
- [ ] Car IDM (Intelligent Driver Model) car-following
- [ ] MOBIL lane-change decision model for cars
- [ ] Pedestrian basic social force model (repulsion + attraction, no adaptive workgroups)
- [ ] OSM import of small HCMC road network into graph structure
- [ ] A* pathfinding on petgraph (no CCH, no rerouting)
- [ ] rstar R-tree spatial index for neighbor queries
- [ ] Traffic signal control at intersections (fixed-time)
- [ ] OD matrices and time-of-day demand profiles for agent spawning
- [ ] winit window with wgpu render surface from Phase 1
- [ ] GPU-instanced 2D rendering with styled agent shapes and direction arrows
- [ ] Zoom/pan camera, visible road lanes, intersection areas
- [ ] egui immediate-mode UI for simulation controls (start/stop/speed/reset)
- [ ] egui dashboard panels for real-time metrics and agent statistics
- [ ] Frame time and throughput benchmarks
- [ ] Gridlock detection at intersections

### Out of Scope

- Multi-GPU / RTX 4090 deployment -- macOS single-GPU first
- 280K agent scale -- targeting ~1K for this slice
- Full 5-district coverage -- one small HCMC area only
- Fixed-point arithmetic (Q16.16/Q12.20/Q8.8) -- deferred to scale-up phase
- Wave-front (Gauss-Seidel) dispatch -- simple parallel dispatch for POC
- CCH pathfinding -- A* on petgraph sufficient for 1K agents
- Prediction ensemble (BPR+ETS+historical) -- no travel time prediction
- Mesoscopic queue model / meso-micro hybrid -- full micro only
- Dynamic rerouting -- agents follow initial A* path
- Bicycle agents -- deferred
- Pedestrian adaptive GPU workgroups -- basic social force only
- deck.gl web visualization -- using native wgpu rendering instead
- FCD/GeoJSON/Parquet data exports -- deferred to later milestone
- Calibration / GEH validation -- no real-world data comparison yet
- Scenario DSL / batch runner -- interactive single-scenario only
- Redis pub/sub / WebSocket scaling -- in-process egui handles local control
- OAuth / authentication -- single-user desktop app
- CesiumJS 3D visualization -- 2D top-down view sufficient
- API server (gRPC/REST) -- deferred to v2

## Context

VELOS has extensive architecture documents in `docs/architect/` (7 documents) designed for a 2x RTX 4090 production deployment. This first slice simplifies that architecture significantly to run on a single macOS Apple Silicon machine, proving the core simulation pipeline and motorbike sublane model work before scaling up.

Key differentiator: Southeast Asian mixed traffic where 80% of vehicles are motorbikes that don't follow lane discipline. The sublane model uses continuous lateral positioning instead of discrete lane assignment.

The codebase currently has architecture docs, presentation slides, and GSD planning tools -- no Rust source code yet.

## Constraints

- **Platform**: macOS Apple Silicon (Metal GPU backend via wgpu)
- **Scale**: ~1K agents on a small HCMC road network segment
- **Toolchain**: Rust nightly (Edition 2024) -- needs portable_simd, async traits
- **App framework**: winit + egui (pure Rust, no webview)
- **UI**: egui immediate-mode GUI rendered via wgpu
- **Arithmetic**: f64 on CPU, f32 on GPU (no fixed-point for POC)
- **No external services**: Everything runs locally, no cloud dependencies

## Tech Stack

| Layer | Choice | Rationale |
|-------|--------|-----------|
| Language | Rust nightly (2024 edition) | portable_simd, async traits |
| GPU | wgpu + WGSL shaders | Cross-platform GPU abstraction, Metal on macOS |
| ECS | hecs | Lightweight, minimal overhead for simulation entities |
| CPU parallel | rayon + tokio | rayon for compute (OSM parse, pathfinding), tokio for async IO |
| Pathfinding | A* on petgraph | Simple, sufficient for 1K agents on small network |
| Spatial index | rstar | R-tree for neighbor queries in agent interactions |
| Window | winit | Cross-platform windowing, proven with wgpu (used by Bevy) |
| UI | egui + egui-wgpu | Immediate-mode GUI rendered on same wgpu surface as simulation |
| Rendering | GPU-instanced wgpu 2D | Styled shapes, direction arrows, zoom/pan, one draw call per type |
| Sim control | In-process | Direct function calls from egui to simulation engine, zero overhead |
| Serialization | bincode (internal) + Parquet (future) | Fast checkpoints now, columnar exports later |

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| f64 CPU / f32 GPU instead of fixed-point | No emulated i64 in WGSL, no golden vectors. Determinism deferred to 280K scale | -- Decided |
| Simple parallel dispatch instead of wave-front | Wave-front matters at 280K for convergence, not at 1K POC scale | -- Decided |
| A* on petgraph instead of custom CCH | CCH is massive custom work. A* sufficient for small network, no rerouting | -- Decided |
| No prediction/meso-micro | These are scale features. POC proves simulation pipeline, not optimization | -- Decided |
| Motorbikes + cars + pedestrians (no bicycles) | Core differentiator + essential interactions. Bicycles deferred | -- Decided |
| Styled + instanced rendering | GPU-instanced draw calls, styled shapes with direction arrows, zoom/pan | -- Decided |
| Rendering from Phase 1 | Visual feedback from day one. Minimal window with dots, grows with features | -- Decided |
| egui in Phase 2 | Add controls when there's real simulation to control | -- Decided |
| winit+egui instead of Tauri+React | No webview/wgpu surface conflict, single Rust binary, proven pattern | -- Decided |
| Nightly Rust | Need portable_simd for math performance | -- Pending |
| ~1K agents first | Prove pipeline on Metal before scaling to 280K on RTX 4090 | -- Decided |
| Keep ~12 crate structure | Create crates as needed, split at 700 lines | -- Decided |

---
*Last updated: 2026-03-06 after project simplification*
