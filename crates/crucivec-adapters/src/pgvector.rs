use async_trait::async_trait;
use crucivec_core::types::{CollectionSchema, FilterPredicate};
use serde::{Deserialize, Serialize};

use crate::pg_client::SimplePgClient;
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
            connection_string: "postgresql://postgres@127.0.0.1:5433/crucivec".to_string(),
            table_name: "vectors".to_string(),
            m: 16,
            ef_construction: 64,
        }
    }
}

/// Production pgvector client adapter communicating via PostgreSQL wire protocol.
pub struct PgvectorAdapter {
    pub config: PgvectorConfig,
    client: SimplePgClient,
}

impl PgvectorAdapter {
    pub fn new(config: PgvectorConfig) -> Self {
        let client =
            SimplePgClient::parse(&config.connection_string).unwrap_or_else(|_| SimplePgClient {
                host: "127.0.0.1".to_string(),
                port: 5433,
                user: "postgres".to_string(),
                password: None,
                database: "crucivec".to_string(),
                timeout: std::time::Duration::from_secs(5),
            });
        Self { config, client }
    }

    fn format_vector(vector: &[f32]) -> String {
        let mut s = String::with_capacity(vector.len() * 12 + 2);
        s.push('[');
        for (i, val) in vector.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            s.push_str(&format!("{:.6}", val));
        }
        s.push(']');
        s
    }
}

#[async_trait]
impl VectorEngineAdapter for PgvectorAdapter {
    async fn init(&self, schema: &CollectionSchema) -> anyhow::Result<()> {
        let ext_sql = "CREATE EXTENSION IF NOT EXISTS vector;";
        let table_sql = format!(
            "CREATE TABLE IF NOT EXISTS {} (id BIGINT PRIMARY KEY, embedding vector({}));",
            self.config.table_name, schema.dimension
        );
        let index_sql = format!(
            "CREATE INDEX IF NOT EXISTS {}_hnsw ON {} USING hnsw (embedding vector_l2_ops) WITH (m = {}, ef_construction = {});",
            self.config.table_name, self.config.table_name, self.config.m, self.config.ef_construction
        );

        let init_sql = format!("{} {} {}", ext_sql, table_sql, index_sql);
        self.client.execute_query(&init_sql).await?;
        Ok(())
    }

    async fn upsert(&self, id: u64, vector: &[f32]) -> anyhow::Result<()> {
        let vec_literal = Self::format_vector(vector);
        let sql = format!(
            "INSERT INTO {} (id, embedding) VALUES ({}, '{}') ON CONFLICT (id) DO UPDATE SET embedding = EXCLUDED.embedding;",
            self.config.table_name, id, vec_literal
        );
        self.client.execute_query(&sql).await?;
        Ok(())
    }

    async fn upsert_batch(&self, points: &[(u64, Vec<f32>)]) -> anyhow::Result<()> {
        if points.is_empty() {
            return Ok(());
        }

        let mut sql = format!(
            "INSERT INTO {} (id, embedding) VALUES ",
            self.config.table_name
        );
        for (i, (id, vec)) in points.iter().enumerate() {
            if i > 0 {
                sql.push_str(", ");
            }
            sql.push_str(&format!("({}, '{}')", id, Self::format_vector(vec)));
        }
        sql.push_str(" ON CONFLICT (id) DO UPDATE SET embedding = EXCLUDED.embedding;");

        self.client.execute_query(&sql).await?;
        Ok(())
    }

    async fn delete(&self, id: u64) -> anyhow::Result<()> {
        let sql = format!("DELETE FROM {} WHERE id = {};", self.config.table_name, id);
        self.client.execute_query(&sql).await?;
        Ok(())
    }

    async fn search(
        &self,
        vector: &[f32],
        k: usize,
        filter: Option<&FilterPredicate>,
    ) -> anyhow::Result<Vec<u64>> {
        let vec_literal = Self::format_vector(vector);
        let where_clause = match filter {
            Some(FilterPredicate::High {
                allowed_remainders, ..
            }) => {
                let rems = allowed_remainders
                    .iter()
                    .map(|r| r.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                format!("WHERE (id % 10) IN ({})", rems)
            }
            Some(FilterPredicate::Medium {
                target_remainder, ..
            }) => {
                format!("WHERE (id % 10) = {}", target_remainder)
            }
            Some(FilterPredicate::Needle {
                target_remainder, ..
            }) => {
                format!("WHERE (id % 1000) = {}", target_remainder)
            }
            None => String::new(),
        };

        let sql = if where_clause.is_empty() {
            format!(
                "SELECT id FROM {} ORDER BY embedding <-> '{}' LIMIT {};",
                self.config.table_name, vec_literal, k
            )
        } else {
            format!(
                "SELECT id FROM {} {} ORDER BY embedding <-> '{}' LIMIT {};",
                self.config.table_name, where_clause, vec_literal, k
            )
        };
        let ids = self.client.execute_query(&sql).await?;
        Ok(ids)
    }
}
