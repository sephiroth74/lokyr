//! The automation's requests that the platform's bridges, the scripts' and
//! the shortcuts' of the system, hand the app as
//! [`AppEvent::Automation`](crate::AppEvent::Automation), and the answers
//! that the app gives back by
//! [`Platform::answer_automation`](crate::Platform::answer_automation). The
//! windows, the tabs and the panes go by the app's ids of them.

/// What a script or a shortcut asks of the app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// The windows, each of its tabs, and each tab of its panes.
    Workspace,
    /// Opens a window, its pane in `folder`, running `command` at its
    /// shell's start, each if given.
    NewWindow {
        /// The folder of its pane.
        folder: Option<String>,
        /// The command line that its shell runs as it starts.
        command: Option<String>,
        /// The environment variables that its shell takes, by their names.
        environment: Vec<(String, String)>,
    },
    /// Opens a tab in `window`, or in the window in front, as a window's
    /// pane starts.
    NewTab {
        /// The window.
        window: Option<u64>,
        /// The folder of its pane.
        folder: Option<String>,
        /// The command line that its shell runs as it starts.
        command: Option<String>,
        /// The environment variables that its shell takes, by their names.
        environment: Vec<(String, String)>,
    },
    /// Splits `pane`, or the focused one, with a new pane at `edge`, as a
    /// window's pane starts.
    Split {
        /// The pane.
        pane: Option<u64>,
        /// Where the new pane goes.
        edge: Edge,
        /// The folder of its pane.
        folder: Option<String>,
        /// The command line that its shell runs as it starts.
        command: Option<String>,
        /// The environment variables that its shell takes, by their names.
        environment: Vec<(String, String)>,
    },
    /// Sends `text` to `pane`, or to the focused one, as typed.
    SendText {
        /// The pane.
        pane: Option<u64>,
        /// The text.
        text: String,
    },
    /// Sends `pane`, or the focused one, the key of `keys`, as `ctrl+c`,
    /// pressed and released.
    SendKey {
        /// The pane.
        pane: Option<u64>,
        /// The key and its modifiers, as a binding writes them.
        keys: String,
    },
    /// Brings a window, a tab or a pane forward.
    Focus(Target),
    /// Closes a window, a tab or a pane.
    Close(Target),
    /// Runs a command of the app's by its name and arguments.
    Run {
        /// The command's name.
        command: String,
        /// Its arguments, by their names.
        args: Vec<(String, Arg)>,
    },
    /// The text of `pane`, or of the focused one: its screen, or its
    /// selection.
    PaneText {
        /// The pane.
        pane: Option<u64>,
        /// Its selection, rather than its screen.
        selection: bool,
    },
    /// Switches to the session of `name`, making it first if there is none
    /// of that name.
    SwitchSession {
        /// The session's name.
        name: String,
    },
    /// The commands that the command palette lists, each of its name, its
    /// title and its entry's arguments.
    Commands,
}

/// A command's argument, as a script gives it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Arg {
    /// A text.
    Text(String),
    /// A whole number.
    Integer(i64),
    /// True or false.
    Bool(bool),
}

/// Where a split's new pane goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    /// On the right.
    Right,
    /// On the left.
    Left,
    /// Below.
    Down,
    /// Above.
    Up,
}

/// A window, a tab or a pane, by its id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// A window.
    Window(u64),
    /// A tab.
    Tab(u64),
    /// A pane.
    Pane(u64),
}

/// The app's answer to a request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    /// Done, of nothing to tell.
    Done,
    /// What a window, a tab or a split opened.
    Opened {
        /// The window.
        window: u64,
        /// The tab.
        tab: u64,
        /// The pane.
        pane: u64,
    },
    /// The windows, in their order.
    Workspace(Vec<Window>),
    /// A text.
    Text(String),
    /// The commands, in the app's order.
    Commands(Vec<Command>),
    /// Refused or failed: the code's number, whose hundreds tell its
    /// category, and what went wrong.
    Error {
        /// The code's number.
        code: u16,
        /// What went wrong.
        message: String,
    },
}

/// A window, as an answer tells it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Window {
    /// Its id.
    pub id: u64,
    /// Its title.
    pub title: String,
    /// Whether it is the window in front.
    pub focused: bool,
    /// Its tabs, in their order.
    pub tabs: Vec<Tab>,
}

/// A tab, as an answer tells it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tab {
    /// Its id.
    pub id: u64,
    /// Its title.
    pub title: String,
    /// Whether its window shows it.
    pub active: bool,
    /// Its panes.
    pub panes: Vec<Pane>,
}

/// A command as the command palette lists it, as an answer tells it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    /// Its name, as `tabs.new`.
    pub name: String,
    /// Its title, as the menus show it.
    pub title: String,
    /// The arguments that its entry gives, by their names.
    pub args: Vec<(String, Arg)>,
}

/// A pane, as an answer tells it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pane {
    /// Its id.
    pub id: u64,
    /// Its title.
    pub title: String,
    /// The folder that its program works in, as it told it.
    pub folder: Option<String>,
    /// Whether it has its tab's keyboard.
    pub focused: bool,
}
