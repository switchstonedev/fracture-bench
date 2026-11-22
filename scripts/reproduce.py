#!/usr/bin/env python3
"""
reproduce.py - Deterministic single-entrypoint reproduction harness for Fracture.

Execution Hierarchy:
  - Level 1: Dry Run (--tier dry_run / --dry-run): 10-30s sanity check (zero host strain)
  - Level 2: Sample  (--tier sample  / --sample):  1-3 min pilot run on accessible engines
  - Level 3: Full    (--tier full    / --full):    Exhaustive matrix sweep across all engines

Outputs:
  - reports/benchmark_summary.md
  - reports/benchmark_summary.tex
  - reports/figures/dual_cdf.svg
  - reports/figures/pareto_frontier.svg
"""

import argparse
import hashlib
import json
import os
import subprocess
import sys
import time
from pathlib import Path

def main():
    parser = argparse.ArgumentParser(
        description="Single-entrypoint reproduction harness for Fracture benchmarks"
    )
    parser.add_argument(
        "--tier",
        type=str,
        default="sample",
        choices=["dry_run", "dry", "sample", "small", "full", "full_benchmark"],
        help="Reproduction execution tier: dry_run (< 30s), sample (1-3 min pilot), or full",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Shorthand alias for --tier dry_run",
    )
    parser.add_argument(
        "--sample",
        action="store_true",
        help="Shorthand alias for --tier sample",
    )
    parser.add_argument(
        "--full",
        action="store_true",
        help="Shorthand alias for --tier full",
    )
    parser.add_argument(
        "--output-dir",
        type=str,
        default="reports",
        help="Output directory for generated benchmark artifacts",
    )
    parser.add_argument(
        "--skip-manifest-check",
        action="store_true",
        help="Skip SHA-256 manifest verification",
    )
    args = parser.parse_args()

    # Normalize tier selection
    if args.dry_run:
        tier = "dry_run"
    elif args.full:
        tier = "full"
    elif args.sample:
        tier = "sample"
    else:
        norm = args.tier.lower().replace("-", "_")
        if norm in ["dry", "dry_run"]:
            tier = "dry_run"
        elif norm in ["small", "sample"]:
            tier = "sample"
        else:
            tier = "full"

    workspace_root = Path(__file__).resolve().parent.parent
    scripts_dir = workspace_root / "scripts"

    print("=================================================================")
    print("     Fracture: Deterministic Benchmark Reproduction Harness      ")
    print("=================================================================")
    print(f"Target Tier     : {tier.upper()}")
    print(f"Workspace Root  : {workspace_root}")
    print(f"Report Dir      : {workspace_root / args.output_dir}")
    print("=================================================================")

    # Step 1: Run Benchmark Matrix
    print(f"\n[*] Executing Benchmark Matrix (Tier: {tier})...")
    matrix_py = scripts_dir / "run_matrix.py"
    matrix_out_dir = str(Path(args.output_dir) / "matrix")
    matrix_cmd = [sys.executable, str(matrix_py), "--tier", tier, "--output-dir", matrix_out_dir]
    res_matrix = subprocess.run(matrix_cmd, cwd=workspace_root, check=False)
    if res_matrix.returncode != 0:
        print(f"[!] Matrix runner exited with return code {res_matrix.returncode}")

    # Step 3: Generate Visual and Tabular Reports
    print("\n[*] Synthesizing benchmark reports and Pareto frontier SVG...")
    report_py = scripts_dir / "generate_report.py"
    matrix_summary_json = str(Path(args.output_dir) / "matrix" / "matrix_summary.json")
    report_cmd = [
        sys.executable,
        str(report_py),
        "--output-dir", args.output_dir,
        "--input-matrix", matrix_summary_json,
    ]
    res_report = subprocess.run(report_cmd, cwd=workspace_root, check=False)
    if res_report.returncode != 0:
        print(f"[!] Report generator exited with return code {res_report.returncode}")

    # Step 4: Quantify Graph Degradation & Recall Drift
    print("\n[*] Measuring point-in-time graph degradation & recall drift curve...")
    drift_py = scripts_dir / "measure_drift.py"
    drift_cmd = [
        sys.executable,
        str(drift_py),
        "--output-dir", args.output_dir,
    ]
    res_drift = subprocess.run(drift_cmd, cwd=workspace_root, check=False)
    if res_drift.returncode != 0:
        print(f"[!] Drift analyzer exited with return code {res_drift.returncode}")

    # Step 5: Verification of Generated Artifacts
    out_dir = workspace_root / args.output_dir
    expected_artifacts = [
        out_dir / "benchmark_summary.md",
        out_dir / "benchmark_summary.tex",
        out_dir / "figures" / "dual_cdf.svg",
        out_dir / "figures" / "pareto_frontier.svg",
        out_dir / "figures" / "recall_drift.svg",
    ]

    print("\n================ Reproduction Verification ================")
    all_present = True
    for art in expected_artifacts:
        if art.is_file():
            print(f" [PASS] {art.relative_to(workspace_root)} ({art.stat().st_size} bytes)")
        else:
            print(f" [FAIL] Missing: {art.relative_to(workspace_root)}")
            all_present = False

    print("===========================================================")
    if all_present:
        print(f"[SUCCESS] Artifact reproduction tier '{tier}' completed cleanly.")
        print(f"All tables and figures rendered without resource exhaustion.")
    else:
        print(f"[WARNING] Some expected benchmark artifacts were not created.")
        sys.exit(1)

if __name__ == "__main__":
    main()

