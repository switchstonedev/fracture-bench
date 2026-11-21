#![cfg(target_arch = "x86_64")]

use std::arch::x86_64::*;

/// Computes the dot product between two f32 slices using 256-bit AVX2 + FMA instructions.
/// Slices must have identical lengths.
///
/// # Safety
/// The caller must ensure that:
/// - The host CPU supports AVX2 and FMA instruction sets.
/// - Slices `a` and `b` have equal lengths and point to valid memory.
#[target_feature(enable = "avx2,fma")]
#[inline]
pub unsafe fn dot_product_avx2(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len();
    let mut sum256_0 = _mm256_setzero_ps();
    let mut sum256_1 = _mm256_setzero_ps();

    let chunks = len / 16;
    let mut i = 0;

    for _ in 0..chunks {
        let va0 = _mm256_loadu_ps(a.as_ptr().add(i));
        let vb0 = _mm256_loadu_ps(b.as_ptr().add(i));
        sum256_0 = _mm256_fmadd_ps(va0, vb0, sum256_0);

        let va1 = _mm256_loadu_ps(a.as_ptr().add(i + 8));
        let vb1 = _mm256_loadu_ps(b.as_ptr().add(i + 8));
        sum256_1 = _mm256_fmadd_ps(va1, vb1, sum256_1);

        i += 16;
    }

    if i + 8 <= len {
        let va = _mm256_loadu_ps(a.as_ptr().add(i));
        let vb = _mm256_loadu_ps(b.as_ptr().add(i));
        sum256_0 = _mm256_fmadd_ps(va, vb, sum256_0);
        i += 8;
    }

    let combined = _mm256_add_ps(sum256_0, sum256_1);

    // Horizontal reduction of 8-lane 256-bit vector to single f32 scalar
    let high128 = _mm256_extractf128_ps(combined, 1);
    let low128 = _mm256_castps256_ps128(combined);
    let sum128 = _mm_add_ps(high128, low128);
    let shuf = _mm_movehl_ps(sum128, sum128);
    let sum64 = _mm_add_ps(sum128, shuf);
    let shuf2 = _mm_shuffle_ps(sum64, sum64, 1);
    let sum32 = _mm_add_ss(sum64, shuf2);

    let mut result = _mm_cvtss_f32(sum32);

    // Tail reduction for remaining elements (< 8)
    while i < len {
        result += *a.get_unchecked(i) * *b.get_unchecked(i);
        i += 1;
    }

    result
}

/// Computes squared Euclidean distance between two f32 slices using 256-bit AVX2 + FMA instructions.
///
/// # Safety
/// The caller must ensure that:
/// - The host CPU supports AVX2 and FMA instruction sets.
/// - Slices `a` and `b` have equal lengths and point to valid memory.
#[target_feature(enable = "avx2,fma")]
#[inline]
pub unsafe fn euclidean_distance_sq_avx2(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len();
    let mut sum256 = _mm256_setzero_ps();

    let chunks = len / 8;
    let mut i = 0;

    for _ in 0..chunks {
        let va = _mm256_loadu_ps(a.as_ptr().add(i));
        let vb = _mm256_loadu_ps(b.as_ptr().add(i));
        let diff = _mm256_sub_ps(va, vb);
        sum256 = _mm256_fmadd_ps(diff, diff, sum256);
        i += 8;
    }

    let high128 = _mm256_extractf128_ps(sum256, 1);
    let low128 = _mm256_castps256_ps128(sum256);
    let sum128 = _mm_add_ps(high128, low128);
    let shuf = _mm_movehl_ps(sum128, sum128);
    let sum64 = _mm_add_ps(sum128, shuf);
    let shuf2 = _mm_shuffle_ps(sum64, sum64, 1);
    let sum32 = _mm_add_ss(sum64, shuf2);

    let mut result = _mm_cvtss_f32(sum32);

    while i < len {
        let d = *a.get_unchecked(i) - *b.get_unchecked(i);
        result += d * d;
        i += 1;
    }

    result
}
