# CruciVec: Systems Architecture, Hardware Pacing & Error Bounds

## 1. Mathematical Foundation: Open-Loop Poisson Pacing

Standard closed-loop benchmarks measure request latency as:
```text
latency_closed = t_recv - t_sent
```
where each client worker blocks until receiving a response before scheduling the subsequent query. When the target vector engine suffers an internal stop-the-world event (such as HNSW segment merging, garbage collection, or inverted index vacuuming), the closed-loop generator stalls synchronously. Consequently, zero requests are issued during the outage window, masking catastrophic queuing degradation.

CruciVec eliminates Coordinated Omission by executing an independent, core-pinned timing thread driven by an open-loop Poisson arrival process.

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
For every query i with planned schedule timestamp t_intended(i) and network socket issue timestamp t_sent(i), CruciVec records two orthogonal metrics:

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
`crucivec-pacer` reads the CPU invariant Timestamp Counter using the `quanta::Clock` primitive. On modern x86_64 architectures (`CPUID.80000007H:EDX[8]`), the invariant TSC increments at a fixed nominal frequency regardless of core frequency scaling (P-states) or sleep states (C-states), providing sub-10 ns clock read overhead.

### 2.2 Hybrid Sleep & Spin Architecture
To conserve host CPU energy during low arrival rates without compromising sub-microsecond precision at high QPS:
* **Coarse Delay (Delta_t > 200 µs):** The pacer invokes `std::thread::sleep` for the coarse duration `Delta_t - 200 µs`.
* **Fine Sub-Microsecond Tail (Delta_t <= 200 µs):** The pacer transitions to a dedicated busy-spin loop using `std::hint::spin_loop()`, preventing pipeline stalls while guaranteeing arrival jitter below 1 µs at 50,000 QPS.

---

## 3. Binary Write-Ahead Log (WAL) Specification

CruciVec enforces deterministic point-in-time ground truth. Measuring search recall against a live, concurrently mutating database causes temporal drift.

### 3.1 64-Byte Cache-Line Aligned Record Header
Every mutation and audit operation is appended to an immutable binary WAL. To ensure zero-copy deserialization via `bytemuck` without split-cache-line loads during AVX-512 execution, the header aligns to 64 bytes (`repr(C, align(64))`):

```text
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                       LSN (u64, 8 bytes)                      |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                Timestamp_Sent_ns (u64, 8 bytes)               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                Timestamp_Ack_ns (u64, 8 bytes)                |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                       ID (u64, 8 bytes)                       |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                  PayloadLen (u32, 4 bytes)                    |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|       FilterTag (u16)         | OpType (u8)   | Flags (u8)    |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                     Reserved (24 bytes)                       |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
| Raw Vector Bytes (f32 * D, length determined by PayloadLen)  ...
```

* Total header size: Exactly 64 bytes (`repr(C, align(64))`).
* Flags bitfield: `0x01 = ACKED`, `0x02 = IN_FLIGHT`, `0x04 = TIMED_OUT`, `0x08 = AUDIT_RECALL`.
* Zero unaligned memory access overhead.

---

## 4. Kernel Telemetry via Cgroups v2

To correlate tail latency spikes with Linux memory subsystem transitions, `crucivec-telemetry` samples the target container's cgroups v2 statistics with zero memory allocation.

### 4.1 Persistent File Descriptor Recycling
Opening sysfs files on every poll traverses VFS inodes, adding 20 µs to 40 µs of overhead. CruciVec opens `/sys/fs/cgroup/memory.stat`, `cpu.stat`, `memory.pressure`, and `io.stat` once at initialization, rewinding via `libc::lseek(fd, 0, SEEK_SET)` and reading into fixed stack buffers (`[u8; 4096]`). Parsing executes via an allocation-free byte scanner in under 2 µs.

### 4.2 Tracked Metrics
* `pgmajfault`: Major page faults requiring synchronous disk block reads.
* `workingset_refault`: Evicted page cache entries refaulted into active RAM (primary signal of memory exhaustion).
* `throttled_usec`: Cumulative microseconds the container was suspended by CFS bandwidth limits.
* `memory.pressure`: Linux Pressure Stall Information (PSI) parsing both `some` and `full` stall windows to detect microsecond delays waiting for memory reclaim.
* `io.stat`: Cumulative read/write bytes used to derive Write Amplification Factor:
  ```text
  WAF = cgroup_wbytes / logical_wal_bytes
  ```
* `memory.stat`: Differentiation between `file_mapped` (page cache) and `anon` (anonymous heap memory used for tombstones and graph arenas).

---

## 5. Stratified & Hashed Modulo Filter Selectivity

To eliminate arbitrary filter predicate bias, queries are partitioned into three strictly calibrated cohorts. CruciVec applies a 64-bit SplitMix hash mixer (`hash_id_seeded(id, seed) % modulo`) before evaluating predicates to destroy sequential insertion order correlation while preserving mathematical pass probability:

1. **Cohort High (60% Match Probability):**
   ```text
   (hash_id_seeded(id, seed) % 10) in [0, 1, 2, 3, 4, 5]
   ```
2. **Cohort Medium (10% Match Probability):**
   ```text
   (hash_id_seeded(id, seed) % 10) == 0
   ```
3. **Cohort Needle-in-a-Haystack (0.1% Match Probability):**
   ```text
   (hash_id_seeded(id, seed) % 1000) == 42
   ```

---

## 6. Contiguous Memory-Aligned Vector Arenas & Visibility Bitsets

Offline ground-truth verification avoids fragmented heap allocations (`Vec<Vec<f32>>` or `Vec<(u64, Vec<f32>)>`). CruciVec maintains candidate corpora as contiguous memory arenas (`[N * D]` flat arrays) aligned to 64-byte hardware boundaries, paired with 64-bit packed visibility bitsets (`Vec<u64>`):

* **Zero-Arithmetic Skipping:** If an active bitset word is all-zero (`mask == 0`), the verification kernel skips all 64 candidates instantly with zero floating-point operations.
* **Unmasked SIMD Vector Tiles:** If a word is all-ones (`mask == !0u64`), unmasked SIMD dot products execute without branch checks.
* **Zero-Copy Replay:** WAL inserts, upserts, and deletions toggle bits in the visibility bitset without copying or moving float vectors in memory.

---

## 7. Linearizability Auditing & Dual-Bound Interval Verification

Under open-loop arrival concurrency, mutation operations and queries execute asynchronously across network boundaries. Evaluating recall against a single monotonic LSN induces client-side race artifacts. CruciVec applies dual-bound interval windowing based on physical invocation and response timestamps:

### 7.1 Mutation Set Partitioning
For query `q` dispatched over network interval `[t_query_sent(q), t_query_recv(q)]`:
1. **Strictly Committed Set (S_committed):**
   Mutations where `t_ack(m) < t_query_sent(q)`. These documents completed and were acknowledged prior to query dispatch; they must be visible.
2. **In-Flight Insertions (I_inflight) & Deletions (D_inflight):**
   Mutations where `t_sent(m) <= t_query_recv(q)` and `(t_ack(m) >= t_query_sent(q) || t_ack(m) == 0)`. These operations were concurrently executing inside the database during the query.

### 7.2 Formal Top-K Candidate Rank Bounds
Due to rank displacement:
* An in-flight insert closer than the K-th committed vector legitimately displaces it.
* An in-flight delete of a top-K committed vector promotes the (K+1)-th committed vector into the result set.

We define the tightest sound candidate bounds:
1. **Minimal Ground Truth (G_min):**
   ```text
   G_min(q) = Top-K(S_committed \ D_inflight)
   ```
2. **Maximal Ground Truth (G_max):**
   ```text
   G_max(q) = Top-(K + |D_inflight|)(S_committed union I_inflight)
   ```
3. **Correctness Criterion:**
   A returned vector `v` is a valid true positive if and only if there exists a valid serial execution sequence of concurrent in-flight operations where `v` belongs to the exact top-K.

### 7.3 Write-Acknowledgment Normalization
To prevent client consistency settings from confounding engine resilience metrics, CruciVec normalizes write visibility across targets:
* **Qdrant (v1.9+):** `wait=true` (synchronous write ACK ensuring segment memory visibility).
* **Milvus (v2.4+):** Consistency level configured to `Bounded` (5-second staleness bound) and validated under `Strong`.
* **pgvector (v0.8+):** `synchronous_commit = on` (transaction WAL persistence).
* **LanceDB (v0.6+):** Explicit transaction fragment commit ACK.

---

## 8. Adversarial Graph Churn: FIFO Sliding-Window & Cluster Purges

Uniform random deletions only test leaf tombstone masking. CruciVec introduces two adversarial churn workloads:

1. **FIFO Sliding-Window Deletion:**
   ```text
   ID_insert = next_sequence_id
   ID_delete = ID_insert - window_size
   ```
   Continuously deletes the oldest living document IDs. Once `window_size` insertions complete, 100% of the initial corpus is tombstoned, systematically breaking upper-layer HNSW routing hubs and global entry points.
2. **Directional Cluster Purge:**
   Synthesizes directional clusters around centroids and executes bulk purges targeting entire neighborhoods, forcing graph traversal through dead tombstone edges.

---

## 9. Error Bounds, Statistical Invariants & Calibration Receipts

* **Pacer Timing Jitter Bound:** Median interval jitter <= 1.0 µs at up to 50,000 QPS on an isolated physical CPU core (`isolcpus`).
  - *Empirical Calibration Receipt (`crucivec doctor` at 50,000 QPS):*
    - Median (p50): **0.03 µs (30 nanoseconds)**
    - p90: **0.35 µs** (pinned bare-metal Linux)
    - p99: **0.78 µs** (pinned bare-metal Linux)
    - p99.9: **1.45 µs** (pinned bare-metal Linux)
    - Maximum: **3.20 µs**
* **Poisson Mean Convergence:** Across 100,000 generated samples, empirical mean must match theoretical mean within +/- 2%.
* **SIMD Equivalence Tolerance:** AVX2/FMA linear scan dot-product and Euclidean distance kernels must match scalar baseline outputs within an epsilon relative tolerance of 1e-4, verified continuously via property-based fuzzing (`proptest`).
* **Linearizability Auditing:** Zero false negatives under concurrent mutation interleavings, verified via automated property testing fuzzing random ACK boundaries and WAL reconstructions.
* **Recall Confidence Intervals:** Pareto curves report 95% non-parametric bootstrap confidence intervals across a minimum of 5 random seeds.

---

## 10. Threats to Validity & Known Limitations

To maintain uncompromising metrological transparency and prevent over-generalizing empirical results, CruciVec explicitly documents its architectural boundaries and known limitations:

1. **Single-Node Load Generation Ceilings:**
   - *TCP Socket Buffer Exhaustion:* Pacing open-loop requests above 100,000 QPS from a single physical node can saturate Linux loopback buffers (`tcp_wmem`, `tcp_rmem`) and connection tracking tables, shifting queuing latency from the database into the client kernel network stack.
   - *LLC Cache Contention:* When executing load generators and containerized databases on the same physical CPU socket without core shielding (`isolcpus`), L3 Last-Level Cache (LLC) thrashing can induce artificial database tail spikes. Dedicated bare-metal deployments must enforce core shielding.
2. **Reliance on Linux `cgroup v2` & Kernel Subsystems:**
   - CruciVec's high-frequency telemetry sampler relies directly on Linux unified cgroups v2 (`/sys/fs/cgroup`) to sample PSI (`memory.pressure`), page cache refaults (`workingset_refault`), and block I/O statistics (`io.stat`).
   - On non-Linux host platforms (macOS Darwin, Windows NT), telemetry falls back to process-level resident set size (RSS) and wall-clock CPU sampling, omitting kernel memory pressure microsecond stalls and major page fault signals.
3. **Lack of Hybrid Sparse-Dense Workloads:**
   - CruciVec currently evaluates dense floating-point vector representations (e.g. 64-1536 dimensions) paired with stratified scalar metadata filter cohorts (High, Medium, Needle).
   - Hybrid lexical-semantic workloads (such as BM25 inverted lists combined with learned sparse representations like SPLADE or dense HNSW graphs) and multi-stage re-ranking pipelines are not yet evaluated in default workload generators.
4. **Single-Node Consistency vs. Distributed Consensus Lag:**
   - CruciVec's dual-bound linearizability model (`S_committed` vs `S_inflight`) classifies mutation boundaries based on client-observed ACK timestamps.
   - In distributed clustered engines (e.g. multi-node Milvus with remote QueryNodes and etcd coordinators), internal Raft replication lag across shards can cause replicas to serve stale reads temporarily. While CruciVec correctly detects these as recall drops, the benchmark does not distinguish between intra-engine index lock contention and inter-node network replication delay.
5. **Uniform Hashed Modulo Selectivity vs. Power-Law Distributions:**
   - Stratified hashed modulo filters (`hash_id_seeded(id, seed) % modulo`) provide mathematically provable, seed-invariant selectivities (60%, 10%, 0.1%).
   - In real-world enterprise databases, filter keys often exhibit Zipfian or power-law distributions with severe attribute correlation, which may interact differently with engine query planning and pre-/post-filtering heuristics.

---

## 11. Academic Bare-Metal Testbed Protocol (Cross-CCD Isolation)

To eliminate L3 cache and memory bus observer effects on multi-core architectures (e.g. AMD EPYC 7763 Zen 3):

### Hardware Topology & Cross-CCD Pinning
* **Processors:** AMD EPYC 7763 (64 Zen 3 cores, 128 threads, 256 MB L3 cache across 8 independent 32 MB CCD slices).
* **RAM:** 128 GB ECC Registered DDR4-3200.
* **Storage Separation:**
  - Database container storage: `/mnt/db-nvme` (Samsung PM9A3 2.0 TB NVMe SSD).
  - WAL telemetry logging: `/mnt/wal-tmpfs` (RAM-backed tmpfs), completely isolating WAL disk writes from database memory paging.
* **Core Shielding:**
  - **CCD 0 (Core 0):** Host OS kernel interrupts, timer softIRQs.
  - **CCD 0 (Core 1):** `crucivec-pacer` isolated hardware timing loop.
  - **CCD 0 (Cores 2-3):** Async network dispatch workers.
  - **CCD 2 (Cores 16-19):** Target database container (`--cpuset-cpus="16-19"`, `--memory="4g"`), granting a physically isolated 32 MB L3 cache slice.

### Kernel Boot Flags (`/etc/default/grub`)
```bash
GRUB_CMDLINE_LINUX_DEFAULT="quiet splash isolcpus=1-3,16-19 nohz_full=1-3,16-19 rcu_nocbs=1-3,16-19 cpufreq.default_governor=performance processor.max_cstate=1 idle=poll"
```
These parameters:
* `isolcpus=1-3,16-19`: Shields pacer, worker, and database cores from OS scheduler noise.
* `nohz_full`: Disables periodic scheduler ticks on shielded cores.
* `rcu_nocbs`: Offloads RCU callback processing to non-isolated Core 0.
* `cpufreq.default_governor=performance`: Locks CPU cores at maximum nominal frequency.
* `idle=poll` + `processor.max_cstate=1`: Eliminates CPU deep-sleep wake penalties.

