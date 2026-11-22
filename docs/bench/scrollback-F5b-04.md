# The restored scrollback's cost at the quit and the start

Measured on 22 November 2026 on an M5 Pro on AC power, the load average 3.1 to 3.3. The cost is the time that keeping and reading back the panes' scrollback adds to the quit and to the next start, and the bytes of the files that it keeps. The measure is the app's own, [`scrollback_costs`](../../crates/app-core/src/app/tests/restore.rs), a release build of the app over the platform's fake, whose results are in [results.txt](scrollback-F5b-04/results.txt).

## Method

- **The panes**: 1, 10 and 50 tabs of a pane each, `cat` their program, of the default config: 100 columns, `[scrollback] limit` of 10 MiB a pane and `budget` of 100 MiB that the panes share.
- **The fill**: empty; half full, 6000 lines of 94 cells of color and bold; full, 20 000 such lines, past the limit, so that each pane keeps some 12 000 rows.
- **The quit**: the windows kept, `keep_windows`, timed, with `restore-scrollback` off and on, so that the difference is the scrollback's own: each pane archived within its limit and its file written.
- **The start**: the next start of the windows kept, timed until every pane is made, off and on; on, each pane reads its file and its terminal reads the rows back before its program starts.

## Results

| Panes | Fill | Quit, off | Quit, on | Start, off | Start, on | Files |
|---|---|---|---|---|---|---|
| 1 | half | 0.9 ms | 7.9 ms | 9.3 ms | 14.9 ms | 0.7 MB |
| 1 | full | 0.8 ms | 13.1 ms | 8.4 ms | 17.1 ms | 1.4 MB |
| 10 | half | 1.0 ms | 61.8 ms | 28.6 ms | 71.7 ms | 7.3 MB |
| 10 | full | 1.1 ms | 122.5 ms | 33.4 ms | 93.3 ms | 13.7 MB |
| 50 | half | 1.5 ms | 123.0 ms | 152.7 ms | 298.1 ms | 12.8 MB |
| 50 | full | 1.5 ms | 110.9 ms | 158.3 ms | 300.0 ms | 11.4 MB |

Empty panes keep no file, and cost nothing at either end.

- **A full pane costs some 12 ms at the quit, 9 ms at the start and 1.4 MB on disk.** A pane full at the default limit holds some 12 000 rows of 100 columns, whose bytes of text and styles are some seven times fewer than its pages'.
- **The panes' shared budget bounds the cost whatever their number.** Fifty panes keep no more than ten full ones, as `[scrollback] budget` takes the oldest rows of every pane past its 100 MiB: some 12 MB of files, 110 to 123 ms at the quit.
- **The quit stays within its 300 ms**, at 123 ms at most.
- **The start reads the panes back on the main thread, before the first frame**: 60 ms more for ten full panes, and 145 ms for fifty, each terminal reading its rows before its program starts. It costs nothing unless `restore-scrollback` is on, which it is not by default, and the startup's budget, measured without windows to restore, holds.
- **No compression is needed**: the files take some 14 MB at most of the default budget, and are removed as their panes read them.
