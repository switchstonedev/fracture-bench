use async_trait::async_trait;
use crucivec_core::types::{CollectionSchema, DistanceMetric, FilterPredicate};
use serde::{Deserialize, Serialize};

use crate::http_client::SimpleHttpClient;
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
    client: SimpleHttpClient,
}

impl MilvusAdapter {
    pub fn new(config: MilvusConfig) -> Self {
        let client = SimpleHttpClient::new(&config.endpoint)
            .unwrap_or_else(|_| SimpleHttpClient {
                host: "127.0.0.1".to_string(),
                port: 19530,
                timeout: std::time::Duration::from_secs(5),
            });
        Self { config, client }
    }
}

#[async_trait]
impl VectorEngineAdapter for MilvusAdapter {
    async fn init(&self, schema: &CollectionSchema) -> anyhow::Result<()> {
        let metric = match schema.metric {
            DistanceMetric::DotProduct => "IP",
            DistanceMetric::Euclidean => "L2",
            DistanceMetric::Cosine => "COSINE",
        };

        let body = serde_json::json!({
            "collectionName": self.config.collection_name,
            "dimension": schema.dimension,
            "metricType": metric
        });

        let (status, resp) = self.client.request("POST", "/v2/vectordb/collections/create", Some(&body)).await?;
        if status == 200 || status == 400 {
            Ok(())
        } else {
            anyhow::bail!("Milvus init failed with status {}: {:?}", status, resp);
        }
    }

    async fn upsert(&self, id: u64, vector: &[f32]) -> anyhow::Result<()> {
        let body = serde_json::json!({
            "collectionName": self.config.collection_name,
            "data": [
                {
                    "id": id,
                    "vector": vector
                }
            ]
        });

        let (status, resp) = self.client.request("POST", "/v2/vectordb/entities/upsert", Some(&body)).await?;
        if status == 200 {
            Ok(())
        } else {
            anyhow::bail!("Milvus upsert failed with status {}: {:?}", status, resp);
        }
    }

    async fn delete(&self, id: u64) -> anyhow::Result<()> {
        let body = serde_json::json!({
            "collectionName": self.config.collection_name,
            "filter": format!("id in [{}]", id)
        });

        let (status, resp) = self.client.request("POST", "/v2/vectordb/entities/delete", Some(&body)).await?;
        if status == 200 {
            Ok(())
        } else {
            anyhow::bail!("Milvus delete failed with status {}: {:?}", status, resp);
        }
    }

    async fn search(&self, vector: &[f32], k: usize, _filter: Option<&FilterPredicate>) -> anyhow::Result<Vec<u64>> {
        let body = serde_json::json!({
            "collectionName": self.config.collection_name,
            "data": [vector],
            "limit": k
        });

        let (status, resp) = self.client.request("POST", "/v2/vectordb/entities/search", Some(&body)).await?;
        if status != 200 {
            anyhow::bail!("Milvus search failed with status {}: {:?}", status, resp);
        }

        let mut results = Vec::with_capacity(k);
        if let Some(data) = resp.get("data").and_then(|d| d.as_array()) {
            for item in data {
                if let Some(id_val) = item.get("id").and_then(|i| i.as_u64()) {
                    results.push(id_val);
                }
            }
        }

        Ok(results)
    }
}
