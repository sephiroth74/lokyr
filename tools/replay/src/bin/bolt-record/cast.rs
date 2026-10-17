//! Recordings written as asciicast v2:
//! <https://docs.asciinema.org/manual/asciicast/v2/>.

use std::fmt::Write as _;
use std::time::Instant;

use serde_json::json;

/// A recording under way: its header and its events, one JSON line each.
#[derive(Debug)]
pub struct Cast {
    started: Instant,
    lines: Vec<String>,
    /// The end of the last output, when it stops inside a character, which
    /// waits for the rest: an event's data is text.
    partial: Vec<u8>,
}

impl Cast {
    pub fn new(cols: u16, rows: u16, term: &str) -> Self {
        let header = json!({
            "version": 2,
            "width": cols,
            "height": rows,
            "env": { "TERM": term },
        });
        Self {
            started: Instant::now(),
            lines: vec![header.to_string()],
            partial: Vec::new(),
        }
    }

    /// The program wrote `bytes`.
    pub fn output(&mut self, bytes: &[u8]) {
        self.partial.extend_from_slice(bytes);
        let complete = match std::str::from_utf8(&self.partial) {
            Ok(_) => self.partial.len(),
            // An error that the end of the bytes cuts short is a character
            // that goes on in the next output.
            Err(error) if error.error_len().is_none() => error.valid_up_to(),
            Err(_) => self.partial.len(),
        };
        if complete == 0 {
            return;
        }
        let rest = self.partial.split_off(complete);
        let text = String::from_utf8_lossy(&self.partial).into_owned();
        self.partial = rest;
        self.event("o", &text);
    }

    /// The bytes typed into the program: keys, or the terminal's replies.
    pub fn input(&mut self, bytes: &[u8]) {
        self.event("i", &String::from_utf8_lossy(bytes));
    }

    /// A marker, for tests to look at the screen there.
    pub fn mark(&mut self, label: &str) {
        self.event("m", label);
    }

    /// The recording, header first, a line each.
    pub fn finish(mut self) -> String {
        if !self.partial.is_empty() {
            let text = String::from_utf8_lossy(&std::mem::take(&mut self.partial)).into_owned();
            self.event("o", &text);
        }
        let mut text = String::new();
        for line in &self.lines {
            let _ = writeln!(text, "{line}");
        }
        text
    }

    /// What the program wrote, as the recording keeps it.
    #[cfg(test)]
    pub fn outputs(&self) -> impl Iterator<Item = String> + '_ {
        self.lines.iter().skip(1).filter_map(
            |line| match serde_json::from_str::<serde_json::Value>(line)
                .ok()?
                .as_array()?
                .as_slice()
            {
                [_, kind, data] if kind == "o" => data.as_str().map(str::to_owned),
                _ => None,
            },
        )
    }

    fn event(&mut self, kind: &str, data: &str) {
        // In milliseconds, so that the files stay short.
        let seconds = (self.started.elapsed().as_secs_f64() * 1000.0).round() / 1000.0;
        self.lines.push(json!([seconds, kind, data]).to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_character_that_two_outputs_split_comes_whole() {
        let mut cast = Cast::new(10, 2, "xterm-256color");
        cast.output("a\u{4e2d}".as_bytes().split_at(2).0);
        cast.output(&"a\u{4e2d}".as_bytes()[2..]);
        cast.input(b"\x1b[A");
        cast.mark("start");
        cast.output(b"\xff!");
        assert_eq!(
            cast.outputs().collect::<Vec<_>>(),
            ["a", "\u{4e2d}", "\u{fffd}!"]
        );
        let text = cast.finish();
        let lines: Vec<serde_json::Value> = text
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(lines[0]["width"], 10);
        assert_eq!(lines[0]["env"]["TERM"], "xterm-256color");
        let kinds: Vec<&str> = lines[1..]
            .iter()
            .map(|event| event[1].as_str().unwrap())
            .collect();
        assert_eq!(kinds, ["o", "o", "i", "m", "o"]);
        assert_eq!(lines[3][2], "\u{1b}[A");
    }
}
