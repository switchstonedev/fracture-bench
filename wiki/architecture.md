---
title: System Architecture & Subsystem Boundaries
summary: Formal systems design, TSC hardware timing, zero-copy WAL materialization, and kernel telemetry boundaries.
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
        subgraph Core0 ["Core 0 (OS Shield)"]
            OS[Linux Interrupts & SoftIRQs]
        end

        subgraph Core1 ["Core 1 (Pacer - isolcpus)"]
            Pacer[crucivec-pacer<br/>quanta TSC + Inverse-CDF]
            WAL[Binary WAL Writer<br/>Aligned 32B Header]
        end

        subgraph Cores23 ["Cores 2-3 (Workers)"]
            Channel[Bounded Lock-Free Queue<br/>Drop & Omission Tracking]
            Workers[Tokio Async Dispatchers<br/>Dual HdrHistograms]
        end

        subgraph Cores47 ["Cores 4-7 (Target Container)"]
            TargetDB[(Target Vector DB<br/>Qdrant / Milvus / pgvector / LanceDB)]
            Telemetry[crucivec-telemetry<br/>Persistent-fd /sys/fs/cgroup Scraper]
        end
    end

    subgraph OfflineAnalysis ["Offline Evaluation Pipeline"]
        GroundTruth[crucivec verify<br/>Rayon + AVX2 Linear Scan]
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
*   `crates/crucivec-pacer`: High-precision arrival scheduler running on an isolated, pinned CPU core.
*   `crates/crucivec-core`: Domain types, 32-byte naturally aligned binary Write-Ahead Log (WAL), and AVX2/NEON SIMD linear-scan $k$-NN kernels.
*   `crates/crucivec-telemetry`: Allocation-free Linux `/sys/fs/cgroup` parser.
*   `crates/crucivec-adapters`: Unified database client trait implementations.
*   `crates/crucivec-cli`: Main executable binary with `run`, `verify`, and `profile` subcommands.

## 3. Architecture Decision Records
*   Initial Baseline: [[wiki/adr/001-initial-architecture]]
*   Open-Loop WAL Architecture: [[wiki/adr/002-open-loop-wal-architecture]]
