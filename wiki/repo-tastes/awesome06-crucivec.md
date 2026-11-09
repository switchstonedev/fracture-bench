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
