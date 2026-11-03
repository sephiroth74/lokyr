//! The main menu, the context menus, and alerts.

use crate::KeyCode;

/// The application's main menu: on the Mac, the menu bar, whose first menu
/// is the application's own.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MainMenu {
    /// Its menus, from the left.
    pub menus: Vec<Menu>,
}

/// A menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Menu {
    /// Its title.
    pub title: String,
    /// Its items, from the top.
    pub items: Vec<MenuItem>,
    /// The part that it plays for the system, if one.
    pub role: Option<MenuRole>,
}

/// A menu that opens over a window, at the pointer as a right click opens
/// it, or at a menu panel's anchor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContextMenu {
    /// Its items, from the top: a command's comes as
    /// [`WindowEvent::ContextCommand`](crate::WindowEvent::ContextCommand)
    /// when it is chosen.
    pub items: Vec<MenuItem>,
    /// The commands whose items are dimmed: the others' are enabled, as the
    /// main menu's dimmed ones are not.
    pub dimmed: Vec<String>,
    /// The commands whose items are ticked, as a toggle's that is on.
    pub ticked: Vec<String>,
}

/// The part that a menu plays for the system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuRole {
    /// The windows' menu, which the system lists the windows in.
    Window,
    /// The help's menu, which the system gives a field to search the menus.
    Help,
}

/// An item of a menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuItem {
    /// An item that runs a command, which comes as
    /// [`AppEvent::Command`](crate::AppEvent::Command) when it is chosen, or
    /// as [`WindowEvent::ContextCommand`](crate::WindowEvent::ContextCommand)
    /// from a context menu.
    Command {
        /// What the item shows.
        title: String,
        /// The command.
        command: String,
        /// The keys that choose it, which the item shows.
        shortcut: Option<Shortcut>,
    },
    /// An item that does what the system does, as hiding the application.
    Standard {
        /// What the item shows.
        title: String,
        /// What it does.
        action: StandardAction,
        /// The keys that choose it, which the item shows.
        shortcut: Option<Shortcut>,
    },
    /// A line between groups of items.
    Separator,
    /// A menu within the menu.
    Submenu(Menu),
}

/// What a menu's item of the system's does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StandardAction {
    /// Hides the application.
    Hide,
    /// Hides the other applications.
    HideOthers,
    /// Shows every application.
    ShowAll,
    /// Minimizes the window that has the focus.
    Minimize,
    /// Zooms the window that has the focus, or takes it back.
    Zoom,
    /// Brings every window of the application to the front.
    BringAllToFront,
    /// Opens the system's palette of emoji and symbols, whose character
    /// comes to the window that has the focus as an input method's
    /// [`Ime::Commit`](crate::Ime::Commit).
    CharacterPalette,
}

/// The keys that choose a menu item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shortcut {
    /// The key.
    pub key: ShortcutKey,
    /// Command, or Super.
    pub command: bool,
    /// Shift.
    pub shift: bool,
    /// Either Alt, or Option.
    pub alt: bool,
    /// Control.
    pub control: bool,
}

/// The key of a [`Shortcut`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShortcutKey {
    /// The key that types this character, in the current layout.
    Char(char),
    /// A key that types no character, such as an arrow or a function key.
    Code(KeyCode),
}

/// A question or a notice, which waits for the user to close it with one of
/// its buttons.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alert {
    /// How serious it is.
    pub style: AlertStyle,
    /// What it says, in a line.
    pub message: String,
    /// What it explains under the message; empty for nothing.
    pub detail: String,
    /// Its buttons, from the default one, which Return chooses. The
    /// platform gives an alert without buttons an OK button.
    pub buttons: Vec<String>,
    /// The title of a checkbox under the detail, as "Don't ask again",
    /// whose state the answer tells; `None` for none.
    pub suppression: Option<String>,
    /// A field of text under the detail, which holds this text as the
    /// alert opens, and whose text the answer gives; `None` for none.
    pub input: Option<String>,
}

/// How serious an alert is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlertStyle {
    /// It informs.
    #[default]
    Informational,
    /// It warns of what an answer does.
    Warning,
    /// It warns of something that cannot be undone.
    Critical,
}

/// An alert that is open, which [`AppEvent::AlertClosed`](crate::AppEvent::AlertClosed)
/// names when it closes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AlertId(u64);

impl AlertId {
    /// The alert of the platform's number `id`, which no other alert of the
    /// process takes.
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    /// The platform's number for the alert.
    pub fn get(self) -> u64 {
        self.0
    }
}
