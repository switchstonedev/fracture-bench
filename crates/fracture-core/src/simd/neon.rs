/// Computes scalar fallback dot product.
#[inline(always)]
pub fn dot_product_scalar(a: &[f32], b: &[f32]) -> f32 {
    let mut sum0 = 0.0f32;
    let mut sum1 = 0.0f32;
    let mut sum2 = 0.0f32;
    let mut sum3 = 0.0f32;

    let chunks = a.len() / 4;
    let remainder = a.len() % 4;

    for i in 0..chunks {
        let base = i * 4;
        sum0 += a[base] * b[base];
        sum1 += a[base + 1] * b[base + 1];
        sum2 += a[base + 2] * b[base + 2];
        sum3 += a[base + 3] * b[base + 3];
    }

    let tail_start = chunks * 4;
    let mut tail = 0.0f32;
    for i in 0..remainder {
        tail += a[tail_start + i] * b[tail_start + i];
    }

    (sum0 + sum1) + (sum2 + sum3) + tail
}

/// Computes scalar fallback squared Euclidean distance.
#[inline(always)]
pub fn euclidean_distance_sq_scalar(a: &[f32], b: &[f32]) -> f32 {
    let mut sum = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        let diff = *x - *y;
        sum += diff * diff;
    }
    sum
}
