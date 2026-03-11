---
phase: 19
slug: 3d-city-scene
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-11
---

# Phase 19 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust built-in) + naga WGSL validation |
| **Config file** | Cargo.toml [dev-dependencies] |
| **Quick run command** | `cargo test -p velos-net --lib building_import && cargo test -p velos-gpu --lib building_geometry terrain` |
| **Full suite command** | `cargo test -p velos-net -p velos-gpu` |
| **Estimated runtime** | ~5 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p velos-net --lib building_import && cargo test -p velos-gpu --lib building_geometry terrain`
- **After every plan wave:** Run `cargo test -p velos-net -p velos-gpu`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 5 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 19-01-01 | 01 | 1 | R3D-06 | unit | `cargo test -p velos-net --lib building_import` | ❌ W0 | ⬜ pending |
| 19-01-02 | 01 | 1 | R3D-06 | unit | `cargo test -p velos-net --lib building_import::tests::test_height` | ❌ W0 | ⬜ pending |
| 19-01-03 | 01 | 1 | R3D-06 | unit | `cargo test -p velos-gpu --lib building_geometry::tests` | ❌ W0 | ⬜ pending |
| 19-01-04 | 01 | 1 | R3D-06 | unit | `cargo test -p velos-gpu --lib building_geometry::tests::test_vertex_size` | ❌ W0 | ⬜ pending |
| 19-01-05 | 01 | 1 | R3D-06 | unit | `cargo test -p velos-gpu --lib renderer3d::tests::test_building_3d_wgsl` | ❌ W0 | ⬜ pending |
| 19-02-01 | 02 | 1 | R3D-07 | unit | `cargo test -p velos-gpu --lib terrain::tests::test_parse_hgt` | ❌ W0 | ⬜ pending |
| 19-02-02 | 02 | 1 | R3D-07 | unit | `cargo test -p velos-gpu --lib terrain::tests::test_grid_to_mesh` | ❌ W0 | ⬜ pending |
| 19-02-03 | 02 | 1 | R3D-07 | unit | `cargo test -p velos-gpu --lib terrain::tests::test_void_fill` | ❌ W0 | ⬜ pending |
| 19-02-04 | 02 | 1 | R3D-07 | unit | `cargo test -p velos-gpu --lib renderer3d::tests::test_terrain_wgsl` | ❌ W0 | ⬜ pending |
| 19-03-01 | 03 | 2 | R3D-06+R3D-07 | smoke/manual | Visual verification in app | Manual | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/velos-net/src/building_import.rs` — building extraction module with tests
- [ ] `crates/velos-gpu/src/building_geometry.rs` — extrusion geometry with tests
- [ ] `crates/velos-gpu/src/terrain.rs` — SRTM parsing + mesh generation with tests
- [ ] `crates/velos-gpu/shaders/building_3d.wgsl` — building shader + naga validation test
- [ ] `crates/velos-gpu/shaders/terrain.wgsl` — terrain shader + naga validation test

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Buildings + terrain render correctly in 3D scene | R3D-06+R3D-07 | Visual correctness requires human eye | Run app, orbit camera around District 1, verify buildings have correct height, terrain has elevation, no z-fighting |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 5s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
