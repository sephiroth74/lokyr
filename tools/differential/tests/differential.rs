//! The recordings and the test corpora, fed to the VT core and to
//! `alacritty_terminal`: every point where their screens differ must be one
//! of [`EXPLAINED`], with its reason, and each of those must still differ.

use std::collections::BTreeMap;
use std::path::PathBuf;

use bolt_differential::Pair;
use bolt_replay::recording::Recording;

/// The sequences that the parser's tests feed it.
mod corpus {
    include!("../../../crates/vt-core/testdata/parser-corpus.rs");
}

/// Markus Kuhn's UTF-8 stress test, whose lines the fuzz seeds take.
const KUHN: &[u8] = include_bytes!("../../../crates/vt-core/testdata/UTF-8-test.txt");

/// Why the screens of ill-formed UTF-8 differ.
const REPLACEMENT: &str = "the VT core replaces each maximal subpart of ill-formed UTF-8 with U+FFFD, \
     as the Unicode Standard recommends; vte replaces a run of them with fewer, and drops a lone \
     continuation byte";

/// Why U+D7FF takes another width.
const JAMO: &str = "U+D7FF, of Hangul Jamo Extended-B, takes no cell in the VT core, as the whole \
     block does with glibc's wcwidth; unicode-width, which alacritty_terminal uses, gives its \
     unassigned code points one";

/// Why the national sets differ.
const BRITISH: &str = "ESC ( A and its kin designate the British set in the VT core, whose # is \
     a pound sign, as on DEC's terminals and in xterm; alacritty_terminal knows only ASCII and \
     DEC's line drawing";

/// Why DECCOLM leaves other screens.
const DECCOLM: &str = "DECCOLM does nothing in the VT core without the window's operations, as in \
     xterm without mode 40; alacritty_terminal clears the screen and the scroll region, without \
     changing the width";

/// Why vttest's scroll tests draw elsewhere.
const CUD: &str = "CUD from above the scroll region stops at its bottom margin in the VT core, as in \
     xterm and as vttest expects; alacritty_terminal goes on to the last row";

/// The points where the screens differ, each with why.
const EXPLAINED: &[(&str, &str)] = &[
    ("kuhn-102", REPLACEMENT),
    ("kuhn-105", REPLACEMENT),
    ("kuhn-106", REPLACEMENT),
    ("kuhn-107", REPLACEMENT),
    ("kuhn-108", REPLACEMENT),
    ("kuhn-109", REPLACEMENT),
    ("kuhn-110", REPLACEMENT),
    ("kuhn-114", REPLACEMENT),
    ("kuhn-115", REPLACEMENT),
    ("kuhn-154", REPLACEMENT),
    ("kuhn-155", REPLACEMENT),
    ("kuhn-156", REPLACEMENT),
    ("kuhn-157", REPLACEMENT),
    ("kuhn-169", REPLACEMENT),
    ("kuhn-208", REPLACEMENT),
    ("kuhn-209", REPLACEMENT),
    ("kuhn-210", REPLACEMENT),
    ("kuhn-211", REPLACEMENT),
    ("kuhn-221", REPLACEMENT),
    ("kuhn-222", REPLACEMENT),
    ("kuhn-223", REPLACEMENT),
    ("kuhn-224", REPLACEMENT),
    ("kuhn-232", REPLACEMENT),
    ("kuhn-233", REPLACEMENT),
    ("kuhn-234", REPLACEMENT),
    ("kuhn-235", REPLACEMENT),
    ("kuhn-236", REPLACEMENT),
    ("kuhn-247", REPLACEMENT),
    ("kuhn-249", REPLACEMENT),
    ("kuhn-251", REPLACEMENT),
    ("kuhn-252", REPLACEMENT),
    ("kuhn-257", REPLACEMENT),
    ("kuhn-258", REPLACEMENT),
    ("kuhn-259", REPLACEMENT),
    ("kuhn-261", REPLACEMENT),
    ("kuhn-262", REPLACEMENT),
    ("kuhn-263", REPLACEMENT),
    ("kuhn-75", REPLACEMENT),
    ("kuhn-76", REPLACEMENT),
    ("kuhn-93", REPLACEMENT),
    ("kuhn-89", JAMO),
    (
        "kuhn-all",
        "the whole stress test, whose lines differ as those above do",
    ),
    ("vttest@vt100-sets-1", BRITISH),
    ("vttest@shifts-1", BRITISH),
    ("vttest@locking-shifts-1", BRITISH),
    ("vttest@single-shifts-1", BRITISH),
    ("vttest@single-shifts-2", BRITISH),
    ("vttest@movements-3", DECCOLM),
    ("vttest@movements-4", DECCOLM),
    ("vttest@vt102-11", DECCOLM),
    ("vttest@vt102-12", DECCOLM),
    ("vttest@screen-7", CUD),
    ("vttest@screen-9", CUD),
];

fn recordings() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/vt-core/tests/recordings")
}

/// Every point of every input where the screens differ, with how.
fn divergences() -> BTreeMap<String, String> {
    let mut found = BTreeMap::new();
    for name in ["vim", "htop", "tmux", "less", "fzf", "vttest"] {
        let bytes = std::fs::read(recordings().join(format!("{name}.cast"))).unwrap();
        let recording = Recording::parse(&bytes).unwrap();
        let (cols, rows) = recording.size.expect("a recording has its size");
        let mut pair = Pair::new(cols, rows);
        let mut fed = 0;
        let mut points: Vec<(usize, &str)> = recording
            .marks
            .iter()
            .map(|(chunks, label)| (*chunks, label.as_str()))
            .collect();
        points.push((recording.chunks.len(), "exit"));
        for (chunks, label) in points {
            for chunk in &recording.chunks[fed..chunks] {
                pair.feed(chunk);
            }
            fed = chunks;
            if let Some(difference) = pair.difference() {
                found.insert(format!("{name}@{label}"), difference);
            }
        }
    }
    let lines = KUHN
        .split(|&byte| byte == b'\n')
        .enumerate()
        .filter(|(_, line)| !line.is_ascii());
    let inputs = corpus::CORPUS
        .iter()
        .enumerate()
        .map(|(index, input)| (format!("corpus-{index}"), input.to_vec()))
        .chain([("corpus-all".to_owned(), corpus::CORPUS.concat())])
        .chain(lines.map(|(index, line)| (format!("kuhn-{}", index + 1), line.to_vec())))
        .chain([("kuhn-all".to_owned(), KUHN.to_vec())]);
    for (name, input) in inputs {
        let mut pair = Pair::new(80, 24);
        pair.feed(&input);
        if let Some(difference) = pair.difference() {
            found.insert(name, difference);
        }
    }
    found
}

#[test]
fn every_divergence_is_explained() {
    let found = divergences();
    let unexplained: Vec<String> = found
        .iter()
        .filter(|(name, _)| !EXPLAINED.iter().any(|(explained, _)| explained == name))
        .map(|(name, difference)| format!("{name}: {difference}"))
        .collect();
    let stale: Vec<&str> = EXPLAINED
        .iter()
        .map(|&(name, _)| name)
        .filter(|name| !found.contains_key(*name))
        .collect();
    assert!(
        unexplained.is_empty() && stale.is_empty(),
        "unexplained:\n{}\nexplained, but no longer different: {stale:?}",
        unexplained.join("\n")
    );
}
