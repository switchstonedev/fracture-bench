# Canonical Vector Datasets & Stratified Modulo Partitions

This directory specifies the datasets and partition contracts used to evaluate target vector databases under \textsc{Fracture}.

---

## 1. Supported Corpora

| Dataset | Dimensionality | Distance Metric | Base Vector Count | Raw Vector Footprint (f32) | Estimated Graph Size (M=16) |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **SIFT-1M** | 128 dimensions | Euclidean ($L_2$) | 1,000,000 | ~512 MB | ~1.5 GB |
| **Cohere-1M** | 768 dimensions | Cosine / Dot Product | 1,000,000 | ~3.07 GB | ~5.5 GB |

---

## 2. Stratified Modulo Filter Selectivity Cohorts

To prevent synthetic evaluation bias in filtered retrieval, queries are partitioned into three strictly calibrated selectivity tiers based on document identifier modulo constraints:

*   **Cohort High (60% Match Probability):**
    ```text
    filter: (id % 10) in [0, 1, 2, 3, 4, 5]
    ```
    Models broad domain filtering (e.g., category or organization scoping).

*   **Cohort Medium (10% Match Probability):**
    ```text
    filter: (id % 10) == 0
    ```
    Models specific tag or date-bucket scoping.

*   **Cohort Needle-in-a-Haystack (0.1% Match Probability):**
    ```text
    filter: (id % 1000) == 42
    ```
    Models strict access control or narrow tenant ID filtering. Exposes whether the vector index employs pre-filtering with scalar inverted indices or falls back to degenerative post-filtering scans.

---

## 3. Ground Truth Verification Invariant

Exact $k$-NN ground truth is not evaluated against static embeddings. For each query, the observed committed Log Sequence Number (LSN) from the Write-Ahead Log is extracted, and brute-force SIMD linear scans are executed post-hoc across active vectors committed prior to that LSN cutoff.

