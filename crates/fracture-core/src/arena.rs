use std::collections::HashMap;
use crate::types::DistanceMetric;
use crate::simd::{dot_product, euclidean_distance_sq, cosine_distance};

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
            if mask == 0 {
                // Skip entire block of 64 tombstoned candidates zero-copy
                continue;
            }

            let start_idx = word_idx * 64;
            let end_idx = (start_idx + 64).min(self.count);

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
    pub fn find_top_k(
        &self,
        query: &[f32],
        k: usize,
        metric: DistanceMetric,
    ) -> Vec<(u64, f32)> {
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
                scored.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            }
            DistanceMetric::Euclidean | DistanceMetric::Cosine => {
                // Lower distance is better
                scored.sort_unstable_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            }
        }

        scored.truncate(k);
        scored
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
