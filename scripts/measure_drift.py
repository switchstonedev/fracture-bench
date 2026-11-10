#!/usr/bin/env python3
"""
measure_drift.py - Quantify Graph Degradation & Recall Drift over Mutation Progression.
Replays binary WAL at sequential LSN boundaries to measure how dead node tombstones
and HNSW graph fragmentation erode Recall@K over time.
Outputs:
  - reports/figures/recall_drift.svg
  - reports/recall_drift_summary.json
"""

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

def find_binary(workspace_root: Path) -> Path:
    candidates = [
        workspace_root / "target" / "release" / "crucivec.exe",
        workspace_root / "target" / "release" / "crucivec",
        workspace_root / "target" / "debug" / "crucivec.exe",
        workspace_root / "target" / "debug" / "crucivec",
    ]
    for c in candidates:
        if c.is_file():
            return c
    raise FileNotFoundError("crucivec executable not found")

def generate_svg_recall_drift(
    checkpoints: List[Dict[str, Any]],
    output_path: Path,
):
    """
    Renders an SVG curve plotting Mean Recall@10 (Y-axis) vs. LSN / Tombstone Accumulation (X-axis).
    """
    svg_width = 720
    svg_height = 420
    margin = {"top": 45, "right": 50, "bottom": 65, "left": 75}
    plot_width = svg_width - margin["left"] - margin["right"]
    plot_height = svg_height - margin["top"] - margin["bottom"]

    if not checkpoints:
        return

    max_lsn = max(max(c["cutoff_lsn"] for c in checkpoints), 1)

    def x_coord(lsn: int) -> float:
        ratio = max(0.0, min(1.0, lsn / max_lsn))
        return margin["left"] + ratio * plot_width

    def y_coord(recall: float) -> float:
        r = max(0.0, min(1.0, recall))
        return margin["top"] + (1.0 - r) * plot_height

    svg_lines = [
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {svg_width} {svg_height}" width="{svg_width}" height="{svg_height}">',
        '<style>',
        '  .title { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; font-size: 14px; font-weight: bold; fill: #111; }',
        '  .axis { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; font-size: 11px; fill: #555; }',
        '  .grid { stroke: #e0e0e0; stroke-dasharray: 3,3; }',
        '  .legend { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; font-size: 11px; }',
        '  .line { fill: none; stroke: #d95f02; stroke-width: 2.5; }',
        '  .point { fill: #d95f02; stroke: #fff; stroke-width: 1.5; }',
        '</style>',
        f'<rect width="{svg_width}" height="{svg_height}" fill="#ffffff" />',
        f'<text x="{margin["left"]}" y="25" class="title">Graph Fragmentation: Point-in-Time Recall@10 vs. Mutation LSN Progression</text>',
    ]

    # Horizontal grid lines (Recall: 0.0 to 1.0)
    for r in [0.0, 0.2, 0.4, 0.6, 0.8, 1.0]:
        y = y_coord(r)
        svg_lines.append(f'<line x1="{margin["left"]}" y1="{y}" x2="{margin["left"] + plot_width}" y2="{y}" class="grid" />')
        svg_lines.append(f'<text x="{margin["left"] - 10}" y="{y + 4}" text-anchor="end" class="axis">{r:.1f}</text>')

    # Vertical grid lines (LSN checkpoints)
    for c in checkpoints:
        x = x_coord(c["cutoff_lsn"])
        svg_lines.append(f'<line x1="{x}" y1="{margin["top"]}" x2="{x}" y2="{margin["top"] + plot_height}" class="grid" />')
        svg_lines.append(f'<text x="{x}" y="{margin["top"] + plot_height + 16}" text-anchor="middle" class="axis">LSN {c["cutoff_lsn"]}</text>')
        t_count = c.get("tombstone_count", 0)
        svg_lines.append(f'<text x="{x}" y="{margin["top"] + plot_height + 30}" text-anchor="middle" font-size="9" fill="#888">({t_count} dead)</text>')

    # Polyline points
    poly_points = [f"{x_coord(c['cutoff_lsn']):.1f},{y_coord(c['mean_recall_at_10']):.1f}" for c in checkpoints]
    svg_lines.append(f'<polyline points="{" ".join(poly_points)}" class="line" />')

    for c in checkpoints:
        cx = x_coord(c["cutoff_lsn"])
        cy = y_coord(c["mean_recall_at_10"])
        svg_lines.append(f'<circle cx="{cx:.1f}" cy="{cy:.1f}" r="5" class="point" />')
        val_label = f"{c['mean_recall_at_10']:.3f}"
        svg_lines.append(f'<text x="{cx:.1f}" y="{cy - 8:.1f}" text-anchor="middle" font-size="10" font-weight="bold" fill="#333">{val_label}</text>')

    svg_lines.append(f'<text x="{margin["left"] + plot_width / 2}" y="{svg_height - 10}" text-anchor="middle" class="axis">Monotonic Mutation Sequence & Accumulated Dead-Node Tombstones</text>')
    svg_lines.append(f'<text transform="rotate(-90)" x="-{margin["top"] + plot_height / 2}" y="20" text-anchor="middle" class="axis">Mean Recall@10 (SIMD Ground Truth)</text>')

    svg_lines.append('</svg>')

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text("\n".join(svg_lines), encoding="utf-8")
    print(f"Generated Recall Drift SVG -> {output_path}")

def main():
    parser = argparse.ArgumentParser(description="Evaluate Point-in-Time Recall Drift Across LSN Checkpoints")
    parser.add_argument("--wal-path", type=str, default=None, help="Path to binary WAL file")
    parser.add_argument("--target", type=str, default="mock", help="Target adapter to verify against WAL")
    parser.add_argument("--endpoint", type=str, default=None, help="Optional endpoint for target database")
    parser.add_argument("--checkpoints", type=int, default=5, help="Number of LSN checkpoints to sample")
    parser.add_argument("--output-dir", type=str, default="reports", help="Output directory")
    args = parser.parse_args()

    workspace_root = Path(__file__).resolve().parent.parent
    binary = find_binary(workspace_root)
    out_dir = (workspace_root / args.output_dir).resolve()
    fig_dir = out_dir / "figures"
    fig_dir.mkdir(parents=True, exist_ok=True)

    # Locate or generate WAL file
    wal_file = None
    if args.wal_path:
        p = Path(args.wal_path)
        if p.is_file():
            wal_file = p

    if wal_file is None:
        runs_dir = workspace_root / "runs"
        sample_wals = sorted(runs_dir.glob("*.wal"))
        if sample_wals:
            wal_file = sample_wals[0]

    # If no WAL exists yet, generate a fast synthetic churn WAL with 200 operations
    if wal_file is None or not wal_file.is_file():
        wal_file = workspace_root / "runs" / "churn_drift_eval.wal"
        wal_file.parent.mkdir(parents=True, exist_ok=True)
        print(f"Generating synthetic mutation WAL -> {wal_file}...")
        seed_cmd = [
            str(binary),
            "seed",
            "--target", "mock",
            "--count", "300",
            "--batch-size", "50",
            "--wal-path", str(wal_file),
        ]
        subprocess.run(seed_cmd, cwd=workspace_root, check=True, capture_output=True)

    print("===========================================================")
    print("  CruciVec: Measuring Graph Fragmentation & Recall Drift   ")
    print("===========================================================")
    print(f"WAL File       : {wal_file}")
    print(f"Target Adapter : {args.target}")
    print(f"Checkpoints    : {args.checkpoints}")
    print("===========================================================")

    # Intermediate checkpoint directory in results/drift
    interm_dir = workspace_root / "results" / "drift"
    interm_dir.mkdir(parents=True, exist_ok=True)

    # Determine cutoff LSN milestones
    lsn_steps = [60, 120, 180, 240, 300]

    drift_records: List[Dict[str, Any]] = []

    for lsn in lsn_steps:
        verify_out = interm_dir / f"drift_checkpoint_lsn_{lsn}.json"
        cmd = [
            str(binary),
            "verify",
            "--wal-path", str(wal_file),
            "--cutoff-lsn", str(lsn),
            "--k", "10",
            "--target", args.target,
            "--output", str(verify_out),
        ]
        if args.endpoint:
            cmd.extend(["--endpoint", args.endpoint])

        res = subprocess.run(cmd, cwd=workspace_root, capture_output=True, text=True, check=False)
        if res.returncode == 0 and verify_out.is_file():
            try:
                rep = json.loads(verify_out.read_text(encoding="utf-8"))
                rec = {
                    "cutoff_lsn": lsn,
                    "active_vectors": rep.get("active_vectors_count", 0),
                    "tombstone_count": rep.get("tombstone_count", 0),
                    "mean_recall_at_10": rep.get("mean_recall_at_k", 1.0),
                }
                drift_records.append(rec)
                print(f"  Checkpoint LSN {lsn:4d} | Active: {rec['active_vectors']:3d} | Tombstones: {rec['tombstone_count']:3d} | Recall@10: {rec['mean_recall_at_10']:.4f}")
            except Exception as e:
                print(f"  Failed parsing checkpoint {lsn}: {e}")
        else:
            print(f"  Notice: Verification failed for LSN {lsn}: {res.stderr.strip()}")

    if drift_records:
        generate_svg_recall_drift(drift_records, fig_dir / "recall_drift.svg")
        summary_file = out_dir / "recall_drift_summary.json"
        summary_file.write_text(json.dumps(drift_records, indent=2), encoding="utf-8")
        print(f"Saved Recall Drift Summary -> {summary_file}")

if __name__ == "__main__":
    main()
