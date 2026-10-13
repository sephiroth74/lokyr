#!/usr/bin/env -S just --justfile

alias b := build
alias f := fmt
alias l := lint
alias t := test

# The nightly toolchain that cargo-fuzz needs, pinned like the main one:
# bump it in a dedicated commit.
fuzz_toolchain := "nightly-2026-10-12"

# Platform-independent crates, which must keep building for Linux.
portable := "-p saetta-vt-core -p saetta-pty -p saetta-session -p saetta-search -p saetta-kernel -p saetta-text-core -p saetta-render-core -p saetta-ui-core -p saetta-platform-core"

# List available commands.
_default:
    just --list --unsorted

# Build the workspace.
build:
    cargo build --locked --workspace --all-targets

# Test the workspace.
test:
    cargo test --locked --workspace

# Format the codebase.
fmt:
    cargo fmt --all

# Check that the codebase is formatted.
fmt-check:
    cargo fmt --all -- --check

# Run clippy on the workspace, with warnings as errors.
clippy:
    cargo clippy --locked --workspace --all-targets --all-features -- -D warnings

# Lint the codebase: formatting, then clippy.
lint: fmt-check clippy

# Check the platform-independent crates for Linux, from any host.
check-linux:
    cargo check --locked --target x86_64-unknown-linux-gnu {{ portable }}

# Test the platform-independent crates on the host (used by the Linux CI job).
test-portable:
    cargo test --locked {{ portable }}

# Check advisories, licenses, banned crates and sources.
deny:
    cargo deny --locked check

# Check the dependency rules of the workspace.
check-deps:
    cargo xtask check-deps

# Run the CI checks locally. Linux is covered by `check-linux`.
ci: lint build test check-linux deny check-deps

# Run the benchmarks (not implemented yet).
bench:
    @echo "bench: not implemented yet" >&2 && exit 1

# Fuzz a target for `seconds`, seeded from the tests (needs `cargo install cargo-fuzz --locked`).
fuzz target="parser" seconds="600": fuzz-seeds
    rustup toolchain install {{ fuzz_toolchain }} --profile minimal
    mkdir -p fuzz/corpus/{{ target }}
    cargo +{{ fuzz_toolchain }} fuzz run {{ target }} fuzz/corpus/{{ target }} fuzz/seeds/{{ target }} -- -max_total_time={{ seconds }}

# Export the inputs of the parser's tests as fuzzing seeds.
fuzz-seeds:
    SAETTA_FUZZ_SEEDS="{{ justfile_directory() }}/fuzz/seeds/parser" cargo test --locked -p saetta-vt-core --lib parser::seeds::export_fuzz_seeds -- --ignored --exact

# Build the signed app bundle (not implemented yet).
bundle:
    @echo "bundle: not implemented yet" >&2 && exit 1
