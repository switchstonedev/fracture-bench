# Fracture Reproduction & Installation Guide

This guide provides step-by-step instructions to install, verify, and reproduce the empirical results for *"Fracture: Measuring Vector Database Resilience Under Continuous Mutation [Experiment, Analysis & Benchmark]"* in compliance with ACM Artifact Review and Badging Version 1.1 (*Artifacts Available*, *Artifacts Evaluated - Functional*, *Results Reproduced*).

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
* **Container Runtimes (Optional):** Docker Engine 24.0+ / Docker Compose v2, or Apptainer / Singularity 3.8+ for rootless HPC environments.

---

## 2. Multi-Tier Benchmark Execution Hierarchy

To accommodate reviewer time constraints, Fracture exposes a deterministic three-tier reproduction harness:

| Tier | Flag | Target Duration | Description |
| :--- | :--- | :--- | :--- |
| **Level 1 — Dry Run** | `--dry-run` / `--tier dry_run` | 10–30 seconds | Fast sanity check. Verifies audited checksum manifest (`sha256sums.txt`), executes mock workload sweeps, generates sample reports, and renders SVG curves. |
| **Level 2 — Sample / Pilot** | `--sample` / `--tier sample` | 2–5 minutes | Evaluates accessible local engines and mock harness under 1,000 QPS across all 3 workloads (churn-70-20-10, read-heavy-90-10, write-heavy-50-50). |
| **Level 3 — Full Benchmark** | `--full` / `--tier full` | 3–4 hours | Full experimental matrix replicating all paper claims, multi-engine comparisons (Qdrant, pgvector, Milvus, LanceDB), and Pareto frontier sweeps. |

---

## 3. Quickstart Reproduction (Single-Entrypoint)

### Step 1: Clone and Inspect
```bash
git clone https://github.com/Awesome06/paper6.git
cd paper6
```

### Step 2: Level 1 Dry-Run Verification (< 30 seconds)
```bash
python Scripts/reproduce.py --tier dry_run
# or alternatively via Makefile:
make bench-dry
```
This performs:
1. Audited SHA-256 digest verification (`sha256sums.txt`).
2. Rust binary compilation check.
3. Dry-run execution of workload matrix.
4. Rendering of publication SVGs (`reports/figures/dual_cdf.svg`, `reports/figures/pareto_frontier.svg`, `reports/figures/recall_drift.svg`).
5. Generation of summary tables (`reports/benchmark_summary.md`, `reports/benchmark_summary.tex`).

### Step 3: Level 2 Sample Pilot Run (2–5 minutes)
```bash
python Scripts/reproduce.py --tier sample
# or via Makefile:
make bench-sample
```

### Step 4: Level 3 Full Benchmark Run
```bash
# Optional: Start background vector engines via Docker Compose
make docker-up

# Execute complete matrix sweep
python Scripts/reproduce.py --tier full
# or via Makefile:
make bench-full
```

---

## 4. Containerized Reproduction

### Rootless Docker
```bash
# Build hermetic reproduction container
docker build -t fracture-paper .

# Execute Level 1 dry-run inside isolated container
docker run --rm -v $(pwd)/reports:/workspace/reports fracture-paper python3 Scripts/reproduce.py --tier dry_run
```

### Apptainer / Singularity (HPC & Multi-User Clusters)
```bash
# Build Singularity Image Format (SIF)
apptainer build fracture-paper.sif Apptainer.def

# Execute dry-run without root privileges
apptainer run fracture-paper.sif --tier dry_run
```

---

## 5. Automated Runtime Fallback Matrix

| Condition | Failure State | Automated Fallback Behavior |
| :--- | :--- | :--- |
| **Checksum Mismatch** | Corrupted or modified fixture | Aborts execution immediately, reporting the corrupted file path. |
| **Offline Engine** | Docker or database container unstarted | Detects connection failure and preserves `{metric_originTech}` placeholders without crashing matrix. |
| **Host Without cgroups v2** | Windows, macOS, or legacy Linux | Fallback to user-space thread timing with explicit notice. |
| **Reviewer Time Limit** | Limited evaluation window | Run Level 1 (`--dry-run`) or Level 2 (`--sample`) for complete validation within minutes. |
