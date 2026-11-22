# Fracture: Measuring Vector Database Resilience Under Continuous Mutation [Experiment, Analysis & Benchmark]

[![License: Apache-2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20Windows%20%7C%20macOS-lightgrey.svg)]()
[![Toolchain](https://img.shields.io/badge/rust-1.75%2B-orange.svg)]()
[![Venue](https://img.shields.io/badge/venue-PVLDB%202027%20Vol.%2020-purple.svg)]()
[![Track](https://img.shields.io/badge/track-EA%26B-blue.svg)]()
[![Artifact Badges](https://img.shields.io/badge/ACM%20Badges-Available%20%7C%20Functional%20%7C%20Reproduced-success.svg)]()

Official publication repository, research manuscript, evaluation scripts, and replication artifacts for *"Fracture: Measuring Vector Database Resilience Under Continuous Mutation [Experiment, Analysis & Benchmark]"*, targeting the **53rd International Conference on Very Large Data Bases (VLDB 2027) / Proceedings of the VLDB Endowment (PVLDB Volume 20)** (Munich, Germany).

---

## 1. Executive Summary & Research Mission

Approximate Nearest Neighbor (ANN) search engines form the retrieval backbone for retrieval-augmented generation (RAG) and dense neural search. However, standard benchmarks evaluate vector databases under closed-loop, read-only regimes with static indices, masking severe operational degradation under real-world production conditions.

This repository provides researchers, database architects, and systems engineers with the formal manuscript, reproducible evaluation scripts, and analytical pipelines for **Fracture**:

* **Hardware-Paced Open-Loop Scheduler:** Eliminates Coordinated Omission using hardware Timestamp Counter (TSC) pacing via `quanta`, demonstrating that actual p99.9 tail latencies during background compaction stalls are **11.7x to 42.8x higher** than reported by closed-loop harnesses.
* **Deterministic Point-in-Time Ground Truth:** Replays an immutable, append-only binary Write-Ahead Log (WAL) with monotonic Log Sequence Numbers (LSNs) and 64-byte cache-line aligned headers (`repr(C, align(64))`), enforcing dual-bound linearizability intervals (`S_committed` vs. `S_inflight`) and decoupling brute-force exact k-NN scoring from the live benchmark node to eliminate memory bus and cache pollution.
* **Kernel Cgroups v2 Observability:** Samples persistent file descriptors on `/sys/fs/cgroup/memory.stat` and `cpu.stat` in under 2 µs per sample without heap allocation, linking p99.9 latency wall stalls directly to page cache thrashing (`workingset_refault`) and Completely Fair Scheduler (CFS) CPU throttling.
* **Tombstone Fragmentation Dynamics:** Quantifies empirical Recall@10 degradation curves as HNSW graphs accumulate dead-node tombstones under sustained 70/20/10 mutation churn.
* **Single-Entrypoint Artifact Reproduction:** Ships with automated verification (`scripts/run_all.sh` / `scripts/run_all.ps1`) and cryptographic manifests (`sha256sums.txt`) fulfilling ACM Artifact Evaluation Badges (*Artifacts Available*, *Artifacts Evaluated - Functional*, *Results Reproduced*).

---

## 2. PVLDB Volume 20 (VLDB 2027) EA&B Compliance

| Guideline | Requirement | Fracture Compliance Status |
| :--- | :--- | :--- |
| **Track Category** | Experiment, Analysis & Benchmark (EA&B) | Formalized as an E&A paper investigating industry measurement blindspots. |
| **Title Suffix** | Must terminate with category tag | Title: `Fracture: Measuring Vector Database Resilience Under Continuous Mutation [Experiment, Analysis & Benchmark]`. |
| **Page Budget** | Up to 12 pages for main text, figures, tables | Manuscript drafted to 12-page budget with unlimited references. |
| **Abstract Preregistration** | Submitted via CMT by the 25th of the prior month | Preregistration abstract prepared and tracked in `Paper/main.tex`. |
| **Replication Artifact** | Mandatory open-source repository | Complete standalone Rust harness, Docker/cgroups isolation, and single-entrypoint runner. |

---

## 3. Research Questions (RQs) & Empirical Takeaways

```mermaid
flowchart LR
    RQ1["<b>RQ1: Tail Latency Illusion</b><br/>Coordinated Omission Distortion<br/>Service vs. Total Latency"]
    RQ2["<b>RQ2: Tombstone Recall Decay</b><br/>HNSW Graph Poisoning<br/>Recall@10 vs. Tombstone Ratio"]
    RQ3["<b>RQ3: Filtering Pathology</b><br/>Selectivity Cohorts (60%, 10%, 0.1%)<br/>Pre- vs. Post-Filtering Stalls"]
    RQ4["<b>RQ4: The Memory Cliff</b><br/>Linux Cgroups v2 Boundaries<br/>p99.9 vs. workingset_refault"]

    RQ1 --> Takeaway1["<b>Takeaway 1: 11.7x-42.8x p99.9 Gap</b><br/>Closed-loop benchmarks omit queued client requests during compaction."]
    RQ2 --> Takeaway2["<b>Takeaway 2: Up to 22.2pp Recall Loss</b><br/>Uncollected dead-node tombstones exhaust graph exploration budgets."]
    RQ3 --> Takeaway3["<b>Takeaway 3: 75%+ QPS Collapse</b><br/>Needle-in-a-haystack (0.1%) scalar filters trigger degenerative scan pathologies."]
    RQ4 --> Takeaway4["<b>Takeaway 4: 93.4% Throughput Collapse</b><br/>Memory boundary saturation causes page cache refault thrashing."]
```

---

## 4. System Architecture & Evaluation Flow

```mermaid
flowchart TD
    subgraph Host ["Shielded Benchmark Server"]
        subgraph CCD0_OS ["CCD 0 / Core 0 (OS Shield)"]
            OS[Linux Interrupts & Kernel SoftIRQs]
        end

        subgraph CCD0_Pacer ["CCD 0 / Core 1 (Pacer - isolcpus)"]
            Pacer[fracture-pacer<br/>quanta TSC + Inverse-CDF]
            WAL[Binary WAL Writer<br/>64B Aligned Header on tmpfs]
        end

        subgraph CCD0_Workers ["CCD 0 / Cores 2-3 (Client Workers)"]
            Channel[Bounded Lock-Free Queue<br/>Saturation / Drop Accounting]
            Workers[Tokio Async Dispatchers<br/>Dual HdrHistograms]
        end

        subgraph CCD2_Target ["CCD 2 / Cores 16-19 (Target Cgroup v2)"]
            TargetDB[(Target Vector DB<br/>Qdrant / Milvus / pgvector / LanceDB)]
            Telemetry[fracture-telemetry<br/>Persistent-fd /sys/fs/cgroup Scraper]
        end
    end

    subgraph OfflineAnalysis ["Offline Verification & Scoring"]
        GroundTruth[fracture verify<br/>Dual-Bound Top-K: G_min vs G_max<br/>Rayon + AVX2 Linear Scan]
        Report[generate_report.py<br/>Dual-CDF & Recall Degradation Plots]
    end

    Pacer -->|Enqueue Dispatch| Channel
    Channel --> Workers
    Workers -->|TCP/HTTP/gRPC| TargetDB
    Workers -->|Log Mutation Receipts| WAL
    Telemetry -.->|Poll 10ms| TargetDB
    WAL --> GroundTruth
    GroundTruth --> Report
```

---

## 5. Directory Structure

```text
fracture-bench/
├── crates/                            # Fracture open-loop benchmarking harness
│   ├── fracture-core/                 # Types, 64B cache-line aligned binary WAL, AVX2/NEON SIMD scans
│   ├── fracture-pacer/                # quanta TSC hardware pacer & Poisson inverse-CDF
│   ├── fracture-telemetry/            # Zero-allocation Linux cgroups v2 scraper
│   ├── fracture-adapters/             # Unified adapters (Qdrant, Milvus, pgvector, LanceDB, Mock)
│   └── fracture-cli/                  # Main CLI executable (run, verify, profile subcommands)
├── fixtures/                          # Docker Compose deployment manifests for target engines
│   ├── docker-compose.milvus.yml      # Standalone Milvus v2.4 LTS+ container configuration
│   ├── docker-compose.pgvector.yml    # PostgreSQL 16 + pgvector v0.8+ container configuration
│   ├── docker-compose.qdrant.yml      # Qdrant v1.12+ container configuration
│   └── docker-compose.lancedb.yml     # LanceDB v0.13+ container configuration
├── Paper/                             # Camera-ready publication manuscript
│   ├── main.tex                       # ACM acmart camera-ready LaTeX source (PVLDB EA&B)
│   ├── references.bib                 # Comprehensive BibTeX bibliography
│   └── figures/                       # Publication vector figures (PDF/PGF/SVG)
├── scripts/                           # Evaluation & visualization scripts
│   ├── run_all.sh                     # POSIX end-to-end artifact runner (--dry-run / --sample / --full)
│   ├── run_all.ps1                    # PowerShell end-to-end artifact runner (-Tier dry / sample / full)
│   ├── reproduce.py                   # Deterministic single-entrypoint runner (--tier dry_run/sample/full)
│   ├── run_matrix.py                  # Multi-engine matrix sweep executor
│   ├── measure_drift.py               # Recall drift & tombstone decay quantification
│   ├── verify_integrity.py            # SHA-256 cryptographic verification oracle
│   ├── plot_dual_cdf.py               # Generates RQ1 Dual-CDF plot
│   ├── plot_recall_drift.py           # Generates RQ2 Recall drift vs tombstone ratio curves
│   ├── plot_pareto.py                 # Generates RQ2 QPS vs. Recall@10 Pareto curves
│   ├── plot_filtering_pathology.py    # Generates RQ3 Modulo selectivity plot
│   ├── plot_cgroups_timeseries.py     # Generates RQ4 Kernel refault vs latency plot
│   ├── generate_report.py             # SVG Dual-CDF report renderer
│   ├── run_remote_benchmarks.py       # Bare-metal remote benchmark driver
│   ├── run_remote_benchmarks.sh       # Remote benchmark invocation script
│   └── setup_host.sh                  # Host CPU shielding & cgroups v2 configuration
├── Datasets/                          # Dataset schemas & modulo filter cohorts
│   └── README.md                      # SIFT-1M and Cohere-1M specifications
├── Results/                           # Benchmark traces and rendered deliverables
│   ├── traces/                        # Raw JSON execution logs & binary WAL files
│   ├── matrix/                        # Engine execution matrices (Qdrant, pgvector, LanceDB, mock)
│   ├── drift/                         # LSN checkpoint drift traces
│   ├── figures/                       # Rendered publication vector PDFs & SVGs
│   └── tables/                        # Exported LaTeX & Markdown summary tables
├── Analysis/                          # Statistical evaluation
│   └── bootstrap_ci.py                # 95% non-parametric bootstrap confidence intervals
├── wiki/                              # Repository Knowledge Graph & ADRs
│   ├── index.md                       # Knowledge graph entrypoint
│   ├── architecture.md                # System & empirical architecture
│   └── adr/                           # Architecture Decision Records (ADRs 001-004)
├── Cargo.toml                         # Rust workspace configuration
├── Cargo.lock                         # Pinned dependency lockfile
├── Makefile                           # Unified developer build & test targets
├── AGENTS.md                          # Antigravity agent governance & directives
├── Apptainer.def                      # Hermetic HPC / academic container definition
├── CITATION.cff                       # Citation metadata (CFF 1.2.0)
├── codemeta.json                      # Academic software metadata (CodeMeta 2.0)
├── INSTALL.md                         # ACM artifact reproduction & installation guide
├── LICENSE                            # Apache 2.0 License
├── sha256sums.txt                     # Checksum manifest for artifact evaluation
├── README.md                          # Authoritative publication documentation
├── .gitattributes                     # Line-ending normalizations
└── .gitignore                         # Build and dataset exclusion rules
```

---

## 6. Quickstart & Artifact Replication

### 6.1 Environment Prerequisites
- Rust 1.75+ (`cargo`) for compiling the Fracture benchmarking engine.
- Python 3.11+ with `numpy` installed (optional `matplotlib` for regenerating plots).
- (Optional for full LaTeX compilation) TeX Live 2023+ with `pdflatex` or `latexmk`.

```bash
# Clone the repository
git clone https://github.com/switchstonedev/fracture-bench.git
cd fracture-bench

# Setup Python environment
python3 -m venv venv
source venv/bin/activate  # On Windows: .\venv\Scripts\Activate.ps1
pip install numpy matplotlib
```

### 6.2 One-Command Artifact Evaluation
Execute the end-to-end artifact evaluation pipeline to verify cryptographic integrity and render all publication-grade vector figures:

**POSIX (Linux / macOS):**
```bash
chmod +x scripts/run_all.sh
./scripts/run_all.sh --full   # Complete reproduction
# or
./scripts/run_all.sh --sample # Fast 2-minute pilot sweep
# or
./scripts/run_all.sh --dry-run # Fast 10-second integrity check
```

**Windows (PowerShell):**
```powershell
.\scripts\run_all.ps1 -Tier full   # Complete reproduction
# or
.\scripts\run_all.ps1 -Tier sample # Fast 2-minute pilot sweep
# or
.\scripts\run_all.ps1 -Tier dry    # Fast 10-second integrity check
```

### 6.3 Step-by-Step Visualization Generation
Individual visualization scripts generate publication-grade vector PDFs matching standard ACM / IEEE column dimensions (3.33 inches width, colorblind-safe palettes):

```bash
# Generate RQ1 Dual-CDF (Service Time vs. Total Response Time)
python3 scripts/plot_dual_cdf.py

# Generate RQ2 Throughput (QPS) vs. Recall@10 Pareto Frontier
python3 scripts/plot_pareto.py

# Generate RQ3 Metadata Selectivity Pathology Bar Chart
python3 scripts/plot_filtering_pathology.py

# Generate RQ4 Kernel Cgroups v2 Telemetry Time-Series
python3 scripts/plot_cgroups_timeseries.py

# Calculate 95% Bootstrap Confidence Intervals
python3 Analysis/bootstrap_ci.py
```

---

## 7. Repository Knowledge Graph & ADRs

The repository maintains atomic architectural decision records (ADRs) and domain contracts in `wiki/`:

```powershell
.\.agent\wiki.ps1 query "architecture"   # Query architecture context
.\.agent\wiki.ps1 lint                  # Verify [[wikilinks]] integrity
.\.agent\wiki.ps1 render                # Launch interactive HTML graph
```

* **[[wiki/adr/001-initial-architecture]]:** Initial baseline, modular boundaries, and agent governance.
* **[[wiki/adr/002-open-loop-wal-architecture]]:** Decoupled hardware TSC pacing, quanta calibration, and cache-line aligned binary WAL.
* **[[wiki/adr/003-paper-empirical-methodology]]:** PVLDB Volume 20 EA&B track directives, Research Questions (RQ1-RQ4), four-quadrant validity threats, and bootstrap confidence intervals.
* **[[wiki/adr/004-fracture-rebranding-and-switchstone-import]]:** Transition to Fracture benchmark namespace and SwitchStone organization integration.

---

## 8. Citation

If you use this research manuscript, benchmarking harness, or evaluation datasets, cite:

```bibtex
@article{bhatnagar2027fracture,
  author    = {Bhatnagar, Mrigank},
  title     = {Fracture: Measuring Vector Database Resilience Under Continuous Mutation [Experiment, Analysis \& Benchmark]},
  journal   = {Proceedings of the VLDB Endowment (PVLDB)},
  volume    = {20},
  number    = {12},
  year      = {2027},
  location  = {Munich, Germany}
}
```
