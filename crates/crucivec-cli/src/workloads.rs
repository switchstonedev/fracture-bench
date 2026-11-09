use crucivec_core::types::{FilterPredicate, QueryType};
use rand::Rng;
use rand_xoshiro::rand_core::SeedableRng;
use rand_xoshiro::Xoroshiro128PlusPlus;

/// Calibrated workload generator producing operations according to defined churn ratios.
#[allow(dead_code)]
pub struct WorkloadGenerator {
    rng: Xoroshiro128PlusPlus,
    search_ratio: f64,
    upsert_ratio: f64,
    delete_ratio: f64,
    dimension: usize,
    next_id: u64,
}

impl WorkloadGenerator {
    /// Initializes a 70% Search, 20% Upsert, 10% Delete churn generator.
    pub fn new_churn_70_20_10(dimension: usize, seed: u64) -> Self {
        Self {
            rng: Xoroshiro128PlusPlus::seed_from_u64(seed),
            search_ratio: 0.70,
            upsert_ratio: 0.20,
            delete_ratio: 0.10,
            dimension,
            next_id: 1,
        }
    }

    /// Generates the next workload operation (QueryType).
    #[allow(dead_code)]
    pub fn next_op(&mut self) -> QueryType {
        let p: f64 = self.rng.gen();
        if p < self.search_ratio {
            // Search operation with stratified filter
            let filter_tier: f64 = self.rng.gen();
            let filter = if filter_tier < 0.20 {
                // High selectivity: ~60% pass rate
                Some(FilterPredicate::High {
                    modulo: 10,
                    allowed_remainders: vec![0, 1, 2, 3, 4, 5],
                })
            } else if filter_tier < 0.35 {
                // Medium selectivity: ~10% pass rate
                Some(FilterPredicate::Medium {
                    modulo: 10,
                    target_remainder: 0,
                })
            } else if filter_tier < 0.45 {
                // Needle in a haystack: ~0.1% pass rate
                Some(FilterPredicate::Needle {
                    modulo: 1000,
                    target_remainder: 42,
                })
            } else {
                None
            };
            QueryType::Search { k: 10, filter }
        } else if p < self.search_ratio + self.upsert_ratio {
            let id = self.next_id;
            self.next_id += 1;
            QueryType::Upsert { id }
        } else {
            // Delete operation targeting an existing lower ID
            let target_id = if self.next_id > 10 {
                self.rng.gen_range(1..self.next_id - 5)
            } else {
                1
            };
            QueryType::Delete { id: target_id }
        }
    }

    /// Generates a normalized synthetic query vector with zero NaNs/Infs.
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workload_ratios_and_vector_sanitization() {
        let mut gen = WorkloadGenerator::new_churn_70_20_10(128, 999);
        let mut searches = 0;
        let mut upserts = 0;
        let mut deletes = 0;

        for _ in 0..10_000 {
            match gen.next_op() {
                QueryType::Search { .. } => searches += 1,
                QueryType::Upsert { .. } => upserts += 1,
                QueryType::Delete { .. } => deletes += 1,
            }
        }

        let total = 10_000.0;
        let search_pct = (searches as f64) / total;
        let upsert_pct = (upserts as f64) / total;
        let delete_pct = (deletes as f64) / total;

        assert!((search_pct - 0.70).abs() < 0.03, "Search ratio should be ~70%");
        assert!((upsert_pct - 0.20).abs() < 0.03, "Upsert ratio should be ~20%");
        assert!((delete_pct - 0.10).abs() < 0.02, "Delete ratio should be ~10%");

        let vec = gen.generate_vector();
        assert_eq!(vec.len(), 128);
        for x in vec {
            assert!(!x.is_nan() && !x.is_infinite());
        }
    }
}

