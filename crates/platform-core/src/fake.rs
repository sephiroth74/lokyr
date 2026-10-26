//! A platform without a window system, for tests. It keeps what the app
//! asks of it, and hands the app the events that a test sends, once it has
//! applied what they change, as a backend would.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::{
    Alert, AlertId, AppEvent, Appearance, Error, Handler, MainMenu, Platform, Waker,
    WindowAttributes, WindowEvent, WindowId, WindowSize,
};

/// The scale of the fake's display when a test sets none: a Retina one.
pub const DEFAULT_SCALE: f64 = 2.0;

/// A window that the app opened, as the fake keeps it.
#[derive(Clone, Debug, PartialEq)]
pub struct FakeWindow {
    /// Its title.
    pub title: String,
    /// The size of its content.
    pub size: WindowSize,
    /// Whether it is hidden or covered.
    pub occluded: bool,
}

/// An alert that is open, until a test answers it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FakeAlert {
    /// The alert.
    pub id: AlertId,
    /// The window that it is on, if it is on one.
    pub window: Option<WindowId>,
    /// What it shows.
    pub alert: Alert,
}

/// The fake platform. Its focus moves only with the
/// [`WindowEvent::Focused`] events that a test sends, as a backend tells
/// the focus through them.
#[derive(Debug)]
pub struct Fake {
    scale: f64,
    appearance: Appearance,
    windows: BTreeMap<WindowId, FakeWindow>,
    focused: Option<WindowId>,
    menu: Option<MainMenu>,
    clipboard: Option<String>,
    alerts: Vec<FakeAlert>,
    last_id: u64,
    woken: Arc<AtomicBool>,
    quit: bool,
}

impl Default for Fake {
    fn default() -> Self {
        Self::new(DEFAULT_SCALE)
    }
}

impl Fake {
    /// A fake whose display has the scale `scale`, in the light
    /// appearance, with nothing open.
    pub fn new(scale: f64) -> Self {
        Self {
            scale,
            appearance: Appearance::Light,
            windows: BTreeMap::new(),
            focused: None,
            menu: None,
            clipboard: None,
            alerts: Vec::new(),
            last_id: 0,
            woken: Arc::new(AtomicBool::new(false)),
            quit: false,
        }
    }

    /// Starts the application, as a backend's `run` does.
    pub fn start(&mut self, handler: &mut dyn Handler) {
        handler.started(self);
    }

    /// Hands `handler` an event of `window`, once the fake has applied
    /// what it changes: the size, the focus or the occlusion. Returns
    /// whether it handed it: not to a window that is not open, nor once the
    /// app has quit.
    pub fn send(
        &mut self,
        handler: &mut dyn Handler,
        window: WindowId,
        event: WindowEvent,
    ) -> bool {
        if self.quit {
            return false;
        }
        let Some(state) = self.windows.get_mut(&window) else {
            return false;
        };
        match &event {
            WindowEvent::Resized(size) | WindowEvent::ScaleChanged(size) => state.size = *size,
            WindowEvent::Occluded(occluded) => state.occluded = *occluded,
            WindowEvent::Focused(true) => self.focused = Some(window),
            WindowEvent::Focused(false) if self.focused == Some(window) => self.focused = None,
            _ => {}
        }
        handler.window_event(self, window, event);
        true
    }

    /// Hands `handler` an event of the application, once the fake has
    /// applied the appearance that it brings. Returns whether it handed it:
    /// not once the app has quit.
    pub fn send_app(&mut self, handler: &mut dyn Handler, event: AppEvent) -> bool {
        if self.quit {
            return false;
        }
        if let AppEvent::AppearanceChanged(appearance) = event {
            self.appearance = appearance;
        }
        handler.app_event(self, event);
        true
    }

    /// Hands `handler` an [`AppEvent::Wake`] if a [`Waker`] of the fake woke
    /// it since the last one, as the main thread's loop does. Returns
    /// whether it did.
    pub fn deliver_wake(&mut self, handler: &mut dyn Handler) -> bool {
        if self.quit || !self.woken.swap(false, Ordering::AcqRel) {
            return false;
        }
        handler.app_event(self, AppEvent::Wake);
        true
    }

    /// Closes the open alert `alert` with its button `button`, as the user
    /// would, and hands `handler` the event. Returns whether the alert was
    /// open.
    pub fn answer_alert(
        &mut self,
        handler: &mut dyn Handler,
        alert: AlertId,
        button: usize,
    ) -> bool {
        let Some(index) = self.alerts.iter().position(|open| open.id == alert) else {
            return false;
        };
        self.alerts.remove(index);
        self.send_app(handler, AppEvent::AlertClosed { alert, button })
    }

    /// A window that is open.
    pub fn window(&self, window: WindowId) -> Option<&FakeWindow> {
        self.windows.get(&window)
    }

    /// The windows that are open, from the first opened.
    pub fn windows(&self) -> impl Iterator<Item = WindowId> + '_ {
        self.windows.keys().copied()
    }

    /// The main menu that the app set, if it set one.
    pub fn main_menu(&self) -> Option<&MainMenu> {
        self.menu.as_ref()
    }

    /// The alerts that are open, from the first opened.
    pub fn alerts(&self) -> &[FakeAlert] {
        &self.alerts
    }

    /// Whether the app has quit.
    pub fn has_quit(&self) -> bool {
        self.quit
    }

    fn next_id(&mut self) -> u64 {
        self.last_id += 1;
        self.last_id
    }
}

impl Platform for Fake {
    fn waker(&self) -> Waker {
        let woken = Arc::clone(&self.woken);
        Waker::new(move || woken.store(true, Ordering::Release))
    }

    fn quit(&mut self) {
        self.quit = true;
    }

    fn appearance(&self) -> Appearance {
        self.appearance
    }

    fn create_window(&mut self, attributes: &WindowAttributes) -> Result<WindowId, Error> {
        let id = WindowId::new(self.next_id());
        self.windows.insert(
            id,
            FakeWindow {
                title: attributes.title.clone(),
                size: WindowSize::new(attributes.size, self.scale),
                occluded: false,
            },
        );
        Ok(id)
    }

    fn close_window(&mut self, window: WindowId) {
        self.windows.remove(&window);
        if self.focused == Some(window) {
            self.focused = None;
        }
        // An alert on a window closes with it, unanswered.
        self.alerts.retain(|open| open.window != Some(window));
    }

    fn set_title(&mut self, window: WindowId, title: &str) {
        if let Some(state) = self.windows.get_mut(&window) {
            title.clone_into(&mut state.title);
        }
    }

    fn window_size(&self, window: WindowId) -> Option<WindowSize> {
        self.windows.get(&window).map(|state| state.size)
    }

    fn focus_window(&mut self, _window: WindowId) {}

    fn focused_window(&self) -> Option<WindowId> {
        self.focused
    }

    fn set_main_menu(&mut self, menu: &MainMenu) {
        self.menu = Some(menu.clone());
    }

    fn clipboard_text(&self) -> Option<String> {
        self.clipboard.clone()
    }

    fn set_clipboard_text(&mut self, text: &str) {
        self.clipboard = Some(text.to_owned());
    }

    fn show_alert(&mut self, window: Option<WindowId>, alert: &Alert) -> AlertId {
        let id = AlertId::new(self.next_id());
        self.alerts.push(FakeAlert {
            id,
            window,
            alert: alert.clone(),
        });
        id
    }
}
