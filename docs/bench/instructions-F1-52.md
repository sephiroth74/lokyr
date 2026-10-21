# Instruction counts in CI (F1-52)

The instructions that the VT core takes for one sample of each of vtebench's scenarios, and for 1 MiB of text as `cat` prints it, as callgrind counts them. These are the counts of the first run of the `instructions` job, on GitHub's `ubuntu-latest` runner for x86-64, on 2026-10-20, at commit b4680a5.

## Method

- **The benchmark** is `instructions`, in `tools/differential/benches`, with gungraun 0.20.0 and Valgrind from Ubuntu. Its inputs are those of [the throughput report](throughput-F1-50.md): a terminal of 120 by 40 with its default options, fed in reads of 64 KiB. Each scenario's sample is its payload repeated to 1 MiB at least, then given CR LF as a PTY gives it. The reset and the setup are left out of the counts, as gungraun's `setup` leaves them out.
- **The text for `cat`** comes from a generator that gives the same text each time: words of one to twelve letters, lines of up to fifteen words.
- **The comparison**: `just instructions <base>` counts the commit that a push or a pull request starts from, in a worktree of it, and then the commit under test against it. A rise of more than 3% in any scenario fails the job. The base builds in a target directory of its own, with the toolchain that its `rust-toolchain.toml` names, and runs with the runner of its own gungraun: a commit that changes the toolchain is compared with the counts of another compiler. This first run had no base with the benchmark, so it counted the same commit twice: every count came out the same, to the instruction.

## Counts

| Scenario | Bytes of a sample | Instructions | A byte |
|---|---|---|---|
| cursor_motion | 1 107 600 | 96 099 497 | 86.8 |
| dense_cells | 3 369 678 | 232 420 869 | 69.0 |
| light_cells | 1 123 902 | 11 615 701 | 10.3 |
| medium_cells | 1 072 566 | 101 455 533 | 94.6 |
| scrolling | 1 572 864 | 363 901 861 | 231.4 |
| scrolling_bottom_region | 1 572 864 | 450 585 093 | 286.5 |
| scrolling_bottom_small_region | 1 572 864 | 552 064 565 | 351.0 |
| scrolling_fullscreen | 1 059 448 | 12 012 273 | 11.3 |
| scrolling_top_region | 1 572 864 | 467 757 449 | 297.4 |
| scrolling_top_small_region | 1 572 864 | 592 295 049 | 376.6 |
| sync_medium_cells | 1 116 918 | 104 672 345 | 93.7 |
| unicode | 1 106 384 | 112 344 138 | 101.5 |
| cat | 1 MiB | 28 555 894 | 27.2 |

The scrolling scenarios print three bytes a line, `y` and CR LF: the bottom region takes about 859 instructions a line. The counts that `/usr/bin/time -l` gave on Apple Silicon, about 805, are of another instruction set, which the job does not use.
