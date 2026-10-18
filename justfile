#!/usr/bin/env -S just --justfile

alias b := build
alias f := fmt
alias l := lint
alias t := test

# The nightly toolchain that cargo-fuzz needs, pinned like the main one:
# bump it in a dedicated commit.
fuzz_toolchain := "nightly-2026-10-12"

# The commit of esctest2 (https://github.com/ThomasDickey/esctest2) that
# `just esctest` runs: bump it in a dedicated commit.
esctest_commit := "2798f12149a19c3295e9b4853ab2da4b2eff1b2b"

# Platform-independent crates, which must keep building for Linux.
portable := "-p bolt-vt-core -p bolt-pty -p bolt-session -p bolt-search -p bolt-kernel -p bolt-text-core -p bolt-render-core -p bolt-ui-core -p bolt-platform-core -p bolt-replay"

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

# Compile the terminfo entry xterm-bolt into target/terminfo, with the tic on the PATH or $TIC.
terminfo:
    cargo xtask terminfo

# Generate the Unicode tables of vt-core from the pinned Unicode version (downloads the UCD).
ucd:
    cargo xtask ucd

# Check that the Unicode tables match the pinned Unicode version (downloads the UCD).
ucd-check:
    cargo xtask ucd --check

# Record the sessions of the VT core's tests again (needs vim, htop, tmux, less and fzf).
record:
    cargo run -q -p bolt-replay --bin bolt-record -- crates/vt-core/tests/recordings/*.script

# Run esctest in the headless terminal, from a checkout of the pinned commit in target/esctest2 (downloads it), and print the pass rate and the failures by sequence; `args` go to esctest, as `--include=CUPTests`.
esctest *args:
    #!/usr/bin/env bash
    set -euo pipefail
    dir=target/esctest2
    if [ "$(cat "$dir/.commit" 2>/dev/null)" != "{{ esctest_commit }}" ]; then
        clone=$(mktemp -d)
        trap 'rm -rf "$clone"' EXIT
        git init -q --template= "$clone"
        git -C "$clone" fetch -q --depth 1 https://github.com/ThomasDickey/esctest2 {{ esctest_commit }}
        rm -rf "$dir" && mkdir -p "$dir"
        git -C "$clone" archive FETCH_HEAD | tar -x -C "$dir"
        echo {{ esctest_commit }} > "$dir/.commit"
    fi
    cargo run -q --release -p bolt-replay --bin bolt-esctest -- "$dir/esctest" {{ args }}

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
    BOLT_FUZZ_SEEDS="{{ justfile_directory() }}/fuzz/seeds/parser" cargo test --locked -p bolt-vt-core --lib parser::seeds::export_fuzz_seeds -- --ignored --exact

# Build the signed app bundle (not implemented yet).
bundle:
    @echo "bundle: not implemented yet" >&2 && exit 1
