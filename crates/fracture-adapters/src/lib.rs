pub(crate) mod http_client;
pub mod lancedb;
pub mod milvus;
pub mod mock;
pub(crate) mod pg_client;
pub mod pgvector;
pub mod qdrant;

use async_trait::async_trait;
use fracture_core::types::{CollectionSchema, FilterPredicate};

/// Standard asynchronous interface for target vector database clients.
#[async_trait]
pub trait VectorEngineAdapter: Send + Sync {
    async fn init(&self, schema: &CollectionSchema) -> anyhow::Result<()>;
    async fn upsert(&self, id: u64, vector: &[f32]) -> anyhow::Result<()>;
    async fn delete(&self, id: u64) -> anyhow::Result<()>;
    async fn search(
        &self,
        vector: &[f32],
        k: usize,
        filter: Option<&FilterPredicate>,
    ) -> anyhow::Result<Vec<u64>>;

    /// Batch upsert multiple vectors in a single round-trip.
    async fn upsert_batch(&self, points: &[(u64, Vec<f32>)]) -> anyhow::Result<()> {
        for (id, vec) in points {
            self.upsert(*id, vec).await?;
        }
        Ok(())
    }
}

pub use lancedb::{LanceDbAdapter, LanceDbConfig};
pub use milvus::{MilvusAdapter, MilvusConfig};
pub use mock::MockVectorAdapter;
pub use pgvector::{PgvectorAdapter, PgvectorConfig};
pub use qdrant::{QdrantAdapter, QdrantConfig};
