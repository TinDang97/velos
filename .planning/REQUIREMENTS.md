# Requirements: VELOS

**Defined:** 2026-03-06
**Core Value:** Motorbikes move realistically through traffic using continuous sublane positioning — not forced into discrete lanes like Western traffic models

## v1 Requirements

Requirements for initial release. Each maps to roadmap phases.

### GPU Compute & Foundation

- [ ] **GPU-01**: GPU compute pipeline dispatches agent position/velocity updates each timestep via wgpu/Metal compute shaders using wave-front dispatch pattern
- [ ] **GPU-02**: Fixed-point arithmetic uses Q16.16 for positions, Q12.20 for speeds, Q8.8 for lateral offsets, ensuring bitwise-deterministic results across runs
- [ ] **GPU-03**: hecs ECS stores agent state as components, projected to SoA GPU buffers each frame via queue.write_buffer() with entity-to-GPU index mapping
- [ ] **GPU-04**: CFL numerical stability check validates dt * max_speed < cell_size before each simulation step to prevent agents teleporting

### Vehicle Models

- [ ] **VEH-01**: IDM car-following model adjusts each agent's speed based on gap to leader with ballistic stopping guard preventing negative velocities
- [ ] **VEH-02**: MOBIL lane-change model evaluates lane-change benefit vs politeness threshold (0.3 for HCMC) for car agents
- [ ] **VEH-03**: Motorbike sublane model uses continuous lateral position (FixedQ8_8) enabling filtering between cars, red-light clustering, and swarm behavior
- [ ] **VEH-04**: Pedestrian social force model with adaptive GPU workgroups based on density, including jaywalking probability (0.3 for HCMC)

### Road Network

- [ ] **NET-01**: OSM importer parses OpenStreetMap PBF data for a small HCMC area into a directed road graph with lane counts, speed limits, and one-way rules
- [ ] **NET-02**: rstar R-tree spatial index enables fast neighbor queries (all agents within X meters) for car-following, lane-change, and motorbike gap detection
- [ ] **NET-03**: Fixed-time traffic signal controller manages green/red/amber phases per intersection approach with configurable timing

### Routing & Prediction

- [ ] **RTE-01**: Custom CCH (Customizable Contraction Hierarchies) implementation provides fast shortest-path queries with ~3ms weight update target
- [ ] **RTE-02**: Dynamic CCH weight updates reflect current congestion levels, triggering agent reroutes when travel times change significantly
- [ ] **RTE-03**: In-process prediction ensemble (BPR volume-delay + ETS time series + historical) estimates future travel times in Rust-native code without Python bridge

### Demand

- [ ] **DEM-01**: OD matrix loader reads origin-destination trip tables defining volumes between traffic zones
- [ ] **DEM-02**: Time-of-day profiles shape demand across AM peak (7-9), PM peak (17-19), off-peak, and weekend patterns
- [ ] **DEM-03**: Agent spawner generates agents from OD+ToD data, assigns vehicle type (80% motorbike, 15% car, 5% bus), and injects into network at origins

### Meso-Micro Hybrid

- [ ] **MESO-01**: Mesoscopic queue model simulates distant network areas using simplified link-level flow dynamics
- [ ] **MESO-02**: Graduated buffer zone (100m) transitions agents between meso and micro models with velocity interpolation and IDM parameter relaxation to eliminate phantom waves

### Application

- [ ] **APP-01**: Tauri v2 native macOS window hosts wgpu render surface for simulation visualization alongside React webview for dashboard
- [ ] **APP-02**: wgpu 2D renderer draws top-down view of road network with agents as colored shapes (motorbikes, cars, pedestrians) moving in real-time
- [ ] **APP-03**: Tauri IPC bridge enables simulation control commands (start, stop, pause, speed adjustment, reset) from React frontend to Rust backend
- [ ] **APP-04**: React+TypeScript dashboard (built with Vite) displays simulation controls, real-time metrics, and agent statistics

### Metrics & Performance

- [ ] **PERF-01**: Frame time benchmark measures GPU dispatch + buffer readback duration per simulation step
- [ ] **PERF-02**: Agent throughput metric tracks agents processed per second and GPU utilization percentage

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

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
| GPU-01 | TBD | Pending |
| GPU-02 | TBD | Pending |
| GPU-03 | TBD | Pending |
| GPU-04 | TBD | Pending |
| VEH-01 | TBD | Pending |
| VEH-02 | TBD | Pending |
| VEH-03 | TBD | Pending |
| VEH-04 | TBD | Pending |
| NET-01 | TBD | Pending |
| NET-02 | TBD | Pending |
| NET-03 | TBD | Pending |
| RTE-01 | TBD | Pending |
| RTE-02 | TBD | Pending |
| RTE-03 | TBD | Pending |
| DEM-01 | TBD | Pending |
| DEM-02 | TBD | Pending |
| DEM-03 | TBD | Pending |
| MESO-01 | TBD | Pending |
| MESO-02 | TBD | Pending |
| APP-01 | TBD | Pending |
| APP-02 | TBD | Pending |
| APP-03 | TBD | Pending |
| APP-04 | TBD | Pending |
| PERF-01 | TBD | Pending |
| PERF-02 | TBD | Pending |

**Coverage:**
- v1 requirements: 25 total
- Mapped to phases: 0
- Unmapped: 25

---
*Requirements defined: 2026-03-06*
*Last updated: 2026-03-06 after initial definition*
