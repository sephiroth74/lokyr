# Search over the scrollback

Measured on 2026-10-19 with `cargo bench -p bolt-search`, which `just bench` runs, on the machine of [the F0 baseline](baseline-F0.md): an Apple M5 Pro, macOS 27.0.1, on AC power, idle but for the run. Rust 1.99.0, a release build; criterion's median of 10 samples, which its baseline in `target/criterion` keeps for the runs after.

## Method

- **The scrollback**: a terminal of 120 by 40, with a limit of 4 GiB, through which varied lines of 41 to 93 characters went, numbered, with words among them; compacted, as an idle terminal's is, every 100 000 lines, so that the search reads cold pages as they are kept.
- **A search that finds nothing**, for `needle` and for the regex `line \d+ needle`, backward from the screen's last row, so that every row is read: over 100 000 and 1 000 000 lines, and over one logical line of 1 000 000 cells, soft-wrapped over 8 334 rows, which each search builds again.
- **`find_all`** of `fox`, on every line, over 100 000 lines, the first 1 000 matches pinned and the others counted.

## Result

| Search | Time |
|---|---|
| Literal, 100 000 lines | 27.2 ms |
| Regex, 100 000 lines | 28.8 ms |
| Literal, 1 000 000 lines | 272.8 ms |
| Regex, 1 000 000 lines | 291.8 ms |
| Literal, a line of 1 000 000 cells | 2.5 ms |
| Regex, a line of 1 000 000 cells | 2.6 ms |
| `find_all`, 100 000 lines, a match each | 28.5 ms |

A search over 100 000 lines takes a fifth of the 150 ms of F3's exit criterion, and grows with the lines, ten times as long over ten times as many. That is about 4 ns a cell over the short lines, of some 68 characters each, and 2.5 ns a cell over the long line, whose cells are read as one text, without a line to start and end at each row.
