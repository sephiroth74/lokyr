//! The recordings, the test corpora and a case for each known cause, fed to
//! the VT core and to `alacritty_terminal`: every stretch where their
//! screens differ must be one of [`EXPLAINED`], with its reason, and still
//! differ as the list keeps it.

use std::path::PathBuf;

use bolt_differential::{Difference, Pair, Screen};
use bolt_replay::recording::Recording;

/// The sequences that the parser's tests feed it.
mod corpus {
    include!("../../../crates/vt-core/testdata/parser-corpus.rs");
}

/// Markus Kuhn's UTF-8 stress test, whose lines the fuzz seeds take.
const KUHN: &[u8] = include_bytes!("../../../crates/vt-core/testdata/UTF-8-test.txt");

/// Why lone bytes of ill-formed UTF-8 differ.
const C1: &str = "vte takes a lone byte from 0x80 to 0x9F, which is ill-formed UTF-8, for a C1 \
     control, which alacritty_terminal ignores; the VT core replaces it with U+FFFD, as it does each \
     maximal subpart of ill-formed UTF-8, as the Unicode Standard recommends";

/// Why U+D7FF takes another width.
const JAMO: &str = "U+D7FF, of Hangul Jamo Extended-B, takes no cell in the VT core, as the whole \
     block does with glibc's wcwidth; unicode-width, which alacritty_terminal uses, gives its \
     unassigned code points one";

/// Why an emoji presentation sequence takes another width.
const EMOJI: &str = "VS16 after a pictographic base makes the cluster an emoji presentation \
     sequence in the VT core, two cells wide, as Unicode's emoji presentation says; \
     alacritty_terminal gives each codepoint the width of its own, one for U+2600 and none for \
     U+FE0F";

/// Why the national sets differ.
const BRITISH: &str = "ESC ( A and its kin designate the British set in the VT core, whose # is \
     a pound sign, as on DEC's terminals and in xterm; alacritty_terminal knows only ASCII and \
     DEC's line drawing";

/// Why DECCOLM leaves other screens.
const DECCOLM: &str = "DECCOLM needs the window's operations in the VT core, which these tests \
     leave off, as they are by default, and does nothing without them; alacritty_terminal clears \
     the screen and the scroll region, and puts the cursor home, without changing the width";

/// Why CUU and CUD in origin mode go elsewhere.
const ORIGIN_MOVES: &str = "CUU and CUD in origin mode move within the scroll region in the VT \
     core, as in xterm; alacritty_terminal adds the top margin to the cursor's row a second time, \
     as it moves to the row that it reached as if that were relative to the margin";

/// Why DCH leaves other cells.
const DCH: &str = "DCH of more cells than are right of the cursor deletes those in the VT core, \
     as in xterm; alacritty_terminal clears as many cells back from the row's end, some of them \
     left of the cursor";

/// Why the character after LF at the last column goes elsewhere.
const LINE_FEED: &str = "LF at the last column ends the pending wrap in the VT core, as in xterm; \
     alacritty_terminal keeps it, so that the next character goes to the row after";

/// Why the cursor stays after DECOM's reset.
const DECOM: &str = "DECOM's reset puts the cursor home in the VT core, as its set does, as in \
     xterm and on DEC's terminals; alacritty_terminal puts it home only on the set";

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
    ]
}

/// The stretches where the screens differ, each with why, where the
/// screens differ first, and a digest of both at every point of it. A
/// stretch is named after its input and the points that it takes: the
/// chunks of a recording, from 0, or the lines of Kuhn's file, from 1.
#[rustfmt::skip]
const EXPLAINED: &[(&str, &str, &str, u64)] = &[
    // Each cause on its own.
    ("case-c1", C1, "row 0, column 1: \"�\" against \"b\"", 0xb5fe304a559414ab),
    ("case-jamo", JAMO, "row 0, column 0: \"a\\u{d7ff}\" against \"a\"", 0x9469cc59c98f9c40),
    ("case-emoji", EMOJI, "row 0, column 2: \"[tail]\" against \"y\"", 0x6ff0d7d1f29fd545),
    ("case-british", BRITISH, "row 0, column 0: \"£\" against \"#\"", 0x2c1b83d74bb4b15f),
    ("case-deccolm", DECCOLM, "row 0, column 0: \"X\" against \" \"", 0x352ca43c69523416),
    ("case-origin-moves", ORIGIN_MOVES, "row 6, column 0: \"X\" against \" \"", 0x23e825d4b739efc6),
    ("case-dch", DCH, "row 0, column 0: \"A\" against \" \"", 0x172453be47d33284),
    ("case-line-feed", LINE_FEED, "cursor (79, 1, false) against (79, 1, true)", 0x6c13d188f362634c),
    ("case-decom", DECOM, "row 0, column 0: \"X\" against \" \"", 0x240256643c4545cf),
    // vttest: the screens after a switch to 132 columns or back.
    ("vttest:64-67", DECCOLM, "row 0, column 0: \"*\" against \"T\"", 0x4eb3eb42cd76c133),
    ("vttest:84", DECCOLM, "row 0, column 6: \"*\" against \" \"", 0x77dddcea318ef500),
    ("vttest:89", DECCOLM, "cursor (73, 19, false) against (0, 0, false)", 0xe030094fbdbc4eb5),
    // The soft and the jump scrolls down, in a region of two rows that the
    // cursor goes up 24 rows in.
    ("vttest:96-97", ORIGIN_MOVES, "row 0, column 0: \" \" against \"S\"", 0x7bd0f3c3002c3003),
    ("vttest:104-105", ORIGIN_MOVES, "row 0, column 0: \" \" against \"J\"", 0x4a5d6634efd49f04),
    // The national sets, through G0 and G1, the shifts and the single shifts.
    ("vttest:129-132", BRITISH, "row 7, column 12: \"£\" against \"#\"", 0x5a6d93d078e88f8f),
    ("vttest:135-136", BRITISH, "row 7, column 12: \"£\" against \"#\"", 0xf8afc658e9b985f5),
    ("vttest:139-142", BRITISH, "row 7, column 7: \"£\" against \"#\"", 0xfac562b1edf0772f),
    ("vttest:146-150", BRITISH, "row 7, column 10: \"£\" against \"#\"", 0xf9cddfc82bac2723),
    ("vttest:152-157", BRITISH, "row 7, column 10: \"£\" against \"#\"", 0xaa31cfa96eeb9dd3),
    // DCH of 130 cells, and the staircase of DCH at the last column, which
    // stays until the screen is cleared.
    ("vttest:305-306", DCH, "row 0, column 0: \"B\" against \" \"", 0xfed95cf218b11a0b),
    ("vttest:308-348", DCH, "row 1, column 78: \"B\" against \" \"", 0xddd7d643099fd6ff),
    // Kuhn's stress test.
    ("kuhn:75-76", C1, "row 0, column 38: \"�\" against \"\\\"\"", 0x22229fd13e001735),
    ("kuhn:89", JAMO, "row 0, column 31: \"\\\"\\u{d7ff}\" against \"\\\"\"", 0x702f1c05a2d930e6),
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
    fn is(&self, (name, _, summary, digest): (&str, &str, &str, u64)) -> bool {
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
