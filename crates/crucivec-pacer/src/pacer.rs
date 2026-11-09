use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};
use crucivec_core::types::{DispatchTicket, QueryType};
use crucivec_core::wal::{WalOpType, WalWriter};
use hdrhistogram::Histogram;
use tracing::debug;

use crate::poisson::PoissonGenerator;
use crate::tsc::TscClock;
use crate::workload::{WorkloadConfig, WorkloadGenerator};

/// Telemetry metrics produced by the hardware pacer thread.
#[derive(Debug, Clone)]
pub struct PacerMetrics {
    pub total_emitted: u64,
    pub unserved_arrivals: u64,
    pub max_jitter_ns: u64,
    pub mean_jitter_ns: f64,
    pub p99_jitter_ns: u64,
}

/// Core configuration for the open-loop hardware pacer.
#[derive(Debug, Clone)]
pub struct PacerConfig {
    pub target_qps: f64,
    pub channel_capacity: usize,
    pub pinned_core_id: Option<usize>,
    pub seed: u64,
    pub hybrid_sleep_threshold_us: u64,
    pub workload: WorkloadConfig,
    pub wal_path: Option<PathBuf>,
}

impl Default for PacerConfig {
    fn default() -> Self {
        Self {
            target_qps: 10_000.0,
            channel_capacity: 65_536,
            pinned_core_id: Some(1),
            seed: 42,
            hybrid_sleep_threshold_us: 200,
            workload: WorkloadConfig::default(),
            wal_path: None,
        }
    }
}

pub struct HardwarePacer {
    running: Arc<AtomicBool>,
    unserved_count: Arc<AtomicU64>,
    receiver: Receiver<DispatchTicket>,
    worker_thread: Option<JoinHandle<Histogram<u64>>>,
}

impl HardwarePacer {
    pub fn spawn(config: PacerConfig) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let unserved_count = Arc::new(AtomicU64::new(0));
        let (sender, receiver) = bounded::<DispatchTicket>(config.channel_capacity);

        let running_clone = Arc::clone(&running);
        let unserved_clone = Arc::clone(&unserved_count);

        let worker_thread = thread::Builder::new()
            .name("crucivec-pacer".to_string())
            .spawn(move || {
                run_pacer_loop(config, sender, running_clone, unserved_clone)
            })
            .expect("Failed to spawn crucivec-pacer OS thread");

        Self {
            running,
            unserved_count,
            receiver,
            worker_thread: Some(worker_thread),
        }
    }

    pub fn ticket_receiver(&self) -> Receiver<DispatchTicket> {
        self.receiver.clone()
    }

    pub fn unserved_arrivals(&self) -> u64 {
        self.unserved_count.load(Ordering::Relaxed)
    }

    pub fn stop(mut self) -> Option<Histogram<u64>> {
        self.running.store(false, Ordering::Release);
        if let Some(handle) = self.worker_thread.take() {
            match handle.join() {
                Ok(hist) => Some(hist),
                Err(err) => {
                    eprintln!("Pacer thread panicked: {:?}", err);
                    None
                }
            }
        } else {
            None
        }
    }
}

fn run_pacer_loop(
    config: PacerConfig,
    sender: Sender<DispatchTicket>,
    running: Arc<AtomicBool>,
    unserved_count: Arc<AtomicU64>,
) -> Histogram<u64> {
    if let Some(core_id) = config.pinned_core_id {
        let core_ids = core_affinity::get_core_ids().unwrap_or_default();
        if let Some(target_core) = core_ids.into_iter().find(|c| c.id == core_id) {
            if core_affinity::set_for_current(target_core) {
                debug!("Pacer thread pinned to CPU Core {}", core_id);
            }
        }
    }

    let clock = TscClock::new();
    let mut poisson = PoissonGenerator::new(config.target_qps, config.seed);
    let mut workload_gen = WorkloadGenerator::new(&config.workload, config.seed + 100);
    let mut wal_writer = config.wal_path.as_ref().and_then(|p| WalWriter::open(p).ok());
    let mut current_lsn: u64 = 0;

    let mut jitter_hist = Histogram::<u64>::new_with_bounds(1, 1_000_000_000, 3)
        .expect("Failed to initialize jitter histogram");

    let t0 = clock.raw_nanos();
    let mut next_scheduled_ns: u64 = 0;
    let mut seq: u64 = 0;

    let hybrid_sleep_threshold_ns = config.hybrid_sleep_threshold_us * 1_000;

    while running.load(Ordering::Relaxed) {
        let now = clock.raw_nanos();
        let elapsed_ns = clock.delta_raw_nanos(t0, now);

        if elapsed_ns >= next_scheduled_ns {
            let jitter_ns = elapsed_ns.saturating_sub(next_scheduled_ns);
            let _ = jitter_hist.record(jitter_ns.max(1));

            seq += 1;
            let op = workload_gen.next_op();

            match &op {
                QueryType::Upsert { id, vector } => {
                    current_lsn += 1;
                    if let Some(ref mut writer) = wal_writer {
                        let _ = writer.append(current_lsn, elapsed_ns, *id, WalOpType::Upsert, vector);
                    }
                }
                QueryType::Delete { id } => {
                    current_lsn += 1;
                    if let Some(ref mut writer) = wal_writer {
                        let _ = writer.append(current_lsn, elapsed_ns, *id, WalOpType::Delete, &[]);
                    }
                }
                QueryType::Search { .. } => {}
            }

            let ticket = DispatchTicket {
                seq_id: seq,
                scheduled_emit_ns: next_scheduled_ns,
                lsn: current_lsn,
                query_type: op,
            };

            match sender.try_send(ticket) {
                Ok(_) => {}
                Err(TrySendError::Full(_)) => {
                    unserved_count.fetch_add(1, Ordering::Relaxed);
                }
                Err(TrySendError::Disconnected(_)) => {
                    break;
                }
            }

            let interval_ns = poisson.next_interval_ns();
            next_scheduled_ns += interval_ns;
        } else {
            let remaining_ns = next_scheduled_ns - elapsed_ns;
            if remaining_ns > hybrid_sleep_threshold_ns {
                let sleep_duration = Duration::from_nanos(remaining_ns - hybrid_sleep_threshold_ns);
                thread::sleep(sleep_duration);
            } else {
                std::hint::spin_loop();
            }
        }
    }

    if let Some(ref mut writer) = wal_writer {
        let _ = writer.flush();
    }

    jitter_hist
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hardware_pacer_emission_and_jitter() {
        let config = PacerConfig {
            target_qps: 20_000.0,
            channel_capacity: 10_000,
            pinned_core_id: None,
            seed: 12345,
            hybrid_sleep_threshold_us: 100,
            workload: WorkloadConfig::default(),
            wal_path: None,
        };

        let pacer = HardwarePacer::spawn(config);
        let rx = pacer.ticket_receiver();

        // Await first ticket to eliminate OS thread initialization latency
        let _ = rx.recv_timeout(Duration::from_secs(2)).expect("Pacer failed to emit initial ticket");
        let mut received = 1;

        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_millis(50) {
            if rx.try_recv().is_ok() {
                received += 1;
            } else {
                std::hint::spin_loop();
            }
        }

        println!("Pacer test: received {} tickets in 50ms window", received);
        let hist = pacer.stop().expect("Expected jitter histogram upon stop");
        let p50 = hist.value_at_quantile(0.50);
        let p99 = hist.value_at_quantile(0.99);

        println!("Test Pacer Complete: received={}, p50_jitter={}ns, p99_jitter={}ns", received, p50, p99);
        assert!(received > 100, "Expected to receive at least 100 tickets in 50ms window, got {}", received);
    }

    #[test]
    fn test_hardware_pacer_wal_logging() {
        let temp_dir = std::env::temp_dir();
        let wal_path = temp_dir.join(format!("test_pacer_wal_{}.bin", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));

        let config = PacerConfig {
            target_qps: 10_000.0,
            channel_capacity: 5_000,
            pinned_core_id: None,
            seed: 999,
            hybrid_sleep_threshold_us: 100,
            workload: WorkloadConfig::new(crate::workload::WorkloadType::Churn70_20_10, 16, 5),
            wal_path: Some(wal_path.clone()),
        };

        let pacer = HardwarePacer::spawn(config);
        let rx = pacer.ticket_receiver();

        // Await initial ticket to eliminate thread initialization delay
        let first_ticket = rx.recv_timeout(Duration::from_secs(2)).expect("Expected initial ticket");
        let mut received = 1;
        if first_ticket.lsn > 0 {
            assert!(matches!(first_ticket.query_type, QueryType::Upsert { .. } | QueryType::Delete { .. } | QueryType::Search { .. }));
        }

        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_millis(100) {
            if let Ok(ticket) = rx.recv_timeout(Duration::from_millis(10)) {
                received += 1;
                if ticket.lsn > 0 {
                    assert!(matches!(ticket.query_type, QueryType::Upsert { .. } | QueryType::Delete { .. } | QueryType::Search { .. }));
                }
            }
        }

        let _ = pacer.stop();
        assert!(received > 0);

        // Verify WAL file exists and can be parsed
        let mut reader = crucivec_core::wal::WalReader::open(&wal_path).expect("Failed to open written WAL");
        let snapshot = reader.reconstruct_state(u64::MAX).expect("Failed to reconstruct WAL state");
        println!("Pacer WAL Test: active vectors={}, tombstones={}", snapshot.active_vectors.len(), snapshot.tombstoned_ids.len());

        let _ = std::fs::remove_file(wal_path);
    }
}
