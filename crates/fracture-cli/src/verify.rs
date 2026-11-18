use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

use fracture_adapters::VectorEngineAdapter;
use fracture_core::arena::VectorArena;
use fracture_core::dataset::FvecsReader;
use fracture_core::simd::find_exact_top_k;
use fracture_core::types::{classify_mutation, DistanceMetric, MutationLinearity};
use fracture_core::wal::{WalOpType, WalReader};
use fracture_pacer::workload::{WorkloadConfig, WorkloadGenerator, WorkloadType};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallVerificationReport {
    pub cutoff_lsn: u64,
    pub active_vectors_count: usize,
    pub tombstone_count: usize,
    pub evaluated_queries: usize,
    pub mean_recall_at_k: f64,
    pub k: usize,
}

/// Audit report evaluating linearizable recall with dual-bound interval classification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearizableRecallReport {
    pub evaluated_queries: usize,
    pub mean_linearizable_recall: f64,
    pub committed_hits: usize,
    pub inflight_neutral_hits: usize,
    pub active_vectors_count: usize,
    pub tombstone_count: usize,
    pub k: usize,
}

/// Query observation capturing dual timestamps and returned candidates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryObservation {
    pub query_sent_ns: u64,
    pub query_recv_ns: u64,
    pub vector: Vec<f32>,
    pub returned_ids: Vec<u64>,
}

/// Replay event captured from WAL.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct WalMutationEvent {
    pub lsn: u64,
    pub sent_ns: u64,
    pub ack_ns: u64,
    pub id: u64,
    pub op: WalOpType,
    pub vector: Vec<f32>,
}

/// Loads query vectors from an `.fvecs`, JSON file (`[[f32, ...], ...]`), or binary file (`[u32 count, u32 dim, f32...]`).
pub fn load_queries_from_file(path: impl AsRef<Path>) -> anyhow::Result<Vec<Vec<f32>>> {
    let path = path.as_ref();

    // 1. Try .fvecs reader
    if let Ok(queries) = FvecsReader::read_all(path) {
        if !queries.is_empty() {
            return Ok(queries);
        }
    }

    let mut file = File::open(path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;

    // 2. Try JSON parsing
    if let Ok(queries) = serde_json::from_slice::<Vec<Vec<f32>>>(&buffer) {
        if !queries.is_empty() {
            return Ok(queries);
        }
    }

    // 2. Try raw binary format: [u32 count, u32 dimension, f32 payload...]
    if buffer.len() >= 8 {
        let count = u32::from_le_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]) as usize;
        let dim = u32::from_le_bytes([buffer[4], buffer[5], buffer[6], buffer[7]]) as usize;
        let expected_payload = count * dim * 4;

        if buffer.len() >= 8 + expected_payload && count > 0 && dim > 0 {
            let mut queries = Vec::with_capacity(count);
            let mut offset = 8;
            for _ in 0..count {
                let mut q = Vec::with_capacity(dim);
                for _ in 0..dim {
                    let val = f32::from_le_bytes([
                        buffer[offset],
                        buffer[offset + 1],
                        buffer[offset + 2],
                        buffer[offset + 3],
                    ]);
                    q.push(val);
                    offset += 4;
                }
                queries.push(q);
            }
            return Ok(queries);
        }
    }

    anyhow::bail!("Unrecognized query file format for {:?}: must be JSON array or binary [count, dim, floats]", path)
}

/// Generates a calibrated set of synthetic normalized queries when no file is supplied.
pub fn generate_synthetic_queries(dimension: usize, count: usize, seed: u64) -> Vec<Vec<f32>> {
    let config = WorkloadConfig::new(WorkloadType::SearchOnly, dimension, 10);
    let mut gen = WorkloadGenerator::new(&config, seed);
    let mut queries = Vec::with_capacity(count);
    for _ in 0..count {
        queries.push(gen.generate_vector());
    }
    queries
}

/// Evaluates search Recall@K against point-in-time exact ground truth reconstructed from WAL.
pub fn verify_wal_recall(
    wal_path: impl AsRef<Path>,
    cutoff_lsn: u64,
    queries: &[Vec<f32>],
    target_results: &[Vec<u64>],
    k: usize,
    metric: DistanceMetric,
) -> anyhow::Result<RecallVerificationReport> {
    anyhow::ensure!(!queries.is_empty(), "Query list cannot be empty");
    anyhow::ensure!(
        queries.len() == target_results.len(),
        "Queries and target results counts must match"
    );

    let mut reader = WalReader::open(wal_path)?;
    let snapshot = reader.reconstruct_state(cutoff_lsn)?;

    let candidates: Vec<(u64, Vec<f32>)> = snapshot.active_vectors.into_iter().collect();

    let mut total_recall = 0.0;

    for (query, target_top_k) in queries.iter().zip(target_results.iter()) {
        let exact_top = find_exact_top_k(query, &candidates, k, metric);
        let exact_ids: HashSet<u64> = exact_top.into_iter().map(|(id, _)| id).collect();

        let returned_ids: HashSet<u64> = target_top_k.iter().take(k).copied().collect();
        let hits = exact_ids.intersection(&returned_ids).count();

        let recall = if k > 0 {
            (hits as f64) / (k as f64)
        } else {
            0.0
        };
        total_recall += recall;
    }

    let mean_recall = total_recall / (queries.len() as f64);

    Ok(RecallVerificationReport {
        cutoff_lsn,
        active_vectors_count: candidates.len(),
        tombstone_count: snapshot.tombstoned_ids.len(),
        evaluated_queries: queries.len(),
        mean_recall_at_k: mean_recall,
        k,
    })
}

/// Evaluates strict linearizable recall using dual-bound interval windowing and neutral in-flight scoring.
pub fn verify_linearizable_wal_recall(
    wal_mutations: &[WalMutationEvent],
    observations: &[QueryObservation],
    dimension: usize,
    k: usize,
    metric: DistanceMetric,
) -> anyhow::Result<LinearizableRecallReport> {
    anyhow::ensure!(!observations.is_empty(), "Observations cannot be empty");
    anyhow::ensure!(k > 0, "Top-K must be strictly positive");

    let mut arena = VectorArena::new(dimension, wal_mutations.len().max(128));
    let mut total_linearizable_recall = 0.0;
    let mut total_committed_hits = 0;
    let mut total_inflight_hits = 0;

    for obs in observations {
        let mut inflight_set = HashSet::new();

        for m in wal_mutations {
            let linearity = classify_mutation(
                m.sent_ns,
                m.ack_ns,
                obs.query_sent_ns,
                obs.query_recv_ns,
            );

            match linearity {
                MutationLinearity::Committed => match m.op {
                    WalOpType::Insert | WalOpType::Upsert => {
                        let _ = arena.upsert(m.id, &m.vector);
                    }
                    WalOpType::Delete => {
                        let _ = arena.delete(m.id);
                    }
                    _ => {}
                },
                MutationLinearity::InFlight => {
                    inflight_set.insert(m.id);
                }
                MutationLinearity::Uncommitted => {}
            }
        }

        // Exact ground truth over visible committed arena
        let exact_top = arena.find_top_k(&obs.vector, k, metric);
        let exact_ids: HashSet<u64> = exact_top.into_iter().map(|(id, _)| id).collect();

        let returned_ids: HashSet<u64> = obs.returned_ids.iter().take(k).copied().collect();

        let mut hits = 0;
        for &id in &returned_ids {
            if exact_ids.contains(&id) {
                hits += 1;
                total_committed_hits += 1;
            } else if inflight_set.contains(&id) {
                // Neutral scoring rule: returned in-flight candidate is valid hit
                hits += 1;
                total_inflight_hits += 1;
            }
        }

        let recall = (hits as f64) / (k as f64);
        total_linearizable_recall += recall;
    }

    let mean_recall = total_linearizable_recall / (observations.len() as f64);

    Ok(LinearizableRecallReport {
        evaluated_queries: observations.len(),
        mean_linearizable_recall: mean_recall,
        committed_hits: total_committed_hits,
        inflight_neutral_hits: total_inflight_hits,
        active_vectors_count: arena.active_count(),
        tombstone_count: wal_mutations.len().saturating_sub(arena.active_count()),
        k,
    })
}

/// Executes query search against target adapter and evaluates Recall@K against reconstructed WAL.
pub async fn verify_with_target_adapter(
    wal_path: impl AsRef<Path>,
    cutoff_lsn: u64,
    queries: &[Vec<f32>],
    adapter: Arc<dyn VectorEngineAdapter>,
    k: usize,
    metric: DistanceMetric,
) -> anyhow::Result<RecallVerificationReport> {
    let mut target_results = Vec::with_capacity(queries.len());
    for q in queries {
        let hits = adapter.search(q, k, None).await.unwrap_or_default();
        target_results.push(hits);
    }
    verify_wal_recall(wal_path, cutoff_lsn, queries, &target_results, k, metric)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fracture_core::wal::{WalOpType, WalWriter};

    #[test]
    fn test_verify_wal_recall() {
        let temp_dir = std::env::temp_dir();
        let wal_path = temp_dir.join(format!(
            "test_verify_wal_{}.bin",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        {
            let mut writer = WalWriter::open(&wal_path).unwrap();
            writer
                .append(1, 1000, 10, WalOpType::Insert, &[1.0, 0.0, 0.0, 0.0])
                .unwrap();
            writer
                .append(2, 2000, 20, WalOpType::Insert, &[0.0, 1.0, 0.0, 0.0])
                .unwrap();
            writer
                .append(3, 3000, 30, WalOpType::Insert, &[0.9, 0.1, 0.0, 0.0])
                .unwrap();
            writer.flush().unwrap();
        }

        let queries = vec![vec![1.0, 0.0, 0.0, 0.0]];
        let target_results = vec![vec![10, 30]];

        let report = verify_wal_recall(
            &wal_path,
            3,
            &queries,
            &target_results,
            2,
            DistanceMetric::Euclidean,
        )
        .unwrap();

        assert_eq!(report.active_vectors_count, 3);
        assert_eq!(report.mean_recall_at_k, 1.0);

        let _ = std::fs::remove_file(wal_path);
    }

    #[test]
    fn test_verify_linearizable_wal_recall_neutral_scoring() {
        let mutations = vec![
            // Committed: acked at 150 < query sent at 200
            WalMutationEvent {
                lsn: 1,
                sent_ns: 50,
                ack_ns: 150,
                id: 10,
                op: WalOpType::Insert,
                vector: vec![1.0, 0.0, 0.0, 0.0],
            },
            // InFlight: sent at 180 <= query recv at 250, acked at 260 >= query sent at 200
            WalMutationEvent {
                lsn: 2,
                sent_ns: 180,
                ack_ns: 260,
                id: 20,
                op: WalOpType::Insert,
                vector: vec![0.99, 0.01, 0.0, 0.0],
            },
            // Committed: acked at 190 < query sent at 200
            WalMutationEvent {
                lsn: 3,
                sent_ns: 100,
                ack_ns: 190,
                id: 30,
                op: WalOpType::Insert,
                vector: vec![0.0, 1.0, 0.0, 0.0],
            },
        ];

        // Engine returned candidate 10 (committed) and candidate 20 (in-flight)
        let observations = vec![QueryObservation {
            query_sent_ns: 200,
            query_recv_ns: 250,
            vector: vec![1.0, 0.0, 0.0, 0.0],
            returned_ids: vec![10, 20],
        }];

        let report = verify_linearizable_wal_recall(
            &mutations,
            &observations,
            4,
            2,
            DistanceMetric::DotProduct,
        )
        .unwrap();

        assert_eq!(report.evaluated_queries, 1);
        assert_eq!(report.committed_hits, 1);
        assert_eq!(report.inflight_neutral_hits, 1);
        assert_eq!(report.mean_linearizable_recall, 1.0);
    }
}
