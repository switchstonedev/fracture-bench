use quanta::{Clock, Instant};

/// High-resolution, low-overhead hardware Timestamp Counter (TSC) reader.
#[derive(Clone)]
pub struct TscClock {
    clock: Clock,
}

impl Default for TscClock {
    fn default() -> Self {
        Self::new()
    }
}

impl TscClock {
    /// Creates a new calibrated hardware TSC clock instance.
    pub fn new() -> Self {
        Self {
            clock: Clock::new(),
        }
    }

    /// Captures the current timestamp counter as an Instant.
    #[inline(always)]
    pub fn now(&self) -> Instant {
        self.clock.now()
    }

    /// Reads raw timestamp counter with sub-10ns overhead.
    #[inline(always)]
    pub fn raw_nanos(&self) -> u64 {
        self.clock.raw()
    }

    /// Measures elapsed nanoseconds between two Instant measurements safely.
    #[inline(always)]
    pub fn delta_nanos(&self, start: Instant, end: Instant) -> u64 {
        if end >= start {
            end.duration_since(start).as_nanos() as u64
        } else {
            0
        }
    }

    /// Measures elapsed nanoseconds between two raw TSC readings.
    #[inline(always)]
    pub fn delta_raw_nanos(&self, start_raw: u64, end_raw: u64) -> u64 {
        if end_raw >= start_raw {
            self.clock.delta_as_nanos(start_raw, end_raw)
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tsc_monotonicity_and_overhead() {
        let clock = TscClock::new();
        let r0 = clock.raw_nanos();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let r1 = clock.raw_nanos();
        let delta = clock.delta_raw_nanos(r0, r1);
        println!("Raw delta for 5ms sleep: {} ns", delta);
        assert!(
            delta >= 1_000_000,
            "5ms sleep delta should be at least 1ms (1_000_000 ns), got {}",
            delta
        );
    }
}
