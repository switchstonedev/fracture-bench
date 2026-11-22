#!/usr/bin/env bash
# ==============================================================================
# run_all.sh - Single-entrypoint reproduction script for Fracture Artifact Evaluation (PVLDB Vol. 20 EA&B).
#
# Exposes Three-Tier Benchmark Execution Hierarchy:
#   1. Dry Run (--dry-run / dry): Cryptographic & environment sanity check (< 10s).
#   2. Sample   (--sample / sample): Pilot benchmark, WAL verify, figure rendering (~20s).
#   3. Full     (--full / full): Complete test suite, benchmark replay, bootstrap CI and figure replication (default).
# ==============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${ROOT_DIR}"

RAW_TIER="${1:---full}"
TIER_LOWER="$(echo "${RAW_TIER}" | tr '[:upper:]' '[:lower:]' | sed 's/^--//')"

case "${TIER_LOWER}" in
    dry|dry-run|dry_run)
        BENCH_TIER="dry-run"
        ;;
    sample|small|small_test|smoke)
        BENCH_TIER="sample"
        ;;
    full|full_benchmark)
        BENCH_TIER="full"
        ;;
    *)
        echo "Unknown execution tier: ${RAW_TIER}"
        echo "Valid tiers: --dry-run (dry), --sample (sample), --full (full)"
        exit 1
        ;;
esac

SCRIPTS_DIR="scripts"
if [ ! -d "${SCRIPTS_DIR}" ] && [ -d "Scripts" ]; then
    SCRIPTS_DIR="Scripts"
fi

RES_DIR="results"
if [ ! -d "${RES_DIR}" ] && [ -d "Results" ]; then
    RES_DIR="Results"
fi

echo "=========================================================="
echo "  Fracture: Vector Databases Under the Crucible (Artifacts)  "
echo "  Execution Mode: ${BENCH_TIER}                              "
echo "=========================================================="

# 1. Audit cryptographic integrity of replication bundle
echo "[Step 1/5] Verifying SHA-256 manifests..."
python3 "${SCRIPTS_DIR}/verify_integrity.py"

if [ "${BENCH_TIER}" == "dry-run" ]; then
    echo "=========================================================="
    echo " [DRY-RUN SUCCESSFUL] SHA-256 manifests verified (< 10s)!"
    echo "=========================================================="
    exit 0
fi

# 2. Execute Rust Fracture Load Generator & WAL Verifier
if [ "${BENCH_TIER}" == "full" ]; then
    echo "[Step 2/5] Running Rust harness unit & integration tests..."
    cargo test --quiet
fi

echo "[Step 2/5] Executing open-loop benchmark and point-in-time WAL replay..."
cargo run --bin fracture --quiet -- run --target mock --duration-sec 2 --qps 1000 --wal-path "${RES_DIR}/traces/run.wal" --output "${RES_DIR}/traces/benchmark.json"
cargo run --bin fracture --quiet -- verify --wal-path "${RES_DIR}/traces/run.wal" --cutoff-lsn 500 --k 10
python3 "${SCRIPTS_DIR}/generate_report.py" --input "${RES_DIR}/traces/benchmark.json" --output "${RES_DIR}/figures/dual_cdf.svg"

# 3. Render publication vector plots (ACM / PVLDB 3.33 in)
echo "[Step 3/5] Generating publication-grade vector figures..."
python3 "${SCRIPTS_DIR}/plot_dual_cdf.py"
python3 "${SCRIPTS_DIR}/plot_pareto.py"
python3 "${SCRIPTS_DIR}/plot_filtering_pathology.py"
python3 "${SCRIPTS_DIR}/plot_cgroups_timeseries.py"

if [ "${BENCH_TIER}" == "full" ]; then
    # 4. Compute non-parametric bootstrap confidence intervals
    echo "[Step 4/5] Evaluating 95% bootstrap confidence bounds..."
    python3 Analysis/bootstrap_ci.py
fi

# 5. Final verification of generated deliverables
echo "[Step 5/5] Verifying generated PDF and SVG deliverables..."
ls -lh "${RES_DIR}"/figures/*.pdf "${RES_DIR}"/figures/*.svg

echo "=========================================================="
echo " [SUCCESS - Tier: ${BENCH_TIER}] All deliverables generated."
echo "=========================================================="
