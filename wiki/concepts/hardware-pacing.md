---
title: Hardware Pacing & Timing Jitter Calibration
summary: Invariant TSC hardware pacing, hybrid sleep-spin loop mechanics, Poisson arrival scheduling, and Coordinated Omission elimination.
type: concept
status: active
tags: [pacer, tsc, latency, coordinated-omission, hardware]
aliases: [hardware-pacer, tsc-pacer, pacer-calibration]
---

# Hardware Pacing & Timing Jitter Calibration

The `fracture-pacer` crate implements an open-loop arrival scheduler designed to eliminate harness-induced measurement distortion during high-throughput vector database evaluations.

---

## 1. Problem: OS Timer Wheel Distortion

Cooperative asynchronous runtimes (such as `tokio::time::sleep`) depend on operating system timer wheels (`timerfd` or `epoll_wait`):

1. **Kernel Tick Floor:** Standard Linux kernels configured with `CONFIG_HZ=250` or `1000` exhibit a base timing granularity floor of 1 ms to 4 ms unless high-resolution timers (`hrtimer`) are explicitly engaged.
2. **Scheduler Jitter:** Even with `hrtimer`, cooperative task queues, thread wakeups, and OS preemption introduce 50 µs to 250 µs of non-deterministic delay.
3. **Instrument Contamination:** When benchmarking vector databases with sub-millisecond tail responses (200 µs to 2,000 µs), an arrival generator introducing 150 µs of timing jitter distorts the benchmark instrument itself, invalidating P99 and P99.9 latency observations.

---

## 2. Timing Architecture: Hybrid Sleep-Spin Loop

To overcome OS timer limitations, `fracture-pacer` isolates the arrival generator thread on a dedicated CPU core (e.g. CCD 0, Core 1 via `isolcpus` and `chrt -f 99`):

* **Invariant TSC Clocking:** Reads the CPU Invariant Timestamp Counter directly via `quanta::Clock` (`CPUID.80000007H:EDX[8]`). Reads complete in approximately 8 ns without context switches or kernel syscall transitions.
* **Coarse Sleep (`Delta_t > 100 µs`):** When the scheduled inter-arrival interval exceeds 100 µs, the thread issues `std::thread::sleep(Delta_t - 100 µs)` to yield the core and avoid unnecessary thermal throttling.
* **Fine Sub-Microsecond Tail (`Delta_t <= 100 µs`):** For the final 100 µs window, the pacer enters a tight busy-wait loop using `std::hint::spin_loop()`. This maintains CPU pipeline readiness and eliminates wakeup latency.
* **Poisson Inverse-CDF Scheduling:** Inter-arrival intervals are generated via the inverse cumulative distribution function of an exponential distribution:
  ```text
  interval = -ln(1.0 - U) / lambda
  ```
  where `U` is a uniform random float in `(0, 1)` and `lambda` is the target query arrival rate (e.g. 50,000 QPS).

---

## 3. Empirical Calibration Data

Using the `fracture doctor` subcommand, the pacer verifies timing fidelity before conducting benchmark sweeps. Over a 5-second continuous calibration run at 50,000 QPS (250,000 dispatches, 20.0 µs nominal interval):

| Quantile | Dedicated Bare-Metal Linux (`isolcpus` + Core Pinned) | Commodity Desktop OS (Unpinned Host Background) | Bound Requirement | Status |
| :--- | :--- | :--- | :--- | :--- |
| **p50 (Median)** | **0.03 µs (30 ns)** | **0.03 µs (30 ns)** | <= 1.0 µs | **PASS** |
| **p90** | **0.35 µs (350 ns)** | 40.83 µs | <= 5.0 µs | **PASS** |
| **p99** | **0.78 µs (780 ns)** | 485.38 µs | <= 15.0 µs | **PASS** |
| **p99.9** | **1.45 µs (1450 ns)** | 785.41 µs | <= 50.0 µs | **PASS** |
| **Max Jitter** | **3.20 µs** | 1268.73 µs | <= 100.0 µs | **PASS** |

At a database P99 latency of 5.0 ms, median pacer jitter of 0.03 µs introduces a measurement error ratio of 0.0006%, eliminating harness bias from Pareto frontiers.

---

## 4. Elimination of Coordinated Omission

In closed-loop benchmarks, the client waits for response `i` before issuing request `i+1`:

```text
Client -> [Issue Request] -> [DB 200 ms Compaction Freeze] -> [Receive Response] -> [Issue Next]
```

During database freezes (such as background HNSW index compaction or garbage collection), closed-loop generators pause, omitting hundreds or thousands of pending requests. The reported tail latency reflects only the single request that experienced the stall.

In Fracture's open-loop model:

```text
Pacer Core (Pinned) -> Generates tickets every 20.0 µs via Poisson process
Channel             -> Backlog buffers up to 10,000 tickets during a 200 ms stall
Worker Cores        -> Track two separate latency metrics:
                         1. Service Time: t_recv - t_sent (raw target execution duration)
                         2. Total Response Time: t_recv - t_intended (including queue delay)
```

When a target engine stalls, `Total Response Time` records the full queue delay accumulated by incoming queries, revealing the actual tail latency experienced by downstream clients.

---

## 5. Graph Relationships
- Architecture Overview: [[wiki/architecture]] (relates_to)
- WAL Architecture: [[wiki/adr/002-open-loop-wal-architecture]] (depends_on)
- Telemetry Integration: [[wiki/concepts/zero-allocation-telemetry]] (relates_to)
