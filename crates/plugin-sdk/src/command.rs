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

use serde::{Deserialize, Serialize};

/// The arguments of a call: values by their parameters' names.
pub type Args = BTreeMap<String, Value>;

/// A command, as its registration describes it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Command {
    /// Its name, `namespace.action`.
    pub name: String,
    /// Its title, for the menus and the palette.
    pub title: String,
    /// The group that it belongs to, for the user.
    pub category: Category,
    /// The capabilities that its action needs, which a plugin that runs it
    /// must have; none if not given.
    #[serde(default)]
    pub capabilities: Vec<Capability>,
    /// Its parameters, in their order; none if not given.
    #[serde(default)]
    pub params: Vec<Param>,
    /// The context in which it can run, as a binding's `when` writes it, as
    /// `selection`; every context without one. The menus dim its item
    /// outside it.
    pub when: Option<String>,
}

/// A parameter of a command: its name, the kind of its value, and its
/// default, which makes it optional.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Param {
    /// Its name: lowercase letters, digits and `-`, from a letter.
    pub name: String,
    /// The kind of value that it takes.
    pub kind: Kind,
    /// The value that a call without it takes; `None` for a required one.
    pub default: Option<Value>,
}

/// A value of an argument.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
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
    /// Handling what a pane holds, but through the clipboard: selecting
    /// it, clearing it.
    Edit,
    /// Finding text in a pane, as the find bar and its steps.
    Find,
    /// What a pane shows and how: its scrollback, the font's size.
    View,
}

/// A capability: what a plugin may reach beyond its own computation, which
/// its manifest declares and the user grants.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
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
    /// Running any command as the user's, without declaring it, and
    /// reading the whole registry, with each command's plugin.
    UserCommands,
    /// Opening, closing and moving the windows, the tabs and the panes,
    /// the keyboard's focus between them, a split's ratio, the zoom, and
    /// the app's quit.
    WorkspaceWrite,
    /// The panes' scroll and selection, the screen and the scrollback
    /// cleared, the font's size, the tabs' titles and colors, the tab
    /// bar's place, the inactive panes' fade and the focus that follows
    /// the mouse.
    PaneView,
    /// Drawing over a pane: decorations, the gutter's marks, a search's
    /// matches and a held cursor.
    Decorate,
    /// The cell under the pointer over the panes, and the modifiers held.
    PointerRead,
    /// Keeping the system, or the system and the display, from idle sleep.
    KeepAwake,
    /// The themes' and the font family's previews, and their choices named
    /// in the config, `[theme]` and `[font] family`.
    AppearanceWrite,
    /// The plugins themselves, through Sleip: the installed ones, the
    /// registry's search, and installs, updates and removals; Lokyr's own
    /// plugins' alone.
    Packages,
    /// The config in force, the app's keys and every plugin's section, with
    /// their schemas, read, and its keys written; Lokyr's own plugins'
    /// alone.
    Config,
}

impl Capability {
    /// The capability of no parameter of the manifest's name `name`, as
    /// `pty-write`; `None` of another name, or of one that takes a
    /// parameter, as `net`.
    pub fn of_name(name: &str) -> Option<Self> {
        Some(match name {
            "terminal-read" => Self::TerminalRead,
            "pty-write" => Self::PtyWrite,
            "clipboard-read" => Self::ClipboardRead,
            "clipboard-write" => Self::ClipboardWrite,
            "notifications" => Self::Notifications,
            "open" => Self::Open,
            "process-spawn" => Self::ProcessSpawn,
            "global-hotkey" => Self::GlobalHotkey,
            "input-read" => Self::InputRead,
            "history-read" => Self::HistoryRead,
            "process-read" => Self::ProcessRead,
            "user-commands" => Self::UserCommands,
            "workspace-write" => Self::WorkspaceWrite,
            "pane-view" => Self::PaneView,
            "decorate" => Self::Decorate,
            "pointer-read" => Self::PointerRead,
            "keep-awake" => Self::KeepAwake,
            "appearance-write" => Self::AppearanceWrite,
            "packages" => Self::Packages,
            "config" => Self::Config,
            _ => return None,
        })
    }

    /// The capability of the manifest's name `name`, which takes a
    /// parameter, of `parameter`: a host of `net`, a path of `fs-read` and
    /// `fs-write`, a variable of `env-read`; `None` of another name.
    pub fn of_parameter(name: &str, parameter: String) -> Option<Self> {
        Some(match name {
            "net" => Self::Net(parameter),
            "fs-read" => Self::FsRead(parameter),
            "fs-write" => Self::FsWrite(parameter),
            "env-read" => Self::EnvRead(parameter),
            _ => return None,
        })
    }

    /// Whether holding this capability lets a plugin do what `needed`
    /// asks: the same one; or, of a parameter, one that covers it: of
    /// `fs-read:` and `fs-write:`, a folder of the path, or of a folder
    /// above it, of whole components, neither of `..`; of `env-read:`, a
    /// name that matches the variable, `*` as any characters, as `AWS_*`;
    /// of `net:`, the host, whatever its case.
    pub fn grants(&self, needed: &Capability) -> bool {
        match (self, needed) {
            (Self::FsRead(folder), Self::FsRead(path))
            | (Self::FsWrite(folder), Self::FsWrite(path)) => is_under(path, folder),
            (Self::EnvRead(pattern), Self::EnvRead(name)) => matches(pattern, name),
            (Self::Net(host), Self::Net(asked)) => host
                .trim_end_matches('.')
                .eq_ignore_ascii_case(asked.trim_end_matches('.')),
            _ => self == needed,
        }
    }

    /// Whether the host gives it to no plugin yet, whatever a manifest
    /// declares, as it has no request of it: the network's and the
    /// processes'. A manifest that declares one has it refused, with a
    /// notice, and its plugin runs without it.
    pub fn is_withheld(&self) -> bool {
        matches!(self, Self::Net(_) | Self::ProcessSpawn)
    }

    /// Whether Lokyr's own plugins alone, compiled into the app, may hold
    /// it, which a manifest of another plugin has refused, with a notice:
    /// the plugins' install and removal.
    pub fn is_first_party_only(&self) -> bool {
        matches!(self, Self::Packages | Self::Config)
    }
}

/// Whether `granted` holds a capability that grants `needed`.
pub fn grants(granted: &[Capability], needed: &Capability) -> bool {
    granted.iter().any(|capability| capability.grants(needed))
}

/// Whether `path` is `folder` or under it, of whole components, neither
/// of `..`, which could climb out of it.
fn is_under(path: &str, folder: &str) -> bool {
    use std::path::{Component, Path};
    let (path, folder) = (Path::new(path), Path::new(folder));
    let climbs = |path: &Path| {
        path.components()
            .any(|component| matches!(component, Component::ParentDir))
    };
    !climbs(path) && !climbs(folder) && path.starts_with(folder)
}

/// Whether `text` matches `pattern`, whose `*` stands for any characters,
/// none too.
fn matches(pattern: &str, text: &str) -> bool {
    let mut parts = pattern.split('*');
    let first = parts.next().unwrap_or_default();
    let Some(mut rest) = text.strip_prefix(first) else {
        return false;
    };
    let parts: Vec<&str> = parts.collect();
    let Some((last, middle)) = parts.split_last() else {
        // No `*`: the whole text.
        return rest.is_empty();
    };
    for part in middle {
        match rest.find(part) {
            Some(at) => rest = &rest[at + part.len()..],
            None => return false,
        }
    }
    rest.len() >= last.len() && rest.ends_with(last)
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
            Self::UserCommands => f.write_str("user-commands"),
            Self::WorkspaceWrite => f.write_str("workspace-write"),
            Self::PaneView => f.write_str("pane-view"),
            Self::Decorate => f.write_str("decorate"),
            Self::PointerRead => f.write_str("pointer-read"),
            Self::KeepAwake => f.write_str("keep-awake"),
            Self::AppearanceWrite => f.write_str("appearance-write"),
            Self::Packages => f.write_str("packages"),
            Self::Config => f.write_str("config"),
        }
    }
}

/// What went wrong, by its code, which never changes: the hundreds tell
/// the category. It crosses by its number, as Luau's errors carry it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "u16", try_from = "u16")]
pub enum Code {
    /// 101: no command of the name, or no window, tab or pane of the id,
    /// as one that closed.
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
    /// 109: a config that does not read, or whose values are not the
    /// keys'.
    InvalidConfig,
    /// 201: the command ran, and failed.
    Failed,
    /// 202: what an action needs is not there, as a font's family that is
    /// not installed.
    Unavailable,
    /// 301: a command that the plugin did not declare it runs.
    Undeclared,
    /// 302: a capability that the command needs, and the plugin lacks.
    MissingCapability,
    /// 401: a command that broke, as by a panic.
    Fault,
}

impl From<Code> for u16 {
    fn from(code: Code) -> Self {
        code.number()
    }
}

impl TryFrom<u16> for Code {
    type Error = String;

    /// The code of `number`, one of those that [`Code::number`] gives.
    fn try_from(number: u16) -> Result<Self, Self::Error> {
        Code::ALL
            .into_iter()
            .find(|code| code.number() == number)
            .ok_or_else(|| format!("{number} is no error's code"))
    }
}

impl Code {
    /// Every code, in the order of their numbers.
    pub const ALL: [Self; 14] = [
        Self::NotFound,
        Self::UnknownArgument,
        Self::MissingArgument,
        Self::WrongKind,
        Self::InvalidName,
        Self::AlreadyRegistered,
        Self::InvalidSchema,
        Self::InvalidBinding,
        Self::InvalidConfig,
        Self::Failed,
        Self::Unavailable,
        Self::Undeclared,
        Self::MissingCapability,
        Self::Fault,
    ];

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
            Self::InvalidConfig => 109,
            Self::Failed => 201,
            Self::Unavailable => 202,
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
            | Self::InvalidBinding
            | Self::InvalidConfig => "invalid",
            Self::Failed | Self::Unavailable => "failed",
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

/// Whether `name` is a name of a part: lowercase letters, digits and `-`,
/// from a letter.
pub fn is_part(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes.next().is_some_and(|first| first.is_ascii_lowercase())
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// Whether `name` is a command's name, `namespace.action`, each part of
/// lowercase letters, digits and `-`, from a letter.
pub fn is_name(name: &str) -> bool {
    name.contains('.') && name.split('.').all(is_part)
}

/// Whether `value` is of `kind`, an integer of a number too.
pub fn takes(kind: Kind, value: &Value) -> bool {
    value.kind() == kind || (kind == Kind::Number && value.kind() == Kind::Integer)
}

impl Command {
    /// Checks the command as the host registers it: its name is
    /// `namespace.action`, its title a label, as
    /// [`crate::text::check_label`] tells, its parameters have names of
    /// their own and defaults of their kinds, and its `when` is a context.
    pub fn check(&self) -> Result<(), Error> {
        let name = &self.name;
        if !is_name(name) {
            return Err(Error::new(
                Code::InvalidName,
                format!("`{name}` is not a command's name, `namespace.action`"),
            ));
        }
        crate::text::check_label(&self.title, &format!("the title of `{name}`"))?;
        for (at, param) in self.params.iter().enumerate() {
            if !is_part(&param.name) {
                return Err(Error::new(
                    Code::InvalidName,
                    format!("`{}` of `{name}` is not a parameter's name", param.name),
                ));
            }
            if self.params[..at]
                .iter()
                .any(|other| other.name == param.name)
            {
                return Err(Error::new(
                    Code::InvalidSchema,
                    format!("`{name}` has two parameters `{}`", param.name),
                ));
            }
            if let Some(default) = &param.default
                && !takes(param.kind, default)
            {
                return Err(Error::new(
                    Code::InvalidSchema,
                    format!(
                        "the default of `{}` of `{name}` is not {}",
                        param.name, param.kind
                    ),
                ));
            }
        }
        if let Some(when) = &self.when
            && let Err(error) = crate::keys::When::parse(when)
        {
            return Err(Error::new(
                Code::InvalidSchema,
                format!("the context of `{name}`, `{when}`, is not one: {error}"),
            ));
        }
        Ok(())
    }

    /// Checks a call of the command with `args`, and returns its arguments,
    /// with the defaults of those that it leaves out: an integer for a
    /// number becomes a number.
    pub fn check_call(&self, mut args: Args) -> Result<Args, Error> {
        let name = &self.name;
        if let Some(unknown) = args
            .keys()
            .find(|arg| !self.params.iter().any(|param| &param.name == *arg))
        {
            return Err(Error::new(
                Code::UnknownArgument,
                format!("`{name}` takes no argument `{unknown}`"),
            ));
        }
        for param in &self.params {
            match args.get_mut(&param.name) {
                Some(value) if !takes(param.kind, value) => {
                    return Err(Error::new(
                        Code::WrongKind,
                        format!(
                            "the argument `{}` of `{name}` is {}, not {}",
                            param.name,
                            param.kind,
                            value.kind()
                        ),
                    ));
                }
                Some(value) => {
                    if let (Kind::Number, Value::Integer(whole)) = (param.kind, &*value) {
                        // An integer of more than 53 bits rounds.
                        *value = Value::Number(*whole as f64);
                    }
                }
                None => match &param.default {
                    Some(default) => {
                        args.insert(param.name.clone(), default.clone());
                    }
                    None => {
                        return Err(Error::new(
                            Code::MissingArgument,
                            format!("`{name}` needs the argument `{}`", param.name),
                        ));
                    }
                },
            }
        }
        Ok(args)
    }
}

/// The commands that a plugin's manifest says it runs: names, and
/// namespaces, as `tabs.*`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Uses {
    patterns: Vec<String>,
}

impl Uses {
    /// The commands of `patterns`.
    pub fn new(patterns: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            patterns: patterns.into_iter().map(Into::into).collect(),
        }
    }

    /// Whether the command `name` is among them.
    pub fn allows(&self, name: &str) -> bool {
        self.patterns
            .iter()
            .any(|pattern| match pattern.strip_suffix('*') {
                // `tabs.*` takes `tabs.new`, not `tabs` nor `tabsx.new`.
                Some(prefix) => prefix.ends_with('.') && name.starts_with(prefix),
                None => pattern == name,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_authors_reference_names_every_capability_slot_and_code() {
        let reference = include_str!("../../../docs/plugins/reference.md");
        // Each capability's name in the manifest, of a match that a new one
        // fails to build until it is listed.
        let name = |capability: &Capability| match capability {
            Capability::Net(_) => "net:<host>".to_owned(),
            Capability::FsRead(_) => "fs-read:<path>".to_owned(),
            Capability::FsWrite(_) => "fs-write:<path>".to_owned(),
            Capability::EnvRead(_) => "env-read:<name>".to_owned(),
            Capability::TerminalRead
            | Capability::PtyWrite
            | Capability::ClipboardRead
            | Capability::ClipboardWrite
            | Capability::Notifications
            | Capability::Open
            | Capability::ProcessSpawn
            | Capability::GlobalHotkey
            | Capability::InputRead
            | Capability::HistoryRead
            | Capability::ProcessRead
            | Capability::UserCommands
            | Capability::WorkspaceWrite
            | Capability::PaneView
            | Capability::Decorate
            | Capability::PointerRead
            | Capability::KeepAwake
            | Capability::AppearanceWrite
            | Capability::Packages
            | Capability::Config => capability.to_string(),
        };
        let every = [
            Capability::TerminalRead,
            Capability::PtyWrite,
            Capability::ClipboardRead,
            Capability::ClipboardWrite,
            Capability::Notifications,
            Capability::Open,
            Capability::ProcessSpawn,
            Capability::Net(String::new()),
            Capability::FsRead(String::new()),
            Capability::FsWrite(String::new()),
            Capability::GlobalHotkey,
            Capability::EnvRead(String::new()),
            Capability::InputRead,
            Capability::HistoryRead,
            Capability::ProcessRead,
            Capability::UserCommands,
            Capability::WorkspaceWrite,
            Capability::PaneView,
            Capability::Decorate,
            Capability::PointerRead,
            Capability::KeepAwake,
            Capability::AppearanceWrite,
            Capability::Packages,
            Capability::Config,
        ];
        for capability in &every {
            let shown = format!("`{}`", name(capability));
            assert!(reference.contains(&shown), "{shown}");
        }
        #[cfg(feature = "unstable")]
        for slot in crate::ui::Slot::ALL {
            let shown = format!("| `{}` |", slot.name());
            assert!(reference.contains(&shown), "{shown}");
        }
        for code in Code::ALL {
            let shown = format!("| {} | `{}` |", code.number(), code.kind());
            assert!(reference.contains(&shown), "{shown}");
        }
    }

    #[test]
    fn a_command_s_title_is_a_label() {
        let command = |title: &str| Command {
            name: "acme.go".to_owned(),
            title: title.to_owned(),
            category: Category::View,
            capabilities: Vec::new(),
            params: Vec::new(),
            when: None,
        };
        assert_eq!(command("Go").check(), Ok(()));
        let long = "x".repeat(crate::text::LONGEST_LABEL + 1);
        for wrong in [long.as_str(), "C\u{200B}opy", "Go\n"] {
            let error = command(wrong).check().unwrap_err();
            assert_eq!(error.code, Code::InvalidSchema);
            assert!(error.message.contains("`acme.go`"), "{}", error.message);
        }
    }

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
            (Code::InvalidConfig, 109, "invalid", ErrorCategory::Usage),
            (Code::Failed, 201, "failed", ErrorCategory::Failure),
            (Code::Unavailable, 202, "failed", ErrorCategory::Failure),
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
    fn a_code_crosses_by_its_number_both_ways() {
        for code in Code::ALL {
            let text = serde_json::to_string(&code).unwrap();
            assert_eq!(text, code.number().to_string());
            assert_eq!(serde_json::from_str::<Code>(&text).unwrap(), code);
        }
        // An error, of its code's number; a number of no code is refused.
        let error = Error::new(Code::MissingCapability, "x");
        let text = serde_json::to_string(&error).unwrap();
        assert!(text.contains("\"code\":302"), "{text}");
        assert_eq!(serde_json::from_str::<Error>(&text).unwrap(), error);
        assert!(serde_json::from_str::<Code>("150").is_err());
        assert!(serde_json::from_str::<Code>("\"not-found\"").is_err());
        assert_eq!(Code::ALL.len(), 14);
    }

    #[test]
    fn capabilities_take_their_manifest_s_names() {
        assert_eq!(Capability::PtyWrite.to_string(), "pty-write");
        assert_eq!(
            Capability::EnvRead("AWS_*".to_owned()).to_string(),
            "env-read:AWS_*"
        );
        assert_eq!(Capability::ProcessRead.to_string(), "process-read");
        assert_eq!(Capability::WorkspaceWrite.to_string(), "workspace-write");
        assert_eq!(Capability::PaneView.to_string(), "pane-view");
        assert_eq!(Capability::Decorate.to_string(), "decorate");
        assert_eq!(Capability::PointerRead.to_string(), "pointer-read");
        assert_eq!(Capability::KeepAwake.to_string(), "keep-awake");
        assert_eq!(
            Capability::of_name("appearance-write"),
            Some(Capability::AppearanceWrite)
        );
        assert_eq!(
            Capability::of_name("keep-awake"),
            Some(Capability::KeepAwake)
        );
        assert_eq!(Capability::Packages.to_string(), "packages");
        assert_eq!(Capability::of_name("packages"), Some(Capability::Packages));
        assert!(Capability::Packages.is_first_party_only());
        assert!(!Capability::Packages.is_withheld());
        assert!(!Capability::KeepAwake.is_first_party_only());
        assert_eq!(Capability::of_name("config"), Some(Capability::Config));
        assert!(Capability::Config.is_first_party_only());
    }

    #[test]
    fn a_capability_reads_of_the_name_that_it_writes() {
        let plain = [
            Capability::TerminalRead,
            Capability::PtyWrite,
            Capability::ClipboardRead,
            Capability::ClipboardWrite,
            Capability::Notifications,
            Capability::Open,
            Capability::ProcessSpawn,
            Capability::GlobalHotkey,
            Capability::InputRead,
            Capability::HistoryRead,
            Capability::ProcessRead,
            Capability::UserCommands,
            Capability::WorkspaceWrite,
            Capability::PaneView,
            Capability::Decorate,
            Capability::PointerRead,
        ];
        for capability in plain {
            assert_eq!(
                Capability::of_name(&capability.to_string()),
                Some(capability)
            );
        }
        assert_eq!(Capability::of_name("net"), None);
        assert_eq!(Capability::of_name("decorations"), None);
        let net = Capability::of_parameter("net", "lokyr.dev".to_owned()).unwrap();
        assert_eq!(net.to_string(), "net:lokyr.dev");
        assert_eq!(Capability::of_parameter("pty-write", String::new()), None);
    }

    #[test]
    fn a_capability_of_a_parameter_grants_what_its_parameter_covers() {
        let fs = |path: &str| Capability::FsWrite(path.to_owned());
        let held = fs("/Users/ada/notes");
        assert!(held.grants(&fs("/Users/ada/notes")));
        assert!(held.grants(&fs("/Users/ada/notes/today.md")));
        assert!(held.grants(&fs("/Users/ada/notes/a/b")));
        // Of whole components, and never of `..`, nor another kind.
        assert!(!held.grants(&fs("/Users/ada/notes-old/x")));
        assert!(!held.grants(&fs("/Users/ada/notes/../.ssh/id_ed25519")));
        assert!(!held.grants(&fs("/Users/ada")));
        assert!(!held.grants(&Capability::FsRead("/Users/ada/notes/x".to_owned())));
        assert!(!fs("/Users/ada/../bob").grants(&fs("/Users/bob/x")));
        let env = |name: &str| Capability::EnvRead(name.to_owned());
        assert!(env("AWS_*").grants(&env("AWS_PROFILE")));
        assert!(env("AWS_*").grants(&env("AWS_")));
        assert!(!env("AWS_*").grants(&env("GITHUB_TOKEN")));
        assert!(!env("AWS_*").grants(&env("XAWS_PROFILE")));
        assert!(env("*_TOKEN").grants(&env("GITHUB_TOKEN")));
        assert!(env("A*B*C").grants(&env("AxxBxxC")));
        assert!(!env("A*B*C").grants(&env("AxxC")));
        assert!(!env("A*C").grants(&env("AC_")));
        assert!(env("HOME").grants(&env("HOME")));
        assert!(!env("HOME").grants(&env("HOMEDIR")));
        assert!(env("*").grants(&env("ANY")));
        let net = |host: &str| Capability::Net(host.to_owned());
        assert!(net("api.github.com").grants(&net("API.GitHub.com.")));
        assert!(!net("github.com").grants(&net("api.github.com")));
        // The others, of equality alone.
        assert!(Capability::PaneView.grants(&Capability::PaneView));
        assert!(!Capability::WorkspaceWrite.grants(&Capability::PaneView));
        assert!(grants(
            &[Capability::Decorate, env("AWS_*")],
            &env("AWS_REGION")
        ));
        assert!(!grants(&[Capability::Decorate], &Capability::PointerRead));
    }
}
