pub mod avx2;
pub mod neon;

use crate::types::DistanceMetric;
use rayon::prelude::*;
use std::cmp::Ordering;

/// Computes the dot product between two f32 vectors with dynamic CPU feature dispatch.
pub fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "Vector slice dimensions must match");

    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
            return unsafe { avx2::dot_product_avx2(a, b) };
        }
    }

    neon::dot_product_scalar(a, b)
}

/// Computes squared Euclidean distance between two f32 vectors with dynamic CPU feature dispatch.
pub fn euclidean_distance_sq(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "Vector slice dimensions must match");

    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
            return unsafe { avx2::euclidean_distance_sq_avx2(a, b) };
        }
    }

    neon::euclidean_distance_sq_scalar(a, b)
}

/// Computes Cosine distance ($1.0 - \text{cosine\_similarity}$) between two vectors.
pub fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    let dot = dot_product(a, b);
    let norm_a = dot_product(a, a).sqrt();
    let norm_b = dot_product(b, b).sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 1.0;
    }
    1.0 - (dot / (norm_a * norm_b))
}

/// Evaluates exact ground truth top-K nearest neighbors using Rayon multi-threaded brute-force scan.
/// Returns sorted vector of `(id, distance)` pairs.
pub fn find_exact_top_k(
    query: &[f32],
    candidates: &[(u64, Vec<f32>)],
    k: usize,
    metric: DistanceMetric,
) -> Vec<(u64, f32)> {
    if candidates.is_empty() || k == 0 {
        return Vec::new();
    }

    // Parallel scoring across available cores using Rayon
    let mut scored: Vec<(u64, f32)> = candidates
        .par_iter()
        .map(|(id, vec)| {
            let score = match metric {
                DistanceMetric::DotProduct => -dot_product(query, vec), // Negate so lower score = closer
                DistanceMetric::Euclidean => euclidean_distance_sq(query, vec),
                DistanceMetric::Cosine => cosine_distance(query, vec),
            };
            (*id, score)
        })
        .collect();

    // Partial sort or full sort to extract top-K smallest distances
    if scored.len() > k {
        scored.select_nth_unstable_by(k, |a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));
        scored.truncate(k);
    }

    scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));
    scored
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simd_vs_scalar_equivalence() {
        let v1: Vec<f32> = (0..128).map(|x| (x as f32) * 0.01).collect();
        let v2: Vec<f32> = (0..128).map(|x| ((x * 2) as f32) * 0.01).collect();

        let dot_scalar = neon::dot_product_scalar(&v1, &v2);
        let dot_dispatch = dot_product(&v1, &v2);
        assert!(
            (dot_scalar - dot_dispatch).abs() < 1e-4,
            "Scalar and SIMD dot products must match within epsilon"
        );

        let l2_scalar = neon::euclidean_distance_sq_scalar(&v1, &v2);
        let l2_dispatch = euclidean_distance_sq(&v1, &v2);
        assert!(
            (l2_scalar - l2_dispatch).abs() < 1e-4,
            "Scalar and SIMD L2 distances must match within epsilon"
        );
    }

    #[test]
    fn test_exact_top_k() {
        let query = vec![1.0, 0.0, 0.0, 0.0];
        let candidates = vec![
            (1, vec![1.0, 0.0, 0.0, 0.0]), // Distance 0
            (2, vec![0.0, 1.0, 0.0, 0.0]), // Distance 2
            (3, vec![0.9, 0.1, 0.0, 0.0]), // Distance 0.02
        ];

        let top = find_exact_top_k(&query, &candidates, 2, DistanceMetric::Euclidean);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].0, 1);
        assert_eq!(top[1].0, 3);
    }
}
