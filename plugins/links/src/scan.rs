//! Finding the URL or the path that a line's text holds around one of its
//! characters, as text writes them: between spaces, quotes or brackets,
//! with the punctuation of a sentence after them, and a path with the line
//! and the column that compilers and `grep -n` write after it.

/// What a line holds around a character.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Found {
    /// A URL of one of the schemes that open.
    Url(String),
    /// A path, absolute, from `~`, or relative, without the line and the
    /// column after it, as `src/main.rs:42:7` has them.
    Path(String),
}

/// The URLs' beginnings, their schemes with what follows them.
const SCHEMES: [&str; 4] = ["https://", "http://", "file://", "mailto:"];

/// The characters that end a URL: none is part of one, and text quotes or
/// encloses one with them.
const URL_ENDS: [char; 4] = ['"', '`', '<', '>'];

/// The characters that end a path, as text quotes, encloses or lists it
/// with them, or gives it as an option's value, as `--file=x`.
const PATH_ENDS: [char; 14] = [
    '"', '\'', '`', '(', ')', '[', ']', '{', '}', '<', '>', ',', ';', '|',
];

/// The punctuation that a sentence puts after a URL or a path, which is
/// not part of it.
const PUNCTUATION: [char; 7] = ['.', ',', ';', ':', '!', '?', '\''];

/// The URL or the path that `text` holds around its character `at`, by
/// the characters' count; `None` on a space.
pub fn find(text: &str, at: usize) -> Option<Found> {
    let chars: Vec<char> = text.chars().collect();
    if chars.get(at).is_none_or(|c| c.is_whitespace()) {
        return None;
    }
    let start = chars[..at]
        .iter()
        .rposition(|c| c.is_whitespace())
        .map_or(0, |space| space + 1);
    let end = chars[at..]
        .iter()
        .position(|c| c.is_whitespace())
        .map_or(chars.len(), |space| at + space);
    let word = &chars[start..end];
    let at = at - start;
    url(word, at)
        .map(Found::Url)
        .or_else(|| path(word, at).map(Found::Path))
}

/// The URL of `word` that its character `at` is part of: from the last
/// scheme that starts at it or before it, after no letter nor digit, to
/// the first character that ends a URL, without the punctuation after it,
/// nor its brackets that close none that it opens.
fn url(word: &[char], at: usize) -> Option<String> {
    let lower: Vec<char> = word.iter().map(char::to_ascii_lowercase).collect();
    let (begin, scheme) = (0..=at)
        .rev()
        .filter(|&i| i == 0 || !word[i - 1].is_alphanumeric())
        .find_map(|i| {
            SCHEMES
                .iter()
                .find(|scheme| {
                    lower[i..]
                        .iter()
                        .copied()
                        .take(scheme.len())
                        .eq(scheme.chars())
                })
                .map(|scheme| (i, scheme.len()))
        })?;
    let end = word[begin..]
        .iter()
        .position(|c| URL_ENDS.contains(c))
        .map_or(word.len(), |stop| begin + stop);
    let end = without_punctuation(word, begin, end);
    (end > begin + scheme && at < end).then(|| word[begin..end].iter().collect())
}

/// The end of `word`'s part from `begin` to `end` without the punctuation
/// after it, nor the closing brackets after it that close none within it.
fn without_punctuation(word: &[char], begin: usize, mut end: usize) -> usize {
    while end > begin {
        let last = word[end - 1];
        let part = &word[begin..end];
        let unclosed = |open, close| {
            last == close
                && part.iter().filter(|&&c| c == close).count()
                    > part.iter().filter(|&&c| c == open).count()
        };
        if PUNCTUATION.contains(&last) || unclosed('(', ')') || unclosed('[', ']') {
            end -= 1;
        } else {
            break;
        }
    }
    end
}

/// The path of `word` that its character `at` is part of: within the
/// characters that end a path, without the punctuation after it and the
/// line and the column after that, and, of a list of paths with `:` between
/// them, the one at `at`, as `$PATH` has them.
fn path(word: &[char], at: usize) -> Option<String> {
    if PATH_ENDS.contains(&word[at]) {
        return None;
    }
    let begin = word[..at]
        .iter()
        .rposition(|c| PATH_ENDS.contains(c) || *c == '=')
        .map_or(0, |stop| stop + 1);
    let end = word[at..]
        .iter()
        .position(|c| PATH_ENDS.contains(c))
        .map_or(word.len(), |stop| at + stop);
    let mut end = without_place(word, begin, end);
    while end > begin && word[end - 1] == '.' && !all_dots(&word[begin..end]) {
        end -= 1;
    }
    // A click on the line or the column takes the path before them.
    let at = at.min(end.saturating_sub(1));
    if at < begin || word[at] == ':' {
        return None;
    }
    let begin = word[begin..at]
        .iter()
        .rposition(|&c| c == ':')
        .map_or(begin, |colon| begin + colon + 1);
    let end = word[at..end]
        .iter()
        .position(|&c| c == ':')
        .map_or(end, |colon| at + colon);
    // Another scheme, as `ssh://host`, and what follows it, are no path.
    let slashes = |at: usize| word[at..].starts_with(&['/', '/']);
    if (begin > 0 && word[begin - 1] == ':' && slashes(begin))
        || (word.get(end) == Some(&':') && slashes(end + 1))
    {
        return None;
    }
    let path: String = word[begin..end].iter().collect();
    is_path(&path).then_some(path)
}

/// The end of `word`'s part from `begin` to `end` without the punctuation
/// after it, then the line and the column, as `:42:7`, and the `:` after
/// them that `grep -n` and compilers write.
fn without_place(word: &[char], begin: usize, end: usize) -> usize {
    let mut end = end;
    while end > begin && matches!(word[end - 1], ',' | ';' | '!' | '?' | ':') {
        end -= 1;
    }
    for _ in 0..2 {
        let digits = word[begin..end]
            .iter()
            .rev()
            .take_while(|c| c.is_ascii_digit())
            .count();
        if digits > 0 && end - digits > begin + 1 && word[end - digits - 1] == ':' {
            end -= digits + 1;
        }
    }
    end
}

/// Whether the last part of `path`, after its last `/`, is dots alone, as
/// `..` is.
fn all_dots(path: &[char]) -> bool {
    let last = path.rsplit(|&c| c == '/').next().unwrap_or_default();
    !last.is_empty() && last.iter().all(|&c| c == '.')
}

/// Whether `text` can be a path, as a file that `ls` lists: `.`, `..` or
/// `~`, or a text with a letter or a digit, but not a number, nor a
/// version, as `1.5` and `v1.2.3`.
fn is_path(text: &str) -> bool {
    if matches!(text, "." | ".." | "~") {
        return true;
    }
    let version = text.strip_prefix(['v', 'V']).unwrap_or(text);
    text.chars().any(char::is_alphanumeric)
        && !version.chars().all(|c| c.is_ascii_digit() || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What `text` holds around its first `^`, which marks the character
    /// and is not part of the text.
    fn at(marked: &str) -> Option<Found> {
        let at = marked.chars().position(|c| c == '^').unwrap();
        find(&marked.replacen('^', "", 1), at)
    }

    fn url(text: &str) -> Option<Found> {
        Some(Found::Url(text.to_owned()))
    }

    fn path(text: &str) -> Option<Found> {
        Some(Found::Path(text.to_owned()))
    }

    #[test]
    fn a_url_ends_before_the_punctuation_and_the_brackets_around_it() {
        assert_eq!(
            at("see ^https://lokyr.dev/a?b=1#c."),
            url("https://lokyr.dev/a?b=1#c")
        );
        assert_eq!(
            at("see https://lokyr.dev/^docs, then"),
            url("https://lokyr.dev/docs")
        );
        assert_eq!(at("(https://lokyr.^dev)"), url("https://lokyr.dev"));
        assert_eq!(at("[a](https://lokyr.^dev/x)."), url("https://lokyr.dev/x"));
        // Its own brackets stay.
        assert_eq!(
            at("https://en.wikipedia.org/wiki/Rust_(^programming_language)"),
            url("https://en.wikipedia.org/wiki/Rust_(programming_language)")
        );
        assert_eq!(at("<^http://a.b/c>"), url("http://a.b/c"));
        assert_eq!(at("'https://a.b/^c',"), url("https://a.b/c"));
        assert_eq!(at("\"https://a.b/^c\"x"), url("https://a.b/c"));
        assert_eq!(at("HTTPS://A.B/^C"), url("HTTPS://A.B/C"));
        assert_eq!(at("mailto:me@^lokyr.dev"), url("mailto:me@lokyr.dev"));
        assert_eq!(at("^file:///etc/hosts"), url("file:///etc/hosts"));
        // A scheme alone, or within a word, is none.
        assert_eq!(at("^https://"), None);
        assert_eq!(at("xhttps://a.^b"), None);
        // Of two, the one at the character.
        assert_eq!(at("http://a.b|https://^c.d"), url("https://c.d"));
    }

    #[test]
    fn a_path_ends_before_its_line_and_column() {
        assert_eq!(at("--> src/^main.rs:42:7"), path("src/main.rs"));
        assert_eq!(at("src/main.rs:4^2:7: error"), path("src/main.rs"));
        assert_eq!(at("src/main.rs:42^:"), path("src/main.rs"));
        assert_eq!(at("README.^md,"), path("README.md"));
        assert_eq!(at("see ^README.md."), path("README.md"));
        assert_eq!(at("~/^notes.txt"), path("~/notes.txt"));
        assert_eq!(at("^~"), path("~"));
        assert_eq!(at("cd ^.."), path(".."));
        assert_eq!(at("../^lib/x"), path("../lib/x"));
        assert_eq!(at("/usr/^bin"), path("/usr/bin"));
        assert_eq!(at("PATH=/usr/bin:/^bin"), path("/bin"));
        assert_eq!(at("PATH=/usr/^bin:/bin"), path("/usr/bin"));
        assert_eq!(at("--file=^build/out"), path("build/out"));
        assert_eq!(at("(src/^lib.rs)"), path("src/lib.rs"));
        assert_eq!(at("'a b/^c'"), path("b/c"));
        assert_eq!(at("^.zshrc"), path(".zshrc"));
        // A word alone, as `ls` lists a directory's files.
        assert_eq!(at("^Desktop"), path("Desktop"));
        // Nothing on the `:` between two paths, nor after another scheme.
        assert_eq!(at("/usr/bin^:/bin"), None);
        assert_eq!(at("ssh://host/^x"), None);
        assert_eq!(at("s^sh://host/x"), None);
    }

    #[test]
    fn a_word_that_is_no_path_finds_nothing() {
        for marked in [
            "^1.5",
            "v1.2.^3",
            "^// comment",
            "^/",
            "^...",
            "a ^ b",
            "^",
            "(^)",
        ] {
            assert_eq!(at(marked), None, "{marked}");
        }
        assert_eq!(find("abc", 3), None);
    }
}
