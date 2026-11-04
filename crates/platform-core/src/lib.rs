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
use std::time::{Duration, Instant, SystemTime};

#[cfg(any(test, feature = "conformance"))]
pub mod conformance;
mod event;
#[cfg(any(test, feature = "fake"))]
pub mod fake;
mod geometry;
mod input;
mod menu;
mod quick;
mod runs;
#[cfg(test)]
mod tests;
mod waker;

pub use event::{AppEvent, Appearance, Dropped, WindowEvent};
pub use geometry::{ContentRect, PixelSize, Point, Size, Titlebar, WindowSize};
pub use input::{
    Ime, KeyCode, KeyEvent, KeyState, Modifiers, MouseButton, PointerArea, PointerShape,
    ScrollDelta, ScrollPhase,
};
pub use menu::{
    Alert, AlertId, AlertStyle, ContextMenu, MainMenu, Menu, MenuItem, MenuRole, Shortcut,
    ShortcutKey, StandardAction,
};
pub use quick::{GlobalKey, Length, QuickEdge, QuickPlace, QuickScreen, ScreenRect};
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
    /// The size of its content, in points, below its titlebar.
    pub size: Size,
    /// Whether it keeps its frame, its place and its size, as the user
    /// moves or resizes it, for a window that restores it: not where the
    /// platform places it, as it opens.
    pub remember: bool,
    /// Whether it opens at the frame that a window kept last, if one did,
    /// rather than of `size`, cascaded or centred: on a screen that is
    /// still there.
    pub restore: bool,
    /// Whether it opens of the size of the frame that a window kept last,
    /// if one did and it does not restore it, rather than of `size`, where
    /// the platform places it, cascaded or centred; within the screen.
    pub restore_size: bool,
}

/// A layer of a window over its content, which the window server blinks
/// without the app.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Layer {
    /// The text that blinks, under the cursor's.
    BlinkingText,
    /// The cursor's cells, over everything else.
    Cursor,
}

/// Where a layer of a window goes, and how it blinks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerPlace {
    /// Its top left corner, in points from the content's.
    pub origin: Point,
    /// Its size, in points.
    pub size: Size,
    /// How it blinks; `None` to show it, steady.
    pub blink: Option<Blink>,
}

/// A blink of a layer, which the window server runs without the app: from `start`, on, it holds visible, fades out, holds hidden, and
/// fades in, and goes round again; without fades, it switches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Blink {
    /// When it started, at its visible hold's start.
    pub start: Instant,
    /// How long it holds visible.
    pub visible: Duration,
    /// Its fade out.
    pub fade_out: Duration,
    /// The fade out's curve.
    pub fade_out_curve: Easing,
    /// How long it holds hidden.
    pub hidden: Duration,
    /// Its fade in.
    pub fade_in: Duration,
    /// The fade in's curve.
    pub fade_in_curve: Easing,
}

/// The part of an indeterminate progress that the window server moves
/// without the app's frames: a segment across a bar, from out of its left
/// edge to out of its right one, or an arc round a ring, clockwise, round
/// again, within the bar or the ring and the part of the window where it
/// shows; where the system reduces motion, held still, its opacity
/// pulsing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sweep {
    /// The bar's top left corner, or the ring's square's, in points from
    /// the content's.
    pub origin: Point,
    /// Its size, in points.
    pub size: Size,
    /// The part of the window where it shows, its top left corner and its
    /// size, in points, which clips it.
    pub within: (Point, Size),
    /// The radius of the bar's corners and of the segment's, in points.
    pub radius: f64,
    /// What moves.
    pub shape: SweepShape,
    /// The segment's color: sRGB's encoded components, from 0 to 1, and its
    /// opacity.
    pub color: [f64; 4],
    /// How long a sweep takes, from one edge to the other.
    pub period: Duration,
    /// When the sweeps started, at the bar's left, so that a sweep placed
    /// again goes on where it was.
    pub start: Instant,
}

/// What a sweep moves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SweepShape {
    /// A segment of a bar, across it.
    Bar {
        /// Its width, in points.
        width: f64,
    },
    /// An arc of a ring, round it, from its top.
    Ring {
        /// The part of the turn that it takes, from 0 to 1.
        span: f64,
        /// Its line's thickness, in points.
        thickness: f64,
    },
}

/// How a fade goes, from its start to its end.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Easing {
    /// At a constant rate.
    #[default]
    Linear,
    /// Slow at its start.
    EaseIn,
    /// Slow at its end.
    EaseOut,
    /// Slow at its start and at its end.
    EaseInOut,
}

/// What the process has used of the machine, as the system counts it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    /// The wakeups that brought the processor out of idle, since the
    /// process started.
    pub idle_wakeups: u64,
    /// The wakeups of interrupts, as a timer's, idle or not, since the
    /// process started.
    pub interrupt_wakeups: u64,
    /// The memory that the system counts against the process, in bytes:
    /// its footprint.
    pub footprint: u64,
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

/// What a target that opens is, as [`Platform::resolve_target`] tells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetKind {
    /// A URL of another scheme than `file`, which its application opens.
    Url,
    /// A directory, which the file manager opens.
    Directory,
    /// A file, which its application opens.
    File,
    /// A file that would run rather than open, as a program, an
    /// application or a script that a terminal runs.
    Program,
}

/// A target that opens, as [`Platform::resolve_target`] tells it: what it
/// is, by which the app decides what to do, and where it leads, which is
/// what opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    /// What it is.
    pub kind: TargetKind,
    /// A URL, as it was given, or the absolute path of the file that a
    /// path, or a `file` URL, leads to through its links and the system's
    /// aliases.
    pub location: String,
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

    /// The appearance that the app has: its own, or the system's.
    fn appearance(&self) -> Appearance;

    /// Whether the system's keyboard navigation reaches every control, as
    /// Tab moves the focus, the buttons too: Keyboard navigation in macOS's
    /// settings. Without it, Tab reaches the fields and the lists alone.
    fn keyboard_navigation(&self) -> bool;

    /// Whether the system reduces motion, Reduce motion in macOS's
    /// accessibility settings: the interface's transitions end at once.
    fn reduce_motion(&self) -> bool;

    /// Gives the app the appearance `appearance`, its windows' and its
    /// menus' too, whatever the system's is, or the system's again with
    /// `None`. [`Platform::appearance`] tells it at once; a change can come
    /// as [`AppEvent::AppearanceChanged`] too.
    fn set_appearance(&mut self, appearance: Option<Appearance>);

    /// The user's languages, each with a region, in the order of the
    /// system's preferences, as the names of POSIX locales without their
    /// encoding, as `it_IT` and `en_IT`: the system's language and region
    /// first, then the other languages that the user prefers.
    fn locales(&self) -> Vec<String>;

    /// The pixels in a point of the screen that a new window opens on: 1 or
    /// 2 on the Mac.
    fn screen_scale(&self) -> f64;

    /// What the process has used of the machine so far, as the system
    /// counts it; none where the system does not tell.
    fn usage(&self) -> Option<Usage>;

    /// When the process started, as the system recorded it; none where the
    /// system does not tell.
    fn process_started(&self) -> Option<SystemTime>;

    /// Puts a press of the key `code`, typing `text`, then its release, at
    /// the end of the app's queue of events, for `window`, as the keyboard
    /// would, stamped now: a bench's keys, which come back as
    /// [`WindowEvent::Key`] the way the keyboard's do.
    fn post_key(&mut self, window: WindowId, code: KeyCode, text: &str);

    /// Opens a window, in front of the others, where it takes the focus:
    /// down and to the right of the window that has the focus, or of the
    /// front one of the app's, as the system cascades windows, or in the
    /// screen's centre.
    fn create_window(&mut self, attributes: &WindowAttributes) -> Result<WindowId, Error>;

    /// Opens a quick window, of the title `title`, hidden: one without a
    /// titlebar, which slides in from an edge of a screen over every app's
    /// windows, those in full screen too, and out again, as
    /// [`Platform::show_quick_window`] says. It keeps no frame.
    fn create_quick_window(&mut self, title: &str) -> Result<WindowId, Error>;

    /// Slides the quick window `window` in, as `place` says, if it is
    /// hidden, at the size of its place, and gives it the keyboard's focus,
    /// which it takes from the app that has it; or, with `None`, slides it
    /// out, and gives the focus back to that app, if the focus is still
    /// the app's own. A window that is no quick window takes nothing.
    fn show_quick_window(&mut self, window: WindowId, place: Option<&QuickPlace>);

    /// Takes the keys of `keys` from every app, as the system gives them,
    /// in place of those that it took before: each press of one comes as
    /// [`AppEvent::GlobalKey`], with its index. Gives the indices of those
    /// that the system refused, as another app takes them.
    fn set_global_keys(&mut self, keys: &[GlobalKey]) -> Vec<usize>;

    /// Closes a window: no event comes from it any more.
    fn close_window(&mut self, window: WindowId);

    /// Whether `window` keeps its frame as the user moves or resizes it
    /// from now, as [`WindowAttributes::remember`] has it.
    fn set_window_remember(&mut self, window: WindowId, remember: bool);

    /// Sets the title of a window.
    fn set_title(&mut self, window: WindowId, title: &str);

    /// The size of a window's content.
    fn window_size(&self, window: WindowId) -> Option<WindowSize>;

    /// The shortest time between two refreshes of the display that a
    /// window is on: a sixtieth of a second at 60 Hz.
    fn refresh_interval(&self, window: WindowId) -> Option<Duration>;

    /// What a renderer draws the window's content into: on the Mac, its
    /// content view, whose layer is a `CAMetalLayer`. It is valid while the
    /// platform is borrowed, as no window can close meanwhile; a renderer
    /// that keeps the layer retains it. `None` for a platform that draws
    /// nothing, such as the fake one.
    fn window_handle(&self, window: WindowId) -> Option<WindowHandle<'_>>;

    /// What a renderer draws a layer of a window into, over its content:
    /// on the Mac, a view of its own, whose layer is a `CAMetalLayer`, clear
    /// where nothing is drawn, which the window server blinks, as
    /// [`Platform::place_layer`] says. Valid as [`Platform::window_handle`]'s
    /// is; `None` for a platform that draws nothing, such as the fake one.
    fn layer_handle(&self, window: WindowId, layer: Layer) -> Option<WindowHandle<'_>>;

    /// Places `layer` of `window`, which shows what the renderer drew into
    /// it, blinking as the place says; `None` hides it.
    fn place_layer(&mut self, window: WindowId, layer: Layer, place: Option<&LayerPlace>);

    /// Shows `sweeps` over `window`'s content, under its layers, in place of
    /// those that it showed: none hides them. Those as they were go on
    /// unchanged.
    fn set_sweeps(&mut self, window: WindowId, sweeps: &[Sweep]);

    /// Asks for one [`WindowEvent::Frame`] of a window: at once, once the
    /// current call returns, if the window drew no frame since its display's
    /// last refresh, as after a while without one; otherwise at the next
    /// refresh, so that frames keep to the display's pace. The requests made
    /// before a frame come as that one. A window that asks for no frame
    /// wakes nothing, and one that is hidden or covered, as
    /// [`WindowEvent::Occluded`] tells, gets the frame that it asked for
    /// once it shows again.
    fn request_frame(&mut self, window: WindowId);

    /// Brings a window in front of the others, with the focus; a hidden
    /// quick window, which [`Platform::show_quick_window`] shows, takes
    /// nothing.
    fn focus_window(&mut self, window: WindowId);

    /// The window that has the keyboard's focus, if one of the
    /// application's has it.
    fn focused_window(&self) -> Option<WindowId>;

    /// Where the pointer is now, over `window`'s content or out of it, in
    /// points from the content's top left corner, as a press of a modifier
    /// asks without a move; `None` for a window that is not open.
    fn pointer_position(&self, window: WindowId) -> Option<Point>;

    /// Sets the main menu, in place of the one before it.
    fn set_main_menu(&mut self, menu: &MainMenu);

    /// Enables the main menu's items of `command`, or dims them, which then
    /// neither run nor take their keys; every command's are enabled until
    /// the app says otherwise, through a change of the menu too.
    fn set_command_enabled(&mut self, command: &str, enabled: bool);

    /// Ticks the main menu's items of `command`, or takes their tick away,
    /// as the choice that is in force among others; none is ticked until
    /// the app says so, and a change of the menu keeps the ticks.
    fn set_command_checked(&mut self, command: &str, checked: bool);

    /// Opens `menu` at `position` of `window`'s content, once the call in
    /// progress has returned, so that the windows go on drawing while it is
    /// open: its chosen item comes as [`WindowEvent::ContextCommand`]. A
    /// menu that is open closes first.
    fn show_context_menu(&mut self, window: WindowId, position: Point, menu: &ContextMenu);

    /// Shows the system's definition of `text`, in its dictionary's panel,
    /// over `window`, where the text is: the left end of its baseline at
    /// `baseline`, from the content's top left corner, in `font`, the family
    /// and its size in points, or in the system's font of a fixed pitch.
    fn show_definition(
        &mut self,
        window: WindowId,
        text: &str,
        baseline: Point,
        font: Option<(&str, f64)>,
    );

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

    /// Ends the input method's composition in `window`, whose text the app
    /// keeps as it stands, as a click elsewhere keeps it: the input method
    /// forgets it, and types nothing of it.
    fn end_composition(&mut self, window: WindowId);

    /// Makes `window` translucent, over `backdrop`, which covers its whole
    /// frame, the titlebar too, under what the app draws: what is behind
    /// the window shows through it, and through what the app draws clear;
    /// or, with `None`, keeps the window opaque, as it opens.
    fn set_translucent(&mut self, window: WindowId, backdrop: Option<Backdrop>);

    /// Gives `window` the appearance `appearance`, its titlebar's too,
    /// whatever the app's is, as a window of a light background needs a
    /// dark title over it; or the app's again with `None`.
    fn set_window_appearance(&mut self, window: WindowId, appearance: Option<Appearance>);

    /// Whether the pointer's moves over `window` with no button held come
    /// as [`WindowEvent::MouseMoved`]: not as it opens, as each move would
    /// wake the app. Moves with a button held always come.
    fn track_mouse_moves(&mut self, window: WindowId, track: bool);

    /// Gives the pointer over `window`'s content `shape`; the arrow until
    /// the app asks for another.
    fn set_pointer_shape(&mut self, window: WindowId, shape: PointerShape);

    /// Turns the system's secure entry of the keyboard on or off, as the
    /// app's input is a password's: on the Mac, Secure Keyboard Entry,
    /// which keeps other apps from reading the keys. Off until the app
    /// turns it on, and off again as the app ends.
    fn set_secure_input(&mut self, on: bool);

    /// Gives the pointer over each of `areas` of `window`'s content its own
    /// shape, the first area over those after it, and all over the shape
    /// of the rest, in place of the areas before; none until the app asks.
    /// The pointer changes as it moves, without waking the app.
    fn set_pointer_areas(&mut self, window: WindowId, areas: &[PointerArea]);

    /// Gives `window` the titlebar `titlebar`, of the system's title by
    /// default: its content's size then comes anew as
    /// [`WindowEvent::Resized`], of the titlebar's height and of the room
    /// of the window's buttons.
    fn set_titlebar(&mut self, window: WindowId, titlebar: Titlebar);

    /// The parts of `window`'s titlebar that are its content's, as the tabs
    /// of a bar that it draws there, in place of those before: a press
    /// there, its drags and its release come to the app, and the pointer's
    /// moves over the titlebar too while the window's titlebar is of
    /// [`Titlebar::Tabs`]. A press elsewhere on the titlebar is the
    /// window's, which a drag moves and a double click zooms or minimizes,
    /// as the system's settings say. None until the app gives them.
    fn set_titlebar_areas(&mut self, window: WindowId, areas: &[ContentRect]);

    /// Whether an accessibility client asked for the tree of `window`,
    /// which [`WindowEvent::AccessibilityAsked`] told: its changes go to
    /// [`Platform::update_accessibility`] from then on.
    fn accessibility_asked(&self, window: WindowId) -> bool;

    /// Gives the accessibility tree of `window` `update`: whole the first
    /// time after [`WindowEvent::AccessibilityAsked`], its nodes changed
    /// after; nothing before a client asked.
    fn update_accessibility(&mut self, window: WindowId, update: accesskit::TreeUpdate);

    /// Which Option keys act as Meta, the left one and the right one: their
    /// keys come as they are, never through the input method, so that a
    /// dead key that Option makes does not compose.
    fn set_option_as_meta(&mut self, left: bool, right: bool);

    /// Starts the application's updates, where it can update itself, as
    /// its bundle holds an updater on the Mac: a check for a newer version
    /// now and then, a day apart at least, while `check` holds, and those
    /// that [`Platform::check_for_updates`] asks. Whether the application
    /// can update itself; the updater may still be loading, out of the
    /// main thread, as the call returns. A second call changes nothing.
    fn start_updates(&mut self, check: bool) -> bool;

    /// Turns the updates' checks now and then on or off, once
    /// [`Platform::start_updates`] has started them.
    fn set_update_checks(&mut self, check: bool);

    /// Checks for a newer version now, and shows what it finds, a newer
    /// version to install or none, in the updater's own window; once the
    /// updater has loaded, for a check asked before. Nothing where the
    /// application cannot update itself.
    fn check_for_updates(&mut self);

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

    /// What `target`, a URL or an absolute path, is, and where it leads: a
    /// `file` URL is its path's, if it names this machine; a path's links
    /// and the system's aliases are followed, so that what the app decides
    /// by is what opens. A path that leads nowhere, a `file` URL of another
    /// machine and a text that is no URL fail. The platform tells the
    /// facts, and decides nothing: the app decides what opens, and how.
    fn resolve_target(&self, target: &str) -> Result<Target, Error>;

    /// Opens `target`, as [`Platform::resolve_target`] told it, with the
    /// system's handler: a URL in its application, as the browser; a file
    /// in its application, and a directory in the file manager. A
    /// [`TargetKind::Program`] fails, and nothing runs.
    fn open(&mut self, target: &Target) -> Result<(), Error>;

    /// The system's name and version, as a report of a problem tells
    /// them: `macOS 27.2 (27C5032e)` on the Mac, its build in brackets.
    fn system_version(&self) -> String;

    /// Shows `target`, as [`Platform::resolve_target`] told it, a file, a
    /// directory or a program, in the file manager, selected; a URL fails.
    fn reveal(&mut self, target: &Target) -> Result<(), Error>;

    /// Opens `path`, a file, in the system's text editor: the application
    /// that opens plain text, whatever the file's own type opens in. What
    /// fails once the system took the open on, as an editor that does not
    /// start, comes as [`AppEvent::OpenFailed`].
    fn open_in_text_editor(&mut self, path: &Path) -> Result<(), Error>;
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
