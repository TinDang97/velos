# Pitfalls Research

**Domain:** Scaling Rust/wgpu traffic microsimulation from desktop POC to production digital twin platform
**Researched:** 2026-03-07
**Confidence:** MEDIUM-HIGH (verified against wgpu docs, GPU simulation papers, RoutingKit CCH docs, deck.gl performance guides, Redis scaling literature)

**Context:** v1.0 shipped a 1.5K-agent desktop app (egui/winit, CPU physics, A* routing, single GPU render). v1.1 scales to 280K agents with multi-GPU compute, CCH routing, prediction ensemble, deck.gl web visualization, gRPC/REST/WebSocket API, and Docker deployment. These pitfalls focus specifically on what breaks during that transition.

## Critical Pitfalls

### Pitfall 1: GPU Compute Not Actually in the Sim Loop -- The "Proven But Not Wired" Trap

**What goes wrong:**
v1.0 shipped with GPU compute "proven via tests" but CPU-side ECS physics running the actual sim loop. Scaling to 280K agents requires GPU compute to be the real physics driver, not a parallel test path. Teams commonly maintain both CPU and GPU paths "for testing" and never fully cut over. The CPU path silently handles edge cases the GPU path doesn't, creating a false sense that GPU compute works. When you finally remove the CPU fallback at 280K scale, dozens of unhandled edge cases surface simultaneously.

**Why it happens:**
CPU physics is easier to debug (breakpoints, print statements, no buffer mapping). Developers keep it as a "reference" and defer full GPU cutover. At 1.5K agents, CPU is fast enough that nobody notices the GPU path isn't actually running. The PROJECT.md explicitly documents this: "GPU compute pipeline proven via tests but not wired into main sim loop."

**How to avoid:**
1. **Kill the CPU physics path immediately.** Make the GPU compute shader the sole physics driver in Phase 1 of v1.1. Do not maintain a parallel CPU path "for debugging."
2. **Build a GPU validation compute pass** that runs after each physics step and checks invariants (no NaN, no negative speed, no position > edge length, gap >= s0). This replaces the debugging convenience of CPU physics.
3. **Run the v1.0 test suite against GPU output.** All 185 existing tests must pass when physics runs on GPU. Failing tests reveal the unhandled edge cases early.

**Warning signs:**
- "We'll switch to GPU physics later" appearing in code reviews
- GPU tests passing but integration tests still using CPU physics
- Frame time benchmarks not matching expected GPU throughput (because GPU isn't doing the work)

**Phase to address:**
Phase 1 (Foundation). The very first task of v1.1 is wiring GPU compute into the main sim loop. Gate G1 should require GPU physics, not just GPU rendering.

---

### Pitfall 2: Multi-GPU Partitioning Assumes wgpu Multi-Adapter Works for Compute

**What goes wrong:**
The architecture specifies 2-4 GPUs via `wgpu::Instance::enumerate_adapters()`, each owning a spatial partition. However, wgpu's multi-adapter support for compute workloads is not well-documented and has known issues. On some platforms, `enumerate_adapters()` returns only one adapter, or creating compute pipelines on secondary adapters fails silently. The WebGPU spec does not expose explicit multi-GPU (SLI/CrossFire). wgpu's native-only multi-adapter support is an extension beyond the spec.

**Why it happens:**
wgpu abstracts GPU access through the WebGPU model, which was designed for single-GPU web browsers. Multi-adapter is a native-only extension. Developers assume "enumerate returns multiple devices, create pipelines on each" will just work, but driver-level quirks (especially mixed GPU vendors, or iGPU+dGPU configurations) cause unexpected failures. Spike S2 in the roadmap exists precisely for this reason.

**How to avoid:**
1. **Run Spike S2 before writing any multi-GPU code.** The architecture correctly identifies this risk. Do not skip the spike.
2. **Design the partition abstraction so single-GPU is a trivial specialization.** `MultiGpuScheduler` with `partitions.len() == 1` should be identical to single-GPU operation. This makes the NO-GO fallback (single GPU, 200K agents) a configuration change, not a rewrite.
3. **If multi-adapter works, validate buffer transfers under load.** Transfer 64KB (the expected per-step boundary agent volume) between GPUs while both are running compute dispatches. Measure latency under contention, not in isolation.
4. **On macOS (Metal backend), multi-GPU is irrelevant.** Apple Silicon has unified memory and a single GPU. Multi-GPU testing requires NVIDIA hardware (Linux/Vulkan). Plan for platform-specific code paths.

**Warning signs:**
- `enumerate_adapters()` returns only integrated GPU on multi-GPU systems
- `request_adapter()` in Docker container returns `None` (known wgpu issue with discrete GPUs in containers)
- Buffer copies between adapters hang or produce zeroed data
- Compute pipeline creation on secondary adapter fails with "unsupported feature"

**Phase to address:**
Phase 1 (Spike S2, Week 1-2). Binary GO/NO-GO. If NO-GO, immediately scope down to single-GPU 200K agents and save 4+ weeks of multi-GPU development.

---

### Pitfall 3: Wave-Front Dispatch Starves GPU Occupancy

**What goes wrong:**
Wave-front (Gauss-Seidel) dispatch processes agents sequentially within each lane. With an average of 5.6 agents/lane, each workgroup does only 5-6 sequential operations before idling. GPU compute shaders achieve peak throughput when workgroups run thousands of threads in parallel. At 5.6 agents/lane, GPU occupancy may be <10%, making wave-front slower than naive parallel dispatch despite its correctness advantages.

**Why it happens:**
The architecture's performance budget estimates 2ms for 280K agents, but this assumes full GPU utilization. With 50K workgroups each doing 5-6 sequential operations, the GPU is massively underutilized -- it has capacity for millions of concurrent threads. The sequential-within-lane constraint converts a massively parallel problem into 50K tiny serial chains. The 11x headroom estimate may be consumed entirely by low occupancy.

**How to avoid:**
1. **Spike S1 is non-negotiable.** The GO criterion (>40% of naive parallel throughput) is the right test. If wave-front achieves only 10-20% of parallel throughput, the architecture must pivot to EVEN/ODD with correction passes.
2. **Benchmark with realistic lane distributions, not averages.** HCMC has arterials with 30 agents/lane and alleys with 0-1 agents/lane. The long-tail lanes dominate execution time because the GPU waits for the slowest workgroup.
3. **Consider hybrid dispatch:** Wave-front for dense lanes (>10 agents), parallel for sparse lanes (<5 agents). The sparse lanes have negligible collision risk, making parallel dispatch safe.
4. **Profile GPU occupancy explicitly** using platform tools (Metal System Trace on macOS, Nsight Compute on NVIDIA). Throughput numbers alone don't reveal the problem -- you need occupancy metrics.

**Warning signs:**
- GPU utilization <30% during compute dispatch (check with platform profiler)
- Frame time not improving when reducing agent count (GPU is already idle)
- Single dense arterial lane dominating total frame time (workgroup load imbalance)
- Spike S1 result near the GO/NO-GO boundary (40-50% of parallel)

**Phase to address:**
Phase 1 (Spike S1, Week 1-2). If marginal, implement the hybrid dispatch strategy in Phase 2 when real traffic patterns are available to profile against.

---

### Pitfall 4: deck.gl CPU Attribute Generation Blocks Rendering at 280K Points

**What goes wrong:**
deck.gl generates vertex attributes on the CPU main thread by default. At 280K points with per-frame updates (10 Hz), the CPU must process 2.8M attribute updates per second. This blocks both rendering and user interaction. The dashboard becomes unresponsive -- mouse events queue behind attribute generation, creating 200-500ms input lag.

**Why it happens:**
deck.gl's ScatterplotLayer and similar layers call accessor functions for each data element to build typed arrays for the GPU. At 280K elements, this JavaScript loop takes 30-80ms per frame depending on attribute count. Since this runs on the main thread, it blocks the browser's event loop. The deck.gl docs explicitly warn: "Built-in attribute generation can become a major bottleneck in performance since it is done on CPU in the main thread."

**How to avoid:**
1. **Pre-calculate binary attributes server-side.** Send pre-packed Float32Arrays via WebSocket in the exact format deck.gl expects. Use `data: {length: N, attributes: {getPosition: {value: float32Array, size: 3}}}` to bypass accessor functions entirely. This is deck.gl's highest-performance data path.
2. **Viewport-based filtering.** Only send agents visible in the current viewport. At typical zoom levels, visible agents are 5K-30K, not 280K. The server must know the client's viewport bounds (send them via WebSocket).
3. **Use server-side aggregation for zoomed-out views.** When zoomed out to see all 5 districts, individual agents are sub-pixel. Send density heatmap tiles instead of individual points. Switch to individual agents when zoomed in past a threshold.
4. **Use WebWorkers for attribute generation** if server-side pre-packing is not feasible. Offload the typed array construction to a worker thread so the main thread stays responsive.

**Warning signs:**
- Dashboard FPS drops below 10 when simulation is running at 10 Hz
- Mouse pan/zoom has visible lag (>100ms response)
- Browser DevTools "Long Task" warnings during WebSocket message processing
- CPU usage at 100% on a single core in the browser tab

**Phase to address:**
Phase 2 (Visualization). When the agent count increases from 10K (G1) to 280K (G3), the visualization pipeline must switch from accessor-based to binary attribute streaming. This should be designed from the start but becomes critical at G3.

---

### Pitfall 5: Fixed-Point Arithmetic Performance Penalty Exceeds Budget

**What goes wrong:**
The architecture estimates a 20% performance penalty for fixed-point over float32, based on manual 64-bit emulation in WGSL. Real-world penalty is likely 40-80% because: (a) every multiply requires 4 partial products + carries, (b) the IDM acceleration formula has 6+ multiplications per agent per step, (c) branch divergence from overflow checks reduces SIMT efficiency. At 280K agents, this could push frame time from 8ms to 14-18ms, consuming the entire headroom budget.

**Why it happens:**
The 20% estimate comes from simple microbenchmarks (single multiply). The IDM formula chains multiplications: `s_star = s0 + v*T + (v*delta_v)/(2*sqrt(a*b))`. Each `*` is a fix_mul with 4 partial products. The `sqrt` in fixed-point requires iterative approximation (Newton-Raphson, 4-6 iterations). The total cost compounds multiplicatively, not additively.

**How to avoid:**
1. **Defer fixed-point to Phase 3 or later.** The architecture already marks it as "optional" and provides an `@invariant` float32 fallback. Take the fallback. Cross-GPU determinism is a nice-to-have for POC, not a requirement.
2. **If fixed-point is required, profile the FULL IDM formula** (not just `fix_mul`) before committing. Build a WGSL benchmark that runs the complete agent update with all IDM terms in fixed-point vs. float32. The compound cost is what matters.
3. **Consider mixed precision:** Use float32 for intermediate IDM computation, convert to fixed-point only for position/speed storage. This gives deterministic state while allowing fast float computation.
4. **Investigate `@invariant` on Metal/Vulkan.** If the Metal shader compiler respects `@invariant` for compute outputs (not just vertex outputs), float32 may be deterministic enough on a single GPU vendor. Test empirically before assuming non-determinism.

**Warning signs:**
- Frame time doubles when switching from float32 to fixed-point in the IDM shader
- Fixed-point `sqrt` approximation consuming >50% of per-agent compute time
- Engineers spending weeks debugging fixed-point overflow edge cases instead of building features
- Determinism requirement not actually needed until multi-GPU (Phase 2+)

**Phase to address:**
Phase 3 (Calibration/Hardening). Fixed-point is optional in the roadmap. Use float32 for Phase 1-2. Only implement fixed-point if multi-GPU cross-device determinism is validated as a real requirement (not theoretical).

---

### Pitfall 6: Meso-Micro Transition Creates Velocity Discontinuities and Phantom Congestion

**What goes wrong:**
When agents transfer from the mesoscopic queue model to microscopic simulation (or vice versa), their velocity and spacing jump discontinuously. Meso models track aggregate flow; micro models track individual vehicle kinematics. An agent leaving a meso queue at free-flow speed and entering micro simulation finds itself 2m behind a slower leader, causing emergency braking. Conversely, agents entering meso from micro lose their individual spacing, and when they exit meso back to micro, they're evenly spaced at the queue model's average headway, regardless of their original micro spacing. This creates artificial congestion waves at every meso-micro boundary.

**Why it happens:**
This is a fundamental modeling challenge documented extensively in hybrid simulation literature. The meso model uses segment-level aggregate speed; the micro model uses vehicle-level IDM. At the boundary, there is no smooth interpolation -- the agent jumps from one model to another. The 100m graduated buffer zone in the architecture helps but does not eliminate the problem, because the velocity-matching insertion must correctly account for the acceleration profile of the target model.

**How to avoid:**
1. **Velocity-matching insertion with warm-up distance.** When an agent enters micro from meso, spawn it 200m upstream of the meso-micro boundary with the meso segment's average speed. Let IDM naturally adjust over those 200m. Do not insert at the boundary with an instantaneous velocity assignment.
2. **Graduated acceleration constraints.** In the 100m buffer zone, linearly interpolate between meso constraints (no acceleration model) and micro constraints (full IDM). Buffer zone agents use a simplified IDM with relaxed parameters.
3. **Validate boundary flow conservation.** The total flow (vehicles/hour) crossing the boundary must be equal on both sides. If meso pushes more agents than micro can absorb (because micro has tighter spacing), agents queue in the buffer and congestion propagates into the meso region.
4. **Benchmark against a full-micro baseline.** Run the same scenario with full micro on the entire network (at reduced agent count if needed). Compare travel times at meso-micro boundaries. If boundary travel time exceeds full-micro by >10%, the transition is injecting artificial delay.

**Warning signs:**
- Speed profiles show a consistent dip at every meso-micro boundary (visible in FCD output)
- Queue spillback from micro into meso that doesn't exist in a full-micro run
- Agents bunching at meso-micro transition points (visible in deck.gl heatmap)
- Calibration GEH failing specifically on links near meso-micro boundaries

**Phase to address:**
Phase 3 (Weeks 25-28 per roadmap). The meso-micro buffer is E1's responsibility. Build it with validation against a full-micro reference scenario before integrating with the calibration loop.

---

### Pitfall 7: Docker Container Cannot Access GPU via wgpu

**What goes wrong:**
Containerizing the Rust simulation engine in Docker, wgpu's `request_adapter()` returns `None` even though `nvidia-smi` works inside the container. The simulation fails to start. This is a known wgpu issue specifically with discrete GPUs in Docker containers. The NVIDIA Container Toolkit exposes GPU compute capabilities (CUDA), but wgpu uses Vulkan (not CUDA), and the Vulkan ICD (Installable Client Driver) loader needs separate configuration.

**Why it happens:**
NVIDIA Container Toolkit sets `NVIDIA_DRIVER_CAPABILITIES=compute,utility` by default, which exposes CUDA but not Vulkan graphics/display capabilities. wgpu on Linux uses Vulkan, which requires `NVIDIA_DRIVER_CAPABILITIES=all` or `NVIDIA_DRIVER_CAPABILITIES=compute,utility,graphics,display`. Additionally, the Vulkan ICD JSON files (`/usr/share/vulkan/icd.d/`) must be mounted into the container, and `libvulkan.so` must be available.

**How to avoid:**
1. **Set `NVIDIA_DRIVER_CAPABILITIES=all`** in the Docker environment or compose file.
2. **Install Vulkan loader in the container.** Add `libvulkan-dev` (Debian) or `vulkan-loader` (Alpine) to the Dockerfile.
3. **Mount Vulkan ICD files.** Map `/usr/share/vulkan/icd.d/` from host to container.
4. **Use `--gpus all` flag** in `docker run`, or `deploy.resources.reservations.devices` in Docker Compose.
5. **Test container GPU access early.** Build a minimal Docker image that runs `wgpu::Instance::enumerate_adapters()` and prints results. Do this before containerizing the full simulation.
6. **For macOS development**, GPU passthrough to Docker is not supported. Docker containerization is a deployment concern for Linux servers only. Develop natively on macOS, deploy in Docker on Linux.

**Warning signs:**
- `nvidia-smi` works in container but wgpu finds no adapters
- `VK_ERROR_INITIALIZATION_FAILED` in container logs
- Container runs fine with CPU fallback but GPU dispatch fails
- Works on bare metal, fails in Docker with identical binary

**Phase to address:**
Phase 4 (Infrastructure/Deployment). Docker containerization happens late in the timeline but should be validated with a minimal wgpu-in-Docker test during Phase 2 to avoid a surprise at deployment time.

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Keep CPU physics path alongside GPU | Easier debugging, test reference | Two code paths to maintain, bugs only manifest on GPU path, false confidence that GPU works | Never for v1.1. Kill CPU physics path immediately. Use GPU validation compute pass for debugging |
| Skip viewport filtering in WebSocket stream | Simpler server, all clients get same data | 280K-point JSON/binary messages overwhelm browser at 10 Hz. 100 concurrent viewers = 2.8M points/sec/client | Phase 1 only (10K agents). Must implement before G3 (280K) |
| Use A* until CCH is ready | No preprocessing delay, familiar code from v1.0 | At 500 reroutes/step, A* costs ~250ms vs CCH's ~0.7ms. Makes prediction-driven rerouting infeasible | Phase 1 only (<10K agents, no rerouting). Replace before Phase 2 prediction integration |
| Single Redis instance for pub/sub | Simple ops, no cluster management | Redis pub/sub is fire-and-forget (no backpressure). Slow subscribers drop messages silently. At 10 Hz x 100 viewers, a single Redis instance may hit memory limits | Acceptable for <20 viewers. Cluster or add backpressure monitoring before 100-viewer load test |
| Parquet checkpoints as synchronous writes | Simpler checkpoint logic | At 280K agents, Parquet serialization takes 200-500ms, blocking the sim loop. At 10 Hz, this is 2-5 missed frames per checkpoint | Never synchronous. Use `tokio::spawn_blocking` from day one. Cost is minimal, benefit is immediate |
| Skip METIS for partition, use geographic split | No METIS dependency, simpler code | Geographic splits (by district) produce unbalanced partitions. D1 has 3x the road density of D10. GPU0 overloaded, GPU1 idle | Only if METIS integration proves too complex. Monitor partition balance: max agents on any GPU / average agents should be < 1.3 |

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| wgpu compute in Docker | Assuming `nvidia-smi` working = wgpu working. wgpu uses Vulkan, not CUDA. | Set `NVIDIA_DRIVER_CAPABILITIES=all`, install `libvulkan-dev`, mount Vulkan ICD files. Test wgpu adapter enumeration in a minimal container before building full image |
| deck.gl + WebSocket binary frames | Sending JSON objects for 280K agent positions. JSON parse time alone is 50-100ms per frame | Send pre-packed Float32Arrays in binary WebSocket frames. Use `data.attributes` API to bypass deck.gl accessor functions. Binary is 10-50x faster than JSON for large point datasets |
| Redis pub/sub + WebSocket relay | Publishing full 280K-agent frame to a single Redis channel. All relay pods receive all data regardless of client viewport | Partition into spatial tiles (e.g., 500m grid cells). Each tile is a separate Redis channel. Relay pods subscribe only to channels matching their connected clients' viewports |
| tonic gRPC + simulation state | Holding a lock on the ECS world while serializing a gRPC response. This blocks the simulation loop for the entire serialization duration | Snapshot simulation state into a read-only buffer at the end of each step (already needed for GPU readback). gRPC handlers read from the snapshot, never from live ECS state |
| CCH + prediction ensemble | Running CCH weight customization synchronously in the sim loop when prediction updates arrive. Customization takes 3-10ms, which is acceptable but adds jitter to frame time | Use `ArcSwap` pattern: prediction writes new weights to a new CCH metric, atomically swaps it in. Sim loop reads the currently-active metric with zero blocking. The architecture already specifies this -- do not deviate |
| Parquet checkpoints + hecs ECS | Iterating hecs archetypes in arbitrary order, producing non-deterministic Parquet row ordering. Checkpoint restore recreates entities in different order, breaking GPU buffer index mapping | Assign a stable `AgentId` (monotonic u32) to each agent. Sort by AgentId before Parquet write. On restore, rebuild the `AgentId -> gpu_buffer_index` mapping. Never rely on hecs entity insertion order |
| egui desktop -> deck.gl web migration | Trying to port egui UI components to React/deck.gl incrementally while maintaining both frontends | Clean break. Build deck.gl dashboard from scratch using the gRPC/WebSocket API. Do not attempt to share UI state between egui and React. The API layer is the interface boundary |
| rayon + tokio in same binary | Using `block_on` inside a rayon thread (deadlocks tokio runtime) or using `tokio::spawn` for CPU-bound simulation work (starves async tasks) | Strict separation: rayon owns CPU-bound simulation work (physics, sorting, pathfinding). tokio owns I/O (gRPC, WebSocket, Parquet writes). Bridge via `tokio::sync::mpsc` channels. Never cross runtimes |

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Sending 280K agent positions over WebSocket at 10 Hz without spatial filtering | Browser tab crashes, WebSocket connection drops, Redis memory spikes | Viewport-based filtering: server only sends agents in client's visible bbox. Server-side density aggregation at low zoom | >50K agents without filtering; >10K agents without binary encoding |
| Single wgpu buffer for all agent data (no SoA) | Cannot update positions without re-uploading velocities, routes, profiles. Upload bandwidth becomes bottleneck | Structure-of-Arrays buffer layout. Separate GPU buffers for position, kinematics, route index. Update only changed buffers per frame | >100K agents with per-frame full-buffer uploads |
| GPU->CPU readback every frame for all agents | PCIe bandwidth consumed by 280K * 52 bytes = 14.5 MB per frame at 10 Hz = 145 MB/s | Read back only what the API needs: positions + speeds for visualization (280K * 12 bytes = 3.4 MB). Route/profile data stays GPU-resident. Use async readback (double-buffered map) | >100K agents with full-state readback at 10 Hz |
| Per-lane sorting on CPU before GPU dispatch | At 50K lanes, even with rayon, radix sort costs 2-5ms. Exceeds per-step CPU budget | Maintain sorted order incrementally. Agents rarely change lanes (<1%/step). Use insertion sort for lane-changed agents only. Or GPU-side bitonic sort | >20K lanes with full re-sort each step |
| CCH customization triggered by every prediction update | Prediction updates every 60 sim-seconds (600 steps), but intermediate partial updates also trigger customization. 6 customizations per minute at 3-10ms each | Batch prediction updates. Only trigger CCH customization when the full ensemble update completes, not on partial results. Use a dirty flag + update-on-next-step pattern | >10 customization triggers per sim-minute |
| Parquet checkpoint writes blocking sim loop | At 280K agents, serializing to Parquet takes 200-500ms. Sim loop stalls for 2-5 frames at 10 Hz | Write checkpoints asynchronously. Clone the state snapshot (cheap: 280K * 52 bytes = 14.5 MB memcpy), send to a background tokio task. Sim loop continues immediately | Any checkpoint write of >50ms without async offload |
| Redis pub/sub message accumulation for slow subscribers | Redis accumulates unsent messages for slow WebSocket clients. Memory grows unbounded. Redis OOM kills the process | Set per-client message TTL. If a client falls behind by >5 frames, drop intermediate frames and send only the latest. Monitor Redis `used_memory` and `pubsub_channels` metrics | >50 concurrent viewers with heterogeneous network speeds |

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| gRPC API without authentication for simulation control (start/stop/reset) | Anyone on the network can stop or corrupt a running simulation. Calibration runs ruined by accidental or malicious API calls | Add API key authentication at minimum. For internal deployment, use mTLS between services. Rate-limit destructive operations (reset, parameter change) |
| WebSocket endpoint accepts arbitrary viewport coordinates | Malicious client sends viewport covering entire world, server sends full 280K agent stream. 100 such clients = DoS | Server-side viewport validation. Clamp viewport to HCMC bounding box. Rate-limit viewport change requests (max 10/sec). Cap per-client bandwidth |
| Parquet checkpoint files contain full simulation state including calibration parameters | Checkpoint files shared externally expose proprietary calibration data (OD matrices, signal timings, demand profiles) | Separate state checkpoints (agent positions/speeds) from calibration data (parameters, OD matrices). Encrypt calibration data at rest. Add access control to checkpoint storage |
| Docker container runs wgpu as root | GPU device files (/dev/nvidia*) require specific permissions. Running as root is common workaround but exposes host filesystem | Use NVIDIA Container Toolkit's `--user` flag. Map only required device files. Run simulation process as non-root user inside container |

## UX Pitfalls

| Pitfall | User Impact | Better Approach |
|---------|-------------|-----------------|
| Showing 280K individual dots at city-wide zoom level | Visual noise -- individual agents are sub-pixel at city scale. Browser rendering slows to <5 FPS | Level-of-detail rendering: heatmap/flow-lines at city zoom, individual agents at street zoom. Switch at a defined zoom threshold. Common in deck.gl applications |
| No indication of sim-time vs wall-time ratio | User runs scenario expecting "real-time" but simulation is actually 5x faster or 0.5x slower. Results misinterpreted | Permanent display: "Sim: 07:32:15 | Wall: 0:04:12 | Speed: 1.8x". Flash when speed multiplier changes. Pause indicator must be unmissable |
| Dashboard loads empty while simulation initializes | First-time user sees blank map for 5-10 seconds during 280K agent spawn. Assumes system is broken | Show loading progress: "Spawning agents: 142K / 280K". Render road network immediately (from PMTiles). Add agents incrementally as they spawn |
| Multi-GPU partition boundaries visible as rendering artifacts | Agents near partition boundaries flicker or jump as they transfer between GPUs. Visible as a line across the map | Overlap boundary visualization by 50m. Both GPUs render agents in the overlap zone. Client-side deduplication by agent ID. Or: render from a single merged position buffer (CPU merges GPU outputs before sending to viz) |
| WebSocket disconnect not handled gracefully | User's browser loses connection (laptop sleep, network hiccup). Dashboard freezes on last received frame with no indication | Heartbeat ping/pong (every 5s). On disconnect: show "Reconnecting..." overlay. Auto-reconnect with exponential backoff. On reconnect: request full state snapshot, not just deltas |

## "Looks Done But Isn't" Checklist

- [ ] **Multi-GPU partitioning:** Often missing overlap zone for boundary agents. Agent crosses boundary but neither GPU claims it for 1 step -- agent disappears for a frame then reappears. Verify: track boundary-crossing agents, confirm zero missed steps.
- [ ] **CCH weight customization:** Often missing invalidation of cached shortest paths after weight update. Agents rerouted before the update continue on stale routes that may now be suboptimal. Verify: after customization, rerouted agents use fresh CCH queries.
- [ ] **Prediction ensemble:** Often missing the cold-start problem. Historical matcher has no history for the first simulated hour. BPR needs baseline free-flow speeds. Verify: first 600 sim-seconds produce reasonable predictions (test with known demand).
- [ ] **Parquet checkpoint restore:** Often missing GPU buffer re-upload. ECS state loads from Parquet but GPU buffers still contain stale data. The v1.0 PITFALLS.md flagged this -- it remains critical at 280K scale. Verify: restore checkpoint, advance 1 step, compare against a reference run.
- [ ] **WebSocket binary protocol:** Often missing endianness specification. Server (Rust, little-endian on x86) sends Float32Array bytes. Browser on ARM64 (big-endian possible) misinterprets. Verify: test with explicit endian check; use DataView on client.
- [ ] **Redis pub/sub spatial tiling:** Often missing tile boundary agents. An agent at the exact boundary of two tiles is sent in neither tile's channel. Verify: agents within 10m of tile boundaries appear in adjacent tiles (overlap).
- [ ] **gRPC streaming:** Often missing backpressure handling. If the gRPC client doesn't consume messages fast enough, tonic buffers grow unbounded. Verify: slow client test -- connect a gRPC client that reads 1 message/sec while server produces 10/sec.
- [ ] **Calibration GEH:** Often missing the distinction between "link volume" GEH and "turning movement" GEH. GEH < 5 on link volumes can mask terrible turning movement accuracy. Verify: compute GEH separately for link volumes and turning movements at key intersections.
- [ ] **Docker health check:** Often missing GPU health in container health check. Container reports "healthy" (HTTP 200 from API) but GPU compute has silently failed (device lost). Verify: health check endpoint runs a trivial GPU dispatch and reports GPU status.

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| GPU compute not wired into sim loop | MEDIUM | Remove CPU physics path. Wire GPU dispatch into main loop. Fix failing tests one by one. ~1-2 weeks depending on edge case count |
| Multi-GPU wgpu adapter failure | LOW | Fall back to single-GPU. Reduce target to 200K agents. Configuration change + performance tuning. ~2 days |
| Wave-front occupancy too low | MEDIUM | Implement EVEN/ODD + 3-pass correction (fallback from architecture). ~1 week to implement and validate |
| deck.gl attribute bottleneck | MEDIUM | Implement server-side binary attribute packing. Requires changes to both server (Rust binary serialization) and client (deck.gl data format). ~3-5 days |
| Fixed-point performance penalty | LOW | Fall back to float32 + @invariant. Accept statistical equivalence. ~1 day to revert shaders |
| Meso-micro velocity discontinuity | HIGH | Requires iterative tuning of buffer zone parameters, warm-up distances, and insertion logic. No quick fix -- this is a research problem. ~2-4 weeks of experimentation |
| Docker GPU access failure | LOW | Fix container configuration (env vars, Vulkan libs, ICD files). Well-documented problem. ~1 day |
| Redis pub/sub memory exhaustion | MEDIUM | Add per-client message TTL, implement frame dropping for slow clients, add monitoring alerts. ~2-3 days |
| CCH incorrect paths after customization | HIGH | Likely wrong node ordering. Must rebuild ordering with nested dissection. All shortcuts recomputed. ~3-5 days (same as v1.0 pitfall) |

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| GPU compute not in sim loop | Phase 1 (Foundation) | All v1.0 tests pass with GPU physics as sole driver. No CPU physics code path exists |
| Multi-GPU adapter failure | Phase 1 (Spike S2) | Trivial compute shader runs on 2 adapters from single process. 64KB buffer transfer < 0.1ms |
| Wave-front GPU occupancy | Phase 1 (Spike S1) | Wave-front achieves > 40% of naive parallel throughput on target hardware |
| deck.gl 280K point bottleneck | Phase 2 (Scale) | Dashboard maintains 30 FPS with 280K agents updating at 10 Hz. Input latency < 50ms |
| Fixed-point performance | Phase 3 (optional) | Full IDM formula in fixed-point within 50% of float32 throughput. If not, use float32 fallback |
| Meso-micro velocity discontinuity | Phase 3 (Calibration) | Travel time at meso-micro boundaries within 10% of full-micro reference. No artificial congestion visible |
| Docker GPU access | Phase 2 (early validation) | Minimal wgpu-in-Docker test passes: enumerate adapters, run trivial compute, read results |
| Redis pub/sub scaling | Phase 2 (WebSocket relay) | 100 concurrent viewers sustained for 10 minutes. Redis memory stable. Zero dropped connections |
| CCH ordering correctness | Phase 1-2 (Routing) | 1000 random queries match Dijkstra with 3 different weight configurations. Shortcut count < 3x edge count |
| Parquet checkpoint blocking | Phase 2 (Checkpoint) | Checkpoint write for 280K agents completes with <5ms sim loop stall (async offload) |
| WebSocket binary encoding | Phase 2 (API) | Client on ARM64 and x86 both render agents at correct positions from same binary stream |
| gRPC backpressure | Phase 2 (API) | Slow client test: server stable after 60 seconds with client consuming at 1/10th production rate |

## Sources

- [wgpu multi-adapter limitations -- WebGPU does not expose multi-GPU](https://wgpu.rs/)
- [wgpu in Docker -- request_adapter returns None with discrete GPUs (gfx-rs/wgpu#2123)](https://github.com/gfx-rs/wgpu/issues/2123)
- [wgpu multithreading performance issues (gfx-rs/wgpu#5525)](https://github.com/gfx-rs/wgpu/discussions/5525)
- [deck.gl performance best practices -- attribute generation bottleneck](https://deck.gl/docs/developer-guide/performance)
- [deck.gl real-time data update patterns (Discussion #6869)](https://github.com/visgl/deck.gl/discussions/6869)
- [deck.gl real-time data best practices (Discussion #6274)](https://github.com/visgl/deck.gl/discussions/6274)
- [LPSim: Large Scale Multi-GPU Traffic Simulation (2024)](https://arxiv.org/html/2406.08496v1)
- [GPU-accelerated Large-scale Simulator for Transportation (2024)](https://arxiv.org/html/2406.10661v1)
- [CCH paper -- node ordering must be metric-independent (Dibbelt et al., 2014)](https://arxiv.org/pdf/1402.0402)
- [RoutingKit CCH documentation -- best CH orders don't yield good CCHs](https://github.com/RoutingKit/RoutingKit/blob/master/doc/CustomizableContractionHierarchy.md)
- [Hybrid meso-micro traffic simulation transition artifacts (Burghout, KTH)](https://www.diva-portal.org/smash/get/diva2:14700/FULLTEXT01.pdf)
- [Meso-micro consistency requirements (ScienceDirect)](https://www.sciencedirect.com/science/article/pii/S187704281101411X)
- [Redis pub/sub WebSocket scaling pitfalls (Ably)](https://ably.com/blog/scaling-pub-sub-with-websockets-and-redis)
- [WebSocket scaling challenges (DEV Community)](https://dev.to/ably/challenges-of-scaling-websockets-3493)
- [NVIDIA Container Toolkit -- GPU driver capabilities](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/latest/docker-specialized.html)
- [wgpu buffer mapping and polling patterns](https://tillcode.com/rust-wgpu-compute-minimal-example-buffer-readback-and-performance-tips/)
- [Parquet Rust crate -- arrow-rs integration](https://arrow.apache.org/rust/parquet/index.html)
- [WebGPU double precision discussion -- no f64/i64 runtime support (gpuweb#2805)](https://github.com/gpuweb/gpuweb/issues/2805)

---
*Pitfalls research for: VELOS v1.1 Digital Twin Platform -- scaling from POC to production*
*Researched: 2026-03-07*
