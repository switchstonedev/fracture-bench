---
title: Open-Loop Hardware Pacing & Binary WAL Architecture
summary: Architectural decision establishing quanta-based TSC pacing, thread pinning, and 32-byte naturally aligned binary WAL replay for point-in-time ground truth.
type: adr
status: active
tags: [adr, pacer, tsc, wal, ground-truth]
aliases: [adr-002, open-loop-wal]
---

# ADR 002: Open-Loop Hardware Pacing & Binary WAL Architecture

## Context
Standard closed-loop vector database benchmarks suffer from Coordinated Omission: during target database compaction or GC stalls, client workers block, hiding queuing latency from tail percentiles. Concurrently, evaluating recall against mutable shared state introduces temporal measurement drift.

## Decision
1. **Decoupled Hardware TSC Pacing:** Implement `fracture-pacer` on a dedicated OS thread pinned via `isolcpus` using `quanta::Instant` to achieve sub-microsecond arrival scheduling jitter at up to 50,000 QPS.
2. **Dual-Histogram Latency Accounting:** Measure both Service Time (t_recv - t_sent) and Total Coordinated-Omission-Corrected Response Time (t_recv - t_intended).
3. **Aligned 32-Byte Binary WAL:** Persist all mutations to an append-only binary Write-Ahead Log with monotonic Log Sequence Numbers (LSNs) and 8-byte natural field alignment, enabling zero-copy casting with `bytemuck`.
4. **Decoupled Brute-Force SIMD Replay:** Eliminate memory bus and cache contention during live benchmark runs by evaluating exact top-K ground truth post-hoc using AVX2/NEON linear scan kernels.

## Consequences
- Pacer thread runs 100% busy-spin under high QPS (> 10k QPS) on an isolated core.
- Benchmark clients never block the pacing loop; queue overflow increments an unserved omission counter.
- Ground-truth exact k-NN computation is completely isolated from live runs.

## References
- System Architecture: [[wiki/architecture]] (relates_to)
- Initial Baseline: [[wiki/adr/001-initial-architecture]] (relates_to)

