# CruciVec: The Open-Loop, Vendor-Neutral Crucible for Vector Databases

[![Status](https://img.shields.io/badge/status-active-brightgreen.svg)]()
[![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20Windows%20%7C%20macOS-blue.svg)]()
[![Toolchain](https://img.shields.io/badge/rust-1.75%2B-orange.svg)]()
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache%202.0-yellow.svg)](LICENSE)
[![Benchmarks](https://img.shields.io/badge/benchmarks-reproducible-success.svg)]()

CruciVec is an adversarial, open-loop benchmarking harness for vector search engines. It replaces synthetic "clean room" evaluations with realistic production workloads: continuous 70/20/10 mutation churn, hard deletes, HNSW graph fragmentation (dead-node tombstones), stratified metadata filtering, and strict kernel cgroups v2 memory budgets.

---

## 1. System Architecture

```mermaid
flowchart TD
    subgraph Host ["Shielded Benchmark Server"]
        subgraph Core0 ["Core 0 (OS Shield)"]
            OS[Linux Interrupts & Kernel SoftIRQs]
        end

        subgraph Core1 ["Core 1 (Pacer - isolcpus)"]
            Pacer[crucivec-pacer<br/>quanta TSC + Inverse-CDF]
            WAL[Binary WAL Writer<br/>Aligned 32B Header]
        end

        subgraph Cores23 ["Cores 2-3 (Client Workers)"]
            Channel[Bounded Lock-Free Queue<br/>Saturation / Drop Accounting]
            Workers[Tokio Async Dispatchers<br/>Dual HdrHistograms]
        end

        subgraph Cores47 ["Cores 4-7 (Target Cgroup v2)"]
            TargetDB[(Target Vector DB<br/>Qdrant / Milvus / pgvector / LanceDB)]
            Telemetry[crucivec-telemetry<br/>Persistent-fd /sys/fs/cgroup Scraper]
        end
    end

    subgraph OfflineAnalysis ["Offline Verification & Scoring"]
        GroundTruth[crucivec verify<br/>Rayon + AVX2 Linear Scan]
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

CruciVec isolates load generation, target execution, kernel telemetry, and mathematical scoring into independent subsystems to eliminate measurement perturbation:

*   **`crucivec-pacer`:** High-precision arrival scheduler running on an isolated, pinned CPU core. Uses hardware Timestamp Counter (`quanta`) to drive Poisson arrivals with sub-microsecond jitter at 50,000 QPS.
*   **`crucivec-core`:** Domain types, 32-byte naturally aligned binary Write-Ahead Log (WAL), and AVX2/NEON SIMD linear-scan $k$-NN kernels for decoupled ground-truth calculation.
*   **`crucivec-telemetry`:** Zero-allocation Linux `/sys/fs/cgroup` parser sampling page faults, workingset refaults, and CFS CPU throttling in under 2 µs per sample using persistent file descriptors.
*   **`crucivec-adapters`:** Unified client trait implementations for Qdrant (gRPC), pgvector (tokio-postgres), Milvus, and LanceDB (isolated).
*   **`crucivec-cli`:** Main executable binary with `run`, `verify`, and `profile` subcommands.

---

## 2. Core Invariants

1. **Elimination of Coordinated Omission:**
   CruciVec tracks dual histograms per worker task:
   *   `service_time`: Network round-trip duration ($t_{\text{recv}} - t_{\text{sent}}$).
   *   `total_response_time`: True end-to-end latency including client queue delay ($t_{\text{recv}} - t_{\text{intended}}$).
2. **Deterministic Point-in-Time Ground Truth:**
   Ground truth is never evaluated against mutable live state. Mutations are appended to an immutable binary WAL with monotonic Log Sequence Numbers (LSNs). Queries record their observed commit boundary, and ground-truth nearest neighbors are reconstructed post-hoc.
3. **Decoupled Ground-Truth Computation:**
   Brute-force SIMD scans are executed offline to prevent memory bus and cache pollution during the live benchmark run.
4. **Stratified Modulo Selectivity:**
   Metadata filter queries are partitioned into three calibrated cohorts:
   *   High Selectivity: ~60% match rate (`id % 10 in [0..5]`).
   *   Medium Selectivity: ~10% match rate (`id % 10 == 0`).
   *   Needle-in-a-Haystack: ~0.1% match rate (`id % 1000 == 42`).

---

## 3. Quick Start

### Build CruciVec
```bash
cargo build --release
```

### Run an Open-Loop Benchmark
```bash
./target/release/crucivec run \
    --target qdrant \
    --qps 10000 \
    --workload churn-70-20-10 \
    --dataset cohere-1m
```

### Replay WAL for Offline Recall@K Verification
```bash
./target/release/crucivec verify \
    --wal-path ./runs/qdrant_churn.wal \
    --queries ./data/cohere_queries.bin \
    --k 10
```

---

## 4. Repository Knowledge Graph

Architectural Decision Records (ADRs) are documented in `wiki/`:
*   [[wiki/adr/001-initial-architecture]]: Foundational baseline and modular boundaries.
*   [[wiki/adr/002-open-loop-wal-architecture]]: Pacer TSC design, bytemuck WAL layout, and SIMD linear scans.
