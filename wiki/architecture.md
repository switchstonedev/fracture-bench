---
title: System Architecture & Subsystem Boundaries
summary: Formal systems design, TSC hardware timing, zero-copy WAL materialization, cross-CCD isolation, and kernel telemetry boundaries.
type: architecture
status: active
tags: [architecture, design, systems]
aliases: [system-architecture]
---

# System Architecture & Subsystem Boundaries

## 1. System Topology

```mermaid
flowchart TD
    subgraph Host ["Shielded Benchmark Server"]
        subgraph CCD0_Core0 ["CCD 0 / Core 0 (OS Shield)"]
            OS[Linux Interrupts & SoftIRQs]
        end

        subgraph CCD0_Core1 ["CCD 0 / Core 1 (Pacer - isolcpus)"]
            Pacer[crucivec-pacer<br/>quanta TSC + Inverse-CDF]
            WAL[Binary WAL Writer<br/>64B Aligned Header on tmpfs]
        end

        subgraph CCD0_Cores23 ["CCD 0 / Cores 2-3 (Workers)"]
            Channel[Bounded Lock-Free Queue<br/>Drop & Omission Tracking]
            Workers[Tokio Async Dispatchers<br/>Dual HdrHistograms]
        end

        subgraph CCD2_Cores1619 ["CCD 2 / Cores 16-19 (Target Container)"]
            TargetDB[(Target Vector DB<br/>Qdrant / Milvus / pgvector / LanceDB)]
            Telemetry[crucivec-telemetry<br/>Persistent-fd /sys/fs/cgroup Scraper]
        end
    end

    subgraph OfflineAnalysis ["Offline Evaluation Pipeline"]
        GroundTruth[crucivec verify<br/>Dual-Bound Top-K (G_min vs G_max)<br/>Rayon + AVX2 Linear Scan]
        Report[generate_report.py<br/>Dual-CDFs & Pareto Frontiers]
    end

    Pacer -->|DispatchTicket| Channel
    Pacer -->|Append Mutation| WAL
    Channel -->|Non-blocking Recv| Workers
    Workers -->|Queries & Churn Writes| TargetDB
    Telemetry -.->|Poll cgroups v2| TargetDB
    WAL --> GroundTruth
    Workers --> Report
    Telemetry --> Report
    GroundTruth --> Report
```

## 2. Subsystem Layout
*   `crates/fracture-pacer`: High-precision arrival scheduler running on an isolated, pinned CPU core (CCD 0 / Core 1).
*   `crates/fracture-core`: Domain types, 64-byte cache-line aligned binary Write-Ahead Log (WAL), contiguous `VectorArena` with 64-bit visibility bitset skipping, and dual-bound linearizability top-K verification.
*   `crates/fracture-telemetry`: Allocation-free Linux `/sys/fs/cgroup` parser sampling memory refaults, PSI stalls, and CFS throttling.
*   `crates/fracture-adapters`: Unified database client trait implementations across Qdrant, Milvus, pgvector, and LanceDB.
*   `crates/fracture-cli`: Main executable binary with `run`, `verify`, and `profile` subcommands.

## 3. Architecture Decision Records
*   Initial Baseline: [[wiki/adr/001-initial-architecture]]
*   Open-Loop WAL Architecture: [[wiki/adr/002-open-loop-wal-architecture]]
