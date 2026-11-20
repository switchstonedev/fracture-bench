use crate::simd::{cosine_distance, dot_product, euclidean_distance_sq};
use crate::types::DistanceMetric;
use std::collections::HashMap;

/// Contiguous memory-aligned vector arena with 64-bit active visibility bitsets.
/// Implements cache-blocked batch retrieval over contiguous buffers, avoiding heap fragmentation.
#[derive(Debug, Clone)]
pub struct VectorArena {
    pub data: Vec<f32>,
    pub dimension: usize,
    pub count: usize,
    pub visibility: Vec<u64>,
    pub id_to_idx: HashMap<u64, usize>,
    pub idx_to_id: Vec<u64>,
}

impl VectorArena {
    /// Allocates a new contiguous vector arena.
    pub fn new(dimension: usize, capacity: usize) -> Self {
        assert!(dimension > 0, "Dimension must be strictly positive");
        let bitset_words = capacity.div_ceil(64);
        Self {
            data: Vec::with_capacity(capacity * dimension),
            dimension,
            count: 0,
            visibility: vec![0u64; bitset_words],
            id_to_idx: HashMap::with_capacity(capacity),
            idx_to_id: Vec::with_capacity(capacity),
        }
    }

    /// Inserts or replaces a vector embedding with an assigned document ID.
    pub fn upsert(&mut self, id: u64, vector: &[f32]) -> Result<usize, &'static str> {
        if vector.len() != self.dimension {
            return Err("Vector dimension does not match arena dimension");
        }

        if let Some(&existing_idx) = self.id_to_idx.get(&id) {
            // Update in-place in contiguous arena
            let offset = existing_idx * self.dimension;
            self.data[offset..offset + self.dimension].copy_from_slice(vector);
            self.visibility[existing_idx / 64] |= 1u64 << (existing_idx % 64);
            Ok(existing_idx)
        } else {
            let idx = self.count;
            let required_word = idx / 64;
            if required_word >= self.visibility.len() {
                self.visibility.push(0u64);
            }

            self.data.extend_from_slice(vector);
            self.visibility[required_word] |= 1u64 << (idx % 64);
            self.id_to_idx.insert(id, idx);
            self.idx_to_id.push(id);
            self.count += 1;
            Ok(idx)
        }
    }

    /// Marks a document ID as deleted (tombstone) zero-copy by clearing its visibility bit.
    pub fn delete(&mut self, id: u64) -> bool {
        if let Some(&idx) = self.id_to_idx.get(&id) {
            let word_idx = idx / 64;
            let bit_mask = 1u64 << (idx % 64);
            if (self.visibility[word_idx] & bit_mask) != 0 {
                self.visibility[word_idx] &= !bit_mask;
                return true;
            }
        }
        false
    }

    /// Checks if a document ID is currently active and visible.
    #[inline(always)]
    pub fn is_visible(&self, id: u64) -> bool {
        if let Some(&idx) = self.id_to_idx.get(&id) {
            let word_idx = idx / 64;
            let bit_mask = 1u64 << (idx % 64);
            (self.visibility[word_idx] & bit_mask) != 0
        } else {
            false
        }
    }

    /// Returns the number of currently active, non-tombstoned vectors in the arena.
    pub fn active_count(&self) -> usize {
        let mut total = 0;
        for &word in &self.visibility {
            total += word.count_ones() as usize;
        }
        total
    }

    /// Batch scores a query across all active vectors in the arena using bitset-skipping.
    #[allow(clippy::needless_range_loop)]
    pub fn score_query(&self, query: &[f32], out_scores: &mut [f32]) -> Result<(), &'static str> {
        if query.len() != self.dimension {
            return Err("Query dimension mismatch");
        }
        if out_scores.len() < self.count {
            return Err("Output score buffer length is smaller than arena count");
        }

        let dim = self.dimension;
        for (word_idx, &mask) in self.visibility.iter().enumerate() {
            let start_idx = word_idx * 64;
            let end_idx = (start_idx + 64).min(self.count);

            if mask == 0 {
                // Entire block of 64 candidates is tombstoned: mark all as NEG_INFINITY zero-copy
                out_scores[start_idx..end_idx].fill(f32::NEG_INFINITY);
                continue;
            }

            for i in start_idx..end_idx {
                let bit = 1u64 << (i % 64);
                if (mask & bit) != 0 {
                    let offset = i * dim;
                    let candidate = &self.data[offset..offset + dim];
                    out_scores[i] = dot_product(query, candidate);
                } else {
                    out_scores[i] = f32::NEG_INFINITY;
                }
            }
        }
        Ok(())
    }

    /// Finds exact top-K nearest neighbors over visible candidates.
    pub fn find_top_k(&self, query: &[f32], k: usize, metric: DistanceMetric) -> Vec<(u64, f32)> {
        if k == 0 || self.count == 0 {
            return Vec::new();
        }

        let mut scored: Vec<(u64, f32)> = Vec::with_capacity(self.active_count());
        let dim = self.dimension;

        for (word_idx, &mask) in self.visibility.iter().enumerate() {
            if mask == 0 {
                continue;
            }

            let start_idx = word_idx * 64;
            let end_idx = (start_idx + 64).min(self.count);

            for i in start_idx..end_idx {
                let bit = 1u64 << (i % 64);
                if (mask & bit) != 0 {
                    let offset = i * dim;
                    let candidate = &self.data[offset..offset + dim];
                    let score = match metric {
                        DistanceMetric::DotProduct => dot_product(query, candidate),
                        DistanceMetric::Euclidean => euclidean_distance_sq(query, candidate),
                        DistanceMetric::Cosine => cosine_distance(query, candidate),
                    };
                    let doc_id = self.idx_to_id[i];
                    scored.push((doc_id, score));
                }
            }
        }

        match metric {
            DistanceMetric::DotProduct => {
                // Higher dot product is better
                scored.sort_unstable_by(|a, b| {
                    b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            DistanceMetric::Euclidean | DistanceMetric::Cosine => {
                // Lower distance is better
                scored.sort_unstable_by(|a, b| {
                    a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)
                });
            }
        }

        scored.truncate(k);
        scored
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn test_vector_arena_crud_and_bitset_skipping() {
        let mut arena = VectorArena::new(4, 128);
        assert_eq!(arena.active_count(), 0);

        arena.upsert(10, &[1.0, 0.0, 0.0, 0.0]).unwrap();
        arena.upsert(20, &[0.0, 1.0, 0.0, 0.0]).unwrap();
        arena.upsert(30, &[0.0, 0.0, 1.0, 0.0]).unwrap();

        assert_eq!(arena.active_count(), 3);
        assert!(arena.is_visible(10));
        assert!(arena.is_visible(20));
        assert!(arena.is_visible(30));

        let top = arena.find_top_k(&[1.0, 0.0, 0.0, 0.0], 2, DistanceMetric::DotProduct);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].0, 10);
        assert!((top[0].1 - 1.0).abs() < 1e-5);

        // Delete 10
        assert!(arena.delete(10));
        assert_eq!(arena.active_count(), 2);
        assert!(!arena.is_visible(10));

        let top_after = arena.find_top_k(&[1.0, 0.0, 0.0, 0.0], 2, DistanceMetric::DotProduct);
        assert_eq!(top_after.len(), 2);
        assert_ne!(top_after[0].0, 10);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(30))]
        #[test]
        fn test_proptest_simd_bitset_skipping_matches_scalar_with_random_tombstones(
            dim in proptest::sample::select(vec![4usize, 8, 16, 32, 64]),
            num_vectors in 10usize..=80usize,
            tombstone_pct in 0u32..=100u32,
            k in 1usize..=10usize,
            raw_floats in proptest::collection::vec(-10.0f32..10.0f32, 80 * 64),
            query_floats in proptest::collection::vec(-10.0f32..10.0f32, 64),
        ) {
            let query = &query_floats[0..dim];
            let mut arena = VectorArena::new(dim, num_vectors);
            let mut deleted_ids = std::collections::HashSet::new();
            let mut active_records = Vec::new();

            for i in 0..num_vectors {
                let id = (i as u64) + 1;
                let offset = i * dim;
                let vec_slice = &raw_floats[offset..offset + dim];
                arena.upsert(id, vec_slice).unwrap();

                // Deterministically decide tombstone based on percentage
                if ((id * 37) % 100) < (tombstone_pct as u64) {
                    arena.delete(id);
                    deleted_ids.insert(id);
                } else {
                    active_records.push((id, vec_slice.to_vec()));
                }
            }

            prop_assert_eq!(arena.active_count(), active_records.len());

            // 1. Check score_query against scalar dot product
            let mut scores = vec![0.0f32; num_vectors];
            arena.score_query(query, &mut scores).unwrap();

            for i in 0..num_vectors {
                let id = (i as u64) + 1;
                if deleted_ids.contains(&id) {
                    prop_assert_eq!(scores[i], f32::NEG_INFINITY);
                    prop_assert!(!arena.is_visible(id));
                } else {
                    let offset = i * dim;
                    let vec_slice = &raw_floats[offset..offset + dim];
                    let scalar_dot = crate::simd::neon::dot_product_scalar(query, vec_slice);
                    let diff = (scores[i] - scalar_dot).abs();
                    let rel_diff = diff / scalar_dot.abs().max(1.0);
                    prop_assert!(rel_diff < 1e-4 || diff < 1e-3, "Dot product tolerance exceeded: diff={}, rel_diff={}", diff, rel_diff);
                    prop_assert!(arena.is_visible(id));
                }
            }

            // 2. Check find_top_k for DotProduct
            let top_k_dot = arena.find_top_k(query, k, DistanceMetric::DotProduct);
            for (ret_id, _) in &top_k_dot {
                prop_assert!(!deleted_ids.contains(ret_id), "Tombstoned ID {:?} returned in top-k!", ret_id);
            }

            // 3. Check find_top_k for Euclidean
            let top_k_l2 = arena.find_top_k(query, k, DistanceMetric::Euclidean);
            for (ret_id, _) in &top_k_l2 {
                prop_assert!(!deleted_ids.contains(ret_id), "Tombstoned ID {:?} returned in Euclidean top-k!", ret_id);
            }

            // Compare top_k_l2 scores with scalar euclidean distance
            if !active_records.is_empty() {
                let mut scalar_l2_scored: Vec<(u64, f32)> = active_records
                    .iter()
                    .map(|(id, vec)| (*id, crate::simd::neon::euclidean_distance_sq_scalar(query, vec)))
                    .collect();
                scalar_l2_scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
                let expected_len = k.min(active_records.len());
                prop_assert_eq!(top_k_l2.len(), expected_len);

                for (idx, (ret_id, ret_score)) in top_k_l2.iter().enumerate() {
                    let (exp_id, exp_score) = scalar_l2_scored[idx];
                    let diff = (*ret_score - exp_score).abs();
                    let rel_diff = diff / exp_score.abs().max(1.0);
                    prop_assert!(rel_diff < 1e-4 || diff < 1e-3, "Euclidean tolerance exceeded: diff={}, rel_diff={}", diff, rel_diff);
                    if rel_diff < 1e-5 {
                        prop_assert_eq!(*ret_id, exp_id);
                    }
                }
            } else {
                prop_assert!(top_k_l2.is_empty());
            }
        }
    }
}
