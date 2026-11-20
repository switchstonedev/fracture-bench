#!/usr/bin/env python3
"""
run_matrix.py - Automated multi-engine benchmark matrix runner for Fracture.
Adheres to ops/academic-artifact-evaluation three-tier hierarchy:
  - Level 1: dry_run (< 30s, zero load, connectivity and directory validation)
  - Level 2: sample (1-3 min, low QPS / small corpus pilot test)
  - Level 3: full (complete QPS sweep across target engines)

Standardized placeholder policy:
Any unexecuted or unreachable engine metric is recorded strictly as:
  {metric_originTech} (e.g. {achievedQps_milvus}, {totalP99_pgvector}, {recall10_lancedb})
Never emit fabricated or unmeasured numbers.
"""

import argparse
import json
import os
import socket
import subprocess
import sys
import time
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

TARGET_DEFAULTS = {
    "mock": {"host": None, "port": None, "endpoint": None},
    "qdrant": {"host": "127.0.0.1", "port": 6333, "endpoint": "http://127.0.0.1:6333"},
    "pgvector": {"host": "127.0.0.1", "port": 5433, "endpoint": "127.0.0.1:5433"},
    "milvus": {"host": "127.0.0.1", "port": 19530, "endpoint": "http://127.0.0.1:19530"},
    "lancedb": {"host": None, "port": None, "endpoint": "/tmp/fracture_lancedb"},
}

def is_port_open(host: Optional[str], port: Optional[int], timeout: float = 0.5) -> bool:
    if host is None or port is None:
        return True
    try:
        with socket.create_connection((host, port), timeout=timeout):
            return True
    except (socket.timeout, ConnectionRefusedError, OSError):
        return False

def find_binary(workspace_root: Path, custom_path: Optional[str] = None) -> Path:
    if custom_path:
        p = Path(custom_path).resolve()
        if p.is_file():
            return p
        raise FileNotFoundError(f"Specified binary not found: {custom_path}")

    if sys.platform == "win32":
        candidates = [
            workspace_root / "target" / "release" / "fracture.exe",
            workspace_root / "target" / "debug" / "fracture.exe",
            workspace_root / "target" / "release" / "fracture",
            workspace_root / "target" / "debug" / "fracture",
        ]
    else:
        candidates = [
            workspace_root / "target" / "release" / "fracture",
            workspace_root / "target" / "debug" / "fracture",
            workspace_root / "target" / "release" / "fracture.exe",
            workspace_root / "target" / "debug" / "fracture.exe",
        ]
    for c in candidates:
        if c.is_file():
            return c

    print("Building fracture debug binary...")
    subprocess.run(["cargo", "build", "-p", "fracture-bench"], cwd=workspace_root, check=True)
    for c in candidates:
        if c.is_file():
            return c
    raise FileNotFoundError("Could not find or build fracture executable")

def format_placeholder(metric: str, origin_tech: str) -> str:
    return f"{{{metric}_{origin_tech}}}"

def run_dry_run_cell(
    binary: Path,
    target: str,
    workload: str,
    qps: int,
    endpoint: Optional[str],
    out_json: Path,
) -> Dict[str, Any]:
    cmd = [
        str(binary),
        "run",
        "--target", target,
        "--workload", workload,
        "--qps", str(qps),
        "--duration-sec", "1",
        "--dry-run",
        "--output", str(out_json),
    ]
    if endpoint:
        cmd.extend(["--endpoint", endpoint])

    res = subprocess.run(cmd, capture_output=True, text=True, check=False)
    if res.returncode != 0:
        print(f"  [Dry Run Notice] {target} dry-run returned exit code {res.returncode}: {res.stderr.strip()}")

    return {
        "target": target,
        "workload": workload,
        "target_qps": qps,
        "achieved_qps": None,
        "service_p50_us": None,
        "service_p99_us": None,
        "total_p50_us": None,
        "total_p99_us": None,
        "unserved_arrivals": None,
        "recall_at_10": None,
        "status": "dry_run",
    }

def run_sample_cell(
    binary: Path,
    target: str,
    workload: str,
    qps: int,
    endpoint: Optional[str],
    out_json: Path,
    workspace_root: Path,
) -> Dict[str, Any]:
    wal_dir = workspace_root / "runs"
    wal_dir.mkdir(parents=True, exist_ok=True)
    sample_wal = wal_dir / f"sample_{target}.wal"

    # Step 1: Small seed
    seed_cmd = [
        str(binary),
        "seed",
        "--target", target,
        "--count", "200",
        "--batch-size", "100",
        "--wal-path", str(sample_wal),
    ]
    if endpoint:
        seed_cmd.extend(["--endpoint", endpoint])
    subprocess.run(seed_cmd, capture_output=True, text=True, check=False)

    # Step 2: 1-second benchmark
    bench_cmd = [
        str(binary),
        "run",
        "--target", target,
        "--workload", workload,
        "--qps", str(qps),
        "--duration-sec", "1",
        "--output", str(out_json),
    ]
    if endpoint:
        bench_cmd.extend(["--endpoint", endpoint])

    bench_res = subprocess.run(bench_cmd, capture_output=True, text=True, check=False)

    # Step 3: Fast recall verification
    recall_val: Optional[float] = None
    if sample_wal.is_file():
        verify_json = out_json.with_suffix(".recall.json")
        verify_cmd = [
            str(binary),
            "verify",
            "--wal-path", str(sample_wal),
            "--cutoff-lsn", "100",
            "--k", "10",
            "--target", target,
            "--output", str(verify_json),
        ]
        if endpoint:
            verify_cmd.extend(["--endpoint", endpoint])
        v_res = subprocess.run(verify_cmd, capture_output=True, text=True, check=False)
        if v_res.returncode == 0 and verify_json.is_file():
            try:
                v_data = json.loads(verify_json.read_text(encoding="utf-8"))
                recall_val = float(v_data.get("mean_recall_at_k", 0.0))
            except Exception:
                pass

    if out_json.is_file():
        try:
            b_data = json.loads(out_json.read_text(encoding="utf-8"))
            rpt = b_data.get("report", {})
            return {
                "target": target,
                "workload": workload,
                "target_qps": qps,
                "achieved_qps": rpt.get("achieved_qps"),
                "service_p50_us": rpt.get("service_time_p50_us"),
                "service_p99_us": rpt.get("service_time_p99_us"),
                "total_p50_us": rpt.get("total_time_p50_us"),
                "total_p99_us": rpt.get("total_time_p99_us"),
                "unserved_arrivals": rpt.get("unserved_arrivals"),
                "recall_at_10": recall_val,
                "status": "executed",
            }
        except Exception:
            pass

    return {
        "target": target,
        "workload": workload,
        "target_qps": qps,
        "achieved_qps": None,
        "service_p50_us": None,
        "service_p99_us": None,
        "total_p50_us": None,
        "total_p99_us": None,
        "unserved_arrivals": None,
        "recall_at_10": None,
        "status": "error",
    }

def main():
    parser = argparse.ArgumentParser(description="Automated Fracture Benchmark Matrix Runner")
    parser.add_argument(
        "--tier",
        type=str,
        default="dry_run",
        choices=["dry_run", "dry", "sample", "small", "full", "full_benchmark"],
        help="Execution tier: dry_run (< 30s), sample (1-3m pilot), or full",
    )
    parser.add_argument(
        "--output-dir",
        type=str,
        default="results/matrix",
        help="Directory to store matrix JSON reports",
    )
    parser.add_argument("--binary", type=str, default=None, help="Path to fracture executable")
    parser.add_argument(
        "--targets",
        type=str,
        default="mock,qdrant,pgvector,milvus,lancedb",
        help="Comma-separated list of target engines",
    )
    parser.add_argument(
        "--workloads",
        type=str,
        default="churn-70-20-10,read-heavy-90-10,write-heavy-50-50",
        help="Comma-separated list of workloads",
    )
    args = parser.parse_args()

    norm_tier = args.tier.lower().replace("-", "_")
    if norm_tier in ["dry", "dry_run"]:
        tier = "dry_run"
    elif norm_tier in ["small", "sample"]:
        tier = "sample"
    else:
        tier = "full"

    workspace_root = Path(__file__).resolve().parent.parent
    binary_path = find_binary(workspace_root, args.binary)
    out_dir = (workspace_root / args.output_dir).resolve()
    out_dir.mkdir(parents=True, exist_ok=True)

    targets = [t.strip() for t in args.targets.split(",") if t.strip()]
    workloads = [w.strip() for w in args.workloads.split(",") if w.strip()]

    if tier == "dry_run":
        qps_levels = [1000]
    elif tier == "sample":
        qps_levels = [100]
    else:
        qps_levels = [1000, 2500, 5000, 10000]

    print("===========================================================")
    print("  Fracture: Automated Matrix Benchmark Runner             ")
    print("===========================================================")
    print(f"Execution Tier : {tier.upper()}")
    print(f"Target Engines : {', '.join(targets)}")
    print(f"Workloads      : {', '.join(workloads)}")
    print(f"QPS Levels     : {qps_levels}")
    print(f"Binary Path    : {binary_path}")
    print(f"Output Path    : {out_dir}")
    print("===========================================================")

    matrix_results: List[Dict[str, Any]] = []

    for target in targets:
        defaults = TARGET_DEFAULTS.get(target, {"host": None, "port": None, "endpoint": None})
        endpoint = defaults["endpoint"]
        host = defaults["host"]
        port = defaults["port"]

        accessible = is_port_open(host, port)
        if not accessible:
            print(f"[*] Engine '{target}' is offline/unreachable on {host}:{port}. Metric placeholders will be preserved.")

        for workload in workloads:
            for qps in qps_levels:
                out_json = out_dir / f"{target}_{workload}_{qps}qps.json"
                print(f"--> Running cell: target={target}, workload={workload}, qps={qps} [Tier: {tier}]")

                if tier == "dry_run":
                    cell = run_dry_run_cell(binary_path, target, workload, qps, endpoint, out_json)
                elif tier == "sample":
                    if not accessible and target != "mock":
                        cell = {
                            "target": target,
                            "workload": workload,
                            "target_qps": qps,
                            "achieved_qps": None,
                            "service_p50_us": None,
                            "service_p99_us": None,
                            "total_p50_us": None,
                            "total_p99_us": None,
                            "unserved_arrivals": None,
                            "recall_at_10": None,
                            "status": "unreachable",
                        }
                    else:
                        cell = run_sample_cell(binary_path, target, workload, qps, endpoint, out_json, workspace_root)
                else:
                    # Full execution
                    if not accessible and target != "mock":
                        cell = {
                            "target": target,
                            "workload": workload,
                            "target_qps": qps,
                            "achieved_qps": None,
                            "service_p50_us": None,
                            "service_p99_us": None,
                            "total_p50_us": None,
                            "total_p99_us": None,
                            "unserved_arrivals": None,
                            "recall_at_10": None,
                            "status": "unreachable",
                        }
                    else:
                        cell = run_sample_cell(binary_path, target, workload, qps, endpoint, out_json, workspace_root)

                matrix_results.append(cell)

    summary_file = out_dir / "matrix_summary.json"
    summary_data = {
        "tier": tier,
        "timestamp_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "targets": targets,
        "workloads": workloads,
        "qps_levels": qps_levels,
        "cells": matrix_results,
    }

    summary_file.write_text(json.dumps(summary_data, indent=2), encoding="utf-8")
    print("===========================================================")
    print(f"Matrix run complete! Summary written to: {summary_file}")
    print("===========================================================")

if __name__ == "__main__":
    main()
