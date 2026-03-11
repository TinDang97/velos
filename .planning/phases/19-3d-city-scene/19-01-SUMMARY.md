---
phase: 19-3d-city-scene
plan: 01
subsystem: rendering
tags: [osm, building-extrusion, earcut, wgsl, gpu, geometry, pbf]

# Dependency graph
requires:
  - phase: 18-3d-rendering-core
    provides: "Renderer3D, mesh_3d.wgsl shader patterns, CameraUniform/LightingUniform bind groups"
provides:
  - "BuildingFootprint extraction from OSM PBF with height inference"
  - "BuildingVertex (40 bytes) with position, normal, color"
  - "generate_building_geometry: merged vertex/index buffers for single draw call"
  - "building_3d.wgsl shader with Lambert diffuse + ambient lighting"
affects: [19-03-PLAN, 19-02-PLAN]

# Tech tracking
tech-stack:
  added: [earcutr (ear-cutting triangulation)]
  patterns: [two-pass PBF import, merged geometry buffers, per-vertex color baking]

key-files:
  created:
    - crates/velos-net/src/building_import.rs
    - crates/velos-gpu/src/building_geometry.rs
    - crates/velos-gpu/shaders/building_3d.wgsl
  modified:
    - crates/velos-net/src/lib.rs
    - crates/velos-gpu/src/lib.rs
    - crates/velos-gpu/src/renderer3d.rs

key-decisions:
  - "No instancing for buildings -- unique geometry per building in merged buffer (single draw call)"
  - "Outward wall normals computed as cross(up, edge) for CCW polygon in XZ plane"
  - "Only Way-type buildings for POC (no multipolygon Relations)"
  - "Base color #D4C5A9 beige with deterministic +/-5% variation per building centroid"

patterns-established:
  - "Building import two-pass pattern: pass 1 collects nodes, pass 2 extracts building ways"
  - "Merged vertex/index buffer pattern for static geometry (no instancing needed)"

requirements-completed: [R3D-06]

# Metrics
duration: 8min
completed: 2026-03-11
---

# Phase 19 Plan 01: Building Data Pipeline Summary

**OSM building footprint extraction with height inference and extruded 3D geometry generation using earcutr triangulation and lit WGSL shader**

## Performance

- **Duration:** 8 min
- **Started:** 2026-03-11T05:25:43Z
- **Completed:** 2026-03-11T05:33:43Z
- **Tasks:** 2
- **Files modified:** 6

## Accomplishments
- BuildingFootprint extraction from OSM PBF with height from tags (height, building:levels, default 10.5m)
- Building extrusion geometry with correct wall normals, roof triangulation, and merged vertex/index buffers
- building_3d.wgsl shader with Lambert diffuse + ambient shading matching mesh_3d.wgsl bind groups
- 26 unit tests passing including integration test (2081 buildings from district1.osm.pbf)

## Task Commits

Each task was committed atomically:

1. **Task 1: Building footprint extraction from OSM PBF** - `3b74d28` (feat)
2. **Task 2: Building extrusion geometry and shader** - `9ef5ddd` (feat)

## Files Created/Modified
- `crates/velos-net/src/building_import.rs` - BuildingFootprint struct, compute_building_height, ensure_ccw, import_buildings
- `crates/velos-gpu/src/building_geometry.rs` - BuildingVertex (40 bytes), generate_building_geometry with earcut roof + wall quads
- `crates/velos-gpu/shaders/building_3d.wgsl` - Lit building shader with CameraUniform + LightingUniform bind groups
- `crates/velos-net/src/lib.rs` - Added building_import module and exports
- `crates/velos-gpu/src/lib.rs` - Added building_geometry module
- `crates/velos-gpu/src/renderer3d.rs` - Added building_3d.wgsl naga validation test

## Decisions Made
- No instancing for buildings: each has unique geometry in merged vertex/index buffers (single draw call)
- Outward wall normals = (dy/len, 0, -dx/len) for CCW polygon edges in XZ plane
- Only Way-type buildings for POC (multipolygon Relations skipped -- documented limitation)
- Base color #D4C5A9 warm beige with deterministic +/-5% brightness variation per building centroid hash
- 2D (x, y) maps to 3D (x, Y, y) consistent with established project convention

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed clippy warnings in building_import.rs**
- **Found during:** Task 2 (clippy verification)
- **Issue:** collapsible_if and ptr_arg clippy warnings treated as errors with -D warnings
- **Fix:** Collapsed nested if-let chains; changed ensure_ccw parameter from &mut Vec to &mut [_]
- **Files modified:** crates/velos-net/src/building_import.rs
- **Verification:** cargo clippy -p velos-net -p velos-gpu -- -D warnings passes clean
- **Committed in:** 9ef5ddd (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 bug fix)
**Impact on plan:** Clippy fix is standard code quality. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- BuildingFootprint and generate_building_geometry ready for Plan 03 renderer integration
- building_3d.wgsl uses same CameraUniform + LightingUniform bind groups as mesh_3d.wgsl
- Plan 02 (terrain) and Plan 03 (renderer wiring) can proceed independently

---
*Phase: 19-3d-city-scene*
*Completed: 2026-03-11*
