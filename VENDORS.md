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
* **Version:** `qdrant/qdrant:v1.8.0`
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
* **Version:** `pgvector/pgvector:pg16`
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
* **Version:** `milvusdb/milvus:v2.3.4` (Standalone)
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
* **Version:** `lancedb` (embedded)
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

## 3. Maintainer Configuration Review Protocol

Database maintainers wishing to audit, refine, or optimize their engine's configuration in Fracture must adhere to the following rules:

1. **Deterministic Index Creation:** Any recommended index tuning must be applied via adapter initialization code (`VectorEngineAdapter::init`) or the Docker compose fixture.
2. **No Memory Limit Bypasses:** Container definitions must retain cgroup memory ceilings to ensure fair comparison under working-set pressure.
3. **Open-Source Client Libraries:** All client adapter bindings must be native Rust or accessible via standardized C ABI / HTTP / gRPC protocols.
