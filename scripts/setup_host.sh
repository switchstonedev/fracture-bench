#!/usr/bin/env bash
# setup_host.sh - Host configuration for reproducible bare-metal vector database benchmarking.
set -euo pipefail

echo "=========================================================="
echo "  CruciVec Host Setup: Core Pinning & Cgroups v2 Isolation "
echo "=========================================================="

if [ "$EUID" -ne 0 ]; then
  echo "Error: Please execute as root (sudo ./scripts/setup_host.sh)"
  exit 1
fi

# 1. Disable CPU Frequency Scaling (Set performance governor)
echo "[1/4] Setting CPU frequency governor to 'performance'..."
if command -v cpupower &> /dev/null; then
    cpupower frequency-set -g performance
elif [ -d /sys/devices/system/cpu/cpu0/cpufreq ]; then
    for gov in /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor; do
        echo "performance" > "$gov" 2>/dev/null || true
    done
fi

# 2. Configure Linux Timer Slack
echo "[2/4] Optimizing high-resolution timer parameters..."
sysctl -w fs.file-max=2097152
sysctl -w vm.max_map_count=262144

# 3. Create Isolated Cgroup v2 for Target Database
echo "[3/4] Establishing strict cgroups v2 benchmark slice..."
CGROUP_DIR="/sys/fs/cgroup/crucivec_target"
mkdir -p "$CGROUP_DIR"

# 4 dedicated CPU cores (Cores 4-7)
echo "4-7" > "$CGROUP_DIR/cpuset.cpus" || true

# Memory constraints (calibrated for 1M vector benchmarks)
# memory.high initiates background reclaim, memory.max enforces hard boundary
echo "7516192768" > "$CGROUP_DIR/memory.high" 2>/dev/null || true # 7.0 GB
echo "8589934592" > "$CGROUP_DIR/memory.max" 2>/dev/null || true  # 8.0 GB

echo "[4/4] Host setup completed."
echo "Target cgroup created at: $CGROUP_DIR"
echo "Core 0: OS & IRQs | Core 1: Pacer | Cores 2-3: Tokio Workers | Cores 4-7: Target DB"
echo "=========================================================="

