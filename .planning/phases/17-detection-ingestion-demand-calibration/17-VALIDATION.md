---
phase: 17
slug: detection-ingestion-demand-calibration
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-10
---

# Phase 17 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (built-in) + Python unittest |
| **Config file** | Cargo workspace test configuration (existing) |
| **Quick run command** | `cargo test -p velos-api` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p velos-api`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 17-01-01 | 01 | 1 | DET-01 | integration | `cargo test -p velos-api --test grpc_integration -- stream_detections` | ❌ W0 | ⬜ pending |
| 17-01-02 | 01 | 1 | DET-03 | integration | `cargo test -p velos-api --test grpc_integration -- register_camera` | ❌ W0 | ⬜ pending |
| 17-02-01 | 02 | 1 | DET-02 | unit | `cargo test -p velos-api -- aggregator` | ❌ W0 | ⬜ pending |
| 17-02-02 | 02 | 1 | DET-05 | unit | `cargo test -p velos-api -- aggregator::speed` | ❌ W0 | ⬜ pending |
| 17-03-01 | 03 | 2 | CAL-01 | unit | `cargo test -p velos-api -- calibration` | ❌ W0 | ⬜ pending |
| 17-03-02 | 03 | 2 | DET-04 | manual-only | Visual verification: camera cone on map | N/A | ⬜ pending |
| 17-04-01 | 04 | 3 | DET-06 | integration | `cargo test -p velos-api --test grpc_integration && python tools/python/test_detection_client.py` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `proto/velos/v2/detection.proto` — protobuf definition (all gRPC tests depend on this)
- [ ] `crates/velos-api/build.rs` — tonic-build configuration
- [ ] Workspace `Cargo.toml` additions: tonic, prost, tokio
- [ ] `crates/velos-api/tests/grpc_integration.rs` — stubs for DET-01, DET-03, DET-06
- [ ] `crates/velos-api/src/aggregator.rs` unit tests — stubs for DET-02, DET-05
- [ ] `crates/velos-api/src/calibration.rs` unit tests — stubs for CAL-01
- [ ] `tools/python/test_detection_client.py` — Python client test for DET-06

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Camera FOV cone overlay on map | DET-04 | Visual rendering verification | 1. Register camera via gRPC, 2. Open map view, 3. Verify semi-transparent cone polygon at camera position, 4. Toggle via egui checkbox |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
