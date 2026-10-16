//! What a search looks for: literal text or a regex, with the options that
//! apply to both.

use std::fmt;
use std::ops::Range;
use std::sync::atomic::AtomicBool;

use memchr::memmem::{Finder, FinderRev};
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
    Literal {
        forward: Finder<'static>,
        backward: FinderRev<'static>,
    },
    Regex(Box<Regex>),
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
        let matcher = Matcher::Literal {
            forward: Finder::new(&needle).into_owned(),
            backward: FinderRev::new(&needle).into_owned(),
        };
        Some(Self {
            matcher,
            case,
            whole_word: false,
        })
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

    /// The query, matching whole words only, if `on`: a match then has
    /// neither a letter, a digit nor `_` right before it or right after it.
    #[must_use]
    pub fn whole_word(mut self, on: bool) -> Self {
        self.whole_word = on;
        self
    }

    /// Whether the text is searched in lower case, as an insensitive
    /// literal is.
    pub(crate) fn lowers_ascii(&self) -> bool {
        matches!(self.matcher, Matcher::Literal { .. }) && self.case == Case::Insensitive
    }

    /// The first match in `text` that starts at `from` or after it, unless
    /// `cancel` is set first.
    pub(crate) fn first_from(
        &self,
        text: &[u8],
        mut from: usize,
        cancel: &AtomicBool,
    ) -> Result<Option<Range<usize>>, Cancelled> {
        while from <= text.len() {
            let Some(found) = self.any_from(text, from) else {
                break;
            };
            if self.accepts(text, &found) {
                return Ok(Some(found));
            }
            Cancelled::check(cancel)?;
            from = next_char(text, found.start);
        }
        Ok(None)
    }

    /// The last match in `text` that starts before `before`, unless
    /// `cancel` is set first.
    pub(crate) fn last_before(
        &self,
        text: &[u8],
        mut before: usize,
        cancel: &AtomicBool,
    ) -> Result<Option<Range<usize>>, Cancelled> {
        while let Some(found) = self.any_before(text, before, cancel)? {
            if self.accepts(text, &found) {
                return Ok(Some(found));
            }
            before = found.start;
        }
        Ok(None)
    }

    /// Whether `found` counts: not empty, and a whole word if the query
    /// asks for whole words.
    fn accepts(&self, text: &[u8], found: &Range<usize>) -> bool {
        if found.is_empty() {
            return false;
        }
        if !self.whole_word {
            return true;
        }
        let word = |c: char| c.is_alphanumeric() || c == '_';
        let before = str_of(&text[..found.start]).chars().next_back();
        let after = str_of(&text[found.end..]).chars().next();
        !before.is_some_and(word) && !after.is_some_and(word)
    }

    /// The first match that starts at `from` or after it, of any kind.
    fn any_from(&self, text: &[u8], from: usize) -> Option<Range<usize>> {
        match &self.matcher {
            Matcher::Literal { forward, .. } => {
                let at = from + forward.find(&text[from..])?;
                Some(at..at + forward.needle().len())
            }
            Matcher::Regex(regex) => {
                let found = regex.find(Input::new(text).range(from..))?;
                Some(found.range())
            }
        }
    }

    /// The last match that starts before `before`, of any kind, overlaps
    /// with later ones included, unless `cancel` is set first.
    fn any_before(
        &self,
        text: &[u8],
        before: usize,
        cancel: &AtomicBool,
    ) -> Result<Option<Range<usize>>, Cancelled> {
        match &self.matcher {
            Matcher::Literal { backward, .. } => {
                // A match that starts before `before` ends before it and the
                // literal's length, but for its last byte.
                let len = backward.needle().len();
                let end = (before + len - 1).min(text.len());
                Ok(backward.rfind(&text[..end]).map(|at| at..at + len))
            }
            Matcher::Regex(_) => {
                // Each match from the start of the line, one per start: a
                // long line with long matches that overlap takes long.
                let mut last = None;
                let mut from = 0;
                while from < before {
                    let Some(found) = self.any_from(text, from) else {
                        break;
                    };
                    if found.start >= before {
                        break;
                    }
                    Cancelled::check(cancel)?;
                    from = next_char(text, found.start);
                    last = Some(found);
                }
                Ok(last)
            }
        }
    }
}

/// `text` as a `str`: the text of a line is UTF-8, and a match starts and
/// ends between two characters.
fn str_of(text: &[u8]) -> &str {
    std::str::from_utf8(text).expect("a line's text is UTF-8, cut between characters")
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
