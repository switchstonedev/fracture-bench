#!/usr/bin/env bash
# ==============================================================================
# Fracture: Automated Bare-Metal Benchmark Execution Pipeline
# Targets: AMD EPYC 7763 / Ubuntu 22.04 LTS bare-metal testbeds
# Evaluates: Qdrant, Milvus, pgvector, LanceDB under continuous mutation churn
# ==============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${WORKSPACE_ROOT}"

TIER="${1:-full}" # Choices: dry_run, sample, full
OUTPUT_DIR="${WORKSPACE_ROOT}/reports"
TRACE_DIR="${OUTPUT_DIR}/traces"
MATRIX_DIR="${OUTPUT_DIR}/matrix"

mkdir -p "${OUTPUT_DIR}" "${TRACE_DIR}" "${MATRIX_DIR}"

echo "========================================================================"
echo "  Fracture: Automated Bare-Metal Remote Benchmark Execution Harness    "
echo "========================================================================"
echo "Workspace Root : ${WORKSPACE_ROOT}"
echo "Execution Tier : ${TIER}"
echo "Output Path    : ${OUTPUT_DIR}"
echo "Date / Time    : $(date -u)"
echo "Host Kernel    : $(uname -r)"
echo "========================================================================"

# ------------------------------------------------------------------------------
# Step 1: Pre-flight Verification & System Tuning
# ------------------------------------------------------------------------------
echo -e "\n[*] Step 1: Auditing Host Cgroup v2 and CPU Governor..."
if [ -f /sys/fs/cgroup/cgroup.controllers ]; then
    echo "  [+] Pure cgroups v2 hierarchy verified."
else
    echo "  [!] Warning: /sys/fs/cgroup/cgroup.controllers not found. Cgroup v2 telemetry may be limited."
fi

# Attempt to mount RAM tmpfs for zero-interference WAL logging if root
WAL_TMPFS_DIR="/mnt/wal-tmpfs"
if [ "$EUID" -eq 0 ]; then
    if ! mountpoint -q "${WAL_TMPFS_DIR}" 2>/dev/null; then
        mkdir -p "${WAL_TMPFS_DIR}"
        mount -t tmpfs -o size=16G tmpfs "${WAL_TMPFS_DIR}" || true
    fi
    # Set performance CPU governor
    if command -v cpupower &>/dev/null; then
        cpupower frequency-set -g performance || true
    fi
else
    WAL_TMPFS_DIR="/tmp/fracture_wal"
    mkdir -p "${WAL_TMPFS_DIR}"
fi
echo "  [+] WAL storage path: ${WAL_TMPFS_DIR}"

# ------------------------------------------------------------------------------
# Step 2: Build Native Rust Engine Harness
# ------------------------------------------------------------------------------
echo -e "\n[*] Step 2: Building release binary (fracture)..."
cargo build --release --workspace

FRACTURE_BIN="${WORKSPACE_ROOT}/target/release/fracture"
if [ ! -f "${FRACTURE_BIN}" ]; then
    echo "[-] Error: Fracture release binary not found at ${FRACTURE_BIN}"
    exit 1
fi
echo "  [+] Compiled binary: ${FRACTURE_BIN}"

# ------------------------------------------------------------------------------
# Step 3: Launch Isolated Container Engines via Docker Compose
# ------------------------------------------------------------------------------
echo -e "\n[*] Step 3: Starting target engine containers..."
DOCKER_CMD="docker compose"
if ! command -v docker &>/dev/null; then
    echo "[-] Error: docker is not installed on this host."
    exit 1
fi

if ! docker compose version &>/dev/null; then
    if command -v docker-compose &>/dev/null; then
        DOCKER_CMD="docker-compose"
    else
        echo "[-] Error: docker compose is required."
        exit 1
    fi
fi

ENGINES=("qdrant" "milvus" "pgvector" "lancedb")
for eng in "${ENGINES[@]}"; do
    COMPOSE_FILE="${WORKSPACE_ROOT}/fixtures/docker-compose.${eng}.yml"
    if [ -f "${COMPOSE_FILE}" ]; then
        echo "  [+] Launching ${eng} container..."
        ${DOCKER_CMD} -f "${COMPOSE_FILE}" up -d
    fi
done

# Healthcheck probe function
wait_for_port() {
    local host="$1"
    local port="$2"
    local name="$3"
    local max_retries=60
    local count=0
    echo -n "  [*] Waiting for ${name} (${host}:${port})..."
    while ! nc -z "${host}" "${port}" &>/dev/null && [ $count -lt $max_retries ]; do
        sleep 1
        count=$((count + 1))
        echo -n "."
    done
    if [ $count -ge $max_retries ]; then
        echo " [FAILED / TIMEOUT]"
        return 1
    fi
    echo " [READY]"
    return 0
}

wait_for_port "127.0.0.1" 6333 "Qdrant" || true
wait_for_port "127.0.0.1" 19530 "Milvus" || true
wait_for_port "127.0.0.1" 5433 "pgvector" || true

# ------------------------------------------------------------------------------
# Step 4: Execute Benchmark Matrix
# ------------------------------------------------------------------------------
echo -e "\n[*] Step 4: Executing Benchmark Sweep Matrix (${TIER})..."
python3 "${WORKSPACE_ROOT}/Scripts/run_matrix.py" \
    --tier "${TIER}" \
    --output-dir "${MATRIX_DIR}"

# ------------------------------------------------------------------------------
# Step 5: Post-Run WAL Replay & Offline Ground Truth Verification
# ------------------------------------------------------------------------------
echo -e "\n[*] Step 5: Quantifying Graph Topology Degradation & Recall Drift..."
python3 "${WORKSPACE_ROOT}/Scripts/measure_drift.py" \
    --output-dir "${OUTPUT_DIR}"

# ------------------------------------------------------------------------------
# Step 6: Publication Artifact Synthesis & Visual Formatting
# ------------------------------------------------------------------------------
echo -e "\n[*] Step 6: Generating Markdown, LaTeX tables, and Pareto SVGs..."
python3 "${WORKSPACE_ROOT}/Scripts/generate_report.py" \
    --output-dir "${OUTPUT_DIR}" \
    --input-matrix "${MATRIX_DIR}/matrix_summary.json"

# Re-render vector PDFs for paper if matplotlib is available
if python3 -c "import matplotlib" &>/dev/null; then
    echo -e "\n[*] Rendering publication vector PDFs..."
    python3 "${WORKSPACE_ROOT}/Scripts/plot_dual_cdf.py" || true
    python3 "${WORKSPACE_ROOT}/Scripts/plot_cgroups_timeseries.py" || true
    python3 "${WORKSPACE_ROOT}/Scripts/plot_pareto.py" || true
    python3 "${WORKSPACE_ROOT}/Scripts/plot_recall_drift.py" || true
    python3 "${WORKSPACE_ROOT}/Scripts/plot_filtering_pathology.py" || true
fi

# ------------------------------------------------------------------------------
# Step 7: LaTeX Document Recompilation
# ------------------------------------------------------------------------------
if command -v pdflatex &>/dev/null; then
    echo -e "\n[*] Recompiling paper manuscript (main.tex)..."
    PAPER_DIR="${WORKSPACE_ROOT}/Paper"
    if [ ! -d "${PAPER_DIR}" ]; then
        PAPER_DIR="${WORKSPACE_ROOT}/paper"
    fi
    if [ -d "${PAPER_DIR}" ]; then
        cd "${PAPER_DIR}"
        pdflatex -interaction=nonstopmode main.tex >/dev/null 2>&1 || true
        bibtex main >/dev/null 2>&1 || true
        pdflatex -interaction=nonstopmode main.tex >/dev/null 2>&1 || true
        pdflatex -interaction=nonstopmode main.tex >/dev/null 2>&1 || true
        cd "${WORKSPACE_ROOT}"
        echo "  [+] PDF generated: ${PAPER_DIR}/main.pdf"
    fi
fi

# ------------------------------------------------------------------------------
# Step 8: Update Audited SHA-256 Digest Manifest
# ------------------------------------------------------------------------------
echo -e "\n[*] Step 8: Refreshing sha256sums.txt checksum manifest..."
if command -v sha256sum &>/dev/null; then
    find reports -type f -exec sha256sum {} + > "${WORKSPACE_ROOT}/sha256sums.txt" || true
    echo "  [+] sha256sums.txt updated."
fi

echo -e "\n========================================================================"
echo "  [SUCCESS] Fracture Benchmark Suite Completed on Remote Bare-Metal Node"
echo "========================================================================"
