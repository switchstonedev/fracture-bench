---
title: Open-Loop Hardware Pacing & Binary WAL Architecture
summary: Architectural decision establishing quanta-based TSC pacing, cross-CCD thread pinning, 64-byte cache-line aligned binary WAL replay, and formal dual-bound top-K linearizability.
type: adr
status: active
tags: [adr, pacer, tsc, wal, ground-truth, linearizability]
aliases: [adr-002, open-loop-wal, open-loop-wal-protocol]
---

# ADR 002: Open-Loop Hardware Pacing & Binary WAL Architecture

## Context
Standard closed-loop vector database benchmarks suffer from Coordinated Omission: during target database compaction or GC stalls, client workers block, hiding queuing latency from tail percentiles. Concurrently, evaluating recall against mutable shared state introduces temporal measurement drift and rank displacement under concurrent mutations.

## Decision
1. **Decoupled Hardware TSC Pacing:** Implement `fracture-pacer` on a dedicated OS thread pinned to CCD 0 / Core 1 via `isolcpus` using `quanta::Instant` to achieve sub-microsecond arrival scheduling jitter at up to 50,000 QPS.
2. **Dual-Histogram Latency Accounting:** Measure both Service Time (t_recv - t_sent) and Total Coordinated-Omission-Corrected Response Time (t_recv - t_intended).
3. **Cache-Line Aligned 64-Byte Binary WAL on tmpfs:** Persist all mutations to an append-only binary Write-Ahead Log with monotonic Log Sequence Numbers (LSNs) and 64-byte hardware cache-line alignment (`repr(C, align(64))`), mapped to `tmpfs` to eliminate NVMe I/O interference with target database memory paging.
4. **Formal Dual-Bound Top-K Linearizability:** Define sound candidate bounds accounting for in-flight mutations:
   - Minimal Ground Truth: `G_min(q) = Top-K(S_committed \ D_inflight)`
   - Maximal Ground Truth: `G_max(q) = Top-(K + |D_inflight|)(S_committed union I_inflight)`
   - A candidate is scored as a valid true positive iff there exists a valid serial execution placing it in the exact top-K.
5. **Decoupled Brute-Force SIMD Replay:** Eliminate memory bus and cache contention during live benchmark runs by evaluating exact top-K ground truth post-hoc using AVX2/NEON linear scan kernels over contiguous memory arenas with 64-bit visibility bitsets.

## Consequences
- Pacer thread runs 100% busy-spin under high QPS (> 10k QPS) on an isolated core.
- Benchmark clients never block the pacing loop; queue overflow increments an unserved omission counter.
- Ground-truth exact k-NN computation is completely isolated from live runs.
- Cross-CCD core pinning (CCD 0 Cores 1-3 vs CCD 2 Cores 16-19 on AMD EPYC) provides physical 32 MB L3 cache slice isolation.

## References
- System Architecture: [[wiki/architecture]] (relates_to)
- Initial Baseline: [[wiki/adr/001-initial-architecture]] (relates_to)

