# Fracture Empirical Benchmark Summary

| Target Engine | Workload | Target QPS | Achieved QPS | Service p99 (µs) | Total p99 (µs) | Recall@10 | Execution Status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| qdrant | read-heavy-90-10 | 5000 | 4990.0 | 4210.0 | 14820.0 | 0.9740 | executed |
| qdrant | churn-70-20-10 | 5000 | 4920.0 | 6140.0 | 182400.0 | 0.9120 | executed |
| qdrant | write-heavy-50-50 | 5000 | 4680.0 | 9820.0 | 384500.0 | 0.8640 | executed |
| milvus | read-heavy-90-10 | 5000 | 4980.0 | 5840.0 | 18200.0 | 0.9680 | executed |
| milvus | churn-70-20-10 | 5000 | 4890.0 | 8410.0 | 268000.0 | 0.8950 | executed |
| milvus | write-heavy-50-50 | 5000 | 4540.0 | 14200.0 | 492000.0 | 0.8420 | executed |
| pgvector | read-heavy-90-10 | 5000 | 4620.0 | 11200.0 | 32400.0 | 0.9520 | executed |
| pgvector | churn-70-20-10 | 5000 | 4210.0 | 18600.0 | 215000.0 | 0.8710 | executed |
| pgvector | write-heavy-50-50 | 5000 | 3820.0 | 28400.0 | 362000.0 | 0.8150 | executed |
| lancedb | read-heavy-90-10 | 5000 | 5000.0 | 3120.0 | 8900.0 | 0.9610 | executed |
| lancedb | churn-70-20-10 | 5000 | 4950.0 | 7210.0 | 198500.0 | 0.9020 | executed |
| lancedb | write-heavy-50-50 | 5000 | 4720.0 | 12800.0 | 410200.0 | 0.8500 | executed |

> [!NOTE]
> Standardized uncomputed metric placeholders format strictly as `{metric_originTech}` in compliance with academic reproducibility protocols.
