//! Hints: what a line's text holds that the keyboard may pick, a match of
//! a pattern of the config, a URL, a path, its line too, a hash of git's or
//! an IP address, each of its characters; and the labels of one or two
//! letters that pick them, the shortest for the first.

use regex_automata::meta::Regex;

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
    /// A match of a pattern of the config, and the URL that it opens, of
    /// its template, if it has one.
    Pattern(Option<String>),
}

/// A pattern of the config: what it matches, and the template of the URL
/// that a match opens, if it has one, of `$0` for the whole match and `$1`
/// to `$9` for its groups.
#[derive(Clone, Debug)]
pub struct Pattern {
    pub regex: Regex,
    pub url: Option<String>,
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

/// The hints of `text`, a line's, in their order, none over another: the
/// matches of `patterns` first, then, in each word between spaces that no
/// backslash escapes, its URLs, at each scheme after a character other
/// than a letter or a digit, else a path at its start, past a bracket or a
/// quote, or the text between the quotes that start it, else a hash of
/// git's or an IPv4 address, over none of them. Of a time linear in the
/// line, but for the patterns' searches.
pub fn in_line(text: &str, patterns: &[Pattern]) -> Vec<LineHint> {
    let chars: Vec<char> = text.chars().collect();
    let matched = matches(text, patterns);
    // Whether the hint from `first` to `end` meets a match, whose spans,
    // none over another, end in their order.
    let meets_match = |first: usize, end: usize| {
        let before = matched.partition_point(|other| other.first < end);
        before > 0 && matched[before - 1].end > first
    };
    let mut found: Vec<LineHint> = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        if chars[at].is_whitespace() {
            at += 1;
            continue;
        }
        let end = (at..chars.len())
            .find(|&i| chars[i].is_whitespace() && (i == 0 || chars[i - 1] != '\\'))
            .unwrap_or(chars.len());
        let mut next = end;
        for hint in in_word(&chars, at, end) {
            if found.last().is_none_or(|last| last.end <= hint.first)
                && !meets_match(hint.first, hint.end)
            {
                next = next.max(hint.end);
                found.push(hint);
            }
        }
        at = next;
    }
    found.extend(matched);
    found.sort_by_key(|hint| hint.first);
    found
}

/// The hints of the word of `chars` from `first` to `end`, in their order:
/// its URLs; else the URL or the path between the quotes that start it,
/// past the word, when they hold a space, or the path at its start, past a
/// bracket or a quote; else its hash or its address.
fn in_word(chars: &[char], first: usize, end: usize) -> Vec<LineHint> {
    let hint = |from: usize, to: usize, kind: Kind| LineHint {
        first: from,
        end: to,
        text: chars[from..to].iter().collect(),
        kind,
    };
    let word = &chars[first..end];
    let urls = scan::urls(word);
    if !urls.is_empty() {
        return urls
            .into_iter()
            .map(|(from, to)| {
                let url = chars[first + from..first + to].iter().collect();
                hint(first + from, first + to, Kind::Link(Found::Url(url)))
            })
            .collect();
    }
    if let Some((open, close)) = quoted(chars, first) {
        let inside = &chars[open..close];
        if let Some(to) = scan::urls(inside)
            .first()
            .and_then(|&(from, to)| (from == 0).then_some(to))
        {
            let url = inside[..to].iter().collect();
            return vec![hint(open, open + to, Kind::Link(Found::Url(url)))];
        }
        if let Some(found) = path_of(inside, 0, open) {
            return vec![found];
        }
    }
    let path = [0, 1]
        .into_iter()
        .filter(|&at| at < word.len())
        .find_map(|at| path_of(word, at, first));
    if let Some(found) = path {
        return vec![found];
    }
    self::word(chars, first, end).into_iter().collect()
}

/// The hint of the path of `word`, whose first character is the line's
/// `offset`, that its character `at` is part of, with its line and its
/// column after it: of a slash or of a line after it, as `a/b` or
/// `main.rs:42`, not every word.
fn path_of(word: &[char], at: usize, offset: usize) -> Option<LineHint> {
    let (path, (from, to), place) = scan::path_span(word, at)?;
    if !path.contains('/') && place.is_none() {
        return None;
    }
    let to = to + place.as_ref().map_or(0, |place| place.text.chars().count());
    Some(LineHint {
        first: offset + from,
        end: offset + to,
        text: word[from..to].iter().collect(),
        kind: Kind::Link(Found::Path(path, place)),
    })
}

/// The text between the quotes, `"` or `'`, that `chars`' word at `first`
/// starts with, when it holds a space, as `git status` quotes a path: up to
/// the next quote of the same, before no letter nor digit; of where the
/// text starts and ends.
fn quoted(chars: &[char], first: usize) -> Option<(usize, usize)> {
    let quote = *chars.get(first).filter(|c| matches!(c, '"' | '\''))?;
    let close = first + 1 + chars[first + 1..].iter().position(|&c| c == quote)?;
    let inside = &chars[first + 1..close];
    (chars.get(close + 1).is_none_or(|c| !c.is_alphanumeric())
        && inside.iter().any(|c| c.is_whitespace()))
    .then_some((first + 1, close))
}

/// The matches of `patterns` in `text`, none over another, the first
/// pattern's before the later's, in their order in the line.
fn matches(text: &str, patterns: &[Pattern]) -> Vec<LineHint> {
    // The index of the character at each byte that starts one.
    let mut chars_at = vec![0; text.len() + 1];
    for (index, (byte, _)) in text.char_indices().enumerate() {
        chars_at[byte] = index;
    }
    chars_at[text.len()] = text.chars().count();
    // The matches taken, by their first characters, of their ends.
    let mut taken: std::collections::BTreeMap<usize, LineHint> = std::collections::BTreeMap::new();
    for pattern in patterns {
        let mut caps = pattern.regex.create_captures();
        let mut at = 0;
        while at <= text.len() {
            let input = regex_automata::Input::new(text).span(at..text.len());
            pattern.regex.search_captures(&input, &mut caps);
            let Some(whole) = caps.get_match() else {
                break;
            };
            let (start, end) = (whole.start(), whole.end());
            // A match of nothing moves on a character.
            at = if end > start {
                end
            } else {
                text[end..]
                    .chars()
                    .next()
                    .map_or(text.len() + 1, |c| end + c.len_utf8())
            };
            if end == start {
                continue;
            }
            let (first, last) = (chars_at[start], chars_at[end]);
            // Of the matches taken, none over another, the one that starts
            // last before this one ends is the one that could meet it.
            if taken
                .range(..last)
                .next_back()
                .is_some_and(|(_, other)| other.end > first)
            {
                continue;
            }
            let group = |index: usize| {
                caps.get_group(index)
                    .map_or("", |span| &text[span.start..span.end])
            };
            taken.insert(
                first,
                LineHint {
                    first,
                    end: last,
                    text: text[start..end].to_owned(),
                    kind: Kind::Pattern(pattern.url.as_deref().map(|url| expand(url, group))),
                },
            );
        }
    }
    taken.into_values().collect()
}

/// `template` of each `$0` to `$9` the text of `group` of its number, the
/// whole match of 0, and of `$$` a `$`.
pub fn expand<'a>(template: &str, group: impl Fn(usize) -> &'a str) -> String {
    let mut expanded = String::new();
    let mut chars = template.chars().peekable();
    while let Some(character) = chars.next() {
        if character != '$' {
            expanded.push(character);
            continue;
        }
        match chars.peek().copied() {
            Some('$') => {
                chars.next();
                expanded.push('$');
            }
            Some(digit @ '0'..='9') => {
                chars.next();
                expanded.push_str(group(digit as usize - '0' as usize));
            }
            _ => expanded.push('$'),
        }
    }
    expanded
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

/// Whether `alphabet` is one of labels: two lowercase ASCII letters or
/// digits at least, each once, as a label takes them and a capital copies.
pub fn is_alphabet(alphabet: &str) -> bool {
    let letters: Vec<char> = alphabet.chars().collect();
    letters.len() >= 2
        && letters
            .iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && letters
            .iter()
            .enumerate()
            .all(|(at, c)| !letters[at + 1..].contains(c))
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
        in_line(line, &[])
            .into_iter()
            .map(|hint| hint.text)
            .collect()
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
        let hint = &in_line("x <https://a.io/b> y", &[])[0];
        assert_eq!((hint.first, hint.end), (3, 17));
    }

    #[test]
    fn a_url_glued_to_the_text_before_it_is_a_hint_at_its_scheme() {
        assert_eq!(
            texts("read [the docs](https://lokyr.dev/docs) first"),
            ["https://lokyr.dev/docs"]
        );
        assert_eq!(
            texts(r#"<a href="https://a.example/x">"#),
            ["https://a.example/x"]
        );
        assert_eq!(
            texts("--url=https://b.example/y?q=1."),
            ["https://b.example/y?q=1"]
        );
        // A line of JSON, of no space: each of its URLs.
        assert_eq!(
            texts(r#"{"a":"https://c.example/1","b":["http://d.example/2"]}"#),
            ["https://c.example/1", "http://d.example/2"]
        );
        // Not after a letter or a digit, as a word that holds the scheme.
        assert!(texts("xhttps://e.example 9https://f.example").is_empty());
    }

    #[test]
    fn a_path_of_escaped_spaces_or_between_quotes_is_a_hint_of_its_text() {
        let found = in_line(r"cat my\ notes/to\ do.md now", &[]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].text, r"my\ notes/to\ do.md");
        assert_eq!(
            found[0].kind,
            Kind::Link(Found::Path("my notes/to do.md".to_owned(), None))
        );
        assert_eq!(
            texts(r#"modified:   "src/a dir/file.rs:12" and src/b.rs"#),
            ["src/a dir/file.rs:12", "src/b.rs"]
        );
    }

    #[test]
    // A test's own clock, as it measures the plugin's time.
    #[allow(clippy::disallowed_methods)]
    fn a_long_line_is_scanned_in_linear_time() {
        // 600 columns over 65 rows of words, URLs, paths, quotes, hashes
        // and addresses.
        let piece = "see https://lokyr.dev/p/1 src/main.rs:4 'a b' c0ffee1 10.0.0.1 x ";
        let line: String = piece.chars().cycle().take(600 * 65).collect();
        let start = std::time::Instant::now();
        let found = in_line(&line, &[]);
        let took = start.elapsed();
        assert!(found.len() > 2000, "{}", found.len());
        // Within 10 ms of an optimized build, and a debug build's slack.
        let most = if cfg!(debug_assertions) { 400 } else { 10 };
        assert!(took.as_millis() < most, "{took:?}");
    }

    #[test]
    fn a_pattern_s_matches_come_first_of_their_url_and_over_the_others() {
        let jira = Pattern {
            regex: Regex::new(r"PROJ-(\d+)").unwrap(),
            url: Some("https://jira.example.com/browse/$0?n=$1&x=$$".to_owned()),
        };
        let plain = Pattern {
            regex: Regex::new(r"[a-f0-9]{8}").unwrap(),
            url: None,
        };
        let found = in_line("fix PROJ-42 in deadbee1, see é PROJ-7", &[jira, plain]);
        let shown: Vec<(&str, usize, &Kind)> = found
            .iter()
            .map(|hint| (hint.text.as_str(), hint.first, &hint.kind))
            .collect();
        assert_eq!(
            shown,
            [
                (
                    "PROJ-42",
                    4,
                    &Kind::Pattern(Some(
                        "https://jira.example.com/browse/PROJ-42?n=42&x=$".to_owned()
                    ))
                ),
                ("deadbee1", 15, &Kind::Pattern(None)),
                (
                    "PROJ-7",
                    31,
                    &Kind::Pattern(Some(
                        "https://jira.example.com/browse/PROJ-7?n=7&x=$".to_owned()
                    ))
                ),
            ]
        );
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
        // An alphabet of lowercase letters and digits, each once.
        assert!(is_alphabet(ALPHABET) && is_alphabet("12"));
        assert!(
            !is_alphabet("a") && !is_alphabet("aa") && !is_alphabet("aB") && !is_alphabet("a-")
        );
    }
}
