---
title: SIMD Linear Scan Arenas & Dual-Bound Linearizability
summary: Cache-line aligned contiguous vector arenas, 64-bit visibility bitset skipping, AVX2 distance kernels, and dual-bound linearizability top-K verification.
type: concept
status: active
tags: [simd, recall, linearizability, avx2, arenas, verification]
aliases: [simd-arenas, dual-bound-linearizability, vector-arenas]
---

# SIMD Linear Scan Arenas & Dual-Bound Linearizability

The `fracture-core` crate implements exact ground-truth nearest neighbor verification under concurrent mutation churn (70% search, 20% insert, 10% delete) using cache-line aligned vector arenas and dual-bound linearizability auditing.

---

## 1. Problem: Memory Fragmentation & Tombstone Overhead

Calculating ground-truth recall for high-dimensional vectors during continuous database mutations requires computing exact cosine or Euclidean distances across millions of candidate vectors:

1. **Pointer Chasing:** Storing vectors in fragmented collections (`Vec<Vec<f32>>` or `HashMap<u64, Vec<f32>>`) scatters floating-point buffers across the heap, causing continuous L1/L2 data cache misses.
2. **Tombstone Overhead:** In mutation-heavy workloads with high delete rates, computing distances for tombstoned (soft-deleted) vectors wastes millions of floating-point operations.
3. **Linearizability Ambiguity:** In concurrent open-loop workloads, mutations execute concurrently with queries. If a mutation commit timestamp falls within a query's execution window, whether the document should appear in the ground truth set is ambiguous under standard single-bound recall definitions.

---

## 2. Contiguous Aligned Vector Arenas & 64-Bit Bitsets

`fracture-core` organizes document vectors in a memory-mapped or heap-allocated contiguous arena (`VectorArena`):

* **64-Byte Hardware Boundary Alignment:** All vector float arrays are laid out contiguously in memory (`[N * D]` flat array) aligned to 64-byte boundaries. This aligns vector slices with CPU cache lines, eliminating split-cache-line loads during SIMD memory fetches.
* **Packed 64-Bit Visibility Bitsets:** Document visibility and liveness are tracked using a packed `Vec<u64>` bitset where bit `i` represents document ID `i`.
* **Zero-Arithmetic Block Skipping:** During linear scan, the verification kernel inspects each 64-bit mask word:
  - If `mask == 0`, all 64 candidate documents in the block are tombstoned; the kernel skips the entire 64-vector block instantly without performing a single floating-point operation.
  - If `mask == !0u64` (all bits set), the kernel executes an unmasked 256-bit AVX2 FMA dot-product kernel (`_mm256_fmadd_ps`) across all 64 vectors without branch evaluation.
  - For partial masks, bit manipulation instructions (`tzcnt` / trailing zero count) locate active vectors directly.

```text
64-Bit Visibility Mask Word
[ 00000000 ... 00000000 ] -> mask == 0: Skip 64 vectors (0 FLOPs)
[ 11111111 ... 11111111 ] -> mask == !0: Unmasked AVX2 FMA vector tile
[ 10100000 ... 00000001 ] -> tzcnt bitscan: Process only active vectors
```

---

## 3. Dual-Bound Linearizability Auditing

Under open-loop concurrency, a query dispatch interval `[t_sent, t_recv]` overlaps with ongoing insert and delete mutations recorded in the binary Write-Ahead Log (WAL).

To prevent false recall penalties caused by concurrent in-flight state mutations, `fracture verify` evaluates recall against dual bounds:

```text
Timeline:
------------------[Mutation 1 Committed: t1]------------------
             |                                     |
       [Query Sent: t_sent]               [Query Recv: t_recv]
             |                                     |
------------------[Mutation 2 In-Flight: t2]------------------
```

1. **Strictly Committed Corpus (`S_committed`):** Includes all documents committed strictly before `t_sent` and excludes documents deleted before `t_sent`.
2. **In-Flight Candidate Corpus (`S_inflight`):** Includes documents inserted or deleted in the interval `[t_sent, t_recv]`.
3. **Dual Bounds (`G_min` and `G_max`):**
   - **`G_min` (Pessimistic Ground Truth):** Exact top-K candidates computed strictly from `S_committed`.
   - **`G_max` (Optimistic Ground Truth):** Exact top-K candidates considering valid in-flight mutations.
4. **Linearizable Recall@K:** A database query result is validated if every returned hit belongs to the valid linearizable envelope between `G_min` and `G_max`. This mathematically guarantees zero false negatives caused by concurrency delays in the benchmarking harness.

---

## 4. Graph Relationships
- Architecture Overview: [[wiki/architecture]] (relates_to)
- WAL Architecture: [[wiki/adr/002-open-loop-wal-architecture]] (depends_on)
- Hardware Pacing: [[wiki/concepts/hardware-pacing]] (relates_to)
- Author-OSS Governance: [[wiki/adr/005-author-oss-benchmark-governance]] (relates_to)
