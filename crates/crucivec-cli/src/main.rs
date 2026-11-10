mod runner;
mod verify;
mod workloads;

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use clap::{Parser, Subcommand};
use crucivec_adapters::{
    LanceDbAdapter, LanceDbConfig, MilvusAdapter, MilvusConfig, MockVectorAdapter, PgvectorAdapter,
    PgvectorConfig, QdrantAdapter, QdrantConfig, VectorEngineAdapter,
};
use crucivec_core::dataset::SyntheticDataset;
use crucivec_core::types::{CollectionSchema, DistanceMetric};
use crucivec_core::wal::{WalOpType, WalReader, WalWriter};
use crucivec_pacer::{HardwarePacer, PacerConfig, WorkloadConfig, WorkloadType};
use crucivec_telemetry::TelemetrySampler;
use runner::BenchmarkRunner;

#[derive(Parser, Debug)]
#[command(
    name = "crucivec",
    version,
    about = "The open-loop crucible for vector databases"
)]
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

        /// Workload profile [churn-70-20-10, read-heavy-90-10, write-heavy-50-50, search-only]
        #[arg(long, default_value = "churn-70-20-10")]
        workload: String,

        /// Optional path to record mutation WAL file
        #[arg(long)]
        wal_path: Option<PathBuf>,

        /// Optional path to target Linux cgroup v2 directory (e.g. /sys/fs/cgroup/docker/...)
        #[arg(long)]
        cgroup_path: Option<PathBuf>,

        /// Optional path to export JSON benchmark results
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Target database endpoint URI (e.g. http://127.0.0.1:6333)
        #[arg(short, long)]
        endpoint: Option<String>,

        /// Number of nearest neighbors to retrieve per query
        #[arg(short, long, default_value_t = 10)]
        k: usize,

        /// Optional path to dataset file (.fvecs, .bvecs, .bin, or .json) providing realistic query vectors
        #[arg(long)]
        dataset: Option<PathBuf>,

        /// Perform dry-run validation without issuing load or running pacing loops
        #[arg(long)]
        dry_run: bool,
    },

    /// Pre-populate target vector engine with initial corpus (bulk ingestion)
    Seed {
        /// Target vector database adapter [qdrant, pgvector, milvus, lancedb, mock]
        #[arg(short, long, default_value = "mock")]
        target: String,

        /// Number of vectors to seed into the collection (when generating synthetic corpus)
        #[arg(short, long, default_value_t = 10000)]
        count: usize,

        /// Vector dimensionality
        #[arg(long, default_value_t = 128)]
        dimension: usize,

        /// Batch size for bulk upsert requests
        #[arg(short, long, default_value_t = 500)]
        batch_size: usize,

        /// Optional path to input dataset file (.fvecs, .bvecs, .bin, or .json)
        #[arg(short, long)]
        dataset: Option<PathBuf>,

        /// Target database endpoint URI
        #[arg(short, long)]
        endpoint: Option<String>,

        /// Optional WAL path to record seed state for offline verification
        #[arg(long)]
        wal_path: Option<PathBuf>,

        /// Perform dry-run validation of dataset and connectivity without inserting vectors
        #[arg(long)]
        dry_run: bool,
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

        /// Optional path to query vectors file (.json or .bin)
        #[arg(short, long)]
        queries: Option<PathBuf>,

        /// Optional target engine to evaluate live recall against WAL [mock, qdrant, pgvector, milvus, lancedb]
        #[arg(short, long)]
        target: Option<String>,

        /// Optional endpoint for target engine
        #[arg(short, long)]
        endpoint: Option<String>,

        /// Optional path to export JSON recall verification report
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Perform dry-run validation without issuing k-NN evaluation
        #[arg(long)]
        dry_run: bool,
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

fn create_adapter(target: &str, endpoint: Option<String>) -> Arc<dyn VectorEngineAdapter> {
    match target.to_lowercase().as_str() {
        "mock" => Arc::new(MockVectorAdapter::new(500, 50)), // Simulate 50ms compaction pauses
        "qdrant" => {
            let mut cfg = QdrantConfig::default();
            if let Some(ep) = endpoint {
                cfg.endpoint = ep;
            }
            Arc::new(QdrantAdapter::new(cfg))
        }
        "milvus" => {
            let mut cfg = MilvusConfig::default();
            if let Some(ep) = endpoint {
                cfg.endpoint = ep;
            }
            Arc::new(MilvusAdapter::new(cfg))
        }
        "pgvector" => {
            let mut cfg = PgvectorConfig::default();
            if let Some(ep) = endpoint {
                cfg.connection_string = ep;
            }
            Arc::new(PgvectorAdapter::new(cfg))
        }
        "lancedb" => {
            let mut cfg = LanceDbConfig::default();
            if let Some(ep) = endpoint {
                cfg.uri = ep;
            }
            Arc::new(LanceDbAdapter::new(cfg))
        }
        other => {
            eprintln!(
                "Unknown target engine '{}', defaulting to Mock adapter",
                other
            );
            Arc::new(MockVectorAdapter::default())
        }
    }
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
            workload,
            wal_path,
            cgroup_path,
            output,
            endpoint,
            k,
            dataset,
            dry_run,
        } => {
            println!("===========================================================");
            println!("  CruciVec: Running Open-Loop Hardware-Paced Benchmark     ");
            println!("===========================================================");
            println!("Target Engine : {}", target);
            println!("Target QPS    : {}", qps);
            println!("Duration      : {}s", duration_sec);
            println!("Workers       : {}", workers);
            println!("Vector Dim    : {}", dimension);
            println!("Workload      : {}", workload);
            if dry_run {
                println!("Execution Mode: Dry Run (zero load, validation only)");
            }
            if let Some(ref d) = dataset {
                println!("Query Dataset : {:?}", d);
            }
            if let Some(ref w) = wal_path {
                if let Some(parent) = w.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                println!("WAL Output    : {:?}", w);
            }
            if let Some(ref o) = output {
                if let Some(parent) = o.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
            }
            if let Some(ref c) = cgroup_path {
                println!("Cgroup v2 Path: {:?}", c);
            }

            let adapter = create_adapter(&target, endpoint);

            // Initialize target schema
            let schema = CollectionSchema {
                name: "crucivec_bench".to_string(),
                dimension,
                metric: DistanceMetric::Cosine,
            };
            if let Err(e) = adapter.init(&schema).await {
                eprintln!("Notice: Engine init returned: {}", e);
            }

            if dry_run {
                println!("[Dry Run] Probing engine connectivity with a test query...");
                let test_vec = vec![0.0f32; dimension];
                let _ = adapter.search(&test_vec, 1, None).await;
                println!(
                    "[Dry Run] Validation successful. Engine '{}' is accessible.",
                    target
                );

                if let Some(out_path) = output {
                    let full_export = serde_json::json!({
                        "target": target,
                        "target_qps": qps,
                        "dimension": dimension,
                        "workload": workload,
                        "dry_run": true,
                        "report": {
                            "duration_sec": 0.0,
                            "total_completed": 0,
                            "unserved_arrivals": 0,
                            "achieved_qps": 0.0,
                            "service_time_p50_us": 0.0,
                            "service_time_p90_us": 0.0,
                            "service_time_p99_us": 0.0,
                            "service_time_p999_us": 0.0,
                            "total_time_p50_us": 0.0,
                            "total_time_p90_us": 0.0,
                            "total_time_p99_us": 0.0,
                            "total_time_p999_us": 0.0,
                        },
                        "pacer_jitter_p99_us": 0.0,
                        "telemetry": {
                            "total_samples": 0,
                            "net_pgmajfault": 0,
                            "net_workingset_refault": 0,
                            "net_throttled_usec": 0
                        }
                    });
                    let mut f = File::create(&out_path)?;
                    f.write_all(serde_json::to_string_pretty(&full_export)?.as_bytes())?;
                    println!("Exported benchmark JSON -> {:?}", out_path);
                }
                return Ok(());
            }

            let query_vectors = if let Some(ref d_path) = dataset {
                let qs = verify::load_queries_from_file(d_path)?;
                println!("Loaded {} query vectors from {:?}", qs.len(), d_path);
                Some(Arc::new(qs))
            } else {
                None
            };

            let w_type: WorkloadType = workload.parse().unwrap_or(WorkloadType::Churn70_20_10);
            let w_config = WorkloadConfig::new(w_type, dimension, k);

            // 1. Launch Hardware Pacer
            let pacer_config = PacerConfig {
                target_qps: qps,
                channel_capacity: 65_536,
                pinned_core_id: None, // Pin on bare metal runs
                seed: 42,
                hybrid_sleep_threshold_us: 100,
                workload: w_config,
                wal_path,
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
            let report = runner
                .run(
                    rx,
                    adapter,
                    running,
                    Duration::from_secs(duration_sec),
                    pacer.unserved_arrivals(),
                    query_vectors,
                )
                .await;

            let jitter_hist = pacer.stop();
            let (_samples, telemetry_summary) = if let Some(s) = sampler {
                s.stop()
            } else {
                (Vec::new(), Default::default())
            };

            let pacer_p99_jitter = jitter_hist
                .as_ref()
                .map(|h| h.value_at_quantile(0.99) as f64 / 1000.0)
                .unwrap_or(0.0);

            println!("\n================ Benchmark Results ================");
            println!("Completed Operations: {}", report.total_completed);
            println!("Achieved Throughput : {:.1} QPS", report.achieved_qps);
            println!("Unserved Drops      : {}", report.unserved_arrivals);
            println!("---------------- Latency Percentiles --------------");
            println!("Service Time (p50)  : {:.2} µs", report.service_time_p50_us);
            println!("Service Time (p90)  : {:.2} µs", report.service_time_p90_us);
            println!("Service Time (p99)  : {:.2} µs", report.service_time_p99_us);
            println!(
                "Service Time (p99.9): {:.2} µs",
                report.service_time_p999_us
            );
            println!("Total Time   (p50)  : {:.2} µs", report.total_time_p50_us);
            println!("Total Time   (p90)  : {:.2} µs", report.total_time_p90_us);
            println!("Total Time   (p99)  : {:.2} µs", report.total_time_p99_us);
            println!("Total Time   (p99.9): {:.2} µs", report.total_time_p999_us);
            println!("Pacer Jitter (p99)  : {:.2} µs", pacer_p99_jitter);
            if telemetry_summary.total_samples > 0 {
                println!("---------------- Kernel Telemetry -----------------");
                println!("Major Page Faults   : {}", telemetry_summary.net_pgmajfault);
                println!(
                    "Workingset Refaults : {}",
                    telemetry_summary.net_workingset_refault
                );
                println!(
                    "CFS Throttled µs    : {}",
                    telemetry_summary.net_throttled_usec
                );
            }
            println!("===================================================");

            // Export results if output path requested
            if let Some(out_path) = output {
                let full_export = serde_json::json!({
                    "target": target,
                    "target_qps": qps,
                    "dimension": dimension,
                    "workload": workload,
                    "report": report,
                    "pacer_jitter_p99_us": pacer_p99_jitter,
                    "telemetry": telemetry_summary,
                });
                let mut f = File::create(&out_path)?;
                f.write_all(serde_json::to_string_pretty(&full_export)?.as_bytes())?;
                println!("Exported benchmark JSON -> {:?}", out_path);
            }
        }

        Commands::Seed {
            target,
            count,
            dimension,
            batch_size,
            dataset,
            endpoint,
            wal_path,
            dry_run,
        } => {
            println!("===========================================================");
            println!("  CruciVec: Pre-populating Target Database Corpus (Seed)   ");
            println!("===========================================================");
            println!("Target Engine : {}", target);
            println!("Target Count  : {}", count);
            println!("Vector Dim    : {}", dimension);
            println!("Batch Size    : {}", batch_size);
            if dry_run {
                println!("Execution Mode: Dry Run (zero load, validation only)");
            }
            if let Some(ref d) = dataset {
                println!("Dataset File  : {:?}", d);
            } else {
                println!("Dataset Mode  : Synthetic (L2-normalized)");
            }
            if let Some(ref w) = wal_path {
                if let Some(parent) = w.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                println!("WAL Output    : {:?}", w);
            }

            let adapter = create_adapter(&target, endpoint);
            let schema = CollectionSchema {
                name: "crucivec_bench".to_string(),
                dimension,
                metric: DistanceMetric::Cosine,
            };
            if let Err(e) = adapter.init(&schema).await {
                eprintln!("Notice: Engine init returned: {}", e);
            }

            if dry_run {
                println!(
                    "[Dry Run] Target engine '{}' initialized successfully.",
                    target
                );
                if let Some(ref d) = dataset {
                    let qs = verify::load_queries_from_file(d)?;
                    println!(
                        "[Dry Run] Dataset {:?} validated: contains {} vectors.",
                        d,
                        qs.len()
                    );
                } else {
                    println!(
                        "[Dry Run] Synthetic generator validated: {} vectors of dim {}.",
                        count, dimension
                    );
                }
                println!("[Dry Run] Complete: 0 vectors written.");
                return Ok(());
            }

            let mut wal_writer = wal_path.as_ref().and_then(|p| WalWriter::open(p).ok());
            let mut current_lsn: u64 = 0;

            let start_time = std::time::Instant::now();
            let mut seeded_count = 0;

            if let Some(ref data_path) = dataset {
                let all_vectors = verify::load_queries_from_file(data_path)?;
                let total = all_vectors.len().min(count);
                println!(
                    "Loaded {} vectors from dataset file (seeding up to {})",
                    all_vectors.len(),
                    total
                );

                for chunk in all_vectors[..total].chunks(batch_size) {
                    let mut batch = Vec::with_capacity(chunk.len());
                    for vec in chunk {
                        seeded_count += 1;
                        let id = seeded_count as u64;
                        if let Some(ref mut writer) = wal_writer {
                            current_lsn += 1;
                            let elapsed_ns = start_time.elapsed().as_nanos() as u64;
                            let _ =
                                writer.append(current_lsn, elapsed_ns, id, WalOpType::Upsert, vec);
                        }
                        batch.push((id, vec.clone()));
                    }
                    adapter.upsert_batch(&batch).await?;
                    if seeded_count % 5000 == 0 || seeded_count == total {
                        let elapsed_sec = start_time.elapsed().as_secs_f64().max(0.001);
                        let rate = seeded_count as f64 / elapsed_sec;
                        println!(
                            "  -> [{}/{} vectors seeded ({:.1}%)] - {:.1} vec/s",
                            seeded_count,
                            total,
                            (seeded_count as f64 / total as f64) * 100.0,
                            rate
                        );
                    }
                }
            } else {
                let mut synth = SyntheticDataset::new(dimension, 42);
                let total = count;
                while seeded_count < total {
                    let this_batch_size = batch_size.min(total - seeded_count);
                    let mut batch = Vec::with_capacity(this_batch_size);
                    for _ in 0..this_batch_size {
                        seeded_count += 1;
                        let id = seeded_count as u64;
                        let vec = synth.next_vector();
                        if let Some(ref mut writer) = wal_writer {
                            current_lsn += 1;
                            let elapsed_ns = start_time.elapsed().as_nanos() as u64;
                            let _ =
                                writer.append(current_lsn, elapsed_ns, id, WalOpType::Upsert, &vec);
                        }
                        batch.push((id, vec));
                    }
                    adapter.upsert_batch(&batch).await?;
                    if seeded_count % 5000 == 0 || seeded_count == total {
                        let elapsed_sec = start_time.elapsed().as_secs_f64().max(0.001);
                        let rate = seeded_count as f64 / elapsed_sec;
                        println!(
                            "  -> [{}/{} vectors seeded ({:.1}%)] - {:.1} vec/s",
                            seeded_count,
                            total,
                            (seeded_count as f64 / total as f64) * 100.0,
                            rate
                        );
                    }
                }
            }

            let total_sec = start_time.elapsed().as_secs_f64().max(0.001);
            let throughput = seeded_count as f64 / total_sec;

            println!("================ Seeding Completed ================");
            println!("Seeded Vectors      : {}", seeded_count);
            println!("Total Elapsed Time  : {:.2}s", total_sec);
            println!("Ingestion Throughput: {:.1} vectors/sec", throughput);
            if let Some(w) = wal_path {
                println!("Mutation WAL Saved  : {:?} (final LSN: {})", w, current_lsn);
            }
            println!("===================================================");
        }

        Commands::Verify {
            wal_path,
            cutoff_lsn,
            k,
            queries,
            target,
            endpoint,
            output,
            dry_run,
        } => {
            println!("===========================================================");
            println!("  CruciVec: Replaying Binary WAL for Point-in-Time Ground Truth ");
            println!("===========================================================");
            println!("WAL File   : {:?}", wal_path);
            println!("Cutoff LSN : {}", cutoff_lsn);
            println!("K          : {}", k);
            if dry_run {
                println!("Execution Mode: Dry Run (zero load, validation only)");
            }

            let mut reader = WalReader::open(&wal_path)?;
            if dry_run {
                println!(
                    "[Dry Run] Successfully opened and validated WAL file {:?}",
                    wal_path
                );
                if let Some(out_path) = output {
                    if let Some(parent) = out_path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let report = verify::RecallVerificationReport {
                        cutoff_lsn,
                        active_vectors_count: 0,
                        tombstone_count: 0,
                        evaluated_queries: 0,
                        k,
                        mean_recall_at_k: 0.0,
                    };
                    let mut f = File::create(&out_path)?;
                    f.write_all(serde_json::to_string_pretty(&report)?.as_bytes())?;
                    println!("Exported verification JSON -> {:?}", out_path);
                }
                println!("[Dry Run] Complete: WAL header valid.");
                return Ok(());
            }
            let snapshot = reader.reconstruct_state(cutoff_lsn)?;
            println!("Active Vectors : {}", snapshot.active_vectors.len());
            println!("Tombstone Count: {}", snapshot.tombstoned_ids.len());

            if snapshot.active_vectors.is_empty() {
                println!(
                    "Notice: WAL contains zero active vectors at cutoff LSN {}.",
                    cutoff_lsn
                );
                return Ok(());
            }

            // Determine query vectors
            let dim = snapshot
                .active_vectors
                .values()
                .next()
                .map(|v| v.len())
                .unwrap_or(128);
            let query_list = if let Some(ref q_path) = queries {
                verify::load_queries_from_file(q_path)?
            } else {
                verify::generate_synthetic_queries(dim, 50, 42)
            };
            println!("Evaluated Queries: {}", query_list.len());

            let target_adapter = create_adapter(target.as_deref().unwrap_or("mock"), endpoint);

            // If using mock adapter, load active vectors into mock adapter to test exact retrieval
            if target.is_none() || target.as_deref() == Some("mock") {
                let schema = CollectionSchema {
                    name: "verify".into(),
                    dimension: dim,
                    metric: DistanceMetric::Cosine,
                };
                let _ = target_adapter.init(&schema).await;
                for (&id, vec) in &snapshot.active_vectors {
                    let _ = target_adapter.upsert(id, vec).await;
                }
            }

            let report = verify::verify_with_target_adapter(
                &wal_path,
                cutoff_lsn,
                &query_list,
                target_adapter,
                k,
                DistanceMetric::Cosine,
            )
            .await?;

            println!("---------------- Ground Truth Verification --------------");
            println!("Cutoff LSN         : {}", report.cutoff_lsn);
            println!("Active Vectors     : {}", report.active_vectors_count);
            println!("Tombstones         : {}", report.tombstone_count);
            println!("Evaluated Queries  : {}", report.evaluated_queries);
            println!("Top-K Candidates   : {}", report.k);
            println!(
                "Mean Recall@{}     : {:.4}",
                report.k, report.mean_recall_at_k
            );
            println!("=========================================================");

            if let Some(out_path) = output {
                if let Some(parent) = out_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let mut f = File::create(&out_path)?;
                f.write_all(serde_json::to_string_pretty(&report)?.as_bytes())?;
                println!("Exported verification JSON -> {:?}", out_path);
            }
        }

        Commands::Profile {
            cgroup_path,
            duration_sec,
        } => {
            println!(
                "Profiling cgroup v2 metrics at {:?} for {}s...",
                cgroup_path, duration_sec
            );
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
