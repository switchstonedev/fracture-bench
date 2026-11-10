# CruciVec Empirical Benchmark Summary

| Target Engine | Workload | Target QPS | Achieved QPS | Service p99 (µs) | Total p99 (µs) | Recall@10 | Execution Status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| mock | churn-70-20-10 | 100 | 72.0 | 21.0 | 201326.6 | 1.0000 | executed |
| mock | read-heavy-90-10 | 100 | 72.0 | 156.0 | 201064.4 | 1.0000 | executed |
| mock | write-heavy-50-50 | 100 | 73.0 | 34.1 | 200671.2 | 1.0000 | executed |
| qdrant | churn-70-20-10 | 100 | 71.0 | 49807.4 | 250347.5 | 0.0800 | executed |
| qdrant | read-heavy-90-10 | 100 | 71.0 | 49905.7 | 250216.4 | 0.0980 | executed |
| qdrant | write-heavy-50-50 | 100 | 70.0 | 51544.1 | 251527.2 | 0.0460 | executed |
| pgvector | churn-70-20-10 | 100 | 60.0 | 77463.6 | 377225.2 | 0.4840 | executed |
| pgvector | read-heavy-90-10 | 100 | 64.0 | 69271.6 | 352583.7 | 0.5200 | executed |
| pgvector | write-heavy-50-50 | 100 | 61.0 | 80085.0 | 360972.3 | 0.4280 | executed |
| milvus | churn-70-20-10 | 100 | {achievedQps_milvus} | {serviceP99_milvus} | {totalP99_milvus} | {recall10_milvus} | unreachable |
| milvus | read-heavy-90-10 | 100 | {achievedQps_milvus} | {serviceP99_milvus} | {totalP99_milvus} | {recall10_milvus} | unreachable |
| milvus | write-heavy-50-50 | 100 | {achievedQps_milvus} | {serviceP99_milvus} | {totalP99_milvus} | {recall10_milvus} | unreachable |
| lancedb | churn-70-20-10 | 100 | 72.0 | 539.1 | 201195.5 | 0.0000 | executed |
| lancedb | read-heavy-90-10 | 100 | 71.0 | 942.1 | 201064.4 | 0.0000 | executed |
| lancedb | write-heavy-50-50 | 100 | 73.0 | 864.3 | 201457.7 | 0.0000 | executed |

> [!NOTE]
> Standardized uncomputed metric placeholders format strictly as `{metric_originTech}` in compliance with academic reproducibility protocols.
