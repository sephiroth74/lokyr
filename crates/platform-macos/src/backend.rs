//! The [`Platform`] that the app's handler talks to.

use std::cell::Cell;
use std::collections::{BTreeMap, HashMap};
use std::rc::{Rc, Weak};
use std::time::Instant;

use lokyr_platform_core::{
    Alert, AlertId, Appearance, Backdrop, Error, MainMenu, Platform, Point, Size, TimerId, Waker,
    WindowAttributes, WindowEvent, WindowHandle, WindowId, WindowSize,
};
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSAlert, NSApplication, NSEvent, NSEventModifierFlags, NSEventType, NSPasteboard,
    NSPasteboardTypeString,
};
use objc2_foundation::{NSPoint, NSString, NSTimer};
use raw_window_handle::{AppKitWindowHandle, RawWindowHandle};

use crate::menu::MenuTarget;
use crate::shared::{Pending, Shared};
use crate::window::Window;
use crate::{alert, appearance, menu, timer};

/// An alert that is open.
struct OpenAlert {
    alert: Retained<NSAlert>,
    window: Option<WindowId>,
    buttons: usize,
}

/// The platform of a running application.
pub(crate) struct Backend {
    mtm: MainThreadMarker,
    shared: Weak<Shared>,
    waker: Waker,
    windows: BTreeMap<WindowId, Window>,
    alerts: HashMap<AlertId, OpenAlert>,
    /// The timers that have not fired, or whose firing waits its turn.
    timers: HashMap<TimerId, Retained<NSTimer>>,
    /// Which Option keys act as Meta, which every window's view reads.
    option_as_meta: Rc<Cell<(bool, bool)>>,
    /// The target of the main menu's items, which the menu does not retain.
    menu_target: Option<Retained<MenuTarget>>,
    last_id: u64,
    quit: bool,
    stopped: bool,
}

impl Backend {
    pub(crate) fn new(mtm: MainThreadMarker, shared: Weak<Shared>, waker: Waker) -> Self {
        Self {
            mtm,
            shared,
            waker,
            windows: BTreeMap::new(),
            alerts: HashMap::new(),
            timers: HashMap::new(),
            option_as_meta: Rc::new(Cell::new((false, false))),
            menu_target: None,
            last_id: 0,
            quit: false,
            stopped: false,
        }
    }

    pub(crate) fn has_window(&self, window: WindowId) -> bool {
        self.windows.contains_key(&window)
    }

    /// The alert `alert`, if it is open.
    pub(crate) fn alert(&self, alert: AlertId) -> Option<Retained<NSAlert>> {
        self.alerts.get(&alert).map(|open| open.alert.clone())
    }

    /// Forgets the alert `alert`, which closed, and returns how many
    /// buttons it had; `None` if it was not open.
    pub(crate) fn take_alert(&mut self, alert: AlertId) -> Option<usize> {
        self.alerts.remove(&alert).map(|open| open.buttons)
    }

    /// Forgets the timer `timer`, which fired, and returns whether it was
    /// running: one that the app cancelled is not.
    pub(crate) fn take_timer(&mut self, timer: TimerId) -> bool {
        self.timers.remove(&timer).is_some()
    }

    /// Ends the application now, without waiting for the app: its handler
    /// panicked.
    pub(crate) fn quit_now(&mut self) {
        self.quit = true;
    }

    /// Whether the app has quit.
    pub(crate) fn has_quit(&self) -> bool {
        self.quit
    }

    /// Tells the window's pacer that its frame came.
    pub(crate) fn frame_delivered(&self, window: WindowId) {
        if let Some(window) = self.windows.get(&window) {
            window.frame_delivered();
        }
    }

    /// Ends the loop of the alert without a window that runs.
    pub(crate) fn abort_modal(&self) {
        NSApplication::sharedApplication(self.mtm).abortModal();
    }

    pub(crate) fn is_stopped(&self) -> bool {
        self.stopped
    }

    /// Stops the application's loop, which makes `run` return.
    pub(crate) fn stop(&mut self) {
        self.stopped = true;
        let app = NSApplication::sharedApplication(self.mtm);
        app.stop(None);
        // The loop stops once it has handled an event: this one.
        let event = NSEvent::otherEventWithType_location_modifierFlags_timestamp_windowNumber_context_subtype_data1_data2(
            NSEventType::ApplicationDefined,
            NSPoint::ZERO,
            NSEventModifierFlags::empty(),
            0.0,
            0,
            None,
            0,
            0,
            0,
        );
        if let Some(event) = event {
            app.postEvent_atStart(&event, true);
        }
    }

    /// Takes every window and alert, once the application has ended, for
    /// the caller to close the windows, and stops the timers.
    pub(crate) fn take_windows(&mut self) -> Vec<Window> {
        self.alerts.clear();
        for (_, timer) in self.timers.drain() {
            timer.invalidate();
        }
        std::mem::take(&mut self.windows).into_values().collect()
    }

    fn next_id(&mut self) -> u64 {
        self.last_id += 1;
        self.last_id
    }
}

impl Platform for Backend {
    fn waker(&self) -> Waker {
        self.waker.clone()
    }

    fn quit(&mut self) {
        self.quit = true;
    }

    fn appearance(&self) -> Appearance {
        appearance::current(self.mtm)
    }

    fn create_window(&mut self, attributes: &WindowAttributes) -> Result<WindowId, Error> {
        let id = WindowId::new(self.next_id());
        let window = Window::new(
            self.mtm,
            self.shared.clone(),
            id,
            attributes,
            Rc::clone(&self.option_as_meta),
        );
        // The window is in the map before it takes the focus, whose event
        // follows this call.
        self.windows.insert(id, window);
        self.windows[&id].show();
        Ok(id)
    }

    fn close_window(&mut self, window: WindowId) {
        let Some(closing) = self.windows.remove(&window) else {
            return;
        };
        // An alert on the window closes with it, unanswered.
        self.alerts.retain(|_, open| open.window != Some(window));
        closing.close();
    }

    fn set_title(&mut self, window: WindowId, title: &str) {
        if let Some(window) = self.windows.get(&window) {
            window.set_title(title);
        }
    }

    fn window_size(&self, window: WindowId) -> Option<WindowSize> {
        self.windows.get(&window).map(Window::size)
    }

    fn window_handle(&self, window: WindowId) -> Option<WindowHandle<'_>> {
        let view = self.windows.get(&window)?.view_address();
        let raw = RawWindowHandle::AppKit(AppKitWindowHandle::new(view));
        // SAFETY: the view is the window's content view, which lives as long
        // as the window, and no window closes while the backend is borrowed.
        Some(unsafe { WindowHandle::borrow_raw(raw) })
    }

    fn request_frame(&mut self, window: WindowId) {
        let Some(open) = self.windows.get(&window) else {
            return;
        };
        if open.request_frame()
            && let Some(shared) = self.shared.upgrade()
        {
            shared.enqueue(Pending::Window(window, WindowEvent::Frame));
        }
    }

    fn focus_window(&mut self, window: WindowId) {
        if let Some(window) = self.windows.get(&window) {
            NSApplication::sharedApplication(self.mtm).activate();
            window.show();
        }
    }

    fn focused_window(&self) -> Option<WindowId> {
        self.windows
            .iter()
            .find(|(_, window)| window.is_focused())
            .map(|(&id, _)| id)
    }

    fn set_main_menu(&mut self, menu: &MainMenu) {
        let (ns_menu, target) = menu::build(self.mtm, self.shared.clone(), menu);
        NSApplication::sharedApplication(self.mtm).setMainMenu(Some(&ns_menu));
        self.menu_target = Some(target);
    }

    fn clipboard_text(&self) -> Option<String> {
        // SAFETY: the type is AppKit's constant.
        let kind = unsafe { NSPasteboardTypeString };
        NSPasteboard::generalPasteboard()
            .stringForType(kind)
            .map(|text| text.to_string())
    }

    fn set_clipboard_text(&mut self, text: &str) {
        // SAFETY: the type is AppKit's constant.
        let kind = unsafe { NSPasteboardTypeString };
        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.clearContents();
        pasteboard.setString_forType(&NSString::from_str(text), kind);
    }

    fn show_alert(&mut self, window: Option<WindowId>, alert: &Alert) -> AlertId {
        let id = AlertId::new(self.next_id());
        let ns_alert = alert::new(self.mtm, alert);
        let window = window.filter(|window| self.windows.contains_key(window));
        match window {
            Some(window) => {
                alert::begin_sheet(
                    &ns_alert,
                    self.windows[&window].ns_window(),
                    self.shared.clone(),
                    id,
                );
            }
            // Without a window, the alert runs its own loop, once the call
            // in progress has returned.
            None => {
                if let Some(shared) = self.shared.upgrade() {
                    shared.enqueue(Pending::Modal(id));
                }
            }
        }
        self.alerts.insert(
            id,
            OpenAlert {
                alert: ns_alert,
                window,
                buttons: alert.buttons.len(),
            },
        );
        id
    }

    fn start_timer(&mut self, at: Instant) -> TimerId {
        let id = TimerId::new(self.next_id());
        let ns_timer = timer::start(self.shared.clone(), id, at);
        self.timers.insert(id, ns_timer);
        id
    }

    fn cancel_timer(&mut self, timer: TimerId) {
        if let Some(ns_timer) = self.timers.remove(&timer) {
            ns_timer.invalidate();
        }
    }

    fn set_text_area(&mut self, window: WindowId, origin: Point, size: Size) {
        if let Some(window) = self.windows.get(&window) {
            window.set_text_area(origin, size);
        }
    }

    fn set_option_as_meta(&mut self, left: bool, right: bool) {
        self.option_as_meta.set((left, right));
    }

    fn set_translucent(&mut self, window: WindowId, backdrop: Option<Backdrop>) {
        if let Some(window) = self.windows.get(&window) {
            window.set_translucent(backdrop);
        }
    }
}
