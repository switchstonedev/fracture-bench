.PHONY: all build test lint check run-mock clean bench-all bench-dry bench-sample bench-full drift docker-up docker-down

all: build

build:
	cargo build --release

test:
	cargo test

check:
	cargo check

lint:
	cargo clippy --workspace --all-targets -- -D warnings

run-mock:
	cargo run -p crucivec-cli -- run --target mock --qps 5000 --duration-sec 5

bench-all:
	python scripts/reproduce.py --tier sample

bench-dry:
	python scripts/reproduce.py --tier dry_run

bench-sample:
	python scripts/reproduce.py --tier sample

bench-full:
	python scripts/reproduce.py --tier full

drift:
	python scripts/measure_drift.py

docker-up:
	docker compose -f fixtures/docker-compose.qdrant.yml up -d
	docker compose -f fixtures/docker-compose.pgvector.yml up -d

docker-down:
	docker compose -f fixtures/docker-compose.qdrant.yml down
	docker compose -f fixtures/docker-compose.pgvector.yml down

clean:
	cargo clean

