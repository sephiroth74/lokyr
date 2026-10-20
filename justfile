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

# The commit of vttest (https://github.com/ThomasDickey/vttest-snapshots)
# that `just vttest` builds, its snapshot t20251205: bump it in a dedicated
# commit.
vttest_commit := "0229d7171a8574a2bf406c6ce14549f65d810e51"

# The commit of vtebench (https://github.com/alacritty/vtebench) whose
# scenarios `just bench` measures, rebuilt from its scripts, with its data
# files: bump it in a dedicated commit.
vtebench_commit := "ead80032e57dee2e75f0b51f2ea67528647d9944"

# Platform-independent crates, and the tools of the terminal's tests and
# tasks, which must keep building, and passing their tests, on Linux.
portable := "-p bolt-vt-core -p bolt-pty -p bolt-session -p bolt-search -p bolt-kernel -p bolt-text-core -p bolt-render-core -p bolt-ui-core -p bolt-platform-core -p bolt-replay -p bolt-differential -p xtask"

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

# Test the platform-independent crates on the host (used by the Linux CI job), every suite even after one fails.
test-portable:
    cargo test --locked --no-fail-fast {{ portable }}

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

# Record the sessions of the VT core's tests again (needs vim, htop, tmux, less and fzf, and builds vttest).
record: vttest
    cargo run -q --release -p bolt-replay --bin bolt-record -- crates/vt-core/tests/recordings/*.script

# Run esctest in the headless terminal, from a checkout of the pinned commit in target/esctest2 (downloads it), and print the pass rate and the failures by sequence; `args` go to esctest, each whole, as `--include='CUP|CUB'`.
[positional-arguments]
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
    cargo run -q --release -p bolt-replay --bin bolt-esctest -- "$dir/esctest" "$@"

# Build vttest from the pinned commit into target/vttest (downloads it), for the recording of its screens.
vttest:
    #!/usr/bin/env bash
    set -euo pipefail
    dir=target/vttest
    if [ "$(cat "$dir/.commit" 2>/dev/null)" != "{{ vttest_commit }}" ]; then
        clone=$(mktemp -d)
        trap 'rm -rf "$clone"' EXIT
        git init -q --template= "$clone"
        git -C "$clone" fetch -q --depth 1 https://github.com/ThomasDickey/vttest-snapshots {{ vttest_commit }}
        rm -rf "$dir" && mkdir -p "$dir"
        git -C "$clone" archive FETCH_HEAD | tar -x -C "$dir"
        (cd "$dir" && ./configure --quiet > /dev/null && make --quiet > /dev/null)
        echo {{ vttest_commit }} > "$dir/.commit"
    fi

# Measure throughput: the criterion benchmarks of the VT core and of search, then both cores, the VT core and alacritty_terminal, over vtebench's scenarios and a file of 1 GiB, which `cat` would print; on an idle machine.
bench: vtebench
    cargo bench --locked -p bolt-vt-core -p bolt-search
    cargo build --locked --release -p bolt-differential --bin bolt-throughput -p bolt-replay --bin bolt-replay
    # The file of 1 GiB, written again when a source is newer.
    if [ ! -f target/cat-1g.txt ] || [ -n "$(find crates tools -name '*.rs' -newer target/cat-1g.txt -print -quit)" ]; then target/release/bolt-throughput --write-cat target/cat-1g.txt; fi
    target/release/bolt-throughput --vtebench target/vtebench --runs 5 target/cat-1g.txt
    target/release/bolt-replay --bench --cols 120 --rows 40 target/cat-1g.txt

# Fetch vtebench's scenarios from the pinned commit into target/vtebench (downloads it): their scripts, which nothing runs, and their data files.
vtebench:
    #!/usr/bin/env bash
    set -euo pipefail
    dir=target/vtebench
    if [ "$(cat "$dir/.commit" 2>/dev/null)" != "{{ vtebench_commit }}" ]; then
        clone=$(mktemp -d "${TMPDIR:-/tmp}/vtebench.XXXXXX")
        trap 'rm -rf "$clone"' EXIT
        git init -q --template= "$clone"
        git -C "$clone" fetch -q --depth 1 https://github.com/alacritty/vtebench {{ vtebench_commit }}
        rm -rf "$dir" && mkdir -p "$dir"
        git -C "$clone" archive FETCH_HEAD benchmarks | tar -x -C "$dir"
        echo {{ vtebench_commit }} > "$dir/.commit"
    fi

# Fuzz a target for `seconds`, seeded from the parser's tests (needs `cargo install cargo-fuzz --locked`).
fuzz target="parser" seconds="600": fuzz-seeds
    rustup toolchain install {{ fuzz_toolchain }} --profile minimal
    mkdir -p fuzz/corpus/{{ target }}
    cargo +{{ fuzz_toolchain }} fuzz run {{ target }} fuzz/corpus/{{ target }} fuzz/seeds/parser -- -max_total_time={{ seconds }}

# Fuzz every target at once for `seconds`, a day by default, each on a core of its own; the logs go to fuzz/logs.
fuzz-all seconds="86400": fuzz-seeds
    #!/usr/bin/env bash
    set -u
    rustup toolchain install {{ fuzz_toolchain }} --profile minimal
    mkdir -p fuzz/logs
    cd fuzz
    pids=()
    for target in parser terminal differential; do
        mkdir -p corpus/$target
        (
            cargo +{{ fuzz_toolchain }} fuzz run $target corpus/$target seeds/parser -- -max_total_time={{ seconds }} >logs/$target.log 2>&1
            code=$?
            echo "$target: exit $code"
            exit $code
        ) &
        pids+=($!)
    done
    # The run fails if any target does, as a crash stops it.
    failed=0
    for pid in "${pids[@]}"; do
        wait "$pid" || failed=1
    done
    exit $failed

# Export the inputs of the parser's tests as fuzzing seeds.
fuzz-seeds:
    BOLT_FUZZ_SEEDS="{{ justfile_directory() }}/fuzz/seeds/parser" cargo test --locked -p bolt-vt-core --lib parser::seeds::export_fuzz_seeds -- --ignored --exact

# Build the signed app bundle (not implemented yet).
bundle:
    @echo "bundle: not implemented yet" >&2 && exit 1
