# esctest, after the second compatibility fixes (F1-46)

Measured on 2026-10-18 with `just esctest`, after F1-46, against [the run after F1-45](esctest-F1-45.md). The full report, every failure with its reason, is in [esctest-F1-46/report.txt](esctest-F1-46/report.txt).

## What changed in the method

- **The runner** names a failure by the line of its exception, and requires esctest's closing line, whose counts must be those of the tests that it read; the checksums' failures now group as `ChecksumException`.
- **xterm's default colors**, black on white, which esctest's reset sets before each test and expects OSC 110 and 111 to bring back.
- **The window's operations** are allowed in the terminal, as xterm's `allowWindowOps` allows them, and esctest is told so with `--options=xtermWinopsEnabled`. Five tests that xterm fails without them stop being known bugs and count: 533 applicable tests, against 528.
- **The clipboard**: the terminal lets programs ask for it, and the runner answers each read with what the suite wrote last, as a user who allows the reads would.
- **A display** of 1920 by 1080 pixels, 240 by 67 cells, which the runner tells the terminal; the harness plays the window, giving the PTY the size that a program sets.

The whole run takes 40 seconds.

## Result

**496 of 533 applicable tests pass (93.1%)**, against 402 of 528 (76.1%) after F1-45's review fixes, with 34 known bugs of xterm left out.

| Step | Tests it took |
|---|---|
| The special colors: OSC 5 and 105, and OSC 4 and 104 from 256 | 12 |
| The VT420's rectangles: DECCRA, DECERA, DECFRA, DECSERA | 24 |
| The VT420's columns: DECIC, DECDC, DECBI, DECFI | 20 |
| ISO protection (SPA and EPA), CBT past the left margin, the title stack, mode 41, DECBKM | 7 |
| DECCOLM with modes 40 and 95, the resizes in cells (XTWINOPS 8 t, DECSLPP, DECSNLS), XTSAVE and XTRESTORE, xterm's title modes, OSC 52's base64 as xterm reads it | 14, 5 of them among those that the window's operations made applicable |
| The window's operations: iconify, move, resize in pixels, maximize, full screen, and the reports of the window and the display | 17 |

## What fails still

| Cause | Tests | Why it stays |
|---|---|---|
| X11's other color spaces, CIEXYZ, CIEuvY, CIExyY, CIELab, CIELuv, TekHVC and RGBi, in OSC 4, the dynamic colors and the special ones | 21 | xterm reads them through Xlib's color management, whose results come from the measured intensity tables of one monitor that Xlib keeps: `rgbi:0.5/0.5/0.5`, half of each component, reads back as `c1c1/bbbb/bbbb`. Matching them would mean copying those tables for the test's sake; programs write colors as `rgb:` or `#`, which the terminal reads |
| DA and DA2, which tell a VT220 with ANSI colors, `CSI ? 62 ; 22 c`, and `CSI > 1 ; 10 ; 0 c`, as Ghostty does, where esctest's level 5 asks for xterm's VT520, its 13 attributes, type 64 and a version of 314 or more | 4 | Telling a VT520 and xterm's version would have programs that read them take the terminal for an xterm of that patch level, and turn on what that xterm has |
| DECRQM of modes that the terminal does not have: KAM, which locks the keyboard, SRM, local echo, DECPFF and DECPEX, the printer's, DECHEBM, the Hebrew keyboard, and DECNRCM, the national replacement character sets | 6 | It answers 0, a mode it does not know, rather than claim modes that do nothing; a keyboard that programs can lock, and local echo, are not wanted, and there is no printer |
| DECSCL's levels: esctest checks that level 2 has no DECRQM, level 3 no DECSLRM and level 4 no DECNCSM, and DECRQSS of DECSCL after a request for level 65 | 4 | The terminal does at every level what it does, and takes levels 61 and 62 alone, the VT220's that DA tells |
| Known bugs of xterm that Bolt does not share: DECSED and DECSEL leave ISO protection alone, as the tests ask, where xterm respects it | 2 | esctest fails them with "Should have failed", as it expects xterm's bug |
