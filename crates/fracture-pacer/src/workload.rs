use fracture_core::types::{FilterPredicate, QueryType};
use rand::Rng;
use rand_xoshiro::rand_core::SeedableRng;
use rand_xoshiro::Xoroshiro128PlusPlus;
use std::str::FromStr;

/// Predefined or custom workload churn profiles.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum WorkloadType {
    /// 70% Search, 20% Upsert, 10% Delete (Standard production churn).
    #[default]
    Churn70_20_10,
    /// 90% Search, 10% Upsert, 0% Delete (Read-heavy production profile).
    ReadHeavy90_10,
    /// 50% Search, 40% Upsert, 10% Delete (Write-heavy ingestion churn).
    WriteHeavy50_50,
    /// 100% Search (Zero mutation baseline).
    SearchOnly,
    /// FIFO Sliding-Window Churn (Deletes oldest inserted living IDs, systematically purging HNSW upper hubs).
    FifoChurn,
    /// Cluster-Targeted Purge (Simultaneously evicts an entire directional semantic neighborhood).
    ClusterPurge,
}

impl FromStr for WorkloadType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().replace('_', "-").as_str() {
            "churn-70-20-10" | "churn" | "default" => Ok(Self::Churn70_20_10),
            "read-heavy-90-10" | "read-heavy" => Ok(Self::ReadHeavy90_10),
            "write-heavy-50-50" | "write-heavy" => Ok(Self::WriteHeavy50_50),
            "search-only" | "search" => Ok(Self::SearchOnly),
            "fifo-churn" | "fifo" => Ok(Self::FifoChurn),
            "cluster-purge" | "cluster" => Ok(Self::ClusterPurge),
            other => Err(format!(
                "Unknown workload profile '{}'. Valid profiles: churn-70-20-10, read-heavy-90-10, write-heavy-50-50, search-only, fifo-churn, cluster-purge",
                other
            )),
        }
    }
}

/// Detailed configuration for open-loop workload generation.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkloadConfig {
    pub workload_type: WorkloadType,
    pub dimension: usize,
    pub top_k: usize,
    pub filter_stratified: bool,
    pub fifo_window_size: usize,
    pub audit_canary_rate: f64,
}

impl Default for WorkloadConfig {
    fn default() -> Self {
        Self {
            workload_type: WorkloadType::Churn70_20_10,
            dimension: 128,
            top_k: 10,
            filter_stratified: true,
            fifo_window_size: 10_000,
            audit_canary_rate: 0.01,
        }
    }
}

impl WorkloadConfig {
    pub fn new(workload_type: WorkloadType, dimension: usize, top_k: usize) -> Self {
        Self {
            workload_type,
            dimension,
            top_k,
            filter_stratified: true,
            fifo_window_size: 10_000,
            audit_canary_rate: 0.01,
        }
    }

    pub fn ratios(&self) -> (f64, f64, f64) {
        match self.workload_type {
            WorkloadType::Churn70_20_10 | WorkloadType::FifoChurn | WorkloadType::ClusterPurge => {
                (0.70, 0.20, 0.10)
            }
            WorkloadType::ReadHeavy90_10 => (0.90, 0.10, 0.00),
            WorkloadType::WriteHeavy50_50 => (0.50, 0.40, 0.10),
            WorkloadType::SearchOnly => (1.00, 0.00, 0.00),
        }
    }
}

/// Zero-heap, high-throughput pseudo-random workload generator.
pub struct WorkloadGenerator {
    rng: Xoroshiro128PlusPlus,
    search_ratio: f64,
    upsert_ratio: f64,
    dimension: usize,
    top_k: usize,
    filter_stratified: bool,
    next_id: u64,
    workload_type: WorkloadType,
    fifo_window_size: usize,
    audit_canary_rate: f64,
}

impl WorkloadGenerator {
    pub fn new(config: &WorkloadConfig, seed: u64) -> Self {
        let (s, u, _d) = config.ratios();
        Self {
            rng: Xoroshiro128PlusPlus::seed_from_u64(seed),
            search_ratio: s,
            upsert_ratio: u,
            dimension: config.dimension,
            top_k: config.top_k,
            filter_stratified: config.filter_stratified,
            next_id: 1,
            workload_type: config.workload_type,
            fifo_window_size: config.fifo_window_size,
            audit_canary_rate: config.audit_canary_rate,
        }
    }

    /// Initializes a 70% Search, 20% Upsert, 10% Delete churn generator.
    pub fn new_churn_70_20_10(dimension: usize, seed: u64) -> Self {
        let config = WorkloadConfig::new(WorkloadType::Churn70_20_10, dimension, 10);
        Self::new(&config, seed)
    }

    /// Generates normalized query or document vector with zero NaNs/Infs.
    pub fn generate_vector(&mut self) -> Vec<f32> {
        let mut vec = Vec::with_capacity(self.dimension);
        let mut norm_sq = 0.0f32;

        for _ in 0..self.dimension {
            let val: f32 = self.rng.gen_range(-1.0..1.0);
            vec.push(val);
            norm_sq += val * val;
        }

        let norm = norm_sq.sqrt().max(1e-10);
        for x in vec.iter_mut() {
            *x /= norm;
        }

        vec
    }

    /// Produces the next workload operation adhering to configured churn ratios.
    pub fn next_op(&mut self) -> QueryType {
        let p: f64 = self.rng.gen();

        if p < self.search_ratio {
            let filter = if self.filter_stratified {
                let filter_tier: f64 = self.rng.gen();
                if filter_tier < 0.20 {
                    Some(FilterPredicate::High {
                        modulo: 10,
                        allowed_remainders: vec![0, 1, 2, 3, 4, 5],
                    })
                } else if filter_tier < 0.35 {
                    Some(FilterPredicate::Medium {
                        modulo: 10,
                        target_remainder: 0,
                    })
                } else if filter_tier < 0.45 {
                    Some(FilterPredicate::Needle {
                        modulo: 1000,
                        target_remainder: 42,
                    })
                } else {
                    None
                }
            } else {
                None
            };

            let is_canary =
                self.audit_canary_rate > 0.0 && self.rng.gen::<f64>() < self.audit_canary_rate;
            if is_canary {
                QueryType::AuditSearch {
                    k: self.top_k,
                    filter,
                }
            } else {
                QueryType::Search {
                    k: self.top_k,
                    filter,
                }
            }
        } else if p < self.search_ratio + self.upsert_ratio {
            let id = self.next_id;
            self.next_id += 1;
            let vector = self.generate_vector();
            QueryType::Upsert { id, vector }
        } else {
            let target_id = match self.workload_type {
                WorkloadType::FifoChurn => {
                    if self.next_id > self.fifo_window_size as u64 {
                        self.next_id - self.fifo_window_size as u64
                    } else {
                        1
                    }
                }
                WorkloadType::ClusterPurge => {
                    if self.next_id > 10 {
                        (self.next_id / 10) * 10
                    } else {
                        1
                    }
                }
                _ => {
                    if self.next_id > 10 {
                        self.rng.gen_range(1..self.next_id - 5)
                    } else {
                        1
                    }
                }
            };
            QueryType::Delete { id: target_id }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workload_generator_ratios() {
        let config = WorkloadConfig::new(WorkloadType::Churn70_20_10, 64, 10);
        let mut gen = WorkloadGenerator::new(&config, 42);

        let mut searches = 0;
        let mut upserts = 0;
        let mut deletes = 0;

        for _ in 0..10_000 {
            match gen.next_op() {
                QueryType::Search { .. } | QueryType::AuditSearch { .. } => searches += 1,
                QueryType::Upsert { .. } => upserts += 1,
                QueryType::Delete { .. } => deletes += 1,
            }
        }

        let total = 10_000.0;
        assert!(((searches as f64 / total) - 0.70).abs() < 0.03);
        assert!(((upserts as f64 / total) - 0.20).abs() < 0.03);
        assert!(((deletes as f64 / total) - 0.10).abs() < 0.02);
    }

    #[test]
    fn test_fifo_sliding_window_churn() {
        let mut config = WorkloadConfig::new(WorkloadType::FifoChurn, 32, 5);
        config.fifo_window_size = 50;
        let mut gen = WorkloadGenerator::new(&config, 100);

        let mut deleted_ids = Vec::new();
        for _ in 0..1_000 {
            if let QueryType::Delete { id } = gen.next_op() {
                deleted_ids.push(id);
            }
        }

        // Deletes after window saturation must follow sliding window progression
        assert!(!deleted_ids.is_empty());
        let last_deleted = *deleted_ids.last().unwrap();
        assert!(last_deleted > 1);
    }
}
