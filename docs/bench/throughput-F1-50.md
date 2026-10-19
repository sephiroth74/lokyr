# Throughput, the VT core against `alacritty_terminal` (F1-50)

Measured on 2026-10-19 with `just bench`, after F1-53, on the machine of [the F0 baseline](baseline-F0.md): an Apple M5 Pro, macOS 27.0.1, on AC power, idle but for the run. Rust 1.99.0, release builds, `alacritty_terminal` 0.26.0.

## Method

- **Both cores** are fed the same bytes, as a PTY hands output over, in reads of 64 KiB, by `bolt-throughput` (`tools/differential`): a terminal of 120 by 40 each, with its default options. The VT core keeps 10 MiB of scrollback, about 10 000 rows at that width, and `alacritty_terminal` 10 000 lines. Each figure is the median of 5 runs, each into a new terminal; only the feeding counts.
- **vtebench's scenarios**, at commit `ead8003` of <https://github.com/alacritty/vtebench>, which `just vtebench` fetches. Nothing runs its scripts: `bolt-throughput` rebuilds what they print at 120 by 40, with the data files of the checkout for the vim sessions and the Unicode symbols, and turns each line feed into CR LF, as a PTY does. As vtebench does, a sample resets the terminal and runs the setup, both left out of the time, then the payload, repeated to 1 MiB at least; a run takes 8 samples.
- **`cat` of 1 GiB**: the repository's Rust sources, repeated, their lines ended by CR LF, which `bolt-throughput --write-cat` writes to `target/cat-1g.txt`; `bolt-replay --bench` replays it too.
- **criterion** times the VT core's parser alone, with a handler that does nothing, the terminal that it drives, and the grid's own operations, on generated inputs of 1 MiB.

## Both cores

| Scenario | Bytes timed | VT core | `alacritty_terminal` | Ratio |
|---|---|---|---|---|
| cursor_motion | 8.5 MiB | 313.8 MiB/s | 376.7 MiB/s | 0.83 |
| dense_cells | 25.7 MiB | 375.4 MiB/s | 391.8 MiB/s | 0.96 |
| light_cells | 8.6 MiB | 823.0 MiB/s | 321.5 MiB/s | 2.56 |
| medium_cells | 8.2 MiB | 200.9 MiB/s | 265.7 MiB/s | 0.76 |
| scrolling | 8.0 MiB | 83.8 MiB/s | 90.4 MiB/s | 0.93 |
| scrolling_bottom_region | 8.0 MiB | 8.5 MiB/s | 235.5 MiB/s | 0.04 |
| scrolling_bottom_small_region | 8.0 MiB | 11.7 MiB/s | 73.4 MiB/s | 0.16 |
| scrolling_fullscreen | 8.0 MiB | 761.7 MiB/s | 269.9 MiB/s | 2.82 |
| scrolling_top_region | 8.0 MiB | 8.5 MiB/s | 42.5 MiB/s | 0.20 |
| scrolling_top_small_region | 8.0 MiB | 11.5 MiB/s | 75.8 MiB/s | 0.15 |
| sync_medium_cells | 8.5 MiB | 204.7 MiB/s | 274.4 MiB/s | 0.75 |
| unicode | 8.4 MiB | 147.3 MiB/s | 375.2 MiB/s | 0.39 |
| `cat` of 1 GiB | 1024 MiB | 541.0 MiB/s | 164.1 MiB/s | 3.30 |

`bolt-replay --bench` replays the same file at 489.5 MiB/s, the median of 5 runs: it copies the file into chunks first, and makes the terminal at the replay's size from its default.

## The VT core alone

| Benchmark | Throughput |
|---|---|
| parser, plain ASCII | 12.6 GiB/s |
| parser, colored output | 887 MiB/s |
| parser, Japanese text with emoji | 738 MiB/s |
| parser, the cursor moved for every character | 542 MiB/s |
| terminal, plain ASCII | 1.00 GiB/s |
| terminal, colored output | 313 MiB/s |
| terminal, Japanese text with emoji | 138 MiB/s |
| terminal, the cursor moved for every character | 305 MiB/s |
| grid, scrolling at the scrollback's limit | 72.2 M rows/s |
| grid, writing a row of 120 columns and scrolling | 10.6 M rows/s |
| reflow, 120 to 80 columns, 1 000 to 50 000 lines | 828 000 to 850 000 rows/s |

Plain ASCII passes the parser at memory speed, as its runs of text go to `print_ascii` whole. Reflow takes about 1.2 µs a row, some 11% more than criterion's earlier measure of 2026-10-16.

## Against the budget

The budget is at least `alacritty_terminal`'s throughput. The VT core passes it in three of the thirteen: `cat` of plain text, three times as fast, full rows that scroll, and screens full of one letter, which take the fast path of printable ASCII. It falls short in nine, which F1-51 takes:

- **Scrolling within a region**, 5 to 28 times slower: every LF at the bottom margin moves each row of the region one row up, some 340 ns a line of 38 rows, where `alacritty_terminal` rotates its ring of rows and swaps back only the rows outside the region, some 13 ns a line. The 200 MiB/s that F1-62 measured were of lines of 120 characters, where vtebench's lines are of one, `y` and CR LF, so that each scroll weighs on three bytes.
- **Unicode**, 0.39: the symbols' file is non-ASCII almost whole, which takes the grapheme segmentation and the width of each codepoint, where `alacritty_terminal` takes one lookup of `unicode-width` a character and no clusters; criterion's Japanese text, at 138 MiB/s, agrees.
- **The vim sessions**, 0.75, the cursor's moves, 0.83, and colored cells, 0.96: dense escape sequences, SGR and CUP, where the parser's own rate, 540 to 890 MiB/s, leaves the terminal's handling as the larger part.
- **Scrolling with the scrollback**, 0.93: a line of one character a scroll, where pushing the row into the scrollback, and a page in from the pool at each page's last row, is most of the time.
