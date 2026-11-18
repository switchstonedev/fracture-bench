use async_trait::async_trait;
use fracture_core::types::{CollectionSchema, FilterPredicate};
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
            connection_string: "postgresql://postgres@127.0.0.1:5433/fracture".to_string(),
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
                database: "fracture".to_string(),
                timeout: std::time::Duration::from_secs(5),
            });
        Self { config, client }
    }

    /// Encodes a vector into pgvector's native binary wire representation (format code 1):
    /// [dim: i16_be] [unused: u16_be] [f32_be_0] [f32_be_1] ...
    pub fn encode_binary(vector: &[f32], dst: &mut Vec<u8>) -> Result<(), &'static str> {
        encode_pgvector_binary(vector, dst)
    }

    /// Formats raw IEEE-754 binary vectors as zero-text hex byte literals:
    /// `\x[dim_be][unused_be][f32_be...]::vector`
    /// Eliminates ASCII float formatting and bypasses PostgreSQL string parsing overhead.
    pub fn format_vector_binary_hex(vector: &[f32]) -> String {
        let mut bytes = Vec::with_capacity(4 + vector.len() * 4);
        let _ = encode_pgvector_binary(vector, &mut bytes);
        let mut hex = String::with_capacity(2 + bytes.len() * 2);
        hex.push_str("\\x");
        for b in bytes {
            use std::fmt::Write;
            let _ = write!(hex, "{:02x}", b);
        }
        hex
    }

    fn format_vector(vector: &[f32]) -> String {
        Self::format_vector_binary_hex(vector)
    }
}

/// Encodes a vector into pgvector's big-endian binary layout:
/// - `dim`: 16-bit signed integer (network byte order)
/// - `unused`: 16-bit reserved word (0x0000)
/// - `elements`: sequence of 32-bit IEEE-754 floats in big-endian
pub fn encode_pgvector_binary(vector: &[f32], dst: &mut Vec<u8>) -> Result<(), &'static str> {
    if vector.is_empty() {
        return Err("Vector dimensions must be non-zero");
    }
    if vector.len() > i16::MAX as usize {
        return Err("Vector dimension exceeds PostgreSQL i16 binary limit");
    }

    let dim = vector.len() as i16;
    dst.reserve(4 + vector.len() * 4);
    dst.extend_from_slice(&dim.to_be_bytes());
    dst.extend_from_slice(&0u16.to_be_bytes());

    for &coord in vector {
        if coord.is_nan() || coord.is_infinite() {
            return Err("Vector coordinate contains NaN or Infinity");
        }
        dst.extend_from_slice(&coord.to_be_bytes());
    }

    Ok(())
}

/// Decodes pgvector binary bytes back into a slice of f32 floats.
pub fn decode_pgvector_binary(src: &[u8]) -> Result<Vec<f32>, &'static str> {
    if src.len() < 4 {
        return Err("Binary payload too short for pgvector header");
    }
    let dim = i16::from_be_bytes([src[0], src[1]]) as usize;
    let expected_len = 4 + dim * 4;
    if src.len() < expected_len {
        return Err("Binary payload length smaller than declared dimensions");
    }

    let mut vector = Vec::with_capacity(dim);
    let mut offset = 4;
    for _ in 0..dim {
        let float_val = f32::from_be_bytes([
            src[offset],
            src[offset + 1],
            src[offset + 2],
            src[offset + 3],
        ]);
        vector.push(float_val);
        offset += 4;
    }
    Ok(vector)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pgvector_binary_roundtrip_bit_equivalence() {
        let original = vec![0.12345f32, -98.7654f32, 1.0f32, 0.0f32];
        let mut encoded = Vec::new();
        encode_pgvector_binary(&original, &mut encoded).unwrap();

        assert_eq!(encoded.len(), 4 + original.len() * 4);
        let decoded = decode_pgvector_binary(&encoded).unwrap();
        assert_eq!(decoded.len(), original.len());

        for (a, b) in original.iter().zip(decoded.iter()) {
            assert!((a - b).abs() < 1e-7);
        }
    }

    #[test]
    fn test_format_vector_binary_hex() {
        let v = vec![1.0f32, 2.0f32];
        let hex = PgvectorAdapter::format_vector_binary_hex(&v);
        assert!(hex.starts_with("\\x"));
        // 2 elements -> 4 bytes header + 8 bytes floats = 12 bytes = 24 hex chars + 2 prefix = 26 chars
        assert_eq!(hex.len(), 26);
    }
}
