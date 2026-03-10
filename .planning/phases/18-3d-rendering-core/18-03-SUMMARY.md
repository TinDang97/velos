---
phase: 18-3d-rendering-core
plan: 03
subsystem: rendering
tags: [wgpu, glTF, LOD, lighting, instanced-rendering, WGSL, billboards]

requires:
  - phase: 18-01
    provides: "OrbitCamera, Renderer3D scaffold, depth buffer, ground plane, MeshInstance3D/BillboardInstance3D types"
provides:
  - "LightingUniform with time-of-day keyframe interpolation"
  - "3-tier LOD classification (Mesh/Billboard/Dot) with hysteresis"
  - "glTF mesh loader with procedural box fallback"
  - "mesh_3d.wgsl: lit instanced 3D mesh shader (diffuse + ambient)"
  - "billboard_3d.wgsl: camera-facing billboard shader with ambient tint"
  - "Renderer3D agent rendering pipeline (mesh + billboard instanced draw)"
affects: [18-04, visualization]

tech-stack:
  added: [gltf-1.4]
  patterns: [gpu-instanced-rendering, lod-hysteresis, time-of-day-lighting, camera-facing-billboards]

key-files:
  created:
    - crates/velos-gpu/src/lighting.rs
    - crates/velos-gpu/src/lod.rs
    - crates/velos-gpu/src/mesh_loader.rs
    - crates/velos-gpu/shaders/mesh_3d.wgsl
    - crates/velos-gpu/shaders/billboard_3d.wgsl
    - assets/models/.gitkeep
  modified:
    - crates/velos-gpu/src/renderer3d.rs
    - crates/velos-gpu/src/lib.rs
    - crates/velos-gpu/Cargo.toml

key-decisions:
  - "Separate bind group layouts: camera-only (ground/road) vs camera+lighting (mesh/billboard)"
  - "CameraUniform3D extended to 112 bytes with eye_position, camera_right, camera_up for billboard orientation"
  - "Billboard uses 6-vertex quad (2 triangles) from vertex_index, no vertex buffer needed"
  - "Mesh rendering uses back-face culling; billboards use no culling"
  - "LOD boundary at exactly threshold goes to lower tier (50m -> Billboard, not Mesh)"

patterns-established:
  - "LOD hysteresis: downgrade at threshold*1.1, upgrade at exact threshold"
  - "Lighting keyframes: 4 presets (night/dawn/noon/sunset) with lerp interpolation"
  - "Fallback meshes: procedural box generation when .glb files not found"
  - "Per-vehicle-type instance buffers for GPU instanced rendering"

requirements-completed: [R3D-03, R3D-05]

duration: 11min
completed: 2026-03-10
---

# Phase 18 Plan 03: Agent Rendering Pipeline Summary

**3-tier LOD agent rendering (mesh/billboard/dot) with glTF loading, diffuse+ambient lighting, and GPU-instanced draw calls**

## Performance

- **Duration:** 11 min
- **Started:** 2026-03-10T15:58:46Z
- **Completed:** 2026-03-10T16:09:44Z
- **Tasks:** 2
- **Files modified:** 9

## Accomplishments
- Time-of-day lighting system with 4 keyframes (night/dawn/noon/sunset) and smooth interpolation
- 3-tier LOD classification with hysteresis preventing flicker at tier boundaries
- glTF mesh loader with procedural box fallback for all 7 vehicle types
- Two new WGSL shaders: lit mesh rendering and camera-facing billboards
- Renderer3D extended with complete agent rendering pipeline (instanced draw calls)
- 194 tests passing including naga shader validation for both new shaders

## Task Commits

Each task was committed atomically:

1. **Task 1: Lighting system and LOD classification** - `3afd8d5` (test+feat -- TDD)
2. **Task 2: glTF mesh loader, 3D shaders, Renderer3D agent pipeline** - `d6b514c` (feat)

## Files Created/Modified
- `crates/velos-gpu/src/lighting.rs` - LightingUniform (48 bytes), 4 keyframes, compute_lighting() with lerp
- `crates/velos-gpu/src/lod.rs` - LodTier enum, classify_lod() with hysteresis band
- `crates/velos-gpu/src/mesh_loader.rs` - Vertex3D, LoadedMesh, load_glb(), generate_fallback_box(), MeshSet
- `crates/velos-gpu/shaders/mesh_3d.wgsl` - Instanced lit mesh shader (diffuse + ambient from sun direction)
- `crates/velos-gpu/shaders/billboard_3d.wgsl` - Camera-facing billboard shader with ambient tint
- `crates/velos-gpu/src/renderer3d.rs` - Extended with lighting uniform, mesh/billboard pipelines, agent draw calls
- `crates/velos-gpu/src/lib.rs` - New module declarations and public exports
- `crates/velos-gpu/Cargo.toml` - Added gltf 1.4 dependency
- `assets/models/.gitkeep` - Model directory placeholder

## Decisions Made
- Separate bind group layouts for ground/road (camera-only) vs mesh/billboard (camera+lighting) to avoid breaking existing ground_plane.wgsl which expects only 1 binding
- CameraUniform3D is 112 bytes (not 128) -- 7 vec4-aligned fields is valid GPU alignment
- Billboard expands vertices from vertex_index in shader (no vertex buffer needed), 6 vertices per instance for a quad
- LOD exact boundary (50m, 200m) classifies to the cheaper tier when no previous tier exists
- Mesh rendering uses back-face culling for correct solid appearance; billboards have no culling

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed CameraUniform3D size assertion**
- **Found during:** Task 2 (Renderer3D extension)
- **Issue:** Initial test expected 128 bytes but actual struct is 112 bytes (7 vec4-aligned fields)
- **Fix:** Corrected test assertion to 112 bytes
- **Files modified:** crates/velos-gpu/src/renderer3d.rs
- **Verification:** test_camera_uniform_3d_size passes

**2. [Rule 1 - Bug] Fixed clippy warnings (loop iteration pattern, dead code)**
- **Found during:** Task 2 (clippy gate)
- **Issue:** `for i in 0..n` pattern instead of `.iter().enumerate()`, unused struct field
- **Fix:** Used idiomatic iterator pattern, prefixed unused field with underscore
- **Files modified:** crates/velos-gpu/src/lighting.rs, crates/velos-gpu/src/renderer3d.rs
- **Verification:** `cargo clippy -p velos-gpu -- -D warnings` passes clean

---

**Total deviations:** 2 auto-fixed (2 bugs)
**Impact on plan:** Minor fixes for correctness and lint compliance. No scope creep.

## Issues Encountered
None -- plan executed cleanly.

## User Setup Required
None - no external service configuration required. Place CC0 `.glb` models in `assets/models/` for real mesh rendering (falls back to procedural boxes otherwise).

## Next Phase Readiness
- Agent rendering pipeline ready for Plan 04 (view mode toggle wiring)
- Plan 04 needs to wire LOD classification into the frame loop using OrbitCamera eye position
- Dot-tier agents should route through existing 2D dot pipeline (Plan 04 wiring)

---
*Phase: 18-3d-rendering-core*
*Completed: 2026-03-10*
