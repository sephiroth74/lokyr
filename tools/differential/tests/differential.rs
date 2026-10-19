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
        ("del", b"t\x7f\x1b[b".to_vec()),
        (
            "insert-wrap",
            "\x1b[2;1Hxy\x1b[4h\x1b[1;80H\u{4e2d}".as_bytes().to_vec(),
        ),
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
    ("case-del", Del, "row 0, column 1: \"t\" against \" \"", 0xf5ff7417417c11f2),
    ("case-insert-wrap", InsertWrap, "row 1, column 2: \"x\" against \" \"", 0x9e9b52e4702fb9e6),
    ("case-rep-reset", RepReset, "cursor (0, 0, false) against (1, 0, false)", 0xdda5b7060e56b3fc),
    ("case-no-wrap-wide", NoWrapWide, "row 0, column 78: \"中\" against \" \"", 0x5295db36c915ae0e),
    ("case-wide-insert-wrap", WideEdit, "row 1, column 79: \" \" against \"中\"", 0xa6f67a1eb14b3262),
    ("case-shift", Shift, "row 0, column 0: \"└\" against \"m\"", 0x5d79a90666309afc),
    // vttest: the screens after a switch to 132 columns or back.
    ("vttest:64-67", Deccolm, "row 0, column 0: \"*\" against \"T\"", 0x4eb3eb42cd76c133),
    ("vttest:84", Deccolm, "row 0, column 6: \"*\" against \" \"", 0x77dddcea318ef500),
    ("vttest:89", Deccolm, "cursor (73, 19, false) against (0, 0, false)", 0xe030094fbdbc4eb5),
    // The soft and the jump scrolls down, in a region of two rows that the
    // cursor goes up 24 rows in.
    ("vttest:96-97", OriginMoves, "row 0, column 0: \" \" against \"S\"", 0x7bd0f3c3002c3003),
    ("vttest:104-105", OriginMoves, "row 0, column 0: \" \" against \"J\"", 0x4a5d6634efd49f04),
    // The national sets, through G0 and G1, the shifts and the single shifts.
    ("vttest:129-132", British, "row 7, column 12: \"£\" against \"#\"", 0x5a6d93d078e88f8f),
    ("vttest:135-136", British, "row 7, column 12: \"£\" against \"#\"", 0xf8afc658e9b985f5),
    ("vttest:139-142", British, "row 7, column 7: \"£\" against \"#\"", 0xfac562b1edf0772f),
    ("vttest:146-150", British, "row 7, column 10: \"£\" against \"#\"", 0xf9cddfc82bac2723),
    ("vttest:152-157", British, "row 7, column 10: \"£\" against \"#\"", 0xaa31cfa96eeb9dd3),
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
