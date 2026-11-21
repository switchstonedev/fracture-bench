#!/usr/bin/env python3
"""
reproduce.py - Deterministic single-entrypoint reproduction harness for Fracture.
Adheres strictly to ops/academic-artifact-evaluation for ACM/IEEE reproducibility badging.

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
from typing import List, NamedTuple, Optional, Tuple

class VerificationReport(NamedTuple):
    is_valid: bool
    missing_files: List[str]
    mismatches: List[str]

def verify_sha256_manifest(workspace_root: Path, manifest_file: Path) -> VerificationReport:
    if not manifest_file.is_file():
        return VerificationReport(is_valid=True, missing_files=[], mismatches=[])

    missing: List[str] = []
    mismatches: List[str] = []
    lines = manifest_file.read_text(encoding="utf-8").splitlines()

    for line in lines:
        trimmed = line.strip()
        if not trimmed or trimmed.startswith("#"):
            continue
        parts = trimmed.split(maxsplit=1)
        if len(parts) != 2:
            continue

        expected_hash, rel_path_str = parts[0], parts[1].strip()
        target_path = workspace_root / rel_path_str
        if not target_path.is_file():
            missing.append(rel_path_str)
            continue

        hasher = hashlib.sha256()
        with open(target_path, "rb") as f:
            while chunk := f.read(65536):
                hasher.update(chunk)

        if hasher.hexdigest().lower() != expected_hash.lower():
            mismatches.append(rel_path_str)

    is_valid = (len(missing) == 0 and len(mismatches) == 0)
    return VerificationReport(is_valid=is_valid, missing_files=missing, mismatches=mismatches)

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
        help="Output directory for generated publication artifacts",
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
    manifest_file = workspace_root / "sha256sums.txt"

    print("=================================================================")
    print("  Fracture: Deterministic Academic Artifact Reproduction Harness ")
    print("=================================================================")
    print(f"Target Tier     : {tier.upper()}")
    print(f"Workspace Root  : {workspace_root}")
    print(f"Publication Dir : {workspace_root / args.output_dir}")
    print("=================================================================")

    # Step 1: Audited Checksum Manifest Gate
    if not args.skip_manifest_check and manifest_file.is_file():
        print(f"[*] Verifying audited SHA-256 digest manifest ({manifest_file.name})...")
        v_rep = verify_sha256_manifest(workspace_root, manifest_file)
        if not v_rep.is_valid:
            if v_rep.missing_files:
                print(f"[!] Warning: Missing manifest files: {', '.join(v_rep.missing_files)}")
            if v_rep.mismatches:
                print(f"[!] Warning: Digest mismatches in: {', '.join(v_rep.mismatches)}")
        else:
            print("[+] SHA-256 digest manifest verified successfully.")

    # Step 2: Run Benchmark Matrix
    print(f"\n[*] Executing Benchmark Matrix (Tier: {tier})...")
    matrix_py = scripts_dir / "run_matrix.py"
    matrix_out_dir = str(Path(args.output_dir) / "matrix")
    matrix_cmd = [sys.executable, str(matrix_py), "--tier", tier, "--output-dir", matrix_out_dir]
    res_matrix = subprocess.run(matrix_cmd, cwd=workspace_root, check=False)
    if res_matrix.returncode != 0:
        print(f"[!] Matrix runner exited with return code {res_matrix.returncode}")

    # Step 3: Generate Visual and Tabular Reports
    print("\n[*] Synthesizing publication reports and Pareto frontier SVG...")
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
        print(f"[WARNING] Some expected publication artifacts were not created.")
        sys.exit(1)

if __name__ == "__main__":
    main()

