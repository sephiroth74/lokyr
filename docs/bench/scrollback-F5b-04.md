# The restored scrollback's cost at the quit and the start

Measured on 22 November 2026 on an M5 Pro on AC power, the load average 3.1 to 3.3. The cost is the time that keeping and reading back the panes' scrollback adds to the quit and to the next start, and the bytes of the files that it keeps. The measure is the app's own, [`scrollback_costs`](../../crates/app-core/src/app/tests/restore.rs), a release build of the app over the platform's fake, whose results are in [results.txt](scrollback-F5b-04/results.txt).

## Method

- **The panes**: 1, 10 and 50 tabs of a pane each, `cat` their program, of the default config: 100 columns, `[scrollback] limit` of 10 MiB a pane and `budget` of 100 MiB that the panes share.
- **The fill**: empty; half full, 6000 lines of 94 cells of color and bold; full, 20 000 such lines, past the limit, so that each pane keeps some 12 000 rows.
- **The quit**: the windows kept, `keep_windows`, timed, with `restore-scrollback` off and on, so that the difference is the scrollback's own: each pane archived within its limit and its file written.
- **The start**: the next start of the windows kept, timed until every pane is made, off and on, which is when the first frame can draw; on, the time until every pane shows its rows too, from the same start.
- **Two designs of the start**: first each pane's file read and its rows read back on the main thread, before its program started; then, as the first measures found that cost before the first frame, each pane's file read and its rows read back on the pane's own thread, a slice at a time, before the program's first output, which waits meanwhile.

## Results

Of the rows read back on each pane's own thread, as the app does:

| Panes | Fill | Quit, off | Quit, on | Start, off | Start, on | Rows shown | Files |
|---|---|---|---|---|---|---|---|
| 1 | half | 0.5 ms | 6.9 ms | 7.6 ms | 7.4 ms | 11.3 ms | 0.7 MB |
| 1 | full | 0.6 ms | 12.6 ms | 7.6 ms | 7.6 ms | 12.8 ms | 1.4 MB |
| 10 | half | 0.9 ms | 64.7 ms | 26.6 ms | 33.4 ms | 36.3 ms | 7.3 MB |
| 10 | full | 1.2 ms | 115.6 ms | 33.0 ms | 40.1 ms | 43.0 ms | 13.7 MB |
| 50 | half | 1.3 ms | 122.0 ms | 146.9 ms | 183.5 ms | 185.5 ms | 12.8 MB |
| 50 | full | 1.5 ms | 111.2 ms | 158.2 ms | 188.8 ms | 189.2 ms | 11.4 MB |

Of the rows read back on the main thread, the first design:

| Panes | Fill | Start, off | Start, on |
|---|---|---|---|
| 1 | full | 8.4 ms | 17.1 ms |
| 10 | full | 33.4 ms | 93.3 ms |
| 50 | full | 158.3 ms | 300.0 ms |

Empty panes keep no file, and cost nothing at either end.

- **A full pane costs some 12 ms at the quit and 1.4 MB on disk, and nothing before the first frame.** Its rows show some 5 ms after the panes are made, a frame later. A pane full at the default limit holds some 12 000 rows of 100 columns, whose bytes of text and styles are some seven times fewer than its pages'.
- **The panes' shared budget bounds the cost whatever their number.** Fifty panes keep no more than ten full ones, as `[scrollback] budget` takes the oldest rows of every pane past its 100 MiB: some 12 MB of files, 110 to 123 ms at the quit.
- **The quit stays within its 300 ms**, at 123 ms at most.
- **Reading the rows back on the panes' own threads takes most of the start's cost away**: on the main thread, ten full panes started 60 ms later and fifty 145 ms later, of the files read, some 6 ms a full one, and the rows read back. On the panes' threads, ten full panes start 7 ms later, and fifty 31 to 37 ms later, as the panes' programs start while the other panes read their rows, on the same cores and of a process that grows. It costs nothing unless `restore-scrollback` is on, which it is not by default, and the startup's budget, measured without windows to restore, holds.
- **No compression is needed**: the files take some 14 MB at most of the default budget, and are removed as their panes read them.
