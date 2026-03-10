# Phase 17: Detection Ingestion & Demand Calibration - Context

**Gathered:** 2026-03-10
**Status:** Ready for planning

<domain>
## Phase Boundary

External CV services push vehicle/pedestrian detection data into VELOS via gRPC, and the system uses those detections to adjust simulation demand (OD spawn rates). Cameras are registered at runtime with position, FOV, and network mapping. Detection counts are aggregated per class over configurable time windows. The system adjusts OD spawn rates based on observed-vs-simulated count ratios. Python and Rust client libraries connect for integration testing.

Requirements: DET-01, DET-02, DET-03, DET-04, DET-05, DET-06, CAL-01

</domain>

<decisions>
## Implementation Decisions

### gRPC API Contract
- Bidirectional streaming: client streams DetectionEvent batches, server streams acknowledgments with per-batch status
- Minimal DetectionEvent message: camera_id, timestamp, vehicle_class (enum: motorbike/car/bus/truck/bicycle/pedestrian), count, optional speed_kmh
- New `velos-api` crate for gRPC server (tonic) — general-purpose API crate per architecture plan
- Protobuf definitions at proto/velos/v2/ with versioned package path
- tonic-build generates Rust code; Python client uses grpcio-tools for stubs
- gRPC server runs on tokio runtime alongside the winit app

### Camera Registration & Mapping
- Cameras registered via gRPC RegisterCamera RPC at runtime (no config file)
- Camera position stored at exact lat/lon (not snapped to network nodes)
- FOV mapped to road network via geometric cone: camera position + heading + angle, rstar spatial queries determine which edges fall within the FOV cone
- Semi-transparent cone polygon rendered on map showing camera FOV coverage, toggleable via egui checkbox
- Cameras lost on restart unless re-registered by CV client

### Detection Aggregation
- Configurable time windows, default 5 minutes
- Per-class counts: HashMap<VehicleClass, u32> per window per camera
- Speed estimation: averaged per class per window (mean speed + sample count)
- Rolling retention: last 1 hour (12 windows at 5-min default)
- Older windows dropped automatically

### Demand Calibration Feedback
- Multiplicative scaling factor: ratio = observed_count / simulated_count per camera zone
- Applied to OD pairs whose routes pass through camera-covered edges
- Ratio clamped to [0.5, 2.0] to prevent wild demand swings
- Calibration runs every aggregation window (5 minutes)
- Calibrated demand stored as overlay on OdMatrix — HashMap<(Zone, Zone), f32> scaling factors
- Original OdMatrix unchanged; Spawner reads base * overlay
- Similar pattern to PredictionOverlay (ArcSwap lock-free reads)
- Minimal egui dashboard panel: per-camera observed count, simulated count, current ratio, last update time (toggleable)

### Claude's Discretion
- Exact protobuf message field types and naming conventions
- gRPC server port and configuration
- FOV cone rendering precision (triangle vs arc sector approximation)
- Edge coverage algorithm details for FOV-to-edge intersection
- Calibration smoothing or damping strategy within the [0.5, 2.0] clamp
- Simulated count collection method (counting agents passing camera edges)
- egui panel layout and styling

</decisions>

<specifics>
## Specific Ideas

- Calibration overlay follows the same ArcSwap lock-free pattern as PredictionOverlay — spawner reads atomically without blocking the simulation frame
- velos-api crate is the planned general-purpose API crate from the architecture docs — detection service is its first concrete service, future REST/WebSocket endpoints will live here too
- Python client library is critical for integration testing — external CV services (YOLO inference) are Python-based

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `velos-demand/src/od_matrix.rs`: OdMatrix with zone pairs, get_trips/set_trips — calibration overlay wraps this
- `velos-demand/src/spawner.rs`: Spawner with generate_spawns() — needs to read calibration overlay when computing spawn counts
- `velos-demand/src/tod_profile.rs`: TodProfile with hcmc_weekday() — time-of-day modulation independent of calibration
- `velos-predict/src/lib.rs`: PredictionStore with ArcSwap pattern — reuse same lock-free overlay pattern for calibration factors
- `velos-net/src/snap.rs`: snap_to_nearest_edge() + rstar spatial index — reuse for FOV-to-edge spatial queries
- `velos-signal/src/lib.rs`: LoopDetector for count-based actuation — similar counting pattern for detection aggregation
- `velos-gpu/src/sim_render.rs`: 2D rendering pipeline — extend with camera icon + FOV cone overlay
- `velos-gpu/src/camera.rs`: Camera2D for viewport — camera overlay rendering integrates here

### Established Patterns
- TOML config in data/hcmc/ for static configuration (vehicle_params, signal_config) — cameras are runtime-only via gRPC
- ECS components in velos-core — no new ECS components needed (cameras are not simulation entities)
- Per-crate error enums with thiserror — velos-api will define ApiError
- tokio for async I/O (gRPC server), rayon for compute — gRPC server on tokio, simulation on rayon

### Integration Points
- velos-api (new): gRPC server with DetectionService, communicates with SimWorld via channel or shared state
- velos-demand: Spawner reads calibration overlay to adjust spawn rates
- velos-gpu/sim_render: Camera FOV cone rendering as new overlay layer
- velos-gpu/app: Start gRPC server on separate tokio task during app initialization
- egui UI: New "Calibration" panel with per-camera metrics, toggleable camera overlay checkbox

</code_context>

<deferred>
## Deferred Ideas

- CAL-02 (continuous calibration during running simulation from streaming data) — Phase 20
- Built-in YOLO inference (CV-01, CV-02, CV-03) — future milestone
- Detection confidence heatmap overlay (DAN-01) — future milestone
- Cross-camera vehicle re-identification (DAN-02) — future milestone
- Camera config persistence (save registered cameras to TOML for restart) — could be a small follow-up

</deferred>

---

*Phase: 17-detection-ingestion-demand-calibration*
*Context gathered: 2026-03-10*
