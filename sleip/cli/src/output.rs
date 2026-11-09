//! What Sleip tells: lines of text for a person, or, of `--json`, events of
//! NDJSON on the standard output, a JSON object a line, each of its
//! `event`, as the app reads them.

use serde_json::{Map, Value, json};

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
            println!("{}", Value::Object(object));
        } else if !text.is_empty() {
            println!("{text}");
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
