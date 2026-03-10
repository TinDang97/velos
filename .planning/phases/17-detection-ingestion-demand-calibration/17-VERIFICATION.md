---
phase: 17-detection-ingestion-demand-calibration
verified: 2026-03-10T09:30:00Z
status: passed
score: 5/5 must-haves verified
re_verification: false
human_verification:
  - test: "Camera FOV cones render on 2D simulation map"
    expected: "Semi-transparent cone polygons visible at camera positions on the map"
    why_human: "Visual rendering cannot be verified programmatically"
  - test: "Python client connects to running gRPC server end-to-end"
    expected: "test_detection_client.py registers camera, streams batches, receives acks"
    why_human: "Requires running simulation and Python client in separate terminals"
  - test: "Calibration panel shows live metrics"
    expected: "egui panel displays per-camera observed/simulated/ratio after Python client pushes detections"
    why_human: "UI rendering and real-time data flow need visual confirmation"
---

# Phase 17: Detection Ingestion & Demand Calibration Verification Report

**Phase Goal:** External CV services can push detection data into VELOS via gRPC, and the system uses those detections to adjust simulation demand
**Verified:** 2026-03-10T09:30:00Z
**Status:** passed
**Re-verification:** No -- initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | A gRPC client can stream vehicle/pedestrian detection events to VELOS and receive acknowledgment per batch | VERIFIED | `DetectionServiceImpl::stream_detections` implements bidirectional streaming with per-batch `DetectionAck` (detection.rs:57-115). 4 integration tests pass including `test_stream_detections` exercising batch streaming. |
| 2 | User can register cameras with position, FOV, and network edge/junction mapping, then see camera positions with FOV coverage areas overlaid on the simulation map | VERIFIED | `CameraRegistry::register` stores camera with FOV-to-edge spatial query (camera.rs:46-77). `build_camera_overlay_vertices` renders FOV cones on map (sim_render.rs:73+). gRPC `register_camera` RPC wired (detection.rs:117-139). Visual rendering needs human confirmation. |
| 3 | Detection counts per class are aggregated over configurable time windows and speed estimation data is accepted per camera | VERIFIED | `DetectionAggregator` with 5-min windows, 1-hr retention, per-class counts, weighted speed averaging (aggregator.rs:48-135). 8 unit tests pass covering windowing, speed averaging, and GC. |
| 4 | System adjusts OD spawn rates based on observed-vs-simulated count ratios, with demand changes reflected in agent spawn behavior | VERIFIED | `CalibrationOverlay` with ArcSwap (calibration.rs:34-77). `compute_calibration_factors` computes EMA-smoothed, clamped [0.5, 2.0] ratios (calibration.rs:144-207). `Spawner::generate_spawns_calibrated` multiplies trip counts by factors (spawner.rs:149-197). `SimWorld::step_calibration` recomputes every 300 sim-seconds (sim_calibration.rs:108-169). Spawner reads overlay in `sim_lifecycle.rs:32-36`. 10 calibration + 1 spawner tests pass. |
| 5 | Python and Rust client libraries can connect to the gRPC service and push detection events for integration testing | VERIFIED | Rust: `grpc_integration.rs` (269 lines, 4 tests all pass) exercises RegisterCamera, ListCameras, StreamDetections, and unknown camera error handling. Python: `detection_client.py` (182 lines) wraps grpcio stubs with `VelosDetectionClient` class. `test_detection_client.py` (255 lines) provides integration test script. |

**Score:** 5/5 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `proto/velos/v2/detection.proto` | gRPC service definition | VERIFIED | 80 lines. DetectionService with 3 RPCs, VehicleClass enum, all message types present. |
| `crates/velos-api/Cargo.toml` | Crate manifest | VERIFIED | tonic, prost, tokio, rstar, arc-swap, velos-demand, velos-net dependencies. |
| `crates/velos-api/build.rs` | Proto compilation | VERIFIED | `tonic_prost_build::configure().compile_protos()` targeting detection.proto. |
| `crates/velos-api/src/lib.rs` | Module declarations and factory | VERIFIED | 67 lines. All 6 modules declared, re-exports, `create_detection_service` factory function. |
| `crates/velos-api/src/error.rs` | ApiError with Status mapping | VERIFIED | 71 lines. 4 error variants, From<ApiError> for tonic::Status, 4 unit tests. |
| `crates/velos-api/src/bridge.rs` | ApiBridge channel types | VERIFIED | 181 lines. ApiCommand enum, ApiBridge with try_recv/drain, bounded 256 capacity, 5 unit tests. |
| `crates/velos-api/src/camera.rs` | Camera, CameraRegistry, edges_in_fov | VERIFIED | 354 lines. FOV cone query with AABB pre-filter and angle normalization, 7 unit tests. |
| `crates/velos-api/src/aggregator.rs` | TimeWindow, DetectionAggregator | VERIFIED | 283 lines. Windowed counts, weighted speed averaging, GC, 8 unit tests. |
| `crates/velos-api/src/detection.rs` | DetectionServiceImpl | VERIFIED | 163 lines. Implements all 3 RPCs: stream_detections (bidirectional), register_camera, list_cameras. |
| `crates/velos-api/src/calibration.rs` | CalibrationOverlay, CalibrationStore | VERIFIED | 344 lines. ArcSwap pattern, EMA alpha=0.3, clamp [0.5, 2.0], 10 unit tests. |
| `crates/velos-demand/src/spawner.rs` | generate_spawns_calibrated | VERIFIED | Method multiplies OD pair trip counts by calibration factors. 1 test passes. |
| `crates/velos-gpu/src/app.rs` | gRPC server startup, egui panel | VERIFIED | tokio::runtime::Runtime on std::thread, Server::builder.add_service, calibration panel with observed/simulated/ratio grid. |
| `crates/velos-gpu/src/sim.rs` | SimWorld with calibration fields | VERIFIED | calibration_store, api_bridge fields present. step_api_commands/step_calibration called in tick paths. |
| `crates/velos-gpu/src/sim_calibration.rs` | API command drain, calibration step | VERIFIED | 170 lines. step_api_commands drains up to 64/frame. step_calibration recomputes every 300s with ECS agent count query. |
| `crates/velos-gpu/src/sim_render.rs` | Camera FOV cone rendering | VERIFIED | build_camera_overlay_vertices function with FOV cone geometry. Toggleable via show_cameras bool. |
| `crates/velos-api/tests/grpc_integration.rs` | Rust integration tests | VERIFIED | 269 lines. 4 tests all pass: register, list, stream, unknown camera. |
| `tools/python/detection_client.py` | Python client SDK | VERIFIED | 182 lines. VelosDetectionClient class wrapping DetectionServiceStub. |
| `tools/python/test_detection_client.py` | Python test script | VERIFIED | 255 lines. Registers camera, streams batches, prints acks. |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| build.rs | detection.proto | tonic_prost_build compile | WIRED | `compile_protos(&["../../proto/velos/v2/detection.proto"])` |
| Cargo.toml | crates/velos-api | workspace members | WIRED | velos-api in workspace members list |
| detection.rs | bridge.rs | cmd_tx.send(ApiCommand::DetectionBatch) | WIRED | Lines 93-100 forward batch to SimWorld |
| camera.rs | velos-net/snap.rs | RTree<EdgeSegment> | WIRED | `edges_in_fov` receives `&RTree<EdgeSegment>` and queries it |
| detection.rs | camera.rs | registry.lock().contains() | WIRED | Lines 76-82 validate camera IDs |
| calibration.rs | spawner.rs | CalibrationOverlay read in generate_spawns_calibrated | WIRED | sim_lifecycle.rs:32-36 reads `calibration_store.current()` and passes factors |
| app.rs | detection.rs | Server::builder.add_service(detection_service) | WIRED | Lines 150-167 start gRPC server with DetectionServiceServer |
| sim_calibration.rs | bridge.rs | try_recv via bridge.drain() | WIRED | Line 76 calls `bridge.drain(MAX_COMMANDS_PER_FRAME)` |
| sim_render.rs | camera.rs | CameraRegistry read for FOV cones | WIRED | app.rs reads camera_registry, passes cameras to build_camera_overlay_vertices |
| grpc_integration.rs | detection.rs | DetectionServiceClient | WIRED | Line 16 imports client, tests connect and exercise RPCs |
| detection_client.py | detection.proto | grpcio stubs | WIRED | Imports detection_pb2_grpc.DetectionServiceStub |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|------------|-------------|--------|----------|
| DET-01 | 17-01, 17-02 | gRPC service accepting detection events | SATISFIED | DetectionService with StreamDetections RPC, proto definition, tonic implementation |
| DET-02 | 17-02 | Aggregation into per-class counts per window | SATISFIED | DetectionAggregator with configurable 5-min windows, per-class HashMap counts |
| DET-03 | 17-02 | Camera registration with position, FOV, edge mapping | SATISFIED | CameraRegistry.register with edges_in_fov spatial query via rstar |
| DET-04 | 17-04 | Camera positions and FOV overlay on map | SATISFIED | build_camera_overlay_vertices with FOV cone geometry, toggleable via checkbox |
| DET-05 | 17-02 | Speed estimation data accepted per camera | SATISFIED | DetectionEvent.speed_kmh optional field, TimeWindow.speed_samples with weighted averaging |
| DET-06 | 17-04 | Python and Rust client libraries | SATISFIED | Rust integration tests (4 tests pass), Python VelosDetectionClient class |
| CAL-01 | 17-03 | OD spawn rate adjustment from observed vs simulated | SATISFIED | CalibrationOverlay with EMA-smoothed ratios, Spawner.generate_spawns_calibrated, step_calibration every 300s |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| (none) | - | - | - | No anti-patterns detected |

No TODOs, FIXMEs, placeholders, or empty implementations found in any Phase 17 files.

### Human Verification Required

### 1. Camera FOV Cone Rendering

**Test:** Start VELOS (`cargo run -p velos-gpu`), register a camera via Python client, enable "Show Cameras" checkbox
**Expected:** Semi-transparent FOV cone polygon visible at camera position on the 2D map
**Why human:** Visual rendering output cannot be verified programmatically

### 2. End-to-End Python Client Flow

**Test:** Run `python tools/python/test_detection_client.py` while VELOS is running
**Expected:** Camera registers, detection batches stream, acks received with OK status
**Why human:** Requires live gRPC server and cross-process communication

### 3. Calibration Panel Live Metrics

**Test:** After Python client pushes detections, check egui "Calibration Panel"
**Expected:** Panel shows per-camera observed count, simulated count, ratio, camera name
**Why human:** UI rendering and real-time data flow need visual confirmation

### Gaps Summary

No gaps found. All 5 success criteria truths verified. All 7 requirement IDs (DET-01 through DET-06, CAL-01) are satisfied with substantive implementations backed by 38 unit tests and 4 integration tests (all passing). All key links are wired -- no orphaned artifacts. The codebase compiles cleanly (`cargo check -p velos-gpu` succeeds).

The phase delivers a complete detection ingestion and demand calibration pipeline: proto contract, gRPC service, camera registration with FOV spatial queries, windowed aggregation with speed averaging, ArcSwap-based calibration overlay, EMA-smoothed ratio computation, spawner integration, background gRPC server alongside winit, egui calibration panel, camera FOV rendering, and both Rust and Python client SDKs.

Three items flagged for human verification are visual/integration concerns that require a running application.

---

_Verified: 2026-03-10T09:30:00Z_
_Verifier: Claude (gsd-verifier)_
