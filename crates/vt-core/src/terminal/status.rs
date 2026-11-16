//! The Program Status Protocol, OSC 7501: a program tells whether it is
//! idle, working, blocked on the user, done or failed, of a progress, a
//! title and a message, each record of its `id` in a tree of tasks, which
//! the terminal keeps and tells the embedder of, as revision 0.4 of the
//! proposal has it: <https://control-codes.page/proposals/osc-7501/>.

use super::clipboard;
use super::{Event, Terminal};
use crate::parser::StringEnd;

/// The most bytes of a whole report, its `ESC ]` and `ST` among them.
pub const MOST_REPORT: usize = 4096;

/// The most records that the terminal keeps: the least recently updated
/// goes as another comes past it.
pub const MOST_STATUSES: usize = 256;

/// The most bytes of a key, of an `app`, of an `id` and of each of its
/// segments, and the most segments of an `id`.
const MOST_KEY: usize = 16;
const MOST_APP: usize = 32;
const MOST_ID: usize = 128;
const MOST_SEGMENT: usize = 32;
const MOST_SEGMENTS: usize = 8;

/// The most bytes of a `title` and of a `msg`, encoded and decoded.
const MOST_TITLE: (usize, usize) = (256, 192);
const MOST_MSG: (usize, usize) = (2732, 2048);

/// The answer to `OSC 7501 ; ? ST`: the states and the kinds that the
/// terminal takes.
const ANSWER: &str = "7501;?:states=idle,working,done,blocked,error:kinds=permission,question,auth";

/// Where a program is in its work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusState {
    /// It waits for nothing, as a shell at its prompt or an agent between
    /// tasks.
    Idle,
    /// It works.
    Working,
    /// It waits for the user, of [`Status::kind`].
    Blocked,
    /// It finished what it was asked.
    Done,
    /// It failed.
    Error,
}

/// What a blocked program waits for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusKind {
    /// A permission, as to run a command.
    Permission,
    /// An answer to a question.
    Question,
    /// A login.
    Auth,
}

/// A program's status, as its last report of an `id` told it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    /// Its task's path in the tree, its segments joined by `/`; empty for
    /// the root.
    pub id: String,
    /// Where it is.
    pub state: StatusState,
    /// What it waits for, while blocked.
    pub kind: Option<StatusKind>,
    /// How far along it is, from 0 to 100, while it works or waits; none
    /// for an indeterminate progress.
    pub progress: Option<u8>,
    /// The program's stable name, as `claude`, which a task of none takes
    /// of the nearest one above it, [`Terminal::status_app`].
    pub app: Option<String>,
    /// A short label of the task.
    pub title: Option<String>,
    /// A line of what it does, waits for or ended with.
    pub msg: Option<String>,
}

/// What a report asks: a record to keep, or a record and the ones under it
/// to take away.
enum Report {
    Keep(Status),
    Clear(String),
}

impl Terminal {
    /// The programs' statuses that OSC 7501 told and that the terminal
    /// keeps, from the least recently updated.
    pub fn statuses(&self) -> &[Status] {
        &self.statuses
    }

    /// The `app` of `status`, or of the nearest task above it that has one.
    pub fn status_app<'a>(&'a self, status: &'a Status) -> Option<&'a str> {
        if let Some(app) = &status.app {
            return Some(app);
        }
        let mut id = status.id.as_str();
        while !id.is_empty() {
            id = id.rsplit_once('/').map_or("", |(above, _)| above);
            if let Some(app) = self
                .statuses
                .iter()
                .find(|record| record.id == id)
                .and_then(|record| record.app.as_deref())
            {
                return Some(app);
            }
        }
        None
    }

    /// OSC 7501 of `fields`, the string after `7501;`: the query answered,
    /// as it ended; a report kept, or the records that it clears taken
    /// away; one that does not read, or past a bound, ignored whole.
    pub(super) fn report_status(&mut self, fields: &[u8], end: StringEnd) {
        if fields.trim_ascii() == b"?" {
            self.reply_osc(ANSWER, end);
            return;
        }
        // `ESC ] 7501 ;` before the fields, and `ST` after them.
        if fields.len() + "\x1b]7501;\x1b\\".len() > MOST_REPORT {
            return;
        }
        match read(fields) {
            Some(Report::Keep(status)) => {
                self.statuses.retain(|record| record.id != status.id);
                if self.statuses.len() >= MOST_STATUSES {
                    self.statuses.remove(0);
                }
                self.statuses.push(status);
                self.events.push(Event::StatusChanged);
            }
            Some(Report::Clear(id)) => {
                let before = self.statuses.len();
                self.statuses.retain(|record| !within(&record.id, &id));
                if self.statuses.len() != before {
                    self.events.push(Event::StatusChanged);
                }
            }
            None => {}
        }
    }

    /// The records of programs that work, wait or are idle go, as a new
    /// prompt begins (OSC 133 `A`); those done or failed stay until the
    /// embedder no longer shows them.
    pub(super) fn drop_statuses_at_prompt(&mut self) {
        let before = self.statuses.len();
        self.statuses
            .retain(|record| matches!(record.state, StatusState::Done | StatusState::Error));
        if self.statuses.len() != before {
            self.events.push(Event::StatusChanged);
        }
    }

    /// Takes away the records that `keep` refuses, as the embedder no
    /// longer shows them or their program exited: whether one went, of no
    /// event, as the embedder knows what it took away.
    pub fn forget_statuses(&mut self, keep: impl Fn(&Status) -> bool) -> bool {
        let before = self.statuses.len();
        self.statuses.retain(keep);
        self.statuses.len() != before
    }
}

/// Whether the task of `id` is `top` or one under it.
fn within(id: &str, top: &str) -> bool {
    top.is_empty()
        || id == top
        || id
            .strip_prefix(top)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// What the report of `fields` asks, or none of one that does not read or
/// is past a bound.
fn read(fields: &[u8]) -> Option<Report> {
    let mut pairs: Vec<(&[u8], &[u8])> = Vec::new();
    for pair in fields.split(|&byte| byte == b':') {
        let Some(at) = pair.iter().position(|&byte| byte == b'=') else {
            continue;
        };
        let (key, value) = (pair[..at].trim_ascii(), pair[at + 1..].trim_ascii());
        if key.len() > MOST_KEY {
            return None;
        }
        // A malformed pair is skipped, the rest of the report applying.
        if key.is_empty()
            || !key.iter().all(u8::is_ascii_lowercase)
            || !value.iter().all(|&byte| is_value_byte(byte))
        {
            continue;
        }
        // A repeated key takes its last value.
        pairs.retain(|(known, _)| *known != key);
        pairs.push((key, value));
    }
    let value = |name: &[u8]| {
        pairs
            .iter()
            .find(|(key, _)| *key == name)
            .map(|&(_, value)| value)
    };
    let id = match value(b"id") {
        Some(id) => task_id(id)?,
        None => String::new(),
    };
    let state = match value(b"state")? {
        b"idle" => StatusState::Idle,
        b"working" => StatusState::Working,
        b"blocked" => StatusState::Blocked,
        b"done" => StatusState::Done,
        b"error" => StatusState::Error,
        b"clear" => return Some(Report::Clear(id)),
        _ => return None,
    };
    let kind = (state == StatusState::Blocked)
        .then(|| match value(b"kind")? {
            b"permission" => Some(StatusKind::Permission),
            b"question" => Some(StatusKind::Question),
            b"auth" => Some(StatusKind::Auth),
            _ => None,
        })
        .flatten();
    let progress = matches!(state, StatusState::Working | StatusState::Blocked)
        .then(|| {
            std::str::from_utf8(value(b"progress")?)
                .ok()?
                .parse::<u8>()
                .ok()
                .filter(|&progress| progress <= 100)
        })
        .flatten();
    let app = match value(b"app") {
        Some(app) if app.len() > MOST_APP => return None,
        Some(app)
            if app
                .iter()
                .all(|&byte| byte.is_ascii_alphanumeric() || b"_.+-".contains(&byte)) =>
        {
            Some(String::from_utf8_lossy(app).into_owned())
        }
        _ => None,
    };
    let title = match value(b"title") {
        Some(title) => Some(free_text(title, MOST_TITLE)?),
        None => None,
    };
    let msg = match value(b"msg") {
        Some(msg) => Some(free_text(msg, MOST_MSG)?),
        None => None,
    };
    Some(Report::Keep(Status {
        id,
        state,
        kind,
        progress,
        app,
        title,
        msg,
    }))
}

/// Whether `byte` may be in a value: a letter, a digit, or one of
/// `_ . , + / = -`.
fn is_value_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"_.,+/=-".contains(&byte)
}

/// The task's path of `id`, or none of one past its bounds or of an empty
/// segment, which names no task.
fn task_id(id: &[u8]) -> Option<String> {
    if id.len() > MOST_ID {
        return None;
    }
    let segments: Vec<&[u8]> = id.split(|&byte| byte == b'/').collect();
    let fine = segments.len() <= MOST_SEGMENTS
        && segments
            .iter()
            .all(|segment| !segment.is_empty() && segment.len() <= MOST_SEGMENT);
    fine.then(|| String::from_utf8_lossy(id).into_owned())
}

/// The text of the base64 `encoded`, of `(most encoded, most decoded)`
/// bytes: UTF-8 of no control character, a newline or a tab among them;
/// none of one past its bounds, of no base64 or of another text.
fn free_text(encoded: &[u8], (most_encoded, most_decoded): (usize, usize)) -> Option<String> {
    if encoded.len() > most_encoded {
        return None;
    }
    // Base64 whole, its padding at its end alone.
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
    let bytes = clipboard::decode(digits)?;
    if bytes.len() > most_decoded {
        return None;
    }
    let text = String::from_utf8(bytes).ok()?;
    (!text.chars().any(char::is_control)).then_some(text)
}

#[cfg(test)]
mod tests;
