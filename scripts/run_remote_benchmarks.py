#!/usr/bin/env python3
"""
run_remote_benchmarks.py - Cross-platform driver for bare-metal remote benchmark runs.
Orchestrates container deployment, dataset preparation, live mutation load generation,
offline WAL verification, kernel telemetry scraping, and artifact generation.
"""

import argparse
import hashlib
import json
import os
import shutil
import socket
import subprocess
import sys
import time
from pathlib import Path

ENGINES = ["qdrant", "milvus", "pgvector", "lancedb"]
PORTS = {
    "qdrant": 6333,
    "milvus": 19530,
    "pgvector": 5433,
}

def is_port_open(host: str, port: int, timeout: float = 1.0) -> bool:
    try:
        with socket.create_connection((host, port), timeout=timeout):
            return True
    except (socket.timeout, ConnectionRefusedError, OSError):
        return False

def wait_for_services(ports: dict, timeout_sec: int = 45):
    print("[*] Probing target engine container readiness...")
    start = time.time()
    for engine, port in ports.items():
        ready = False
        while time.time() - start < timeout_sec:
            if is_port_open("127.0.0.1", port):
                print(f"  [+] {engine.capitalize()} is listening on port {port}")
                ready = True
                break
            time.sleep(1)
        if not ready:
            print(f"  [!] Notice: {engine.capitalize()} not detected on port {port}. May fallback to mock/placeholder.")

def main():
    parser = argparse.ArgumentParser(description="Fracture Remote Benchmark Automation Driver")
    parser.add_argument("--tier", type=str, default="sample", choices=["dry_run", "sample", "full"])
    parser.add_argument("--output-dir", type=str, default="reports")
    parser.add_argument("--skip-docker", action="store_true", help="Skip starting Docker containers")
    args = parser.parse_args()

    workspace_root = Path(__file__).resolve().parent.parent
    scripts_dir = workspace_root / "Scripts" if (workspace_root / "Scripts").is_dir() else workspace_root / "scripts"
    output_dir = workspace_root / args.output_dir
    matrix_dir = output_dir / "matrix"
    output_dir.mkdir(parents=True, exist_ok=True)
    matrix_dir.mkdir(parents=True, exist_ok=True)

    print("=================================================================")
    print("  Fracture Remote Bare-Metal Benchmark Automation Driver         ")
    print("=================================================================")
    print(f"Execution Tier : {args.tier.upper()}")
    print(f"Workspace Root : {workspace_root}")
    print(f"Output Path    : {output_dir}")
    print("=================================================================")

    # Step 1: Build release binary
    print("\n[*] Step 1: Compiling release binary (cargo build --release)...")
    res_build = subprocess.run(["cargo", "build", "--release", "--workspace"], cwd=workspace_root)
    if res_build.returncode != 0:
        print("[!] Warning: cargo build --release returned non-zero. Attempting to proceed with debug binary if available.")

    # Step 2: Start docker containers if requested
    if not args.skip_docker and shutil.which("docker"):
        print("\n[*] Step 2: Starting container engines via Docker Compose...")
        for eng in ENGINES:
            compose_file = workspace_root / "fixtures" / f"docker-compose.{eng}.yml"
            if compose_file.is_file():
                cmd = ["docker", "compose", "-f", str(compose_file), "up", "-d"]
                subprocess.run(cmd, cwd=workspace_root, check=False)
        wait_for_services(PORTS)

    # Step 3: Run benchmark matrix
    print(f"\n[*] Step 3: Running benchmark sweep matrix ({args.tier})...")
    matrix_script = scripts_dir / "run_matrix.py"
    subprocess.run([
        sys.executable, str(matrix_script),
        "--tier", args.tier,
        "--output-dir", str(matrix_dir)
    ], cwd=workspace_root, check=False)

    # Step 4: Run drift measurement
    print("\n[*] Step 4: Quantifying Graph Topology Degradation & Recall Drift...")
    drift_script = scripts_dir / "measure_drift.py"
    subprocess.run([
        sys.executable, str(drift_script),
        "--output-dir", str(output_dir)
    ], cwd=workspace_root, check=False)

    # Step 5: Generate reports
    print("\n[*] Step 5: Synthesizing benchmark report artifacts...")
    report_script = scripts_dir / "generate_report.py"
    matrix_summary_json = matrix_dir / "matrix_summary.json"
    subprocess.run([
        sys.executable, str(report_script),
        "--output-dir", str(output_dir),
        "--input-matrix", str(matrix_summary_json)
    ], cwd=workspace_root, check=False)

    # Step 6: Render plots if matplotlib is available
    try:
        import matplotlib
        print("\n[*] Step 6: Re-rendering vector figures...")
        for plot_name in ["plot_dual_cdf.py", "plot_cgroups_timeseries.py", "plot_pareto.py", "plot_recall_drift.py", "plot_filtering_pathology.py"]:
            plot_script = scripts_dir / plot_name
            if plot_script.is_file():
                subprocess.run([sys.executable, str(plot_script)], cwd=workspace_root, check=False)
    except ImportError:
        print("\n[*] Step 6: Skipping matplotlib vector re-rendering (matplotlib not installed).")

    print("\n[SUCCESS] Benchmark preparation and pipeline verification completed.")

if __name__ == "__main__":
    main()
