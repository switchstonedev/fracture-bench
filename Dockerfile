# Hermetic reproduction container for Fracture Benchmark
FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive
ENV PATH="/root/.cargo/bin:${PATH}"

RUN apt-get update && apt-get install -y --no-install-recommends \
    curl \
    ca-certificates \
    build-essential \
    python3 \
    python3-pip \
    git \
    && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain 1.75.0
RUN pip3 install --no-cache-dir numpy pandas matplotlib

WORKDIR /workspace
COPY . /workspace

RUN cargo build --release -p fracture-bench

ENTRYPOINT ["python3", "Scripts/reproduce.py"]
CMD ["--tier", "sample"]
