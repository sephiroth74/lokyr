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
            print(text);
        }
    }

    /// Tells that the command waits for `what`, on the standard error of a
    /// person.
    pub fn waiting(self, what: &str) {
        if self.json {
            self.tell("waiting", json!({ "for": what }), "");
        } else {
            eprintln!("sleip: waiting for {what}");
        }
    }

    /// Tells the error `message`, on the standard error of a person.
    pub fn error(self, message: &str) {
        if self.json {
            self.tell("error", json!({ "message": message }), "");
        } else {
            eprintln!("sleip: {message}");
        }
    }
}
