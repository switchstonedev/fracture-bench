#!/usr/bin/env python3
"""
bootstrap_ci.py - Non-parametric 95% Bootstrap Confidence Interval Estimator.
Computes empirical confidence intervals for QPS and Recall@K across benchmark seeds.
"""

from typing import Tuple
import numpy as np

def compute_bootstrap_ci(
    data: np.ndarray,
    num_bootstrap: int = 10000,
    confidence_level: float = 0.95
) -> Tuple[float, float, float]:
    """
    Computes (mean, lower_ci, upper_ci) using non-parametric percentile bootstrap.
    """
    if len(data) == 0:
        raise ValueError("Data array cannot be empty")

    mean_val = float(np.mean(data))
    if len(data) == 1:
        return mean_val, mean_val, mean_val

    boot_means = np.empty(num_bootstrap)
    n = len(data)
    for i in range(num_bootstrap):
        sample = np.random.choice(data, size=n, replace=True)
        boot_means[i] = np.mean(sample)

    alpha = 1.0 - confidence_level
    lower_pct = (alpha / 2.0) * 100.0
    upper_pct = (1.0 - alpha / 2.0) * 100.0

    lower_ci = float(np.percentile(boot_means, lower_pct))
    upper_ci = float(np.percentile(boot_means, upper_pct))

    return mean_val, lower_ci, upper_ci

if __name__ == "__main__":
    # Example validation for Qdrant Recall@10 across 5 evaluation runs
    seeds_recall = np.array([0.962, 0.959, 0.964, 0.961, 0.963])
    mean, low, high = compute_bootstrap_ci(seeds_recall)
    print(f"Recall@10 Bootstrap Estimate: {mean:.4f} (95% CI: [{low:.4f}, {high:.4f}])")

