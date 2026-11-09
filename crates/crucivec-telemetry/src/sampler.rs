use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::cgroups_v2::{CgroupReader, CgroupStats, TelemetryError};

/// Recorded timestamped telemetry entry.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TelemetrySample {
    pub elapsed_ms: u64,
    pub stats: CgroupStats,
}

/// Aggregated telemetry results exported after benchmark completion.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelemetrySummary {
    pub total_samples: usize,
    pub initial_stats: CgroupStats,
    pub final_stats: CgroupStats,
    pub net_pgmajfault: u64,
    pub net_workingset_refault: u64,
    pub net_throttled_usec: u64,
}

/// Asynchronous background sampler polling cgroups v2 statistics.
pub struct TelemetrySampler {
    running: Arc<AtomicBool>,
    worker_handle: Option<JoinHandle<Vec<TelemetrySample>>>,
}

impl TelemetrySampler {
    /// Launches the background telemetry sampling thread.
    pub fn spawn(cgroup_path: PathBuf, sample_interval: Duration) -> Result<Self, TelemetryError> {
        let mut reader = CgroupReader::open(&cgroup_path)?;
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = Arc::clone(&running);

        let worker_handle = thread::Builder::new()
            .name("crucivec-telemetry".to_string())
            .spawn(move || {
                let start = Instant::now();
                let mut samples = Vec::with_capacity(10_000);

                while running_clone.load(Ordering::Relaxed) {
                    if let Ok(stats) = reader.sample() {
                        let elapsed_ms = start.elapsed().as_millis() as u64;
                        samples.push(TelemetrySample { elapsed_ms, stats });
                    }
                    thread::sleep(sample_interval);
                }

                samples
            })
            .map_err(|e| TelemetryError::Io(e))?;

        Ok(Self {
            running,
            worker_handle: Some(worker_handle),
        })
    }

    /// Stops the sampler and returns collected telemetry samples and summary.
    pub fn stop(mut self) -> (Vec<TelemetrySample>, TelemetrySummary) {
        self.running.store(false, Ordering::Release);
        let samples = if let Some(handle) = self.worker_handle.take() {
            handle.join().unwrap_or_default()
        } else {
            Vec::new()
        };

        let summary = if let (Some(first), Some(last)) = (samples.first(), samples.last()) {
            TelemetrySummary {
                total_samples: samples.len(),
                initial_stats: first.stats,
                final_stats: last.stats,
                net_pgmajfault: last.stats.pgmajfault.saturating_sub(first.stats.pgmajfault),
                net_workingset_refault: last.stats.workingset_refault.saturating_sub(first.stats.workingset_refault),
                net_throttled_usec: last.stats.throttled_usec.saturating_sub(first.stats.throttled_usec),
            }
        } else {
            TelemetrySummary::default()
        };

        (samples, summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telemetry_sampler_lifecycle() {
        let sampler = TelemetrySampler::spawn(PathBuf::from("."), Duration::from_millis(5)).unwrap();
        thread::sleep(Duration::from_millis(30));
        let (samples, summary) = sampler.stop();

        assert!(!samples.is_empty(), "Sampler should have captured at least one sample");
        assert!(summary.total_samples > 0);
    }
}
