//! The events that come in from the platform.

use crate::{
    AlertId, Ime, KeyEvent, Modifiers, MouseButton, Point, ScrollDelta, ScrollPhase, TimerId,
    WindowId, WindowSize,
};

/// Something that happened to a window, or in it.
#[derive(Clone, Debug, PartialEq)]
pub enum WindowEvent {
    /// The content changed size: in a live resize, once a step.
    Resized(WindowSize),
    /// The window moved to a display of another scale, which changes the
    /// size of its content in pixels.
    ScaleChanged(WindowSize),
    /// The window took the keyboard's focus, or lost it.
    Focused(bool),
    /// The window became hidden, minimized or covered, or visible again.
    Occluded(bool),
    /// The user asked to close the window, which stays open until the app
    /// closes it.
    CloseRequested,
    /// An item of the window's context menu was chosen: its command.
    ContextCommand(String),
    /// The window is ready for the frame that the app asked for, with
    /// [`Platform::request_frame`](crate::Platform::request_frame): the app
    /// draws it now.
    Frame,
    /// A key.
    Key(KeyEvent),
    /// The input method.
    Ime(Ime),
    /// A button of the mouse went down or up.
    MouseButton {
        /// The button.
        button: MouseButton,
        /// Whether it went down.
        pressed: bool,
        /// Where the pointer is.
        position: Point,
        /// The modifiers held.
        mods: Modifiers,
        /// The clicks in a row that this one makes, as the system's double
        /// click interval counts them: 2 for a double click.
        clicks: u32,
    },
    /// The pointer moved over the content, with buttons held or not.
    MouseMoved {
        /// Where it is.
        position: Point,
        /// The modifiers held.
        mods: Modifiers,
    },
    /// A wheel or a trackpad scrolled.
    Scroll {
        /// How far.
        delta: ScrollDelta,
        /// Where a trackpad's gesture is; `None` for a wheel.
        phase: Option<ScrollPhase>,
        /// Whether the scroll is the momentum that goes on after the
        /// fingers lifted.
        momentum: bool,
        /// Where the pointer is.
        position: Point,
        /// The modifiers held.
        mods: Modifiers,
    },
}

/// Something that happened to the application.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppEvent {
    /// Another thread woke the main thread, through a
    /// [`Waker`](crate::Waker).
    Wake,
    /// An item of the main menu was chosen: its command.
    Command(String),
    /// An alert closed, with the index of the button that closed it.
    AlertClosed {
        /// The alert.
        alert: AlertId,
        /// Its button, by its index in [`Alert::buttons`](crate::Alert::buttons).
        button: usize,
        /// Whether its checkbox, [`Alert::suppression`](crate::Alert::suppression),
        /// was ticked; `false` without one.
        suppressed: bool,
    },
    /// The system's appearance changed.
    AppearanceChanged(Appearance),
    /// The system asked the application to quit, as when the user logs out
    /// or chooses Quit in the Dock; the system waits until the app calls
    /// [`Platform::quit`](crate::Platform::quit), or declines with
    /// [`Platform::refuse_quit`](crate::Platform::refuse_quit).
    QuitRequested,
    /// A timer that the app started fired.
    Timer(TimerId),
    /// The user clicked a notification that the app showed for a window,
    /// with [`Platform::notify`](crate::Platform::notify).
    NotificationClicked(WindowId),
}

/// The system's appearance.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Appearance {
    /// Light.
    #[default]
    Light,
    /// Dark.
    Dark,
}
