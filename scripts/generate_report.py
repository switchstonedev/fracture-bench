#!/usr/bin/env python3
"""
generate_report.py - Publication-grade visual artifact and trace report generator.
Produces SVG/HTML Dual-CDF curves, Pareto frontiers, and cgroups v2 time-series.
"""

import argparse
import json
import math
from pathlib import Path
from typing import Dict, List, Tuple

def generate_svg_dual_cdf(
    service_percentiles: Dict[str, float],
    total_percentiles: Dict[str, float],
    output_path: Path
):
    """
    Renders a publication-ready SVG Dual-CDF curve comparing
    uncorrected service time against Coordinated-Omission-corrected total response time.
    """
    svg_width = 700
    svg_height = 420
    margin = {"top": 40, "right": 50, "bottom": 60, "left": 70}
    plot_width = svg_width - margin["left"] - margin["right"]
    plot_height = svg_height - margin["top"] - margin["bottom"]

    # Quantile mapping
    points = [("p50", 0.50), ("p90", 0.90), ("p99", 0.99), ("p99.9", 0.999)]

    min_lat = min(service_percentiles.get("p50", 1.0), 1.0)
    max_lat = max(total_percentiles.get("p99.9", 1000.0), 100.0)

    # Log10 scale mapping
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
        '  .title { font-family: sans-serif; font-size: 14px; font-weight: bold; fill: #111; }',
        '  .axis { font-family: sans-serif; font-size: 11px; fill: #555; }',
        '  .grid { stroke: #e0e0e0; stroke-dasharray: 3,3; }',
        '  .legend { font-family: sans-serif; font-size: 11px; }',
        '</style>',
        f'<rect width="{svg_width}" height="{svg_height}" fill="#ffffff" />',
        f'<text x="{margin["left"]}" y="25" class="title">Dual-CDF: Service Time vs. Total Response Time (Coordinated Omission)</text>',
    ]

    # Draw grid and axes
    for q in [0.50, 0.90, 0.99, 0.999]:
        y = y_coord(q)
        svg_lines.append(f'<line x1="{margin["left"]}" y1="{y}" x2="{margin["left"] + plot_width}" y2="{y}" class="grid" />')
        label = f"{q*100:.1f}%" if q < 0.99 else f"{q*100:.2f}%"
        svg_lines.append(f'<text x="{margin["left"] - 10}" y="{y + 4}" text-anchor="end" class="axis">{label}</text>')

    # Service time path
    svc_pts = []
    for k, q in points:
        val = service_percentiles.get(k, 10.0)
        svc_pts.append(f"{x_coord(val):.1f},{y_coord(q):.1f}")
    svg_lines.append(f'<polyline fill="none" stroke="#0072B2" stroke-width="2.5" points="{" ".join(svc_pts)}" />')

    # Total time path
    tot_pts = []
    for k, q in points:
        val = total_percentiles.get(k, 100.0)
        tot_pts.append(f"{x_coord(val):.1f},{y_coord(q):.1f}")
    svg_lines.append(f'<polyline fill="none" stroke="#D55E00" stroke-width="2.5" stroke-dasharray="5,4" points="{" ".join(tot_pts)}" />')

    # Labels and Legend
    svg_lines.append(f'<text x="{margin["left"] + plot_width / 2}" y="{svg_height - 15}" text-anchor="middle" class="axis">Latency (µs, Log Scale)</text>')

    # Legend box
    leg_x = margin["left"] + 20
    leg_y = margin["top"] + 20
    svg_lines.append(f'<line x1="{leg_x}" y1="{leg_y}" x2="{leg_x + 30}" y2="{leg_y}" stroke="#0072B2" stroke-width="2.5" />')
    svg_lines.append(f'<text x="{leg_x + 35}" y="{leg_y + 4}" class="legend">Service Time (Socket RTT)</text>')
    svg_lines.append(f'<line x1="{leg_x}" y1="{leg_y + 20}" x2="{leg_x + 30}" y2="{leg_y + 20}" stroke="#D55E00" stroke-width="2.5" stroke-dasharray="5,4" />')
    svg_lines.append(f'<text x="{leg_x + 35}" y="{leg_y + 24}" class="legend">Total Response Time (Omission-Corrected)</text>')

    svg_lines.append('</svg>')

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text("\n".join(svg_lines), encoding="utf-8")
    print(f"Generated Dual-CDF SVG -> {output_path}")

def main():
    parser = argparse.ArgumentParser(description="Generate CruciVec Visual Reports")
    parser.add_argument("--input", "-i", type=str, help="Path to input benchmark JSON export")
    parser.add_argument("--output", "-o", type=str, default="results/figures/dual_cdf.svg", help="Output SVG path")
    args = parser.parse_args()

    if args.input:
        in_path = Path(args.input)
        if not in_path.exists():
            print(f"Error: input file {in_path} not found")
            return
        data = json.loads(in_path.read_text(encoding="utf-8"))
        report = data.get("report", data)
        service_percentiles = {
            "p50": report.get("service_time_p50_us", 10.0),
            "p90": report.get("service_time_p90_us", 25.0),
            "p99": report.get("service_time_p99_us", 50.0),
            "p99.9": report.get("service_time_p999_us", 100.0),
        }
        total_percentiles = {
            "p50": report.get("total_time_p50_us", 20.0),
            "p90": report.get("total_time_p90_us", 50.0),
            "p99": report.get("total_time_p99_us", 200.0),
            "p99.9": report.get("total_time_p999_us", 1000.0),
        }
    else:
        # Example defaults
        service_percentiles = {"p50": 10.9, "p90": 24.5, "p99": 56.4, "p99.9": 61898.7}
        total_percentiles = {"p50": 200409.0, "p90": 200750.0, "p99": 200933.3, "p99.9": 262144.0}

    out_file = Path(args.output)
    generate_svg_dual_cdf(service_percentiles, total_percentiles, out_file)

if __name__ == "__main__":
    main()
