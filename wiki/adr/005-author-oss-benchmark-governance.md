---
title: Author-OSS Benchmark Suite Governance & Deliverables Architecture
summary: Architectural transition from academic publication artifact packaging to authoritative author-maintained open source (mode: author-oss), establishing unified SVG/Markdown deliverables and directory governance.
type: adr
status: active
tags: [adr, governance, author-oss, benchmarking, deliverables]
aliases: [adr-005, author-oss-governance]
---

# ADR 005: Author-OSS Benchmark Suite Governance & Deliverables Architecture

## Context
Fracture was originally structured around academic research paper publication workflows, camera-ready LaTeX templates, and double-blind artifact evaluation requirements (e.g. `Paper/`, LaTeX tabular summary `.tex` exports, pre-rendered PDF figures, `CITATION.cff`, and rigid SHA-256 integrity checksum files). In an author-maintained open-source benchmark suite (`mode: author-oss`), these academic assets create directory clutter, redundant output split-brains (`reports/` vs `results/`), and operational friction without providing value to systems engineers, database operators, or downstream contributors.

## Decision
1. **Author-OSS Governance Declaration (`mode: author-oss`):**
   Formally transition repository governance to Author-Maintained Open-Source. The author/maintainer exercises architectural authority, rapid prototyping, and direct feature velocity without upstream committee gatekeeping or academic review bureaucracy.
2. **Purge Academic & Paper Overhead:**
   Permanently delete all publication manuscript sources (`Paper/`), academic reproduction guides, citation metadata manifests (`CITATION.cff`, `codemeta.json`), container packaging descriptors (`Apptainer.def`), and checksum oracles (`sha256sums.txt`).
3. **Web-First & GFM Deliverables (Zero LaTeX Invariant):**
   Standardize all visual and tabular outputs on native GitHub-Flavored Markdown (`benchmark_summary.md`), machine-readable JSON sweeps, and web-native vector SVGs (`dual_cdf.svg`, `pareto_frontier.svg`, `recall_drift.svg`). Eliminate all LaTeX `.tex` table generation and print-oriented vector PDF figures.
4. **Standardize Benchmark Evaluation Pillars:**
   Replace paper-style "Research Questions (RQ1-RQ4)" with operational Benchmark Evaluation Pillars:
   - Pillar 1: Tail Latency Under Coordinated Omission (Service vs. Total Latency)
   - Pillar 2: Tombstone Recall Decay (HNSW Graph Poisoning Under Churn)
   - Pillar 3: Metadata Filtering Pathology (Selectivity Cohort Stalls)
   - Pillar 4: Cgroups v2 Memory Boundary Cliff (Workingset Refault Thrashing)
5. **Unified Output Directory Strategy:**
   Eliminate the split-brain between `reports/` and `results/` by consolidating benchmark deliverables into a single canonical directory hierarchy.

## Consequences
- The codebase remains lean, modern, and aligned with industry-standard vector database benchmarking repositories (such as `vector-db-benchmark` and `ann-benchmarks`).
- Visualization and summary tables render natively in web browsers and GitHub Markdown viewers without requiring LaTeX toolchains or PDF viewers.
- Automated CI workflows and local benchmark invocations execute cleanly across all three tiers (dry-run, sample, full) without checksum verification overhead.

## References
- Root Knowledge Graph: [[wiki/index]] (relates_to)
- System Architecture: [[wiki/architecture]] (relates_to)
- Three-Tier Benchmark Evaluation: [[wiki/adr/003-three-tier-evaluation-and-reproduction]] (relates_to)
