# Stack Research: v1.1 Digital Twin Platform

**Domain:** GPU-accelerated traffic microsimulation -- scaling from desktop POC to web-based digital twin platform
**Researched:** 2026-03-07
**Confidence:** HIGH (core libraries verified via crates.io, official docs, and release notes)

## Scope

This document covers ONLY the stack additions and changes needed for v1.1. The v1.0 validated stack (Rust nightly, wgpu, hecs, petgraph, rstar, egui) is NOT re-researched. See the v1.0 STACK.md in git history for that analysis.

## Existing Stack -- Version Updates Required

These are already in the workspace but need version bumps for v1.1:

| Technology | v1.0 Version | v1.1 Target | Change Reason |
|------------|-------------|-------------|---------------|
| wgpu | 27 | 28.0.0 | Multi-GPU improvements, better compute dispatch. Already specified in arch docs. |
| tokio | (implicit) | 1.50.0 (LTS: 1.47.x) | Required by tonic 0.14 and axum 0.8. Use LTS 1.47.x for stability. |
| rayon | (implicit) | 1.11.0 | CCH construction parallelism, staggered reroute parallelism. |
| glam | 0.29 | 0.29 | No change needed. Still used for CPU-side vector math. |
| arc-swap | (not in v1.0) | 1.8.2 | Prediction overlay atomic swap pattern (ArcSwap<PredictionOverlay>). |
| serde | (not in workspace) | 1.0.228 | Required by parquet, postcard, serde_json, flatbuffers, and checkpoint metadata. |

## New Rust Crate Additions

### API Server Layer

| Crate | Version | Purpose | Integration Point |
|-------|---------|---------|-------------------|
| tonic | 0.14.3 | gRPC server for simulation control | velos-api crate. Defines VelosSimulation service per 05-visualization-api.md proto contract. Shares tokio runtime with axum. |
| tonic-build | 0.14.x | Protobuf codegen from .proto files | Build script in velos-api. Generates Rust types from proto/velos/v2/*.proto. |
| prost | 0.13.x | Protobuf runtime (required by tonic) | Auto-included via tonic. Message serialization/deserialization. |
| axum | 0.8.8 | REST + WebSocket gateway | velos-api crate. REST routes (GET /api/v1/status, etc.) and WebSocket relay for tile-based frame streaming. Path syntax: /{param}. |
| axum-extra | 0.10.x | WebSocket utilities, typed headers | WebSocket upgrade handling for tile-based pub/sub relay per 05-visualization-api.md. |
| tower | 0.5.x | Middleware (timeout, rate-limit, tracing) | Shared middleware between tonic and axum. Both are tower-based. |
| tower-http | 0.6.x | HTTP-specific middleware (CORS, compression) | CORS headers for deck.gl dashboard requests. Compression for REST responses. |

**Why tonic 0.14 (not 0.12):** v1.0 STACK.md listed tonic 0.12.x, but 0.14.3 is the current stable release. The jump from 0.12 to 0.14 includes performance improvements and better error types. No breaking API changes for our use case.

**Why axum 0.8 (not 0.7):** axum 0.8 released Jan 2025, is current stable. New path syntax /{param}. Drops #[async_trait] requirement. Matches tokio ecosystem version expectations.

### Data Storage and Export

| Crate | Version | Purpose | Integration Point |
|-------|---------|---------|-------------------|
| parquet | 57.x | Checkpoint snapshots + FCD/edge stats export | velos-output crate for columnar data export. velos-core for checkpoint save/restore. arrow-rs 57.0.0 introduced 4x faster thrift metadata parser. |
| arrow | 57.x | Arrow columnar format (required by parquet crate) | Used internally by parquet. Also enables zero-copy interop if downstream tools read Arrow directly. |
| serde_json | 1.0.x | Checkpoint metadata (meta.json), config files | velos-core checkpoint metadata, scenario config parsing. |
| flatbuffers | 25.12.19 | Binary WebSocket frame protocol | velos-api for tile-based agent position frames over WebSocket. 8 bytes per agent as defined in 05-visualization-api.md (TileFrame schema). |

**Why parquet/arrow (not just postcard for everything):** Parquet is the right choice for checkpoint and output because: (1) columnar layout matches ECS SoA pattern; (2) Zstd compression yields ~15MB for 280K agents; (3) readable by Python/R/DuckDB for downstream analysis without custom deserialization. Postcard stays for any internal IPC where schema portability does not matter.

**Why flatbuffers (not protobuf) for WebSocket frames:** FlatBuffers provides zero-copy deserialization -- critical for 10Hz 280K-agent frame streaming at ~32KB per viewport. Protobuf requires a full deserialize step. The 05-visualization-api.md spec already defines the TileFrame schema in FlatBuffers format.

### Redis Pub/Sub

| Crate | Version | Purpose | Integration Point |
|-------|---------|---------|-------------------|
| redis | 1.0.4 | Redis client for pub/sub tile frame fan-out | velos-api WebSocket relay pods. Simulation publishes per-tile frames to Redis channels. Relay pods subscribe to tiles matching client viewports. Use `features = ["tokio-comp", "aio"]` for async. |

**Why redis (not in-process channels):** v1.0 correctly avoided Redis for single-user desktop. v1.1 needs horizontal WebSocket scaling (100+ concurrent viewers per 05-visualization-api.md). Redis pub/sub decouples the simulation producer from N relay consumers. Stateless relay pods can scale independently.

### Prediction and Math

| Crate | Version | Purpose | Integration Point |
|-------|---------|---------|-------------------|
| ndarray | 0.17.1 | N-dimensional arrays for historical speed data | velos-predict HistoricalMatcher: Array3<f32> indexed by [edge_id][hour_of_day][day_type] per 03-routing-prediction.md. Also used in calibration for matrix operations. |
| arc-swap | 1.8.2 | Atomic swap for PredictionOverlay | velos-predict stores Arc<ArcSwap<PredictionOverlay>>. Background tokio task computes new predictions every 60s, swaps atomically. Simulation reads lock-free. |

**Why ndarray (not nalgebra):** ndarray is purpose-built for N-dimensional data arrays (our historical speed tables are 3D: edge x hour x day_type). nalgebra is a linear algebra library optimized for matrix multiplication and decomposition -- overkill for array indexing and interpolation.

### Calibration

| Crate | Version | Purpose | Integration Point |
|-------|---------|---------|-------------------|
| argmin | 0.10.x | Bayesian optimization framework | velos-calibrate crate. GEH/RMSE objective function optimized via CMA-ES or Nelder-Mead. argmin provides the optimization loop; we provide the cost function. |
| argmin-math | 0.4.x | Math backend for argmin (ndarray-based) | Required by argmin. Use ndarray backend (`features = ["ndarray_latest"]`). |

**Why argmin (not custom optimizer):** argmin provides production-quality optimization algorithms (Nelder-Mead, CMA-ES, L-BFGS, particle swarm) with built-in checkpointing and observers. Writing Bayesian optimization from scratch is error-prone. argmin's observer pattern also integrates naturally with our Prometheus metrics for tracking calibration convergence.

### Observability

| Crate | Version | Purpose | Integration Point |
|-------|---------|---------|-------------------|
| tracing | 0.1.41 | Structured logging throughout all crates | Replace env_logger/log with tracing spans and events. Instrument simulation steps, GPU dispatches, pathfinding queries with structured fields (step, sim_time, frame_time_ms). |
| tracing-subscriber | 0.3.x | Log formatting, filtering, output | Console output during development. JSON output in Docker deployment. Layer-based composition. |
| metrics | 0.24.x | Metrics facade (gauges, counters, histograms) | velos-core SimMetrics: frame_time_ms histogram, agent_count gauge, gridlock_events counter per 06-infrastructure.md. |
| metrics-exporter-prometheus | 0.16.x | Prometheus /metrics HTTP endpoint | Exposes metrics at :9090/metrics. Scraped by Prometheus container in Docker Compose. |

**Why tracing (not log/env_logger):** v1.0 uses log + env_logger which provides unstructured text logging. v1.1 needs structured fields (step=27150, frame_time_ms=8.2, reroute_count=12) for Grafana dashboards and debugging at 280K agent scale. tracing is the Rust ecosystem standard for this. It is backwards-compatible with log via tracing-log bridge.

**Why metrics + metrics-exporter-prometheus (not prometheus crate directly):** The metrics facade provides a clean API (counter!, gauge!, histogram!) decoupled from the exporter. If we later switch from Prometheus to Datadog or OpenTelemetry, only the exporter changes. The prometheus crate forces Prometheus-specific types into application code.

### Utility Crates

| Crate | Version | Purpose | Integration Point |
|-------|---------|---------|-------------------|
| chrono | 0.4.43 | Timestamp handling for checkpoints, demand profiles | Checkpoint naming (format!("checkpoint_{sim_time}_{timestamp}")). Time-of-day mapping for demand profiles and historical prediction. |
| smallvec | 1.15.1 | Stack-allocated small vectors for routes | Route struct uses SmallVec<[u32; 16]> per 01-simulation-engine.md ECS layout. Most routes are <16 edges, avoiding heap allocation. |
| geojson | 0.24.x | GeoJSON export for GIS tools | velos-output GeoJSON export (QGIS, ArcGIS compatibility). |
| quick-xml | 0.39.0 | SUMO FCD XML export | velos-output SUMO-compatible Floating Car Data export for ecosystem compatibility. |

### Docker and Infrastructure (not Rust crates)

| Technology | Version | Purpose | Notes |
|------------|---------|---------|-------|
| Redis | 7.x (Alpine image) | Pub/sub message broker for WebSocket scaling | redis:7-alpine Docker image. Minimal footprint. Only used for pub/sub, not as a database. |
| Prometheus | 2.48+ | Metrics collection and alerting | prom/prometheus:v2.48.0 Docker image. Scrapes /metrics from velos-api. |
| Grafana | 10.2+ | Dashboards and visualization | grafana/grafana:10.2.0. Pre-provisioned dashboards for simulation KPIs. |
| Nginx | Alpine | Static file server for PMTiles and dashboard | Serves hcmc.pmtiles via HTTP range requests. Also serves built deck.gl dashboard. |
| Docker Compose | 3.9 spec | Orchestration for POC deployment | Single-node multi-container. nvidia runtime for GPU passthrough. |

## Frontend Stack (dashboard/ directory -- TypeScript)

Replaces egui desktop UI with web-based analytics dashboard.

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| React | 19.x | Dashboard UI framework | Ecosystem maturity, deck.gl has first-class React bindings (@deck.gl/react). |
| TypeScript | 5.7+ | Type safety | Non-negotiable for any frontend. |
| Vite | 6.x | Build tool and dev server | Fast HMR, native ESM. Standard choice for React projects. |
| deck.gl | 9.2.x | GPU-accelerated 2D map visualization | Primary visualization. ScatterplotLayer for 280K agent dots at 60 FPS. HeatmapLayer for density. IconLayer for flow arrows. Built-in WebGL instancing. |
| @deck.gl/react | 9.2.x | React bindings for deck.gl | Declarative layer composition in JSX. |
| maplibre-gl | 5.19.x | Base map renderer (vector tiles) | Open-source Mapbox GL fork. Renders PMTiles via pmtiles:// protocol. Full style spec support. |
| pmtiles | 4.4.0 | PMTiles client-side loader | Loads vector tiles from static .pmtiles file via HTTP range requests. Zero tile server needed. |
| CesiumJS | 1.139.x | 3D visualization (optional/secondary) | Stakeholder demos. OSM building extrusions + terrain. Self-hosted tiles (no Cesium Ion dependency). |
| pnpm | 9.x | Package manager | Project convention. |

**Why deck.gl (not Mapbox GL layers or custom WebGL):** deck.gl is purpose-built for large-scale geospatial data visualization. Its ScatterplotLayer handles 200K+ points at 60 FPS using WebGL instancing -- exactly what we need for 280K agents. MapLibre alone cannot handle this point density performantly. Custom WebGL would be reinventing deck.gl.

**Why CesiumJS is secondary (not primary):** CesiumJS excels at 3D globe visualization but is heavier than deck.gl for 2D analytics. Traffic engineering analysis is done in 2D (heatmaps, flow arrows, congestion overlays). CesiumJS is for presentation/demos only -- defer implementation if time-constrained.

## Workspace Cargo.toml Additions

```toml
[workspace.dependencies]
# --- EXISTING (version bumps) ---
wgpu = "28"                                          # was "27"
tokio = { version = "1.47", features = ["full"] }    # explicit, LTS
rayon = "1.11"                                       # explicit version

# --- NEW: Serialization ---
serde = { version = "1", features = ["derive"] }
serde_json = "1"
flatbuffers = "25.12"
parquet = { version = "57", features = ["zstd"] }
arrow = "57"

# --- NEW: API Server ---
tonic = "0.14"
tonic-build = "0.14"
prost = "0.13"
axum = "0.8"
axum-extra = { version = "0.10", features = ["typed-header"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["cors", "compression-gzip"] }

# --- NEW: Redis ---
redis = { version = "1.0", features = ["tokio-comp", "aio"] }

# --- NEW: Prediction / Math ---
ndarray = "0.17"
arc-swap = "1.8"

# --- NEW: Calibration ---
argmin = "0.10"
argmin-math = { version = "0.4", features = ["ndarray_latest"] }

# --- NEW: Observability ---
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
metrics = "0.24"
metrics-exporter-prometheus = "0.16"

# --- NEW: Utility ---
chrono = { version = "0.4", features = ["serde"] }
smallvec = { version = "1.15", features = ["serde"] }
geojson = "0.24"
quick-xml = { version = "0.39", features = ["serialize"] }

# --- NEW: Checkpoint ---
postcard = { version = "1.1", features = ["alloc"] }
```

## Frontend package.json Additions

```bash
# Core visualization
pnpm add deck.gl @deck.gl/core @deck.gl/layers @deck.gl/react @deck.gl/aggregation-layers
pnpm add maplibre-gl pmtiles
pnpm add react react-dom

# 3D visualization (optional, defer if time-constrained)
pnpm add cesium @cesium/widgets

# Dev dependencies
pnpm add -D typescript vite @vitejs/plugin-react @types/react @types/react-dom
```

## Alternatives Considered

| Recommended | Alternative | Why Not Alternative |
|-------------|-------------|---------------------|
| tonic 0.14 (gRPC) | tarpc / capnproto-rpc | tonic is the de facto Rust gRPC implementation. Ecosystem tooling (grpcurl, Postman) works out of the box. tarpc lacks protobuf interop with non-Rust clients. |
| axum 0.8 (REST) | actix-web 4 | axum shares the tokio/tower ecosystem with tonic. Running both on one runtime is trivial. actix-web uses its own runtime, creating friction. |
| parquet 57 (storage) | postcard (binary) | Parquet enables downstream analysis in Python/DuckDB without custom deserialization. Postcard is better for pure-Rust internal IPC. Use both: postcard for internal, parquet for external. |
| flatbuffers (WebSocket) | protobuf (WebSocket) | FlatBuffers provides zero-copy reads -- critical for 10Hz frame streaming. Protobuf requires full deserialization. FlatBuffers schema matches the compact TileFrame format (8 bytes/agent). |
| redis 1.0 (pub/sub) | NATS / ZeroMQ | Redis is already in the Docker Compose stack. Adding another broker increases ops complexity for no benefit at POC scale (100 viewers). NATS would be warranted at 10K+ viewers. |
| ndarray 0.17 (arrays) | nalgebra | ndarray is for N-dimensional data indexing. Our historical speed table is Array3<f32>. nalgebra is for linear algebra (matrix multiply, decomposition) -- not our use case. |
| deck.gl 9.2 (2D viz) | Mapbox GL JS | Mapbox GL is proprietary with usage-based pricing. MapLibre (open fork) handles base maps. deck.gl handles the data overlay (280K points). |
| metrics facade | prometheus crate | metrics crate provides a backend-agnostic API. Avoids vendor lock-in to Prometheus-specific types in application code. |
| tracing | log + env_logger | tracing provides structured spans and fields. Essential for debugging 280K-agent simulations. log only provides unstructured text. |

## What NOT to Add

| Avoid | Why | Already Have / Use Instead |
|-------|-----|---------------------------|
| Tauri | v1.1 moves from desktop to web dashboard. No native window shell needed. Simulation runs as a headless service with web frontend. | winit (kept for dev-mode rendering), axum (web server), deck.gl (dashboard) |
| egui for production UI | egui was right for v1.0 desktop POC. v1.1 dashboard needs charts, maps, complex layouts that egui handles poorly. | deck.gl + React dashboard. Keep egui only as optional dev-mode overlay. |
| Arrow IPC / Python bridge | Explicitly rejected in 03-routing-prediction.md. In-process Rust-native ensemble is faster and simpler. | Built-in BPR + ETS + historical ensemble in velos-predict |
| Martin tile server | PMTiles static files served by Nginx eliminate the need for a tile server process. | PMTiles + Nginx (zero additional services) |
| Nominatim geocoding | Not needed for POC. Agents use edge IDs, not addresses. | Direct edge ID references |
| 3DCityDB | No CityGML dataset exists for HCMC. OSM building:levels extrusions in deck.gl/CesiumJS suffice. | deck.gl ColumnLayer for building extrusions |
| Kubernetes | Docker Compose is sufficient for single-node 2-4 GPU deployment at POC scale. K8s adds operational complexity without benefit. | Docker Compose 3.9 |
| NATS / Kafka / RabbitMQ | Redis pub/sub is sufficient for tile-based frame fan-out at 100 viewers. Additional message brokers add unnecessary ops burden. | Redis 7 (already in stack for pub/sub) |
| bincode | Unmaintained (RUSTSEC-2025-0141). | postcard 1.1 for internal binary serialization |
| reqwest / hyper (client) | No outbound HTTP calls needed. Simulation is self-contained. Calibration data loaded from local files. | Direct file I/O |
| diesel / sqlx | No SQL database in the architecture. All state is ECS + Parquet files. | Parquet + arrow-rs |
| bevy / bevy_ecs | Bevy pulls in an entire game engine. hecs is the right minimal ECS. | hecs 0.11 (already validated) |
| fast_paths (CH crate) | Does not support dynamic weight customization -- the whole reason we need CCH. | Custom CCH implementation |

## Crate-to-Feature Mapping

Shows which new crate serves which v1.1 feature:

| v1.1 Feature | New Crates Required |
|--------------|---------------------|
| Multi-GPU wave-front dispatch | wgpu 28 (bump), smallvec |
| Fixed-point arithmetic | No new crates (pure WGSL + Rust math) |
| CCH dynamic pathfinding | No new crates (custom implementation on petgraph) |
| BPR + ETS + historical prediction | ndarray, arc-swap, chrono |
| Meso-micro hybrid simulation | No new crates (pure Rust math in velos-meso) |
| deck.gl web visualization | deck.gl, maplibre-gl, pmtiles (frontend) |
| CesiumJS 3D visualization | cesium (frontend) |
| gRPC API | tonic, tonic-build, prost |
| REST/WebSocket API | axum, axum-extra, tower, tower-http, flatbuffers |
| Redis pub/sub scaling | redis |
| Docker deployment | No Rust crates (infrastructure-only) |
| Prometheus/Grafana monitoring | metrics, metrics-exporter-prometheus, tracing, tracing-subscriber |
| Parquet checkpoint/output | parquet, arrow, serde_json, chrono |
| GEH/RMSE calibration | argmin, argmin-math, ndarray |
| Emissions modeling (HBEFA) | No new crates (lookup tables in Rust) |
| Scenario DSL | No new crates (parser built with Rust standard library or nom if complex) |
| FCD/GeoJSON/CSV export | geojson, quick-xml |
| PMTiles map serving | pmtiles (frontend), Nginx (infrastructure) |
| Pedestrian adaptive workgroups | No new crates (GPU compute in WGSL) |
| Bus dwell time / bicycle agents | No new crates (agent model math in velos-vehicle) |

## Version Compatibility Matrix

| Package A | Compatible With | Notes |
|-----------|-----------------|-------|
| wgpu 28.0 | naga 28.0 (bundled) | Versions locked in wgpu monorepo. |
| tonic 0.14 | prost 0.13, tokio 1.47+ | tonic-build codegen produces prost 0.13 compatible code. |
| axum 0.8 | tokio 1.47+, tower 0.5 | Shares tower middleware with tonic. Both on same tokio runtime. |
| parquet 57 | arrow 57 | Version-locked in apache/arrow-rs monorepo. Always use matching versions. |
| argmin 0.10 | ndarray 0.17 via argmin-math 0.4 | argmin-math bridges argmin to ndarray. Use `ndarray_latest` feature. |
| tracing 0.1 | tracing-subscriber 0.3 | Stable pair. tracing-log 0.2 bridges log crate compatibility. |
| metrics 0.24 | metrics-exporter-prometheus 0.16 | Version pair maintained together. |
| deck.gl 9.2 | maplibre-gl 5.x | deck.gl uses MapView with maplibre-gl as base map renderer. |
| deck.gl 9.2 | React 19 | @deck.gl/react 9.2.x supports React 18 and 19. |

## Confidence Assessment

| Area | Confidence | Rationale |
|------|------------|-----------|
| API server (tonic + axum) | HIGH | Both are mature, production-proven, same tokio ecosystem. Verified versions on crates.io. |
| Parquet/Arrow for checkpoints | HIGH | arrow-rs 57 is actively maintained by Apache. Parquet is the standard columnar format. |
| FlatBuffers for WebSocket | MEDIUM | flatbuffers crate works but has less community usage than protobuf. Schema maintenance is an extra step. Verify FlatBuffers read performance in JavaScript matches expectations. |
| Redis pub/sub scaling | HIGH | redis-rs 1.0 is stable. Redis pub/sub for fan-out is a well-known pattern. Straightforward implementation. |
| deck.gl visualization | HIGH | deck.gl 9.x handles 200K+ points at 60 FPS. Well-documented, active development (9.2.11 released days ago). |
| CesiumJS 3D | MEDIUM | CesiumJS works but is heavy. Self-hosting tiles (no Cesium Ion) requires careful setup. Mark as optional/deferrable. |
| argmin calibration | MEDIUM | argmin is the best Rust optimization crate but has a smaller community than Python scipy.optimize. Verify CMA-ES convergence on GEH objective before committing. |
| Custom CCH pathfinding | MEDIUM | No off-the-shelf Rust CCH crate. Algorithm is well-documented academically but custom implementation is significant engineering. Highest risk item in the stack. |
| Observability (tracing + metrics) | HIGH | tracing and metrics are the Rust ecosystem standards. Prometheus integration is well-documented. |

## Sources

- [tonic crates.io](https://crates.io/crates/tonic) -- version 0.14.3 verified
- [tonic docs.rs](https://docs.rs/crate/tonic/latest) -- 0.14.3 API docs
- [axum 0.8 announcement](https://tokio.rs/blog/2025-01-01-announcing-axum-0-8-0) -- breaking changes documented
- [axum crates.io](https://crates.io/crates/axum) -- version 0.8.8 verified
- [parquet crates.io](https://crates.io/crates/parquet) -- version 57.x verified
- [Apache Arrow Rust 57.0.0 release](https://arrow.apache.org/blog/2025/10/30/arrow-rs-57.0.0/) -- 4x faster metadata parser
- [flatbuffers crates.io](https://crates.io/crates/flatbuffers) -- version 25.12.19 verified
- [redis crates.io](https://crates.io/crates/redis) -- version 1.0.4 verified
- [ndarray crates.io](https://crates.io/crates/ndarray) -- version 0.17.1 verified
- [arc-swap crates.io](https://crates.io/crates/arc-swap) -- version 1.8.2 verified
- [argmin website](https://argmin-rs.org/) -- optimization algorithms documented
- [tracing crates.io](https://crates.io/crates/tracing) -- version 0.1.41 verified
- [metrics-exporter-prometheus crates.io](https://crates.io/crates/metrics-exporter-prometheus) -- version 0.16+ verified
- [deck.gl npm](https://www.npmjs.com/package/deck.gl) -- version 9.2.11 verified
- [maplibre-gl npm](https://www.npmjs.com/package/maplibre-gl) -- version 5.19.0 verified
- [pmtiles npm](https://www.npmjs.com/package/pmtiles) -- version 4.4.0 verified
- [CesiumJS npm](https://www.npmjs.com/package/cesium) -- version 1.139.1 verified
- [tokio crates.io](https://crates.io/crates/tokio) -- version 1.50.0, LTS 1.47.x verified
- [rayon crates.io](https://crates.io/crates/rayon) -- version 1.11.0 verified
- [chrono crates.io](https://crates.io/crates/chrono) -- version 0.4.43 verified
- [smallvec crates.io](https://crates.io/crates/smallvec) -- version 1.15.1 verified
- [quick-xml crates.io](https://crates.io/crates/quick-xml) -- version 0.39.0 verified
- [serde crates.io](https://crates.io/crates/serde) -- version 1.0.228 verified

---
*Stack research for: VELOS v1.1 Digital Twin Platform -- stack additions for scaling from desktop POC to web-based 280K-agent simulation platform*
*Researched: 2026-03-07*
