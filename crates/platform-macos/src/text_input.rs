//! What a window's view keeps for the input method: the text typed last,
//! which it can replace, as press and hold replaces a letter with its
//! accented one; the text that it composes; what it did with the key that
//! the view handed it; and where the typed text goes. With the turns of
//! AppKit's UTF-16 ranges into the byte ranges of Rust's strings.

use std::ops::Range;

use bolt_platform_core::{Point, Size};
use objc2::runtime::AnyObject;
use objc2_foundation::{NSAttributedString, NSNotFound, NSRange, NSString};

/// The most characters of the text typed last that the view keeps.
const TYPED: usize = 32;

/// The view's side of the input method.
#[derive(Debug, Default)]
pub(crate) struct TextInput {
    /// The text typed since the last key that types nothing, as the input
    /// method sees the document before its cursor: the program has it, and
    /// replacing some of it means deleting it first.
    pub(crate) typed: String,
    /// The text that the input method composes, and the part of it that it
    /// selects, in UTF-16 units, as AppKit counts them.
    pub(crate) marked: Option<(String, NSRange)>,
    /// What the input method did with the key that the view handed it,
    /// while it handles one.
    pub(crate) handling: Option<Handled>,
    /// Where the typed text goes, from the top left corner of the view, in
    /// points.
    pub(crate) area: (Point, Size),
}

impl TextInput {
    /// The document as the input method reads it: the text typed last, then
    /// the text that it composes.
    pub(crate) fn document(&self) -> String {
        let mut document = self.typed.clone();
        if let Some((marked, _)) = &self.marked {
            document.push_str(marked);
        }
        document
    }

    /// Types `text` in place of `replacing`, a range of the document, as an
    /// input method asks, or at its end, in place of the marked text;
    /// returns how many characters of the text typed last go, which the
    /// program has to delete.
    pub(crate) fn insert(&mut self, text: &str, replacing: NSRange) -> usize {
        let composing = self.marked.take().is_some();
        let mut deleted = 0;
        if !composing && replacing.location != NSNotFound as usize {
            let from = bytes(&self.typed, replacing).start;
            deleted = self.typed[from..].chars().count();
            self.typed.truncate(from);
        }
        self.typed.push_str(text);
        let extra = self.typed.chars().count().saturating_sub(TYPED);
        if extra > 0 {
            let at = self
                .typed
                .char_indices()
                .nth(extra)
                .map_or(self.typed.len(), |(at, _)| at);
            self.typed.drain(..at);
        }
        deleted
    }
}

/// What the input method did with a key.
#[derive(Debug, Default)]
pub(crate) struct Handled {
    /// The text that it committed.
    pub(crate) inserted: Option<String>,
    /// How many characters typed before it deletes first.
    pub(crate) deleted: usize,
    /// Whether it composed: it set, changed or ended the marked text.
    pub(crate) composed: bool,
    /// Whether the key turned out to be a command, one that types nothing.
    pub(crate) commanded: bool,
}

/// The text of `string`, an `NSString` or an `NSAttributedString`, as
/// AppKit hands either to an input client.
pub(crate) fn text_of(string: &AnyObject) -> String {
    if let Some(attributed) = string.downcast_ref::<NSAttributedString>() {
        attributed.string().to_string()
    } else if let Some(string) = string.downcast_ref::<NSString>() {
        string.to_string()
    } else {
        String::new()
    }
}

/// The range of no characters.
pub(crate) fn not_found() -> NSRange {
    NSRange::new(NSNotFound as usize, 0)
}

/// The UTF-16 units of `text`, as AppKit counts its length.
pub(crate) fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// The bytes of `text` that `range`, of UTF-16 units, covers, kept within
/// the text; a range of no units, where AppKit puts the cursor, gives the
/// bytes before the cursor.
pub(crate) fn bytes(text: &str, range: NSRange) -> Range<usize> {
    let byte = |units: usize| {
        let mut counted = 0;
        for (at, character) in text.char_indices() {
            if counted >= units {
                return at;
            }
            counted += character.len_utf16();
        }
        text.len()
    };
    if range.location == NSNotFound as usize {
        return text.len()..text.len();
    }
    let start = byte(range.location);
    let end = byte(range.location.saturating_add(range.length)).max(start);
    start..end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_ranges_become_byte_ranges() {
        // にほん: three units, of three bytes each.
        assert_eq!(bytes("にほん", NSRange::new(1, 1)), 3..6);
        assert_eq!(bytes("にほん", NSRange::new(3, 0)), 9..9);
        // 🦀 takes two units and four bytes.
        assert_eq!(bytes("a🦀b", NSRange::new(1, 2)), 1..5);
        assert_eq!(bytes("a🦀b", NSRange::new(3, 1)), 5..6);
        // Past the text, its end.
        assert_eq!(bytes("ab", NSRange::new(5, 2)), 2..2);
        assert_eq!(bytes("ab", not_found()), 2..2);
        assert_eq!(utf16_len("a🦀"), 3);
    }

    #[test]
    fn a_replacement_deletes_what_it_covers_first() {
        let mut input = TextInput::default();
        assert_eq!(input.insert("cafe", not_found()), 0);
        // Press and hold puts `é` in place of the `e` typed last.
        assert_eq!(input.insert("é", NSRange::new(3, 1)), 1);
        assert_eq!(input.typed, "café");
        // A commit of composed text replaces the marked text alone.
        input.marked = Some(("にほ".to_owned(), NSRange::new(2, 0)));
        assert_eq!(input.document(), "caféにほ");
        assert_eq!(input.insert("日本", NSRange::new(4, 2)), 0);
        assert_eq!(input.typed, "café日本");
        assert_eq!(input.marked, None);
        // The text kept is the last 32 characters.
        input.insert(&"x".repeat(40), not_found());
        assert_eq!(input.typed, "x".repeat(32));
    }

    #[test]
    fn either_string_gives_its_text() {
        let plain = NSString::from_str("´");
        assert_eq!(text_of(&plain), "´");
        let attributed = NSAttributedString::from_nsstring(&NSString::from_str("日本"));
        assert_eq!(text_of(&attributed), "日本");
    }
}
