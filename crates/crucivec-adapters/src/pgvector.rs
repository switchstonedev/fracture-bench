use async_trait::async_trait;
use crucivec_core::types::{CollectionSchema, FilterPredicate};
use serde::{Deserialize, Serialize};

use crate::VectorEngineAdapter;

/// pgvector database connection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PgvectorConfig {
    pub connection_string: String,
    pub table_name: String,
    pub m: usize,
    pub ef_construction: usize,
}

impl Default for PgvectorConfig {
    fn default() -> Self {
        Self {
            connection_string: "postgresql://postgres:postgres@127.0.0.1:5432/crucivec".to_string(),
            table_name: "vectors".to_string(),
            m: 16,
            ef_construction: 64,
        }
    }
}

/// pgvector client adapter.
pub struct PgvectorAdapter {
    pub config: PgvectorConfig,
}

impl PgvectorAdapter {
    pub fn new(config: PgvectorConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl VectorEngineAdapter for PgvectorAdapter {
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

