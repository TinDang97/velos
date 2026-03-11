---
phase: 20
slug: real-time-calibration
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-11
---

# Phase 20 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test + cargo test |
| **Config file** | Cargo.toml (workspace) |
| **Quick run command** | `cargo test -p velos-api --lib calibration && cargo test -p velos-gpu --lib sim_calibration` |
| **Full suite command** | `cargo test -p velos-api -p velos-gpu` |
| **Estimated runtime** | ~15 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p velos-api --lib calibration && cargo test -p velos-gpu --lib sim_calibration`
- **After every plan wave:** Run `cargo test -p velos-api -p velos-gpu`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 15 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 20-01-01 | 01 | 1 | CAL-02a | unit | `cargo test -p velos-api --lib calibration::tests::window_change_triggers` | ❌ W0 | ⬜ pending |
| 20-01-02 | 01 | 1 | CAL-02b | unit | `cargo test -p velos-api --lib calibration::tests::cooldown_prevents_thrashing` | ❌ W0 | ⬜ pending |
| 20-01-03 | 01 | 1 | CAL-02c | unit | `cargo test -p velos-api --lib calibration::tests::min_observation_threshold` | ❌ W0 | ⬜ pending |
| 20-01-04 | 01 | 1 | CAL-02d | unit | `cargo test -p velos-api --lib calibration::tests::decay_toward_baseline` | ❌ W0 | ⬜ pending |
| 20-01-05 | 01 | 1 | CAL-02e | unit | `cargo test -p velos-api --lib calibration::tests::change_cap_limits_jumps` | ❌ W0 | ⬜ pending |
| 20-01-06 | 01 | 1 | CAL-02f | unit | `cargo test -p velos-api --lib calibration::tests::no_data_no_recalibration` | ❌ W0 | ⬜ pending |
| 20-01-07 | 01 | 2 | CAL-02g | unit | `cargo test -p velos-gpu --lib sim_calibration::tests::paused_skips` | ❌ W0 | ⬜ pending |
| 20-01-08 | 01 | 2 | CAL-02h | unit | `cargo test -p velos-api --lib calibration::tests::late_camera_participates` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/velos-api/src/calibration.rs` — test module for window-change detection, cooldown, min observation, decay, change cap, late camera
- [ ] `crates/velos-gpu/src/sim_calibration.rs` — test module for paused_skips, integration trigger tests

*All tests are new — existing calibration tests cover Phase 17 batch behavior only.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Egui panel shows Calibrating/Idle/Stale status | CAL-02 | Visual UI verification | Run sim with detection stream, observe panel status transitions |
| OD spawn rates visually change in response to detections | CAL-02 SC-2 | Requires visual observation | Start sim, inject detections via gRPC, watch agent spawn density |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 15s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
