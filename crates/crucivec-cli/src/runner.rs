use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use crucivec_adapters::VectorEngineAdapter;
use crucivec_core::types::{DispatchTicket, QueryType};
use hdrhistogram::Histogram;
use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;

use crate::workloads::WorkloadGenerator;

type WorkerMetrics = (Histogram<u64>, Histogram<u64>, u64);

/// Final structured benchmark performance report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkReport {
    pub duration_sec: f64,
    pub total_completed: u64,
    pub unserved_arrivals: u64,
    pub achieved_qps: f64,
    pub service_time_p50_us: f64,
    pub service_time_p90_us: f64,
    pub service_time_p99_us: f64,
    pub service_time_p999_us: f64,
    pub total_time_p50_us: f64,
    pub total_time_p90_us: f64,
    pub total_time_p99_us: f64,
    pub total_time_p999_us: f64,
}

/// Decoupled worker pool executing operations against the target adapter.
pub struct BenchmarkRunner {
    workers: usize,
    dimension: usize,
}

impl BenchmarkRunner {
    pub fn new(workers: usize, dimension: usize) -> Self {
        Self { workers, dimension }
    }

    /// Executes the benchmark run, merging worker thread histograms upon completion.
    pub async fn run(
        &self,
        receiver: Receiver<DispatchTicket>,
        adapter: Arc<dyn VectorEngineAdapter>,
        running: Arc<AtomicBool>,
        duration: Duration,
        unserved_arrivals: u64,
    ) -> BenchmarkReport {
        let mut handles: Vec<JoinHandle<WorkerMetrics>> = Vec::with_capacity(self.workers);

        for worker_id in 0..self.workers {
            let rx = receiver.clone();
            let adapter_clone = Arc::clone(&adapter);
            let running_clone = Arc::clone(&running);
            let dimension = self.dimension;

            let handle = tokio::spawn(async move {
                let mut service_hist = Histogram::<u64>::new_with_bounds(1, 60_000_000_000, 3).unwrap();
                let mut total_hist = Histogram::<u64>::new_with_bounds(1, 60_000_000_000, 3).unwrap();
                let mut completed = 0;
                let mut workload = WorkloadGenerator::new_churn_70_20_10(dimension, 1000 + worker_id as u64);

                let run_epoch = Instant::now();

                while running_clone.load(Ordering::Relaxed) {
                    let ticket = match rx.recv_timeout(Duration::from_millis(50)) {
                        Ok(t) => t,
                        Err(_) => continue,
                    };

                    let intended_emit_ns = ticket.scheduled_emit_ns;
                    let t_sent = Instant::now();

                    // Execute request against target database adapter
                    match ticket.query_type {
                        QueryType::Search { k, filter } => {
                            let query_vec = workload.generate_vector();
                            let _ = adapter_clone.search(&query_vec, k, filter.as_ref()).await;
                        }
                        QueryType::Upsert { id, vector } => {
                            let _ = adapter_clone.upsert(id, &vector).await;
                        }
                        QueryType::Delete { id } => {
                            let _ = adapter_clone.delete(id).await;
                        }
                    }

                    let t_recv = Instant::now();
                    let service_time_ns = t_recv.duration_since(t_sent).as_nanos() as u64;

                    // Total response time accounts for client queue delay from scheduled emission
                    let actual_recv_ns = run_epoch.elapsed().as_nanos() as u64;
                    let total_response_time_ns = actual_recv_ns.saturating_sub(intended_emit_ns).max(service_time_ns);

                    let _ = service_hist.record(service_time_ns.max(1));
                    let _ = total_hist.record(total_response_time_ns.max(1));
                    completed += 1;
                }

                (service_hist, total_hist, completed)
            });

            handles.push(handle);
        }

        // Wait for run duration
        tokio::time::sleep(duration).await;
        running.store(false, Ordering::Release);

        // Collect and merge worker histograms
        let mut global_service_hist = Histogram::<u64>::new_with_bounds(1, 60_000_000_000, 3).unwrap();
        let mut global_total_hist = Histogram::<u64>::new_with_bounds(1, 60_000_000_000, 3).unwrap();
        let mut total_completed = 0;

        for h in handles {
            if let Ok((s_hist, t_hist, count)) = h.await {
                global_service_hist.add(&s_hist).unwrap();
                global_total_hist.add(&t_hist).unwrap();
                total_completed += count;
            }
        }

        let dur_sec = duration.as_secs_f64();
        let achieved_qps = (total_completed as f64) / dur_sec;

        BenchmarkReport {
            duration_sec: dur_sec,
            total_completed,
            unserved_arrivals,
            achieved_qps,
            service_time_p50_us: (global_service_hist.value_at_quantile(0.50) as f64) / 1000.0,
            service_time_p90_us: (global_service_hist.value_at_quantile(0.90) as f64) / 1000.0,
            service_time_p99_us: (global_service_hist.value_at_quantile(0.99) as f64) / 1000.0,
            service_time_p999_us: (global_service_hist.value_at_quantile(0.999) as f64) / 1000.0,
            total_time_p50_us: (global_total_hist.value_at_quantile(0.50) as f64) / 1000.0,
            total_time_p90_us: (global_total_hist.value_at_quantile(0.90) as f64) / 1000.0,
            total_time_p99_us: (global_total_hist.value_at_quantile(0.99) as f64) / 1000.0,
            total_time_p999_us: (global_total_hist.value_at_quantile(0.999) as f64) / 1000.0,
        }
    }
}

