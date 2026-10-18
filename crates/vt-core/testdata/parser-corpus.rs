// The sequences that the parser's tests feed it, which the differential
// tests of the terminal feed it too: this file is included by both.

/// Sequences that programs send, with malformed ones mixed in.
pub const CORPUS: &[&[u8]] = &[
    // Text, controls and UTF-8.
    b"hello, world\r\n",
    "naïve café — ✓ 🎉 日本語\r\n".as_bytes(),
    b"tab\there\x08\x07\x00\x0e\x0f",
    // SGR.
    b"\x1b[0m\x1b[1;31mred\x1b[m",
    b"\x1b[38;5;196m\x1b[48;2;10;20;30mrgb\x1b[38;2;255;255;255;48;2;0;0;0m",
    b"\x1b[38:2::255:128:0m\x1b[4:3m\x1b[58:5:9m\x1b[:m\x1b[1::2m",
    // Cursor, erase, scroll region and tabs.
    b"\x1b[H\x1b[2J\x1b[10;20H\x1b[K\x1b[3A\x1b[B\x1b[;H\x1b[5;H",
    b"\x1b[5;20r\x1bM\x1bD\x1bE\x1bH\x1b[3g\x1b[2@\x1b[3P",
    // Modes.
    b"\x1b[?1049h\x1b[?25l\x1b[?2004h\x1b[?1000;1006h\x1b[?2026h\x1b[?2026l",
    b"\x1b[4h\x1b[20l\x1b=\x1b>",
    // Charsets, saving the cursor, alignment test and reset.
    b"\x1b(0lqk\x1b(B\x1b)0\x0e\x0f\x1b(%5",
    b"\x1b7\x1b8\x1b#8\x1bc",
    // Queries and reports.
    b"\x1b[c\x1b[>c\x1b[=c\x1b[5n\x1b[6n\x1b[>0q\x1b[?2026$p\x1b[18t",
    b"\x1b[?1;2c\x1b[?2026;2$y\x1b[8;24;80t",
    // Cursor style, kitty keyboard, mouse and bracketed paste.
    b"\x1b[2 q\x1b[>1u\x1b[?u\x1b[<u\x1b[=1;1u",
    b"\x1b[<0;10;20M\x1b[<0;10;20m\x1b[200~pasted\x1b[201~",
    // OSC.
    b"\x1b]0;title\x07\x1b]2;w\xc3\xa9b\x1b\\",
    b"\x1b]7;file://host/home/user\x1b\\",
    b"\x1b]8;id=1;https://example.com\x1b\\link\x1b]8;;\x1b\\",
    b"\x1b]52;c;aGVsbG8=\x07\x1b]52;c;?\x07",
    b"\x1b]133;A\x07$ \x1b]133;B\x07ls\r\n\x1b]133;C\x07\x1b]133;D;0\x07",
    b"\x1b]10;?\x07\x1b]11;rgb:0000/0000/0000\x1b\\\x1b]4;1;#ff0000\x07",
    b"\x1b]9;4;1;50\x07\x1b]\x07\x1b];\x07",
    // DCS: DECRQSS, XTGETTCAP, a report, sixel, and synchronized updates.
    b"\x1bP$qm\x1b\\\x1bP+q544e\x1b\\\x1bP1$r0m\x1b\\",
    b"\x1bPq#0;2;0;0;0#1;2;100;100;0#1~~@@vv@@~~$-\x1b\\",
    b"\x1bP=1s\x1b\\text\x1bP=2s\x1b\\",
    // APC: kitty graphics.
    b"\x1b_Gf=100,a=T;iVBORw0KGgo=\x1b\\after",
    // SOS and PM.
    b"\x1bXsos\x1b\\\x1b^pm\x1b\\after",
    // Interrupted and malformed sequences.
    b"\x1b[1;2\x18x\x1b[3\x1ay\x1b]0;abc\x18def\x1bP1$qab\x1ax",
    b"\x1b[1\x1b[2m\x1b[1\nA\x1b\x1b\x1b[m",
    b"\x1b[1?hx\x1b[ 1py\x1b[\x7f1\x7fm\x1b(%!Bz",
    b"\x1bP1<qdata\x1b\\x\x1b]0;a\x1b[1m",
];
