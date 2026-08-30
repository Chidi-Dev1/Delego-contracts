# Unified build tooling for all five Delego contract crates.
#
# The same recipe names are mirrored in each crate's package.json
# (build-wasm / test / lint) so contributors can use either entry point.
#
# Requires: cargo + the wasm32-unknown-unknown target (see README).

build-wasm:
    cargo build --target wasm32-unknown-unknown --release

test:
    cargo test --workspace

lint:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

check: lint fmt-check
    cargo check --workspace
