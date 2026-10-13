# VT core

`vt-core` does two translations:
- it turns the bytes coming from the PTY into terminal state (grid, cursor, modes);
- it turns keys and mouse events into the bytes sent back to the PTY.

It knows nothing about windows, GPUs or plugins, so it can be tested headless and fuzzed ([ADR 0007](adr/0007-own-vt-core.md)). Three crates sit around it: `pty` (child processes), `session` (I/O thread and snapshots) and `search` (search over the grid).

## Goals

- **xterm compatibility** for everything real programs use (vim and Neovim, tmux, htop, less, fzf, modern shells), verified with esctest and vttest.
- **Predictable memory**: about 8 bytes per cell, and a scrollback limited in bytes.
- **Headless throughput** at least on par with `alacritty_terminal`.
- **No allocations on the hot path**, i.e. while printing text.

## Parser

- **DEC/ANSI state machine**:
  - Paul Williams' model, the same one `vte` uses, implemented with a table computed at compile time.
  - States: `ground`, `escape`, `escape_intermediate`, `csi_entry`, `csi_param`, `csi_intermediate`, `csi_ignore`, `dcs_entry`, `dcs_param`, `dcs_intermediate`, `dcs_passthrough`, `dcs_ignore`, `osc_string`, `osc_ignore`, `sos_pm_string`, `apc_string`, `apc_ignore`, `string_escape`.
  - Changes to the model:
    - C1 controls are recognized only in their 7-bit form, `ESC` followed by a byte, as in other UTF-8 terminals. Bytes from 0x80 up are UTF-8 in text, payload in OSC, DCS and APC strings, and ignored inside sequences.
    - Williams' `sos_pm_apc_string` is split in two: APC strings reach the handler, while SOS and PM strings are dropped.
    - `string_escape` follows the `ESC` that ends a string and consumes the `\` of ST, so the handler does not see it as an escape sequence.
    - `osc_ignore` and `apc_ignore`, like Williams' `dcs_ignore`, consume the rest of a string past its size limit.
    - Colons separate sub-parameters, BEL also ends OSC strings as in xterm, and DEL is ignored in text.
  - A sequence keeps at most 2 intermediates, its private marker included. No known sequence has more, and one that does is ignored.
- **Why not `vte`**: it discards the contents of APC strings, which kitty graphics needs.
- **Streamed payloads**:
  - OSC, DCS and APC payloads are delivered in pieces (start, chunks, end). The parser buffers nothing: each run of payload bytes in a read is one chunk. The terminal routes the pieces to handlers registered per type.
  - Size limits are configurable: 64 KiB for each type of string, and more for the OSC commands that need it, such as 1 MiB for OSC 52 (the clipboard). The parser reads the OSC number, the digits before the first `;`, to pick the limit. The bytes up to that `;` count against the general limit and the whole string against the command's, so the outcome does not depend on how reads split the string.
  - A string that would pass its limit ends at once as `TooLong`, so the handler can free what it holds. The parser consumes the rest without delivering it, so memory stays constant whatever the payload's size.
  - OSC strings end with ST or BEL, DCS and APC strings with ST, and CAN or SUB cancel any of them. A terminator split across reads is still recognized.
- **CSI parameters** (DCS parameters follow the same rules):
  - Up to 32 values, sub-parameters included: colons separate sub-parameters (e.g. `4:3` for curly underline, `38:2::r:g:b`), semicolons separate parameters.
  - Past the 32nd value, the values are dropped and the sequence is still dispatched (`vte` ignores such a sequence).
  - An empty value reads as 0, so `CSI m` has one parameter. Handlers read most parameters with a default that also replaces 0: `CSI A`, `CSI 0 A` and `CSI 1 A` are the same. A value too large for 16 bits reads as 65535.
  - A private marker (`?`, `>`, `<`, `=`) is valid only as the first byte. Elsewhere it voids the sequence, which is then consumed without being dispatched.
- **UTF-8**:
  - Incremental decoding, so a character can be split across reads.
  - Each maximal subpart of an invalid sequence becomes one U+FFFD, as the Unicode Standard recommends and as Rust's lossy decoding and web browsers do. A character cut short becomes one U+FFFD. Overlong forms, surrogates and values past U+10FFFF become one per byte.
  - C1 controls encoded in UTF-8 (U+0080 to U+009F) are dropped.
- **Fast path**:
  - A SIMD loop finds the first byte outside `0x20..=0x7E`. It uses `core::arch`: NEON on aarch64, SSE2/AVX2 on x86_64, with a scalar fallback.
  - Runs of printable ASCII are written straight into the row's cells, respecting margins, autowrap and insert mode.
  - Only control characters, escapes and multi-byte UTF-8 go through the state machine.
- **Interface to the state**: the parser calls a trait (`Handler`) that the terminal state implements. In tests, an action recorder implements it too, which is handy for differential testing.

## Terminal state

- **Screens**: primary (with scrollback) and alternate (without).
- **Cursor**: position, *pending wrap*, style (DECSCUSR), save and restore (DECSC/DECRC, mode 1048).
- **Modes and geometry**:
  - ANSI and DEC private modes, tab stops;
  - vertical and horizontal scroll regions (DECSTBM, DECSLRM with DECLRMM).
- **Character sets**: G0–G3 with SO/SI, plus DEC Special Graphics.
- **Style and colors**: current style (SGR), 256-color palette, dynamic colors (OSC 4/10/11/12), title stack.
- **Semantics**: current hyperlink (OSC 8) and current semantic zone (OSC 133: prompt, input, output).
- **Keyboard**: the kitty keyboard flag stack, kept separately per screen.
- **Other modes**: mouse, bracketed paste, focus events, synchronized output (2026), grapheme clustering (2027).

## Paged grid

The format is inspired by Ghostty's *page list*.

### Cell (64 bits)

| Bits | Field |
|---|---|
| 0–1 | Content tag: codepoint, codepoint + grapheme, background-only from palette, background-only RGB |
| 2–25 | Content: codepoint (21 bits), palette index (8 bits) or RGB color (24 bits) |
| 26–41 | Style id (16 bits) into the page's style table |
| 42–43 | Width: narrow, wide, spacer-tail, spacer-head |
| 44 | Protected (DECSCA) |
| 45 | Has a hyperlink (the id lives in the page's table) |
| 46–47 | Semantics: output, prompt, input |
| 48–63 | Reserved (image placeholders, future use) |

- **Empty cells**: an empty cell is all zeros, U+0000 in the default style, so a new page is empty without being written.
- **Background-only cells**: cells that carry only a background color (for example after an erase with *background color erase*) don't use up a style.
- **Multi-codepoint graphemes**: emoji ZWJ sequences and combining characters live in a per-page table, keyed by cell position.

### Row

Each row has compact metadata:
- *soft-wrapped*, i.e. continued on the next row;
- continuation of a previous row;
- start of a prompt (OSC 133);
- dirty, i.e. needs redrawing;
- "contains graphemes / hyperlinks / images / styles" flags, so hot paths can skip work.

### Page

- **Layout**: a fixed-size block of memory, starting at 512 KiB (to be tuned). It holds a header, rows, cells, a style table (deduplicated and reference-counted), a grapheme table and a hyperlink table.
- **Growth**: when a table fills up, the page is rebuilt with more capacity.
- **Pool**: pages come from a pool and go back to it. There is no `malloc`/`free` while output scrolls.

### Page list and scrollback

- **Structure**: a doubly linked list. The last pages form the active screen; the earlier ones form the scrollback.
- **Byte-based limits**: one per pane (10 MB by default) and one global limit across all panes. Past the limit, the oldest pages return to the pool.
- **Cold-page compaction**:
  - Short lines waste space in fixed-width pages. So a few screens away from the viewport, rows are rewritten in a variable-length format that drops trailing blank cells.
  - A compacted page returns to the normal format when it is needed again (scrolling, search).
  - lz4 compression on top of compaction is deferred to F7.
- **Pins**:
  - Stable `(page, row, column)` references, registered with the list and updated on scroll, eviction, reflow and compaction.
  - Used by selection, OSC 133 marks, search results, images and link hover.
  - When a pin's page is evicted, the pin becomes "lost", never a dangling pointer.

### Reflow

- **On column changes**, logical lines (sequences of *soft-wrapped* rows) are rewrapped into new pages. A wide character that straddles the edge produces a *spacer-head*.
- **Cursor and pins** are remapped.
- **Alternate screen**: resized without reflow, as xterm does.
- **Cost and correctness**: cost is linear in the scrollback size. Property tests check that shrinking and then widening again loses no content.

## Unicode

- **Tables generated from the UCD** by `xtask ucd`, for a Unicode version pinned in the repo. They cover:
  - East Asian Width, Emoji_Presentation, Extended_Pictographic;
  - combining and *default ignorable* characters;
  - Grapheme_Cluster_Break.
  - `unicode-width` is not enough, because it does not match `wcwidth`.
- **`unicode` mode (default)**: grapheme segmentation per UAX #29, with the width computed for the whole cluster (ZWJ sequences and VS16 → 2, VS15 → 1).
- **`legacy` mode**: one codepoint at a time, like `wcwidth`, for programs that compute widths themselves.
- **Mode 2027**: lets programs query and switch the mode.

## Input

- **Keyboard**:
  - legacy xterm encoding, with `modifyOtherKeys` where needed;
  - the **kitty keyboard protocol** with every flag: disambiguate, event types, alternate keys, all keys as escapes, associated text.
  - The kitty flags live on a per-screen stack (`CSI > u`, `CSI < u`, `CSI = u`, `CSI ? u`).
- **macOS specifics**:
  - Option can act as Meta, configurable separately for the left and right keys;
  - dead keys are supported;
  - IME composition text is never sent to the PTY.
- **Mouse**: X10, 1000, 1002, 1003, SGR (1006), SGR-pixels (1016), alternate scroll (1007).
- **Paste**: bracketed paste (2004), with the contents sanitized ([SECURITY.md](SECURITY.md)).
- **Focus events**: mode 1004.

## PTY and processes (`pty`)

- **Spawning**:
  - `openpty`, then **fork+exec** through `std::process::Command`.
  - An async-signal-safe `pre_exec` runs `setsid()` and `ioctl(TIOCSCTTY)`, and restores the signal mask and handlers.
  - We do not use `posix_spawn`, because it cannot set the controlling terminal.
- **Login shell**: on macOS, launched like Terminal.app does (`/usr/bin/login -flp $USER <shell>`). On Linux, the shell is started with `argv[0]` prefixed by `-`.
- **Environment**:
  - `TERM`, `COLORTERM=truecolor`, `TERM_PROGRAM=saetta`, `TERM_PROGRAM_VERSION`, `SAETTA_RESOURCES_DIR`.
  - A UTF-8 locale if none is set ([SECURITY.md](SECURITY.md#macos-environment)).
  - `TERMINFO` is set only when `TERM=xterm-saetta`.
- **Resize**: `TIOCSWINSZ` with sizes in both cells and pixels; sixel and kitty graphics need the pixels. Resizes are coalesced while a divider is being dragged.
- **Process exit**: EOF on the master plus `waitpid`. What happens next is configurable: close the pane, or show the exit code.
- **Portable API**: the public interface (reader, writer, resize, child) exposes no `RawFd`. A future ConPTY implementation for Windows will therefore not change any callers.

## Session (`session`)

- **I/O thread**: one per pane. It reads slices of at most ~64 KiB and feeds them to the parser under the grid lock, releasing the lock fairly after each slice.
- **Write queue**: collects input (keys, pastes in chunks, replies to terminal reports).
- **Renderer snapshot**: under the lock, it copies only the visible dirty rows and the styles they use.
- **Independence from the main thread**: the I/O thread never waits for it. If the renderer falls behind, it jumps straight to the latest state.

## Terminfo

- **Capabilities of `xterm-saetta`**: it derives from `xterm-256color` and adds:
  - `Tc`/`RGB` (truecolor);
  - `Smulx` (styled underlines) and `Setulc` (underline color);
  - `Ss`/`Se` (cursor style);
  - `Sync` (mode 2026);
  - `BD`/`BE`/`PS`/`PE` (bracketed paste);
  - `Ms` (OSC 52);
  - `fsl`/`tsl`/`hs` (title) and `XT`.
- **Build and install**: compiled at build time with `tic -x` from a recent ncurses (the one shipped with macOS is very old). It is bundled with the app: `Contents/Resources/terminfo` on macOS, `share/terminfo` on Linux.
- **Default `TERM`**: `xterm-256color`, because remote hosts usually don't know `xterm-saetta`. The default will change once the ssh integration (F5a) can install the terminfo on remote hosts or fall back automatically.

## Supported protocols

The "Phase" column shows the [roadmap](ROADMAP.md) phase in which support lands.

| Group | Sequences | Phase |
|---|---|---|
| C0 controls | BEL, BS, HT, LF, VT, FF, CR, SO, SI | F1 |
| ESC | DECSC/DECRC (ESC 7/8), IND, NEL, HTS, RI, RIS, DECKPAM/DECKPNM, G0–G3 charset designation, DECALN | F1 |
| Cursor | CUU, CUD, CUF, CUB, CNL, CPL, CHA, CUP, HVP, VPA, VPR, HPA, HPR, CHT, CBT, DECSCUSR | F1 |
| Editing | ICH, DCH, IL, DL, ECH, ED, EL, DECSED, DECSEL, REP, SU, SD, DECSTBM, DECSLRM, DECSCA, TBC | F1 |
| SGR | 0–9, 21–29, 30–37, 90–97, 38;5 and 38;2 (also with `:`), 39, 40–47, 100–107, 48, 49, 4:0–4:5 (underline styles), 53/55 (overline), 58/59 (underline color) | F1 |
| ANSI modes | IRM (4), LNM (20) | F1 |
| DEC modes | 1 (DECCKM), 5 (DECSCNM), 6 (DECOM), 7 (DECAWM), 12 (blink), 25 (DECTCEM), 45 (reverse wrap), 47/1047/1049 (alternate screen), 1048, 69 (DECLRMM), 1000/1002/1003/1006/1016 (mouse), 1004 (focus), 1007 (alternate scroll), 1036 (Meta sends ESC), 2004 (bracketed paste), 2026 (synchronized output), 2027 (grapheme clustering) | F1 |
| Reports | DA1, DA2, DA3, DSR (5n, 6n), DECRQM, DECRQSS, XTVERSION, XTGETTCAP; XTWINOPS 14t/16t/18t (sizes) and 22t/23t (title stack). Title reporting (21t) is **disabled** by default | F1 |
| OSC | 0/1/2 (title), 4/104 (palette), 7 (cwd), 8 (hyperlinks), 10/11/12 and 110/111/112 (dynamic colors), 52 (clipboard: write yes, read no by default), 133 (semantic prompts), 9 and 777 (notifications), 9;4 (progress), 6;1 (tab color, iTerm2) | F1–F3 |
| OSC 1337 (iTerm2) | SetUserVar, CurrentDir, RemoteHost and ShellIntegrationVersion through a plugin; `File=` (images) in F5c | F5 |
| kitty | Keyboard protocol (F1–F2); graphics via APC `G` (F5c); OSC 99 notifications (F5) | F1–F5 |
| Sixel | DCS `q` | F5c |

Sequences not listed here are ignored. Debug builds also log them, so the list of missing sequences comes from real sessions.
