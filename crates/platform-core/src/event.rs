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
    /// The user started resizing the window, dragging its edge, or ended:
    /// the steps of a live resize come between.
    LiveResize(bool),
    /// The window moved to a display of another scale, which changes the
    /// size of its content in pixels.
    ScaleChanged(WindowSize),
    /// The window took the keyboard's focus, or lost it.
    Focused(bool),
    /// An accessibility client, as VoiceOver, asked for the window's tree:
    /// the app gives it whole, with `Platform::update_accessibility`, then
    /// its changes; until then, the app builds none.
    AccessibilityAsked,
    /// An accessibility client asked for `ActionRequest`'s action on one of
    /// the tree's nodes, as VoiceOver's press or focus.
    AccessibilityAction(accesskit::ActionRequest),
    /// The window became hidden, minimized or covered, or visible again.
    Occluded(bool),
    /// The user asked to close the window, which stays open until the app
    /// closes it.
    CloseRequested,
    /// An item of the window's context menu was chosen: its command.
    ContextCommand(String),
    /// The window's context menu closed, after its item's
    /// [`WindowEvent::ContextCommand`] where one was chosen, or without a
    /// choice.
    ContextMenuClosed,
    /// Files or text were dropped on the window's content.
    Dropped(Dropped),
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
    /// The pointer left the parts of the content where its moves come,
    /// which [`Platform::set_move_areas`](crate::Platform::set_move_areas)
    /// gave, as it went out of the window or over the rest of it.
    MouseExited,
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
    /// A save panel closed: the path that the user chose, or none as the
    /// user cancelled or its window closed.
    SavePanelClosed {
        /// The panel.
        panel: crate::SavePanelId,
        /// The file's path, absolute.
        path: Option<std::path::PathBuf>,
    },
    /// An alert closed, with the index of the button that closed it.
    AlertClosed {
        /// The alert.
        alert: AlertId,
        /// Its button, by its index in [`Alert::buttons`](crate::Alert::buttons).
        button: usize,
        /// Whether its checkbox, [`Alert::suppression`](crate::Alert::suppression),
        /// was ticked; `false` without one.
        suppressed: bool,
        /// The text of its field, [`Alert::input`](crate::Alert::input), as
        /// it closed; `None` without one.
        text: Option<String>,
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
    /// of the app's number, with [`Platform::notify`](crate::Platform::notify).
    NotificationClicked(WindowId, u64),
    /// A key that the app takes from every app, with
    /// [`Platform::set_global_keys`](crate::Platform::set_global_keys),
    /// was pressed: its index.
    GlobalKey(usize),
    /// The user opened the app again, as from its icon in the Dock, while
    /// it shows no window, nor one minimized.
    Reopen,
    /// The system asked the app to open files, as a double click in the
    /// file manager on a file of a type that the app declares, or a file
    /// dropped on its icon: their paths, in their order.
    OpenFiles(Vec<std::path::PathBuf>),
    /// The system asked the app to open URLs other than files', of the
    /// schemes that the app declares, as `ssh` and `x-man-page` links: as
    /// they were written, in their order.
    OpenUrls(Vec<String>),
    /// The system answered the app's ask to make it the default terminal,
    /// [`Platform::make_default_terminal`](crate::Platform::make_default_terminal):
    /// what did not change, each told, as one that the user declined.
    DefaultTerminalMade {
        /// The changes that failed, each what it was and why.
        failed: Vec<String>,
    },
    /// The system told whether the app is its default terminal, another
    /// answer than the one before, as the platform asked it after the start
    /// or as the app became active, out of their way:
    /// [`Platform::is_default_terminal`](crate::Platform::is_default_terminal)
    /// tells it from then.
    DefaultTerminalKnown,
    /// The links of [`Platform::link_tools`](crate::Platform::link_tools)
    /// were made, or not, as it tells.
    ToolsLinked(crate::LinkOutcome),
    /// The user asked for a window, or a tab, at each of `folders`, as
    /// from the file manager's services on folders that it selected.
    OpenFolders {
        /// The folders, in their order; a file's stands for the folder that
        /// holds it.
        folders: Vec<std::path::PathBuf>,
        /// Whether each opens as a tab of the window in front, rather than
        /// as a window of its own.
        tabs: bool,
    },
    /// An open that the platform started failed, as the system told after
    /// the call that started it returned, as
    /// [`Platform::open_in_text_editor`](crate::Platform::open_in_text_editor)'s.
    OpenFailed {
        /// What it opened: a path, or a URL.
        target: String,
        /// What went wrong, its target named.
        error: String,
    },
}

/// What was dropped on a window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dropped {
    /// Files, by their paths, from the first.
    Files(Vec<std::path::PathBuf>),
    /// Text.
    Text(String),
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
