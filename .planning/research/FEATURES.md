# Feature Research

**Domain:** GPU-accelerated traffic microsimulation digital twin platform (v1.1 scale-up)
**Researched:** 2026-03-07
**Confidence:** HIGH (architecture docs are thorough; domain is well-established with SUMO/VISSIM/Aimsun as references; v1.0 POC validated core simulation pipeline)

## Context

v1.0 shipped a desktop POC: 1.5K agents, District 1, egui dashboard, A* routing, CPU-side physics (GPU pipeline tested but not wired). v1.1 is the full v2 architecture: 280K agents across 5 HCMC districts with multi-GPU compute, web visualization, API server, calibration, and deployment infrastructure. This research focuses exclusively on the NEW features needed for v1.1.

## Feature Landscape

### Table Stakes (Users Expect These)

Features that any credible traffic microsimulation digital twin platform must have. These transform the v1.0 desktop demo into a usable platform. Missing any of these means v1.1 is not a viable product.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| **gRPC/REST API for simulation control** | Every modern sim platform exposes programmatic control (SUMO TraCI, VISSIM COM, Aimsun Python SDK). Without an API, the sim is a demo, not a platform. External tools, notebooks, and dashboards all need API access. | MEDIUM | tonic (gRPC) + axum (REST/WebSocket). Protobuf contracts designed in `05-visualization-api.md`. ~20 RPC methods. Depends on: v1.0 simulation engine refactored for server mode. |
| **WebSocket real-time streaming** | Viewers expect live agent positions at 10Hz. SUMO provides TraCI streaming; VISSIM has COM. Web-based streaming via WebSocket is the modern equivalent for browser clients. | MEDIUM | FlatBuffers binary protocol (8 bytes/agent). Spatial tiling (500m cells) for viewport-based subscription. 280K agents = ~32KB/frame for 4 tiles. Depends on: API server, Redis pub/sub. |
| **Parquet checkpoint/restart** | 24-hour simulations crash. Every production sim has save/load (SUMO state save, VISSIM snapshot). Without checkpoint, no batch runs, no crash recovery. | MEDIUM | ECS snapshot to Parquet via arrow-rs. 280K agents = ~15MB compressed (Zstd L3). ~200ms save, ~500ms restore. Rolling window of 10 checkpoints. Depends on: ECS state serialization, Parquet writer. |
| **GEH/RMSE calibration** | Industry standard since FHWA Traffic Analysis Toolbox Vol III. GEH < 5 for 85%+ links is the accepted threshold (confirmed by Florida DOT). Without calibration, model output is an animation, not engineering data. | MEDIUM | GEH statistic implementation + Bayesian optimization via argmin crate. Tunes: OD scaling factors, IDM params per road class, signal timing offsets. Depends on: traffic count data (HCMC DOT, ~50 locations), data export, full agent models running. |
| **Data export (FCD, Parquet, CSV, GeoJSON)** | Researchers analyze results in Python/R/QGIS. Every sim exports data. SUMO exports FCD XML, VISSIM exports CSV. Parquet for big data analytics workflows. | LOW | arrow-rs for Parquet, serde for CSV/GeoJSON, quick-xml for SUMO FCD compatibility. Straightforward once simulation state is accessible. Depends on: simulation producing per-step output. |
| **5-district HCMC road network** | The POC scope. Districts 1, 3, 5, 10, Binh Thanh = ~25K edges, ~15K junctions, ~50km road network. Must be fully connected and clean. | MEDIUM | Extends v1.0 District 1 OSM import. Adds: network cleaning (merge short edges <5m, remove disconnected components), lane count inference by road class, HCMC-specific rules (one-way streets, U-turn points, motorbike-only lanes). Depends on: v1.0 OSM import pipeline. |
| **Time-of-day demand profiles (extended)** | v1.0 has basic ToD. v1.1 needs HCMC-specific weekday/weekend/holiday profiles across 5 districts with proper AM/PM peak shapes. 280K peak agents. | LOW | Extends v1.0 OD/ToD system. Add DemandEvent support for spikes. Gravity model fallback if GPS probe data unavailable. Depends on: v1.0 demand generation. |
| **Bus dwell time modeling** | Buses are 4% of HCMC agents (10K) but disproportionately affect traffic flow by blocking lanes during dwell (HCMC has few dedicated bus bays). Every multi-modal sim handles bus stops. | LOW | Empirical dwell time = 5s fixed + 0.5s/boarding + 0.67s/alighting, capped at 60s. HCMC GTFS available for 130 routes. Depends on: GTFS import, agent type system from v1.0. |
| **Bicycle agents** | 7% of POC agents (20K). Bicycles share road space. Missing bicycle agents creates unrealistic density distribution. SUMO/VISSIM both model bicycles. | LOW | Sublane model (rightmost, no filtering), IDM with v0=15km/h. Similar to motorbike but simpler behavior. Depends on: v1.0 agent type system, sublane infrastructure. |
| **Docker Compose deployment** | Reproducibility is non-negotiable for scientific software. "Works on my machine" is unacceptable. Docker Compose is the minimal viable deployment strategy. | LOW | 7 services: sim (GPU), api, viz, redis, tiles (nginx), prometheus, grafana. docker-compose.yml already specified in `06-infrastructure.md`. Depends on: all service binaries buildable. |
| **Prometheus/Grafana monitoring** | At 280K agents on multi-GPU, you need operational visibility: frame time, GPU utilization, VRAM, gridlock events, boundary transfers. Blind operation at scale is reckless. | LOW | Standard Prometheus exposition + pre-built Grafana dashboards. Key metrics: frame_time_ms (histogram), agent_count (gauge), gpu_utilization, gridlock_events, reroute_count. Depends on: instrumented simulation loop. |
| **PMTiles static map tiles** | Web visualization needs base maps. PMTiles + Nginx = zero-ops tile serving. Eliminates Martin/PostGIS/Nominatim stack (6+ services down to 0 additional). | LOW | One-time tile generation: OSM PBF -> tilemaker -> mbtiles -> pmtiles. Nginx serves static files. MapLibre GL JS consumes via pmtiles:// protocol. Depends on: nothing (independent pipeline). |

### Differentiators (Competitive Advantage)

Features that set VELOS v1.1 apart from all existing traffic simulation platforms. These are where the project competes.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **Multi-GPU wave-front dispatch** | No open-source traffic sim does multi-GPU microscopic simulation. LPSim (UC Berkeley, 2024) is the only published multi-GPU traffic sim and it's mesoscopic only. VELOS doing per-lane Gauss-Seidel wave-front on multiple GPUs with 280K micro agents is genuinely novel. | HIGH | METIS graph partitioning (k-way, k=num_gpus), boundary agent protocol (outbox/inbox staging buffers, ~64KB/step transfer), per-lane wave-front dispatch (50K workgroups). Eliminates v1.0 EVEN/ODD dispatch which has no convergence proof. Depends on: v1.0 GPU pipeline, network graph partitioning. |
| **Fixed-point arithmetic for cross-GPU determinism** | SUMO is deterministic (CPU, single-thread). GPU sims are typically non-deterministic due to IEEE 754 variance across GPU vendors. Bitwise-identical results across AMD/NVIDIA/Intel is a research-grade feature no competitor offers. | HIGH | Q16.16 position (0.015mm resolution), Q12.20 speed, Q8.8 lateral. Manual 64-bit emulation in WGSL (no native i64). ~20% GPU perf cost. Fallback: float32 with @invariant if perf unacceptable. Depends on: wave-front dispatch (sequential within-lane makes FP feasible). |
| **CCH dynamic pathfinding (3ms weight update)** | Standard CH requires 30s full rebuild on weight change. CCH separates topology from weights: 30s initial ordering (once), then 3ms customization for 25K edges whenever prediction updates. 0.02ms per query (25x faster than A*). No open-source sim has CCH -- SUMO uses Dijkstra/A*/CH, all require full rebuild for dynamic weights. | HIGH | Custom CCH implementation in Rust. No production-ready CCH crate exists. Must implement: node ordering, shortcut topology construction, bottom-up weight customization, bidirectional Dijkstra on CCH. 500 queries/step in 0.7ms with rayon (16 cores). Depends on: 5-district network graph, prediction overlay for weight updates. |
| **In-process prediction ensemble** | Competitors either have no prediction (SUMO) or require external Python/ML sidecar (latency, ops complexity). VELOS runs BPR + ETS + historical prediction in-process with ArcSwap for zero-copy, lock-free overlay swap. Predictions are never stale. | MEDIUM | Three models: BPR physics extrapolation (w=0.40), exponential smoothing correction (w=0.35), historical pattern match (w=0.25). Runs every 60 sim-seconds on tokio::spawn. 25K edges x 3 models = ~0.1ms. Depends on: simulation state snapshots, historical calibration data. |
| **Motorbike sublane at GPU scale (200K)** | v1.0 proved the continuous lateral model at 1.5K agents. v1.1 scales to 200K motorbikes on GPU. No other sim handles 200K motorbikes with continuous lateral positioning and swarm behavior. This remains the core differentiator. | MEDIUM | Port v1.0 CPU sublane model to GPU compute shaders with fixed-point lateral position (FixedQ8_8). Gap-based filtering, swarm clustering at signals. Depends on: v1.0 sublane model, GPU wave-front dispatch. |
| **Pedestrian adaptive GPU workgroups** | Standard GPU spatial hash wastes workgroups on empty cells. Prefix-sum compaction dispatches only non-empty cells. 3-8x speedup for non-uniform pedestrian density (crosswalk rush hour vs. empty sidewalks). Novel GPU optimization. | MEDIUM | 4-phase GPU pipeline: count per cell (atomic), prefix-sum scan, scatter into compacted array, social force compute. Variable cell sizes by density zone (2m crosswalk, 5m sidewalk, 10m park). Depends on: v1.0 pedestrian model, GPU compute pipeline. |
| **deck.gl 2D + CesiumJS 3D web visualization** | All competitors are desktop-first (SUMO-GUI, VISSIM desktop, Aimsun Next). Web-based visualization with deck.gl handles 280K points at 60FPS in any browser. Multi-viewer without client install. CesiumJS for 3D stakeholder demos. | MEDIUM | React + TypeScript dashboard. deck.gl layers: ScatterplotLayer (vehicles), HeatmapLayer (density), PathLayer (bus routes), IconLayer (signal states). CesiumJS optional (OSM building extrusions). Depends on: WebSocket streaming, API server, PMTiles, FlatBuffers protocol. |
| **Meso-micro hybrid with graduated buffer zones** | Aimsun has meso-micro hybrid but boundary discontinuities cause phantom congestion (known issue in literature). VELOS's 100m buffer zone with velocity-matching insertion and linear IDM parameter interpolation (T: 2x -> 1x normal, s0: 1.5x -> 1x normal) eliminates boundary artifacts. Active research area with no solved standard approach. | HIGH | Queue-based meso model (O(1)/edge), 100m velocity interpolation buffer, safe insertion protocol (hold vehicle in meso queue if micro is full), IDM parameter relaxation. Depends on: v1.0 micro models working at scale, network zone classification. |
| **Scenario DSL + batch runner + MOE comparison** | SUMO has NETEDIT but no declarative scenario DSL. VISSIM requires COM scripting. A TOML/YAML-based scenario definition with automated parallel batch execution and MOE comparison tables is a significant developer experience improvement. | MEDIUM | Scenario DSL defines: network mutations (block edge, change signal), demand variations (multipliers, events), parameter overrides. Batch runner: parallel execution with different seeds. MOE comparison: throughput, mean delay, travel time index, queue length, LOS distribution. Depends on: checkpoint/restart, calibrated baseline, data export. |
| **HBEFA emissions modeling** | HBEFA 5.1 (Oct 2025) is the European standard for road transport emission factors. SUMO integrates via external tool chain. Having HBEFA natively in Rust means per-agent per-step emissions output (CO2, NOx, PM) with zero pipeline overhead. Valuable for policy evaluation (e.g., motorbike restriction zones). | LOW | Lookup table: (vehicle_type, speed, road_gradient, traffic_situation) -> g/km. HBEFA factors are published data. Includes motorcycle emission factors (critical for HCMC). Depends on: agent speed/type data per step, edge gradient data. |

### Anti-Features (Commonly Requested, Often Problematic)

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| **Wiedemann 99 car-following** | VISSIM users expect it. 10-parameter model claims higher fidelity. | W99 requires PTV-calibrated datasets unavailable for HCMC. Uncalibrated W99 produces worse results than calibrated IDM. Including it creates false expectations. | IDM with 5 physically interpretable parameters. Bayesian optimization tunes within HCMC-specific ranges. |
| **SUMO TraCI compatibility** | Reuse existing SUMO scripts. Large ecosystem. | TraCI is synchronous, single-threaded, fundamentally incompatible with GPU-parallel execution. API surface is ~200 commands. Maintaining compatibility with a moving target is massive ongoing burden. | Native gRPC API. Provide thin Python client (`velos-py`) mirroring TraCI patterns for migration. |
| **Multi-node distributed simulation** | "Scale to 2M agents across a cluster." | 280K agents fit on single-node 2-4 GPUs. Multi-node adds: network latency (100x PCIe), distributed state sync, clock sync, ghost zone complexity. All for a target that doesn't need it. | Single-node multi-GPU. PCIe agent transfer at 64KB/step is negligible. Multi-node deferred to v3 for 2M+ agents. |
| **Real-time sensor data fusion** | Digital twin "should" sync with live traffic. | Requires: data partnerships (HCMC DOT), streaming pipeline (Kafka), data quality filters, clock synchronization. Massive scope creep for marginal POC value. | Offline calibration with historical counts. Replay recorded sensor data. Add live fusion in v3 after model validated. |
| **ML/DL prediction (PyTorch/TensorFlow)** | Deep learning predicts traffic better. | Python sidecar = Arrow IPC latency, ops complexity, version conflicts. ML models need training data that doesn't exist for HCMC yet. | Rust-native BPR+ETS+historical ensemble. If ML needed later, add as gRPC prediction service (not shared-memory IPC). |
| **CityGML 3D buildings** | Photorealistic 3D city visualization. | No CityGML dataset exists for HCMC. Creating one is months of work. | OSM building footprint extrusions in CesiumJS. Heights from `building:levels` tag. 80% visual impact at 1% effort. |
| **Autonomous vehicle models** | "Every new sim should support AVs." | Negligible AV presence in HCMC. Adds agent interaction complexity with zero benefit for HCMC POC. | Defer to v3. Add when HCMC has measurable AV penetration. |
| **Full passenger flow model** | Multi-commodity passenger OD, overcrowding, transfers. | Massive modeling complexity (demand, fare, capacity, transfer network). Bus dwell time model captures 80% of traffic impact at 5% effort. | Simplified bus dwell time (fixed + per-passenger). Defer full passenger flow to transit optimization phase. |
| **Plugin/extension system** | "Let users write custom models." | Plugin APIs create backward compatibility obligations. Premature API stabilization prevents necessary architectural changes. | Provide source code. Users fork and modify. Stabilize APIs after v2 architecture settles. |

## Feature Dependencies

```
[Multi-GPU Wave-Front Dispatch]
    |-- requires --> [v1.0 GPU Compute Pipeline]
    |-- requires --> [Network Graph Partitioning (METIS)]
    |-- requires --> [Fixed-Point Arithmetic]
    |-- enables  --> [280K Agent Scale]

[Fixed-Point Arithmetic]
    |-- requires --> [Wave-Front Dispatch] (sequential in-lane makes FP feasible)
    |-- enables  --> [Cross-GPU Determinism]
    |-- enables  --> [Deterministic Replay for Scenario Comparison]

[CCH Dynamic Pathfinding]
    |-- requires --> [5-District Network Graph]
    |-- requires --> [Prediction Ensemble] (provides dynamic edge weights)
    |-- replaces --> [v1.0 A* on petgraph]
    |-- enables  --> [Dynamic Agent Rerouting (500/step)]

[Prediction Ensemble (BPR+ETS+Historical)]
    |-- requires --> [Simulation State Snapshots]
    |-- requires --> [Historical Calibration Data]
    |-- enhances --> [CCH Routing] (weight updates every 60 sim-seconds)

[Meso-Micro Hybrid]
    |-- requires --> [v1.0 Micro Models (IDM/MOBIL/Sublane) at scale]
    |-- requires --> [Network Zone Classification]
    |-- requires --> [CCH Routing] (meso zones need fast pathfinding)
    |-- conflicts --> [Full-micro-only at 280K] (may not be needed if GPUs handle full micro)

[gRPC/REST API]
    |-- requires --> [v1.0 Simulation Engine (refactored for server mode)]
    |-- enables  --> [WebSocket Streaming]
    |-- enables  --> [Scenario DSL/Batch Runner]
    |-- enables  --> [deck.gl Visualization]
    |-- enables  --> [External Tool Integration]

[WebSocket Streaming + Redis Pub/Sub]
    |-- requires --> [gRPC/REST API]
    |-- requires --> [Redis service]
    |-- enables  --> [deck.gl Real-Time Visualization]
    |-- enables  --> [Multi-Viewer Scaling (100+ clients)]

[deck.gl Web Visualization]
    |-- requires --> [WebSocket Streaming]
    |-- requires --> [PMTiles Map Tiles]
    |-- requires --> [FlatBuffers Binary Protocol]

[CesiumJS 3D Visualization]
    |-- requires --> [deck.gl infrastructure] (shares WebSocket, API, tiles)
    |-- optional --> [Terrain tiles from SRTM DEM]

[GEH/RMSE Calibration]
    |-- requires --> [5-District Network + All Agent Models Running]
    |-- requires --> [Traffic Count Data (HCMC DOT, ~50 locations)]
    |-- requires --> [Data Export (simulated counts for comparison)]
    |-- enables  --> [Validated Model -- output is engineering data, not animation]

[Scenario DSL + Batch Runner]
    |-- requires --> [Checkpoint/Restart (for deterministic replay)]
    |-- requires --> [Data Export (for MOE computation)]
    |-- requires --> [GEH Calibration (baseline must be validated first)]
    |-- enables  --> [MOE Comparison Tables]

[Parquet Checkpoint/Restart]
    |-- requires --> [ECS State Serialization (arrow-rs)]
    |-- enables  --> [Long-Running Simulations (24h+)]
    |-- enables  --> [Scenario Batch Runs]
    |-- enables  --> [Crash Recovery]

[Docker Compose Deployment]
    |-- requires --> [All Service Binaries (sim, api, viz)]
    |-- enables  --> [Reproducible Deployment]
    |-- enables  --> [Monitoring Stack (Prometheus/Grafana)]

[Pedestrian Adaptive Workgroups]
    |-- requires --> [v1.0 Social Force Model]
    |-- requires --> [GPU Prefix-Sum Compaction Shader]
    |-- enhances --> [Pedestrian Performance at 20K agents]

[Bus Dwell + Bicycle Agents]
    |-- requires --> [v1.0 Agent Type System]
    |-- requires --> [GTFS Import (130 HCMC bus routes)]
    |-- enhances --> [Multi-Modal Realism]

[HBEFA Emissions]
    |-- requires --> [Per-Step Agent Speed/Type Data]
    |-- requires --> [Edge Gradient Data]
    |-- enhances --> [Policy Evaluation (motorbike restriction zones)]
```

### Dependency Notes

- **Multi-GPU + Fixed-Point are inseparable:** Fixed-point ensures cross-GPU determinism. Without it, multi-GPU results diverge per-run, making calibration impossible. Ship together.
- **CCH needs Prediction to justify its complexity:** CCH's value is 3ms dynamic weight updates. Without prediction, CCH is just "faster A*" -- still useful but doesn't justify the HIGH implementation cost. Build CCH after prediction overlay exists.
- **Meso-micro may not be needed:** At 280K agents on 2-4 GPUs with ~8ms frame time and 92ms headroom (per `01-simulation-engine.md`), full-micro everywhere is likely feasible. Meso-micro is insurance. Defer unless performance proves otherwise.
- **Scenario DSL requires calibrated baseline:** Running "what-if" scenarios against an uncalibrated model produces meaningless MOE comparisons. Calibration must precede scenario work.
- **deck.gl sits atop a deep infrastructure stack:** Visualization requires WebSocket + Redis + FlatBuffers + API server + PMTiles. The frontend is the easy part; the backend plumbing is the work.
- **API server is the platform unlock:** gRPC/REST transforms a desktop app into a platform. Everything external (visualization, notebooks, scenarios, monitoring) flows through the API. This is the single most important architectural change from v1.0.

## MVP Definition

### Launch With (v1.1 Core Platform)

Minimum viable digital twin -- demonstrates 280K-agent simulation with web visualization and programmatic control.

- [ ] **Multi-GPU wave-front dispatch + fixed-point** -- Performance unlock for 280K agents. These two ship together.
- [ ] **5-district HCMC network** -- Extends v1.0 District 1. Includes network cleaning, lane inference, HCMC-specific rules.
- [ ] **gRPC/REST API** -- Platform transformation. Non-negotiable for any external integration.
- [ ] **WebSocket streaming + Redis pub/sub** -- Enables web visualization and multi-viewer support.
- [ ] **deck.gl 2D visualization** -- Primary web interface. ScatterplotLayer + HeatmapLayer + signal states.
- [ ] **Parquet checkpoint/restart** -- Crash recovery and batch run foundation.
- [ ] **Docker Compose deployment** -- Reproducible 7-service stack.
- [ ] **Prometheus/Grafana monitoring** -- Operational visibility at scale.
- [ ] **Data export (FCD, Parquet, CSV)** -- Minimum output for analysis.
- [ ] **PMTiles map tiles** -- Base map for web visualization.
- [ ] **Bus dwell + bicycle agents** -- Complete the multi-modal agent roster.

### Add After Core Validated (v1.1 Enhancement)

Features to add once the multi-GPU simulation and web platform are proven.

- [ ] **CCH dynamic pathfinding** -- Replace v1.0 A* when routing becomes bottleneck at 500 reroutes/step.
- [ ] **Prediction ensemble (BPR+ETS+historical)** -- Add once CCH is live for dynamic weight updates.
- [ ] **GEH/RMSE calibration with Bayesian optimization** -- Add when traffic count data from HCMC DOT is available and all agent models are running.
- [ ] **Pedestrian adaptive GPU workgroups** -- Upgrade when 20K pedestrians cause perf issues with basic spatial hash.
- [ ] **Scenario DSL + batch runner + MOE comparison** -- Add after calibration validates the base model.
- [ ] **HBEFA emissions** -- Add when policy evaluation use cases emerge.
- [ ] **GeoJSON export** -- Add when GIS users request it.

### Future Consideration (v2+)

- [ ] **Meso-micro hybrid** -- Defer unless full-micro hits performance wall at 280K on 2-4 GPUs. If frame time stays under 15ms, meso is unnecessary complexity.
- [ ] **CesiumJS 3D** -- Presentation-only. Add for stakeholder demos once deck.gl 2D proves the platform.
- [ ] **Actuated signal control** -- HCMC is overwhelmingly fixed-time. Minimal realism gain for HCMC specifically.

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| Multi-GPU wave-front dispatch | HIGH | HIGH | P1 |
| Fixed-point arithmetic | HIGH | HIGH | P1 |
| 5-district HCMC network (OSM) | HIGH | MEDIUM | P1 |
| gRPC/REST API | HIGH | MEDIUM | P1 |
| WebSocket + Redis pub/sub | HIGH | MEDIUM | P1 |
| deck.gl 2D visualization | HIGH | MEDIUM | P1 |
| Parquet checkpoint/restart | HIGH | MEDIUM | P1 |
| Data export (FCD, Parquet, CSV) | HIGH | LOW | P1 |
| Docker Compose deployment | MEDIUM | LOW | P1 |
| Prometheus/Grafana monitoring | MEDIUM | LOW | P1 |
| PMTiles map tiles | MEDIUM | LOW | P1 |
| Bus dwell + bicycle agents | MEDIUM | LOW | P1 |
| CCH dynamic pathfinding | HIGH | HIGH | P2 |
| Prediction ensemble | HIGH | MEDIUM | P2 |
| GEH/RMSE calibration | HIGH | MEDIUM | P2 |
| Pedestrian adaptive workgroups | MEDIUM | MEDIUM | P2 |
| Scenario DSL + batch runner | MEDIUM | MEDIUM | P2 |
| HBEFA emissions | LOW | LOW | P2 |
| Meso-micro hybrid | MEDIUM | HIGH | P3 |
| CesiumJS 3D visualization | LOW | MEDIUM | P3 |
| Actuated signal control | LOW | LOW | P3 |

**Priority key:**
- P1: Must have for v1.1 launch -- core digital twin platform with multi-GPU and web visualization
- P2: Should have -- enables validation, prediction, and scenario analysis
- P3: Nice to have -- defer unless specific need arises

## Competitor Feature Analysis

| Feature | SUMO | VISSIM | Aimsun | LPSim (2024) | VELOS v1.1 |
|---------|------|--------|--------|-------------|-------------|
| **Car-following** | Krauss (default), IDM | Wiedemann 74/99 | Gipps, IDM | Simplified IDM | IDM (HCMC-calibrated) |
| **Lane-change** | LC2013, SL2015 | Proprietary | Proprietary | N/A (meso) | MOBIL + sublane filtering |
| **Motorbike sublane** | SL2015 (discrete, 0.8m slots) | External Driver Model hack | No native support | None | Continuous lateral (FixedQ8_8) |
| **GPU compute** | None (CPU only) | None (CPU only) | None (CPU only) | CUDA multi-GPU (meso) | wgpu multi-GPU (micro) |
| **Scale (real-time)** | ~50-100K (CPU bound) | ~20-50K (CPU bound) | ~100K (meso) | 2.8M (meso, batch) | 280K (micro, real-time) |
| **Pathfinding** | Dijkstra/A*/CH | Proprietary DTA | Proprietary | BFS-like | CCH (3ms dynamic update) |
| **Prediction** | None built-in | Limited | Aimsun Predict (ML) | None | In-process BPR+ETS+historical |
| **Meso-micro hybrid** | Separate mode | No | Yes (flagship) | Meso only | Graduated buffer (P3) |
| **Web visualization** | SUMO-GUI (desktop) | Desktop only | Desktop only | None | deck.gl 2D + CesiumJS 3D |
| **API** | TraCI (TCP socket) | COM interface | Python/C++ SDK | None | gRPC + REST + WebSocket |
| **Calibration** | Manual + external tools | Built-in optimizer | Built-in optimizer | None | GEH + Bayesian (argmin) |
| **Emissions** | HBEFA via external tools | EnViVer integration | None built-in | None | HBEFA native (Rust) |
| **Scenario comparison** | XML scenarios, manual | COM scripting | Built-in | None | DSL + batch runner + MOE |
| **Checkpoint** | State save (XML) | Snapshot | Snapshot | None | Parquet (compressed, rolling) |
| **Determinism** | Yes (CPU, single-thread) | Stochastic (seed) | Stochastic (seed) | Not specified | Yes (fixed-point, cross-GPU) |
| **License** | EPL-2.0 (open) | Commercial | Commercial | Research | Proprietary (TBD) |

**Key competitive positions for v1.1:**
1. **Only GPU-accelerated microscopic traffic sim.** LPSim is GPU but mesoscopic only.
2. **Only sim with native motorbike sublane at 200K+ scale.** SUMO's SL2015 is discrete slots, not continuous.
3. **Only sim with CCH dynamic routing.** All competitors use Dijkstra/A*/CH requiring full rebuild for dynamic weights.
4. **Web-native visualization platform.** All competitors are desktop-first.
5. **Cross-GPU deterministic simulation.** No competitor offers bitwise-identical results across GPU vendors.

## Sources

- [LPSim multi-GPU traffic simulation (UC Berkeley, 2024)](https://arxiv.org/html/2406.08496) - HIGH confidence (peer-reviewed)
- [CCH Survey (Feb 2025)](https://arxiv.org/abs/2502.10519) - HIGH confidence (comprehensive academic survey)
- [FHWA Traffic Analysis Toolbox - MOE definitions](https://ops.fhwa.dot.gov/publications/fhwahop08054/fhwahop08054.pdf) - HIGH confidence (official US DOT)
- [FHWA Microsimulation Guidelines - Calibration](https://ops.fhwa.dot.gov/trafficanalysistools/tat_vol3/sect6.htm) - HIGH confidence (official)
- [Aimsun Hybrid Meso-Micro documentation](https://docs.aimsun.com/next/22.0.2/UsersManual/HybridSimulator.html) - HIGH confidence (official docs)
- [HBEFA 5.1 (Oct 2025)](https://www.hbefa.net/) - HIGH confidence (official)
- [deck.gl framework](https://deck.gl/) - HIGH confidence (official)
- [Hybrid meso-micro traffic simulation (Burghout)](https://www.diva-portal.org/smash/get/diva2:14700/FULLTEXT01.pdf) - HIGH confidence (academic)
- [SUMO vs VISSIM comparison](https://thinktransportation.net/traffic-simulations-software-a-comparison-of-sumo-ptv-vissim-aimsun-and-cube/) - MEDIUM confidence
- [Digital twin real-time calibration](https://www.sciencedirect.com/science/article/pii/S1474034622003160) - MEDIUM confidence
- [Traffic simulation case studies review (2025)](https://ietresearch.onlinelibrary.wiley.com/doi/full/10.1049/itr2.70021) - MEDIUM confidence
- VELOS architecture docs (`docs/architect/00-07`) - HIGH confidence (primary source)

---
*Feature research for: GPU-accelerated traffic microsimulation digital twin platform (v1.1 scale-up)*
*Researched: 2026-03-07*
