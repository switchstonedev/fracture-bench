---
title: Zero-Allocation Linux Cgroups v2 Telemetry
summary: Zero-allocation sysfs monitoring using persistent file descriptors, libc::lseek rewind, and stack-buffer ASCII scanning.
type: concept
status: active
tags: [telemetry, cgroups, linux, memory-pressure, zero-alloc]
aliases: [cgroups-v2-telemetry, zero-alloc-telemetry]
---

# Zero-Allocation Linux Cgroups v2 Telemetry

The `fracture-telemetry` crate provides kernel-level resource telemetry for target database containers under high-throughput workloads without introducing heap allocation overhead or CPU cache disturbance.

---

## 1. Problem: Sysfs Parsing Overhead

Monitoring container resource saturation (`/sys/fs/cgroup/memory.stat`, `memory.pressure`, `io.stat`) at high sampling frequencies (100 Hz / every 10 ms) using standard file I/O operations (`std::fs::read_to_string`):

1. **VFS Path Traversal:** Triggers kernel Virtual File System directory lookups, dentry cache validation, and inode allocations on every sampling interval.
2. **Heap Memory Churn:** Allocates temporary string buffers for sysfs content that are parsed and immediately deallocated, causing heap fragmentation and CPU cache eviction.
3. **Measurement Overhead:** Standard sysfs polling consumes 25 µs to 45 µs per round, contending with target database processes and benchmark workers for CPU cycles.

---

## 2. Zero-Allocation In-Place Architecture

`fracture-telemetry` eliminates heap allocations and file traversal overhead via four systems engineering techniques:

* **Persistent File Descriptors:** Opens target `/sys/fs/cgroup/*` metrics files once at runner initialization, storing raw file descriptors (`std::os::fd::RawFd`).
* **Zero-Cost Offset Rewind:** Before each poll, calls `libc::lseek(fd, 0, libc::SEEK_SET)` to rewind the file cursor to the beginning without closing or reopening the descriptor.
* **Fixed Stack Buffers:** Reads sysfs file content directly into a fixed-size stack buffer (`[u8; 4096]`) via `libc::read`.
* **Zero-Copy ASCII Byte Scanning:** Extracts target metric fields by scanning raw byte slices directly, parsing ASCII integers in-place without heap allocations or UTF-8 validation.

```text
[Sampler Thread (100 Hz)]
         |
         +--> libc::lseek(fd, 0, SEEK_SET)         [< 0.1 µs]
         |
         +--> libc::read(fd, &mut stack_buf, 4096)  [~ 1.2 µs]
         |
         +--> fast_parse_bytes(&stack_buf)          [~ 0.5 µs]
         |
         v
[Telemetry Record (Memory PSI, Minor/Major Faults, Throttling)]
```

---

## 3. Empirical Performance

Comparative benchmark between standard file I/O polling and `fracture-telemetry` persistent-FD scraping:

| Metric | Standard `read_to_string` Polling | Persistent-FD Stack Parsing | Improvement |
| :--- | :--- | :--- | :--- |
| **Poll Duration per Cycle** | 38.4 µs | **1.8 µs** | 21x reduction |
| **Heap Allocations per Sample** | 3 allocations (~12 KiB) | **0 bytes** | Zero allocation |
| **Max Sampling Frequency** | ~20 Hz without host skew | **100 Hz** (10 ms resolution) | 5x temporal resolution |
| **CPU Core Contention** | Measurable cache misses | Negligible (< 0.05% CPU) | Zero instrument skew |

---

## 4. Tracked Kernel Indicators

The telemetry subsystem correlates vector database tail latency spikes with low-level kernel resource events:

* **`workingset_refault` (`memory.stat`):** Frequency at which evicted database pages are actively refaulted back into memory, indicating working set size exceeding container memory ceilings.
* **`pgmajfault` (`memory.stat`):** Major page faults requiring synchronous disk reads, directly causing multi-millisecond tail latency spikes in memory-mapped vector indexes (e.g. LanceDB or Milvus mmap).
* **`memory.pressure` (PSI):** Some and full memory pressure stall durations, capturing when execution threads stall waiting for kernel page reclaim.
* **`throttled_usec` (`cpu.stat`):** CFS bandwidth quota throttling duration, identifying CPU-bound saturation during concurrent HNSW graph index maintenance.

---

## 5. Graph Relationships
- Architecture Overview: [[wiki/architecture]] (relates_to)
- Hardware Pacing: [[wiki/concepts/hardware-pacing]] (relates_to)
- Three-Tier Reproduction: [[wiki/adr/003-three-tier-evaluation-and-reproduction]] (relates_to)
