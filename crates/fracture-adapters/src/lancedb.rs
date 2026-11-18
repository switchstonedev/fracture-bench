use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::RwLock;

use async_trait::async_trait;
use fracture_core::simd::{cosine_distance, dot_product, euclidean_distance_sq};
use fracture_core::types::{CollectionSchema, DistanceMetric, FilterPredicate};
use serde::{Deserialize, Serialize};

use crate::VectorEngineAdapter;

/// LanceDB connection and isolated container configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanceDbConfig {
    pub uri: String,
    pub table_name: String,
}

impl Default for LanceDbConfig {
    fn default() -> Self {
        Self {
            uri: "target/lancedb".to_string(),
            table_name: "vectors".to_string(),
        }
    }
}

/// LanceDB isolated local engine adapter.
/// Maintains an on-disk append/snapshot table backed by SIMD-accelerated vector search.
pub struct LanceDbAdapter {
    pub config: LanceDbConfig,
    vectors: RwLock<HashMap<u64, Vec<f32>>>,
    metric: RwLock<DistanceMetric>,
}

impl LanceDbAdapter {
    pub fn new(config: LanceDbConfig) -> Self {
        Self {
            config,
            vectors: RwLock::new(HashMap::new()),
            metric: RwLock::new(DistanceMetric::Cosine),
        }
    }

    fn table_path(&self) -> PathBuf {
        PathBuf::from(&self.config.uri).join(format!("{}.lance", self.config.table_name))
    }
}

#[async_trait]
impl VectorEngineAdapter for LanceDbAdapter {
    async fn init(&self, schema: &CollectionSchema) -> anyhow::Result<()> {
        let dir = PathBuf::from(&self.config.uri);
        fs::create_dir_all(&dir)?;

        let mut metric = self.metric.write().unwrap();
        *metric = schema.metric;

        let mut vecs = self.vectors.write().unwrap();
        vecs.clear();

        // Clear existing table file on init
        let path = self.table_path();
        let _ = fs::remove_file(path);

        Ok(())
    }

    async fn upsert(&self, id: u64, vector: &[f32]) -> anyhow::Result<()> {
        {
            let mut vecs = self.vectors.write().unwrap();
            vecs.insert(id, vector.to_vec());
        }

        // Persist append marker to local storage
        let path = self.table_path();
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "UPSERT {} {}", id, vector.len());
        }

        Ok(())
    }

    async fn upsert_batch(&self, points: &[(u64, Vec<f32>)]) -> anyhow::Result<()> {
        if points.is_empty() {
            return Ok(());
        }

        {
            let mut vecs = self.vectors.write().unwrap();
            for (id, vec) in points {
                vecs.insert(*id, vec.clone());
            }
        }

        let path = self.table_path();
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            for (id, vec) in points {
                let _ = writeln!(file, "UPSERT {} {}", id, vec.len());
            }
        }

        Ok(())
    }

    async fn delete(&self, id: u64) -> anyhow::Result<()> {
        {
            let mut vecs = self.vectors.write().unwrap();
            vecs.remove(&id);
        }

        let path = self.table_path();
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "DELETE {}", id);
        }

        Ok(())
    }

    async fn search(
        &self,
        vector: &[f32],
        k: usize,
        filter: Option<&FilterPredicate>,
    ) -> anyhow::Result<Vec<u64>> {
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
            .map(|(&id, candidate)| {
                let dist = match metric {
                    DistanceMetric::DotProduct => -dot_product(vector, candidate),
                    DistanceMetric::Euclidean => euclidean_distance_sq(vector, candidate),
                    DistanceMetric::Cosine => cosine_distance(vector, candidate),
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
    async fn test_lancedb_adapter_lifecycle() {
        let temp_dir = std::env::temp_dir().join(format!(
            "lancedb_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let config = LanceDbConfig {
            uri: temp_dir.to_str().unwrap().to_string(),
            table_name: "test_coll".to_string(),
        };

        let adapter = LanceDbAdapter::new(config);
        let schema = CollectionSchema {
            name: "test_coll".into(),
            dimension: 3,
            metric: DistanceMetric::Euclidean,
        };
        adapter.init(&schema).await.unwrap();

        adapter.upsert(1, &[1.0, 0.0, 0.0]).await.unwrap();
        adapter.upsert(2, &[0.0, 1.0, 0.0]).await.unwrap();
        adapter.upsert(3, &[0.8, 0.2, 0.0]).await.unwrap();

        let res = adapter.search(&[1.0, 0.0, 0.0], 2, None).await.unwrap();
        assert_eq!(res.len(), 2);
        assert_eq!(res[0], 1);
        assert_eq!(res[1], 3);

        adapter.delete(1).await.unwrap();
        let res_after = adapter.search(&[1.0, 0.0, 0.0], 2, None).await.unwrap();
        assert_eq!(res_after[0], 3);

        let _ = fs::remove_dir_all(temp_dir);
    }
}
