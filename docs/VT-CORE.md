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
  - A loop finds the first byte outside `0x20..=0x7E`, 16 bytes at a time, as two 64-bit words. The arithmetic that marks the bytes outside the range also finds the first of them (SWAR, SIMD within a register). It is safe code whose speed does not depend on the compiler vectorizing it, and it matched hand-written NEON intrinsics in the measurements, so `vt-core` has no `unsafe` code.
  - Runs of printable ASCII are written straight into the row's cells, with one page lookup and one style acquisition per stretch of a row, respecting autowrap and insert mode. ASCII takes no Unicode lookup: each character is one cell wide, starts its own cluster unless a prepended character comes before it, and is already in every normal form.
  - A run goes through the slow path, character by character, while something that changes how ASCII prints is active: a character set other than ASCII, protected cells, left and right margins, or an open hyperlink, until the fast path learns it. A property test checks that both paths leave the terminal in the same state.
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
- **Printing**, the path of every character of text:
  - A codepoint either joins the cluster printed before it, as the segmenter decides in `unicode` mode or a zero width does in `legacy` mode, or starts a new one at the cursor. A joining codepoint goes to its cluster's cell even while the wrap is pending, and the cluster stays open until the cursor moves or a control comes; other sequences, such as SGR, leave it open.
  - A codepoint of no width with nothing to join, such as a mark right after a carriage return, joins the character left of the cursor if there is one, and is dropped otherwise.
  - When VS16 widens a cluster, its cell takes a spacer tail and the cursor moves on one column; VS15 does the reverse. A cluster at the last column has no room to widen and stays narrow.
  - Writing over half of a wide character empties its other half. A wide character at the last column leaves a spacer head there and wraps, or, without autowrap, goes one column back.
  - Insert mode moves the rest of the row right, cells with their graphemes and links, and empties a wide character that would lose its second half off the row.
  - Wrapping marks the row soft-wrapped and the next one as its continuation, for reflow.
- **C0 controls**: BEL rings; BS moves left and, with the wrap pending, leaves the last column; HT goes to the next tab stop, every 8 columns by default, or to the last column; LF, VT and FF go down a row and scroll at the bottom; CR goes to the first column; SO and SI select G1 and G0. The others do nothing.

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
- "contains graphemes / hyperlinks / images / styles" flags, so hot paths can skip work. They mean "may contain": they are reset when the row is cleared, not when the last such cell goes.

The metadata fits in one byte.

### Page

- **Layout**: a fixed memory budget, 512 KiB to start with (to be tuned). For a given width, it sets how many rows fit next to the tables: 781 at 80 columns, 312 at 200. The page holds a header, the row metadata, the cells, a style table (deduplicated and reference-counted), a grapheme table and a hyperlink table. By default the tables have room for 256 styles, 128 graphemes, 32 link targets and 512 linked cells, about 23 KiB.
- **Allocation**: each part is allocated once, when the page is created, and never grows. They are separate allocations rather than one block, which in safe Rust would mean packing every table into words; the pool keeps the guarantee that matters, no allocation while output scrolls.
- **Style table**: its capacity is set with the page's. Equal styles share an id, every cell holds a reference to its style, and the last reference frees the id. The default style, id 0, is not counted, so unstyled text costs nothing. A lookup hashes the style packed in 128 bits, so even a style change on every character stays cheap.
- **Grapheme table**: the codepoints after the first, for the cells whose cluster has several, in a fixed arena of 4-codepoint chunks and keyed by cell. A cell becomes a grapheme when a codepoint is added to it; overwriting or clearing it frees the entry.
- **Hyperlink table**: the OSC 8 targets, id and URI, in an arena of 32-byte chunks, deduplicated and reference-counted like the styles, and a map from each linked cell to its link. The terminal's pen holds a reference to the link it writes, and lets go even when a write fails.
- **Shared machinery**: the tables share one open-addressing index and one allocator of ids. A lookup stops after one turn of the index, which never happens in a sound one, so a bug fails instead of hanging the terminal.
- **Growth**: when a table fills up, the page is rebuilt with more capacity, so it becomes larger than a standard page. The error says which table is full, and `Capacity::grown_for` doubles that one. Every table keeps its ids, so the cells are copied as they are.
- **Pool**: pages come from a pool and go back to it. There is no `malloc`/`free` while output scrolls.

### Page list and scrollback

- **Structure**: a doubly linked list of indexes into a slab, oldest first, rather than of pointers. The last rows form the active screen; the earlier ones form the scrollback. Scrolling adds an empty row at the bottom, in a new page when the last one is full.
- **Page ids**: a page's id carries the generation of its slot, so the id of a page that left the list goes stale instead of naming another page.
- **Eviction**: when a new page would pass the byte limits below, the oldest page leaves first: it is emptied and goes back to the pool, which hands it out again, so the list keeps no spare page and scrolling at the limit allocates nothing. A page grown past the standard size is freed instead. The oldest page leaves only when the rows after it cover the screen.
- **Byte-based limits**:
  - One per pane, 10 MiB by default, over the pages of the list and of its pool.
  - A global budget shared by all panes, 100 MiB by default, counted atomically since each pane has its own I/O thread.
  - A pane that needs a page past either limit makes room with its own oldest pages: a busy pane never takes the scrollback of an idle one.
  - A screen always gets its pages, even past the limits. When a page grows, or another pane's screen passes the budget, the pane frees its oldest pages, at its next new page, until it is within the limits again. So only the pages that screens need can pass them.
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

- **Tables generated from the UCD** by `cargo xtask ucd`, for Unicode 18.0.0, pinned in the repo. They cover:
  - East Asian Width, Emoji_Presentation, Extended_Pictographic;
  - the marks (Mn, Me, Mc), the format characters (Cf) and the *default ignorable* ones;
  - Grapheme_Cluster_Break, and Indic_Conjunct_Break for rule GB9c of UAX #29;
  - the East Asian Width of Unicode 8, for `unicode-widths = "8"`.
  - `unicode-width` is not enough, because it does not match `wcwidth`.
- **The generated file** (`crates/vt-core/src/unicode/tables.rs`) is committed, with the name and date of every UCD file it comes from. `just ucd-check` downloads the files again and checks that they give the same file. The range tables are searched by binary search, one property at a time. Printing reads a table of three stages instead, generated from the same data, which gives in one lookup the eight properties that printing reads: both East_Asian_Widths, Grapheme_Cluster_Break, Indic_Conjunct_Break, Extended_Pictographic, and whether the code point is a mark, a format character or default-ignorable. The generator picks the block size that takes the fewest bytes: about 34 KiB for Unicode 18. The range tables stay as the reference that a test compares the table with, for every code point.
- **`unicode` mode (default)**: grapheme segmentation per UAX #29, with the width computed for the whole cluster.
  - The segmenter takes one codepoint at a time, as text arrives, and passes the conformance test of the UCD, `GraphemeBreakTest.txt`, which `cargo xtask ucd` copies next to the tables.
  - Unicode 18 changed rule GB9c: a consonant joins after a linker and extenders, with no consonant needed before the linker.
  - A cluster is as wide as its first codepoint, except that emoji presentation changes it: VS16 after a pictographic or a keycap base gives 2, VS15 gives 1, and a flag or an emoji ZWJ sequence gives 2. With the Unicode 8 tables, which knew no emoji presentation, a cluster is always as wide as its first codepoint.
- **`legacy` mode**: one codepoint at a time, like `wcwidth`, for programs that compute widths themselves. A codepoint of no width joins the cell before it.
- **The width of a codepoint**, as Markus Kuhn's `wcwidth` counts it: none for controls, nonspacing and enclosing marks, format and default-ignorable characters, and the Hangul vowels and final consonants that combine into syllables, but one for the soft hyphen; two for wide and fullwidth characters, and for the ambiguous ones when they count as wide; one otherwise.
- **Mode 2027**: lets programs query and switch the mode.
- **Width tables that match the programs'**: a program that counts a character's width differently from the terminal misplaces the cursor and the text after it. Two options make the terminal count as the programs do, in either mode:
  - `unicode-widths`: the tables of the Unicode version that Saetta ships, or those of Unicode 8, for programs that predate Unicode 9. Unicode 8 gives most emoji one cell, and Unicode 9 gave them two. `xtask ucd` generates the Unicode 8 table too.
  - `ambiguous-width`: the cells that East Asian Ambiguous characters take (①, ※, ○, Greek and Cyrillic letters…): 1 by default, 2 for CJK setups whose programs count them as two, like vim's `ambiwidth=double`. It is an explicit option, not a guess from the locale, because it must match the programs.
  - A change applies to new output; the cells already written keep their widths.
- **Normalization**: the `normalization` option puts the text in a Unicode normal form as it is printed: none (the default), NFC, NFD, or HFS+, Apple's variant of NFD, which leaves the ranges U+2000–U+2FFF, U+F900–U+FAFF and U+2F800–U+2FAFF alone. NFC shows the decomposed names of macOS files composed, Hangul included, and a composed character takes no grapheme entry. Each cluster is normalized as it is built, and text already in the normal form passes a quick check without being rewritten. The data comes from the `unicode-normalization` crate, at Unicode 17 while the tables are at 18: canonical decompositions never change for existing characters, so the gap covers at most the new characters of Unicode 18 that have one.

### Settings

The `[terminal]` section of the config:

```toml
[terminal]
term = "xterm-256color"   # what TERM tells programs: xterm-saetta, xterm-256color, xterm, ansi, vt100, vt220, dtterm, linux…
unicode-widths = "current" # current | 8
ambiguous-width = 1       # 1 | 2
normalization = "none"    # none | nfc | nfd | hfs+
```

`term` changes only what programs are told, never the emulation, which stays xterm's. The other values describe subsets of what xterm does, so they work as they are.

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
  - `TERM`, from the `term` option ([Settings](#settings)), `COLORTERM=truecolor`, `TERM_PROGRAM=saetta`, `TERM_PROGRAM_VERSION`, `SAETTA_RESOURCES_DIR`.
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
- **Encodings** (F5b): the terminal works in UTF-8 only. Other encodings, chosen per profile, come after the MVP as a conversion in the session, with `encoding_rs`: the bytes from the PTY become UTF-8 before the parser, and the input goes back in the program's encoding. Latin-1, windows-125x, Shift_JIS, EUC-JP, EUC-KR, GB18030, Big5 and ISO-2022-JP are among them. C1 controls stay 7-bit only.

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
- **Default `TERM`**: `xterm-256color`, because remote hosts usually don't know `xterm-saetta`. The `term` option chooses another ([Settings](#settings)). The default will change once the ssh integration (F5a) can install the terminfo on remote hosts or fall back automatically.

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
