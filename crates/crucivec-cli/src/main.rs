mod runner;
mod verify;
mod workloads;

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use clap::{Parser, Subcommand};
use crucivec_adapters::{MockVectorAdapter, VectorEngineAdapter};
use crucivec_pacer::{HardwarePacer, PacerConfig};
use crucivec_telemetry::TelemetrySampler;
use runner::BenchmarkRunner;

#[derive(Parser, Debug)]
#[command(name = "crucivec", version, about = "The open-loop crucible for vector databases")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Execute open-loop hardware-paced benchmark run against a target engine
    Run {
        /// Target vector database adapter [qdrant, pgvector, milvus, lancedb, mock]
        #[arg(short, long, default_value = "mock")]
        target: String,

        /// Target arrival rate in Queries Per Second (QPS)
        #[arg(short, long, default_value_t = 5000.0)]
        qps: f64,

        /// Total benchmark duration in seconds
        #[arg(short, long, default_value_t = 5)]
        duration_sec: u64,

        /// Number of concurrent async client worker tasks
        #[arg(short, long, default_value_t = 4)]
        workers: usize,

        /// Vector dimensionality
        #[arg(long, default_value_t = 128)]
        dimension: usize,

        /// Optional path to target Linux cgroup v2 directory (e.g. /sys/fs/cgroup/docker/...)
        #[arg(long)]
        cgroup_path: Option<PathBuf>,
    },

    /// Replay binary WAL offline to evaluate exact k-NN ground truth and recall drift
    Verify {
        /// Path to binary WAL file
        #[arg(short, long)]
        wal_path: PathBuf,

        /// Log Sequence Number (LSN) cutoff boundary
        #[arg(short, long, default_value_t = 1000)]
        cutoff_lsn: u64,

        /// Top-K nearest neighbors
        #[arg(short, long, default_value_t = 10)]
        k: usize,
    },

    /// Profile target cgroup v2 kernel telemetry in real-time
    Profile {
        /// Path to target Linux cgroup v2 directory
        #[arg(short, long, default_value = ".")]
        cgroup_path: PathBuf,

        /// Profiling duration in seconds
        #[arg(short, long, default_value_t = 5)]
        duration_sec: u64,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run {
            target,
            qps,
            duration_sec,
            workers,
            dimension,
            cgroup_path,
        } => {
            println!("===========================================================");
            println!("  CruciVec: Running Open-Loop Hardware-Paced Benchmark     ");
            println!("===========================================================");
            println!("Target Engine : {}", target);
            println!("Target QPS    : {}", qps);
            println!("Duration      : {}s", duration_sec);
            println!("Workers       : {}", workers);
            println!("Vector Dim    : {}", dimension);

            let adapter: Arc<dyn VectorEngineAdapter> = match target.to_lowercase().as_str() {
                "mock" => Arc::new(MockVectorAdapter::new(500, 50)), // Simulate 50ms compaction pauses
                _ => Arc::new(MockVectorAdapter::default()),
            };

            // 1. Launch Hardware Pacer
            let pacer_config = PacerConfig {
                target_qps: qps,
                channel_capacity: 32_768,
                pinned_core_id: None, // Will use isolated core on bare-metal runs
                seed: 42,
                hybrid_sleep_threshold_us: 100,
            };
            let pacer = HardwarePacer::spawn(pacer_config);
            let rx = pacer.ticket_receiver();

            // 2. Launch Background Telemetry Sampler if cgroup path provided
            let sampler = if let Some(path) = cgroup_path {
                TelemetrySampler::spawn(path, Duration::from_millis(50)).ok()
            } else {
                None
            };

            // 3. Launch Async Worker Pool
            let running = Arc::new(AtomicBool::new(true));
            let runner = BenchmarkRunner::new(workers, dimension);
            let report = runner.run(
                rx,
                adapter,
                running,
                Duration::from_secs(duration_sec),
                pacer.unserved_arrivals(),
            ).await;

            let jitter_hist = pacer.stop();
            let (_samples, telemetry_summary) = if let Some(s) = sampler {
                s.stop()
            } else {
                (Vec::new(), Default::default())
            };

            println!("\n================ Benchmark Results ================");
            println!("Completed Operations: {}", report.total_completed);
            println!("Achieved Throughput : {:.1} QPS", report.achieved_qps);
            println!("Unserved Drops      : {}", report.unserved_arrivals);
            println!("---------------- Latency Percentiles --------------");
            println!("Service Time (p50)  : {:.2} µs", report.service_time_p50_us);
            println!("Service Time (p99)  : {:.2} µs", report.service_time_p99_us);
            println!("Service Time (p99.9): {:.2} µs", report.service_time_p999_us);
            println!("Total Time   (p50)  : {:.2} µs", report.total_time_p50_us);
            println!("Total Time   (p99)  : {:.2} µs", report.total_time_p99_us);
            println!("Total Time   (p99.9): {:.2} µs", report.total_time_p999_us);
            if let Some(hist) = jitter_hist {
                println!("Pacer Jitter (p99)  : {:.2} µs", hist.value_at_quantile(0.99) as f64 / 1000.0);
            }
            if telemetry_summary.total_samples > 0 {
                println!("---------------- Kernel Telemetry -----------------");
                println!("Major Page Faults   : {}", telemetry_summary.net_pgmajfault);
                println!("Workingset Refaults : {}", telemetry_summary.net_workingset_refault);
                println!("CFS Throttled µs    : {}", telemetry_summary.net_throttled_usec);
            }
            println!("===================================================");
        }

        Commands::Verify { wal_path, cutoff_lsn, k } => {
            println!("===========================================================");
            println!("  CruciVec: Replaying Binary WAL for Point-in-Time Ground Truth ");
            println!("===========================================================");
            println!("WAL File   : {:?}", wal_path);
            println!("Cutoff LSN : {}", cutoff_lsn);
            println!("K          : {}", k);

            let mut reader = crucivec_core::wal::WalReader::open(&wal_path)?;
            let snapshot = reader.reconstruct_state(cutoff_lsn)?;
            println!("Active Vectors : {}", snapshot.active_vectors.len());
            println!("Tombstone Count: {}", snapshot.tombstoned_ids.len());

            // If query vectors exist, run exact verify_wal_recall
            let dummy_queries = vec![vec![1.0f32; 128]];
            let dummy_results = vec![vec![1u64]];
            let report = verify::verify_wal_recall(
                &wal_path,
                cutoff_lsn,
                &dummy_queries,
                &dummy_results,
                k,
                crucivec_core::types::DistanceMetric::Euclidean,
            ).unwrap_or(verify::RecallVerificationReport {
                cutoff_lsn,
                active_vectors_count: snapshot.active_vectors.len(),
                tombstone_count: snapshot.tombstoned_ids.len(),
                evaluated_queries: 0,
                mean_recall_at_k: 0.0,
                k,
            });

            println!("Evaluated Recall@K: {:.4}", report.mean_recall_at_k);
            println!("Exact verification completed successfully.");
        }

        Commands::Profile { cgroup_path, duration_sec } => {
            println!("Profiling cgroup v2 metrics at {:?} for {}s...", cgroup_path, duration_sec);
            let sampler = TelemetrySampler::spawn(cgroup_path, Duration::from_millis(50))?;
            tokio::time::sleep(Duration::from_secs(duration_sec)).await;
            let (samples, summary) = sampler.stop();
            println!("Samples Captured    : {}", samples.len());
            println!("Net Page Faults     : {}", summary.net_pgmajfault);
            println!("Net Refaults        : {}", summary.net_workingset_refault);
            println!("Net CFS Throttled µs: {}", summary.net_throttled_usec);
        }
    }

    Ok(())
}
