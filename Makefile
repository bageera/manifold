# Manifold — developer gate targets.
# The FIRST thing to run before pushing (the CI gate runs the same steps).
#
#   make gate     # fmt + clippy + tests + docs (the pre-pipeline gate)
#   make fmt      # fix formatting in place
#   make check    # compile all targets
#   make test     # full test suite (release)
#   make clippy   # zero-warning bar
#   make doc      # rustdoc with warnings denied
#   make bench    # criterion decode-throughput benches
#   make sizes    # deterministic bitstream-size report
#   make corpus   # one-command fair lossless comparison (webp/jxl if installed)
#   make fuzz     # mutation fuzzer (must exit 0)
#   make clean

export PATH := /opt/homebrew/opt/rustup/bin:$(HOME)/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$(PATH)
export RUSTUP_HOME := $(HOME)/.rustup
export RUSTDOCFLAGS := -D warnings

.PHONY: gate fmt check test clippy doc bench sizes corpus fuzz clean

gate: fmt-check clippy test doc
	@echo "GATE: OK (fmt, clippy, tests, docs)"

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

check:
	cargo check --all-targets

test:
	cargo test --release

clippy:
	cargo clippy --all-targets -- -D warnings

doc:
	cargo doc --no-deps

bench:
	cargo bench -p manifold-codec

sizes:
	cargo build --release
	python3 scripts/report_sizes.py

corpus:
	cargo build --release
	python3 scripts/compare_lossless.py

fuzz:
	cargo build --release
	python3 scripts/fuzz.py --trials 300

clean:
	cargo clean