//! Platform interface implemented by each OS backend.
//!
//! The app asks things of the platform through [`Platform`], and the
//! platform calls the app back through [`Handler`], on the main thread. A
//! backend's `run` starts the application, calls [`Handler::started`], and
//! hands the app every event that comes in until the app calls
//! [`Platform::quit`].
//!
//! Nothing here is a platform's own type: windows and alerts are numbers,
//! and the rest plain values, so that the crate builds, and the app's logic
//! runs, on every platform.

#![forbid(unsafe_code)]

mod event;
#[cfg(any(test, feature = "fake"))]
pub mod fake;
mod geometry;
mod input;
mod menu;
#[cfg(test)]
mod tests;
mod waker;

pub use event::{AppEvent, Appearance, WindowEvent};
pub use geometry::{PixelSize, Point, Size, WindowSize};
pub use input::{
    Ime, KeyCode, KeyEvent, KeyState, Modifiers, MouseButton, ScrollDelta, ScrollPhase,
};
pub use menu::{Alert, AlertId, AlertStyle, MainMenu, Menu, MenuItem, Shortcut, ShortcutKey};
pub use waker::Waker;

/// A window, which no other window of the process takes, even once it is
/// closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WindowId(u64);

impl WindowId {
    /// The window of the platform's number `id`, which it never gives
    /// another window of the process.
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    /// The platform's number for the window.
    pub fn get(self) -> u64 {
        self.0
    }
}

/// How a new window starts.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowAttributes {
    /// Its title.
    pub title: String,
    /// The size of its content, in points.
    pub size: Size,
}

/// What the app asks of the platform, from the main thread.
///
/// A window that is closed, or that never was, takes nothing: what is done
/// to it does nothing, and what is asked of it gives `None`.
pub trait Platform {
    /// A handle that wakes the main thread from any thread.
    fn waker(&self) -> Waker;

    /// Ends the application once the current call of the [`Handler`]
    /// returns: no event comes after it, and the backend's `run` returns.
    fn quit(&mut self);

    /// The system's appearance.
    fn appearance(&self) -> Appearance;

    /// Opens a window, in front of the others, where it takes the focus.
    fn create_window(&mut self, attributes: &WindowAttributes) -> Result<WindowId, Error>;

    /// Closes a window: no event comes from it any more.
    fn close_window(&mut self, window: WindowId);

    /// Sets the title of a window.
    fn set_title(&mut self, window: WindowId, title: &str);

    /// The size of a window's content.
    fn window_size(&self, window: WindowId) -> Option<WindowSize>;

    /// Brings a window in front of the others, with the focus.
    fn focus_window(&mut self, window: WindowId);

    /// The window that has the keyboard's focus, if one of the
    /// application's has it.
    fn focused_window(&self) -> Option<WindowId>;

    /// Sets the main menu, in place of the one before it.
    fn set_main_menu(&mut self, menu: &MainMenu);

    /// The text on the clipboard, if it holds text.
    fn clipboard_text(&self) -> Option<String>;

    /// Puts text on the clipboard, in place of what it held.
    fn set_clipboard_text(&mut self, text: &str);

    /// Opens an alert, on `window` or on its own when `None`, and returns
    /// at once: [`AppEvent::AlertClosed`] tells which button closed it.
    fn show_alert(&mut self, window: Option<WindowId>, alert: &Alert) -> AlertId;
}

/// What the platform calls on the app, on the main thread.
///
/// A call never starts inside another: what the app causes during a call,
/// such as the focus that a new window takes, comes as an event once the
/// call returns.
pub trait Handler {
    /// The application started: the app opens its first window here.
    fn started(&mut self, platform: &mut dyn Platform);

    /// Something happened to the application.
    fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent);

    /// Something happened to one of the windows that the app opened, and has
    /// not closed.
    fn window_event(&mut self, platform: &mut dyn Platform, window: WindowId, event: WindowEvent);
}

/// What the platform could not do.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The application could not start.
    #[error("the application could not start: {0}")]
    Start(String),
    /// A window could not be opened.
    #[error("the window could not be opened: {0}")]
    Window(String),
}
