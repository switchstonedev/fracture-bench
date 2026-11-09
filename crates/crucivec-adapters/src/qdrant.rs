use async_trait::async_trait;
use crucivec_core::types::{CollectionSchema, DistanceMetric, FilterPredicate};
use serde::{Deserialize, Serialize};

use crate::http_client::SimpleHttpClient;
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
            endpoint: "http://127.0.0.1:6333".to_string(),
            collection_name: "crucivec_bench".to_string(),
            ef_search: Some(100),
        }
    }
}

/// Production Qdrant client adapter communicating via async HTTP REST protocol.
pub struct QdrantAdapter {
    pub config: QdrantConfig,
    client: SimpleHttpClient,
}

impl QdrantAdapter {
    pub fn new(config: QdrantConfig) -> Self {
        let client = SimpleHttpClient::new(&config.endpoint)
            .unwrap_or_else(|_| SimpleHttpClient {
                host: "127.0.0.1".to_string(),
                port: 6333,
                timeout: std::time::Duration::from_secs(5),
            });
        Self { config, client }
    }
}

#[async_trait]
impl VectorEngineAdapter for QdrantAdapter {
    async fn init(&self, schema: &CollectionSchema) -> anyhow::Result<()> {
        let metric_str = match schema.metric {
            DistanceMetric::DotProduct => "Dot",
            DistanceMetric::Euclidean => "Euclid",
            DistanceMetric::Cosine => "Cosine",
        };

        let body = serde_json::json!({
            "vectors": {
                "size": schema.dimension,
                "distance": metric_str
            }
        });

        let path = format!("/collections/{}", self.config.collection_name);
        let (status, resp) = self.client.request("PUT", &path, Some(&body)).await?;
        if status == 200 || status == 409 {
            Ok(())
        } else {
            anyhow::bail!("Qdrant init collection failed with status {}: {:?}", status, resp);
        }
    }

    async fn upsert(&self, id: u64, vector: &[f32]) -> anyhow::Result<()> {
        let body = serde_json::json!({
            "points": [
                {
                    "id": id,
                    "vector": vector,
                    "payload": { "id": id }
                }
            ]
        });

        let path = format!("/collections/{}/points?wait=true", self.config.collection_name);
        let (status, resp) = self.client.request("PUT", &path, Some(&body)).await?;
        if status == 200 {
            Ok(())
        } else {
            anyhow::bail!("Qdrant upsert failed with status {}: {:?}", status, resp);
        }
    }

    async fn delete(&self, id: u64) -> anyhow::Result<()> {
        let body = serde_json::json!({
            "points": [id]
        });

        let path = format!("/collections/{}/points/delete?wait=true", self.config.collection_name);
        let (status, resp) = self.client.request("POST", &path, Some(&body)).await?;
        if status == 200 {
            Ok(())
        } else {
            anyhow::bail!("Qdrant delete failed with status {}: {:?}", status, resp);
        }
    }

    async fn search(&self, vector: &[f32], k: usize, _filter: Option<&FilterPredicate>) -> anyhow::Result<Vec<u64>> {
        let ef = self.config.ef_search.unwrap_or(100);
        let body = serde_json::json!({
            "vector": vector,
            "limit": k,
            "with_payload": false,
            "with_vector": false,
            "params": {
                "ef": ef
            }
        });

        let path = format!("/collections/{}/points/search", self.config.collection_name);
        let (status, resp) = self.client.request("POST", &path, Some(&body)).await?;
        if status != 200 {
            anyhow::bail!("Qdrant search failed with status {}: {:?}", status, resp);
        }

        let mut results = Vec::with_capacity(k);
        if let Some(hits) = resp.get("result").and_then(|r| r.as_array()) {
            for hit in hits {
                if let Some(id_val) = hit.get("id").and_then(|i| i.as_u64()) {
                    results.push(id_val);
                }
            }
        }

        Ok(results)
    }
}
