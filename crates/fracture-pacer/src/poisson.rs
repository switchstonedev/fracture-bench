use rand::Rng;
use rand_xoshiro::rand_core::SeedableRng;
use rand_xoshiro::Xoroshiro128PlusPlus;

/// Inverse-CDF Poisson inter-arrival interval generator.
/// Generates intervals in nanoseconds according to an exponential distribution:
/// Delta_t = -ln(U) / lambda
pub struct PoissonGenerator {
    rng: Xoroshiro128PlusPlus,
    lambda_per_ns: f64,
}

impl PoissonGenerator {
    /// Creates a new Poisson arrival generator with a target query arrival rate in QPS.
    pub fn new(target_qps: f64, seed: u64) -> Self {
        assert!(target_qps > 0.0, "Target QPS must be strictly positive");
        // Convert QPS to events per nanosecond: lambda_per_ns = target_qps / 1_000_000_000.0
        let lambda_per_ns = target_qps / 1_000_000_000.0;
        Self {
            rng: Xoroshiro128PlusPlus::seed_from_u64(seed),
            lambda_per_ns,
        }
    }

    /// Computes the next inter-arrival interval in nanoseconds.
    #[inline(always)]
    pub fn next_interval_ns(&mut self) -> u64 {
        // Sample U in (0.0, 1.0]
        let u: f64 = self.rng.gen();
        let u_clamped = if u <= 0.0 { 1e-15 } else { u };
        let delta_ns = -u_clamped.ln() / self.lambda_per_ns;
        delta_ns.max(1.0) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_poisson_distribution_mean() {
        let target_qps = 50_000.0;
        let expected_mean_ns = 1_000_000_000.0 / target_qps; // 20,000 ns
        let mut gen = PoissonGenerator::new(target_qps, 42);

        let n = 100_000;
        let mut sum_ns: u64 = 0;
        for _ in 0..n {
            sum_ns += gen.next_interval_ns();
        }

        let empirical_mean_ns = (sum_ns as f64) / (n as f64);
        let error_ratio = (empirical_mean_ns - expected_mean_ns).abs() / expected_mean_ns;

        // Empirical mean of 100k Poisson samples must match theoretical mean within 2%
        assert!(
            error_ratio < 0.02,
            "Empirical Poisson mean {} deviated from theoretical {} by {:.2}%",
            empirical_mean_ns,
            expected_mean_ns,
            error_ratio * 100.0
        );
    }
}
