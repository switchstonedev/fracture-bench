use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::Duration;

use async_trait::async_trait;
use crucivec_core::simd::{dot_product, euclidean_distance_sq};
use crucivec_core::types::{CollectionSchema, DistanceMetric, FilterPredicate};

use crate::VectorEngineAdapter;

/// In-memory mock adapter used for verification, dry-runs, and regression testing.
pub struct MockVectorAdapter {
    vectors: RwLock<HashMap<u64, Vec<f32>>>,
    metric: RwLock<DistanceMetric>,
    stall_interval_ticks: u64,
    stall_duration_ms: u64,
    call_counter: AtomicU64,
}

impl Default for MockVectorAdapter {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

impl MockVectorAdapter {
    /// Creates a mock adapter. If `stall_interval_ticks > 0`, artificially injects
    /// `stall_duration_ms` latency every `stall_interval_ticks` calls to simulate index compaction pauses.
    pub fn new(stall_interval_ticks: u64, stall_duration_ms: u64) -> Self {
        Self {
            vectors: RwLock::new(HashMap::new()),
            metric: RwLock::new(DistanceMetric::Euclidean),
            stall_interval_ticks,
            stall_duration_ms,
            call_counter: AtomicU64::new(0),
        }
    }

    async fn maybe_stall(&self) {
        if self.stall_interval_ticks > 0 && self.stall_duration_ms > 0 {
            let count = self.call_counter.fetch_add(1, Ordering::Relaxed);
            if count > 0 && count.is_multiple_of(self.stall_interval_ticks) {
                tokio::time::sleep(Duration::from_millis(self.stall_duration_ms)).await;
            }
        }
    }
}

#[async_trait]
impl VectorEngineAdapter for MockVectorAdapter {
    async fn init(&self, schema: &CollectionSchema) -> anyhow::Result<()> {
        let mut metric = self.metric.write().unwrap();
        *metric = schema.metric;
        let mut vecs = self.vectors.write().unwrap();
        vecs.clear();
        Ok(())
    }

    async fn upsert(&self, id: u64, vector: &[f32]) -> anyhow::Result<()> {
        self.maybe_stall().await;
        let mut vecs = self.vectors.write().unwrap();
        vecs.insert(id, vector.to_vec());
        Ok(())
    }

    async fn upsert_batch(&self, points: &[(u64, Vec<f32>)]) -> anyhow::Result<()> {
        self.maybe_stall().await;
        let mut vecs = self.vectors.write().unwrap();
        for (id, vec) in points {
            vecs.insert(*id, vec.clone());
        }
        Ok(())
    }

    async fn delete(&self, id: u64) -> anyhow::Result<()> {
        self.maybe_stall().await;
        let mut vecs = self.vectors.write().unwrap();
        vecs.remove(&id);
        Ok(())
    }

    async fn search(
        &self,
        query: &[f32],
        k: usize,
        filter: Option<&FilterPredicate>,
    ) -> anyhow::Result<Vec<u64>> {
        self.maybe_stall().await;
        let vecs = self.vectors.read().unwrap();
        let metric = *self.metric.read().unwrap();

        let mut scored: Vec<(u64, f32)> = vecs
            .iter()
            .filter(|(&id, _)| {
                if let Some(pred) = filter {
                    pred.matches(id)
                } else {
                    true
                }
            })
            .map(|(&id, vec)| {
                let dist = match metric {
                    DistanceMetric::DotProduct => -dot_product(query, vec),
                    DistanceMetric::Euclidean => euclidean_distance_sq(query, vec),
                    DistanceMetric::Cosine => {
                        1.0 - (dot_product(query, vec)
                            / (dot_product(query, query).sqrt() * dot_product(vec, vec).sqrt())
                                .max(1e-10))
                    }
                };
                (id, dist)
            })
            .collect();

        scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        let results: Vec<u64> = scored.into_iter().take(k).map(|(id, _)| id).collect();
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_adapter_crud() {
        let adapter = MockVectorAdapter::default();
        let schema = CollectionSchema {
            name: "test_coll".into(),
            dimension: 4,
            metric: DistanceMetric::Euclidean,
        };
        adapter.init(&schema).await.unwrap();

        adapter.upsert(1, &[1.0, 0.0, 0.0, 0.0]).await.unwrap();
        adapter.upsert(2, &[0.0, 1.0, 0.0, 0.0]).await.unwrap();
        adapter.upsert(3, &[0.9, 0.1, 0.0, 0.0]).await.unwrap();

        let hits = adapter
            .search(&[1.0, 0.0, 0.0, 0.0], 2, None)
            .await
            .unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0], 1);
        assert_eq!(hits[1], 3);

        adapter.delete(1).await.unwrap();
        let hits_after = adapter
            .search(&[1.0, 0.0, 0.0, 0.0], 2, None)
            .await
            .unwrap();
        assert_eq!(hits_after[0], 3);

        // Test batch upsert and filter
        let batch = vec![
            (10, vec![1.0, 0.0, 0.0, 0.0]),
            (11, vec![0.0, 1.0, 0.0, 0.0]),
            (20, vec![0.8, 0.2, 0.0, 0.0]),
        ];
        adapter.upsert_batch(&batch).await.unwrap();

        let filter = FilterPredicate::Medium {
            modulo: 10,
            target_remainder: 0,
        };
        let filtered_hits = adapter
            .search(&[1.0, 0.0, 0.0, 0.0], 5, Some(&filter))
            .await
            .unwrap();
        // Only IDs where id % 10 == 0: 10, 20
        assert!(filtered_hits.contains(&10));
        assert!(filtered_hits.contains(&20));
        assert!(!filtered_hits.contains(&2));
        assert!(!filtered_hits.contains(&3));
        assert!(!filtered_hits.contains(&11));
    }
}
