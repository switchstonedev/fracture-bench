# CruciVec Empirical Benchmark Summary

| Target Engine | Workload | Target QPS | Achieved QPS | Service p99 (µs) | Total p99 (µs) | Recall@10 | Execution Status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| mock | churn-70-20-10 | 100 | 72.0 | 37.4 | 200540.2 | 1.0000 | executed |
| mock | read-heavy-90-10 | 100 | 71.0 | 40.6 | 200671.2 | 1.0000 | executed |
| mock | write-heavy-50-50 | 100 | 72.0 | 151.7 | 200540.2 | 1.0000 | executed |
| qdrant | churn-70-20-10 | 100 | 71.0 | 29245.4 | 229244.9 | 0.0800 | executed |
| qdrant | read-heavy-90-10 | 100 | 71.0 | 21086.2 | 220987.4 | 0.0980 | executed |
| qdrant | write-heavy-50-50 | 100 | 71.0 | 49938.4 | 250216.4 | 0.0460 | executed |
| pgvector | churn-70-20-10 | 100 | 64.0 | 55640.1 | 321388.5 | 0.4840 | executed |
| pgvector | read-heavy-90-10 | 100 | 64.0 | 68157.4 | 329515.0 | 0.5200 | executed |
| pgvector | write-heavy-50-50 | 100 | 63.0 | 67436.5 | 332660.7 | 0.4120 | executed |
| milvus | churn-70-20-10 | 100 | {achievedQps_milvus} | {serviceP99_milvus} | {totalP99_milvus} | {recall10_milvus} | unreachable |
| milvus | read-heavy-90-10 | 100 | {achievedQps_milvus} | {serviceP99_milvus} | {totalP99_milvus} | {recall10_milvus} | unreachable |
| milvus | write-heavy-50-50 | 100 | {achievedQps_milvus} | {serviceP99_milvus} | {totalP99_milvus} | {recall10_milvus} | unreachable |
| lancedb | churn-70-20-10 | 100 | 72.0 | 455.4 | 201195.5 | 0.0000 | executed |
| lancedb | read-heavy-90-10 | 100 | 71.0 | 409.3 | 200802.3 | 0.0000 | executed |
| lancedb | write-heavy-50-50 | 100 | 73.0 | 581.1 | 201064.4 | 0.0000 | executed |

> [!NOTE]
> Standardized uncomputed metric placeholders format strictly as `{metric_originTech}` in compliance with academic reproducibility protocols.
