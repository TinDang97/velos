---
phase: 18
slug: 3d-rendering-core
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-10
---

# Phase 18 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test + naga validation (already in dev-deps) |
| **Config file** | Cargo.toml [dev-dependencies] naga = "27" |
| **Quick run command** | `cargo test -p velos-gpu --lib` |
| **Full suite command** | `cargo test -p velos-gpu` |
| **Estimated runtime** | ~15 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p velos-gpu --lib -x`
- **After every plan wave:** Run `cargo test -p velos-gpu`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 15 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 18-01-01 | 01 | 1 | R3D-01 | unit | `cargo test -p velos-gpu orbit_camera -x` | ❌ W0 | ⬜ pending |
| 18-01-02 | 01 | 1 | R3D-01 | unit | `cargo test -p velos-gpu depth_texture -x` | ❌ W0 | ⬜ pending |
| 18-02-01 | 02 | 1 | R3D-02 | unit | `cargo test -p velos-gpu road_surface -x` | ❌ W0 | ⬜ pending |
| 18-02-02 | 02 | 1 | R3D-02 | unit | `cargo test -p velos-gpu lane_marking -x` | ❌ W0 | ⬜ pending |
| 18-03-01 | 03 | 2 | R3D-03 | unit | `cargo test -p velos-gpu lod_classify -x` | ❌ W0 | ⬜ pending |
| 18-03-02 | 03 | 2 | R3D-03 | unit | `cargo test -p velos-gpu mesh_loader -x` | ❌ W0 | ⬜ pending |
| 18-03-03 | 03 | 2 | R3D-03 | unit | `cargo test -p velos-gpu instance_3d_size -x` | ❌ W0 | ⬜ pending |
| 18-04-01 | 04 | 3 | R3D-04 | unit | `cargo test -p velos-gpu view_toggle -x` | ❌ W0 | ⬜ pending |
| 18-05-01 | 05 | 3 | R3D-05 | unit | `cargo test -p velos-gpu lighting_keyframe -x` | ❌ W0 | ⬜ pending |
| 18-05-02 | 05 | 3 | R3D-05 | unit | `cargo test -p velos-gpu lighting_uniform_size -x` | ❌ W0 | ⬜ pending |
| 18-00-01 | 00 | 0 | R3D-01 | unit | `cargo test -p velos-gpu --test render_tests` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/velos-gpu/src/orbit_camera.rs` — OrbitCamera tests (matrix validity, pitch clamp, state mapping)
- [ ] `crates/velos-gpu/src/road_surface.rs` — Road polygon generation tests
- [ ] `crates/velos-gpu/src/mesh_loader.rs` — glTF loading tests (need test .glb fixture)
- [ ] `crates/velos-gpu/src/lighting.rs` — Keyframe interpolation tests
- [ ] `crates/velos-gpu/src/renderer3d.rs` — Instance struct size tests, LOD classification tests
- [ ] `assets/models/test_cube.glb` — Minimal test fixture for mesh_loader tests
- [ ] WGSL validation tests for new shaders in existing `render_tests.rs`

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Full 3D render pipeline visual verification | R3D-01 to R3D-05 | Requires visual inspection of rendered output | Run app, toggle to 3D mode with [V], verify depth ordering, road surfaces, LOD transitions, and lighting changes |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 15s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
