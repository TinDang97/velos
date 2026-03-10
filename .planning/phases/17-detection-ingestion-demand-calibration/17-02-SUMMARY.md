---
phase: 17-detection-ingestion-demand-calibration
plan: 02
subsystem: api
tags: [grpc, tonic, rstar, spatial, camera, aggregation, detection]

requires:
  - phase: 17-01
    provides: proto types, ApiBridge, ApiError, crate scaffold

provides:
  - CameraRegistry with FOV-to-edge spatial query via rstar
  - DetectionAggregator with windowed per-class counts and speed averaging
  - DetectionServiceImpl implementing all three gRPC RPCs
  - Factory function create_detection_service for app wiring

affects: [17-03-PLAN, 17-04-PLAN]

tech-stack:
  added: [tokio-stream 0.1]
  patterns: [rstar AABB + angular cone filter, time-windowed aggregation with GC, tonic bidirectional streaming via ReceiverStream]

key-files:
  created:
    - crates/velos-api/src/camera.rs
    - crates/velos-api/src/aggregator.rs
  modified:
    - crates/velos-api/src/detection.rs
    - crates/velos-api/src/lib.rs
    - crates/velos-api/Cargo.toml
    - Cargo.toml

key-decisions:
  - "edges_in_fov uses AABB bounding-box pre-filter then angle normalization to [-PI, PI] for wraparound correctness"
  - "DetectionAggregator uses i32 keys for vehicle class (matching prost proto enum encoding) not Rust enum"
  - "DetectionServiceImpl holds Arc<RTree<EdgeSegment>> and Arc<EquirectangularProjection> for camera registration"
  - "ReceiverStream wraps mpsc channel for bidirectional streaming response type"
  - "tokio-stream 0.1 added to workspace for ReceiverStream; tokio time feature added for timeout"

patterns-established:
  - "FOV spatial query: AABB envelope query + distance check + angle normalization for cone filter"
  - "Time-windowed aggregation: floor(timestamp / window_size) * window_size for window start"
  - "Speed averaging: (sum_speed * count, total_count) tuple for weighted mean computation"

requirements-completed: [DET-01, DET-02, DET-03, DET-05]

duration: 5min
completed: 2026-03-10
---

# Phase 17 Plan 02: Core gRPC Service Summary

**CameraRegistry with rstar FOV-to-edge spatial queries, DetectionAggregator with 5-min windowed counts and weighted speed averaging, and DetectionService handler implementing bidirectional streaming, camera registration, and camera listing RPCs**

## Performance

- **Duration:** 5 min
- **Started:** 2026-03-10T08:18:30Z
- **Completed:** 2026-03-10T08:23:48Z
- **Tasks:** 2
- **Files modified:** 6

## Accomplishments
- CameraRegistry with sequential ID assignment and edges_in_fov spatial query handling heading wraparound at 0/360 boundary
- DetectionAggregator with configurable window duration (default 5min), retention (default 1hr), GC, and weighted speed averaging
- DetectionServiceImpl implementing stream_detections (bidirectional), register_camera (unary with bridge), list_cameras (unary read-only)
- Factory function create_detection_service wiring all shared state for tonic server setup
- 15 new unit tests (7 camera, 8 aggregator) all passing, clippy clean

## Task Commits

Each task was committed atomically:

1. **Task 1: CameraRegistry and DetectionAggregator with unit tests** - `e2e01be` (feat)
2. **Task 2: DetectionService gRPC handler implementation** - `a7d8052` (feat)

## Files Created/Modified
- `crates/velos-api/src/camera.rs` - Camera struct, CameraRegistry, edges_in_fov spatial query (7 tests)
- `crates/velos-api/src/aggregator.rs` - TimeWindow, DetectionAggregator with windowed counts and speed samples (8 tests)
- `crates/velos-api/src/detection.rs` - DetectionServiceImpl with stream_detections, register_camera, list_cameras
- `crates/velos-api/src/lib.rs` - Module declarations, re-exports, create_detection_service factory
- `crates/velos-api/Cargo.toml` - Added rstar, tokio-stream, velos-net dependencies
- `Cargo.toml` - Added tokio-stream to workspace deps, tokio time feature

## Decisions Made
- edges_in_fov uses angle normalization to [-PI, PI] range (handles wraparound at 0/360 correctly)
- DetectionAggregator stores vehicle class as i32 (matching prost proto encoding) rather than a Rust enum, simplifying HashMap key usage
- DetectionServiceImpl stores Arc<RTree<EdgeSegment>> and Arc<EquirectangularProjection> to avoid passing through bridge for camera registration
- Bidirectional streaming uses tokio::spawn + ReceiverStream pattern for async client stream processing
- register_camera does local registration AND forwards to SimWorld via bridge (local for immediate response, bridge for simulation awareness)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] tokio-stream dependency missing**
- **Found during:** Task 2
- **Issue:** ReceiverStream requires tokio-stream crate, not included in workspace deps
- **Fix:** Added tokio-stream 0.1 to workspace Cargo.toml and velos-api Cargo.toml
- **Files modified:** Cargo.toml, crates/velos-api/Cargo.toml
- **Committed in:** a7d8052 (Task 2 commit)

**2. [Rule 3 - Blocking] tokio time feature needed for timeout**
- **Found during:** Task 2
- **Issue:** tokio::time::timeout requires the "time" feature on tokio, which was not enabled
- **Fix:** Added "time" to tokio workspace features
- **Files modified:** Cargo.toml
- **Committed in:** a7d8052 (Task 2 commit)

---

**Total deviations:** 2 auto-fixed (2 blocking)
**Impact on plan:** Both fixes necessary for gRPC handler compilation. No scope creep.

## Issues Encountered
None beyond the deviations documented above.

## Next Phase Readiness
- DetectionService fully implemented, ready for integration test client (Plan 04)
- CameraRegistry and DetectionAggregator shared state ready for calibration overlay (Plan 03)
- Factory function provides clean wiring point for app.rs integration

---
*Phase: 17-detection-ingestion-demand-calibration*
*Completed: 2026-03-10*
