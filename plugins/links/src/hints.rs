//! Hints: what a line's text holds that the keyboard may pick, a URL, a
//! path, its line too, a hash of git's or an IP address, each of its
//! characters; and the labels of one or two letters that pick them, the
//! shortest for the first.

use crate::scan::{self, Found};

/// The letters of the labels, the home row's first.
pub const ALPHABET: &str = "asdfghjklqwertyuiopzxcvbnm";

/// What a hint is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A URL, or a path, and the line and the column after it, as the
    /// line's text writes them, as [`scan::find`] finds them.
    Link(Found),
    /// A hash of git's: from 7 to 40 hexadecimal digits, of a digit and a
    /// letter at least.
    Hash,
    /// An IPv4 address, and its port after it.
    Ip,
}

/// A hint of a line's text: its characters, from `first` to the one before
/// `end`, its text and what it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineHint {
    pub first: usize,
    pub end: usize,
    pub text: String,
    pub kind: Kind,
}

/// The hints of `text`, a line's, in their order, none over another: in
/// each word between spaces, a URL or a path, else a hash of git's or an
/// IPv4 address.
pub fn in_line(text: &str) -> Vec<LineHint> {
    let chars: Vec<char> = text.chars().collect();
    let mut found: Vec<LineHint> = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        if chars[at].is_whitespace() {
            at += 1;
            continue;
        }
        let end = chars[at..]
            .iter()
            .position(|c| c.is_whitespace())
            .map_or(chars.len(), |len| at + len);
        let hint = link(text, &chars, at, end).or_else(|| word(&chars, at, end));
        if let Some(hint) = hint
            && found.last().is_none_or(|last| last.end <= hint.first)
        {
            at = hint.end.max(end);
            found.push(hint);
            continue;
        }
        at = end;
    }
    found
}

/// The URL or the path of the word of `chars` from `first` to `end`, the
/// characters of `text`, of its first two characters, past a bracket or a
/// quote, with a path's line and column after it.
fn link(text: &str, chars: &[char], first: usize, end: usize) -> Option<LineHint> {
    for at in [first, first + 1].into_iter().filter(|&at| at < end) {
        for found in scan::find(text, at) {
            let (shown, kind) = match &found {
                Found::Url(url) => (url.clone(), found.clone()),
                Found::Path(path, place) => {
                    // A path of a slash or of a line after it, as `a/b` or
                    // `main.rs:42`, not every word.
                    if !path.contains('/') && place.is_none() {
                        continue;
                    }
                    let placed = place
                        .as_ref()
                        .map_or_else(|| path.clone(), |place| format!("{path}{}", place.text));
                    (placed, found.clone())
                }
            };
            if let Some((from, to)) = crate::span_of(text, &shown, at)
                && from >= first
                && to <= end.max(to)
                && from < end
            {
                let text: String = chars[from..to].iter().collect();
                return Some(LineHint {
                    first: from,
                    end: to,
                    text,
                    kind: Kind::Link(kind),
                });
            }
        }
    }
    None
}

/// The hash of git's or the IPv4 address of the word of `chars` from
/// `first` to `end`, past the punctuation around it.
fn word(chars: &[char], first: usize, end: usize) -> Option<LineHint> {
    let around = |c: &char| "()[]{}<>\"'`,.;:!?".contains(*c);
    let start = first + chars[first..end].iter().take_while(|c| around(c)).count();
    let stop = end
        - chars[start..end]
            .iter()
            .rev()
            .take_while(|c| around(c))
            .count();
    if start >= stop {
        return None;
    }
    let text: String = chars[start..stop].iter().collect();
    let kind = if is_hash(&text) {
        Kind::Hash
    } else if is_ip(&text) {
        Kind::Ip
    } else {
        return None;
    };
    Some(LineHint {
        first: start,
        end: stop,
        text,
        kind,
    })
}

/// Whether `text` is a hash of git's, whole or short: from 7 to 40
/// hexadecimal digits, lowercase, of a digit and a letter at least.
fn is_hash(text: &str) -> bool {
    (7..=40).contains(&text.len())
        && text
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
        && text.chars().any(|c| c.is_ascii_digit())
        && text.chars().any(|c| c.is_ascii_alphabetic())
}

/// Whether `text` is an IPv4 address, of four numbers from 0 to 255, and
/// of a port after it, as `10.0.0.1:8080`.
fn is_ip(text: &str) -> bool {
    let (address, port) = match text.split_once(':') {
        Some((address, port)) => (address, Some(port)),
        None => (text, None),
    };
    let parts: Vec<&str> = address.split('.').collect();
    let number = |part: &str| {
        !part.is_empty()
            && part.len() <= 3
            && part.chars().all(|c| c.is_ascii_digit())
            && part.parse::<u16>().is_ok_and(|n| n <= 255)
    };
    parts.len() == 4
        && parts.iter().all(|part| number(part))
        && port.is_none_or(|port| {
            !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) && port.len() <= 5
        })
}

/// The labels of `count` hints, of the letters of `alphabet`, none the
/// start of another: one letter each while the letters go round, then of
/// two, for the hints past the first ones, of letters that no label of one
/// takes; as many as the letters' pairs at most.
pub fn labels(count: usize, alphabet: &str) -> Vec<String> {
    let letters: Vec<char> = alphabet.chars().collect();
    let size = letters.len();
    if size == 0 {
        return Vec::new();
    }
    let count = count.min(size * size);
    if count <= size {
        return letters.iter().take(count).map(char::to_string).collect();
    }
    // The letters that start labels of two, from the end, as few as hold
    // the rest: `size - prefixes` of one, `prefixes * size` of two.
    let mut prefixes = 1;
    while size - prefixes + prefixes * size < count {
        prefixes += 1;
    }
    let singles = size - prefixes;
    let mut labels: Vec<String> = letters[..singles].iter().map(char::to_string).collect();
    'pairs: for first in &letters[singles..] {
        for second in &letters {
            if labels.len() == count {
                break 'pairs;
            }
            labels.push(format!("{first}{second}"));
        }
    }
    labels
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(line: &str) -> Vec<String> {
        in_line(line).into_iter().map(|hint| hint.text).collect()
    }

    #[test]
    fn a_line_holds_its_urls_paths_hashes_and_addresses() {
        assert_eq!(
            texts("   --> plugins/tabs/src/tests.rs:1404:13"),
            ["plugins/tabs/src/tests.rs:1404:13"]
        );
        assert_eq!(
            texts("see https://doc.rust-lang.org/error_codes/E0425.html."),
            ["https://doc.rust-lang.org/error_codes/E0425.html"]
        );
        assert_eq!(
            texts("commit 4482b9e9 on 192.168.1.20:8080, (deadbeef1)"),
            ["4482b9e9", "192.168.1.20:8080", "deadbeef1"]
        );
        assert_eq!(
            texts("main.rs:42 and src/lib.rs"),
            ["main.rs:42", "src/lib.rs"]
        );
        // Not every word: no slash nor line, a number alone, a hash of no
        // digit, an address of a part past 255.
        assert!(texts("just words, 1234567 cafebabe 10.0.0.256 a.b").is_empty());
        // Its characters' places.
        let hint = &in_line("x <https://a.io/b> y")[0];
        assert_eq!((hint.first, hint.end), (3, 17));
    }

    #[test]
    fn the_first_hints_take_labels_of_one_letter_and_the_rest_of_two_none_the_start_of_another() {
        assert_eq!(labels(3, "asd"), ["a", "s", "d"]);
        // Four of three letters: two of one, and two of the last's.
        assert_eq!(labels(4, "asd"), ["a", "s", "da", "ds"]);
        let many = labels(100, ALPHABET);
        assert_eq!(many.len(), 100);
        for (at, label) in many.iter().enumerate() {
            for (other_at, other) in many.iter().enumerate() {
                assert!(
                    at == other_at || !other.starts_with(label.as_str()),
                    "{label} {other}"
                );
            }
        }
        assert_eq!(many[0], "a");
        assert!(many.iter().all(|label| label.len() <= 2));
        // As many as the pairs at most.
        assert_eq!(labels(1000, "ab").len(), 4);
        assert!(labels(3, "").is_empty());
    }
}
