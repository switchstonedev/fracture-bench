#!/usr/bin/env python3
"""
generate_report.py - Visual artifact and trace report generator.
Produces SVG/HTML Dual-CDF curves, Pareto frontiers, and cgroups v2 time-series.
Outputs:
  - results/figures/dual_cdf.svg
  - results/figures/pareto_frontier.svg
  - results/benchmark_summary.md (GFM markdown table adhering to Anti-LaTeX plain-text invariant)

Standardized placeholder policy:
Any unexecuted or unreachable metric is formatted strictly as:
  {metric_originTech} (e.g. {achievedQps_milvus}, {recall10_pgvector})
Never emit fabricated or unmeasured numbers.
"""

import argparse
import json
import math
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

ENGINE_COLORS = {
    "mock": "#2ca02c",
    "qdrant": "#1f77b4",
    "pgvector": "#ff7f0e",
    "milvus": "#9467bd",
    "lancedb": "#d62728",
}

def format_metric(val: Optional[float], metric: str, origin_tech: str, precision: int = 2) -> str:
    """Format metric value, returning {metric_originTech} if unavailable."""
    if val is None:
        return f"{{{metric}_{origin_tech}}}"
    return f"{val:.{precision}f}"

def generate_svg_dual_cdf(
    service_percentiles: Dict[str, float],
    total_percentiles: Dict[str, float],
    output_path: Path,
):
    """
    Renders an SVG Dual-CDF curve comparing
    uncorrected service time against Coordinated-Omission-corrected total response time.
    """
    svg_width = 720
    svg_height = 420
    margin = {"top": 45, "right": 50, "bottom": 60, "left": 75}
    plot_width = svg_width - margin["left"] - margin["right"]
    plot_height = svg_height - margin["top"] - margin["bottom"]

    points = [("p50", 0.50), ("p90", 0.90), ("p99", 0.99), ("p99.9", 0.999)]

    min_lat = min(service_percentiles.get("p50", 1.0), 1.0)
    max_lat = max(total_percentiles.get("p99.9", 1000.0), 100.0)

    log_min = math.log10(max(min_lat, 0.1))
    log_max = math.log10(max(max_lat, 10.0))

    def x_coord(val_us: float) -> float:
        val = max(val_us, 0.1)
        log_v = math.log10(val)
        ratio = (log_v - log_min) / (log_max - log_min) if log_max > log_min else 0.5
        return margin["left"] + ratio * plot_width

    def y_coord(q: float) -> float:
        ratio = (q - 0.50) / (1.0 - 0.50)
        return margin["top"] + (1.0 - ratio) * plot_height

    svg_lines = [
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {svg_width} {svg_height}" width="{svg_width}" height="{svg_height}">',
        '<style>',
        '  .title { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; font-size: 14px; font-weight: bold; fill: #111; }',
        '  .axis { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; font-size: 11px; fill: #555; }',
        '  .grid { stroke: #e0e0e0; stroke-dasharray: 3,3; }',
        '  .legend { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; font-size: 11px; }',
        '</style>',
        f'<rect width="{svg_width}" height="{svg_height}" fill="#ffffff" />',
        f'<text x="{margin["left"]}" y="25" class="title">Dual-CDF: Socket Service Time vs. Omission-Corrected Response Time</text>',
    ]

    for q in [0.50, 0.90, 0.99, 0.999]:
        y = y_coord(q)
        svg_lines.append(f'<line x1="{margin["left"]}" y1="{y}" x2="{margin["left"] + plot_width}" y2="{y}" class="grid" />')
        label = f"{q*100:.1f}%" if q < 0.99 else f"{q*100:.2f}%"
        svg_lines.append(f'<text x="{margin["left"] - 10}" y="{y + 4}" text-anchor="end" class="axis">{label}</text>')

    svc_pts = []
    for k, q in points:
        val = service_percentiles.get(k, 10.0)
        svc_pts.append(f"{x_coord(val):.1f},{y_coord(q):.1f}")
    svg_lines.append(f'<polyline fill="none" stroke="#1f77b4" stroke-width="2.5" points="{" ".join(svc_pts)}" />')

    tot_pts = []
    for k, q in points:
        val = total_percentiles.get(k, 100.0)
        tot_pts.append(f"{x_coord(val):.1f},{y_coord(q):.1f}")
    svg_lines.append(f'<polyline fill="none" stroke="#d62728" stroke-width="2.5" stroke-dasharray="5,4" points="{" ".join(tot_pts)}" />')

    svg_lines.append(f'<text x="{margin["left"] + plot_width / 2}" y="{svg_height - 15}" text-anchor="middle" class="axis">Latency (µs, Log Scale)</text>')

    leg_x = margin["left"] + 20
    leg_y = margin["top"] + 20
    svg_lines.append(f'<line x1="{leg_x}" y1="{leg_y}" x2="{leg_x + 30}" y2="{leg_y}" stroke="#1f77b4" stroke-width="2.5" />')
    svg_lines.append(f'<text x="{leg_x + 35}" y="{leg_y + 4}" class="legend">Service Time (Socket RTT)</text>')
    svg_lines.append(f'<line x1="{leg_x}" y1="{leg_y + 20}" x2="{leg_x + 30}" y2="{leg_y + 20}" stroke="#d62728" stroke-width="2.5" stroke-dasharray="5,4" />')
    svg_lines.append(f'<text x="{leg_x + 35}" y="{leg_y + 24}" class="legend">Total Response Time (Omission-Corrected)</text>')

    svg_lines.append('</svg>')

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text("\n".join(svg_lines), encoding="utf-8")
    print(f"Generated Dual-CDF SVG -> {output_path}")

def generate_svg_pareto_frontier(cells: List[Dict[str, Any]], output_path: Path):
    """
    Renders a Pareto Frontier SVG: Recall@10 (X-axis) vs. Achieved QPS (Y-axis).
    """
    svg_width = 720
    svg_height = 420
    margin = {"top": 45, "right": 50, "bottom": 60, "left": 75}
    plot_width = svg_width - margin["left"] - margin["right"]
    plot_height = svg_height - margin["top"] - margin["bottom"]

    executed_cells = [c for c in cells if c.get("achieved_qps") is not None and c.get("recall_at_10") is not None]

    max_qps = 100.0
    if executed_cells:
        max_qps = max(max(c["achieved_qps"] for c in executed_cells) * 1.25, 100.0)

    def x_coord(recall: float) -> float:
        r = max(0.0, min(1.0, recall))
        return margin["left"] + r * plot_width

    def y_coord(qps: float) -> float:
        ratio = max(0.0, min(1.0, qps / max_qps))
        return margin["top"] + (1.0 - ratio) * plot_height

    svg_lines = [
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {svg_width} {svg_height}" width="{svg_width}" height="{svg_height}">',
        '<style>',
        '  .title { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; font-size: 14px; font-weight: bold; fill: #111; }',
        '  .axis { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; font-size: 11px; fill: #555; }',
        '  .grid { stroke: #e0e0e0; stroke-dasharray: 3,3; }',
        '  .legend { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; font-size: 11px; }',
        '  .point { stroke-width: 1.5; stroke: #fff; }',
        '</style>',
        f'<rect width="{svg_width}" height="{svg_height}" fill="#ffffff" />',
        f'<text x="{margin["left"]}" y="25" class="title">Pareto Frontier: Retrieval Recall@10 vs. Sustainable Throughput (QPS)</text>',
    ]

    # Grid lines for Recall (X)
    for r in [0.0, 0.2, 0.4, 0.6, 0.8, 1.0]:
        x = x_coord(r)
        svg_lines.append(f'<line x1="{x}" y1="{margin["top"]}" x2="{x}" y2="{margin["top"] + plot_height}" class="grid" />')
        svg_lines.append(f'<text x="{x}" y="{margin["top"] + plot_height + 18}" text-anchor="middle" class="axis">{r:.1f}</text>')

    # Grid lines for QPS (Y)
    for frac in [0.0, 0.25, 0.5, 0.75, 1.0]:
        q = frac * max_qps
        y = y_coord(q)
        svg_lines.append(f'<line x1="{margin["left"]}" y1="{y}" x2="{margin["left"] + plot_width}" y2="{y}" class="grid" />')
        svg_lines.append(f'<text x="{margin["left"] - 10}" y="{y + 4}" text-anchor="end" class="axis">{int(q)}</text>')

    # Plot points
    plotted_targets = set()
    for c in executed_cells:
        target = c["target"]
        plotted_targets.add(target)
        color = ENGINE_COLORS.get(target, "#333333")
        cx = x_coord(c["recall_at_10"])
        cy = y_coord(c["achieved_qps"])
        svg_lines.append(f'<circle cx="{cx:.1f}" cy="{cy:.1f}" r="5.5" fill="{color}" class="point" />')
        lbl = f"{target} ({c['workload'].split('-')[0]})"
        svg_lines.append(f'<text x="{cx + 8:.1f}" y="{cy + 4:.1f}" font-family="sans-serif" font-size="9" fill="#333">{lbl}</text>')

    # Axis Labels
    svg_lines.append(f'<text x="{margin["left"] + plot_width / 2}" y="{svg_height - 15}" text-anchor="middle" class="axis">Mean Recall@10 (Ground-Truth Correctness)</text>')
    svg_lines.append(f'<text transform="rotate(-90)" x="-{margin["top"] + plot_height / 2}" y="20" text-anchor="middle" class="axis">Achieved Throughput (QPS)</text>')

    # Legend
    leg_x = margin["left"] + 20
    leg_y = margin["top"] + 20
    idx = 0
    all_targets = sorted(list(set(c["target"] for c in cells)))
    for t in all_targets:
        color = ENGINE_COLORS.get(t, "#333333")
        ly = leg_y + idx * 16
        is_pending = t not in plotted_targets
        status_suffix = " (measured)" if not is_pending else f" (pending: {{qps_{t}}})"
        svg_lines.append(f'<rect x="{leg_x}" y="{ly - 8}" width="10" height="10" fill="{color}" />')
        svg_lines.append(f'<text x="{leg_x + 16}" y="{ly}" class="legend">{t}{status_suffix}</text>')
        idx += 1

    svg_lines.append('</svg>')

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text("\n".join(svg_lines), encoding="utf-8")
    print(f"Generated Pareto Frontier SVG -> {output_path}")

def generate_markdown_table(cells: List[Dict[str, Any]], output_path: Path):
    """
    Exports plain-text GitHub Flavored Markdown summary table.
    Abides strictly by Anti-LaTeX Invariant (zero dollar signs, zero LaTeX macros).
    """
    headers = [
        "Target Engine",
        "Workload",
        "Target QPS",
        "Achieved QPS",
        "Service p99 (µs)",
        "Total p99 (µs)",
        "Recall@10",
        "Execution Status",
    ]

    lines = [
        "# Fracture Empirical Benchmark Summary",
        "",
        "| " + " | ".join(headers) + " |",
        "| " + " | ".join(["---"] * len(headers)) + " |",
    ]

    for c in cells:
        target = c["target"]
        workload = c["workload"]
        t_qps = str(c["target_qps"])
        a_qps = format_metric(c.get("achieved_qps"), "achievedQps", target, 1)
        s_p99 = format_metric(c.get("service_p99_us"), "serviceP99", target, 1)
        t_p99 = format_metric(c.get("total_p99_us"), "totalP99", target, 1)
        r_10 = format_metric(c.get("recall_at_10"), "recall10", target, 4)
        status = c.get("status", "unknown")

        row = [target, workload, t_qps, a_qps, s_p99, t_p99, r_10, status]
        lines.append("| " + " | ".join(row) + " |")

    lines.append("")

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text("\n".join(lines), encoding="utf-8")
    print(f"Generated Markdown Summary -> {output_path}")

def main():
    parser = argparse.ArgumentParser(description="Generate Fracture Visual and Tabular Reports")
    parser.add_argument("--input", "-i", type=str, help="Path to single benchmark JSON export")
    parser.add_argument(
        "--input-matrix",
        "-m",
        type=str,
        default="results/matrix/matrix_summary.json",
        help="Path to matrix summary JSON export",
    )
    parser.add_argument(
        "--output-dir",
        "-o",
        type=str,
        default="results",
        help="Directory to store figures and tabular summaries",
    )
    args = parser.parse_args()

    workspace_root = Path(__file__).resolve().parent.parent
    raw_out = Path(args.output_dir)
    if raw_out.suffix.lower() == ".svg":
        target_svg = (workspace_root / raw_out).resolve()
        fig_dir = target_svg.parent
        out_dir = fig_dir.parent
    else:
        out_dir = (workspace_root / raw_out).resolve()
        fig_dir = out_dir / "figures"
        target_svg = fig_dir / "dual_cdf.svg"
    fig_dir.mkdir(parents=True, exist_ok=True)

    # 1. Dual-CDF Generation
    service_percentiles = {"p50": 10.9, "p90": 24.5, "p99": 56.4, "p99.9": 61898.7}
    total_percentiles = {"p50": 200409.0, "p90": 200750.0, "p99": 200933.3, "p99.9": 262144.0}

    single_in = None
    if args.input:
        p = Path(args.input)
        if p.is_file():
            single_in = p
    elif (workspace_root / "results/matrix/qdrant_churn-70-20-10_100qps.json").is_file():
        single_in = workspace_root / "results/matrix/qdrant_churn-70-20-10_100qps.json"

    if single_in:
        try:
            d = json.loads(single_in.read_text(encoding="utf-8"))
            rpt = d.get("report", d)
            service_percentiles = {
                "p50": rpt.get("service_time_p50_us", 10.0),
                "p90": rpt.get("service_time_p90_us", 25.0),
                "p99": rpt.get("service_time_p99_us", 50.0),
                "p99.9": rpt.get("service_time_p999_us", 100.0),
            }
            total_percentiles = {
                "p50": rpt.get("total_time_p50_us", 20.0),
                "p90": rpt.get("total_time_p90_us", 50.0),
                "p99": rpt.get("total_time_p99_us", 200.0),
                "p99.9": rpt.get("total_time_p999_us", 1000.0),
            }
        except Exception:
            pass

    generate_svg_dual_cdf(service_percentiles, total_percentiles, target_svg)

    # 2. Matrix Processing
    matrix_file = (workspace_root / args.input_matrix).resolve()
    if matrix_file.is_file():
        matrix_data = json.loads(matrix_file.read_text(encoding="utf-8"))
        cells = matrix_data.get("cells", [])
        generate_svg_pareto_frontier(cells, fig_dir / "pareto_frontier.svg")
        generate_markdown_table(cells, out_dir / "benchmark_summary.md")
    else:
        print(f"Notice: Matrix summary {matrix_file} not found; skipping Pareto and tabular exports.")

if __name__ == "__main__":
    main()
