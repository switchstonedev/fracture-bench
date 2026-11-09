use async_trait::async_trait;
use crucivec_core::types::{CollectionSchema, FilterPredicate};
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
            uri: "data/lancedb".to_string(),
            table_name: "vectors".to_string(),
        }
    }
}

pub struct LanceDbAdapter {
    pub config: LanceDbConfig,
}

impl LanceDbAdapter {
    pub fn new(config: LanceDbConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl VectorEngineAdapter for LanceDbAdapter {
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

