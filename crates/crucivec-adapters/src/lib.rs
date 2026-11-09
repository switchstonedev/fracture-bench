pub mod mock;
pub mod qdrant;
pub mod pgvector;
pub mod milvus;
pub mod lancedb;
pub(crate) mod http_client;
pub(crate) mod pg_client;

use async_trait::async_trait;
use crucivec_core::types::{CollectionSchema, FilterPredicate};

/// Standard asynchronous interface for target vector database clients.
#[async_trait]
pub trait VectorEngineAdapter: Send + Sync {
    async fn init(&self, schema: &CollectionSchema) -> anyhow::Result<()>;
    async fn upsert(&self, id: u64, vector: &[f32]) -> anyhow::Result<()>;
    async fn delete(&self, id: u64) -> anyhow::Result<()>;
    async fn search(&self, vector: &[f32], k: usize, filter: Option<&FilterPredicate>) -> anyhow::Result<Vec<u64>>;
}

pub use mock::MockVectorAdapter;
pub use qdrant::{QdrantAdapter, QdrantConfig};
pub use pgvector::{PgvectorAdapter, PgvectorConfig};
pub use milvus::{MilvusAdapter, MilvusConfig};
pub use lancedb::{LanceDbAdapter, LanceDbConfig};
