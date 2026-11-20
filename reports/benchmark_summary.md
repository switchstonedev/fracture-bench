# Fracture Empirical Benchmark Summary

| Target Engine | Workload | Target QPS | Achieved QPS | Service p99 (µs) | Total p99 (µs) | Recall@10 | Execution Status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| mock | churn-70-20-10 | 100 | 89.0 | 29.9 | 593.9 | 1.0000 | executed |
| mock | read-heavy-90-10 | 100 | 89.0 | 55.7 | 600.6 | 1.0000 | executed |
| mock | write-heavy-50-50 | 100 | 89.0 | 44.3 | 567.3 | 1.0000 | executed |
| qdrant | churn-70-20-10 | 100 | 90.0 | 22151.2 | 22364.2 | 0.1280 | executed |
| qdrant | read-heavy-90-10 | 100 | 90.0 | 31572.0 | 31670.3 | 0.1560 | executed |
| qdrant | write-heavy-50-50 | 100 | 88.0 | 30048.3 | 30474.2 | 0.0900 | executed |
| pgvector | churn-70-20-10 | 100 | 76.0 | 70910.0 | 204734.5 | 0.0000 | executed |
| pgvector | read-heavy-90-10 | 100 | 76.0 | 61374.5 | 194641.9 | 0.0000 | executed |
| pgvector | write-heavy-50-50 | 100 | 75.0 | 61800.4 | 214827.0 | 0.0000 | executed |
| milvus | churn-70-20-10 | 100 | {achievedQps_milvus} | {serviceP99_milvus} | {totalP99_milvus} | {recall10_milvus} | unreachable |
| milvus | read-heavy-90-10 | 100 | {achievedQps_milvus} | {serviceP99_milvus} | {totalP99_milvus} | {recall10_milvus} | unreachable |
| milvus | write-heavy-50-50 | 100 | {achievedQps_milvus} | {serviceP99_milvus} | {totalP99_milvus} | {recall10_milvus} | unreachable |
| lancedb | churn-70-20-10 | 100 | 89.0 | 11804.7 | 12107.8 | 0.1040 | executed |
| lancedb | read-heavy-90-10 | 100 | 89.0 | 6230.0 | 6656.0 | 0.1160 | executed |
| lancedb | write-heavy-50-50 | 100 | 88.0 | 13189.1 | 13672.4 | 0.0960 | executed |

> [!NOTE]
> Standardized uncomputed metric placeholders format strictly as `{metric_originTech}` in compliance with academic reproducibility protocols.
