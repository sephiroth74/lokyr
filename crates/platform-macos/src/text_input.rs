//! What a window's view keeps for the input method: the text typed last,
//! which it can replace, as press and hold replaces a letter with its
//! accented one; the text that it composes; what it did with the key that
//! the view handed it; and where the typed text goes. With the turns of
//! AppKit's UTF-16 ranges into the byte ranges of Rust's strings.

use std::ops::Range;

use bolt_platform_core::{Ime, KeyCode, KeyEvent, KeyState, Modifiers, Point, Size, WindowEvent};
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

/// What the input method did with a key, in its order.
#[derive(Debug, Default)]
pub(crate) struct Handled {
    pub(crate) steps: Vec<Step>,
}

/// One thing that the input method did with a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// It typed `text`, in place of the `deleted` characters typed before
    /// it.
    Inserted {
        /// The text.
        text: String,
        /// The characters typed before it that it replaces.
        deleted: usize,
    },
    /// It set the marked text, or ended it without typing, when empty.
    Marked {
        /// The text.
        text: String,
        /// Its part that the input method selects, in bytes.
        selection: Range<usize>,
    },
    /// It turned the key into a command, one that types nothing, while it
    /// composed or not.
    Command {
        /// Whether it still composed.
        composing: bool,
    },
}

/// The events of `key`, which the input method handled with `steps`, in
/// their order. A key that it typed as the key types it, as plain typing
/// does, goes on as the key, so that the encoder sees every key that a
/// program may report; so does a key that it turned into a command once
/// it no longer composes, after the text that it typed with it. A key that
/// it did nothing with goes on as the key, unless it composed before or
/// after it, or it is a repeat, which press and hold keeps.
pub(crate) fn events(
    key: KeyEvent,
    composing: bool,
    steps: Vec<Step>,
    still: bool,
) -> Vec<WindowEvent> {
    match steps.as_slice() {
        [Step::Inserted { text, deleted: 0 }] if !composing && *text == key.text => {
            return vec![WindowEvent::Key(key)];
        }
        [] => {
            let kept = composing || still || key.state == KeyState::Repeated;
            return if kept {
                Vec::new()
            } else {
                vec![WindowEvent::Key(key)]
            };
        }
        _ => {}
    }
    let mut events = Vec::new();
    for step in steps {
        match step {
            Step::Inserted { text, deleted } => {
                events.extend((0..deleted).map(|_| WindowEvent::Key(backspace())));
                events.push(WindowEvent::Ime(Ime::Commit(text)));
            }
            Step::Marked { text, selection } => {
                events.push(WindowEvent::Ime(Ime::Preedit { text, selection }));
            }
            Step::Command { composing: false } => events.push(WindowEvent::Key(key.clone())),
            Step::Command { composing: true } => {}
        }
    }
    events
}

/// A press of Backspace, which deletes a character typed before.
pub(crate) fn backspace() -> KeyEvent {
    KeyEvent {
        code: KeyCode::Backspace,
        state: KeyState::Pressed,
        mods: Modifiers::NONE,
        text: "\u{7f}".to_owned(),
        unmodified: "\u{7f}".to_owned(),
        plain: "\u{7f}".to_owned(),
    }
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

    fn key(code: KeyCode, text: &str, state: KeyState) -> KeyEvent {
        KeyEvent {
            code,
            state,
            mods: Modifiers::NONE,
            text: text.to_owned(),
            unmodified: text.to_owned(),
            plain: text.to_owned(),
        }
    }

    fn inserted(text: &str, deleted: usize) -> Step {
        Step::Inserted {
            text: text.to_owned(),
            deleted,
        }
    }

    fn marked(text: &str) -> Step {
        Step::Marked {
            text: text.to_owned(),
            selection: text.len()..text.len(),
        }
    }

    fn commit(text: &str) -> WindowEvent {
        WindowEvent::Ime(Ime::Commit(text.to_owned()))
    }

    fn preedit(text: &str) -> WindowEvent {
        WindowEvent::Ime(Ime::Preedit {
            text: text.to_owned(),
            selection: text.len()..text.len(),
        })
    }

    #[test]
    fn plain_typing_and_commands_go_as_keys() {
        let a = key(KeyCode::KeyA, "a", KeyState::Pressed);
        assert_eq!(
            events(a.clone(), false, vec![inserted("a", 0)], false),
            [WindowEvent::Key(a.clone())]
        );
        let left = key(KeyCode::ArrowLeft, "\u{f702}", KeyState::Pressed);
        let command = Step::Command { composing: false };
        assert_eq!(
            events(left.clone(), false, vec![command], false),
            [WindowEvent::Key(left.clone())]
        );
        // A key that the input method does nothing with, as a function key.
        assert_eq!(
            events(left.clone(), false, vec![], false),
            [WindowEvent::Key(left)]
        );
    }

    #[test]
    fn a_commit_and_a_new_composition_come_in_their_order() {
        // Korean, at a syllable's end: `한` typed, `ㄱ` composed.
        let r = key(KeyCode::KeyR, "ㄱ", KeyState::Pressed);
        assert_eq!(
            events(r, true, vec![inserted("한", 0), marked("ㄱ")], true),
            [commit("한"), preedit("ㄱ")]
        );
    }

    #[test]
    fn a_commit_and_a_command_give_both() {
        // Korean's Return after a syllable, and a dead key's.
        let enter = key(KeyCode::Enter, "\r", KeyState::Pressed);
        let steps = vec![inserted("한", 0), Step::Command { composing: false }];
        assert_eq!(
            events(enter.clone(), true, steps, false),
            [commit("한"), WindowEvent::Key(enter.clone())]
        );
        // A command while the input method still composes is its own.
        let steps = vec![Step::Command { composing: true }];
        assert_eq!(events(enter, true, steps, true), []);
    }

    #[test]
    fn what_the_input_method_keeps_goes_nowhere() {
        let n = key(KeyCode::KeyN, "n", KeyState::Pressed);
        assert_eq!(
            events(n.clone(), false, vec![marked("n")], true),
            [preedit("n")]
        );
        // A key that it takes while it composes, and a repeat that it keeps.
        assert_eq!(events(n, true, vec![], true), []);
        let held = key(KeyCode::KeyE, "e", KeyState::Repeated);
        assert_eq!(events(held, false, vec![], false), []);
    }

    #[test]
    fn a_replacement_deletes_first() {
        let two = key(KeyCode::Digit2, "2", KeyState::Pressed);
        assert_eq!(
            events(two, false, vec![inserted("é", 1)], false),
            [WindowEvent::Key(backspace()), commit("é")]
        );
    }

    #[test]
    fn either_string_gives_its_text() {
        let plain = NSString::from_str("´");
        assert_eq!(text_of(&plain), "´");
        let attributed = NSAttributedString::from_nsstring(&NSString::from_str("日本"));
        assert_eq!(text_of(&attributed), "日本");
    }
}
