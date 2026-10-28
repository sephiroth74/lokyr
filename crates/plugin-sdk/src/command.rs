//! Commands: what describes one, the values of its arguments, and the
//! errors of a call.
//!
//! A command has a name, `namespace.action`, as `window.new` or `tabs.goto`;
//! a title, which the menus and the palette show; a category, which groups
//! the commands for the user; the capabilities that its action needs; and
//! its parameters, a flat schema of named values. An error has a code whose
//! hundreds tell its category, and with it how serious it is.

use std::collections::BTreeMap;
use std::fmt;

/// The arguments of a call: values by their parameters' names.
pub type Args = BTreeMap<String, Value>;

/// A command, as its registration describes it.
#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    /// Its name, `namespace.action`.
    pub name: String,
    /// Its title, for the menus and the palette.
    pub title: String,
    /// The group that it belongs to, for the user.
    pub category: Category,
    /// The capabilities that its action needs, which a plugin that runs it
    /// must have.
    pub capabilities: Vec<Capability>,
    /// Its parameters, in their order.
    pub params: Vec<Param>,
}

/// A parameter of a command: its name, the kind of its value, and its
/// default, which makes it optional.
#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    /// Its name: lowercase letters, digits and `-`, from a letter.
    pub name: String,
    /// The kind of value that it takes.
    pub kind: Kind,
    /// The value that a call without it takes; `None` for a required one.
    pub default: Option<Value>,
}

/// A value of an argument.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// Text.
    String(String),
    /// A whole number.
    Integer(i64),
    /// A number, whole or not.
    Number(f64),
    /// True or false.
    Boolean(bool),
}

impl Value {
    /// The kind of the value.
    pub fn kind(&self) -> Kind {
        match self {
            Self::String(_) => Kind::String,
            Self::Integer(_) => Kind::Integer,
            Self::Number(_) => Kind::Number,
            Self::Boolean(_) => Kind::Boolean,
        }
    }
}

/// The kind of a parameter's value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Text.
    String,
    /// A whole number.
    Integer,
    /// A number, which a whole one is too.
    Number,
    /// True or false.
    Boolean,
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::String => "a string",
            Self::Integer => "an integer",
            Self::Number => "a number",
            Self::Boolean => "a boolean",
        })
    }
}

/// The group of a command, for the user: the install screen and the
/// palette list the commands by it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    /// The app itself, as quitting it.
    App,
    /// The windows: opening, closing and moving between them.
    Windows,
    /// The tabs of a window.
    Tabs,
    /// The panes of a tab.
    Panes,
    /// What goes to a pane's program, as typed text.
    Input,
    /// The clipboard.
    Clipboard,
    /// What a pane shows and how: its scrollback, the font's size.
    View,
}

/// A capability: what a plugin may reach beyond its own computation, which
/// its manifest declares and the user grants.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Capability {
    /// Reading the screen's and the scrollback's text, and the selection.
    TerminalRead,
    /// Sending input to a pane's program.
    PtyWrite,
    /// Reading the clipboard.
    ClipboardRead,
    /// Writing to the clipboard.
    ClipboardWrite,
    /// Showing the system's notifications.
    Notifications,
    /// Opening URLs and files with the system's handler.
    Open,
    /// Starting processes.
    ProcessSpawn,
    /// HTTP requests to a host.
    Net(String),
    /// Reading a path.
    FsRead(String),
    /// Writing a path.
    FsWrite(String),
    /// Shortcuts of the whole system.
    GlobalHotkey,
    /// Reading a variable of the app's environment and of the panes'
    /// programs', or those that a name with `*` matches.
    EnvRead(String),
    /// Seeing the keys typed before they reach the program.
    InputRead,
    /// Reading the commands that the shell ran and their output.
    HistoryRead,
    /// Seeing the panes' processes: their command lines and working
    /// directories.
    ProcessRead,
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TerminalRead => f.write_str("terminal-read"),
            Self::PtyWrite => f.write_str("pty-write"),
            Self::ClipboardRead => f.write_str("clipboard-read"),
            Self::ClipboardWrite => f.write_str("clipboard-write"),
            Self::Notifications => f.write_str("notifications"),
            Self::Open => f.write_str("open"),
            Self::ProcessSpawn => f.write_str("process-spawn"),
            Self::Net(host) => write!(f, "net:{host}"),
            Self::FsRead(path) => write!(f, "fs-read:{path}"),
            Self::FsWrite(path) => write!(f, "fs-write:{path}"),
            Self::GlobalHotkey => f.write_str("global-hotkey"),
            Self::EnvRead(name) => write!(f, "env-read:{name}"),
            Self::InputRead => f.write_str("input-read"),
            Self::HistoryRead => f.write_str("history-read"),
            Self::ProcessRead => f.write_str("process-read"),
        }
    }
}

/// What went wrong, by its code, which never changes: the hundreds tell
/// the category.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Code {
    /// 101: no command of the name.
    NotFound,
    /// 102: an argument that the command does not take.
    UnknownArgument,
    /// 103: a required argument missing.
    MissingArgument,
    /// 104: an argument of another kind than its parameter's.
    WrongKind,
    /// 105: a name that is not `namespace.action`, or a parameter's that is
    /// not one.
    InvalidName,
    /// 106: a command registered already.
    AlreadyRegistered,
    /// 107: a schema whose parameters repeat a name, or whose default is of
    /// another kind than its parameter.
    InvalidSchema,
    /// 108: a binding whose keys, command or context is not one.
    InvalidBinding,
    /// 201: the command ran, and failed.
    Failed,
    /// 301: a command that the plugin did not declare it runs.
    Undeclared,
    /// 302: a capability that the command needs, and the plugin lacks.
    MissingCapability,
    /// 401: a command that broke, as by a panic.
    Fault,
}

impl Code {
    /// The code's number.
    pub fn number(self) -> u16 {
        match self {
            Self::NotFound => 101,
            Self::UnknownArgument => 102,
            Self::MissingArgument => 103,
            Self::WrongKind => 104,
            Self::InvalidName => 105,
            Self::AlreadyRegistered => 106,
            Self::InvalidSchema => 107,
            Self::InvalidBinding => 108,
            Self::Failed => 201,
            Self::Undeclared => 301,
            Self::MissingCapability => 302,
            Self::Fault => 401,
        }
    }

    /// The code's kind, as the Luau API names it.
    pub fn kind(self) -> &'static str {
        match self {
            Self::NotFound => "not-found",
            Self::UnknownArgument | Self::MissingArgument | Self::WrongKind => "invalid-args",
            Self::InvalidName
            | Self::AlreadyRegistered
            | Self::InvalidSchema
            | Self::InvalidBinding => "invalid",
            Self::Failed => "failed",
            Self::Undeclared | Self::MissingCapability => "denied",
            Self::Fault => "fault",
        }
    }

    /// The code's category, which its hundreds tell.
    pub fn category(self) -> ErrorCategory {
        match self.number() / 100 {
            1 => ErrorCategory::Usage,
            2 => ErrorCategory::Failure,
            3 => ErrorCategory::Security,
            _ => ErrorCategory::Fault,
        }
    }
}

/// The category of an error, which tells how serious it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ErrorCategory {
    /// Low: the caller asked wrongly.
    Usage,
    /// Medium: the action did not succeed.
    Failure,
    /// High: a permission is missing.
    Security,
    /// Critical: Lokyr or a plugin broke.
    Fault,
}

impl fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Usage => "usage",
            Self::Failure => "failure",
            Self::Security => "security",
            Self::Fault => "fault",
        })
    }
}

/// An error of a command's registration or call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    /// What went wrong.
    pub code: Code,
    /// What went wrong, for a person.
    pub message: String,
}

impl Error {
    /// An error of `code`, which `message` tells.
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "error {} ({}): {}",
            self.code.number(),
            self.code.category(),
            self.message
        )
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_s_hundreds_tell_its_category() {
        let codes = [
            (Code::NotFound, 101, "not-found", ErrorCategory::Usage),
            (
                Code::UnknownArgument,
                102,
                "invalid-args",
                ErrorCategory::Usage,
            ),
            (
                Code::MissingArgument,
                103,
                "invalid-args",
                ErrorCategory::Usage,
            ),
            (Code::WrongKind, 104, "invalid-args", ErrorCategory::Usage),
            (Code::InvalidName, 105, "invalid", ErrorCategory::Usage),
            (
                Code::AlreadyRegistered,
                106,
                "invalid",
                ErrorCategory::Usage,
            ),
            (Code::InvalidSchema, 107, "invalid", ErrorCategory::Usage),
            (Code::InvalidBinding, 108, "invalid", ErrorCategory::Usage),
            (Code::Failed, 201, "failed", ErrorCategory::Failure),
            (Code::Undeclared, 301, "denied", ErrorCategory::Security),
            (
                Code::MissingCapability,
                302,
                "denied",
                ErrorCategory::Security,
            ),
            (Code::Fault, 401, "fault", ErrorCategory::Fault),
        ];
        for (code, number, kind, category) in codes {
            assert_eq!(
                (code.number(), code.kind(), code.category()),
                (number, kind, category)
            );
        }
        // Severity grows with the hundreds.
        assert!(ErrorCategory::Usage < ErrorCategory::Failure);
        assert!(ErrorCategory::Security < ErrorCategory::Fault);
        let error = Error::new(Code::Undeclared, "`plugin` runs `pane.paste`");
        assert_eq!(
            error.to_string(),
            "error 301 (security): `plugin` runs `pane.paste`"
        );
    }

    #[test]
    fn capabilities_take_their_manifest_s_names() {
        assert_eq!(Capability::PtyWrite.to_string(), "pty-write");
        assert_eq!(
            Capability::EnvRead("AWS_*".to_owned()).to_string(),
            "env-read:AWS_*"
        );
        assert_eq!(Capability::ProcessRead.to_string(), "process-read");
    }
}
