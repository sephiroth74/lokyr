//! What Sleip tells: lines of text for a person, or, of `--json`, events of
//! NDJSON on the standard output, a JSON object a line, each of its
//! `event`, as the app reads them.

use std::io::Write as _;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Map, Value, json};

/// Whether the standard output closed, as a reader of a pipe that stopped
/// reading, as `head`, closes it: what follows goes nowhere, and the
/// command goes on to its end.
static CLOSED: AtomicBool = AtomicBool::new(false);

/// Writes `line` to the standard output, nothing once it closed.
pub fn print(line: &str) {
    if CLOSED.load(Ordering::Relaxed) {
        return;
    }
    let mut stdout = std::io::stdout().lock();
    if writeln!(stdout, "{line}")
        .and_then(|()| stdout.flush())
        .is_err()
    {
        CLOSED.store(true, Ordering::Relaxed);
    }
}

/// Where a command tells what it does.
#[derive(Clone, Copy, Debug)]
pub struct Out {
    json: bool,
}

impl Out {
    /// Of events of NDJSON, `json`, or of text.
    pub fn new(json: bool) -> Self {
        Self { json }
    }

    /// Whether it tells events of NDJSON.
    pub fn json(self) -> bool {
        self.json
    }

    /// Tells the event `event`, of `fields`, or `text` to a person.
    pub fn tell(self, event: &str, fields: Value, text: &str) {
        if self.json {
            let mut object = Map::new();
            object.insert("event".to_owned(), Value::from(event));
            if let Value::Object(fields) = fields {
                object.extend(fields);
            }
            print(&Value::Object(object).to_string());
        } else if !text.is_empty() {
            print(&printable(text));
        }
    }

    /// Tells that the command waits for `what`, on the standard error of a
    /// person.
    pub fn waiting(self, what: &str) {
        if self.json {
            self.tell("waiting", json!({ "for": what }), "");
        } else {
            eprintln!("sleip: waiting for {}", printable(what));
        }
    }

    /// Tells the error `message`, on the standard error of a person.
    pub fn error(self, message: &str) {
        if self.json {
            self.tell("error", json!({ "message": message }), "");
        } else {
            eprintln!("sleip: {}", printable(message));
        }
    }
}

/// `text` as a terminal shows it as text: each control character but a
/// line's end and a tab, C0 and C1, an escape among them, and each hidden
/// one, a change of direction or a character of no width, of a plugin's
/// or a registry's text, a replacement character, so that no text that
/// Sleip tells moves the cursor, colors the terminal, talks to it or hides
/// what follows.
pub fn printable(text: &str) -> std::borrow::Cow<'_, str> {
    let shown = |character: char| {
        !(character.is_control() && character != '\n' && character != '\t')
            && !lokyr_pkg_model::is_hidden(character)
    };
    if text.chars().all(shown) {
        return std::borrow::Cow::Borrowed(text);
    }
    std::borrow::Cow::Owned(
        text.chars()
            .map(|character| {
                if shown(character) {
                    character
                } else {
                    '\u{FFFD}'
                }
            })
            .collect(),
    )
}

/// `text`, a field of a line, as a name, a version, a description or a
/// capability of the index, as [`printable`] shows it, and its line's ends
/// and tabs a replacement character too, so that it draws no line of its
/// own beside those that Sleip tells.
pub fn one_line(text: &str) -> String {
    printable(text)
        .chars()
        .map(|character| {
            if matches!(character, '\n' | '\t') {
                '\u{FFFD}'
            } else {
                character
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_shows_as_text_at_a_terminal() {
        assert_eq!(printable("plain\ttext\nnext"), "plain\ttext\nnext");
        assert_eq!(
            printable("a\u{1b}[2J\u{1b}]52;c;eA==\u{7}b\rc\u{9b}d"),
            "a\u{FFFD}[2J\u{FFFD}]52;c;eA==\u{FFFD}b\u{FFFD}c\u{FFFD}d"
        );
        assert_eq!(printable("x\u{202E}y\u{2066}z"), "x\u{FFFD}y\u{FFFD}z");
        // Characters of no width and tags hidden; the marks and joiners of
        // scripts kept.
        assert_eq!(printable("a\u{200B}b\u{E0041}c"), "a\u{FFFD}b\u{FFFD}c");
        assert_eq!(printable("\u{200F}שלום\u{200D}"), "\u{200F}שלום\u{200D}");
        assert_eq!(
            one_line("a\nCapabilities: none\tb"),
            "a\u{FFFD}Capabilities: none\u{FFFD}b"
        );
    }
}
