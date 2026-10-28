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

use std::path::Path;
use std::time::Instant;

mod event;
#[cfg(any(test, feature = "fake"))]
pub mod fake;
mod geometry;
mod input;
mod menu;
mod runs;
#[cfg(test)]
mod tests;
mod waker;

pub use event::{AppEvent, Appearance, WindowEvent};
pub use geometry::{PixelSize, Point, Size, WindowSize};
pub use input::{
    Ime, KeyCode, KeyEvent, KeyState, Modifiers, MouseButton, PointerShape, ScrollDelta,
    ScrollPhase,
};
pub use menu::{Alert, AlertId, AlertStyle, MainMenu, Menu, MenuItem, Shortcut, ShortcutKey};
pub use raw_window_handle::WindowHandle;
pub use runs::{RUNS, runs_by_itself};
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

/// A timer that the app started, which no other timer of the process
/// takes, even once it has fired.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TimerId(u64);

impl TimerId {
    /// The timer of the platform's number `id`, which it never gives
    /// another timer of the process.
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    /// The platform's number for the timer.
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

/// The background of a translucent window, under what the app draws, and
/// through which what is behind the window shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Backdrop {
    /// Its color: sRGB's encoded components, from 0 to 1, and its opacity,
    /// from 0, clear, to 1, opaque.
    pub color: [f64; 4],
    /// The radius of the blur of what shows through it, in points; 0 for
    /// none.
    pub blur: f64,
}

/// What a target that opens is, as [`Platform::open`] takes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetKind {
    /// A URL of another scheme than `file`, which its application opens.
    Url,
    /// A directory, which the file manager opens.
    Directory,
    /// A file, which its application opens.
    File,
    /// A file that would run rather than open, as a program, an
    /// application or a script that a terminal runs, which the file manager
    /// shows instead.
    Program,
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
    /// After the system's request, [`AppEvent::QuitRequested`], it
    /// accepts it, and the system may end the process at once.
    fn quit(&mut self);

    /// Declines the system's request to quit, [`AppEvent::QuitRequested`],
    /// as the user cancelled it: what asked, as a logout, gives up. It does
    /// nothing without a request.
    fn refuse_quit(&mut self);

    /// The system's appearance.
    fn appearance(&self) -> Appearance;

    /// The user's languages, each with a region, in the order of the
    /// system's preferences, as the names of POSIX locales without their
    /// encoding, as `it_IT` and `en_IT`: the system's language and region
    /// first, then the other languages that the user prefers.
    fn locales(&self) -> Vec<String>;

    /// Opens a window, in front of the others, where it takes the focus.
    fn create_window(&mut self, attributes: &WindowAttributes) -> Result<WindowId, Error>;

    /// Closes a window: no event comes from it any more.
    fn close_window(&mut self, window: WindowId);

    /// Sets the title of a window.
    fn set_title(&mut self, window: WindowId, title: &str);

    /// The size of a window's content.
    fn window_size(&self, window: WindowId) -> Option<WindowSize>;

    /// What a renderer draws the window's content into: on the Mac, its
    /// content view, whose layer is a `CAMetalLayer`. It is valid while the
    /// platform is borrowed, as no window can close meanwhile; a renderer
    /// that keeps the layer retains it. `None` for a platform that draws
    /// nothing, such as the fake one.
    fn window_handle(&self, window: WindowId) -> Option<WindowHandle<'_>>;

    /// Asks for one [`WindowEvent::Frame`] of a window: at once, once the
    /// current call returns, if the window drew no frame since its display's
    /// last refresh, as after a while without one; otherwise at the next
    /// refresh, so that frames keep to the display's pace. The requests made
    /// before a frame come as that one. A window that asks for no frame
    /// wakes nothing.
    fn request_frame(&mut self, window: WindowId);

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

    /// Starts a timer that fires once, at `at` or a little after, as the
    /// system groups its wakeups, with an [`AppEvent::Timer`]; a time
    /// already past fires once the current call returns. It fires during a
    /// live resize and while a menu is open too.
    fn start_timer(&mut self, at: Instant) -> TimerId;

    /// Stops a timer, which then never fires; one that fired, or never was,
    /// takes nothing.
    fn cancel_timer(&mut self, timer: TimerId);

    /// Where the text that the user types goes in `window`: the cursor's
    /// cell, from the top left corner of its content, in points, by which
    /// the input method places its candidates.
    fn set_text_area(&mut self, window: WindowId, origin: Point, size: Size);

    /// Makes `window` translucent, over `backdrop`, which covers its whole
    /// frame, the titlebar too, under what the app draws: what is behind
    /// the window shows through it, and through what the app draws clear;
    /// or, with `None`, keeps the window opaque, as it opens.
    fn set_translucent(&mut self, window: WindowId, backdrop: Option<Backdrop>);

    /// Whether the pointer's moves over `window` with no button held come
    /// as [`WindowEvent::MouseMoved`]: not as it opens, as each move would
    /// wake the app. Moves with a button held always come.
    fn track_mouse_moves(&mut self, window: WindowId, track: bool);

    /// Gives the pointer over `window`'s content `shape`; the arrow until
    /// the app asks for another.
    fn set_pointer_shape(&mut self, window: WindowId, shape: PointerShape);

    /// Which Option keys act as Meta, the left one and the right one: their
    /// keys come as they are, never through the input method, so that a
    /// dead key that Option makes does not compose.
    fn set_option_as_meta(&mut self, left: bool, right: bool);

    /// Shows the image of the file at `icon` as the application's icon
    /// while it runs, where the system shows running applications, as the
    /// Dock and the application switcher on the Mac; or, with `None`, the
    /// application's own, as its bundle has it. What shows the application
    /// while it does not run, as the Finder, keeps its own.
    fn set_app_icon(&mut self, icon: Option<&Path>) -> Result<(), Error>;

    /// Asks for the user's attention while the application is not the
    /// active one, as the Dock's icon bounces once on the Mac; nothing
    /// while it is.
    fn request_attention(&mut self);

    /// Plays the system's alert sound, as the user chose it.
    fn play_alert_sound(&mut self);

    /// Shows a notification of the system's, of `title` and `body`, for
    /// `window`: a click on it comes as [`AppEvent::NotificationClicked`].
    /// The first asks the user for the permission to show them, which the
    /// system keeps; without it, nothing shows. On the Mac, the application
    /// needs its bundle's identifier: from outside its bundle, nothing shows.
    fn notify(&mut self, window: WindowId, title: &str, body: &str);

    /// What `target`, a URL or an absolute path, is, as [`Platform::open`]
    /// takes it: a `file` URL is its path's, if it names this machine; a
    /// path that does not exist fails.
    fn target_kind(&self, target: &str) -> Result<TargetKind, Error>;

    /// Opens `target`, a URL or an absolute path, with the system's handler:
    /// a URL in its application, as the browser; a file in its
    /// application, and a directory in the file manager. A file that would
    /// run rather than open, a [`TargetKind::Program`], shows in the file
    /// manager, selected, and does not run.
    fn open(&mut self, target: &str) -> Result<(), Error>;

    /// Shows `target`, an absolute path or a `file` URL, in the file
    /// manager, selected; another URL fails.
    fn reveal(&mut self, target: &str) -> Result<(), Error>;
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
    /// An image could not be read as the application's icon.
    #[error("the icon {0} could not be read")]
    Icon(String),
    /// A URL or a path could not be opened.
    #[error("{0} could not be opened: {1}")]
    Open(String, String),
}
