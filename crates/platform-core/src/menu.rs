//! The main menu, and alerts.

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
}

/// An item of a menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuItem {
    /// An item that runs a command, which comes as
    /// [`AppEvent::Command`](crate::AppEvent::Command) when it is chosen.
    Command {
        /// What the item shows.
        title: String,
        /// The command.
        command: String,
        /// The keys that choose it, which the item shows.
        shortcut: Option<Shortcut>,
    },
    /// A line between groups of items.
    Separator,
    /// A menu within the menu.
    Submenu(Menu),
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
