#!/usr/bin/env bash
# ==============================================================================
# run_all.sh - Single-entrypoint reproduction script for Fracture Benchmark.
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
echo "  Fracture: Vector Databases Under the Crucible            "
echo "  Execution Mode: ${BENCH_TIER}                              "
echo "=========================================================="

if [ "${BENCH_TIER}" == "dry-run" ]; then
    echo "[Step 1/3] Executing dry-run sanity check..."
    cargo check --quiet
    cargo run --bin fracture --quiet -- --help >/dev/null
    echo "=========================================================="
    echo " [DRY-RUN SUCCESSFUL] Sanity check completed (< 10s)!"
    echo "=========================================================="
    exit 0
fi

# 1. Execute Rust Fracture Load Generator & WAL Verifier
if [ "${BENCH_TIER}" == "full" ]; then
    echo "[Step 1/4] Running Rust harness unit & integration tests..."
    cargo test --quiet
fi

echo "[Step 1/4] Executing open-loop benchmark and point-in-time WAL replay..."
cargo run --bin fracture --quiet -- run --target mock --duration-sec 2 --qps 1000 --wal-path "${RES_DIR}/traces/run.wal" --output "${RES_DIR}/traces/benchmark.json"
cargo run --bin fracture --quiet -- verify --wal-path "${RES_DIR}/traces/run.wal" --cutoff-lsn 500 --k 10
python3 "${SCRIPTS_DIR}/generate_report.py" --input "${RES_DIR}/traces/benchmark.json" --output "${RES_DIR}/figures/dual_cdf.svg"

# 2. Render vector plots
echo "[Step 2/4] Generating vector figures..."
python3 "${SCRIPTS_DIR}/plot_dual_cdf.py"
python3 "${SCRIPTS_DIR}/plot_pareto.py"
python3 "${SCRIPTS_DIR}/plot_filtering_pathology.py"
python3 "${SCRIPTS_DIR}/plot_cgroups_timeseries.py"

if [ "${BENCH_TIER}" == "full" ]; then
    # 3. Compute non-parametric bootstrap confidence intervals
    echo "[Step 3/4] Evaluating 95% bootstrap confidence bounds..."
    python3 Analysis/bootstrap_ci.py
fi

# 4. Final verification of generated deliverables
echo "[Step 4/4] Verifying generated benchmark deliverables..."
ls -lh "${RES_DIR}"/figures/*.svg

echo "=========================================================="
echo " [SUCCESS - Tier: ${BENCH_TIER}] All deliverables generated."
echo "=========================================================="
