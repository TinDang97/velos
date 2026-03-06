# Requirements: VELOS

**Defined:** 2026-03-06
**Core Value:** Motorbikes move realistically through traffic using continuous sublane positioning -- not forced into discrete lanes like Western traffic models

## v1 Requirements

Requirements for initial release. Each maps to roadmap phases.

### GPU Compute & Foundation

- [ ] **GPU-01**: GPU compute pipeline dispatches agent position/velocity updates each timestep via wgpu/Metal compute shaders using simple parallel dispatch
- [ ] **GPU-02**: f64 arithmetic on CPU, f32 in WGSL shaders. No fixed-point types for POC
- [ ] **GPU-03**: hecs ECS stores agent state as components, projected to SoA GPU buffers each frame via queue.write_buffer() with entity-to-GPU index mapping
- [ ] **GPU-04**: CFL numerical stability check validates dt * max_speed < cell_size before each simulation step

### Rendering

- [ ] **REN-01**: winit native macOS window hosts wgpu render surface with compute and render sharing the same device
- [ ] **REN-02**: GPU-instanced 2D renderer draws styled agent shapes (triangles for motorbikes, rectangles for cars, dots for pedestrians) with direction arrows
- [ ] **REN-03**: Zoom/pan camera controls, visible road lanes, intersection areas marked
- [ ] **REN-04**: One instanced draw call per vehicle type for rendering performance

### Vehicle Models

- [ ] **VEH-01**: IDM car-following model adjusts each agent's speed based on gap to leader with ballistic stopping guard preventing negative velocities
- [ ] **VEH-02**: MOBIL lane-change model evaluates lane-change benefit vs politeness threshold (0.3 for HCMC) for car agents
- [ ] **VEH-03**: Motorbike sublane model uses continuous lateral position enabling filtering between cars, red-light clustering, and swarm behavior
- [ ] **VEH-04**: Pedestrian basic social force model (repulsion from other agents + attraction to destination), including jaywalking probability (0.3 for HCMC)

### Road Network

- [ ] **NET-01**: OSM importer parses OpenStreetMap PBF data for a small HCMC area into a directed road graph with lane counts, speed limits, and one-way rules
- [ ] **NET-02**: rstar R-tree spatial index enables fast neighbor queries (all agents within X meters) for car-following, lane-change, and motorbike gap detection
- [ ] **NET-03**: Fixed-time traffic signal controller manages green/red/amber phases per intersection approach with configurable timing
- [ ] **NET-04**: Edge-local to world coordinate transform system for rendering

### Routing

- [ ] **RTE-01**: A* pathfinding on petgraph provides shortest-path queries for agent route assignment

### Demand

- [ ] **DEM-01**: OD matrix loader reads origin-destination trip tables defining volumes between traffic zones
- [ ] **DEM-02**: Time-of-day profiles shape demand across AM peak (7-9), PM peak (17-19), off-peak, and weekend patterns
- [ ] **DEM-03**: Agent spawner generates agents from OD+ToD data, assigns vehicle type (80% motorbike, 15% car, 5% pedestrian), and injects into network at origins

### Gridlock Detection

- [ ] **GRID-01**: Gridlock detection system identifies and resolves circular waiting at intersections (speed=0 for >300s, configurable resolution: teleport/reroute/signal override)

### Application UI

- [ ] **APP-01**: egui UI controls invoke simulation engine methods directly (start, stop, pause, speed adjustment, reset) via in-process function calls
- [ ] **APP-02**: egui dashboard panels display simulation controls, real-time metrics, and agent statistics

### Metrics & Performance

- [ ] **PERF-01**: Frame time benchmark measures GPU dispatch + buffer readback duration per simulation step
- [ ] **PERF-02**: Agent throughput metric tracks agents processed per second and GPU utilization percentage

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

### Fixed-Point & Determinism

- **FP-01**: Fixed-point arithmetic (Q16.16 position, Q12.20 speed, Q8.8 lateral) for cross-GPU determinism at 280K scale
- **FP-02**: Emulated i64 in WGSL for fixed-point multiply/divide
- **FP-03**: Golden test vectors for CPU-GPU parity verification

### Advanced GPU Dispatch

- **ADV-01**: Per-lane wave-front (Gauss-Seidel) dispatch for convergence at scale
- **ADV-02**: Per-lane leader index computation with dual-leader tracking during lane-change transitions
- **ADV-03**: PCG deterministic pseudo-random hash in WGSL

### Advanced Routing & Prediction

- **ADV-04**: Custom CCH pathfinding with dynamic weight customization (~3ms update target)
- **ADV-05**: Dynamic rerouting when travel times change significantly
- **ADV-06**: In-process prediction ensemble (BPR + ETS + historical)

### Meso-Micro Hybrid

- **ADV-07**: Mesoscopic queue model for distant network areas
- **ADV-08**: Graduated buffer zone (100m) transitions between meso and micro models

### Additional Vehicle Types

- **ADV-09**: Bicycle agents with sublane behavior (rightmost position, no filtering, v0=15km/h)
- **ADV-10**: Pedestrian adaptive GPU workgroups based on density

### API & External Access

- **API-01**: gRPC server (tonic) exposes simulation control and data streaming endpoints
- **API-02**: REST server (axum) provides HTTP endpoints for dashboard and external tool integration

### Calibration & Validation

- **CAL-01**: GEH statistic calculation compares simulated vs observed link volumes (target: GEH < 5 for 85%+ links)
- **CAL-02**: Bayesian optimization (argmin) auto-tunes IDM/MOBIL parameters against field data

### Data Export

- **EXP-01**: FCD (Floating Car Data) export writes agent trajectories to Parquet/CSV
- **EXP-02**: GeoJSON export of road network and agent positions for GIS tools
- **EXP-03**: Link/intersection MOE statistics (travel time, delay, queue length, LOS)

### Visualization

- **VIZ-01**: deck.gl web dashboard for remote/multi-user visualization
- **VIZ-02**: Checkpoint/restart saves simulation state to Parquet snapshots

### Scaling

- **SCALE-01**: Multi-GPU partitioning distributes agents across 2+ GPUs
- **SCALE-02**: Full 5-district HCMC coverage (Districts 1, 3, 5, 10, Binh Thanh)
- **SCALE-03**: Scale to 280K concurrent agents

## Out of Scope

| Feature | Reason |
|---------|--------|
| Wiedemann 99 car-following | 10 calibration params requiring PTV-calibrated datasets that don't exist for HCMC |
| SUMO TraCI compatibility | Maintaining moving-target API compatibility is ongoing burden; conflicts with GPU-first design |
| Activity-based demand (MATSim-style) | Requires hundreds of iterations to converge; conflicts with real-time interactive model |
| 3D visualization (CesiumJS/Unreal) | Consumes GPU budget needed for simulation; no CityGML dataset for HCMC |
| Connected/Autonomous Vehicle models | HCMC has negligible AV presence; diverts from motorbike differentiator |
| Multi-node distributed simulation | 280K agents fit on single node with 2-4 GPUs; premature complexity |
| Plugin/extension system | Creates backward compatibility obligations during active development |
| Real-time sensor data ingestion | Requires streaming infrastructure orthogonal to core simulation |
| OAuth/authentication | Single-user desktop app |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| GPU-01 | Phase 1 | Pending |
| GPU-02 | Phase 1 | Pending |
| GPU-03 | Phase 1 | Pending |
| GPU-04 | Phase 1 | Pending |
| REN-01 | Phase 1 | Pending |
| REN-02 | Phase 1 | Pending |
| REN-03 | Phase 1 | Pending |
| REN-04 | Phase 1 | Pending |
| PERF-01 | Phase 1 | Pending |
| PERF-02 | Phase 1 | Pending |
| VEH-01 | Phase 2 | Pending |
| VEH-02 | Phase 2 | Pending |
| NET-01 | Phase 2 | Pending |
| NET-02 | Phase 2 | Pending |
| NET-03 | Phase 2 | Pending |
| NET-04 | Phase 2 | Pending |
| RTE-01 | Phase 2 | Pending |
| DEM-01 | Phase 2 | Pending |
| DEM-02 | Phase 2 | Pending |
| DEM-03 | Phase 2 | Pending |
| GRID-01 | Phase 2 | Pending |
| APP-01 | Phase 2 | Pending |
| APP-02 | Phase 2 | Pending |
| VEH-03 | Phase 3 | Pending |
| VEH-04 | Phase 3 | Pending |

**Coverage:**
- v1 requirements: 25 total
- Mapped to phases: 25
- Unmapped: 0

---
*Requirements defined: 2026-03-06*
*Last updated: 2026-03-06 after project simplification (30 -> 25 requirements, 5 -> 3 phases)*
