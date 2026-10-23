//! The recordings, the test corpora and a case for each known cause, fed to
//! the VT core and to `alacritty_terminal`: every stretch where their
//! screens differ must be one of [`EXPLAINED`], with its reason, and still
//! differ as the list keeps it.

use std::path::PathBuf;

use bolt_differential::Cause::{self, *};
use bolt_differential::{Difference, Pair, Screen};
use bolt_replay::recording::Recording;

/// The sequences that the parser's tests feed it.
mod corpus {
    include!("../../../crates/vt-core/testdata/parser-corpus.rs");
}

/// Markus Kuhn's UTF-8 stress test, whose lines the fuzz seeds take.
const KUHN: &[u8] = include_bytes!("../../../crates/vt-core/testdata/UTF-8-test.txt");

/// Inputs that show each cause on its own.
fn cases() -> Vec<(&'static str, Vec<u8>)> {
    let line_feed = [&[b'a'; 80][..], b"\n"].concat();
    let split_at_the_end = format!(
        "\x1b[2;1Hx{}\x1b[1;80H\x1b[4h\u{4e2d}",
        "\u{4e2d}".repeat(39)
    );
    vec![
        ("c1", b"a\x80b".to_vec()),
        ("jamo", "a\u{D7FF}b".as_bytes().to_vec()),
        ("emoji", "x\u{2600}\u{FE0F}y\r\n".as_bytes().to_vec()),
        ("british", b"\x1b(A#".to_vec()),
        ("deccolm", b"X\x1b[?3h".to_vec()),
        (
            "origin-moves",
            b"\x1b[5;20r\x1b[?6h\x1b[2;1H\x1b[BX".to_vec(),
        ),
        ("dch", b"ABCDEF\x1b[1;2H\x1b[130P".to_vec()),
        ("line-feed", line_feed),
        ("decom", b"\x1b[5;10H\x1b[?6lX".to_vec()),
        (
            "decom-after-the-restore",
            b"\x1b[5;10H\x1b[?1049h\x1b[2;3H\x1b[?1049;6l".to_vec(),
        ),
        (
            "restored-below",
            b"\x1b[?6h\x1b[20;1H\x1b7\x1b[5;10r\x1b8".to_vec(),
        ),
        ("decaln", b"7\x1b#8".to_vec()),
        ("tab", b"\x1b[80GN\t".to_vec()),
        ("unsupported", b"x\x1b6".to_vec()),
        ("scorc", b"x\x1b[2u".to_vec()),
        ("lines", b"M\x1b[M".to_vec()),
        ("edit-wrap", b"\x1b[80GN\x1b[P".to_vec()),
        ("region", b"y\x1b[24;30r".to_vec()),
        ("highlight", b"x\x1b[;T".to_vec()),
        ("ed1", b"3\n\x1b[1J".to_vec()),
        ("margins", b"\x1b[5;20r\x1b[5H\x1b[AX".to_vec()),
        ("wide-edit", b"\xe4\xb8\xad\x08\x1b[K".to_vec()),
        ("cbt", b"\x1b[80GN\x1b[Z".to_vec()),
        ("wide-insert", b"\x1b[4h\xe4\xb8\xad\x08\xc3\xa9".to_vec()),
        (
            "params",
            b"x\x1b[::::::::::::::::::::::::::::::::r".to_vec(),
        ),
        (
            "moved-head",
            [&b"\x1b[79Gx"[..], "\u{4e2d}\x1b[1;1H\x1b[P".as_bytes()].concat(),
        ),
        ("lnm", b"x\x1b[20h\x0b".to_vec()),
        ("cbt-first", b"\x1b[3gd\x1b[Z".to_vec()),
        (
            "insert-wrap",
            "\x1b[2;1Hxy\x1b[4h\x1b[1;80H\u{4e2d}".as_bytes().to_vec(),
        ),
        ("insert-wrap-splits-the-end", split_at_the_end.into_bytes()),
        ("rep-reset", b" \x1bc\x1b[b".to_vec()),
        (
            "no-wrap-wide",
            "\x1b[?7l\x1b[80G\u{4e2d}".as_bytes().to_vec(),
        ),
        (
            "wide-insert-wrap",
            "\x1b[2;79H\u{4e2d}\x1b[4h\x1b[1;80Hab".as_bytes().to_vec(),
        ),
        ("shift", b"\x0e\x1b8\x1b(0m".to_vec()),
        (
            "saved-origin",
            b"\x1b[2;5r\x1b7\x1b[?6h\x1b8\x1b[HX".to_vec(),
        ),
        (
            "alternate-shift",
            b"\x1b)0\x1b[?1049h\x0e\x1b[?1049lq".to_vec(),
        ),
        ("cha-in-origin", b"\x1b[2;5r\x1b[?6h\x1b[5GX".to_vec()),
        (
            "wide-repeat",
            "\x1b[78G\u{4e2d}\x1b[1G[\x1b[4h\x1b[2b".as_bytes().to_vec(),
        ),
        (
            "wrap-over-wide",
            "\x1b[2;1H\u{4e2d}\x1b[1;80H\u{6587}".as_bytes().to_vec(),
        ),
        (
            "wrap-over-a-tail",
            "\x1b[2;2H\u{4e2d}\x1b[1;80H\u{6587}".as_bytes().to_vec(),
        ),
        (
            "repeat-insert-wrap",
            "\x1b[2;1Hxy\x1b[1;1H\u{4e2d}\x1b[1;80H\x1b[4h\x1b[2b"
                .as_bytes()
                .to_vec(),
        ),
        (
            "repeat-over-wide",
            "\x1b[2;1H\u{4e2d}\x1b[1;78H\u{4e2d}\x1b[b"
                .as_bytes()
                .to_vec(),
        ),
        (
            "insert-at-the-last-row",
            "\x1b[1;2r\x1b[24;79H\u{4e2d}\x1b[4h\u{e9}"
                .as_bytes()
                .to_vec(),
        ),
        (
            "wide-repeat-below",
            "\x1b[3;71H\u{4e2d}\x1b[H[\x1b[4h\x1b[169b"
                .as_bytes()
                .to_vec(),
        ),
        (
            "repeat-over-its-row",
            "\x1b[1;2r\x1b[24;2H\u{4e2d}\x1b[4h\x1b[40b"
                .as_bytes()
                .to_vec(),
        ),
        (
            "repeat-under-a-head",
            "\x1b[1;2r\x1b[23;80H\u{4e2d}\x1b[4h\x1b[40b"
                .as_bytes()
                .to_vec(),
        ),
        (
            "repeat-no-wrap-wide",
            "\x1b[?7l\x1b[2G\u{4e2d}\x1b[40b".as_bytes().to_vec(),
        ),
        (
            "repeat-no-wrap-wide-insert",
            "\x1b[H\u{4e2d}\x1b[1;78H\x1b[?7l\x1b[4h\x1b[2b"
                .as_bytes()
                .to_vec(),
        ),
        (
            "rep-reset-at-the-bottom",
            b"x\x1bc\x1b[24;1H\x1b[100b".to_vec(),
        ),
        (
            "rep-reset-in-a-region",
            b"x\x1bc\x1b[2;5r\x1b[5;1H\x1b[100b".to_vec(),
        ),
        (
            "repeat-insert-wrap-later",
            "\x1b[2;1Hxy\x1b[1;1H\u{4e2d}\x1b[1;76H\x1b[4h\x1b[4b"
                .as_bytes()
                .to_vec(),
        ),
        // DEL, which the comparison holds back from `alacritty_terminal`: REP
        // after it repeats the same character in both, and the screens never
        // part.
        ("del", b"t\x7f\x1b[b".to_vec()),
        ("del-after-ignored-csi", b"\x1b[ 2xt\x7f\x1b[b".to_vec()),
        ("del-at-the-bottom", b"top\x1b[24;80Hu\x7f\x1b[b".to_vec()),
        (
            "del-after-ignored-string",
            b"\x1bP1<x\x1b\\t\x7f\x1b[b".to_vec(),
        ),
        (
            "del-wide-at-the-end",
            "\u{4e2d}\x1b[80G\x7f\x1b[b".as_bytes().to_vec(),
        ),
        (
            "del-after-a-move",
            "\u{4e2d}\x1b[5G\x7f\x1b[b".as_bytes().to_vec(),
        ),
        (
            "del-over-a-wide-start",
            "\x1b[1;80H\u{4e2d}o\x1b[2;1H\x7f\x1b[b".as_bytes().to_vec(),
        ),
        (
            "del-above-the-region",
            b"\x1b[2;24r\x1b[1;78Hm\x7f\x1b[3000b".to_vec(),
        ),
        (
            "del-past-the-screen",
            "\u{4e2d}\x7f\x1b[3000b".as_bytes().to_vec(),
        ),
        // DEL within a character whose bytes did not all come, which ends
        // it in the VT core, and in `alacritty_terminal` the NUL that the
        // comparison gives it instead.
        ("del-within-a-character", b"\xe4\xb8\x7f\xad".to_vec()),
    ]
}

/// The stretches where the screens differ, each with its cause, where the
/// screens differ first, and a digest of both at every point of it. A
/// stretch is named after its input and the points that it takes: the
/// chunks of a recording, from 0, or the lines of Kuhn's file, from 1.
#[rustfmt::skip]
const EXPLAINED: &[(&str, Cause, &str, u64)] = &[
    // Each cause on its own.
    ("case-c1", C1, "row 0, column 1: \"�\" against \"b\"", 0xb5fe304a559414ab),
    ("case-jamo", Jamo, "row 0, column 0: \"a\\u{d7ff}\" against \"a\"", 0x9469cc59c98f9c40),
    ("case-emoji", Emoji, "row 0, column 2: \"[tail]\" against \"y\"", 0x6ff0d7d1f29fd545),
    ("case-british", British, "row 0, column 0: \"£\" against \"#\"", 0x2c1b83d74bb4b15f),
    ("case-deccolm", Deccolm, "row 0, column 0: \"X\" against \" \"", 0x352ca43c69523416),
    ("case-origin-moves", OriginMoves, "row 6, column 0: \"X\" against \" \"", 0x23e825d4b739efc6),
    ("case-dch", Dch, "row 0, column 0: \"A\" against \" \"", 0x172453be47d33284),
    ("case-line-feed", LineFeed, "cursor (79, 1, false) against (79, 1, true)", 0x6c13d188f362634c),
    ("case-decom", Decom, "row 0, column 0: \"X\" against \" \"", 0x240256643c4545cf),
    ("case-decom-after-the-restore", Decom, "cursor (0, 0, false) against (9, 4, false)", 0x2ab210ef8c369348),
    ("case-restored-below", RestoredBelow, "cursor (0, 9, false) against (0, 19, false)", 0xcf6e2324fcfbede2),
    ("case-decaln", Decaln, "cursor (0, 0, false) against (1, 0, false)", 0x37fede7160f1291c),
    ("case-tab", Tab, "cursor (79, 0, true) against (0, 1, false)", 0x6eb63fd2180f4ee3),
    ("case-unsupported", Unsupported, "cursor (0, 0, false) against (1, 0, false)", 0xa4ee28bcf58572da),
    ("case-scorc", Scorc, "cursor (1, 0, false) against (0, 0, false)", 0x2b42b1d2cf45791c),
    ("case-lines", Lines, "cursor (0, 0, false) against (1, 0, false)", 0xdda5b7060e56b3fc),
    ("case-edit-wrap", EditWrap, "cursor (79, 0, false) against (79, 0, true)", 0x4aec1faf54c1ace2),
    ("case-region", Region, "cursor (1, 0, false) against (0, 0, false)", 0xfbfb1fed9cb91eba),
    ("case-highlight", Highlight, "row 0, column 0: \"x\" against \" \"", 0x58eaacd480a29545),
    ("case-ed1", Ed1, "row 0, column 0: \" \" against \"3\"", 0xeb69b17e7832fa20),
    ("case-margins", Margins, "row 3, column 0: \" \" against \"X\"", 0x560aa9f0c6444a0c),
    ("case-wide-edit", WideEdit, "row 0, column 0: \" \" against \"中\"", 0x13151b01a33fafa0),
    ("case-cbt", Cbt, "cursor (72, 0, false) against (72, 0, true)", 0xeef3735dbc9cb0d4),
    ("case-wide-insert", WideEdit, "row 0, column 0: \" \" against \"中\"", 0xff027f1b6102597a),
    ("case-params", Params, "cursor (0, 0, false) against (1, 0, false)", 0xa4ee28bcf58572da),
    ("case-moved-head", MovedHead, "row 0, column 78: \" \" against \"[head]\"", 0x812c5d7a0618045b),
    ("case-lnm", Lnm, "cursor (0, 1, false) against (1, 1, false)", 0x0247b3315edec024),
    ("case-cbt-first", CbtFirst, "cursor (0, 0, false) against (1, 0, false)", 0x15b9c9b48a1f391a),
    ("case-insert-wrap", InsertWrap, "row 1, column 2: \"x\" against \" \"", 0x9e9b52e4702fb9e6),
    ("case-insert-wrap-splits-the-end", InsertWrap, "row 0, column 79: \"[head]\" against \" \"", 0xb5e5ea6ef7780285),
    ("case-rep-reset", RepReset, "cursor (0, 0, false) against (1, 0, false)", 0xdda5b7060e56b3fc),
    ("case-rep-reset-at-the-bottom", RepReset, "row 22, column 0: \" \" against \"x\"", 0x4f213842b75bbe7d),
    ("case-rep-reset-in-a-region", RepReset, "row 3, column 0: \" \" against \"x\"", 0x4c914742fd71ecff),
    ("case-no-wrap-wide", NoWrapWide, "row 0, column 78: \"中\" against \" \"", 0x5295db36c915ae0e),
    ("case-wide-insert-wrap", WideEdit, "row 1, column 79: \" \" against \"中\"", 0xa6f67a1eb14b3262),
    ("case-shift", Shift, "row 0, column 0: \"└\" against \"m\"", 0x5d79a90666309afc),
    ("case-saved-origin", SavedOrigin, "row 0, column 0: \"X\" against \" \"", 0xbcdd974501f4f032),
    ("case-alternate-shift", Shift, "row 0, column 0: \"q\" against \"─\"", 0x4c60d4c23b8884c2),
    ("case-cha-in-origin", OriginMoves, "row 1, column 4: \"X\" against \" \"", 0x6009f262a6b721ca),
    ("case-wide-repeat", WideEdit, "row 0, column 1: \"[\" against \" \"", 0x0109ad620516917f),
    ("case-wrap-over-wide", WrapHead, "row 0, column 79: \"[head]\" against \" \"", 0x56f0605c0088c51f),
    ("case-wrap-over-a-tail", WrapHead, "row 0, column 79: \"[head]\" against \" \"", 0x56f0605c0088c51f),
    ("case-repeat-insert-wrap", InsertWrap, "row 1, column 4: \"x\" against \" \"", 0x3dc2722774760cda),
    ("case-repeat-over-wide", WrapHead, "row 0, column 79: \"[head]\" against \" \"", 0xbbc9d585bdff0f63),
    ("case-insert-at-the-last-row", WideEdit, "row 23, column 79: \" \" against \"中\"", 0x24688a3499ae64f6),
    ("case-wide-repeat-below", WideEdit, "row 2, column 7: \"[\" against \" \"", 0x3102b1f60d99c22a),
    ("case-repeat-over-its-row", WideEdit, "row 23, column 79: \" \" against \"中\"", 0x313398f00735dab8),
    ("case-repeat-under-a-head", WideEdit, "row 22, column 79: \"[head]\" against \" \"", 0xb2db1d38db2a73f5),
    ("case-repeat-no-wrap-wide", NoWrapWide, "row 0, column 77: \" \" against \"中\"", 0x193d0c9ebaec53c7),
    ("case-repeat-no-wrap-wide-insert", NoWrapWide, "row 0, column 77: \" \" against \"中\"", 0xe605cf6bb2d6c515),
    ("case-repeat-insert-wrap-later", InsertWrap, "row 1, column 4: \"x\" against \" \"", 0xe3075037962971da),
    // vttest: the screens after a switch to 132 columns or back.
    ("vttest:64-67", Deccolm, "row 0, column 0: \"*\" against \"T\"", 0x4eb3eb42cd76c133),
    ("vttest:84", Deccolm, "row 0, column 6: \"*\" against \" \"", 0x77dddcea318ef500),
    ("vttest:89", Deccolm, "cursor (73, 19, false) against (0, 0, false)", 0xe030094fbdbc4eb5),
    // The soft and the jump scrolls down, in a region of two rows that the
    // cursor goes up 24 rows in.
    ("vttest:96-97", OriginMoves, "row 0, column 0: \" \" against \"S\"", 0x7bd0f3c3002c3003),
    ("vttest:104-105", OriginMoves, "row 0, column 0: \" \" against \"J\"", 0x4a5d6634efd49f04),
    // The national sets, through G0 and G1 and the locking shifts, and the
    // single shifts, SS2 and SS3, which `alacritty_terminal` does not do.
    ("vttest:129-132", British, "row 7, column 12: \"£\" against \"#\"", 0x5a6d93d078e88f8f),
    ("vttest:135-136", British, "row 7, column 12: \"£\" against \"#\"", 0xf8afc658e9b985f5),
    ("vttest:139-142", British, "row 7, column 7: \"£\" against \"#\"", 0xfac562b1edf0772f),
    ("vttest:146-150", Unsupported, "row 7, column 10: \"£\" against \"#\"", 0xf9cddfc82bac2723),
    ("vttest:152-157", Unsupported, "row 7, column 10: \"£\" against \"#\"", 0xaa31cfa96eeb9dd3),
    // DCH of 130 cells, and the staircase of DCH at the last column, which
    // stays until the screen is cleared.
    ("vttest:305-306", Dch, "row 0, column 0: \"B\" against \" \"", 0xfed95cf218b11a0b),
    ("vttest:308-348", Dch, "row 1, column 78: \"B\" against \" \"", 0xddd7d643099fd6ff),
    // Kuhn's stress test.
    ("kuhn:75-76", C1, "row 0, column 38: \"�\" against \"\\\"\"", 0x22229fd13e001735),
    ("kuhn:89", Jamo, "row 0, column 31: \"\\\"\\u{d7ff}\" against \"\\\"\"", 0x702f1c05a2d930e6),
    ("kuhn:93", C1, "row 0, column 36: \"�\" against \"\\\"\"", 0x6fc773f7e876caac),
    ("kuhn:102", C1, "row 0, column 38: \"�\" against \"\\\"\"", 0x4f6c401126309817),
    ("kuhn:105-110", C1, "row 0, column 31: \"�\" against \"\\\"\"", 0x416c68f9d57267af),
    ("kuhn:114-115", C1, "row 0, column 4: \"�\" against \" \"", 0xf8cf5633ff8d609d),
    ("kuhn:154-157", C1, "row 0, column 62: \"�\" against \"\\\"\"", 0xa9963f2a119d5b2d),
    ("kuhn:169", C1, "row 0, column 23: \"�\" against \"\\\"\"", 0xf793a5ca698d054a),
    ("kuhn:208-211", C1, "row 0, column 38: \"�\" against \"\\\"\"", 0xb5fdf8c67d612847),
    ("kuhn:221-224", C1, "row 0, column 43: \"�\" against \"\\\"\"", 0x3a348eece47a00f7),
    ("kuhn:232-236", C1, "row 0, column 38: \"�\" against \"\\\"\"", 0x29362b9290f674da),
    ("kuhn:247", C1, "row 0, column 30: \"�\" against \"\\\"\"", 0xd69dd1106cbc4d59),
    ("kuhn:249", C1, "row 0, column 30: \"�\" against \"\\\"\"", 0x6b15dc49541b8e97),
    ("kuhn:251-252", C1, "row 0, column 30: \"�\" against \"\\\"\"", 0x83cd5da97bfe8edb),
    ("kuhn:257-259", C1, "row 0, column 48: \"�\" against \"\\\"\"", 0x54273c5ea7ee9c95),
    ("kuhn:261-263", C1, "row 0, column 48: \"�\" against \"\\\"\"", 0xb62bb7d4f372c87b),
];

fn recordings() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/vt-core/tests/recordings")
}

/// What the tests compare at each point of an input.
#[derive(Clone, Copy)]
enum Compare {
    /// The whole screen.
    Screen,
    /// The row of the VT core's cursor, for an input that writes a row at a
    /// time, with the cursor.
    CursorRow,
}

/// An input: its name, the size of the screens, its pieces, each a point
/// to compare at, what to compare there, and what to feed after each
/// comparison.
struct Input {
    name: String,
    size: (u16, u16),
    pieces: Vec<Vec<u8>>,
    compare: Compare,
    after: &'static [u8],
}

impl Input {
    fn whole(name: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            name: name.into(),
            size: (80, 24),
            pieces: vec![bytes],
            compare: Compare::Screen,
            after: b"",
        }
    }
}

fn inputs() -> Vec<Input> {
    let mut inputs = Vec::new();
    for name in ["vim", "htop", "tmux", "less", "fzf", "vttest"] {
        let bytes = std::fs::read(recordings().join(format!("{name}.cast"))).unwrap();
        let recording = Recording::parse(&bytes).unwrap();
        inputs.push(Input {
            name: name.to_owned(),
            size: recording.size.expect("a recording has its size"),
            pieces: recording.chunks,
            compare: Compare::Screen,
            after: b"",
        });
    }
    // Each line on a row of its own, as no line is as wide.
    inputs.push(Input {
        name: "kuhn".to_owned(),
        size: (160, 24),
        pieces: KUHN
            .split(|&byte| byte == b'\n')
            .map(<[u8]>::to_vec)
            .collect(),
        compare: Compare::CursorRow,
        after: b"\r\n",
    });
    for (index, input) in corpus::CORPUS.iter().enumerate() {
        inputs.push(Input::whole(format!("corpus-{index}"), input.to_vec()));
    }
    inputs.push(Input::whole("corpus-all", corpus::CORPUS.concat()));
    for (name, bytes) in cases() {
        inputs.push(Input::whole(format!("case-{name}"), bytes));
    }
    inputs
}

/// A stretch where the screens differ.
struct Stretch {
    /// The input's name, and the points that the stretch takes.
    name: String,
    /// Where the screens differ first, and the rows that differ there.
    first: Difference,
    /// The digest of both screens at each of its points.
    digest: u64,
}

impl Stretch {
    fn is(&self, (name, _, summary, digest): (&str, Cause, &str, u64)) -> bool {
        self.name == name && self.first.summary == summary && self.digest == digest
    }
}

/// FNV-1a, which no toolchain changes, over the screens of a point.
fn digest(mut hash: u64, ours: &Screen, theirs: &Screen) -> u64 {
    for byte in format!("{ours:?}{theirs:?}").bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// FNV-1a's start.
const DIGEST: u64 = 0xcbf2_9ce4_8422_2325;

/// Every stretch of every input where the screens differ.
fn stretches() -> Vec<Stretch> {
    let mut found = Vec::new();
    for input in inputs() {
        let mut pair = Pair::new(input.size.0, input.size.1);
        // The open stretch's first point, first difference and digest.
        let mut open: Option<(usize, Difference, u64)> = None;
        let first = usize::from(input.name == "kuhn");
        let name = |start: usize, last: usize| match (input.pieces.len(), start == last) {
            (1, _) => input.name.clone(),
            (_, true) => format!("{}:{start}", input.name),
            (_, false) => format!("{}:{start}-{last}", input.name),
        };
        for (point, piece) in (first..).zip(&input.pieces) {
            pair.feed(piece);
            let (mut ours, mut theirs) = (pair.bolt(), pair.alacritty());
            if let Compare::CursorRow = input.compare {
                let y = ours.cursor.1;
                (ours, theirs) = (ours.only_row(y), theirs.only_row(y));
            }
            match (ours.first_difference(&theirs), &mut open) {
                (Some(difference), None) => {
                    open = Some((point, difference, digest(DIGEST, &ours, &theirs)));
                }
                (Some(_), Some((_, _, hash))) => *hash = digest(*hash, &ours, &theirs),
                (None, Some(_)) => {
                    let (start, first, digest) = open.take().expect("a stretch is open");
                    found.push(Stretch {
                        name: name(start, point - 1),
                        first,
                        digest,
                    });
                }
                (None, None) => {}
            }
            pair.feed(input.after);
        }
        if let Some((start, first_difference, digest)) = open {
            let last = first + input.pieces.len() - 1;
            found.push(Stretch {
                name: name(start, last),
                first: first_difference,
                digest,
            });
        }
    }
    found
}

#[test]
fn every_divergence_is_explained() {
    let found = stretches();
    let unexplained: Vec<String> = found
        .iter()
        .filter(|stretch| !EXPLAINED.iter().any(|&explained| stretch.is(explained)))
        .map(
            |Stretch {
                 name,
                 first,
                 digest,
             }| {
                let Difference { summary, context } = first;
                format!("    ({name:?}, REASON, {summary:?}, {digest:#018x}), // {context}")
            },
        )
        .collect();
    let stale: Vec<&str> = EXPLAINED
        .iter()
        .filter(|&&explained| !found.iter().any(|stretch| stretch.is(explained)))
        .map(|&(name, ..)| name)
        .collect();
    assert!(
        unexplained.is_empty() && stale.is_empty(),
        "unexplained, or different from what the list keeps:\n{}\nexplained, but no longer so: \
         {stale:?}",
        unexplained.join("\n")
    );
}

#[test]
fn each_case_parts_the_screens_for_its_cause_when_fed_a_byte_at_a_time() {
    for (name, bytes) in cases() {
        let name = format!("case-{name}");
        let listed = EXPLAINED.iter().find(|explained| explained.0 == name);
        let cause = listed.map(|&(_, cause, ..)| cause);
        assert_eq!(
            bolt_differential::explore(80, 24, &bytes),
            Ok(cause),
            "{name}"
        );
    }
}

#[test]
fn each_stretch_of_the_recordings_starts_for_its_cause() {
    for &(name, cause, ..) in EXPLAINED {
        let Some((recording, points)) = name.split_once(':') else {
            continue;
        };
        if recording == "kuhn" {
            continue;
        }
        let start: usize = points.split('-').next().unwrap().parse().unwrap();
        let bytes = std::fs::read(recordings().join(format!("{recording}.cast"))).unwrap();
        let chunks = Recording::parse(&bytes).unwrap().chunks;
        let size = Recording::parse(&bytes).unwrap().size.unwrap();
        let found = bolt_differential::explore_after(
            size.0,
            size.1,
            &chunks[..start].concat(),
            &chunks[start],
        );
        assert_eq!(found, Ok(Some(cause)), "{name}");
    }
}
