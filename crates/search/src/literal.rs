//! Literal text to find, with `memchr`'s finders made once for it.

use memchr::memmem::{Finder, FinderRev};

/// Whether case counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Case {
    /// `a` finds `a` only.
    Sensitive,
    /// `a` finds `a` and `A`: ASCII letters only, which the text and the
    /// literal compare in lower case.
    Insensitive,
}

/// Text to find, as it is.
#[derive(Clone, Debug)]
pub struct Literal {
    forward: Finder<'static>,
    backward: FinderRev<'static>,
    case: Case,
}

impl Literal {
    /// `text`, to find with `case`; `None` when it is empty, which would
    /// match everywhere.
    pub fn new(text: &str, case: Case) -> Option<Self> {
        if text.is_empty() {
            return None;
        }
        let mut needle = text.as_bytes().to_vec();
        if case == Case::Insensitive {
            needle.make_ascii_lowercase();
        }
        Some(Self {
            forward: Finder::new(&needle).into_owned(),
            backward: FinderRev::new(&needle).into_owned(),
            case,
        })
    }

    /// The length of the literal, in bytes of UTF-8.
    pub(crate) fn len(&self) -> usize {
        self.forward.needle().len()
    }

    pub(crate) fn case(&self) -> Case {
        self.case
    }

    /// Where the literal first starts in `text`.
    pub(crate) fn find_in(&self, text: &[u8]) -> Option<usize> {
        self.forward.find(text)
    }

    /// Where the literal last starts in `text`.
    pub(crate) fn rfind_in(&self, text: &[u8]) -> Option<usize> {
        self.backward.rfind(text)
    }
}
