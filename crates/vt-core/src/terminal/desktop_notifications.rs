//! kitty's desktop notifications, OSC 99: a notification's title and body,
//! each in one part or several, of an `id` that ties its parts together,
//! shown once its last part comes, as the embedder shows those of OSC 9
//! and 777: <https://sw.kovidgoyal.net/kitty/desktop-notifications/>. Its
//! icons, its buttons, its actions but the focus, closing it and asking
//! whether it lives are not taken.

use super::clipboard;
use super::osc::text;
use super::{Event, Terminal};
use crate::parser::StringEnd;

/// The most bytes of a part's payload, as the protocol bounds it, encoded.
const MOST_PART: usize = 4096;

/// The most bytes of a notification's title and of its body, of all their
/// parts.
const MOST_TITLE: usize = 1024;
const MOST_BODY: usize = 4096;

/// The most notifications whose parts the terminal holds until their last
/// comes: the oldest goes as another comes past it.
const MOST_HELD: usize = 16;

/// The most bytes of an `id`.
const MOST_ID: usize = 64;

/// The answer to a query, `p=?`, after its `id`: the focus that a click
/// gives, the notifications shown while the window has no focus, as the
/// embedder shows them, and the title and the body.
const ANSWER: &str = "a=focus:o=unfocused:p=title,body";

/// A notification whose last part has not come.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Held {
    id: String,
    title: String,
    body: String,
}

/// What a part carries, by its `p`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Payload {
    Title,
    Body,
    /// A part of the notification that the terminal does not show, as its
    /// icon or its buttons, which may be its last.
    Other,
    /// A question of the terminal's support.
    Query,
    /// A request about notifications shown, as to close one.
    Ignored,
}

impl Terminal {
    /// OSC 99 of `fields`, the string after `99;`: its metadata, its `;`,
    /// and its payload; one without both of its semicolons ignored.
    pub(super) fn desktop_notification(&mut self, fields: &[u8], end: StringEnd) {
        let Some(at) = fields.iter().position(|&byte| byte == b';') else {
            return;
        };
        let (metadata, payload) = (&fields[..at], &fields[at + 1..]);
        let mut id = String::new();
        let mut done = true;
        let mut encoded = false;
        let mut kind = Payload::Title;
        for pair in metadata.split(|&byte| byte == b':') {
            let Some(equals) = pair.iter().position(|&byte| byte == b'=') else {
                continue;
            };
            let (key, value) = (&pair[..equals], &pair[equals + 1..]);
            match key {
                b"i" => id = sanitized(value),
                b"d" => done = value != b"0",
                b"e" => encoded = value == b"1",
                b"p" => {
                    kind = match value {
                        b"title" => Payload::Title,
                        b"body" => Payload::Body,
                        b"icon" | b"buttons" => Payload::Other,
                        b"?" => Payload::Query,
                        b"close" | b"alive" => Payload::Ignored,
                        // A payload of a kind not known is ignored whole.
                        _ => return,
                    }
                }
                // The keys not taken, or not known, are ignored.
                _ => {}
            }
        }
        match kind {
            Payload::Query => {
                self.reply_osc(&format!("99;i={id}:p=?;{ANSWER}"), end);
                return;
            }
            Payload::Ignored => return,
            Payload::Title | Payload::Body | Payload::Other => {}
        }
        if payload.len() > MOST_PART {
            return;
        }
        let part = match kind {
            Payload::Title | Payload::Body if encoded => match base64(payload) {
                Some(bytes) => text(&bytes),
                None => return,
            },
            Payload::Title | Payload::Body => text(payload),
            _ => String::new(),
        };
        let held = match self
            .held_notifications
            .iter()
            .position(|held| held.id == id)
        {
            Some(at) => at,
            None => {
                if self.held_notifications.len() >= MOST_HELD {
                    self.held_notifications.remove(0);
                }
                self.held_notifications.push(Held {
                    id,
                    ..Held::default()
                });
                self.held_notifications.len() - 1
            }
        };
        let notification = &mut self.held_notifications[held];
        match kind {
            Payload::Title => push_within(&mut notification.title, &part, MOST_TITLE),
            Payload::Body => push_within(&mut notification.body, &part, MOST_BODY),
            _ => {}
        }
        if done {
            let Held { title, body, .. } = self.held_notifications.remove(held);
            // One of no title shows its body, as the embedder shows
            // OSC 9's; one of neither shows nothing.
            if !title.is_empty() || !body.is_empty() {
                self.events.push(Event::Notification { title, body });
            }
        }
    }
}

/// The `id` of `value`, of its letters, digits and `_ - + .` alone, as
/// the protocol has the terminal sanitize it, of [`MOST_ID`] bytes at most.
fn sanitized(value: &[u8]) -> String {
    value
        .iter()
        .copied()
        .filter(|&byte| byte.is_ascii_alphanumeric() || b"_-+.".contains(&byte))
        .take(MOST_ID)
        .map(char::from)
        .collect()
}

/// `text` after `kept`, as much of it as fits within `most` bytes, cut
/// between characters.
fn push_within(kept: &mut String, text: &str, most: usize) {
    for character in text.chars() {
        if kept.len() + character.len_utf8() > most {
            break;
        }
        kept.push(character);
    }
}

/// The bytes of the base64 `encoded`, its padding at its end alone; none
/// of another text.
fn base64(encoded: &[u8]) -> Option<Vec<u8>> {
    let digits = encoded
        .strip_suffix(b"==")
        .or_else(|| encoded.strip_suffix(b"="))
        .unwrap_or(encoded);
    if !digits
        .iter()
        .all(|&byte| byte.is_ascii_alphanumeric() || byte == b'+' || byte == b'/')
        || digits.len() % 4 == 1
    {
        return None;
    }
    clipboard::decode(digits)
}

#[cfg(test)]
mod tests;
