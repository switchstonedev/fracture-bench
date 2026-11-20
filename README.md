# Fracture
> High-pressure vector database benchmarking by SwitchStone

[![Crates.io](https://img.shields.io/crates/v/fracture-bench.svg)](https://crates.io/crates/fracture-bench)
[![Documentation](https://docs.rs/fracture-bench/badge.svg)](https://docs.rs/fracture-bench)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20Windows%20%7C%20macOS-lightgrey.svg)]()
[![Toolchain](https://img.shields.io/badge/rust-1.75%2B-orange.svg)]()
[![CI](https://github.com/switchstonedev/fracture-bench/actions/workflows/ci.yml/badge.svg)](https://github.com/switchstonedev/fracture-bench/actions)

Fracture is an adversarial, open-loop benchmarking harness for vector search engines. It replaces synthetic "clean room" evaluations with realistic production workloads: continuous 70/20/10 mutation churn, hard deletes, HNSW graph fragmentation (dead-node tombstones), stratified metadata filtering, and strict kernel cgroups v2 memory budgets.

---

## 1. System Architecture

```mermaid
flowchart TD
    subgraph Host ["Shielded Benchmark Server"]
        subgraph Core0 ["Core 0 (OS Shield)"]
            OS[Linux Interrupts & Kernel SoftIRQs]
        end

        subgraph Core1 ["Core 1 (Pacer - isolcpus)"]
            Pacer[fracture-pacer<br/>quanta TSC + Inverse-CDF]
            WAL[Binary WAL Writer<br/>Aligned 32B Header]
        end

        subgraph Cores23 ["Cores 2-3 (Client Workers)"]
            Channel[Bounded Lock-Free Queue<br/>Saturation / Drop Accounting]
            Workers[Tokio Async Dispatchers<br/>Dual HdrHistograms]
        end

        subgraph Cores47 ["Cores 4-7 (Target Cgroup v2)"]
            TargetDB[(Target Vector DB<br/>Qdrant / Milvus / pgvector / LanceDB)]
            Telemetry[fracture-telemetry<br/>Persistent-fd /sys/fs/cgroup Scraper]
        end
    end

    subgraph OfflineAnalysis ["Offline Verification & Scoring"]
        GroundTruth[fracture verify<br/>Rayon + AVX2 Linear Scan]
        Report[generate_report.py<br/>Dual-CDF & Pareto Frontiers]
    end

    Pacer -->|DispatchTicket| Channel
    Pacer -->|Append Mutation LSN| WAL
    Channel -->|Non-blocking Receive| Workers
    Workers -->|Queries & Churn Writes| TargetDB
    Telemetry -.->|Poll cgroups stats| TargetDB
    WAL --> GroundTruth
    Workers --> Report
    Telemetry --> Report
    GroundTruth --> Report
```

Fracture isolates load generation, target execution, kernel telemetry, and mathematical scoring into independent subsystems to eliminate measurement perturbation:

*   **`fracture-pacer`:** High-precision arrival scheduler running on an isolated, pinned CPU core. Uses hardware Timestamp Counter (`quanta`) to drive Poisson arrivals with sub-microsecond jitter at 50,000 QPS. Supports FIFO sliding-window and cluster-targeted adversarial churn.
*   **`fracture-core`:** Domain types, 64-byte cache-line aligned binary Write-Ahead Log (WAL), contiguous `VectorArena` with 64-bit visibility bitset skipping, dataset readers (.fvecs, .bvecs), and dual-bound linearizability classification.
*   **`fracture-telemetry`:** Zero-allocation Linux `/sys/fs/cgroup` parser sampling page faults, workingset refaults, CFS CPU throttling, Linux PSI (`memory.pressure`), and Write Amplification Factor (`io.stat`) in under 2 µs per sample using persistent file descriptors.
*   **`fracture-adapters`:** Unified client trait implementations for Qdrant (gRPC/HTTP), pgvector (zero-text binary wire format code 1), Milvus, and LanceDB.
*   **`fracture-bench`:** Main executable harness crate publishing the `fracture` CLI with `run`, `seed`, `verify`, `profile`, `doctor`, `cliff-finder`, and `rightsize` subcommands.

---

## 2. Core Invariants

1. **Elimination of Coordinated Omission:**
   Fracture tracks dual histograms per worker task:
   *   `service_time`: Network round-trip duration (t_recv - t_sent).
   *   `total_response_time`: True end-to-end latency including client queue delay (t_recv - t_intended).
2. **Deterministic Point-in-Time Ground Truth & Dual-Bound Linearizability:**
   Ground truth is never evaluated against mutable live state. Mutations are appended to an immutable binary WAL with monotonic Log Sequence Numbers (LSNs) and ACK boundaries. Queries classify candidate spaces into `S_committed` and `S_inflight` under neutral scoring rules.
3. **Decoupled Ground-Truth Computation & Arena Locality:**
   Exact k-NN scans are executed offline using contiguous 64-byte aligned vector arenas with 64-bit active visibility bitsets, skipping tombstoned candidate blocks zero-copy.
4. **Three-Tier Reproduction Hierarchy (ACM/IEEE Badging Standards):**
   *   **Level 1 — Dry Run (`--tier dry_run` / `--dry-run`):** 10-30s wiring and environment sanity check with zero compute load.
   *   **Level 2 — Sample / Pilot (`--tier sample` / `--sample`):** 1-3 min pilot run on accessible engines yielding initial ground-truth metrics without resource lockup.
   *   **Level 3 — Full Benchmark (`--tier full` / `--full`):** Complete QPS sweeps across all engines when dedicated compute infrastructure is available.
5. **Standardized Research Placeholder Policy:**
   Any unexecuted or unreachable engine metric is recorded strictly as `{metric_originTech}` (e.g. `{achievedQps_milvus}`, `{recall10_pgvector}`). Fabricated numbers are strictly forbidden.
6. **Stratified Hashed Modulo Selectivity:**
   Metadata filter queries are partitioned into three calibrated cohorts using seeded hash mixers (`hash_id_seeded(id, seed) % modulo`) to destroy sequential insertion order correlation:
   *   High Selectivity: ~60% match rate (`modulo 10 in [0..5]`).
   *   Medium Selectivity: ~10% match rate (`modulo 10 == 0`).
   *   Needle-in-a-Haystack: ~0.1% match rate (`modulo 1000 == 42`).

---

## 3. Quick Start

### 60-Second Frictionless Demo (Docker Compose)
Run a complete Level 1 Dry Run against Mock and Qdrant in under 30 seconds without installing Rust or configuring local host dependencies:
```bash
docker compose up --build
```
This single command spins up Qdrant, verifies network connectivity, executes dry-run validation, and generates publication artifacts directly into `reports/`:
* `reports/figures/dual_cdf.svg` (Dual-CDF Tail Latency curve)
* `reports/figures/pareto_frontier.svg` (QPS vs Recall@10 Pareto frontier)
* `reports/benchmark_summary.md` (Summary table)

### Pre-Built Static Release Binaries
Pre-compiled static binaries for Linux (x86_64 musl), macOS (Apple Silicon / Intel), and Windows (x86_64) are built on every release tag and available on GitHub Releases:
```bash
# Download latest Linux static release binary
curl -sSL https://github.com/switchstonedev/fracture-bench/releases/latest/download/fracture-linux-x86_64 -o fracture
chmod +x fracture
./fracture --help
```

### Installation

#### Install via Cargo (crates.io)
Publishing as `fracture-bench` installs the `fracture` binary directly:
```bash
cargo install fracture-bench
```

#### Build from Source
```bash
git clone https://github.com/switchstonedev/fracture-bench.git
cd fracture-bench
cargo build --release
```
The compiled binary is available at `./target/release/fracture` (or `./target/release/fracture.exe` on Windows).

### Single-Entrypoint Reproduction Harness
```bash
# Dry run: ultra-fast validation (< 30s)
python scripts/reproduce.py --tier dry_run

# Sample run: 1-3 min pilot evaluation on accessible engines
python scripts/reproduce.py --tier sample

# Full matrix sweep (for dedicated compute environments)
python scripts/reproduce.py --tier full
```

### Self-Calibration Jitter Gate (Doctor Receipts)
Verify host timer resolution and ensure pacer scheduling does not distort benchmark measurements:
```bash
./target/release/fracture doctor --target-qps 50000 --duration-sec 5
```
**Empirical Receipts (50,000 QPS Sustained):**
* **p50 (Median Jitter):** 0.03 µs (30 nanoseconds)
* **p90 Jitter:** 0.35 µs (dedicated pinned Linux) / 40.83 µs (unpinned desktop)
* **p99 Jitter:** 0.78 µs (dedicated pinned Linux) / 485.38 µs (unpinned desktop)
* **p99.9 Jitter:** 1.45 µs (dedicated pinned Linux) / 785.41 µs (unpinned desktop)
* **Measurement Distortion:** Jitter contributes < 0.01% error to database response latencies.

See [`docs/calibration_receipt.md`](file:///docs/calibration_receipt.md) for full quantile tables and distribution receipts.

### Pre-populate Corpus (Bulk Ingestion)
```bash
./target/release/fracture seed \
    --target qdrant \
    --count 100000 \
    --dimension 128 \
    --batch-size 500 \
    --wal-path ./runs/qdrant_seed.wal
```

### Run an Open-Loop Benchmark
```bash
./target/release/fracture run \
    --target qdrant \
    --qps 10000 \
    --workload churn-70-20-10 \
    --duration-sec 5
```

### Replay WAL for Offline Recall@K Verification
```bash
./target/release/fracture verify \
    --wal-path ./runs/qdrant_seed.wal \
    --cutoff-lsn 1000 \
    --k 10 \
    --target qdrant \
    --linearizable \
    --output ./results/qdrant_recall.json
```

### RAM Working-Set Cliff Finder
```bash
# Pinpoint memory exhaustion where page cache thrashing occurs
./target/release/fracture cliff-finder --target qdrant --initial-mb 4096 --min-mb 512
```

### RAM Budget Rightsizing Analyzer
```bash
# Evaluate vector count feasibility against an assigned RAM budget
./target/release/fracture rightsize --target pgvector --count 250000 --dimension 768 --ram-budget-mb 2048
```

---

## 4. Threats to Validity & Known Limitations

To provide complete metrological transparency and guard against benchmark over-generalization, Fracture explicitly documents its operational boundaries:

1. **Single-Node Load Generation Ceilings:** Open-loop arrival pacing holds sub-microsecond jitter up to 50,000 QPS. Beyond 100,000 QPS from a single physical host, Linux TCP socket buffer allocations (`SO_SNDBUF`/`SO_RCVBUF`) and loopback networking become the queuing bottleneck rather than database execution.
2. **Reliance on Linux Cgroups v2 & Invariant TSC:** Full high-frequency PSI memory pressure and I/O Write Amplification Factor tracking require Linux kernel 5.15+ unified cgroups v2. On Windows and macOS, telemetry falls back to process-level RSS without refault or CFS throttling accounting.
3. **Lack of Hybrid Sparse-Dense Workloads:** Fracture currently evaluates dense floating-point vector representations with stratified metadata filters. Hybrid workloads (BM25 + SPLADE + dense HNSW) are planned for future workload profiles.
4. **Single-Node Consistency vs. Distributed Consensus Lag:** Dual-bound linearizability audits client-observed ACK boundaries. In distributed multi-node clusters (e.g. Milvus QueryNodes with Raft/etcd), internal replica synchronization lag can cause temporary read staleness, which Fracture flags as recall loss without isolating internal network hops.
5. **Uniform Hashed Modulo Selectivity vs. Power-Law Skew:** Seeded hashed modulo filters guarantee mathematically provable cohort selectivities (60%, 10%, 0.1%), but do not capture correlated Zipfian multi-tenant key skews found in some production clusters.

See [`ARCHITECTURE.md`](file:///ARCHITECTURE.md#10-threats-to-validity--known-limitations) for full analysis.

---

## 5. Academic Publication & Artifact Badging

Fracture is designed for empirical systems and benchmarking venues:
* **Target Venues:** VLDB (Experiment, Analysis & Benchmark track), SIGMOD (DBTest Workshop), and ACM ICPE.
* **ACM Artifact Evaluation Badges Targeted:**
  - *Artifacts Available:* Public Apache 2.0 repository with complete cryptographic SHA-256 manifest (`sha256sums.txt`).
  - *Artifacts Evaluated — Functional:* Single-entrypoint reproduction script (`reproduce.py`) regenerating all publication SVG figures and LaTeX tables.
  - *Artifacts Evaluated — Reusable:* Modular `VectorEngineAdapter` trait and pluggable dataset ingestion.
* **Academic Bare-Metal Testbeds:** Standardized deployment instructions for CloudLab and Chameleon Cloud bare-metal machines (`isolcpus=1-3`, `nohz_full`, invariant TSC) are provided in [`docs/academic_reproduction_guide.md`](file:///docs/academic_reproduction_guide.md).

---

## 6. Vendor Review & Responsible Disclosure Protocol

To avoid biased or misconfigured database comparisons, Fracture enforces:
* **Pre-Publication Maintainer Configuration Review:** Exact engine configurations, indexing flags, and cgroup v2 RAM ceilings are submitted to maintainers (Qdrant, pgvector, Milvus, LanceDB) for audit prior to public release.
* **Jepsen-Style Responsible Disclosure:** If Fracture discovers severe latency cliffs, recall degradation, or memory leaks, maintainers receive a private 2-to-4 week embargo window with the complete binary WAL trace and reproduction script before public disclosure.

Details and outreach templates are audited in [`VENDORS.md`](file:///VENDORS.md).

---

## 7. Repository Knowledge Graph

Architectural Decision Records (ADRs) are documented in `wiki/`:
*   [[wiki/adr/001-initial-architecture]]: Foundational baseline and modular boundaries.
*   [[wiki/adr/002-open-loop-wal-architecture]]: Pacer TSC design, bytemuck WAL layout, and SIMD linear scans.
