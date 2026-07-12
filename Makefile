# Local quality gates — Rust's ESLint/Prettier stack is rustfmt + clippy.
#
#   make fmt      format
#   make lint     clippy (deny warnings)
#   make test     unit tests
#   make check    fmt-check + clippy + test
#   make build    release binary

.PHONY: fmt fmt-check lint test check build

fmt:
	cargo fmt

fmt-check:
	cargo fmt -- --check

lint:
	cargo clippy --all-targets --all-features -- -D warnings

test:
	cargo test

check: fmt-check lint test

build:
	cargo build --release
