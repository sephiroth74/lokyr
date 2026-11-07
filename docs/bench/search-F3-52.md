# The search of 100 000 lines through the app's service (F3-52)

Measured on 7 November 2026 with `just bench-search`, on the machine of [the F0 baseline](baseline-F0.md): an Apple M5 Pro, macOS 27.2, on AC power, the load average 1.8 as the runs began. Rust 1.99.0, a release build of the kernel's tests.

## Method

- **The scrollback**: a pane of 80 by 24 whose program, `awk`, printed 100 000 lines of exactly 80 columns, `needle` on one in a hundred, 1 000 matches, the others `nobody`; the scrollback's limit raised to 256 MiB, so that it holds them all.
- **The search**, as the find bar asks for it: the plugin's request of a literal, through the kernel's search service, its thread a part of 1 000 rows at a time from the view outward, each part's matches told to the plugin as an event, the plugin's delivery polled every 0.2 ms.
- **First match shown**: from the request until the first part that holds a match came, and the plugin's `show_matches` of it returned, the view scrolled to it.
- **All counted**: from the request until the search's last part came.
- The median of five searches, each ending the one before; the measure ran three times.

## Result

| Run | First match shown | All counted |
|---|---|---|
| 1 | 0.8 ms | 22.5 ms |
| 2 | 1.0 ms | 22.5 ms |
| 3 | 1.0 ms | 28.1 ms |

The search of 100 000 lines counts every match in a sixth of the 150 ms of F3's exit criterion, and shows its first within a millisecond, as the view's rows and the part above them come first. The engine alone takes 27.2 ms over 100 000 lines of 41 to 93 characters ([F1-54](search-F1-54.md)); the service's parts, its pins, its events and the plugin's requests add nothing that shows over these shorter, fuller lines.

A debug build takes about 500 ms for all of them and 10.7 ms for the first. Before F3-52, the main thread took the terminal's lock as the search's thread did, in turn, so that a request of the plugin's could wait for the whole search: the first match then showed as late as the last.
