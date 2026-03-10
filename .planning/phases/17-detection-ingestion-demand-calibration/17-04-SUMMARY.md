---
phase: 17-detection-ingestion-demand-calibration
plan: 04
subsystem: api, gpu
tags: [grpc, camera-fov, python-client, wgpu, detection, overlay]

# Dependency graph
requires:
  - phase: 17-02
    provides: "CameraRegistry, DetectionAggregator, DetectionService gRPC handler"
  - phase: 17-03
    provides: "CalibrationOverlay, gRPC server wiring, egui calibration panel"
provides:
  - "Camera FOV cone rendering on 2D simulation map"
  - "Rust gRPC integration tests for DetectionService"
  - "Python client SDK for detection ingestion"
  - "Speed heatmap overlay with live Python feed"
affects: [20-real-time-calibration]

# Tech tracking
tech-stack:
  added: [grpcio, grpcio-tools, python-protobuf-stubs]
  patterns: [camera-overlay-rendering, python-grpc-client-sdk, speed-heatmap-overlay]

key-files:
  created:
    - crates/velos-api/tests/grpc_integration.rs
    - tools/python/detection_client.py
    - tools/python/test_detection_client.py
    - tools/python/requirements.txt
  modified:
    - crates/velos-gpu/src/sim_render.rs
    - crates/velos-gpu/src/renderer.rs
    - crates/velos-gpu/src/app.rs
    - crates/velos-api/src/camera.rs

key-decisions:
  - "Camera FOV rendered as semi-transparent triangle cone with alpha 0.15 fill and 0.6 outline"
  - "Speed indicator circle rendered at camera position for heatmap overlay"
  - "RegisterCamera uses fire-and-forget pattern (no oneshot reply) for simplicity"
  - "Camera range reduced from 100m to 40m for realistic urban deployment"

patterns-established:
  - "Camera overlay pattern: lock CameraRegistry briefly, build instances, render after roads"
  - "Python gRPC client SDK pattern: wrapper class around generated protobuf stubs"

requirements-completed: [DET-04, DET-06]

# Metrics
duration: 25min
completed: 2026-03-10
---

# Phase 17 Plan 04: Camera FOV Overlay and Client SDKs Summary

**Camera FOV cone overlay on 2D map with speed heatmap, Rust gRPC integration tests, and Python client SDK for detection ingestion**

## Performance

- **Duration:** ~25 min
- **Started:** 2026-03-10T08:38:10Z
- **Completed:** 2026-03-10T09:05:00Z
- **Tasks:** 2 (1 auto + 1 human-verify checkpoint)
- **Files modified:** 7

## Accomplishments
- Camera positions render with semi-transparent FOV cone polygons on the 2D simulation map, toggleable via egui checkbox
- Rust integration tests exercise RegisterCamera, ListCameras, and StreamDetections RPCs
- Python client SDK wraps grpcio stubs with clean API for external CV service integration
- Speed heatmap overlay added with live Python feed for real-time detection visualization
- Camera overlay visibility and performance optimized (fixed per-frame rebuild, adjusted range)

## Task Commits

Each task was committed atomically:

1. **Task 1: Camera FOV cone rendering and Rust/Python client SDKs** - `f83be83` (feat)
   - Follow-up fixes applied during verification:
   - `42d35a5` - fix: make RegisterCamera command fire-and-forget
   - `ca2d869` - fix: improve camera overlay visibility
   - `9846919` - fix: use realistic camera range (100m -> 40m)
   - `cc3d1e4` - perf: fix per-frame camera overlay rebuild
   - `0902a5d` - feat: add speed heatmap overlay and live Python feed
   - `02c1cf2` - fix: add speed indicator circle at camera position
2. **Task 2: Visual verification of camera overlay and end-to-end gRPC flow** - checkpoint approved

## Files Created/Modified
- `crates/velos-gpu/src/sim_render.rs` - Camera FOV cone rendering, speed heatmap overlay
- `crates/velos-gpu/src/renderer.rs` - Camera overlay integration into render pipeline
- `crates/velos-gpu/src/app.rs` - show_cameras toggle wiring
- `crates/velos-api/tests/grpc_integration.rs` - Rust integration tests for all 3 DetectionService RPCs
- `tools/python/detection_client.py` - Python client SDK wrapping grpcio stubs
- `tools/python/test_detection_client.py` - Python integration test script
- `tools/python/requirements.txt` - Python dependencies (grpcio, grpcio-tools)

## Decisions Made
- Camera FOV rendered as semi-transparent triangle cone (alpha 0.15 fill, 0.6 outline) for clear but non-obstructive visualization
- RegisterCamera switched to fire-and-forget pattern instead of oneshot reply channel for simplicity
- Camera range reduced from 100m to 40m for realistic urban CCTV deployment
- Speed indicator circle added at camera position for heatmap overlay context
- Per-frame camera overlay rebuild eliminated via caching for performance

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed RegisterCamera blocking behavior**
- **Found during:** Task 1 verification
- **Issue:** RegisterCamera used oneshot reply channel causing deadlock in some scenarios
- **Fix:** Switched to fire-and-forget pattern via mpsc send
- **Files modified:** crates/velos-api/src/camera.rs
- **Committed in:** 42d35a5

**2. [Rule 1 - Bug] Fixed camera overlay visibility**
- **Found during:** Task 2 visual verification
- **Issue:** Camera overlay was barely visible with initial alpha values
- **Fix:** Adjusted alpha values and cone geometry for better visibility
- **Files modified:** crates/velos-gpu/src/sim_render.rs
- **Committed in:** ca2d869

**3. [Rule 1 - Bug] Fixed unrealistic camera range**
- **Found during:** Task 2 visual verification
- **Issue:** 100m camera range was unrealistic for urban CCTV deployment
- **Fix:** Reduced default range to 40m
- **Files modified:** crates/velos-api/src/camera.rs
- **Committed in:** 9846919

**4. [Rule 1 - Bug] Fixed per-frame camera overlay rebuild**
- **Found during:** Task 2 visual verification
- **Issue:** Camera overlay instances were rebuilt every frame unnecessarily
- **Fix:** Added caching to only rebuild when camera list changes
- **Files modified:** crates/velos-gpu/src/sim_render.rs
- **Committed in:** cc3d1e4

**5. [Rule 2 - Missing Critical] Added speed heatmap overlay**
- **Found during:** Task 2 visual verification
- **Issue:** Speed data from detections had no visual representation
- **Fix:** Added speed heatmap overlay with live Python feed
- **Files modified:** crates/velos-gpu/src/sim_render.rs, crates/velos-gpu/src/renderer.rs
- **Committed in:** 0902a5d, 02c1cf2

---

**Total deviations:** 5 auto-fixed (4 bugs, 1 missing critical)
**Impact on plan:** All fixes improved correctness and usability. Speed heatmap was a natural extension of camera overlay. No scope creep.

## Issues Encountered
None beyond the auto-fixed items above.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Phase 17 is now complete -- all detection ingestion and demand calibration infrastructure is in place
- Phase 20 (Real-Time Calibration) can build on the batch calibration foundation from Plans 01-04
- Phase 18 (3D Rendering Core) is architecturally independent and can proceed

## Self-Check: PASSED

All 7 key files verified present. All 7 commit hashes verified in git log.

---
*Phase: 17-detection-ingestion-demand-calibration*
*Completed: 2026-03-10*
