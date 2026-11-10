---
title: Distilled Lessons - awesome06_crucivec
summary: Architectural lessons and failure traps captured during development.
type: repo-taste
status: active
tags: [distillation, memory, awesome06_crucivec]
---

# Distilled Architectural Lessons: awesome06_crucivec

- See [[index]] for knowledge graph navigation.

## 4. Distilled Architectural Lessons


### Session Distillation: awesome06_crucivec
- **Modified Components:** `crucivec-pacer`, `crucivec-adapters`, `crucivec-cli`, `crucivec-core`, `crucivec-telemetry`
- **Architectural Invariant 1 (Core 1 Pacer WAL Emission):** Emitting mutations and appending 32-byte binary WAL headers on the dedicated pacer thread guarantees strict monotonic LSN ordering without lock contention between worker threads.
- **Architectural Invariant 2 (Point-in-Time Query Cutoff):** Query dispatch tickets capture the current observed LSN boundary, enabling exact offline point-in-time Recall@K evaluation against reconstructed dataset snapshots.
- **Architectural Invariant 3 (Zero-External-SDK Adapters):** Target database clients (Qdrant, Milvus, pgvector, LanceDB) implement native async HTTP/1.1 and PostgreSQL v3 protocol engines using Tokio primitives without requiring third-party SDK dependencies.

### Session Distillation: awesome06_crucivec
- **Modified Components:** .agent/hooks/pre-commit.ps1
- **Encountered Trap:** 17: - **Encountered Trap:** git : error: .agent/hooks/pre-commit.sh: cannot add to the index - missing --add option?
- **Resolution:** Invariant verified via deterministic test suite.

### Session Distillation: awesome06_crucivec
- **Modified Components:** `crucivec-adapters`, `crucivec-cli`, `crucivec-core`, `fixtures`
- **Architectural Invariant 4 (Native Batch Ingestion & Corpus Seeding):** Pre-populating target vector engines via `upsert_batch` achieves over 7,400 vec/s while recording sequential WAL LSNs (1..N) to establish deterministic ground truth prior to open-loop churn benchmarking.
- **Architectural Invariant 5 (Payload & SQL Filter Pushdown):** Stratified modulo selectivity cohorts (High 60%, Medium 10%, Needle 0.1%) map natively to indexed integer payload filters in Qdrant and SQL modulo predicates in pgvector.
- **Encountered Trap:** Host PostgreSQL service conflict on default port 5432; resolved by remapping fixture port to 5433 with `POSTGRES_HOST_AUTH_METHOD=trust`.

### Session Distillation: awesome06_crucivec
- **Modified Components:** None
- **Encountered Trap:** 125:                     anyhow::bail!("PostgreSQL authentication error: {}", err_str);
- **Resolution:** Invariant verified via deterministic test suite.
