# Fracture Reproduction & Installation Guide

This guide provides step-by-step instructions to install, verify, and reproduce the empirical benchmarks for Fracture: Measuring Vector Database Resilience Under Continuous Mutation.

---

## 1. Hardware Profile & System Requirements

### Hardware Requirements
| Resource | Minimum Specification | Recommended Specification |
| :--- | :--- | :--- |
| **CPU Architecture** | x86_64 with AVX2 support | x86_64 AMD Zen 3/Zen 4 EPYC or Intel Xeon Scalable |
| **Physical Cores** | 8 cores | 16+ cores with shielded core pinning (`isolcpus`) |
| **Hardware Counter** | Invariant TSC (`CPUID.80000007H:EDX[8]`) | Invariant TSC for sub-10 ns hardware timestamping |
| **System Memory** | 16 GB RAM | 64+ GB RAM (for 1M vector high-churn stress sweeps) |
| **Disk Storage** | 20 GB free space (SSD) | 50+ GB NVMe SSD |
| **Operating System** | Linux (Kernel >= 5.15) with cgroups v2 | Ubuntu 22.04 LTS / Debian 12 with systemd cgroups v2 |

### Software Dependencies
* **Rust Toolchain:** 1.75+ (`rustup default stable`)
* **Python Runtime:** 3.10+ with standard packages (`numpy`, `pandas`, `matplotlib`)
* **Container Runtimes (Optional):** Docker Engine 24.0+ / Docker Compose v2.

---

## 2. Multi-Tier Benchmark Execution Hierarchy

To accommodate varying hardware and time budgets, Fracture exposes a deterministic three-tier reproduction harness:

| Tier | Flag | Target Duration | Description |
| :--- | :--- | :--- | :--- |
| **Level 1 — Dry Run** | `--dry-run` / `--tier dry_run` | 10–30 seconds | Fast sanity check. Verifies CLI flags, executes mock workload sweeps, generates sample reports, and renders SVG curves. |
| **Level 2 — Sample / Pilot** | `--sample` / `--tier sample` | 2–5 minutes | Evaluates accessible local engines and mock harness under 1,000 QPS across all 3 workloads (churn-70-20-10, read-heavy-90-10, write-heavy-50-50). |
| **Level 3 — Full Benchmark** | `--full` / `--tier full` | 3–4 hours | Full experimental matrix replicating all benchmark targets, multi-engine comparisons (Qdrant, pgvector, Milvus, LanceDB), and Pareto frontier sweeps. |

---

## 3. Quickstart Reproduction (Single-Entrypoint)

### Step 1: Clone and Inspect
```bash
git clone https://github.com/switchstonedev/fracture-bench.git
cd fracture-bench
```

### Step 2: Level 1 Dry-Run Verification (< 30 seconds)
```bash
python scripts/reproduce.py --tier dry_run
# or alternatively via Makefile:
make bench-dry
```
This performs:
1. Rust binary compilation check.
2. Dry-run execution of workload matrix.
3. Rendering of benchmark SVGs (`reports/figures/dual_cdf.svg`, `reports/figures/pareto_frontier.svg`, `reports/figures/recall_drift.svg`).
4. Generation of summary tables (`reports/benchmark_summary.md`, `reports/benchmark_summary.tex`).

### Step 3: Level 2 Sample Pilot Run (2–5 minutes)
```bash
python scripts/reproduce.py --tier sample
# or via Makefile:
make bench-sample
```

### Step 4: Level 3 Full Benchmark Run
```bash
# Optional: Start background vector engines via Docker Compose
make docker-up

# Execute complete matrix sweep
python scripts/reproduce.py --tier full
# or via Makefile:
make bench-full
```

---

## 4. Containerized Reproduction

### Rootless Docker
```bash
# Build hermetic reproduction container
docker build -t fracture-bench .

# Execute Level 1 dry-run inside isolated container
docker run --rm -v $(pwd)/reports:/workspace/reports fracture-bench python3 scripts/reproduce.py --tier dry_run
```

---

## 5. Automated Runtime Fallback Matrix

| Condition | Failure State | Automated Fallback Behavior |
| :--- | :--- | :--- |
| **Offline Engine** | Docker or database container unstarted | Detects connection failure and preserves `{metric_originTech}` placeholders without crashing matrix. |
| **Host Without cgroups v2** | Windows, macOS, or legacy Linux | Fallback to user-space thread timing with explicit notice. |
| **Time Limit** | Limited evaluation window | Run Level 1 (`--dry-run`) or Level 2 (`--sample`) for complete validation within minutes. |
