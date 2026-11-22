# Fracture Vendor-Neutral Infrastructure Audit (VENDORS.md)

Fracture serves as an adversarial, vendor-neutral referee for vector databases. To guarantee absolute metrological integrity, avoid synthetic "clean-room" biases, and eliminate artificial handicaps, all target database configurations, index parameters, and kernel resource boundaries are audited and publicly documented here.

---

## 1. Vendor Referee Invariants

1. **Equal Tuning Budgets:** Every engine is allocated identical CPU core allocations and memory ceilings using Linux `cgroups v2`.
2. **Production Indexing Parameters:** Indices are built using maintainer-recommended production configurations (e.g. HNSW `M=16`, `ef_construction=64-100`) rather than minimal test presets.
3. **No Hidden Background Quotas:** Index builds, segment compactions, and vacuum runs must execute within the target container's assigned cgroup.
4. **Transparent Audit Trail:** Database maintainers can submit pull requests to update configurations or adapter client bindings as long as they adhere to the multi-dimensional testing criteria.

---

## 2. Target Database Configuration Audit

### A. Qdrant
* **Engine Type:** Dedicated Vector Database (Rust)
* **Version:** `qdrant/qdrant:v1.12.1`
* **Network Protocol:** HTTP / REST on port 6333, gRPC on port 6334
* **Vector Schema:**
  - Collection: `fracture_bench`
  - Distance Metric: `Cosine`
  - Vector Dimension: 128 (configurable)
* **HNSW Index Parameters:**
  - `m`: 16
  - `ef_construct`: 100
  - `full_scan_threshold`: 10000
  - `on_disk`: false (in-memory graph)
* **Payload Indexing:**
  - Keyword/Integer payload index on `modulo_10` and `modulo_1000` attributes to enable single-stage bitset filter pushdown.
* **Kernel Cgroup v2 Budget:**
  - `memory.max`: `2048M` (2.0 GiB)
  - `cpu.max`: `200000 100000` (2.0 physical CPU cores)
* **Fixture Definition:** [`fixtures/docker-compose.qdrant.yml`](file:///fixtures/docker-compose.qdrant.yml)

---

### B. pgvector (PostgreSQL 16)
* **Engine Type:** Relational ACID Database Extension (C / PostgreSQL)
* **Version:** `pgvector/pgvector:0.8.0-pg16`
* **Network Protocol:** PostgreSQL wire protocol on port 5433 (remapped from 5432 to avoid host SCRAM conflicts)
* **Authentication:** `POSTGRES_HOST_AUTH_METHOD=trust`
* **Vector Schema:**
  ```sql
  CREATE EXTENSION IF NOT EXISTS vector;
  CREATE TABLE IF NOT EXISTS fracture_bench (
      id BIGINT PRIMARY KEY,
      embedding vector(128),
      category INT
  );
  CREATE INDEX IF NOT EXISTS fracture_hnsw_idx 
  ON fracture_bench USING hnsw (embedding vector_cosine_ops) 
  WITH (m = 16, ef_construction = 64);
  ```
* **PostgreSQL Engine Tuning:**
  - `shared_buffers`: `1024MB`
  - `work_mem`: `64MB`
  - `maintenance_work_mem`: `512MB`
  - `hnsw.ef_search`: `64`
* **Kernel Cgroup v2 Budget:**
  - `memory.max`: `2048M` (2.0 GiB)
  - `cpu.max`: `200000 100000` (2.0 physical CPU cores)
* **Fixture Definition:** [`fixtures/docker-compose.pgvector.yml`](file:///fixtures/docker-compose.pgvector.yml)

---

### C. Milvus
* **Engine Type:** Distributed Cloud-Native Vector Database (Go / C++ Knowhere)
* **Version:** `milvusdb/milvus:v2.4.21` (Standalone)
* **Network Protocol:** HTTP / gRPC on port 19530
* **Architecture:**
  - Milvus Coordinator & QueryNode
  - `etcd` for cluster metadata
  - `MinIO` for object storage persistence
* **Vector Index Parameters:**
  - Index Type: `HNSW`
  - Metric Type: `COSINE`
  - `M`: 16
  - `efConstruction`: 64
  - `ef`: 64 (search runtime)
* **Segment Thresholds:**
  - Max segment size: `512MB`
* **Kernel Cgroup v2 Budget:**
  - `memory.max`: `4096M` (4.0 GiB)
  - `cpu.max`: `200000 100000` (2.0 physical CPU cores)
* **Fixture Definition:** [`fixtures/docker-compose.milvus.yml`](file:///fixtures/docker-compose.milvus.yml)

---

### D. LanceDB
* **Engine Type:** Embedded Columnar Vector Engine (Rust / Apache Arrow)
* **Version:** `lancedb:v0.13.0` (embedded / `lancedb/lancedb:v0.13.0`)
* **Storage Format:** Apache Arrow columnar tables stored in local NVMe / filesystem directory
* **Execution Boundary:** In-process embedded execution, eliminating client-server IPC overhead.
* **Index Configuration:**
  - Index Type: `IVF_PQ` or flat columnar linear scans for small corpora.
  - Vector Metric: `Cosine`
* **Kernel Boundary:** Shares host runner memory space; measured via resident set size (RSS).

---

### E. Mock Engine (Control Baseline)
* **Engine Type:** In-Memory Lock-Free Reference Baseline (Rust)
* **Purpose:**
  - Calibrate the hardware pacer's Poisson arrival generator.
  - Measure baseline operating system timer jitter (`quanta` TSC).
  - Verify zero-contention crossbeam queue dispatching.
* **Storage:** In-memory concurrent map with cosine distance SIMD kernels.

---

## 3. Maintainer Pre-Release Configuration Review Protocol

To avoid publishing benchmarks with suboptimal flags, mismatched connection pools, or unfair memory ceilings, Fracture mandates an open pre-release configuration review with engine maintainers.

### Review Principles
1. **Deterministic Index Creation:** Any recommended index tuning must be applied via adapter initialization code (`VectorEngineAdapter::init`) or the Docker compose fixture in `fixtures/`.
2. **No Memory Limit Bypasses:** Container definitions must retain cgroup v2 memory ceilings to ensure fair comparison under working-set pressure.
3. **Open-Source Client Libraries:** All client adapter bindings must be native Rust or accessible via standardized C ABI / HTTP / gRPC protocols.
4. **Transparent Audit Trail:** All maintainer suggestions, discussions, and tuning updates are committed directly into `VENDORS.md` with links to public GitHub Discussions or pull requests.

### Standard Maintainer Outreach Template
Before publishing flagship comparison charts or official benchmark reports, Fracture reaches out to each target engine's maintainers (via Discord, GitHub Discussions, or issue trackers) using this standardized template:

```text
Subject: Fracture Benchmark Configuration Review & Fair Tuning Audit — [Engine Name]

Dear [Engine Name] Maintainers and Core Team,

I am conducting an empirical systems study on vector database performance under continuous write churn and memory pressure using Fracture (https://github.com/switchstonedev/fracture-bench).

Before publishing our flagship Pareto frontiers and empirical benchmarks, we want to ensure [Engine Name] is configured completely fairly and reflects your recommended production best practices.

Our currently audited configuration for [Engine Name] is:
- Docker Image: [e.g. qdrant/qdrant:v1.8.0 / pgvector:pg16]
- Index Type & Build Parameters: [e.g. HNSW M=16, ef_construct=100]
- Query-time Parameters: [e.g. ef_search=64, k=10]
- Linux Cgroup v2 Budget: [e.g. memory.max = 2048 MB, cpu.max = 2.0 cores]
- Workload Profile: 70% Queries, 20% Upserts, 10% Deletes under open-loop Poisson arrival

Specific Questions for Your Team:
1. Are these index build and query parameters representative of production recommendations for a churn-heavy workload under this memory ceiling?
2. Are there specific engine flags or background compaction settings that should be enabled or tuned to handle concurrent deletions cleanly?
3. Is our connection pooling and network wire format configured optimally for your client interface?

You can review our complete adapter implementation in:
https://github.com/switchstonedev/fracture-bench/tree/main/crates/fracture-adapters/src/[engine].rs

We welcome your feedback and pull requests. If you would like us to re-run our automated reproduction pipeline with your suggested settings, we will gladly incorporate them and credit your team in the benchmark reports and repository.
```

---

## 4. Jepsen-Style Responsible Disclosure Runbook for Performance & Recall Anomalies

When Fracture uncovers an unexpected performance cliff, severe memory leak, or catastrophic recall collapse in a database engine, we adhere strictly to the responsible disclosure playbook established by Jepsen:

1. **Private Maintainer Heads-Up (2-to-4 Week Embargo):**
   Prior to public disclosure on social platforms or preprints, the full reproduction bundle is shared privately with the vendor's engineering leadership or security team.
2. **Deterministic Reproduction Bundle Provided:**
   - **Binary WAL Trace (`.wal`):** The exact immutable sequence of operations that triggered the failure, including nanosecond timestamps and vector embeddings.
   - **Isolated Docker Reproduction Script:** A single shell script spinning up the exact container version and running the reproduction in < 60 seconds.
   - **Kernel Subsystem Telemetry Logs:** High-frequency Linux PSI memory stall data, workingset refault counters, and I/O Write Amplification Factor logs.
3. **Collaborative Verification & Postscript Remediation:**
   Maintainers are invited to analyze the trace and supply an experimental branch, patch, or updated Docker tag. Fracture re-runs the benchmark against the patched engine, documenting both the original anomaly and the verified vendor remediation in the final benchmark reports.

