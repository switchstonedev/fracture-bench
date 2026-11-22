---
title: Three-Tier Empirical Benchmark Hierarchy & Reproduction Harness
summary: Architectural decision establishing deterministic single-entrypoint reproduction, three-tier benchmark execution hierarchy, and standardized metric placeholder discipline.
type: adr
status: active
tags: [adr, benchmarking, reproducibility, pareto, placeholders]
aliases: [adr-003, three-tier-eval]
---

# ADR 003: Three-Tier Empirical Benchmark Hierarchy & Reproduction Harness

## Context
Benchmarking engineers and operators evaluating vector database performance operate under varying resource and time constraints (e.g., quick local sanity checks versus multi-hour exhaustive sweeps on high-memory multi-node clusters). Concurrently, running continuous heavy load runs on development laptops risks memory exhaustion and CPU lockup. Furthermore, in benchmark workflows, baselines that have not yet executed must never be populated with fabricated numbers.

## Decision
1. **Deterministic Single Entrypoint (`scripts/reproduce.py`):**
   Provide a unified, non-interactive Python script that orchestrates environment sanity checks, matrix benchmark sweeps, and automatic generation of benchmark tables and figures.
2. **Three-Tier Execution Hierarchy:**
   - **Level 1 — Dry Run (`--tier dry_run` / `--dry-run`):** Ultra-fast sanity check (< 30 seconds) verifying CLI flags, engine network reachability, and dataset manifests with zero compute or memory strain.
   - **Level 2 — Sample / Pilot (`--tier sample` / `--sample`):** Fast pilot run (1 to 3 minutes) executing small corpus ingestion (200 vectors) and 1-second benchmark runs against active engines (`mock`, `qdrant`, `pgvector`).
   - **Level 3 — Full Benchmark (`--tier full` / `--full`):** Complete experimental sweep replicating full target QPS curves (1000 to 10000 QPS) across all configured database engines.
3. **Standardized Metric Placeholder Policy:**
   Unexecuted cells or offline engines emit standardized machine-readable placeholders formatted strictly as `{metric_originTech}` (e.g. `{achievedQps_milvus}`, `{recall10_pgvector}`, `{serviceP99_milvus}`). Hardcoded or fabricated metrics are strictly forbidden.
4. **Dual Output Lockstep (Markdown & Structured Tables):**
   Generate vector figures (`dual_cdf.svg`, `pareto_frontier.svg`), plain-text GitHub Flavored Markdown summary tables, and structured data blocks directly from raw result matrices without manual copy-pasting.

## Consequences
- Developers and benchmark operators can verify end-to-end functionality in under 30 seconds without pinning CPU resources.
- Partial runs maintain complete transparency; readers and operators can immediately identify executed versus pending benchmarks.
- Benchmark figures and tables remain synchronized with benchmark runs at all times.

## References
- System Architecture: [[wiki/architecture]] (relates_to)
- WAL Replay: [[wiki/adr/002-open-loop-wal-architecture]] (relates_to)
