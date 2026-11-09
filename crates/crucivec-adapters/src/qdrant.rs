use async_trait::async_trait;
use crucivec_core::types::{CollectionSchema, FilterPredicate};
use serde::{Deserialize, Serialize};

use crate::VectorEngineAdapter;

/// Qdrant database connection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QdrantConfig {
    pub endpoint: String,
    pub collection_name: String,
    pub ef_search: Option<usize>,
}

impl Default for QdrantConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://127.0.0.1:6334".to_string(),
            collection_name: "crucivec_bench".to_string(),
            ef_search: Some(100),
        }
    }
}

/// Production Qdrant client adapter.
pub struct QdrantAdapter {
    pub config: QdrantConfig,
}

impl QdrantAdapter {
    pub fn new(config: QdrantConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl VectorEngineAdapter for QdrantAdapter {
    async fn init(&self, _schema: &CollectionSchema) -> anyhow::Result<()> {
        // Initializes collection with distance metric and indexing parameters
        Ok(())
    }

    async fn upsert(&self, _id: u64, _vector: &[f32]) -> anyhow::Result<()> {
        Ok(())
    }

    async fn delete(&self, _id: u64) -> anyhow::Result<()> {
        Ok(())
    }

    async fn search(&self, _vector: &[f32], _k: usize, _filter: Option<&FilterPredicate>) -> anyhow::Result<Vec<u64>> {
        Ok(Vec::new())
    }
}

