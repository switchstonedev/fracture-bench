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
        let client = SimpleHttpClient::new(&config.endpoint).unwrap_or_else(|_| SimpleHttpClient {
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
        if status != 200 && status != 409 {
            anyhow::bail!(
                "Qdrant init collection failed with status {}: {:?}",
                status,
                resp
            );
        }

        // Create payload indices for fast stratified modulo filtering
        let idx_path = format!("/collections/{}/index", self.config.collection_name);
        let idx_rem10 = serde_json::json!({
            "field_name": "rem10",
            "field_schema": "integer"
        });
        let _ = self
            .client
            .request("PUT", &idx_path, Some(&idx_rem10))
            .await;

        let idx_rem1000 = serde_json::json!({
            "field_name": "rem1000",
            "field_schema": "integer"
        });
        let _ = self
            .client
            .request("PUT", &idx_path, Some(&idx_rem1000))
            .await;

        Ok(())
    }

    async fn upsert(&self, id: u64, vector: &[f32]) -> anyhow::Result<()> {
        let body = serde_json::json!({
            "points": [
                {
                    "id": id,
                    "vector": vector,
                    "payload": {
                        "id": id,
                        "rem10": id % 10,
                        "rem1000": id % 1000
                    }
                }
            ]
        });

        let path = format!(
            "/collections/{}/points?wait=true",
            self.config.collection_name
        );
        let (status, resp) = self.client.request("PUT", &path, Some(&body)).await?;
        if status == 200 {
            Ok(())
        } else {
            anyhow::bail!("Qdrant upsert failed with status {}: {:?}", status, resp);
        }
    }

    async fn upsert_batch(&self, points: &[(u64, Vec<f32>)]) -> anyhow::Result<()> {
        if points.is_empty() {
            return Ok(());
        }

        let points_json: Vec<serde_json::Value> = points
            .iter()
            .map(|(id, vec)| {
                serde_json::json!({
                    "id": id,
                    "vector": vec,
                    "payload": {
                        "id": id,
                        "rem10": id % 10,
                        "rem1000": id % 1000
                    }
                })
            })
            .collect();

        let body = serde_json::json!({ "points": points_json });
        let path = format!(
            "/collections/{}/points?wait=true",
            self.config.collection_name
        );
        let (status, resp) = self.client.request("PUT", &path, Some(&body)).await?;
        if status == 200 {
            Ok(())
        } else {
            anyhow::bail!(
                "Qdrant batch upsert failed with status {}: {:?}",
                status,
                resp
            );
        }
    }

    async fn delete(&self, id: u64) -> anyhow::Result<()> {
        let body = serde_json::json!({
            "points": [id]
        });

        let path = format!(
            "/collections/{}/points/delete?wait=true",
            self.config.collection_name
        );
        let (status, resp) = self.client.request("POST", &path, Some(&body)).await?;
        if status == 200 {
            Ok(())
        } else {
            anyhow::bail!("Qdrant delete failed with status {}: {:?}", status, resp);
        }
    }

    async fn search(
        &self,
        vector: &[f32],
        k: usize,
        filter: Option<&FilterPredicate>,
    ) -> anyhow::Result<Vec<u64>> {
        let ef = self.config.ef_search.unwrap_or(100);
        let mut body = serde_json::json!({
            "vector": vector,
            "limit": k,
            "with_payload": false,
            "with_vector": false,
            "params": {
                "ef": ef
            }
        });

        if let Some(f) = filter {
            let filter_json = match f {
                FilterPredicate::High {
                    allowed_remainders, ..
                } => serde_json::json!({
                    "must": [
                        {
                            "key": "rem10",
                            "match": { "any": allowed_remainders }
                        }
                    ]
                }),
                FilterPredicate::Medium {
                    target_remainder, ..
                } => serde_json::json!({
                    "must": [
                        {
                            "key": "rem10",
                            "match": { "value": target_remainder }
                        }
                    ]
                }),
                FilterPredicate::Needle {
                    target_remainder, ..
                } => serde_json::json!({
                    "must": [
                        {
                            "key": "rem1000",
                            "match": { "value": target_remainder }
                        }
                    ]
                }),
            };
            body["filter"] = filter_json;
        }

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
