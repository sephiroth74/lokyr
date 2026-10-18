# esctest, after the first compatibility fixes (F1-45)

Measured on 2026-10-18 with `just esctest`, after F1-45, against [the first run](esctest-F1-44.md). The full report is in [esctest-F1-45/report.txt](esctest-F1-45/report.txt).

## What changed in the method

- **Rectangle checksums** are on in the harness, as iTerm2's notes for esctest ask: DECRQCRA, through which esctest reads the screen, is off by default in Bolt, as the [security defaults](../SECURITY.md#terminal-defaults) say.
- **Cells of 8 by 16 pixels**, so that the sizes in pixels are told.
- **`--xterm-reverse-wrap=383`**: reverse wraparound as xterm does it since 2023, with mode 45 for the rows that wrap and 1045 for the others.

## Result

**403 of 528 applicable tests pass (76.3%)**, against 163 (31.0%) before, with 39 known bugs of xterm left out.

| Fix | Tests it took |
|---|---|
| DECRQCRA | 172 |
| DECSTR, the soft reset, which esctest sends before each test, and which `is2` of xterm-256color sends too | 29 |
| DECRQM of the ANSI modes kept reset for good, DECHCCM, DECNKM and DECSCLM; DECDSR's reports; DECID; DECRQSS of DECSCL, DECSACE, DECSSDT and DECSASD; DECSCL | 30 |
| The extended reverse wraparound, mode 1045, and the modern xterm's reverse wrap | 9 |

## What fails still

| Cause | Tests |
|---|---|
| XTWINOPS: the display's size and the window's place and state, which only the embedder knows, and the resizes | 19 |
| OSC 5 and 105, the special colors | 19 |
| Colors in X11's other spaces: CIEXYZ, CIEuvY, CIExyY, CIELab, CIELuv, TekHVC and RGBI | 14 |
| The rectangle operations of the VT420: DECCRA, DECERA, DECFRA, DECSERA | 24 |
| Columns and indexes of the VT420: DECIC, DECDC, DECBI, DECFI | 20 |
| DECRQM of modes that change what the terminal does: KAM, CRM, SRM, the printer's modes, DECHEBM, DECNRCM, DECBKM | 8 |
| ISO protection (SPA and EPA), which ED, EL, ECH, DECSED and DECSEL respect | 5 |
| DECCOLM, 132 columns, and xterm's save of it | 5 |
| DA and DA2, which tell a VT220, as Ghostty does, where esctest's level 5 asks for a VT520 | 4 |
| DECSCL's levels, which change what the terminal does | 3 |
| DECSLPP and DECSNLS, which resize | 2 |
| Mode 41, xterm's curses fix | 1 |
| CBT at the left margin | 1 |
