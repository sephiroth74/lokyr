# esctest, before the compatibility fixes

Measured on 2026-10-18 with `bolt-esctest`, the first run of [esctest](https://github.com/ThomasDickey/esctest2) in Bolt's headless terminal, to find what the compatibility fixes have to do. The full report, every failure with its reason, is in [esctest-F1-44/report.txt](esctest-F1-44/report.txt).

To repeat the measurement, run `just esctest`. It downloads esctest2 at the pinned commit into `target/esctest2` and runs it in the harness; arguments go to esctest, as `just esctest --include=CUPTests`.

## Method

- **esctest2** at commit `2798f12149a19c3295e9b4853ab2da4b2eff1b2b`, run by macOS's `python3` with `--expected-terminal=xterm`, so that xterm's known bugs are told apart.
- **The terminal** is 80 by 25, the size that esctest asks for at each test's start, with the default options, but for title reports, which xterm allows when it is told to. It does not answer clipboard reads, as no one confirms them.
- **Timeout**: esctest waits half a second for each byte of a reply, rather than its default of a second, since a headless terminal replies at once or never.
- **The pass rate** counts the tests that passed among those that passed or failed. Known bugs of xterm are left out, as esctest leaves them out for xterm; but a test of one of them that passes fails, with "Should have failed", and counts as a failure, though the terminal does what the test asks. The whole run takes two and a half minutes.

## Result

**163 of 526 applicable tests pass (31.0%)**, with 41 known bugs of xterm left out and none skipped.

| Cause | Tests | Where |
|---|---|---|
| No DECRQCRA, the checksum of a rectangle, through which esctest reads every cell it checks | 232 | Most classes: ED, EL, DL, IL, ICH, DCH, ECH, SU, SD, DECSED, DECSEL, the rectangle operations (DECCRA, DECFRA, DECERA, DECSERA), DECIC, DECDC, DECSTBM, DECSET, BS, LF, IND, RI, NEL, FF, REP, VT, APC, PM, SOS, DCS |
| DECRQM of ANSI modes answers 0, not recognized, where xterm tells 2 (reset) or 4 (permanently reset) | 23 | DECRQM |
| Colors | 33 | ChangeColor and ChangeDynamicColor (7 each): a color set in a form that is not read is reported black; ChangeSpecialColor (14) and ResetSpecialColor (5): OSC 5 and 105 are not answered |
| XTWINOPS reports that are not answered: the display's size (15 t, 19 t), the screen's in pixels without a cell size (14 t), the window's place (13 t) and state (11 t); a resize (8 t) does nothing; the title stack's test | 19 | XtermWinops |
| DECDSR reports that are not answered: printer, user-defined keys, keyboard, locator, data integrity, sessions, DECXCPR, DECMSR, DECCKSR | 10 | DECDSR |
| DECRQSS of settings that are not reported: DECSASD, DECSSDT, DECSACE, DECSNLS, DECSLPP, DECSCL | 6 | DECRQSS |
| DECCOLM, 132 columns, which does not resize | 3 | DECSET, RIS |
| DA and DA2 report another level and type than esctest's VT level 5 asks | 4 | DA, DA2 |
| DECSCL, the conformance level | 2 | DECSCL |
| Other modes: DECLRMM reset by DECSTR, reverse wraparound with BS, mode 41 (the curses fix), and xterm's save and restore of modes | 5 | DECSET, XtermSave |
| Cursor moves at the margins and the edges: BS's reverse wrap, CUB, CUD, CPL, CBT, VPA, VPR, RI, DECBI, DECFI, DECSTR and DECSET's tite-inhibit | 25 | BS, CUB, CUD, CPL, CBT, VPA, VPR, RI, DECBI, DECFI, DECSTR, DECSETTiteInhibit |
| DECID, ESC Z, which is not answered | 1 | DECID |

The cursor's failures need a look one at a time. Some may come from the cursor being where an earlier assertion left it, rather than from the sequence that is tested. Once DECRQCRA answers, the tests that read cells will tell whether the sequences that they test are right. Most of the 232 test sequences that the terminal already does, so the pass rate should rise well before the other causes are dealt with.
