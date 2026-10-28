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

/// The characters that end a URL: none is part of one, and text quotes,
/// encloses or separates one with them, CJK's punctuation too.
const URL_ENDS: [char; 26] = [
    '"', '`', '<', '>', '{', '|', '}', '、', '。', '，', '：', '；', '！', '？', '「', '」', '『',
    '』', '（', '）', '【', '】', '〈', '〉', '《', '》',
];

/// The characters that open a pair, with those that close it: a URL after
/// one of them ends at its pair's closing one.
const PAIRS: [(char, char); 16] = [
    ('(', ')'),
    ('[', ']'),
    ('{', '}'),
    ('<', '>'),
    ('\'', '\''),
    ('"', '"'),
    ('`', '`'),
    ('“', '”'),
    ('‘', '’'),
    ('«', '»'),
    ('（', '）'),
    ('「', '」'),
    ('『', '』'),
    ('【', '】'),
    ('〈', '〉'),
    ('《', '》'),
];

/// The characters that end a path, as text quotes, encloses or lists it
/// with them, or gives it as an option's value, as `--file=x`.
const PATH_ENDS: [char; 14] = [
    '"', '\'', '`', '(', ')', '[', ']', '{', '}', '<', '>', ',', ';', '|',
];

/// What a URL can have after it that is not part of it: a sentence's
/// punctuation, and Markdown's marks of emphasis, as GitHub's links take
/// them.
const URL_TRAILING: [char; 10] = ['.', ',', ';', ':', '!', '?', '\'', '*', '_', '~'];

/// The URL or the path that `text` holds around its character `at`, by
/// the characters' count; `None` on a space. Text between quotes that has
/// spaces is one, as `git status` and Python's tracebacks quote a path;
/// a space after a backslash is part of a path.
pub fn find(text: &str, at: usize) -> Option<Found> {
    let chars: Vec<char> = text.chars().collect();
    if chars.get(at).is_none_or(|c| c.is_whitespace()) {
        return None;
    }
    if let Some((begin, end)) = quoted(&chars, at) {
        let found = in_word(&chars[begin..end], at - begin);
        if found.is_some() {
            return found;
        }
    }
    let unescaped = |i: usize| chars[i].is_whitespace() && (i == 0 || chars[i - 1] != '\\');
    let start = (0..at)
        .rev()
        .find(|&i| unescaped(i))
        .map_or(0, |space| space + 1);
    let end = (at..chars.len())
        .find(|&i| unescaped(i))
        .unwrap_or(chars.len());
    in_word(&chars[start..end], at - start)
}

/// The URL, or else the path, of `word` that its character `at` is part
/// of.
fn in_word(word: &[char], at: usize) -> Option<Found> {
    url(word, at)
        .map(Found::Url)
        .or_else(|| path(word, at).map(Found::Path))
}

/// The text between the quotes, `"` or `'`, around `chars`' character `at`,
/// when it has a space: from a quote after no letter nor digit to the
/// next of the same, before none, with no other between.
fn quoted(chars: &[char], at: usize) -> Option<(usize, usize)> {
    ['"', '\''].into_iter().find_map(|quote| {
        let open = (0..at)
            .rev()
            .find(|&i| chars[i] == quote && (i == 0 || !chars[i - 1].is_alphanumeric()))?;
        let close = (at + 1..chars.len()).find(|&i| {
            chars[i] == quote && chars.get(i + 1).is_none_or(|c| !c.is_alphanumeric())
        })?;
        let inside = &chars[open + 1..close];
        (!inside.contains(&quote) && inside.iter().any(|c| c.is_whitespace()))
            .then_some((open + 1, close))
    })
}

/// The URL of `word` that its character `at` is part of: of the URLs that
/// start at a scheme after no ASCII letter nor digit, the first that holds
/// `at`, so that a URL that holds another opens whole wherever it is
/// clicked.
fn url(word: &[char], at: usize) -> Option<String> {
    let lower: Vec<char> = word.iter().map(char::to_ascii_lowercase).collect();
    (0..=at.min(word.len().saturating_sub(1)))
        .filter(|&i| i == 0 || !word[i - 1].is_ascii_alphanumeric())
        .filter_map(|i| {
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
        })
        .find_map(|(begin, scheme)| {
            let end = url_end(word, begin, scheme);
            (end > begin + scheme && at < end).then(|| word[begin..end].iter().collect())
        })
}

/// Where the URL of `word` that starts at `begin`, of a scheme of `scheme`
/// characters, ends: at the closing character of a pair that it comes
/// after, the brackets nested within it counted; at the first character
/// that ends a URL; after CJK text, at the first character past ASCII;
/// without the punctuation after it, nor the brackets that close none
/// within it.
fn url_end(word: &[char], begin: usize, scheme: usize) -> usize {
    let before = begin.checked_sub(1).map(|i| word[i]);
    let pair = before.and_then(|open| PAIRS.iter().copied().find(|&(o, _)| o == open));
    let after_cjk = before.is_some_and(|c| !c.is_ascii() && c.is_alphanumeric());
    let mut depth = 0_usize;
    let mut end = word.len();
    for (i, &c) in word.iter().enumerate().skip(begin + scheme) {
        if let Some((open, close)) = pair {
            if c == close && depth == 0 {
                end = i;
                break;
            }
            if open != close && c == open {
                depth += 1;
            } else if c == close {
                depth -= 1;
            }
        }
        if URL_ENDS.contains(&c) || (after_cjk && !c.is_ascii()) {
            end = i;
            break;
        }
    }
    without_punctuation(word, begin, end, &URL_TRAILING)
}

/// The end of `word`'s part from `begin` to `end` without `trailing`'s
/// characters after it, nor the closing brackets after it that close none
/// within it; the brackets counted once.
fn without_punctuation(word: &[char], begin: usize, mut end: usize, trailing: &[char]) -> usize {
    let count = |c: char| word[begin..end].iter().filter(|&&x| x == c).count();
    // How many more of each bracket close than open.
    let mut parens = count(')') as isize - count('(') as isize;
    let mut brackets = count(']') as isize - count('[') as isize;
    while end > begin {
        let last = word[end - 1];
        if last == ')' && parens > 0 {
            parens -= 1;
        } else if last == ']' && brackets > 0 {
            brackets -= 1;
        } else if !trailing.contains(&last) {
            break;
        }
        end -= 1;
    }
    end
}

/// The path of `word` that its character `at` is part of: within the
/// characters that end a path, without the punctuation after it and the
/// line and the column after that, as `:42:7`, `(42,7)` or `#L42`, and, of
/// a list of paths with `:` between them, the one at `at`, as `$PATH` has
/// them; neither a path of another machine, as `user@host:/etc/hosts`,
/// nor what follows another scheme.
fn path(word: &[char], at: usize) -> Option<String> {
    // A click on a place in brackets takes the path before them.
    let at = place_in_brackets(word, at).unwrap_or(at);
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
    // A sentence's full stop, and `ls -F`'s marks of a link and a program.
    while end > begin
        && ((word[end - 1] == '.' && !all_dots(&word[begin..end]))
            || matches!(word[end - 1], '@' | '*'))
    {
        end -= 1;
    }
    // A click on the line or the column takes the path before them.
    let at = at.min(end.saturating_sub(1));
    if at < begin || word[at] == ':' {
        return None;
    }
    let segment = begin;
    let begin = word[begin..at]
        .iter()
        .rposition(|&c| c == ':')
        .map_or(begin, |colon| begin + colon + 1);
    let end = word[at..end]
        .iter()
        .position(|&c| c == ':')
        .map_or(end, |colon| at + colon);
    // Another scheme, as `ssh://host`, and what follows it, are no path,
    // nor a path of another machine, after `user@host:`.
    let slashes = |at: usize| word[at..].starts_with(&['/', '/']);
    let before = &word[segment..begin.saturating_sub(1).max(segment)];
    let host = before.rsplit(|&c| c == ':').next().unwrap_or_default();
    if (begin > 0 && word[begin - 1] == ':' && (slashes(begin) || host.contains(&'@')))
        || (word.get(end) == Some(&':') && slashes(end + 1))
    {
        return None;
    }
    let path: String = word[begin..end].iter().collect();
    let path = path.replace("\\ ", " ");
    is_path(&path).then_some(path)
}

/// The character before the brackets of a place, as `(42,7)` after a path,
/// that `at`, a character of the place, is in.
fn place_in_brackets(word: &[char], at: usize) -> Option<usize> {
    if !(word[at].is_ascii_digit() || word[at] == ',') {
        return None;
    }
    let open = word[..at].iter().rposition(|&c| c == '(')?;
    let close = at + word[at..].iter().position(|&c| c == ')')?;
    let place = &word[open + 1..close];
    let before = open.checked_sub(1)?;
    (place.iter().all(|c| c.is_ascii_digit() || *c == ',') && !PATH_ENDS.contains(&word[before]))
        .then_some(before)
}

/// The end of `word`'s part from `begin` to `end` without the punctuation
/// after it, then the line and the column, as `:42:7`, and the `:` after
/// them that `grep -n` and compilers write, or a line of a URL of a
/// forge, as `#L42` or `#L42-L50`.
fn without_place(word: &[char], begin: usize, end: usize) -> usize {
    let mut end = end;
    while end > begin && matches!(word[end - 1], ',' | ';' | '!' | '?' | ':') {
        end -= 1;
    }
    if let Some(hash) = word[begin..end].iter().rposition(|&c| c == '#') {
        let hash = begin + hash;
        let line = &word[hash + 1..end];
        if line.first() == Some(&'L')
            && line.len() > 1
            && line
                .iter()
                .all(|c| c.is_ascii_digit() || *c == 'L' || *c == '-')
        {
            end = hash;
        }
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
        assert_eq!(at("'a b/^c'"), path("a b/c"));
        assert_eq!(at("^.zshrc"), path(".zshrc"));
        // A word alone, as `ls` lists a directory's files.
        assert_eq!(at("^Desktop"), path("Desktop"));
        // Nothing on the `:` between two paths, nor after another scheme.
        assert_eq!(at("/usr/bin^:/bin"), None);
        assert_eq!(at("ssh://host/^x"), None);
        assert_eq!(at("s^sh://host/x"), None);
    }

    #[test]
    fn a_url_ends_at_the_pair_that_encloses_it() {
        for (marked, wanted) in [
            (
                "[^https://lokyr.dev](https://lokyr.dev)",
                "https://lokyr.dev",
            ),
            (
                "[https://lokyr.dev](https://lokyr.^dev)",
                "https://lokyr.dev",
            ),
            ("fetch('https://api.x.y/^z').then(r", "https://api.x.y/z"),
            ("urlopen('https://x.y/^z').read()", "https://x.y/z"),
            ("\\url{https://x.y/^z}", "https://x.y/z"),
            ("\\href{https://x.y/^z}{text}", "https://x.y/z"),
            ("**https://x.^y**", "https://x.y"),
            ("|https://x.^y|b|", "https://x.y"),
            ("（https://example.^com）", "https://example.com"),
            ("「https://example.^com」", "https://example.com"),
            (
                "[w](https://en.wikipedia.org/wiki/Rust_(^lang))",
                "https://en.wikipedia.org/wiki/Rust_(lang)",
            ),
        ] {
            assert_eq!(at(marked), url(wanted), "{marked}");
        }
    }

    #[test]
    fn a_url_that_holds_another_opens_whole() {
        let wayback = "https://web.archive.org/web/2020/https://example.com/page";
        for marked in [
            "https://web.^archive.org/web/2020/https://example.com/page",
            "https://web.archive.org/web/2020/https://example.^com/page",
        ] {
            assert_eq!(at(marked), url(wayback), "{marked}");
        }
        assert_eq!(
            at("https://x.y/login?next=https://x.y/dash&token=^abc"),
            url("https://x.y/login?next=https://x.y/dash&token=abc")
        );
    }

    #[test]
    fn a_url_after_cjk_text_ends_with_it() {
        assert_eq!(
            at("詳細はhttps://example.com/^docsを参照"),
            url("https://example.com/docs")
        );
        // After a space, a URL keeps its characters past ASCII.
        assert_eq!(
            at("see https://ja.wikipedia.org/wiki/^日本"),
            url("https://ja.wikipedia.org/wiki/日本")
        );
    }

    #[test]
    fn a_path_with_spaces_is_one_between_quotes_or_escaped() {
        assert_eq!(at("?? \"My ^File.txt\""), path("My File.txt"));
        assert_eq!(
            at("  File \"/Users/x/My ^Project/app.py\", line 3"),
            path("/Users/x/My Project/app.py")
        );
        assert_eq!(at("/Users/x/My\\ ^File.txt"), path("/Users/x/My File.txt"));
        // Quotes that hold no space, or a word's apostrophe, change nothing.
        assert_eq!(at("it's \"^README.md\""), path("README.md"));
        assert_eq!(at("don't open ^notes.txt"), path("notes.txt"));
    }

    #[test]
    fn a_path_leaves_its_marks_and_its_places_out() {
        assert_eq!(at("^link@"), path("link"));
        assert_eq!(at("pro^g*"), path("prog"));
        assert_eq!(at("src/app.ts(4^2,7)"), path("src/app.ts"));
        assert_eq!(at("src/^app.ts(42,7)"), path("src/app.ts"));
        assert_eq!(at("src/^main.rs#L42"), path("src/main.rs"));
        assert_eq!(at("src/main.rs#L4^2-L50"), path("src/main.rs"));
        // A path of another machine is none of this one.
        assert_eq!(at("user@host:/etc/^passwd"), None);
        assert_eq!(at("PATH=/usr/^bin:/bin"), path("/usr/bin"));
    }

    #[test]
    fn closing_brackets_after_a_url_take_linear_time() {
        let text = format!("https://a.b/{}", ")".repeat(200_000));
        assert_eq!(find(&text, 3), url("https://a.b/"));
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
