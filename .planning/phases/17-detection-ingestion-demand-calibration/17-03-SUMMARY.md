---
phase: 17-detection-ingestion-demand-calibration
plan: 03
subsystem: api, demand, gpu
tags: [arcswap, calibration, grpc, tokio, egui, ema]

requires:
  - phase: 17-02
    provides: CameraRegistry, DetectionAggregator, DetectionServiceImpl, create_detection_service

provides:
  - CalibrationOverlay and CalibrationStore with ArcSwap lock-free reads
  - compute_calibration_factors with EMA smoothing and ratio clamping
  - generate_spawns_calibrated on Spawner for OD-pair scaling
  - gRPC server running on background tokio thread alongside winit
  - SimWorld draining API commands per frame via try_recv
  - egui calibration panel showing per-camera observed/simulated/ratio

affects: [17-04-PLAN]

tech-stack:
  added: []
  patterns: [ArcSwap CalibrationStore mirroring PredictionStore, tokio runtime on std::thread for gRPC-winit coexistence, EMA-smoothed ratio clamping]

key-files:
  created:
    - crates/velos-api/src/calibration.rs
    - crates/velos-gpu/src/sim_calibration.rs
  modified:
    - crates/velos-api/src/lib.rs
    - crates/velos-api/Cargo.toml
    - crates/velos-demand/src/spawner.rs
    - crates/velos-gpu/Cargo.toml
    - crates/velos-gpu/src/app.rs
    - crates/velos-gpu/src/lib.rs
    - crates/velos-gpu/src/sim.rs
    - crates/velos-gpu/src/sim_lifecycle.rs

key-decisions:
  - "EMA alpha=0.3 for calibration ratio smoothing; clamp [0.5, 2.0] applied after EMA"
  - "Simulated count threshold <=5 skips calibration (returns previous ratio, default 1.0)"
  - "Edge-to-zone mapping uses nearest centroid heuristic (simplified, full route-based mapping deferred)"
  - "gRPC server started on std::thread with tokio::runtime before winit event_loop.run_app()"
  - "Calibration recomputes every 300 sim-seconds (5 min) matching aggregation window"

patterns-established:
  - "CalibrationStore mirrors PredictionStore: Arc<ArcSwap<T>> with current()/swap()/clone_handle()"
  - "Background tokio runtime for async services alongside synchronous winit main thread"
  - "Per-camera EMA state tracking for calibration ratio convergence"

requirements-completed: [CAL-01]

duration: 10min
completed: 2026-03-10
---

# Phase 17 Plan 03: Calibration Integration Summary

**CalibrationOverlay with ArcSwap lock-free reads, EMA-smoothed ratio computation clamped to [0.5, 2.0], gRPC server on background tokio thread, spawner reading calibration factors per OD pair, and egui panel displaying per-camera calibration state**

## Performance

- **Duration:** 10 min
- **Started:** 2026-03-10T08:26:46Z
- **Completed:** 2026-03-10T08:37:08Z
- **Tasks:** 2
- **Files modified:** 10

## Accomplishments
- CalibrationOverlay struct with per-OD-pair HashMap<(Zone, Zone), f32> factors and ArcSwap store
- compute_camera_ratio with EMA smoothing (alpha=0.3), [0.5, 2.0] clamping, and div-by-zero safety
- compute_calibration_factors mapping camera ratios to OD pairs via edge-to-zone nearest centroid
- Spawner.generate_spawns_calibrated multiplies OD trip counts by calibration factors
- gRPC detection server starts on background std::thread with tokio runtime before winit
- SimWorld.step_api_commands drains up to 64 commands per frame via try_recv
- SimWorld.step_calibration recomputes every 300 sim-seconds, counting agents on covered edges
- egui calibration panel with camera count, gRPC address, per-camera observed/simulated/ratio grid
- 11 new unit tests (10 calibration, 1 spawner) all passing

## Task Commits

Each task was committed atomically:

1. **Task 1: CalibrationOverlay, CalibrationStore, and calibration computation** - `c7a217b` (feat)
2. **Task 2: Wire gRPC server, integrate calibration into Spawner, add egui panel** - `9c4c52d` (feat)

## Files Created/Modified
- `crates/velos-api/src/calibration.rs` - CalibrationOverlay, CalibrationStore, compute functions (10 tests)
- `crates/velos-api/src/lib.rs` - Added calibration type re-exports
- `crates/velos-api/Cargo.toml` - Added velos-demand dependency for Zone type
- `crates/velos-demand/src/spawner.rs` - generate_spawns_calibrated method (1 test)
- `crates/velos-gpu/Cargo.toml` - Added velos-api, tokio, tonic dependencies
- `crates/velos-gpu/src/app.rs` - gRPC server startup on background thread, egui calibration panel
- `crates/velos-gpu/src/lib.rs` - Added sim_calibration module declaration
- `crates/velos-gpu/src/sim.rs` - Added calibration/API fields to SimWorld struct and constructors
- `crates/velos-gpu/src/sim_calibration.rs` - step_api_commands, step_calibration, build_edge_to_zone
- `crates/velos-gpu/src/sim_lifecycle.rs` - spawn_agents uses calibrated path when overlay active

## Decisions Made
- EMA alpha=0.3 balances responsiveness (30% new data) with stability (70% previous) for calibration ratios
- Simulated count threshold <=5 prevents unstable ratios from sparse data; returns previous ratio (not 1.0 reset)
- Edge-to-zone mapping uses nearest centroid heuristic rather than full route-based OD resolution (sufficient for POC, avoids O(cameras * routes) complexity)
- gRPC server on std::thread (not tokio::spawn) to avoid main-thread Cocoa conflict on macOS
- Calibration recomputes every 300 sim-seconds matching the default aggregation window duration
- EMA is applied before clamping to prevent oscillation at clamp boundaries

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing] Clippy or_insert_with on Default type**
- **Found during:** Task 2 (clippy check)
- **Issue:** `or_insert_with(CameraCalibrationState::default)` triggers clippy unwrap_or_default lint
- **Fix:** Changed to `.or_default()`
- **Files modified:** crates/velos-api/src/calibration.rs
- **Committed in:** 9c4c52d (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 missing critical)
**Impact on plan:** Trivial fix. No scope creep.

## Issues Encountered
- sim.rs exceeds 700-line soft limit (781 lines) due to SimWorld struct being the central state container with 30+ fields. The calibration logic itself is properly extracted to sim_calibration.rs (146 lines). This is a pre-existing structural concern, not introduced by this plan.
- Pre-existing clippy errors in sim_render.rs (6 dead_code/unnecessary_cast warnings from Phase 16 camera overlay code) logged to deferred items -- not in scope for this plan.

## Next Phase Readiness
- CalibrationOverlay and CalibrationStore ready for integration test validation (Plan 04)
- gRPC server running alongside simulation, ready for Python client integration testing (Plan 04)
- All Plan 03 artifacts match Plan 04 interface expectations

---
*Phase: 17-detection-ingestion-demand-calibration*
*Completed: 2026-03-10*
