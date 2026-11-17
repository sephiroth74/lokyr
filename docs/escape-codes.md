# Escape codes

<sub>macOS · for program and plugin authors</sub>

Programs control the terminal by writing escape codes among their output: to move the cursor, color text, ask what the terminal supports, or report their progress. This page explains how the codes are written here, how a program finds out that it runs in Lokyr, and lists every code that Lokyr reads, with the version that brought it and its specification.

## Try it

Run this command in Lokyr:

```sh
printf '\033[>q'; IFS= read -rs -t 1 -d '\' reply; reply=${reply#*|}; printf '%s\n' "${reply%$'\033'}"
```

It asks the terminal for its name and version, and prints Lokyr's answer. You should see Lokyr's version, such as:

```text
lokyr 1.2.0
```

## How the codes are written

The pages of this reference write each code the way xterm's [control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html) do, with spaces between its parts to make it easier to read. A program sends it without the spaces.

| Name | Bytes | In `printf` |
|---|---|---|
| ESC | the escape character, `0x1B` | `\033` |
| CSI | ESC `[` | `\033[` |
| OSC | ESC `]` | `\033]` |
| DCS | ESC `P` | `\033P` |
| APC | ESC `_` | `\033_` |
| ST, the string terminator | ESC `\` | `\033\\` |
| BEL | `0x07`, which ends an OSC too | `\a` |

`Ps` stands for one number, `Pm` for several separated by `;`, and `Pt` for text. An OSC that ends with BEL gets its answer ended with BEL, and one that ends with ST gets ST.

## Find out that a program runs in Lokyr

Lokyr sets these variables for the programs of its panes:

| Variable | Value |
|---|---|
| `TERM_PROGRAM` | `lokyr` |
| `TERM_PROGRAM_VERSION` | Lokyr's version, such as `1.2.0` |
| `TERM` | `xterm-256color` by default, as [`[terminal] term`](CONFIGURATION.md#term) sets it |
| `COLORTERM` | `truecolor` |
| `LOKYR_RESOURCES_DIR` | The folder of Lokyr's resources, such as its [shell integration](shell-integration.md) scripts |

The variables don't cross `ssh`, and a program started under `tmux` or `sudo` may not see them. To learn what the terminal supports wherever the program runs, ask the terminal itself: it answers these queries with these exact replies.

| Query | Asks | Lokyr replies |
|---|---|---|
| `CSI > q` | XTVERSION, the terminal's name and version | `DCS > \| lokyr 1.2.0 ST` |
| `CSI c` | DA1, the primary attributes | `CSI ? 62 ; 22 c`: a VT220 with ANSI colors |
| `CSI > c` | DA2, the secondary attributes | `CSI > 1 ; 10 ; 0 c` |
| `CSI = c` | DA3, the unit's identity | `DCS ! \| 00000000 ST` |
| `CSI ? Ps $ p` | DECRQM, whether a mode is set | `CSI ? Ps ; Pm $ y`: `1` set, `2` reset, `3` always set, `4` always reset, `0` unknown |
| `CSI ? u` | The keyboard's reporting flags | `CSI ? Ps u`, the flags in force |
| `CSI ? 996 n` | The color scheme | `CSI ? 997 ; 1 n` for a dark theme, `CSI ? 997 ; 2 n` for a light one |
| `OSC 11 ; ? ST` | The background color, and `10` the text's, `12` the cursor's | `OSC 11 ; rgb:rrrr/gggg/bbbb ST` |
| `DCS + q Pt ST` | XTGETTCAP, a terminfo capability, its name in hexadecimal | `DCS 1 + r Pt = Pt ST`, the name and its value in hexadecimal, or `DCS 0 + r ST` for a name that Lokyr doesn't know |

Prefer a query for each feature, such as DECRQM for a mode, to a check of the name or the version: a feature's query keeps working as Lokyr and other terminals change.

## The codes that Lokyr reads

xterm's [control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html) describe the standard codes, which Lokyr reads as xterm does unless the table says otherwise. The codes of a page of their own link to it: [progress and status](escape-codes/progress-and-status.md) and [notifications](escape-codes/notifications.md). The codes that a later version brought say so in the **Since** column. Lokyr ignores a code that it doesn't read, and shows nothing of it.

### Controls, the cursor and editing

| Family | Codes | Since |
|---|---|---|
| C0 controls | BEL, BS, HT, LF, VT, FF, CR, SO, SI | 0.2 |
| ESC | DECSC and DECRC (ESC 7 and 8), IND, NEL, HTS, RI, DECBI and DECFI (ESC 6 and 9), RIS, DECKPAM and DECKPNM, the G0 to G3 character sets, LS2 and LS3, SS2 and SS3, DECALN | 0.2 |
| The cursor | CUU, CUD, CUF, CUB, CNL, CPL, CHA, CUP, HVP, VPA, VPR, HPA, HPR, CHT, CBT, DECSCUSR (the cursor's shape) | 0.2 |
| Editing | ICH, DCH, IL, DL, ECH, ED, EL, DECSED, DECSEL, REP, SU, SD, DECSTBM, DECSLRM, SCOSC and SCORC, DECSCA, SPA and EPA (ESC V and W), TBC; DECIC and DECDC; the rectangles, DECCRA, DECERA, DECFRA, DECSERA | 0.2 |
| Resets and levels | RIS, DECSTR (soft reset), DECSCL (the conformance level, which DECRQSS reports) | 0.2 |

### Text attributes and modes

| Family | Codes | Since |
|---|---|---|
| SGR, the text's attributes | 0 to 9, 21 to 29, 30 to 37, 90 to 97, 38;5 and 38;2 (also with `:`), 39, 40 to 47, 100 to 107, 48, 49, 4:0 to 4:5 (the underline's styles), 53 and 55 (overline), 58 and 59 (the underline's color) | 0.2 |
| ANSI modes | IRM (4), LNM (20); GATM, SRTM, VEM, HEM, PUM, FEAM, FETM, MATM, TTM, SATM, TSM and EBM reported as always reset | 0.2 |
| DEC modes | 1 (DECCKM), 3 (DECCOLM), 4 (DECSCLM), 5 (DECSCNM), 6 (DECOM), 7 (DECAWM), 12 (the cursor's blink), 25 (DECTCEM), 40 (DECCOLM allowed), 41, 45 (reverse wrap), 1045 (extended reverse wrap), 47, 1047 and 1049 (the alternate screen), 1048, 66 (DECNKM), 67 (DECBKM), 69 (DECLRMM), 95 (DECNCSM), 1000, 1002, 1003, 1006 and 1016 (the mouse), 1004 (focus), 1007 (alternate scroll), 1036 (Meta sends ESC), 2004 (bracketed paste) | 0.2 |
| Synchronized output | Mode 2026, [its specification](https://gist.github.com/christianparpart/d8a62cc1ab659194337d73e399004036) | 0.2 |
| Grapheme clusters | Mode 2027, [its specification](https://github.com/contour-terminal/terminal-unicode-core) | 0.2 |
| The color scheme's reports | Mode 2031 and `CSI ? 996 n`, [their specification](https://contour-terminal.org/vt-extensions/color-palette-update-notifications/) | 0.7 |

### Reports

| Family | Codes | Since |
|---|---|---|
| Identity and state | DA1, DA2, DA3, DECID, DSR (5n and 6n), DECDSR (?6n, ?15n, ?25n, ?26n, ?53n, ?55n, ?56n, ?62n, ?63n, ?75n, ?85n), DECRQM, DECRQSS, DECRQCRA, XTVERSION, XTGETTCAP | 0.2 |
| XTWINOPS | 14t, 16t and 18t (the sizes), 22t and 23t (the title's stack); the title modes (`CSI > Ps t` and `T`); XTSAVE and XTRESTORE (`CSI ? Pm s` and `r`). A program can't move, resize or minimize the window, nor read its title (21t) or a rectangle's checksum (DECRQCRA) | 0.2 |

### OSC

| Code | What it does | Since | Specification |
|---|---|---|---|
| OSC 0, 1 and 2 | The window's and the tab's title | 0.2 | [xterm](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h2-Operating-System-Commands) |
| OSC 4 and 104 | The palette's colors, set, asked for and reset | 0.2 | [xterm](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h2-Operating-System-Commands) |
| OSC 5 and 105 | The special colors | 0.2 | [xterm](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h2-Operating-System-Commands) |
| OSC 7 | The shell's folder, as a `file` URL, which new tabs and splits start in | 0.2 | [Shell integration](shell-integration.md#for-shell-authors) |
| OSC 8 | Hyperlinks | 0.2 | [Hyperlinks in terminals](https://gist.github.com/egmontkob/eb114294efbcd5adb1944c9f3cb5feda) |
| OSC 10, 11 and 12, and 110, 111 and 112 | The text's, the background's and the cursor's colors, set, asked for and reset | 0.2 | [xterm](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h2-Operating-System-Commands) |
| OSC 52 | The clipboard: written, and read only as [`[clipboard] read`](CONFIGURATION.md#read) allows | 0.2 | [xterm](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h2-Operating-System-Commands) |
| [OSC 9](escape-codes/notifications.md#osc-9-a-notification) and [777](escape-codes/notifications.md#osc-777-a-notification-with-a-title) | Desktop notifications | 0.2 | |
| OSC 6 ; 1 | The tab's color | 0.5.1 | [Its escape codes](https://iterm2.com/documentation-escape-codes.html) |
| [OSC 9 ; 4](escape-codes/progress-and-status.md#osc-94-progress) | A program's progress, on its tab | 0.5.1 | [Progress bar sequences](https://learn.microsoft.com/en-us/windows/terminal/tutorials/progress-bar-sequences) |
| OSC 133 | Semantic prompts: where each prompt, command and output starts | 0.7 | [Semantic prompts](https://gitlab.freedesktop.org/Per_Bothner/specifications/blob/master/proposals/semantic-prompts.md), and [Shell integration](shell-integration.md#for-shell-authors) |
| OSC 1337 `File=` | Inline images, whole or in parts (`MultipartFile=`, `FilePart=`, `FileEnd`) | 1.0 | [Inline images](https://iterm2.com/documentation-images.html) |
| [OSC 99](escape-codes/notifications.md#osc-99-a-notification-in-parts) | Desktop notifications of a title and a body, in parts, and the query of what the terminal supports · not released yet | | [Desktop notifications](https://sw.kovidgoyal.net/kitty/desktop-notifications/) |
| [OSC 7501](escape-codes/progress-and-status.md#osc-7501-a-programs-status) | A program's status, such as working, waiting or done · not released yet | | [Program Status Protocol](https://control-codes.page/proposals/osc-7501/) |
| [OSC 21337](escape-codes/progress-and-status.md#osc-21337-a-sessions-status) | A session's status, shown on its tab · not released yet | | [Session status](https://iterm2.com/documentation-session-status.html) |

### The keyboard and graphics

| Family | Codes | Since | Specification |
|---|---|---|---|
| The keyboard protocol that reports every key | `CSI > Ps u`, `CSI < Ps u`, `CSI = Ps ; Ps u` and `CSI ? u` | 0.2 | [Keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/) |
| Graphics | APC `G`, images sent directly | 1.0 | [Graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/) |

## See also

- [Shell integration](shell-integration.md), the codes that a shell sends at each prompt
- [The configuration guide](CONFIGURATION.md): [`term`](CONFIGURATION.md#term), and [`[clipboard] read`](CONFIGURATION.md#read)
