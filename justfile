#!/usr/bin/env -S just --justfile

alias b := build
alias f := fmt
alias l := lint
alias t := test

# The maintainer's signing identity, `LOKYR_SIGN_IDENTITY`, and other
# variables that stay out of the repository, from the untracked
# private/lokyr.env when it is there, which `cargo xtask` reads too; a
# variable of the environment wins.
set dotenv-path := "private/lokyr.env"

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
portable := "-p lokyr-vt-core -p lokyr-pty -p lokyr-session -p lokyr-search -p lokyr-kernel -p lokyr-app-core -p lokyr-text-core -p lokyr-render-core -p lokyr-ui-core -p lokyr-platform-core -p lokyr-replay -p lokyr-differential -p xtask"
# The platform-independent crates whose tests `check-linux` checks for Linux from any host.
portable_tests := "-p lokyr-pty -p lokyr-session -p lokyr-kernel -p lokyr-app-core -p lokyr-text-core -p lokyr-render-core -p lokyr-ui-core -p lokyr-platform-core -p lokyr-replay -p lokyr-differential -p xtask"

# List available commands.
_default:
    just --list --unsorted

# Build the workspace.
build:
    cargo build --locked --workspace --all-targets

# Test the workspace, every suite even after one fails.
test:
    cargo test --locked --workspace --no-fail-fast

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

# Check the platform-independent crates for Linux, from any host, with the tests of those whose tests build for it from macOS: the VT core's and the search's need a C compiler for Linux, as criterion builds C for its target.
check-linux:
    cargo check --locked --target x86_64-unknown-linux-gnu {{ portable }}
    cargo check --locked --target x86_64-unknown-linux-gnu --tests {{ portable_tests }}

# Test the platform-independent crates on the host (used by the Linux CI job), every suite even after one fails.
test-portable:
    cargo test --locked --no-fail-fast {{ portable }}

# Check advisories, licenses, banned crates and sources.
deny:
    cargo deny --locked check

# Check the dependency rules of the workspace.
check-deps:
    cargo xtask check-deps

# Check that the code's comments name no task, phase or ADR.
check-comments:
    cargo xtask check-comments

# Check that the internal links and anchors of the Markdown files resolve.
check-links:
    cargo xtask check-links

# Test the measuring tools of tools/bench, which compare the terminals and read powermetrics' recordings (macOS).
test-bench-tools:
    python3 -m unittest discover -s tools/bench

# Open the gallery of ui-core's nodes and widgets, each in its states, to check them by hand (macOS); `just ui-gallery --images <directory>` draws it, light and dark, into PNGs.
ui-gallery *args:
    cargo run --locked -p lokyr-ui-gallery -- {{args}}

# Run the CI checks locally. Linux is covered by `check-linux`.
ci: lint build test check-linux deny check-deps check-comments check-links bundle

# Build target/Lokyr.app from the release binary, signed with the identity that $LOKYR_SIGN_IDENTITY names, or ad hoc; `--features three-drawables` builds it of three drawables a window.
bundle *args:
    cargo xtask bundle {{args}}

# Build target/Lokyr.app, have Apple notarize it, and staple the ticket: the keychain's profile lokyr-notary, or $LOKYR_NOTARY_PROFILE, has the credentials. It sends the bundle to Apple's notary service.
notarize: bundle
    cargo xtask notarize

# Build target/Lokyr.app, then target/Lokyr-<version>.dmg of it, the bundle and the image each notarized by Apple and stapled. It sends both to Apple's notary service.
dmg: bundle
    cargo xtask dmg

# Release a candidate, X.Y.Z-rc.N, or a release, X.Y.Z, of the commit checked out, which GitHub has, its build's number above the feed's latest, or `--without-feed` for a feed's first release: the bundle and its DMG, notarized, its dSYM and its SBOM, to a draft of its GitHub release, which you publish, and to the feed of tests. It sends the bundle and the DMG to Apple's notary service.
release version *args:
    cargo xtask release {{version}} {{args}}

# Have Lokyr on this Mac read the feed of tests in private/feed, which `cargo xtask feed add <dmg>` writes, and serve it on 127.0.0.1:8123 until Control-C.
feed-serve:
    cargo xtask feed serve

# Compile the terminfo entry xterm-lokyr into target/terminfo, with the tic on the PATH or $TIC.
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
    cargo run -q --release -p lokyr-replay --bin lokyr-record -- crates/vt-core/tests/recordings/*.script

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
    cargo run -q --release -p lokyr-replay --bin lokyr-esctest -- "$dir/esctest" "$@"

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
    cargo bench --locked -p lokyr-vt-core -p lokyr-search
    cargo build --locked --release -p lokyr-differential --bin lokyr-throughput -p lokyr-replay --bin lokyr-replay
    # The file of 1 GiB, written again when a source is newer.
    if [ ! -f target/cat-1g.txt ] || [ -n "$(find crates tools -name '*.rs' -newer target/cat-1g.txt -print -quit)" ]; then target/release/lokyr-throughput --write-cat target/cat-1g.txt; fi
    target/release/lokyr-throughput --vtebench target/vtebench --runs 5 target/cat-1g.txt
    target/release/lokyr-replay --bench --cols 120 --rows 40 target/cat-1g.txt

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

# Count the instructions of vtebench's scenarios, of kitty's tests of CSI sequences and of unique clusters, and of text as `cat` prints it, with callgrind (Linux, with Valgrind; installs gungraun-runner), and fail on a rise of more than 3% from the counts of commit `base`, which a worktree of it takes first. When `base` has no counts to take, as a base without the package `lokyr-differential`, from before the rename, the commit is counted twice, the second time against the first, with a warning; without `base`, once.
instructions base="": _valgrind vtebench
    #!/usr/bin/env bash
    set -euo pipefail
    # The version of gungraun's runner that the Cargo.lock of directory $1
    # holds, as the runner runs only the library of its own version.
    version() { awk '/^name = "gungraun-runner"$/ { getline; gsub(/version = |"/, ""); print }' "$1/Cargo.lock"; }
    head=$(version .)
    if ! cargo install --list | grep -qx "gungraun-runner v$head:"; then
        cargo install --locked gungraun-runner --version "$head"
    fi
    # Both counts go to one place, for the second to compare with the first,
    # and read the one checkout of vtebench, whose place gungraun, which
    # clears the benchmark's environment, has to pass on.
    # An absolute path, as the base runs from its worktree.
    CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target}"
    case "$CARGO_TARGET_DIR" in /*) ;; *) CARGO_TARGET_DIR="$PWD/$CARGO_TARGET_DIR" ;; esac
    export CARGO_TARGET_DIR
    export GUNGRAUN_HOME="$CARGO_TARGET_DIR/gungraun" LOKYR_VTEBENCH="$PWD/target/vtebench" GUNGRAUN_ENVS=LOKYR_VTEBENCH
    count() { cargo bench --locked -p lokyr-differential --bench instructions -- "$@"; }
    # A warning, which a GitHub job shows besides its log.
    warn() { if [ -n "${GITHUB_ACTIONS:-}" ]; then echo "::warning::$1"; else echo "$1"; fi; }
    base="{{ base }}"
    if [ -n "$base" ] && ! git cat-file -e "$base^{commit}" 2>/dev/null; then
        warn "There is no commit $base: the counts are taken alone, against nothing."
        base=""
    fi
    if [ -z "$base" ]; then
        count
        exit
    fi
    tree=$(mktemp -d "${TMPDIR:-/tmp}/instructions.XXXXXX")
    trap 'rmdir "$tree"' EXIT
    git worktree add --quiet --detach "$tree" "$base"
    trap 'git worktree remove --force "$tree"' EXIT
    # A base from before the rename has the benchmark under another
    # package's name, which `count` does not take.
    if [ -f "$tree/tools/differential/benches/instructions.rs" ] \
        && grep -qx 'name = "lokyr-differential"' "$tree/tools/differential/Cargo.toml"; then
        # The base builds apart: cargo tells whether a crate is fresh from
        # its sources' mtimes, and would take the commit under test, checked
        # out before the base, for built already. It builds with its own
        # toolchain, and runs with the runner of its own gungraun.
        runner="$PATH"
        if [ "$(version "$tree")" != "$head" ]; then
            root="$CARGO_TARGET_DIR/gungraun-runner-$(version "$tree")"
            if [ ! -x "$root/bin/gungraun-runner" ]; then
                cargo install --locked --root "$root" gungraun-runner --version "$(version "$tree")"
            fi
            runner="$root/bin:$PATH"
        fi
        (
            cd "$tree"
            export CARGO_TARGET_DIR="$CARGO_TARGET_DIR/instructions-base" PATH="$runner"
            count --save-baseline=base
        )
    else
        # The comparison runs all the same, and the counts must not move.
        warn "$base has no instruction counts of lokyr-differential: the commit is counted twice, the second time against the first."
        count --save-baseline=base
    fi
    count --baseline=base --callgrind-limits='ir=3%'

# Stop `instructions` when Valgrind is not installed, before anything downloads or builds.
_valgrind:
    #!/usr/bin/env bash
    if ! command -v valgrind >/dev/null; then
        echo "instructions: Valgrind is not installed, and callgrind counts the instructions; it runs on Linux." >&2
        exit 1
    fi

# libFuzzer's flags for every run. With several workers on Linux, libFuzzer's fork mode would go
# on past a timeout or a run out of memory, which these make stop the run, as a crash does and as
# they do with one worker. On macOS the fork mode takes both for crashes already, as it reads
# its jobs' waitpid status.
fuzz_flags := "-ignore_timeouts=0 -ignore_ooms=0"

# Fuzz a target for `seconds`, seeded from the parser's tests, with `workers` processes that share its corpus, until a crash stops it; it lists the inputs saved, which go into a directory of its own in fuzz/artifacts/TARGET, named for its log, and fails on a crash, a timeout or a run out of memory, of a job too. More than one worker runs libFuzzer's fork mode, whose jobs restart, so that a worker runs fewer inputs than a process alone. The log goes to fuzz/logs too (needs `cargo install cargo-fuzz --locked`).
fuzz target="parser" seconds="600" workers="1": (_counts "fuzz" seconds workers "1") fuzz-seeds
    #!/usr/bin/env bash
    set -u
    rustup toolchain install {{ fuzz_toolchain }} --profile minimal || exit
    just --justfile "{{ justfile() }}" _fuzz-run {{ target }} {{ seconds }} {{ workers }} "logs/{{ target }}-$(date +%Y%m%d-%H%M%S).log" "{{ fuzz_flags }}"

# Fuzz a target as `fuzz` does, but go on past each crash, timeout or run out of memory for the whole of `seconds`, saving every input that makes one, so that one run finds what several would; it lists them and fails if it saved any, or if a job crashed, timed out or ran out of memory without saving one, as when the system kills it. It needs two workers at least, as libFuzzer goes on past a crash only in its fork mode.
fuzz-collect target="differential" seconds="3600" workers="6": (_counts "fuzz-collect" seconds workers "2") fuzz-seeds
    #!/usr/bin/env bash
    set -u
    rustup toolchain install {{ fuzz_toolchain }} --profile minimal || exit
    just --justfile "{{ justfile() }}" _fuzz-run {{ target }} {{ seconds }} {{ workers }} "logs/{{ target }}-collect-$(date +%Y%m%d-%H%M%S).log" "{{ fuzz_flags }} -ignore_crashes=1"

# Fuzz every target at once for `seconds`, a day by default, each with `workers` processes that share its corpus, on 3 × `workers` cores; each run's logs go to fuzz/logs, one a target, named for the run's start.
fuzz-all seconds="86400" workers="1": (_counts "fuzz-all" seconds workers "1") fuzz-seeds
    #!/usr/bin/env bash
    set -u
    rustup toolchain install {{ fuzz_toolchain }} --profile minimal || exit
    started=$(date +%Y%m%d-%H%M%S)
    pids=()
    for target in parser terminal differential; do
        (
            log=logs/$target-$started.log
            just --justfile "{{ justfile() }}" _fuzz-run $target {{ seconds }} {{ workers }} "$log" "{{ fuzz_flags }}" >/dev/null
            code=$?
            # What the run took, from its log, without the build, which runs on the same
            # code add up to: a crash stops it early, and its first minutes go to the
            # corpus, read before it fuzzes.
            ran=$(grep -oE "^$target: ran for [0-9]+ s$" "fuzz/$log" | tail -1 | grep -oE '[0-9]+ s$')
            if [ {{ workers }} -eq 1 ]; then workers="1 worker"; else workers="{{ workers }} workers"; fi
            echo "$target: exit $code, $workers for ${ran:-no time, as it did not start}, log fuzz/$log"
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

# Stop `recipe` before anything builds when `seconds` is not a count from 1 to what libFuzzer reads, an int's 2147483647, or `workers` from `least` to what cargo-fuzz takes, 65535; or when several workers would run libFuzzer's fork mode from a path with a space, which its jobs' command lines split.
_counts recipe seconds workers least:
    #!/usr/bin/env bash
    # Whether $1 is a whole number from 1 to $2, as long as it at most, which keeps
    # bash's integers from overflowing.
    count() { [[ "$1" =~ ^[1-9][0-9]*$ ]] && [ "${#1}" -le "${#2}" ] && [ "$1" -le "$2" ]; }
    if ! count "{{ seconds }}" 2147483647; then
        echo "{{ recipe }}: the seconds, '{{ seconds }}', are not a count from 1 to 2147483647, as libFuzzer reads them." >&2
        exit 2
    fi
    if ! count "{{ workers }}" 65535; then
        echo "{{ recipe }}: the workers, '{{ workers }}', are not a count of processes from 1 to 65535, as cargo-fuzz takes them." >&2
        exit 2
    fi
    # fuzz-collect's, as cargo-fuzz runs libFuzzer's fork mode for more than one.
    if [ "{{ workers }}" -lt "{{ least }}" ]; then
        echo "{{ recipe }} needs {{ least }} workers at least: with one, cargo-fuzz runs libFuzzer without its fork mode, which alone goes on past a crash." >&2
        exit 2
    fi
    if [ "{{ workers }}" -gt 1 ] && [[ "{{ justfile_directory() }}" == *" "* ]]; then
        echo "{{ recipe }}: libFuzzer's fork mode, which more than one worker runs, joins its jobs' arguments with spaces, and the path of this checkout has one." >&2
        exit 2
    fi

# Run one fuzz target with libFuzzer's `flags`, its output to the terminal and to `log`, under fuzz/, and fail as it does, if one of its jobs crashed, timed out or ran out of memory, or if it saved an input that did, which it lists, with how to reproduce and minimize one. The run saves its inputs into a directory of its own, named for its log, in fuzz/artifacts/TARGET, and its log ends with the time that it ran, without the build, and what the recipe found. An interruption ends the run as a time limit would, and fails it.
_fuzz-run target seconds workers log flags:
    #!/usr/bin/env bash
    set -u
    cd fuzz || exit
    log="{{ log }}"
    if ! mkdir -p "corpus/{{ target }}" "artifacts/{{ target }}" "$(dirname "$log")"; then
        echo "{{ target }}: the run's directories cannot be made under fuzz/." >&2
        exit 1
    fi
    # The inputs that the run saves go here alone, so that another run of the target
    # at once keeps its own.
    saves="artifacts/{{ target }}/$(basename "$log" .log)"
    if [ -e "$saves" ]; then
        echo "{{ target }}: fuzz/$saves is there already, another run's." >&2
        exit 1
    fi
    if ! mkdir "$saves"; then
        echo "{{ target }}: fuzz/$saves cannot be made." >&2
        exit 1
    fi
    # What the recipe finds goes to the log too, which is all that fuzz-all keeps.
    say() { echo "$1" | tee -a "$log" >&2; }
    # An interruption ends the run, whose end the recipe still writes, and the log's
    # writer outlives it, to take the run's last lines.
    interrupted=""
    trap 'interrupted=1' INT TERM HUP
    keep() { (trap '' INT TERM HUP && exec tee "$@"); }
    cargo +{{ fuzz_toolchain }} fuzz build {{ target }} 2>&1 | keep "$log"
    code=${PIPESTATUS[0]}
    if [ -n "$interrupted" ] || [ "$code" -ne 0 ]; then
        rmdir "$saves"
        if [ -n "$interrupted" ]; then
            say "{{ target }}: interrupted while it built."
            [ "$code" -ne 0 ] || code=130
        fi
        exit $code
    fi
    start=$SECONDS
    # libFuzzer takes the last `-artifact_prefix`, after cargo-fuzz's.
    cargo +{{ fuzz_toolchain }} fuzz run --jobs {{ workers }} {{ target }} "corpus/{{ target }}" seeds/parser -- -max_total_time={{ seconds }} "-artifact_prefix=$PWD/$saves/" {{ flags }} 2>&1 | keep -a "$log"
    code=${PIPESTATUS[0]}
    echo "{{ target }}: ran for $((SECONDS - start)) s" | tee -a "$log"
    if [ -n "$interrupted" ]; then
        say "{{ target }}: interrupted."
        [ "$code" -ne 0 ] || code=130
    fi
    # On macOS, with several workers, libFuzzer exits with the status of the job that
    # ended it as waitpid gives it, a crash's 19712, which the shell takes modulo 256,
    # as 0; its log tells. On Linux it exits with the job's code.
    exiting=$(grep -oE '^INFO: exiting: [0-9]+' "$log" | tail -1 | grep -oE '[0-9]+$')
    if [ "$code" -eq 0 ] && [ "${exiting:-0}" != 0 ]; then
        say "{{ target }}: libFuzzer exited with $exiting"
        code=1
    fi
    # The fork mode counts the jobs that crashed, timed out or ran out of memory, those
    # that the system killed too, which save no input. It prints the counter before it
    # counts the last job that it merges, which its exit code tells; the jobs that the
    # time limit leaves running are told by neither.
    counted=$(grep -oE 'oom/timeout/crash: [0-9]+/[0-9]+/[0-9]+' "$log" | tail -1 | grep -oE '[0-9/]+$')
    if [ -n "$counted" ] && [ "$counted" != 0/0/0 ]; then
        say "{{ target }}: jobs ran out of memory, timed out or crashed: $counted"
        code=1
    fi
    inputs() { if [ "$1" -eq 1 ]; then echo "1 input"; else echo "$1 inputs"; fi; }
    # The inputs saved, which a run that goes on past crashes ends with 0 after, and
    # whose names its log may not hold, as the fork mode keeps only a job's errors.
    saved=$(find "$saves" -type f | sort)
    failed=$(echo "$saved" | grep -E '/(crash|timeout|oom|leak)-' || true)
    slow=$(echo "$saved" | grep -E '/slow-unit-' || true)
    if [ -n "$failed" ]; then
        say "{{ target }}: $(inputs "$(echo "$failed" | wc -l | tr -d " ")") saved that crashed, timed out or ran out of memory:"
        echo "$failed" | sed 's|^|  fuzz/|' | tee -a "$log" >&2
        # As cargo-fuzz would say, which looks for them in fuzz/artifacts/TARGET alone.
        first=$(echo "$failed" | head -1)
        say "Reproduce one with: cargo +{{ fuzz_toolchain }} fuzz run {{ target }} fuzz/$first"
        say "Minimize it with: cargo +{{ fuzz_toolchain }} fuzz tmin {{ target }} fuzz/$first"
        code=1
    fi
    # Slow inputs, which the run reports, fail nothing.
    if [ -n "$slow" ]; then
        say "{{ target }}: $(inputs "$(echo "$slow" | wc -l | tr -d " ")") saved that ran slowly, which fail nothing:"
        echo "$slow" | sed 's|^|  fuzz/|' | tee -a "$log" >&2
    fi
    # A run that saved nothing leaves no directory behind.
    rmdir "$saves" 2>/dev/null
    exit $code

# Export the inputs of the parser's tests as fuzzing seeds.
fuzz-seeds:
    LOKYR_FUZZ_SEEDS="{{ justfile_directory() }}/fuzz/seeds/parser" cargo test --locked -p lokyr-vt-core --lib parser::seeds::export_fuzz_seeds -- --ignored --exact
