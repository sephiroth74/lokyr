//! What a search looks for: literal text or a regex, with the options that
//! apply to both.

use std::fmt;
use std::ops::Range;
use std::sync::atomic::AtomicBool;

use memchr::memmem::Finder;
use regex_automata::Input;
use regex_automata::meta::{BuildError, Regex};
use regex_automata::util::syntax;

use crate::Cancelled;

/// Whether case counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Case {
    /// `a` finds `a` only.
    Sensitive,
    /// `a` finds `a` and `A`. Literal text folds ASCII letters only, which
    /// the text and the literal compare in lower case; a regex folds
    /// Unicode's simple case.
    Insensitive,
}

/// What to find, and how.
#[derive(Clone, Debug)]
pub struct Query {
    matcher: Matcher,
    case: Case,
    whole_word: bool,
}

#[derive(Clone, Debug)]
enum Matcher {
    Literal(Finder<'static>),
    Regex(Box<Regex>),
}

/// `text` with each of the regex syntax's own characters escaped, so that
/// a regex of it finds it as it is.
fn escaped(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(
            c,
            '\\' | '.'
                | '+'
                | '*'
                | '?'
                | '('
                | ')'
                | '|'
                | '['
                | ']'
                | '{'
                | '}'
                | '^'
                | '$'
                | '#'
                | '&'
                | '-'
                | '~'
        ) {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

/// A regex that does not compile.
#[derive(Debug)]
pub struct PatternError(Box<BuildError>);

impl fmt::Display for PatternError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the regex does not compile: {}", self.0)
    }
}

impl std::error::Error for PatternError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&*self.0)
    }
}

impl Query {
    /// `text`, as it is, with `case`; `None` when it is empty, which would
    /// match everywhere.
    pub fn literal(text: &str, case: Case) -> Option<Self> {
        if text.is_empty() {
            return None;
        }
        let mut needle = text.as_bytes().to_vec();
        if case == Case::Insensitive {
            needle.make_ascii_lowercase();
        }
        let matcher = Matcher::Literal(Finder::new(&needle).into_owned());
        Some(Self {
            matcher,
            case,
            whole_word: false,
        })
    }

    /// `text`, as it is, with `case`, as [`Query::literal`] gives it; but,
    /// of a case that does not count, text that is not ASCII is a regex of
    /// the text escaped, whose engine folds Unicode's simple case, where a
    /// literal folds ASCII's alone. `None` when it is empty.
    pub fn text(text: &str, case: Case) -> Option<Self> {
        if case == Case::Insensitive && !text.is_ascii() {
            return Self::regex(&escaped(text), case).ok();
        }
        Self::literal(text, case)
    }

    /// The regex `pattern`, with `case`. Its lazy DFAs, forward and
    /// backward, are those of `regex-automata`, which falls back to slower
    /// engines where they give up, as on Unicode's `\b` in text that is
    /// not ASCII.
    pub fn regex(pattern: &str, case: Case) -> Result<Self, PatternError> {
        let syntax = syntax::Config::new().case_insensitive(case == Case::Insensitive);
        let regex = Regex::builder()
            .syntax(syntax)
            .build(pattern)
            .map_err(|error| PatternError(Box::new(error)))?;
        Ok(Self {
            matcher: Matcher::Regex(Box::new(regex)),
            case,
            whole_word: false,
        })
    }

    /// The query, matching whole words only, if `on`: the cells right before
    /// and right after a match then start with neither a letter, a digit nor
    /// `_`.
    #[must_use]
    pub fn whole_word(mut self, on: bool) -> Self {
        self.whole_word = on;
        self
    }

    /// Whether the text is searched in lower case, as an insensitive
    /// literal is.
    pub(crate) fn lowers_ascii(&self) -> bool {
        matches!(self.matcher, Matcher::Literal(_)) && self.case == Case::Insensitive
    }

    /// The first of the matches of `text` that starts at `min` or after
    /// it, unless `cancel` is set first.
    pub(crate) fn first_from(
        &self,
        text: &Text<'_>,
        min: usize,
        cancel: &AtomicBool,
    ) -> Result<Option<Range<usize>>, Cancelled> {
        let mut at = 0;
        while let Some(found) = self.next_match(text, at, cancel)? {
            if found.start >= min {
                return Ok(Some(found));
            }
            at = found.end;
        }
        Ok(None)
    }

    /// The last of the matches of `text` that starts before `before`,
    /// unless `cancel` is set first.
    pub(crate) fn last_before(
        &self,
        text: &Text<'_>,
        before: usize,
        cancel: &AtomicBool,
    ) -> Result<Option<Range<usize>>, Cancelled> {
        let mut last = None;
        let mut at = 0;
        while let Some(found) = self.next_match(text, at, cancel)? {
            if found.start >= before {
                break;
            }
            at = found.end;
            last = Some(found);
        }
        Ok(last)
    }

    /// Goes through the matches of `text`, from its start, with `found`,
    /// unless `cancel` is set first.
    pub(crate) fn each(
        &self,
        text: &Text<'_>,
        cancel: &AtomicBool,
        mut found: impl FnMut(Range<usize>),
    ) -> Result<(), Cancelled> {
        let mut at = 0;
        while let Some(range) = self.next_match(text, at, cancel)? {
            at = range.end;
            found(range);
        }
        Ok(())
    }

    /// The match that comes next from `at`: the leftmost-first one that
    /// counts, a match that does not count letting the search go on a
    /// character after its start. From the start of the text, then from
    /// the end of each match, this gives the text's matches, none of them
    /// overlapping another.
    fn next_match(
        &self,
        text: &Text<'_>,
        mut at: usize,
        cancel: &AtomicBool,
    ) -> Result<Option<Range<usize>>, Cancelled> {
        while at <= text.bytes.len() {
            Cancelled::check(cancel)?;
            let Some(found) = self.any_from(text.bytes, at) else {
                break;
            };
            if self.accepts(text, &found) {
                return Ok(Some(found));
            }
            at = next_char(text.bytes, found.start);
        }
        Ok(None)
    }

    /// Whether `found` counts: not empty, from the start of a cell to the
    /// end of one, and a whole word if the query asks for whole words.
    fn accepts(&self, text: &Text<'_>, found: &Range<usize>) -> bool {
        let Some(cell) = text.cell_at(found.start) else {
            return false;
        };
        if found.is_empty() || !(found.end == text.bytes.len() || text.cell_at(found.end).is_some())
        {
            return false;
        }
        if !self.whole_word {
            return true;
        }
        let word = |c: char| c.is_alphanumeric() || c == '_';
        let before = cell
            .checked_sub(1)
            .and_then(|before| text.first_char(text.starts[before] as usize));
        let after = text.first_char(found.end);
        !before.is_some_and(word) && !after.is_some_and(word)
    }

    /// The first match that starts at `from` or after it, of any kind.
    fn any_from(&self, text: &[u8], from: usize) -> Option<Range<usize>> {
        match &self.matcher {
            Matcher::Literal(finder) => {
                let at = from + finder.find(&text[from..])?;
                Some(at..at + finder.needle().len())
            }
            Matcher::Regex(regex) => {
                let found = regex.find(Input::new(text).range(from..))?;
                Some(found.range())
            }
        }
    }
}

/// The text of a line, as a query searches it.
pub(crate) struct Text<'a> {
    /// The text, in UTF-8.
    pub(crate) bytes: &'a [u8],
    /// Where the text of each of the line's cells starts, in order.
    pub(crate) starts: &'a [u32],
}

impl Text<'_> {
    /// The index of the cell whose text starts at `at`, if one does.
    fn cell_at(&self, at: usize) -> Option<usize> {
        self.starts.binary_search(&u32::try_from(at).ok()?).ok()
    }

    /// The character that starts at `at`, between two characters, unless
    /// the text ends there.
    fn first_char(&self, at: usize) -> Option<char> {
        let end = next_char(self.bytes, at).min(self.bytes.len());
        std::str::from_utf8(self.bytes.get(at..end)?)
            .ok()?
            .chars()
            .next()
    }
}

/// Where the character after the one at `at` starts, or past the end.
fn next_char(text: &[u8], at: usize) -> usize {
    let len = match text.get(at) {
        None => 1,
        Some(&byte) if byte < 0x80 => 1,
        Some(&byte) if byte < 0xE0 => 2,
        Some(&byte) if byte < 0xF0 => 3,
        Some(_) => 4,
    };
    at + len
}
