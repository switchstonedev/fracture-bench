.PHONY: all build test lint check run-mock clean

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

clean:
	cargo clean

