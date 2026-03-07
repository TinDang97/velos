# Project Research Summary

**Project:** VELOS v1.1 Digital Twin Platform
**Domain:** GPU-accelerated traffic microsimulation -- scaling from desktop POC to web-based digital twin platform
**Researched:** 2026-03-07
**Confidence:** HIGH

## Executive Summary

VELOS v1.1 transforms a 1.5K-agent desktop POC (egui, CPU physics, A* routing) into a 280K-agent web-based digital twin platform with multi-GPU compute, CCH dynamic routing, prediction ensemble, gRPC/REST/WebSocket API, deck.gl visualization, and Docker deployment. This is a well-defined migration path from a validated v1.0 codebase (6 crates, 7,802 LOC, 185 passing tests) to a 14-crate multi-service architecture. The domain is mature (SUMO/VISSIM/Aimsun are established competitors), the target stack is verified (all Rust crates confirmed on crates.io, frontend packages on npm), and the architecture documents are thorough. The core differentiator -- GPU-accelerated microscopic simulation with native motorbike sublane at 200K+ scale -- has no direct open-source competitor.

The recommended approach is incremental migration driven by three technical spikes (wave-front GPU dispatch, multi-GPU adapter enumeration, CCH pathfinding) that produce binary GO/NO-GO decisions before committing to expensive implementation. The critical path runs through the "God Crate" split of velos-gpu (monolithic desktop binary to headless simulation library), then GPU engine upgrades (wave-front dispatch, multi-GPU partitioning), then service layer (API server, WebSocket streaming, web visualization). The web platform (deck.gl dashboard, API server) can be built in parallel with engine work and is NOT on the critical path.

The top risks are: (1) GPU compute is "proven but not wired" into v1.0's sim loop -- the very first task must be GPU cutover, not maintaining a parallel CPU path; (2) wgpu multi-adapter support for compute is untested at scale and may not work, requiring fallback to single-GPU 200K agents; (3) wave-front dispatch may starve GPU occupancy at 5.6 agents/lane average, requiring hybrid dispatch; (4) deck.gl CPU attribute generation blocks rendering at 280K points without server-side binary attribute packing; (5) fixed-point arithmetic performance penalty is likely 40-80% (not the estimated 20%) and should be deferred. All five risks have documented fallback strategies with LOW-MEDIUM recovery cost.

## Key Findings

### Recommended Stack

The v1.0 validated stack (Rust nightly, wgpu, hecs, petgraph, rstar) is retained. v1.1 adds approximately 25 new Rust crates and a TypeScript/React frontend. All versions are verified on crates.io/npm. The stack divides cleanly into: API server layer (tonic 0.14 + axum 0.8, sharing tokio runtime), data storage (parquet/arrow 57, flatbuffers 25.12 for WebSocket binary frames), prediction (ndarray 0.17, arc-swap 1.8), calibration (argmin 0.10), observability (tracing 0.1, metrics 0.24 + Prometheus exporter), and frontend (deck.gl 9.2, MapLibre 5.19, React 19, Vite 6).

**Core technologies:**
- **tonic 0.14 + axum 0.8:** gRPC + REST/WebSocket on shared tokio runtime -- the platform unlock that transforms a desktop app into a programmable service
- **parquet/arrow 57:** Columnar checkpoint/export matching ECS SoA pattern, readable by Python/DuckDB without custom deserialization
- **flatbuffers 25.12:** Zero-copy WebSocket frames (8 bytes/agent) for 10Hz streaming to deck.gl -- protobuf requires full deserialization
- **deck.gl 9.2 + MapLibre 5.19:** GPU-accelerated 2D visualization handling 200K+ points at 60 FPS via WebGL instancing
- **redis 1.0:** Pub/sub tile-based frame fan-out enabling horizontal WebSocket relay scaling for 100+ viewers
- **tracing + metrics:** Structured observability replacing v1.0's unstructured logging -- essential for debugging 280K-agent simulations
- **argmin 0.10:** Bayesian optimization for GEH/RMSE calibration without writing custom optimizers

**Critical exclusions:** No Tauri (web replaces desktop), no Python bridge (Rust-native prediction), no Kubernetes (Docker Compose sufficient), no bincode (RUSTSEC-2025-0141), no Wiedemann 99 (uncalibrated W99 worse than calibrated IDM for HCMC).

### Expected Features

**Must have (table stakes -- P1):**
- Multi-GPU wave-front dispatch + fixed-point (performance unlock for 280K agents)
- 5-district HCMC network (25K edges, 15K junctions, network cleaning)
- gRPC/REST API (platform transformation, non-negotiable for external integration)
- WebSocket streaming + Redis pub/sub (enables web visualization and multi-viewer)
- deck.gl 2D visualization (primary web interface)
- Parquet checkpoint/restart (crash recovery, batch run foundation)
- Data export (FCD, Parquet, CSV for researcher analysis)
- Docker Compose deployment (reproducible 7-service stack)
- Prometheus/Grafana monitoring (operational visibility at scale)
- PMTiles map tiles (zero-ops base maps)
- Bus dwell + bicycle agents (complete multi-modal roster)

**Should have (differentiators -- P2):**
- CCH dynamic pathfinding (3ms weight update, 25x faster than A*, no open-source sim has this)
- In-process BPR+ETS+historical prediction ensemble (zero-latency, no Python sidecar)
- GEH/RMSE calibration with Bayesian optimization (model validation for engineering credibility)
- Pedestrian adaptive GPU workgroups (3-8x speedup for non-uniform density)
- Scenario DSL + batch runner + MOE comparison (developer experience advantage)
- HBEFA emissions (policy evaluation for motorbike restriction zones)

**Defer (v2+):**
- Meso-micro hybrid (likely unnecessary if full-micro handles 280K on 2-4 GPUs within 15ms frame time)
- CesiumJS 3D visualization (presentation-only, add for stakeholder demos after deck.gl proves the platform)
- SUMO TraCI compatibility (fundamentally incompatible with GPU-parallel execution)
- Multi-node distributed simulation (280K fits single-node)
- Real-time sensor data fusion (massive scope creep, offline calibration sufficient)
- ML/DL prediction (no training data for HCMC yet)

### Architecture Approach

The architecture migrates from a monolithic desktop binary (velos-gpu as "God Crate" containing app shell, renderer, simulation loop, GPU compute, and camera) to a multi-service platform with strict separation of concerns. The key decomposition: velos-gpu splits into a pure compute library + velos-sim (headless simulation binary) + velos-api (gRPC/REST/WS server). Pedestrian code extracts from velos-vehicle into velos-pedestrian due to fundamentally different GPU dispatch patterns. The v2 dependency graph has 5 levels: leaf crate (velos-core), Level 1 libraries (net, vehicle, signal, demand), Level 2 libraries (gpu, pedestrian, predict, meso, output, calibrate), Level 3 integration (scene), Level 4 binaries (sim, api), and a separate TypeScript workspace (viz).

**Major components:**
1. **velos-sim** -- Headless simulation binary owning the frame pipeline; publishes to Redis; exposes gRPC for control
2. **velos-gpu** -- Pure compute library (device management, multi-GPU partitioning, buffer pools, shader registry); no rendering
3. **velos-api** -- gRPC + REST + WebSocket relay consuming Redis pub/sub tile channels; stateless, horizontally scalable
4. **velos-viz** -- React/TypeScript deck.gl dashboard consuming WebSocket binary frames + REST queries
5. **velos-core** -- ECS world (hecs), frame scheduler, time control, checkpoint to Parquet, shared component types
6. **velos-net** -- Road graph, OSM import, CCH pathfinding (replacing A*), spatial index, edge weight management

### Critical Pitfalls

1. **GPU compute not wired into sim loop** -- v1.0's CPU physics is the real driver; GPU is "proven via tests" only. Kill the CPU path immediately in Phase 1. Build a GPU validation compute pass for debugging. Gate G1 on GPU physics, not just GPU rendering.

2. **wgpu multi-adapter may not work for compute** -- WebGPU spec does not expose multi-GPU; wgpu's native multi-adapter is an extension with known platform issues. Run Spike S2 before any multi-GPU code. Design partition abstraction so single-GPU is a trivial specialization (partitions.len() == 1).

3. **Wave-front dispatch may starve GPU occupancy** -- 5.6 agents/lane average means <10% occupancy. Spike S1 must validate >40% of naive parallel throughput. Fallback: hybrid dispatch (wave-front for dense lanes, parallel for sparse lanes) or EVEN/ODD with correction passes.

4. **deck.gl CPU attribute generation blocks at 280K** -- JavaScript accessor loops take 30-80ms/frame on main thread. Must send pre-packed Float32Arrays via binary WebSocket and use deck.gl's `data.attributes` API to bypass accessors entirely. Implement viewport-based filtering (5K-30K visible agents, not 280K).

5. **Fixed-point arithmetic penalty is 40-80%, not 20%** -- IDM formula chains 6+ fixed-point multiplications plus iterative sqrt. Defer to Phase 3 or later. Use float32 + @invariant fallback. Cross-GPU determinism is nice-to-have for POC, not a requirement.

## Implications for Roadmap

Based on research, suggested phase structure:

### Phase 1: Foundation and Technical Spikes
**Rationale:** The God Crate split unblocks everything. Three spikes (wave-front dispatch, multi-GPU, CCH) must produce GO/NO-GO decisions before expensive implementation. GPU compute must become the real physics driver immediately.
**Delivers:** Decomposed crate structure (velos-gpu as library, velos-sim as binary), GPU physics as sole driver, spike results for wave-front/multi-GPU/CCH, velos-viz scaffold with mock data, v2 component types in velos-core.
**Addresses:** God Crate decomposition, GPU cutover, 5-district network extension, velos-core upgrade (world, scheduler, time, fixed-point types).
**Avoids:** Pitfall 1 (GPU not wired), Pitfall 2 (multi-GPU assumptions), Pitfall 3 (wave-front occupancy).
**Critical gate:** All v1.0 tests pass with GPU physics as sole driver. Spike S1/S2/S3 produce binary GO/NO-GO.

### Phase 2: GPU Engine and Scale
**Rationale:** With spikes validated, implement multi-GPU partitioning, wave-front dispatch, and scale to 280K agents. This is the critical path -- web platform work continues in parallel.
**Delivers:** Multi-GPU wave-front dispatch (or fallback), 280K agent simulation at <15ms frame time, CCH routing (replacing A*), prediction ensemble, motorbike sublane on GPU.
**Addresses:** Multi-GPU wave-front dispatch, fixed-point arithmetic (if spike positive), CCH dynamic pathfinding, prediction ensemble, pedestrian adaptive workgroups, motorbike sublane GPU port.
**Avoids:** Pitfall 3 (occupancy -- use hybrid dispatch if needed), Pitfall 5 (fixed-point -- defer if penalty too high).
**Uses:** wgpu 28, rayon 1.11, ndarray 0.17, arc-swap 1.8.

### Phase 3: Web Platform and API
**Rationale:** Can start early (Phase 1) with mock data and integrate live data once velos-sim produces frames. Not on the critical path but enables all external interaction.
**Delivers:** gRPC/REST API, WebSocket streaming with Redis pub/sub spatial tiling, deck.gl 2D visualization with live agent data, FlatBuffers binary protocol, PMTiles basemap.
**Addresses:** gRPC/REST API, WebSocket + Redis pub/sub, deck.gl visualization, PMTiles, data export.
**Avoids:** Pitfall 4 (deck.gl bottleneck -- binary attributes from day one), Pitfall 7 (Docker GPU -- early validation test).
**Uses:** tonic 0.14, axum 0.8, redis 1.0, flatbuffers 25.12, deck.gl 9.2, MapLibre 5.19.

### Phase 4: Calibration, Output, and Scenarios
**Rationale:** Calibration requires all agent models running + traffic count data + data export. Scenario DSL requires calibrated baseline. These are the validation features that transform animation into engineering data.
**Delivers:** GEH/RMSE calibration with Bayesian optimization, FCD/Parquet/CSV/GeoJSON export, HBEFA emissions, scenario DSL + batch runner + MOE comparison, Parquet checkpoint/restart.
**Addresses:** GEH calibration, data export, HBEFA emissions, scenario DSL, checkpoint/restart.
**Avoids:** Pitfall 6 (meso-micro discontinuities -- benchmark against full-micro before integrating).
**Uses:** argmin 0.10, parquet/arrow 57, geojson 0.24, quick-xml 0.39.

### Phase 5: Deployment and Hardening
**Rationale:** Docker containerization, monitoring, and load testing happen after all services exist and are individually validated.
**Delivers:** Docker Compose 7-service stack, Prometheus/Grafana dashboards, 100-viewer WebSocket load test, bus dwell + bicycle agents, CesiumJS 3D (stretch).
**Addresses:** Docker deployment, monitoring, multi-modal agents, CesiumJS (optional).
**Avoids:** Pitfall 7 (Docker GPU -- use NVIDIA_DRIVER_CAPABILITIES=all, mount Vulkan ICD files).
**Uses:** Docker Compose 3.9, Prometheus 2.48+, Grafana 10.2+, Redis 7 Alpine, Nginx Alpine.

### Phase Ordering Rationale

- **Phase 1 before all else:** The God Crate split is the single gating factor for all v2 work. Cannot add multi-GPU, cannot run headless, cannot serve API until velos-gpu is decomposed. Spikes prevent wasted effort on approaches that may not work.
- **Phase 2 is the critical path:** GPU engine work (wave-front, multi-GPU, CCH) determines whether 280K agents are feasible. Everything downstream depends on a working, performant simulation engine.
- **Phase 3 overlaps Phase 2:** The web platform can start with mock data in Phase 1 and integrate live data as Phase 2 delivers. This maximizes parallelism without creating dependencies.
- **Phase 4 after Phase 2+3:** Calibration needs the full simulation running (Phase 2) and data export working (Phase 3). Scenario DSL needs calibrated baseline. This ordering prevents meaningless MOE comparisons against uncalibrated models.
- **Phase 5 last:** Deployment hardening requires all services to exist. CesiumJS is presentation-only and deferred.

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 1:** Spike S1 (wave-front occupancy) and S2 (multi-GPU adapter) have uncertain outcomes. Research the specific GPU hardware available and wgpu version's multi-adapter capabilities before planning detailed tasks.
- **Phase 2:** CCH implementation has no off-the-shelf Rust crate. Academic papers (Dibbelt 2014) and RoutingKit docs are the primary references. Plan for 2-3 weeks of pure algorithm implementation.
- **Phase 4:** Meso-micro hybrid (if needed) is an active research problem with no standard solution. Burghout's KTH thesis is the best reference but boundary artifacts require iterative tuning.

Phases with standard patterns (skip research-phase):
- **Phase 3:** gRPC + REST + WebSocket with Redis pub/sub is well-documented. tonic + axum share tokio runtime. deck.gl performance patterns are documented in official guides.
- **Phase 5:** Docker Compose deployment and Prometheus/Grafana monitoring are standard ops patterns with extensive documentation.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | All crate versions verified on crates.io. Compatibility matrix validated. No speculative dependencies. |
| Features | HIGH | Domain well-established (SUMO/VISSIM/Aimsun as references). Feature landscape grounded in competitor analysis and v1.0 validation. |
| Architecture | HIGH | Based on existing v1.0 codebase analysis + thorough v2 architecture documents. Migration path is concrete, not abstract. |
| Pitfalls | MEDIUM-HIGH | GPU-specific pitfalls (multi-adapter, wave-front occupancy, fixed-point) have uncertain outcomes requiring spikes. Recovery strategies are documented for all pitfalls. |

**Overall confidence:** HIGH

### Gaps to Address

- **wgpu multi-adapter compute:** No production evidence of wgpu managing 2+ GPUs for compute in a single process. Spike S2 is the only way to validate. If NO-GO, scope down to single-GPU 200K agents (architecture already accounts for this).
- **CCH implementation complexity:** No Rust CCH crate exists. Academic algorithm is well-documented but custom implementation is estimated at 2-3 weeks. RoutingKit (C++) can serve as reference but is not directly portable.
- **HCMC traffic count data availability:** GEH calibration requires ~50 loop counter locations from HCMC DOT. Data access has not been confirmed. Fallback: calibrate against GPS probe data (Grab/Bee) or defer calibration to when data becomes available.
- **deck.gl binary attribute path at scale:** The pre-packed Float32Array approach bypasses accessor functions but has limited community examples at 280K points with 10Hz updates. Needs early prototyping in Phase 1 (velos-viz scaffold).
- **Fixed-point compound cost:** The 40-80% penalty estimate is derived from formula analysis, not empirical measurement. The actual cost depends on GPU architecture (Metal vs Vulkan) and compiler optimizations. Only measurable via Spike S1 variant or dedicated benchmark.

## Sources

### Primary (HIGH confidence)
- VELOS v1.0 codebase (6 crates, 7,802 LOC) -- direct analysis
- VELOS v2 architecture documents (docs/architect/00-07) -- authoritative project spec
- crates.io version verification for all Rust dependencies
- npm package verification for all frontend dependencies
- [LPSim multi-GPU traffic simulation (UC Berkeley, 2024)](https://arxiv.org/html/2406.08496) -- peer-reviewed
- [CCH Survey (Feb 2025)](https://arxiv.org/abs/2502.10519) -- comprehensive academic survey
- [FHWA Traffic Analysis Toolbox](https://ops.fhwa.dot.gov/trafficanalysistools/tat_vol3/sect6.htm) -- official US DOT
- [deck.gl performance best practices](https://deck.gl/docs/developer-guide/performance) -- official docs

### Secondary (MEDIUM-HIGH confidence)
- [wgpu multi-adapter limitations](https://wgpu.rs/) -- official docs but multi-adapter is under-documented
- [RoutingKit CCH documentation](https://github.com/RoutingKit/RoutingKit/blob/master/doc/CustomizableContractionHierarchy.md) -- reference implementation docs
- [Hybrid meso-micro simulation (Burghout, KTH)](https://www.diva-portal.org/smash/get/diva2:14700/FULLTEXT01.pdf) -- academic
- [Redis pub/sub WebSocket scaling (Ably)](https://ably.com/blog/scaling-pub-sub-with-websockets-and-redis) -- industry blog
- [NVIDIA Container Toolkit docs](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/latest/docker-specialized.html) -- official

### Tertiary (MEDIUM confidence)
- Fixed-point performance penalty (40-80%) -- derived from formula analysis, not empirical measurement
- deck.gl 280K point binary streaming -- limited community examples at this scale
- wgpu in Docker discrete GPU issues (gfx-rs/wgpu#2123) -- GitHub issue, may be resolved in wgpu 28

---
*Research completed: 2026-03-07*
*Ready for roadmap: yes*
