//! iTerm2's session status, OSC 21337: a program tells a short status of
//! its session, as "Working", the color of a dot that marks it, the color
//! of its text and a line of detail, which the terminal keeps and tells
//! the embedder of: <https://iterm2.com/documentation-session-status.html>.

use super::colors::parse_color;
use super::{Event, Terminal};
use crate::page::Rgb;

/// The most bytes of the status's text and of its detail.
const MOST_STATUS: usize = 256;
const MOST_DETAIL: usize = 1024;

/// The status of a session, as OSC 21337 tells it: each part none until a
/// report gives it, and once a report clears it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionStatus {
    /// A short text of it, as "Working" or "Waiting".
    pub status: Option<String>,
    /// The color of the dot that marks it.
    pub indicator: Option<Rgb>,
    /// The color of its text.
    pub status_color: Option<Rgb>,
    /// A line of what the program does.
    pub detail: Option<String>,
}

impl SessionStatus {
    /// Whether it tells nothing.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

impl Terminal {
    /// The session's status that OSC 21337 told, empty of none.
    pub fn session_status(&self) -> &SessionStatus {
        &self.session_status
    }

    /// Clears the session's status, as its program left: whether it told
    /// something, of no event, as the embedder knows what it cleared.
    pub fn clear_session_status(&mut self) -> bool {
        !std::mem::take(&mut self.session_status).is_empty()
    }

    /// OSC 21337 of `fields`, the string after `21337;`: its `key=value`
    /// pairs, separated by `;`, each value of `\;` and `\\` for its own
    /// semicolons and backslashes, kept; a key left out keeps its part,
    /// an empty value clears it, and a key not known, or a value that does
    /// not read or is past its bound, is ignored.
    pub(super) fn set_session_status(&mut self, fields: &[u8]) {
        let mut status = self.session_status.clone();
        for (key, value) in pairs(fields) {
            let cleared = value.is_empty();
            match key.as_slice() {
                b"status" => {
                    if let Some(text) = free_text(&value, MOST_STATUS) {
                        status.status = (!cleared).then_some(text);
                    }
                }
                b"detail" => {
                    if let Some(text) = free_text(&value, MOST_DETAIL) {
                        status.detail = (!cleared).then_some(text);
                    }
                }
                b"indicator" if cleared => status.indicator = None,
                b"indicator" => {
                    if let Some(color) = parse_color(&value) {
                        status.indicator = Some(color);
                    }
                }
                b"status-color" if cleared => status.status_color = None,
                b"status-color" => {
                    if let Some(color) = parse_color(&value) {
                        status.status_color = Some(color);
                    }
                }
                _ => {}
            }
        }
        if status != self.session_status {
            self.session_status = status;
            self.events.push(Event::SessionStatusChanged);
        }
    }
}

/// The `key=value` pairs of `fields`, separated by `;`, a backslash before
/// a semicolon or a backslash taking it as it is.
fn pairs(fields: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
    let mut pairs = Vec::new();
    let mut field = Vec::new();
    let mut bytes = fields.iter().copied();
    let mut end = |field: &mut Vec<u8>| {
        if let Some(at) = field.iter().position(|&byte| byte == b'=') {
            pairs.push((field[..at].to_vec(), field[at + 1..].to_vec()));
        }
        field.clear();
    };
    while let Some(byte) = bytes.next() {
        match byte {
            b'\\' => match bytes.next() {
                Some(next @ (b';' | b'\\')) => field.push(next),
                Some(next) => field.extend([b'\\', next]),
                None => field.push(b'\\'),
            },
            b';' => end(&mut field),
            byte => field.push(byte),
        }
    }
    end(&mut field);
    pairs
}

/// The text of `value`, UTF-8 of no control character, of `most` bytes at
/// most; none otherwise.
fn free_text(value: &[u8], most: usize) -> Option<String> {
    if value.len() > most {
        return None;
    }
    let text = std::str::from_utf8(value).ok()?;
    (!text.chars().any(char::is_control)).then(|| text.to_owned())
}

#[cfg(test)]
mod tests;
