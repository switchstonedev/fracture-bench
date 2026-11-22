# Fracture Empirical Benchmark Summary

| Target Engine | Workload | Target QPS | Achieved QPS | Service p99 (µs) | Total p99 (µs) | Recall@10 | Execution Status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| mock | churn-70-20-10 | 1000 | {achievedQps_mock} | {serviceP99_mock} | {totalP99_mock} | {recall10_mock} | dry_run |
| mock | read-heavy-90-10 | 1000 | {achievedQps_mock} | {serviceP99_mock} | {totalP99_mock} | {recall10_mock} | dry_run |
| mock | write-heavy-50-50 | 1000 | {achievedQps_mock} | {serviceP99_mock} | {totalP99_mock} | {recall10_mock} | dry_run |
| qdrant | churn-70-20-10 | 1000 | {achievedQps_qdrant} | {serviceP99_qdrant} | {totalP99_qdrant} | {recall10_qdrant} | dry_run |
| qdrant | read-heavy-90-10 | 1000 | {achievedQps_qdrant} | {serviceP99_qdrant} | {totalP99_qdrant} | {recall10_qdrant} | dry_run |
| qdrant | write-heavy-50-50 | 1000 | {achievedQps_qdrant} | {serviceP99_qdrant} | {totalP99_qdrant} | {recall10_qdrant} | dry_run |
| pgvector | churn-70-20-10 | 1000 | {achievedQps_pgvector} | {serviceP99_pgvector} | {totalP99_pgvector} | {recall10_pgvector} | dry_run |
| pgvector | read-heavy-90-10 | 1000 | {achievedQps_pgvector} | {serviceP99_pgvector} | {totalP99_pgvector} | {recall10_pgvector} | dry_run |
| pgvector | write-heavy-50-50 | 1000 | {achievedQps_pgvector} | {serviceP99_pgvector} | {totalP99_pgvector} | {recall10_pgvector} | dry_run |
| milvus | churn-70-20-10 | 1000 | {achievedQps_milvus} | {serviceP99_milvus} | {totalP99_milvus} | {recall10_milvus} | dry_run |
| milvus | read-heavy-90-10 | 1000 | {achievedQps_milvus} | {serviceP99_milvus} | {totalP99_milvus} | {recall10_milvus} | dry_run |
| milvus | write-heavy-50-50 | 1000 | {achievedQps_milvus} | {serviceP99_milvus} | {totalP99_milvus} | {recall10_milvus} | dry_run |
| lancedb | churn-70-20-10 | 1000 | {achievedQps_lancedb} | {serviceP99_lancedb} | {totalP99_lancedb} | {recall10_lancedb} | dry_run |
| lancedb | read-heavy-90-10 | 1000 | {achievedQps_lancedb} | {serviceP99_lancedb} | {totalP99_lancedb} | {recall10_lancedb} | dry_run |
| lancedb | write-heavy-50-50 | 1000 | {achievedQps_lancedb} | {serviceP99_lancedb} | {totalP99_lancedb} | {recall10_lancedb} | dry_run |
