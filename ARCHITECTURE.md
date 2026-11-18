# Fracture: Systems Architecture, Hardware Pacing & Error Bounds

## 1. Mathematical Foundation: Open-Loop Poisson Pacing

Standard closed-loop benchmarks measure request latency as:
```text
latency_closed = t_recv - t_sent
```
where each client worker blocks until receiving a response before scheduling the subsequent query. When the target vector engine suffers an internal stop-the-world event (such as HNSW segment merging, garbage collection, or inverted index vacuuming), the closed-loop generator stalls synchronously. Consequently, zero requests are issued during the outage window, masking catastrophic queuing degradation.

Fracture eliminates Coordinated Omission by executing an independent, core-pinned timing thread driven by an open-loop Poisson arrival process.

### 1.1 Inverse-CDF Inter-Arrival Sampling
Inter-arrival intervals Delta_t follow an exponential distribution governed by arrival rate lambda (target QPS):
```text
F(Delta_t) = 1 - exp(-lambda * Delta_t)
```
Inverting the cumulative distribution function yields the deterministic sampling formula:
```text
Delta_t = -ln(U) / lambda
```
where U is drawn uniformly from (0.0, 1.0] using an inline `Xoroshiro128PlusPlus` pseudo-random number generator (1.2 ns per sample, zero heap allocation).

### 1.2 Dual-Histogram Latency Decomposition
For every query i with planned schedule timestamp t_intended(i) and network socket issue timestamp t_sent(i), Fracture records two orthogonal metrics:

1. **Network Service Time:**
   ```text
   S(i) = t_recv(i) - t_sent(i)
   ```
   Captures raw database execution and socket transmission duration.

2. **Total Response Time (Coordinated-Omission-Corrected):**
   ```text
   T(i) = t_recv(i) - t_intended(i)
   ```
   Captures end-to-end user experience, including client-side queue delay incurred when the engine stalls.

---

## 2. Hardware Timestamp Counter (TSC) Timing Engine

Cooperative asynchronous timers (such as `tokio::time::sleep`) rely on OS timer wheels and event loop polling, introducing 50 µs to 2,000 µs of task-scheduling jitter.

### 2.1 Invariant TSC Primitives
`fracture-pacer` reads the CPU invariant Timestamp Counter using the `quanta::Clock` primitive. On modern x86_64 architectures (`CPUID.80000007H:EDX[8]`), the invariant TSC increments at a fixed nominal frequency regardless of core frequency scaling (P-states) or sleep states (C-states), providing sub-10 ns clock read overhead.

### 2.2 Hybrid Sleep & Spin Architecture
To conserve host CPU energy during low arrival rates without compromising sub-microsecond precision at high QPS:
* **Coarse Delay (Delta_t > 200 µs):** The pacer invokes `std::thread::sleep` for the coarse duration `Delta_t - 200 µs`.
* **Fine Sub-Microsecond Tail (Delta_t <= 200 µs):** The pacer transitions to a dedicated busy-spin loop using `std::hint::spin_loop()`, preventing pipeline stalls while guaranteeing arrival jitter below 1 µs at 50,000 QPS.

---

## 3. Binary Write-Ahead Log (WAL) Specification

Fracture enforces deterministic point-in-time ground truth. Measuring search recall against a live, concurrently mutating database causes temporal drift.

### 3.1 32-Byte Naturally Aligned Record Header
Every mutation operation (insert, upsert, delete) is appended to an immutable binary WAL. To ensure zero-copy deserialization via `bytemuck` without unaligned memory panics, fields align to 8-byte boundaries:

```text
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                       LSN (u64, 8 bytes)                      |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                  Timestamp_ns (u64, 8 bytes)                  |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                       ID (u64, 8 bytes)                       |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                  PayloadLen (u32, 4 bytes)                    |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
| OpType (u8)   |             Reserved (3 bytes)                |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
| Raw Vector Bytes (f32 * D, length determined by PayloadLen)  ...
```

* Total header size: Exactly 32 bytes (`repr(C)`).
* Alignment: Exactly 8 bytes.
* Zero internal padding holes.

---

## 4. Kernel Telemetry via Cgroups v2

To correlate tail latency spikes with Linux memory subsystem transitions, `fracture-telemetry` samples the target container's cgroups v2 statistics.

### 4.1 Persistent File Descriptor Recycling
Opening sysfs files on every poll traverses VFS inodes, adding 20 µs to 40 µs of overhead. Fracture opens `/sys/fs/cgroup/memory.stat` and `cpu.stat` once at initialization, rewinding via `libc::lseek(fd, 0, SEEK_SET)` and reading into fixed stack buffers (`[u8; 4096]`). Parsing executes via an allocation-free byte scanner in under 2 µs.

### 4.2 Tracked Metrics
* `pgmajfault`: Major page faults requiring synchronous disk block reads.
* `workingset_refault`: Evicted page cache entries refaulted into active RAM (primary signal of memory exhaustion).
* `throttled_usec`: Cumulative microseconds the container was suspended by CFS bandwidth limits.

---

## 5. Stratified Modulo Filter Selectivity

To eliminate arbitrary filter predicate bias, queries are partitioned into three strictly calibrated cohorts based on document ID modulo constraints:

1. **Cohort High (60% Match Probability):**
   ```text
   (id % 10) in [0, 1, 2, 3, 4, 5]
   ```
2. **Cohort Medium (10% Match Probability):**
   ```text
   (id % 10) == 0
   ```
3. **Cohort Needle-in-a-Haystack (0.1% Match Probability):**
   ```text
   (id % 1000) == 42
   ```

---

## 6. Error Bounds & Statistical Invariants

* **Pacer Timing Jitter Bound:** Median interval jitter <= 1.0 µs at up to 50,000 QPS on an isolated physical CPU core (`isolcpus`).
* **Poisson Mean Convergence:** Across 100,000 generated samples, empirical mean must match theoretical mean within +/- 2%.
* **SIMD Equivalence Tolerance:** AVX2/FMA linear scan dot-product and Euclidean distance kernels must match scalar baseline outputs within an epsilon tolerance of 1e-4.
* **Recall Confidence Intervals:** Pareto curves report 95% non-parametric bootstrap confidence intervals across a minimum of 5 random seeds.

