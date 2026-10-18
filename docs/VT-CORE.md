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
  - Size limits are configurable: 64 KiB for each type of string, and more for the OSC commands that need it, such as 1 MiB for OSC 52 (the clipboard). The parser reads the OSC number, the digits before the first `;`, to pick the limit. The bytes up to that `;` count against the general limit and the whole string against the command's, so the outcome does not depend on how reads split the string. New limits apply from the next string: one under way keeps those it started with, the limit of a number that comes later included.
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
  - A run goes through the slow path, character by character, while a character set other than ASCII is in use, which gives the characters other meanings, or a hyperlink is open, which takes each cell into its page's table. Protection and the semantic zone are bits of the cell that the fast path writes too: with shell integration, nearly all output is in a zone. A property test checks that both paths leave the terminal in the same state.
  - Only control characters, escapes and multi-byte UTF-8 go through the state machine.
- **Interface to the state**: the parser calls a trait (`Handler`) that the terminal state implements. In tests, an action recorder implements it too, which is handy for differential testing.

## Terminal state

- **Screens**: primary (with scrollback) and alternate (without).
- **Cursor**: position, *pending wrap*, style (DECSCUSR), save and restore (DECSC/DECRC, mode 1048).
  - Each screen saves its own cursor. DECSC saves the position with its pending wrap, the pen, the character sets and origin mode; DECRC with nothing saved puts the cursor at the top left, with the default pen and character sets, and origin mode off.
  - Moves follow xterm: the relative ones (CUU, CUD, CUF, CUB and their kin) stop at the margins when they start within them, and at the edges of the screen otherwise; the absolute ones (CUP, CHA, VPA) count from the top left of the screen, or of the margins in origin mode, and stay within them. Every move ends the cluster being printed and leaves no wrap pending.
  - DECSCUSR 0 gives the cursor's shape and blinking back to the configuration (see [the cursor options](UI-AND-FEATURES.md#cursor)); the other values override it.
- **Full reset (RIS)**: the terminal goes back to how it was made, its scrollback cleared as `alacritty_terminal` clears it, but it keeps its configuration: size, scrollback limits, widths, normalization, `TERM`, what programs may read back (the title, the clipboard) and the schemes that links may have. It keeps the working directory too, which describes the shell rather than the terminal.
- **Modes and geometry**:
  - ANSI and DEC private modes, tab stops;
  - vertical and horizontal scroll regions (DECSTBM, DECSLRM with DECLRMM). Without DECLRMM, `CSI s` is SCOSC, and `CSI u` without parameters SCORC.
  - The four margins make a rectangle that LF, IND, NEL and RI scroll, at the bottom and top margins, when the cursor is between the left and right ones; below the region, the last row stops LF. IL, DL, ICH, DCH, SU and SD work within the rectangle, and only a region of the whole screen sends rows into the scrollback. HT stops at the right margin and CBT at the left one, from within them. Wide characters across the edges of what moves are emptied whole.
  - **Columns**: DECIC and DECDC insert and delete columns at the cursor's, in the rows of the scroll region, as ICH and DCH do in each, and only with the cursor in the region. DECBI and DECFI move the cursor a column left or right; at the left or the right margin, within the top and bottom ones, they shift the region's columns right or left instead, a column coming in at that margin, and the cursor stays. Outside the margins they move it, as DEC STD 070 says, but not past the screen's edges, where they are ignored.
  - **Rectangles**: DECCRA copies one, with its cells' attributes and links, as through a buffer, so that the copy can overlap its source; DECERA erases one, as ED does, DECSERA the cells of one that DECSCA does not protect, and DECFRA fills one with a character of ASCII or Latin-1, in the pen's style. A rectangle counts from the screen's top left, or from the margins' in origin mode, and stays within the screen or the margins; a wide character that its edges split is emptied whole, and the cursor does not move. The page parameters name the screen's only page.
- **Character sets**: G0–G3, each designated ASCII, DEC Special Graphics or British; the locking shifts SO, SI, LS2 and LS3, and the single shifts SS2 and SS3. A designation of a set the terminal does not know leaves the slot as it was. Only printable ASCII changes meaning: DEC Special Graphics draws lines and boxes from `_` to `~`, as the VT100 did, and its line characters are ambiguous-width.
- **Style and colors**: current style (SGR), 256-color palette, special colors, dynamic colors (OSC 4/5/10/11/12), title stack.
  - The colors start as xterm's until the embedder gives the theme's; OSC 4, 5, 10, 11 and 12 change them, the resets (104, 105, 110 to 112) and RIS bring back the theme's, and `?` asks for one, which gets `rgb:rrrr/gggg/bbbb` back, ended as the query was. Colors are read as X11 writes them, `rgb:` with one to four digits a component and `#` with three to twelve; names of colors are not known.
  - **Special colors**: xterm's five, of bold, underlined, blinking, reverse and italic text, which themes call the bold and the underline colors, and which start as the foreground. OSC 5 and 105 number them from 0, and OSC 4 and 104 from 256, after the palette; OSC 104 without numbers resets them with the palette.
  - SGR follows xterm and kitty: 21 is a double underline, 22 ends both bold and faint, `4:0` to `4:5` choose the underline, and 58 and 59 set the underline color. Extended colors read `38;5;n` and `38;2;r;g;b`, and with colons `38:5:n`, `38:2:id:r:g:b` and `38:2:r:g:b`, which many programs write without the color-space id. A color it cannot read is skipped, taking only the values it read, as in `vte`. With a private marker, `CSI … m` is not SGR (`CSI > … m` is xterm's modifyOtherKeys).
- **Semantics**: the hyperlink and the semantic zone of the text printed next.
  - **Hyperlinks** (OSC 8): the text printed while a link is open is part of it, both halves of a wide character too, in its page's hyperlink table. The `id` parameter, among those that `:` separates, ties the cells of one link together. A URI longer than 2083 bytes, an `id` longer than 250, the limits of [the specification](https://gist.github.com/egmontkob/eb114294efbcd5adb1944c9f3cb5feda), a URI or `id` that is not UTF-8 or holds control characters, or a scheme that the configuration does not allow closes the link instead; `http`, `https`, `mailto` and `file` are allowed by default, in any case. The link keeps its buffers from one to the next.
  - **Semantic prompts** (OSC 133, FinalTerm's): `A` starts a prompt and marks its row, `B` starts the command that the user types, `C` its output, and `D` ends it, back to output. The text printed next is in that zone, which each cell keeps; a cluster that widens gives its second half its own zone. The options after the letter, such as `aid=` and `D`'s exit status, are not read yet.
  - RIS closes the link and goes back to output.
- **Keyboard**: the flags of the [kitty keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/#progressive-enhancement), in a stack for each screen, as kitty keeps them.
  - `CSI > flags u` pushes flags, `CSI < n u` pops `n` entries, and popping the last one resets the flags; `CSI = flags ; mode u` replaces, adds or removes flags in the entry on top, which is there even before a push; `CSI ? u` reports them.
  - A stack holds eight entries, and a push past them drops the oldest. Only the five flags that the protocol defines are kept, so the report tells which ones the terminal knows.
  - Switching screens switches stacks, and RIS empties both.
- **Other modes**: mouse, bracketed paste, focus events, synchronized output (2026), grapheme clustering (2027).
  - Synchronized output holds the drawing of the screen until the program ends the update. Each `CSI ? 2026 h` that starts an update, while none is under way, counts as an update of its own, so that the session gives it a deadline of its own; a start during an update goes on with it. The session ends an update that takes too long, as Ghostty does. RIS ends it too.
  - The modes that change what keys, the mouse and pastes send (DECCKM, DECKPAM, LNM, mouse tracking and encoding, focus events, alternate scroll, Meta sends ESC, bracketed paste) live together, for the input encoders. Resetting any mouse tracking mode turns tracking off, as in xterm.
  - The alternate screens follow xterm: 47 switches, 1047 also erases the alternate screen on the way out, and 1049 saves the cursor and erases it on the way in, and brings the cursor back on the way out. xterm has one cursor for both screens, so the cursor goes along when they switch; each screen still keeps its own cursor for DECSC.
  - Mode 2027 switches between `unicode` and `legacy` mode, ending the cluster being printed; RIS brings back the mode the terminal was made with.
  - Reverse wraparound (45), with autowrap, lets BS and CUB go on from the right margin of the row above, when that row wraps into this one, as far as the top margin. The extended reverse wraparound (1045), xterm's mode 45 as it was until 2023, goes on whether the row above wraps or not, and from the top margin to the bottom one; within the margins, a move goes round them at once, whole turns coming back to the same cell, so that its cost does not grow with its count.
  - DECNKM (66) is the keypad's mode, which DECKPAM and DECKPNM set too; smooth scrolling (DECSCLM, 4) is kept for whoever draws the screen, which `Terminal::smooth_scroll` and the snapshot tell; the screen itself scrolls at once.
  - **DECSTR**, the soft reset, does as xterm's does: the cursor shows and stays where it is, its shape and blinking back to the configuration's; insert mode, origin mode and both reverse wraparounds go off, and autowrap on; the cursor keys and the keypad send their normal codes, and modifyOtherKeys goes off; the margins become the screen's, with DECLRMM off; the character sets, the style and DECSCA go back to their defaults, and the palette to the theme's; and the cursor that DECSC saved goes. The mouse's modes, bracketed paste and the dynamic colors stay, as in xterm. `is2` and `rs2` of the terminfo entry send it. **DECSCL** keeps the conformance level, 61 or 62, the VT220's that DA tells, for DECRQSS, and resets softly, as the VT220 and xterm do; a higher level does nothing, as xterm refuses one above its own.
  - DECRQM replies for every mode above, 4 for the ANSI modes that xterm keeps reset for good (GATM, SRTM, VEM, HEM, PUM, FEAM, FETM, MATM, TTM, SATM, TSM, EBM) and for DECHCCM, and 0 for those the terminal does not know; replies wait in a queue that the embedder takes and writes to the PTY.
- **Printing**, the path of every character of text:
  - A codepoint either joins the cluster printed before it, as the segmenter decides in `unicode` mode or a zero width does in `legacy` mode, or starts a new one at the cursor. A joining codepoint goes to its cluster's cell even while the wrap is pending, and the cluster stays open until the cursor moves or a control comes; other sequences, such as SGR, leave it open. A cluster keeps at most 32 codepoints, more than Unicode's stream-safe format allows marks, and drops those past them, after normalization too.
  - A codepoint of no width with nothing to join, such as a mark right after a carriage return, joins the character left of the cursor if there is one, and is dropped otherwise. A prepended character of no width, such as U+0600 ARABIC NUMBER SIGN, instead starts a cluster at the cursor, which the character after it joins, as UAX #29 has it.
  - When VS16 widens a cluster, its cell takes a spacer tail and the cursor moves on one column; VS15 does the reverse, and the cell that it frees keeps the cluster's style, protection and zone. In insert mode, widening moves the rest of the row right, and narrowing moves it back, within the line. A cluster at the last column has no room to widen and stays narrow.
  - Writing over half of a wide character empties its other half. A wide character at the last column leaves a spacer head there and wraps, or, without autowrap, goes one column back.
  - Insert mode moves the rest of the row right, cells with their graphemes and links, and empties a wide character that would lose its second half off the row; a cluster that widens moves it one more cell.
  - A line ends at the right margin, unless the cursor is past it, and then at the edge of the screen; wrapping goes on at the left margin of the next row, through the scroll region. Only a wrap from the last column of the screen to the first one of another row marks the row soft-wrapped and the next one as its continuation, for reflow; below the scroll region, or right of the right margin, the text goes on in the same row and marks nothing.
- **Editing**:
  - Erased cells, and the cells and rows that inserting, deleting and scrolling bring in, take the pen's background color and nothing else (background color erase).
  - ED, EL and ECH erase every cell; DECSED and DECSEL keep the cells that DECSCA protected, which both print paths mark. A wide character that an erase, an insertion or a deletion would split goes whole, and an erase that reaches the end of a row ends its wrap.
  - ED 2 on the primary screen first moves the rows, up to the last one that holds something, into the scrollback, as `alacritty_terminal` does; ED 3 drops the scrollback.
  - IL and DL work from the cursor's row to the bottom margin, only within the scroll region, and put the cursor at the left margin, as DEC terminals and xterm do. SU and SD scroll the region, the cursor staying, and SU sends the rows that leave a region of the whole screen into the scrollback.
  - REP prints the last character given to print again, through the character sets in use, as `vte` does.
  - Every edit ends the cluster being printed; those at the cursor leave no wrap pending.
- **OSC strings** are read whole, within their size limits, and acted on when they end with ST or BEL. What the embedder has to act on goes to a queue of events: a title changed (OSC 0, 1, 2), the working directory (OSC 7, a `file:` URL), a notification (OSC 9, 777), the progress of a task (OSC 9;4), the tab's color (OSC 6;1, a component at a time), the colors changed, and the clipboard written or, when the configuration allows it, asked for (OSC 52), which the embedder answers after the user confirms. Text from programs loses its control characters. The queue keeps within limits: a change that the terminal's state shows, a title's or the colors', waits once; the tab's color, the progress and the working directory wait as their latest value, once; a write to the clipboards takes the place of the writes that wait to the same clipboards, or to some of them, made since the last read, so that the clipboards end with the program's last write and a read is answered with what was written before it, the places of the writes overwritten being closed up once they are half the queue; and the events with a payload, the notifications and the clipboard's writes and reads, take a MiB at most between two takes, past which they are dropped.
- **Reports**: replies wait in a queue that the embedder takes and writes to the PTY, and carry only what the terminal makes of its own state. The replies to queries that wait take a MiB at most, between two takes: past it, a reply is dropped whole, as some queries make replies far longer than they are, such as XTGETTCAP for `sgr` or the title read again and again. An answer to a clipboard read, which the user allowed, goes whole, and does not count against the replies' limit; it names the clipboards with the letters of the request, each once, and `s0` for none, as xterm does. An embedder with a session answers with `Session::reply_clipboard`, which writes the answer at once.
  - DA1 says VT220 with ANSI colors (`CSI ? 62 ; 22 c`), DA2 `CSI > 1 ; 10 ; 0 c`, and DA3 a unit id of zeros, as Ghostty does; XTVERSION gives the name and version.
  - DECRQSS reports SGR, DECSTBM, DECSLRM, DECSCUSR, DECSCA, DECSCL and DECSACE as the sequences that set them, and DECSSDT and DECSASD as no status line and the main display, with `1 $ r` for a valid request, as xterm does, and `0 $ r` without echoing the request otherwise.
  - **DECRQCRA**, when the configuration allows it, which it does not by default, tells the checksum of a rectangle of the screen, counted from the margins in origin mode and clamped to them, or to the screen: each cell's character, an empty cell a space and the second half of a wide one nothing, summed and negated as DEC's terminals do, in `DCS Pid ! ~ XXXX ST`. The cells' attributes, which DEC's terminals add, are left out, as xterm leaves them out with `checksumExtension`, and as esctest reads the characters alone. Test suites read the screen through it, a cell at a time; a request whose reply could not be queued scans nothing.
  - **DECDSR** tells what a terminal emulator has: DECXCPR the cursor's place, without a page, as DA's VT220 has none; no printer; locked user-defined keys, which the terminal does not have; a keyboard of unknown language; no locator; no macro space, and a macro checksum of 0; no errors; one session. DECID (`ESC Z`) answers as DA does. XTGETTCAP answers from the terminal's terminfo entry, by terminfo name, and answers `TN`, `Co` and `RGB` as xterm does ([Terminfo](#terminfo)).
  - XTWINOPS reports the size in cells, and in pixels once the embedder gives the size of a cell; it keeps a stack of ten entries, which 22 t fills with both titles, the icon's (1) or the window's (2), and 23 t pops, bringing back those asked for of those saved, as xterm does, with a `TitleChanged` event. xterm's third parameter, which reads an entry without popping it, is not read. Programs read the title back (20 t and 21 t) only when the configuration allows it, and then without its control characters.
- **C0 controls**: BEL rings; BS moves left as CUB 1 does, so that, with the wrap pending, it leaves the last column; HT goes to the next tab stop, every 8 columns by default, or to the last column; LF, VT and FF go down a row and scroll at the bottom; CR goes to the left margin, or to the first column from left of it; SO and SI select G1 and G0. The others do nothing.

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
- dirty, i.e. needs redrawing: every change of a cell marks it, and so does a change of the soft wrap or of the prompt's start, which a renderer shows, but not of the continuation, which changes with the soft wrap of the row before; a snapshot clears it as it copies the row;
- "contains graphemes / hyperlinks / images / styles" flags, so hot paths can skip work. They mean "may contain": they are reset when the row is cleared, not when the last such cell goes.

The metadata fits in one byte.

### Page

- **Layout**: a fixed memory budget, 512 KiB to start with (to be tuned). For a given width, it sets how many rows fit next to the tables: 778 at 80 columns, 312 at 200. The page holds a header, the row metadata, the cells in blocks of a row's width, the block that each row uses, a style table (deduplicated and reference-counted), a grapheme table and a hyperlink table. By default the tables have room for 256 styles, 128 graphemes, 32 link targets and 512 linked cells, about 23 KiB.
- **Allocation**: each part is allocated once, when the page is created, and never grows. They are separate allocations rather than one block, which in safe Rust would mean packing every table into words; the pool keeps the guarantee that matters, no allocation while output scrolls.
- **Style table**: its capacity is set with the page's. Equal styles share an id, every cell holds a reference to its style, and the last reference frees the id. The default style, id 0, is not counted, so unstyled text costs nothing. A lookup hashes the style packed in 128 bits, so even a style change on every character stays cheap.
- **Grapheme table**: the codepoints after the first, for the cells whose cluster has several, in a fixed arena of 4-codepoint chunks and keyed by cell. A cell becomes a grapheme when a codepoint is added to it; overwriting or clearing it frees the entry.
- **Hyperlink table**: the OSC 8 targets, id and URI, in an arena of 32-byte chunks, deduplicated and reference-counted like the styles, and a map from each linked cell to its link. The terminal holds a reference to the link while it links a cluster's cells, and lets go of it after, even when the table has no room for one of them.
- **Shared machinery**: the tables share one open-addressing index and one allocator of ids. A lookup stops after one turn of the index, which never happens in a sound one, so a bug fails instead of hanging the terminal.
- **Growth**: when a table fills up, the page is rebuilt with more capacity, so it becomes larger than a standard page. The error says which table is full, and `Capacity::grown_for` doubles that one. Every table keeps its ids, so the cells are copied as they are.
  - A table at its largest capacity does not grow: `grown_for` returns `None`, and `PageList::grow_page` an error. The page then writes without what the table has no room for: a codepoint that would join a cluster, a cell's link, or, when printing, the pen's style. A copy between pages, with `WhenFull::Drop`, takes each cell without what does not fit.
- **Pool**: pages come from a pool and go back to it. There is no `malloc`/`free` while output scrolls.

### Page list and scrollback

- **Structure**: a doubly linked list of indexes into a slab, oldest first, rather than of pointers. The last rows form the active screen; the earlier ones form the scrollback. Scrolling adds an empty row at the bottom, in a new page when the last one is full.
- **Page ids**: a page's id carries the generation of its slot, so the id of a page that left the list goes stale instead of naming another page.
- **Moving rows**: a row of the screen moves into another one, as scrolling down, inserting and deleting lines and scroll regions need. Within a page, the two rows trade their blocks of cells, so nothing is copied and the grapheme and link entries, keyed by cell, stay valid; emptying the target first costs one compare when it is empty already. `PageList::shift_rows_up` and `shift_rows_down`, which scroll regions, IL, DL, RI, SU and SD use across the whole width, empty only the rows that leave and move the others into the rows that the moves before emptied, so that scrolling a region of 38 rows at 120 × 40 runs at about 200 MiB/s. Across pages, each cell's style, grapheme and link are copied into the tables of the other page, which grows when one is full.
- **Eviction**: when a new page would pass the byte limits below, the oldest page leaves first: it is emptied and goes back to the pool, which hands it out again, so the list keeps no spare page and scrolling at the limit allocates nothing. A page grown past the standard size is freed instead, and so are the pages that ED 3 empties, since no new page is about to take them. The oldest page leaves only when the rows after it cover the screen.
- **Byte-based limits**:
  - One per pane, 10 MiB by default, over the pages of the list and of its pool.
  - A global budget shared by all panes, 100 MiB by default, counted atomically since each pane has its own I/O thread.
  - A pane that needs a page past either limit makes room with its own oldest pages: a busy pane never takes the scrollback of an idle one.
  - A screen always gets its pages, even past the limits. When a page grows, or another pane's screen passes the budget, the pane frees its oldest pages, at its next new page, until it is within the limits again. So only the pages that screens need can pass them.
- **Cold-page compaction**:
  - Short lines waste space in fixed-width pages. `PageList::compact(keep)` keeps small the pages whose rows are all `keep` rows or more above the last, and never the screen's: a cold page keeps each row's cells up to its last that is not entirely empty, one row after the other, and the rest of the page as it is, its tables and its rows' blocks with it, so that the entries keyed by cell are still right when it comes back. Its rows, ids and pins stay. A page that this would not make smaller, as with full rows, where the start of each row costs more than the empty cells save, stays as it is. On a scrollback of short lines at 80 columns, such as `ls -la`, it gives back about three quarters of the bytes; on lines of 40 characters, about two fifths.
  - `Terminal::compact_scrollback` compacts what is three screens or more above the primary screen's last row. It is for an idle terminal: compacting allocates, and a cold page that leaves the list is freed, not pooled, so output does not do it.
  - Readers read a cold page as it is: `PageList::rows_from` and `PageList::rows_before` give each row as a `ListRow`, its metadata, its kept cells followed by empty ones, and its clusters, so that reading, as search does, takes no memory and keeps the page cold. A cold page returns to the normal format only to be written: resizing and reflow bring back what they rewrite, and a taller screen the rows that it takes from the scrollback, so that the screen is never cold. Warming counts the pages' bytes again, which could take the list past its limit and a shared budget past its own, making other panes give back their scrollback, so nothing else warms. `PageList::iter_rows`, which reads pages whole, fails on a cold page rather than skipping it.
  - lz4 compression on top of compaction is deferred to F7.
- **Pins**:
  - A pin is a cell of the list, `(page, row, column)`, that the list keeps track of: `PageList::pin` puts one on a cell, `pin_position` says where the cell is now, and `unpin` lets it go. `row_at` and `row_index` go between a row's index in the list, oldest first, and its page and row.
  - A pin follows its cell. Scrolling moves no row, so pins stay; when rows or cells move, as scroll regions, IL, DL, RI, ICH, DCH and insert mode move them, their pins go along; a page that grows keeps them; reflow carries them to where their cells go, and a cold page keeps them.
  - When its cell leaves the list, with its page or the scrollback, or a move drops it, the pin is lost: `pin_position` returns `None`, never a dangling position. Erasing a cell where it is leaves its pins.
  - A page keeps its pins by row: the first pin of each row, whose slot chains it to the row's others, so that a pin is let go of at once, and a row that moves takes its pins along without the page's others being looked at: many pins in a page, as a search's results, cost nothing to the rows that move without them. A pin takes 20 bytes in the slab, and a page with pins 4 bytes for each row up to its last pinned one.
  - Pins live in a slab with generations, so a lost pin frees its slot at once and its id goes stale. Each page's node lists the pins in it, so a move looks only at the pins of the two pages involved, and a page without pins costs nothing.
  - Used by selection, OSC 133 marks, search results, images and link hover.

### Resizing and reflow

- **Rows**, `PageList::resize`: a taller screen takes its new rows from the scrollback, as xterm does, and empty ones at the bottom when the scrollback runs out; the alternate screen, whose scrollback is not shown, adds them at the bottom. A shorter screen drops the empty rows below the cursor first, then sends rows from the top to the scrollback. No text is dropped: when the cursor's row goes to the scrollback too, the cursor stays in the screen's first row, and the program redraws on the resize.
- **Columns, without reflow**: the pages are rebuilt at the new width, each row cut or extended with empty cells. A wide character that loses its second half is emptied, spacer heads go, and the rows no longer wrap; pins in the cut columns are lost, the others keep their cell. The old pages leave the byte limits at once, and go as their rows are copied, so the new pages have the room.
- **The terminal**, `Terminal::resize`: a terminal has two columns at least, as `Terminal::new` makes it, so that a wide character always fits; DECSLRM leaves two too. The primary screen reflows, the alternate one is cut. Each cursor follows its cell, and so do the cursors that DECSC saved, which are pinned for the resize; one whose text went to the scrollback comes to the top row, and one whose column was cut keeps its row, in the last column. A wrap stays pending only at the last column, and goes on in the cell after its text otherwise. The margins become the whole screen, the tab stops go on every eight columns past the old width, and the cluster being printed ends.
- **Reflow**, `PageList::reflow`: on column changes, logical lines are written again into new pages at the new width. A row goes on into the next only when it soft-wraps and the next is its continuation: rows that IL, DL, RI, SU and SD move keep their flags, which then no longer pair up, and the same rule holds for reverse wraparound.
  - A wide character that the edge would split leaves a *spacer head* in the last column and goes on in the next row; spacer heads go when lines join again.
  - The empty cells that end a line are left out, but those up to the cursor, which keeps its place in its line; a cell after a full row's text goes on in the next row, as the wrap would take it. A cursor on a spacer head goes with the wide character.
  - Cells keep their styles, graphemes and links, and the first row of a line its prompt mark. Pins follow their cells, those of a wide character's second half too; a spacer head's go with the wide character, and those on empty cells left out stay as far after the text as they were, within its last row.
  - When the text from the screen's first row to the cursor takes more rows, the empty rows below the cursor take them first, so that the text stays on screen, as iTerm2 keeps it; rows that the scrollback gains move nothing on screen. No text is dropped: when the rows below the cursor push its row into the scrollback, the cursor stays in the screen's first row. A screen with too few rows gets empty ones at the bottom.
  - The old pages leave the limits before the new ones come, as for a cut.
- **Alternate screen**: resized without reflow, as xterm does.
- **Cost and correctness**: cost is linear in the scrollback size, about 1 µs per row of 120 columns. A property test checks that shrinking and then widening again, through other sizes on the way, loses no content, wherever the cursor is and whatever edits moved the rows: every line keeps its cells, with their styles, graphemes and links.

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
  - A cluster is as wide as its first codepoint that has a width, its base, which is its first one unless a prepended character comes before it, except that emoji presentation, which reads the base and what follows it, changes it: VS16 after a pictographic or a keycap base gives 2, VS15 gives 1, and a flag, a pair of regional indicators, or an emoji ZWJ sequence gives 2. With the Unicode 8 tables, which knew no emoji presentation, a cluster is always as wide as its first codepoint that has a width.
- **`legacy` mode**: one codepoint at a time, like `wcwidth`, for programs that compute widths themselves. A codepoint of no width joins the cell before it.
- **The width of a codepoint**, as Markus Kuhn's `wcwidth` counts it: none for controls, nonspacing and enclosing marks, format and default-ignorable characters, and the Hangul vowels and final consonants that combine into syllables, but one for the soft hyphen, and the cells of their East Asian width for the Hangul fillers U+115F, U+3164 and U+FFA0, which are default ignorable; two for wide and fullwidth characters, and for the ambiguous ones when they count as wide; one otherwise.
- **Mode 2027**: lets programs query and switch the mode.
- **Width tables that match the programs'**: a program that counts a character's width differently from the terminal misplaces the cursor and the text after it. Two options make the terminal count as the programs do, in either mode:
  - `unicode-widths`: the tables of the Unicode version that Bolt ships, or those of Unicode 8, for programs that predate Unicode 9. Unicode 8 gives most emoji one cell, and Unicode 9 gave them two. `xtask ucd` generates the Unicode 8 table too.
  - `ambiguous-width`: the cells that East Asian Ambiguous characters take (①, ※, ○, Greek and Cyrillic letters…): 1 by default, 2 for CJK setups whose programs count them as two, like vim's `ambiwidth=double`. It is an explicit option, not a guess from the locale, because it must match the programs.
  - A change applies to new output; the cells already written keep their widths.
- **Normalization**: the `normalization` option puts the text in a Unicode normal form as it is printed: none (the default), NFC, NFD, or HFS+, Apple's variant of NFD, which leaves the ranges U+2000–U+2FFF, U+F900–U+FAFF and U+2F800–U+2FAFF alone. NFC shows the decomposed names of macOS files composed, Hangul included, and a composed character takes no grapheme entry. Each cluster is normalized as it is built, and text already in the normal form passes a quick check without being rewritten. The terminal's buffers are reused, but the crate keeps up to four combining marks inline while it reorders them, so a cluster with more allocates. The data comes from the `unicode-normalization` crate, at Unicode 17 while the tables are at 18: canonical decompositions never change for existing characters, so the gap covers at most the new characters of Unicode 18 that have one.

### Settings

The `[terminal]` section of the config:

```toml
[terminal]
term = "xterm-256color"   # what TERM tells programs: xterm-bolt, xterm-256color, xterm, ansi, vt100, vt220, dtterm, linux…
unicode-widths = "current" # current | 8
ambiguous-width = 1       # 1 | 2
normalization = "none"    # none | nfc | nfd | hfs+
```

`term` changes only what programs are told, never the emulation, which stays xterm's: the session puts it in the environment, and the terminal reports it as `TN` to XTGETTCAP. The other values describe subsets of what xterm does, so they work as they are.

## Input

- **Keyboard**: `input::encode_key`, from a `KeyEvent` that the platform makes once dead keys and input methods are resolved: the key; whether it went down, repeats or went up; its modifiers, with the two Alt keys told apart, and the locks; what it types with every modifier; what it types with Shift alone, as macOS's `charactersIgnoringModifiers` gives it; and the key in the same place on the standard PC-101 layout. The kitty flags in force choose the encoding.
  - **Legacy xterm encoding**, while no flag is set. Releases send nothing, and a repeat is one more press.
    - Control maps characters to C0 as xterm does: the letters and `@[\]^_`, space and `2` to NUL, `3` to `7` to ESC and the four after it, `/` to US, `8` and `?` to DEL; other characters go as they are. With Control or Meta, and without Shift, a key whose character is not ASCII goes as the key of the base layout, as kitty's legacy encoding has it, so that Control with `с` in a Cyrillic layout sends ETX. Backspace sends DEL, and BS with Control; Shift with Tab sends `CSI Z`, which Control leaves as it is and Meta sends after ESC; Return sends CR, or CR LF in LNM.
    - The cursor keys, Home and End send `CSI final`, or `SS3 final` in application mode (DECCKM); Insert, Delete, Page Up and Page Down `CSI n ~`; F1 to F4 `SS3 P` to `SS3 S`, F5 to F20 `CSI n ~`, and Menu F16's `CSI 29 ~`. With modifiers, they all send `CSI 1 ; m final` or `CSI n ; m ~`, where `m` is 1 plus 1 for Shift, 2 for Meta and 4 for Control. The keypad sends `SS3` codes in application mode (DECKPAM), and what it types otherwise, with ESC first for Meta, but neither Control's codes nor `modifyOtherKeys`'s form, as in xterm; without Num Lock, its keys send what the keys they stand for send, and its 5 `CSI E`. The locks, the modifier keys, Print Screen, Pause, the media keys and F21 to F35 send nothing.
    - An Alt that acts as Meta, as `KeyOptions` says for each side, sends ESC before the key, typed without Alt; one that does not types what the platform composed with it, as Option's characters on macOS, and is no modifier for the sequences. ESC is sent whatever mode 1036 says, which is reported but changes nothing: xterm's eight-bit Meta does not suit UTF-8.
    - `modifyOtherKeys`, which programs set with `CSI > 4 ; n m` and query with `CSI ? 4 m`: at 1, the keys whose usual sequence loses a modifier, such as Control with a digit, Control and Shift with a letter, or Return with Control or Shift, send `CSI 27 ; m ; code ~`; at 2, as xterm's `input.c` has it, every key that types a character does with Control or Meta, and so do Return, Escape and Backspace with any modifier, Shift alone included, but for Backspace with Control, which sends BS and is reported only with another modifier, with the code of BS; with Shift alone, so do space and the characters from `@` to DEL, which include the letters, when the key types that character, so that what Option or a dead key composes types. The keypad, the cursor keys and the function keys keep their own sequences, with their modifiers. Shift with Tab is reported with Control or Meta at either level. `CSI > 4 m`, `CSI > m`, which the parser reads as `CSI > 0 m`, and `CSI > 4 n` reset it, and so does RIS.
  - The **[kitty keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/)**, once a program sets flags, with every flag.
    - **Text**: a key that types text sends it, unless every key is reported. A key types nothing with Control, an Alt that acts as Meta, Super, Hyper or Meta, nor when the platform's text has a control character. Enter, Tab and Backspace without modifiers, the locks aside, send their legacy bytes and no release, so that a shell stays usable after a program that left the flags set.
    - **Escape codes**: the other keys send `CSI code ; modifiers u`, with the code of the character the key types unshifted, or the number of the protocol's table, or `CSI 1 ; modifiers final` and `CSI code ; modifiers ~` for the keys that xterm sends those for; F3 is `CSI 13 ~`, since `CSI R` is a cursor position report. The modifiers are 1 plus 1 for Shift, 2 for an Alt that acts as Meta, 4 for Control, 8 for Super, 16 for Hyper, 32 for Meta, 64 for Caps Lock and 128 for Num Lock; with none, the field goes, and so does a code of 1 with nothing after it. For a modifier key, the platform gives the modifiers as the event leaves them. Cursor key mode, the keypad's application mode and `modifyOtherKeys` do not apply.
    - **Disambiguate** (1): Esc, and the keys that a modifier such as Control or an Alt that acts as Meta keeps from typing, send escape codes, and the keypad's keys that type nothing send their own numbers.
    - **Event types** (2): `:2` after the modifiers for a repeat, `:3` for a release, which a key that types text reports as an escape code. Without the flag, a repeat is one more press and a release sends nothing.
    - **Alternate keys** (4): keys that type characters send `code:shifted:base`, the shifted key with Shift, if it differs, and the key of the base layout, if it differs; `code::base` without the first.
    - **All keys** (8): every key sends an escape code, Enter, Tab, Backspace and the keys that type text included. The modifier keys and the locks are reported only with this flag.
    - **Associated text** (16): the text that a press or a repeat types follows the modifiers, as codepoints separated by colons.
    - Without disambiguation or every key, presses that xterm has a sequence for send it, cursor key mode included, unless Super, Hyper, Meta or a lock is held, which xterm's sequences cannot carry; those, and keys that xterm has no sequence for, send their escape codes.
    - `input::encode_text` encodes text that no key typed, such as what an input method commits: as it is, or `CSI 0 ; ; codepoints u` when every key and its text are reported.
  - The kitty flags live on a per-screen stack (`CSI > u`, `CSI < u`, `CSI = u`, `CSI ? u`).
- **macOS specifics**:
  - Option can act as Meta, configurable separately for the left and right keys;
  - dead keys are supported;
  - IME composition text is never sent to the PTY.
- **Mouse**: `input::encode_mouse`, from a `MouseEvent`: a press, a release, a motion with a button held or none, or a step of the wheel, with the modifiers and the cell and the pixel under the pointer. It returns whether the event goes to the program rather than to the terminal, which selects text and scrolls with the others.
  - **Tracking**: X10 (9) reports the presses of the first three buttons, without modifiers; 1000 presses, releases and the wheel; 1002 also motion with a button held; 1003 every motion. The platform reports a motion only when the cell changes, or the pixel, for reports in pixels.
  - **Codes**: the first three buttons are 0 to 2, buttons 8 to 11 are 128 to 131, and the wheel 64 to 67; a motion adds 32, and Shift 4, either Alt 8 and Control 16.
  - **Encodings**: X10's `CSI M` and three bytes, the code and the column and row from 1, each plus 32, where a release is 3, whichever button went up, and positions past 222 are the byte 0, as in xterm; SGR (1006), `CSI < code ; column ; row M`, or `m` for a release, which keeps the button; SGR-pixels (1016), the same with the pixel, from 1.
  - **Alternate scroll** (1007): without tracking, on the alternate screen, each step of the vertical wheel sends the up or down cursor key, as cursor key mode has it.
- **Paste**: `input::encode_paste`. With bracketed paste (2004), the text goes between `CSI 200 ~` and `CSI 201 ~`; without, each line break, CR LF, LF or CR, goes as CR, as Return sends it. Either way, the control characters other than tab and the line breaks are left out, C0, DEL and C1 alike, so that a paste can neither end the bracket with its own `CSI 201 ~` nor send ESC to an editor ([SECURITY.md](SECURITY.md#terminal-defaults)).
- **Focus events**: `input::encode_focus` sends `CSI I` and `CSI O` in mode 1004.

## PTY and processes (`pty`)

- **Spawning**: `pty::spawn(&Options)` returns a `Pty`, which gives readers and writers of the terminal, its device's name, the program's process id and the terminal's foreground process group.
  - `/dev/ptmx`, `grantpt`, `unlockpt`, `ptsname_r` and the slave opened with `O_NOCTTY`, both sides closed on exec as they open, and the terminal's size; then **fork+exec** through `std::process::Command`, with the slave as standard input, output and error. The parent keeps no copy of the slave, so the master reads the end when the program and its children have gone; the reader turns the EIO that Linux gives then into the end.
  - The master does not block: readers and writers wait for it with `poll`, and for a descriptor that becomes ready, for good, as the `Pty` drops: on macOS, a kqueue with a user event that the drop triggers, which no child inherits, by a fork or a spawn; elsewhere, a pipe opened closed on exec, whose end to write the drop closes. `PtyReader::read_until` reads with a deadline, `PtyReader::wait_either` waits for output and for room at once, for a caller that reads and writes on one thread, and `PtyReader` and `PtyWriter` implement `Read` and `Write` on top of the waits.
  - An async-signal-safe `pre_exec` runs `setsid()` and `ioctl(TIOCSCTTY)`, empties the signal mask, which the child has from the thread that spawned it, and gives every signal its default action, since an ignored one stays ignored across exec.
  - The crate's spawns take a lock, as `std`, on macOS, opens the pipe through which a spawn learns that its child failed to exec, then marks it closed on exec: a spawn on another thread in between would give its child the pipe, and keep the first spawn waiting until that child exits. What can wait on a mount that does not answer is done before the lock: the working directory is opened, to be searched rather than read, for the child to enter with `fchdir`, a directory that cannot be opened failing at once with an error of its own; and the program is looked for in the `PATH` of its environment, or the system's default, as `execvp` looks, relative entries in the working directory made absolute, a candidate needing an execute bit in its mode as well as `access`, which an ACL can pass for a file that `execve` refuses. The program keeps its name as `argv[0]`, and one that cannot be run fails at once, with `EACCES` or `ENOENT` as `execvp` would: on macOS, a directory of `PATH` that cannot be searched counts as the program not there. The exec itself, which reads the program, stays under the lock.
  - We do not use `posix_spawn`, because it cannot set the controlling terminal, nor `openpty`, which closes the descriptors on exec only after it opens them, and gives up when the kernel fails an open of `/dev/ptmx` with `EREDRIVEOPEN` (-6). macOS's kernel does that at times when several processes open pseudo-terminals at once, for an errno meant for itself alone; the open is tried again.
  - The program starts in the working directory that the caller gives, or the user's home directory, read from the user database with the name and the shell.
- **Login shell**: on macOS, launched as Terminal.app launches it: `/usr/bin/login -flp $USER /bin/bash --noprofile --norc -c "exec -l -a <name> <shell>"`. `login` records the session as a terminal's; `-f` skips the password, the user being logged in already; `-p` keeps the environment; `-l` keeps the working directory, and keeps the shell from being a login shell too, so bash starts it with `-` before its name. On Linux, the shell is started with `argv[0]` prefixed by `-`.
- **Environment**:
  - `TERM`, from the `term` option ([Settings](#settings)), `COLORTERM=truecolor`, `TERM_PROGRAM=bolt`, `TERM_PROGRAM_VERSION`, `BOLT_RESOURCES_DIR` when the caller gives it, and the caller's own variables last.
  - A UTF-8 locale in `LANG` if none of `LC_ALL`, `LC_CTYPE` and `LANG` is set; the caller chooses it ([SECURITY.md](SECURITY.md#macos-environment)).
  - `TERMINFO`, `COLUMNS` and `LINES` are not inherited: they would describe the terminal that started this one, if one did. `TERMINFO` is set only when `TERM` is `xterm-bolt` or `bolt` and the caller gives the resources, to their `terminfo` directory.
- **Resize**: `Pty::resize`, `TIOCSWINSZ` with sizes in both cells and pixels; sixel and kitty graphics need the pixels. The kernel sends SIGWINCH to the foreground process group. Resizes are coalesced while a divider is being dragged.
- **Process exit**: EOF on the master plus `waitpid`: `Pty::wait` and `Pty::try_wait` tell an `Exit`, a status or the signal that ended the program. The end that the master reads can come after the program's exit, when processes that it started still have the terminal open, or never, so the exit is told apart from it. What happens next is configurable: close the pane, or show the exit code.
- **Closing**: dropping a `Pty` hangs the program up as closing a terminal does, with SIGHUP to the program and to a job in the terminal's foreground, and ends what its readers and writers wait for: reads end, writes fail, and the terminal closes once they drop, so that a program that ignores SIGHUP sees the end of its input. A thread waits for the program, so that it leaves no zombie.
- **Portable API**: the public interface (reader, writer, resize, child) exposes no `RawFd`. A future ConPTY implementation for Windows will therefore not change any callers.

## Session (`session`)

- **`Session`**: `Session::spawn` starts the program in a PTY, with a `Terminal` of the PTY's size, and two threads; `Session::lock` takes the grid lock; `key`, `mouse`, `focus` and `paste` encode input with the terminal's modes and keyboard flags, and `write` sends bytes as they are; `resize` resizes the grid, then the PTY. `wake`, given to `spawn`, is called when the terminal changes and the main thread has seen the last change, `clear_wake` saying so, and when the output ends; `has_ended` and `exit` tell the end and the status. Dropping the session hangs the program up, and its threads end.
- **I/O thread**: one per pane. It reads slices of at most 64 KiB, outside the lock, and feeds them to the parser under the grid lock, which it releases fairly after each slice: a thread that waits for the lock has it before the next slice. Before each slice, the I/O thread waits for the threads that came to the lock before it asked; those that come later wait for its slice to start. Each side has the lock in turn, however often the other takes it.
- **Write queue**: collects input (keys, pastes, replies to terminal reports), in order, for a writer thread, which writes each entry in chunks of 4 KiB and stops between two once the session closes. A paste is one entry, so that no reply comes between its brackets. The I/O thread does not write: a program that floods its output without reading its input, while it asks for replies, would otherwise stop both sides. The queue takes a slice's replies while less than a MiB of replies waits, the one that the writer writes until it asks for the next included, and drops them past it, so that such a program cannot grow the queue without end: as a slice's replies take a MiB at most too, as the terminal keeps them, about two MiB of replies wait at most. Keys, pastes and the clipboard's answers are never dropped, nor counted: `Session::reply_clipboard` queues the answer at once, as a program can wait for it without writing anything.
- **Synchronized output**: each update that starts, a new value of `Terminal::synchronized_updates`, gets a deadline, by default a second away, which the I/O thread keeps, its read waiting until then at most; past it, it ends the update with `Terminal::end_synchronized_output`, unless another has started. Output during the update does not put the deadline off, nor does `CSI ? 2026 h` again, and a write that blocks or fails cannot keep it. The end of the output ends the update under way at once, and so does the end of the I/O thread by a panic, which ends the session as the output's end does, waking the main thread.
- **Renderer snapshot**: `Session::snapshot` brings a `Snapshot`, which the renderer keeps from frame to frame, up to date under the lock, with `Terminal::snapshot`: it copies the visible rows that are dirty, each with the styles that its cells use, its clusters' codepoints, its soft wrap and its prompt's start, and the cursor, the colors and reverse video. `Snapshot::colors_changed` says when the colors or reverse video changed since the last update, which calls for drawing every row again, copied or not. Rows that the screen scrolled are not copied again: the page list counts the screen's scrolls, so the snapshot moves the lines it has up and copies those that came in; a resize or a reflow starts a new epoch of the list, and another screen is another list, after which every row is copied. A snapshot of an unchanged screen copies nothing. While a synchronized update holds the screen, the snapshot stays as it was.
- **Independence from the main thread**: the I/O thread never waits for it. If the renderer falls behind, it jumps straight to the latest state.
- **Encodings** (F5b): the terminal works in UTF-8 only. Other encodings, chosen per profile, come after the MVP as a conversion in the session, with `encoding_rs`: the bytes from the PTY become UTF-8 before the parser, and the input goes back in the program's encoding. Latin-1, windows-125x, Shift_JIS, EUC-JP, EUC-KR, GB18030, Big5 and ISO-2022-JP are among them. C1 controls stay 7-bit only.

## Terminfo

- **The entry**: `xterm-bolt`, also named `bolt`, in `crates/vt-core/terminfo/xterm-bolt.terminfo`. It stands alone, with no `use=`, so that it compiles the same on any machine, whatever `xterm-256color` the machine's database holds.
  - It is `xterm-256color` as ncurses 6.0 describes it, without what the terminal does not do: printing (`mc0`, `mc4`, `mc5`, `mc5i`), meta mode (`smm`, `rmm`) and memory lock (`meml`, `memu`). `kbs` is DEL, which Backspace sends, `Se` restores the cursor style that the user chose (`CSI 0 SP q`), and `kmous` is `\E[<`, so that ncurses asks for SGR mouse reports, which have no limit of 223 columns as X10's do, as Ghostty's entry does; ncurses reads them since late 2014, so the 6.0 library that macOS ships does too, and `XM`, which later versions read, turns them on.
  - It adds:
    - `Tc`, `setrgbf` and `setrgbb` (24-bit color). `RGB` is left out: next to `colors#256`, ncurses 6.1 and later would read it as direct color with 3, 3 and 2 bits per channel;
    - `Su` and `Smulx` (styled underlines) and `Setulc` (underline color);
    - `Smol`/`Rmol` (overline) and `smxx`/`rmxx` (strikethrough);
    - `Sync` (mode 2026);
    - `BD`/`BE`/`PS`/`PE` (bracketed paste);
    - `fe`/`fd` and `kxIN`/`kxOUT` (focus events);
    - `XM` (SGR mouse reports, with `kmous`);
    - `hs`, `tsl`, `fsl` and `dsl` (the title as a status line);
    - `oc` (OSC 104) and `rep` (REP).
  - It keeps, from `xterm-256color`, `Ss`/`Se` (cursor style), `Ms` (OSC 52), `Cs`/`Cr` (cursor color), `E3` and `XT`.
- **Build**: `cargo xtask terminfo` (`just terminfo`) compiles it with `tic -x` into `target/terminfo`, in the layout of the `tic` that compiles it: hexadecimal directories on macOS (`78/xterm-bolt`), letters on Linux. macOS's own `tic`, from ncurses 6.0, is enough: the entry's numbers stay below 32768, so that any `tic` writes the legacy format, which every ncurses reads, macOS's own library included. The task fails if `tic` warns, if `infocmp -x` does not read every capability of the source back with its value, its escapes undone so that the way of writing it does not count, or if the format is another. Colons are written plain, as `infocmp` would escape them.
- **Install**: the app bundles it, in `Contents/Resources/terminfo` on macOS and `share/terminfo` on Linux, and `TERMINFO` points there when `TERM` is `xterm-bolt` or `bolt` ([PTY](#pty-and-processes-pty)).
- **XTGETTCAP** answers from the same source, by terminfo name: a boolean with `DCS 1 + r` and its name, a number in decimal, a string as the terminal sends or reads it (`kbs` is `7F`), without the padding (`$<100/>`) that only `tputs` reads, and a string with parameters as the source writes it, for the program to fill in, as Ghostty and kitty do. It also answers xterm's own names: `TN` and `name`, the `TERM` that programs see; `Co`, the colors' termcap name; and `RGB`, `8/8/8`, the bits of each channel of a direct color. It does not answer other termcap names.
- **Default `TERM`**: `xterm-256color`, because remote hosts usually don't know `xterm-bolt`. The `term` option chooses another ([Settings](#settings)). The default will change once the ssh integration (F5a) can install the terminfo on remote hosts or fall back automatically.

## Supported protocols

The "Phase" column shows the [roadmap](ROADMAP.md) phase in which support lands.

| Group | Sequences | Phase |
|---|---|---|
| C0 controls | BEL, BS, HT, LF, VT, FF, CR, SO, SI | F1 |
| ESC | DECSC/DECRC (ESC 7/8), IND, NEL, HTS, RI, DECBI/DECFI (ESC 6/9), RIS, DECKPAM/DECKPNM, G0–G3 charset designation, LS2/LS3, SS2/SS3, DECALN | F1 |
| Cursor | CUU, CUD, CUF, CUB, CNL, CPL, CHA, CUP, HVP, VPA, VPR, HPA, HPR, CHT, CBT, DECSCUSR | F1 |
| Editing | ICH, DCH, IL, DL, ECH, ED, EL, DECSED, DECSEL, REP, SU, SD, DECSTBM, DECSLRM, SCOSC/SCORC, DECSCA, TBC; the VT420's columns, DECIC, DECDC, and rectangles, DECCRA, DECERA, DECFRA, DECSERA | F1 |
| Resets and levels | RIS, DECSTR (soft reset), DECSCL (conformance level, kept for DECRQSS) | F1 |
| SGR | 0–9, 21–29, 30–37, 90–97, 38;5 and 38;2 (also with `:`), 39, 40–47, 100–107, 48, 49, 4:0–4:5 (underline styles), 53/55 (overline), 58/59 (underline color) | F1 |
| ANSI modes | IRM (4), LNM (20); GATM, SRTM, VEM, HEM, PUM, FEAM, FETM, MATM, TTM, SATM, TSM and EBM reported as permanently reset | F1 |
| DEC modes | 1 (DECCKM), 4 (DECSCLM), 5 (DECSCNM), 6 (DECOM), 7 (DECAWM), 12 (blink), 25 (DECTCEM), 45 (reverse wrap), 1045 (extended reverse wrap), 47/1047/1049 (alternate screen), 1048, 66 (DECNKM), 69 (DECLRMM), 1000/1002/1003/1006/1016 (mouse), 1004 (focus), 1007 (alternate scroll), 1036 (Meta sends ESC), 2004 (bracketed paste), 2026 (synchronized output), 2027 (grapheme clustering) | F1 |
| Reports | DA1, DA2, DA3, DECID, DSR (5n, 6n), DECDSR (?6n, ?15n, ?25n, ?26n, ?53n, ?55n, ?56n, ?62n, ?63n, ?75n, ?85n), DECRQM, DECRQSS, DECRQCRA, XTVERSION, XTGETTCAP; XTWINOPS 14t/16t/18t (sizes) and 22t/23t (title stack). Title reporting (21t) and DECRQCRA are **disabled** by default | F1 |
| OSC | 0/1/2 (title), 4/104 (palette), 5/105 (special colors), 7 (cwd), 8 (hyperlinks), 10/11/12 and 110/111/112 (dynamic colors), 52 (clipboard: write yes, read no by default), 133 (semantic prompts), 9 and 777 (notifications), 9;4 (progress), 6;1 (tab color, iTerm2) | F1–F3 |
| OSC 1337 (iTerm2) | SetUserVar, CurrentDir, RemoteHost and ShellIntegrationVersion through a plugin; `File=` (images) in F5c | F5 |
| kitty | Keyboard protocol (F1–F2); graphics via APC `G` (F5c); OSC 99 notifications (F5) | F1–F5 |
| Sixel | DCS `q` | F5c |

Sequences not listed here are ignored. Debug builds also log them, so the list of missing sequences comes from real sessions.
