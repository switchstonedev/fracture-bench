# Multi-stage Dockerfile for Fracture 60-second reproduction demo
FROM rust:1.78-slim AS builder

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Compile fracture release binary
RUN cargo build --release -p fracture-bench

# Runtime stage: slim Python image with zero external pip dependencies
FROM python:3.11-slim AS runner

WORKDIR /workspace

# Install ca-certificates and curl for healthcheck / network adapter verification
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/*

# Copy compiled fracture binary
COPY --from=builder /build/target/release/fracture /usr/local/bin/fracture
RUN ln -s /usr/local/bin/fracture /workspace/fracture || true

# Copy scripts, fixtures, runs, and configuration
COPY Cargo.toml Cargo.lock README.md ARCHITECTURE.md VENDORS.md sha256sums.txt ./
COPY scripts ./scripts
COPY fixtures ./fixtures
COPY runs ./runs
RUN mkdir -p reports/figures results/matrix target/release && \
    cp /usr/local/bin/fracture target/release/fracture

# Default command: execute Level 1 dry run and generate SVG dual-CDF and Pareto reports
ENTRYPOINT ["python", "scripts/reproduce.py"]
CMD ["--tier", "dry_run", "--skip-manifest-check"]
