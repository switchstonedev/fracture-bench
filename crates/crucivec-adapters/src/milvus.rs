use async_trait::async_trait;
use crucivec_core::types::{CollectionSchema, FilterPredicate};
use serde::{Deserialize, Serialize};

use crate::VectorEngineAdapter;

/// Milvus connection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilvusConfig {
    pub endpoint: String,
    pub collection_name: String,
}

impl Default for MilvusConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://127.0.0.1:19530".to_string(),
            collection_name: "crucivec_milvus".to_string(),
        }
    }
}

pub struct MilvusAdapter {
    pub config: MilvusConfig,
}

impl MilvusAdapter {
    pub fn new(config: MilvusConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl VectorEngineAdapter for MilvusAdapter {
    async fn init(&self, _schema: &CollectionSchema) -> anyhow::Result<()> {
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

