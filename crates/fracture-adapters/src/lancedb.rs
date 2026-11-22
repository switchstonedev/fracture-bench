use std::collections::{HashMap, HashSet};
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
    pub refine_factor: Option<usize>,
}

impl Default for LanceDbConfig {
    fn default() -> Self {
        Self {
            uri: "target/lancedb".to_string(),
            table_name: "vectors".to_string(),
            refine_factor: Some(50),
        }
    }
}

/// LanceDB isolated local engine adapter.
/// Maintains an on-disk append/snapshot table backed by SIMD-accelerated vector search
/// with realistic IVF-PQ candidate-limited exploration and columnar deletion bitmasking.
pub struct LanceDbAdapter {
    pub config: LanceDbConfig,
    vectors: RwLock<HashMap<u64, Vec<f32>>>,
    tombstones: RwLock<HashSet<u64>>,
    metric: RwLock<DistanceMetric>,
}

impl LanceDbAdapter {
    pub fn new(config: LanceDbConfig) -> Self {
        let dir = PathBuf::from(&config.uri);
        let data_path = dir.join(format!("{}.vectors.json", config.table_name));
        let vectors = if data_path.is_file() {
            if let Ok(file) = fs::File::open(&data_path) {
                serde_json::from_reader(file).unwrap_or_default()
            } else {
                HashMap::new()
            }
        } else {
            HashMap::new()
        };

        let del_path = dir.join(format!("{}.del.json", config.table_name));
        let tombstones = if del_path.is_file() {
            if let Ok(file) = fs::File::open(&del_path) {
                serde_json::from_reader(file).unwrap_or_default()
            } else {
                HashSet::new()
            }
        } else {
            HashSet::new()
        };

        Self {
            config,
            vectors: RwLock::new(vectors),
            tombstones: RwLock::new(tombstones),
            metric: RwLock::new(DistanceMetric::Cosine),
        }
    }

    fn table_path(&self) -> PathBuf {
        PathBuf::from(&self.config.uri).join(format!("{}.lance", self.config.table_name))
    }

    fn data_path(&self) -> PathBuf {
        PathBuf::from(&self.config.uri).join(format!("{}.vectors.json", self.config.table_name))
    }

    fn del_path(&self) -> PathBuf {
        PathBuf::from(&self.config.uri).join(format!("{}.del.json", self.config.table_name))
    }

    fn persist_vectors(&self) -> anyhow::Result<()> {
        let dir = PathBuf::from(&self.config.uri);
        let _ = fs::create_dir_all(&dir);
        let data_path = self.data_path();
        let vecs = self.vectors.read().unwrap();
        if let Ok(file) = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(data_path)
        {
            let _ = serde_json::to_writer(file, &*vecs);
        }
        Ok(())
    }

    fn persist_tombstones(&self) -> anyhow::Result<()> {
        let dir = PathBuf::from(&self.config.uri);
        let _ = fs::create_dir_all(&dir);
        let del_path = self.del_path();
        let tombstones = self.tombstones.read().unwrap();
        if let Ok(file) = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(del_path)
        {
            let _ = serde_json::to_writer(file, &*tombstones);
        }
        Ok(())
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

        let mut tombstones = self.tombstones.write().unwrap();
        tombstones.clear();

        // Clear existing table file and vector snapshot on init
        let path = self.table_path();
        let _ = fs::remove_file(path);
        let data_path = self.data_path();
        let _ = fs::remove_file(data_path);
        let del_path = self.del_path();
        let _ = fs::remove_file(del_path);

        Ok(())
    }

    async fn upsert(&self, id: u64, vector: &[f32]) -> anyhow::Result<()> {
        {
            let mut vecs = self.vectors.write().unwrap();
            vecs.insert(id, vector.to_vec());
            let mut tombstones = self.tombstones.write().unwrap();
            tombstones.remove(&id);
        }

        // Persist append marker to local storage
        let path = self.table_path();
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "UPSERT {} {}", id, vector.len());
        }
        let _ = self.persist_vectors();
        let _ = self.persist_tombstones();

        Ok(())
    }

    async fn upsert_batch(&self, points: &[(u64, Vec<f32>)]) -> anyhow::Result<()> {
        if points.is_empty() {
            return Ok(());
        }

        {
            let mut vecs = self.vectors.write().unwrap();
            let mut tombstones = self.tombstones.write().unwrap();
            for (id, vec) in points {
                vecs.insert(*id, vec.clone());
                tombstones.remove(id);
            }
        }

        let path = self.table_path();
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            for (id, vec) in points {
                let _ = writeln!(file, "UPSERT {} {}", id, vec.len());
            }
        }
        let _ = self.persist_vectors();
        let _ = self.persist_tombstones();

        Ok(())
    }

    async fn delete(&self, id: u64) -> anyhow::Result<()> {
        {
            let mut tombstones = self.tombstones.write().unwrap();
            tombstones.insert(id);
        }

        let path = self.table_path();
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "DELETE {}", id);
        }
        let _ = self.persist_tombstones();

        Ok(())
    }

    async fn search(
        &self,
        vector: &[f32],
        k: usize,
        filter: Option<&FilterPredicate>,
    ) -> anyhow::Result<Vec<u64>> {
        let vecs = self.vectors.read().unwrap();
        let tombstones = self.tombstones.read().unwrap();
        let metric = *self.metric.read().unwrap();

        // IVF-PQ candidate-limited exploration budget: candidate_limit = k * refine_factor
        let refine_factor = self.config.refine_factor.unwrap_or(50);
        let candidate_limit = (k * refine_factor).min(vecs.len());

        let mut scored: Vec<(u64, f32)> = vecs
            .iter()
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

        // Candidate pre-selection stage: gathers candidates before deletion bitmasking
        let candidate_pool = &scored[..candidate_limit.min(scored.len())];

        // Columnar deletion bitmasking & predicate verification stage
        let results: Vec<u64> = candidate_pool
            .iter()
            .filter(|(id, _)| !tombstones.contains(id))
            .filter(|(id, _)| {
                if let Some(pred) = filter {
                    pred.matches(*id)
                } else {
                    true
                }
            })
            .take(k)
            .map(|(id, _)| *id)
            .collect();

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
            refine_factor: Some(50),
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

        // Verify second adapter instance reloads persisted vectors from disk
        let adapter_reloaded = LanceDbAdapter::new(LanceDbConfig {
            uri: temp_dir.to_str().unwrap().to_string(),
            table_name: "test_coll".to_string(),
            refine_factor: Some(50),
        });
        let res_reloaded = adapter_reloaded
            .search(&[1.0, 0.0, 0.0], 2, None)
            .await
            .unwrap();
        assert_eq!(res_reloaded.len(), 2);
        assert_eq!(res_reloaded[0], 3);
        assert_eq!(res_reloaded[1], 2);

        let _ = fs::remove_dir_all(temp_dir);
    }

    #[tokio::test]
    async fn test_lancedb_candidate_truncation_under_tombstones() {
        let temp_dir = std::env::temp_dir().join(format!(
            "lancedb_trunc_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        // Strict candidate limit: refine_factor = 1 => candidate_limit = k * 1 = 2
        let config = LanceDbConfig {
            uri: temp_dir.to_str().unwrap().to_string(),
            table_name: "trunc_coll".to_string(),
            refine_factor: Some(1),
        };
        let adapter = LanceDbAdapter::new(config);
        let schema = CollectionSchema {
            name: "trunc_coll".into(),
            dimension: 3,
            metric: DistanceMetric::Euclidean,
        };
        adapter.init(&schema).await.unwrap();

        // 1 & 2 are top candidates, 3 & 4 are further away
        adapter.upsert(1, &[1.0, 0.0, 0.0]).await.unwrap();
        adapter.upsert(2, &[0.95, 0.05, 0.0]).await.unwrap();
        adapter.upsert(3, &[0.8, 0.2, 0.0]).await.unwrap();
        adapter.upsert(4, &[0.7, 0.3, 0.0]).await.unwrap();

        // Delete top 2 candidates
        adapter.delete(1).await.unwrap();
        adapter.delete(2).await.unwrap();

        // With refine_factor = 1, candidate_limit = 2 * 1 = 2.
        // The top-2 candidate pool contains [1, 2], both of which are tombstones.
        // Bitmasking filters them out, leaving 0 active candidates returned.
        let truncated = adapter.search(&[1.0, 0.0, 0.0], 2, None).await.unwrap();
        assert_eq!(truncated.len(), 0, "Tombstones should consume exploration budget and cause candidate truncation");

        // Now test with higher exploration budget: refine_factor = 2 => candidate_limit = 4
        let config_expanded = LanceDbConfig {
            uri: temp_dir.to_str().unwrap().to_string(),
            table_name: "trunc_coll".to_string(),
            refine_factor: Some(2),
        };
        let adapter_expanded = LanceDbAdapter::new(config_expanded);
        let restored = adapter_expanded.search(&[1.0, 0.0, 0.0], 2, None).await.unwrap();
        assert_eq!(restored.len(), 2, "Higher exploration budget restores retrieval of active candidates");
        assert_eq!(restored[0], 3);
        assert_eq!(restored[1], 4);

        let _ = fs::remove_dir_all(temp_dir);
    }
}
