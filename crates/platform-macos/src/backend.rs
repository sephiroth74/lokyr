//! The [`Platform`] that the app's handler talks to.

use std::cell::Cell;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::rc::{Rc, Weak};
use std::time::Instant;

use lokyr_platform_core::{
    Alert, AlertId, Appearance, Backdrop, ContextMenu, Error, MainMenu, MenuRole, Platform, Point,
    PointerShape, Size, TargetKind, TimerId, Waker, WindowAttributes, WindowEvent, WindowHandle,
    WindowId, WindowSize,
};
use objc2::rc::Retained;
use objc2::{AnyThread, MainThreadMarker};
use objc2_app_kit::{
    NSAlert, NSAppearanceCustomization, NSApplication, NSBeep, NSEvent, NSEventModifierFlags,
    NSEventType, NSImage, NSMenu, NSPasteboard, NSPasteboardTypeString, NSRequestUserAttentionType,
    NSScreen, NSView,
};
use objc2_foundation::{NSLocale, NSPoint, NSString, NSTimer};
use raw_window_handle::{AppKitWindowHandle, RawWindowHandle};

use crate::menu::MenuTarget;
use crate::notify::Notifier;
use crate::shared::{Pending, Shared};
use crate::window::Window;
use crate::{alert, appearance, definition, menu, open, timer};

/// An alert that is open.
struct OpenAlert {
    alert: Retained<NSAlert>,
    window: Option<WindowId>,
    buttons: usize,
}

/// A context menu that waits for its turn to open, or is open.
struct OpenMenu {
    window: WindowId,
    position: Point,
    menu: Retained<NSMenu>,
    /// The target of its items, which they do not retain.
    _target: Retained<MenuTarget>,
    /// Whether it is open, rather than waiting.
    open: bool,
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
    /// The commands whose menu items are dimmed, and those whose items are
    /// ticked, which each menu's target reads.
    disabled: menu::Commands,
    checked: menu::Commands,
    /// The last context menu that the app asked for, until it closes.
    context_menu: Option<OpenMenu>,
    last_id: u64,
    quit: bool,
    stopped: bool,
    /// The notifications, once the app shows one: `None` before, and
    /// outside a bundle.
    notifier: Option<Option<Notifier>>,
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
            disabled: Rc::default(),
            checked: Rc::default(),
            context_menu: None,
            last_id: 0,
            quit: false,
            stopped: false,
            notifier: None,
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
    /// buttons it had, whether its checkbox was ticked, and its field's
    /// text; `None` if it was not open.
    pub(crate) fn take_alert(&mut self, alert: AlertId) -> Option<(usize, bool, Option<String>)> {
        self.alerts.remove(&alert).map(|open| {
            (
                open.buttons,
                alert::suppressed(&open.alert),
                alert::text(&open.alert),
            )
        })
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

    /// `window` shows again: the frame that it asked for while it did not,
    /// if one.
    pub(crate) fn request_held_frame(&self, window: WindowId) {
        if let Some(open) = self.windows.get(&window)
            && open.request_held_frame()
            && let Some(shared) = self.shared.upgrade()
        {
            shared.enqueue(Pending::Window(window, WindowEvent::Frame));
        }
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

    /// Answers the system's request to quit, which AppKit waits for: with
    /// `quit`, AppKit ends the process.
    pub(crate) fn answer_termination(&self, quit: bool) {
        NSApplication::sharedApplication(self.mtm).replyToApplicationShouldTerminate(quit);
    }

    /// The context menu that waits for its turn, now open, with the view
    /// that it opens over and where in the view: `None` when none waits, or
    /// when its window closed meanwhile.
    pub(crate) fn open_context_menu(
        &mut self,
    ) -> Option<(Retained<NSMenu>, Retained<NSView>, NSPoint)> {
        let waiting = self.context_menu.as_mut().filter(|waiting| !waiting.open)?;
        let Some(window) = self.windows.get(&waiting.window) else {
            self.context_menu = None;
            return None;
        };
        waiting.open = true;
        let view = window.view();
        // The view's origin is its bottom left corner.
        let location = NSPoint::new(
            waiting.position.x,
            view.bounds().size.height - waiting.position.y,
        );
        Some((waiting.menu.clone(), view, location))
    }

    /// Forgets the context menu that was open, which closed; one that the
    /// app asked for meanwhile waits for its turn.
    pub(crate) fn context_menu_closed(&mut self) {
        if self.context_menu.as_ref().is_some_and(|menu| menu.open) {
            self.context_menu = None;
        }
    }

    /// Closes the context menu that is open, if one is, and forgets it.
    fn cancel_context_menu(&mut self) {
        if let Some(menu) = self.context_menu.take().filter(|menu| menu.open) {
            menu.menu.cancelTrackingWithoutAnimation();
        }
    }

    /// Stops the application's loop, which makes `run` return.
    pub(crate) fn stop(&mut self) {
        self.stopped = true;
        // The loop goes on once the menu's own loop has returned.
        self.cancel_context_menu();
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

    fn refuse_quit(&mut self) {
        if self
            .shared
            .upgrade()
            .is_some_and(|shared| shared.end_terminating())
        {
            self.answer_termination(false);
        }
    }

    fn appearance(&self) -> Appearance {
        appearance::current(self.mtm)
    }

    fn set_appearance(&mut self, appearance: Option<Appearance>) {
        appearance::set(self.mtm, appearance);
    }

    fn screen_scale(&self) -> f64 {
        NSScreen::mainScreen(self.mtm).map_or(1.0, |screen| screen.backingScaleFactor())
    }

    fn locales(&self) -> Vec<String> {
        let current = NSLocale::currentLocale();
        // A language without a region takes the system's.
        let region = current.regionCode();
        let named = |locale: &NSLocale| {
            let region = locale.regionCode().or_else(|| region.clone())?;
            Some(format!("{}_{region}", locale.languageCode()))
        };
        let preferred = NSLocale::preferredLanguages();
        let others = (0..preferred.count())
            .map(|at| NSLocale::localeWithLocaleIdentifier(&preferred.objectAtIndex(at)));
        let mut locales: Vec<String> = Vec::new();
        for locale in std::iter::once(current.clone()).chain(others) {
            if let Some(name) = named(&locale)
                && !locales.contains(&name)
            {
                locales.push(name);
            }
        }
        locales
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
        // Down and to the right of the window with the focus, as the
        // system cascades windows; centred without one.
        let focused = self
            .windows
            .values()
            .find(|window| window.is_focused())
            .map(|window| window.ns_window().frame());
        if !window.is_restored() {
            window.place(focused);
        }
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
        // An alert on the window closes with it, unanswered, and so does
        // its context menu.
        self.alerts.retain(|_, open| open.window != Some(window));
        if self
            .context_menu
            .as_ref()
            .is_some_and(|menu| menu.window == window)
        {
            self.cancel_context_menu();
        }
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
        let built = menu::build(
            self.mtm,
            self.shared.clone(),
            menu,
            (self.disabled.clone(), self.checked.clone()),
        );
        let app = NSApplication::sharedApplication(self.mtm);
        app.setMainMenu(Some(&built.bar));
        for (role, menu) in built.roles {
            match role {
                MenuRole::Window => app.setWindowsMenu(Some(&menu)),
                MenuRole::Help => app.setHelpMenu(Some(&menu)),
            }
        }
        self.menu_target = Some(built.target);
    }

    fn set_command_enabled(&mut self, command: &str, enabled: bool) {
        let mut disabled = self.disabled.borrow_mut();
        if enabled {
            disabled.remove(command);
        } else {
            disabled.insert(command.to_owned());
        }
    }

    fn set_command_checked(&mut self, command: &str, checked: bool) {
        let mut ticked = self.checked.borrow_mut();
        if checked {
            ticked.insert(command.to_owned());
        } else {
            ticked.remove(command);
        }
    }

    fn show_definition(
        &mut self,
        window: WindowId,
        text: &str,
        baseline: Point,
        font: Option<(&str, f64)>,
    ) {
        if let Some(window) = self.windows.get(&window) {
            definition::show(self.mtm, &window.view(), text, baseline, font);
        }
    }

    fn show_context_menu(&mut self, window: WindowId, position: Point, menu: &ContextMenu) {
        if !self.windows.contains_key(&window) {
            return;
        }
        self.cancel_context_menu();
        let (menu, target) = menu::context(self.mtm, self.shared.clone(), window, menu);
        self.context_menu = Some(OpenMenu {
            window,
            position,
            menu,
            _target: target,
            open: false,
        });
        // The menu runs its own loop, once the call in progress has
        // returned.
        if let Some(shared) = self.shared.upgrade() {
            shared.enqueue(Pending::ContextMenu);
        }
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

    fn track_mouse_moves(&mut self, window: WindowId, track: bool) {
        if let Some(window) = self.windows.get(&window) {
            window.track_mouse_moves(track);
        }
    }

    fn set_pointer_shape(&mut self, window: WindowId, shape: PointerShape) {
        if let Some(window) = self.windows.get(&window) {
            window.set_pointer_shape(shape);
        }
    }

    fn request_attention(&mut self) {
        let app = NSApplication::sharedApplication(self.mtm);
        // AppKit asks for this only while the application is not active.
        if !app.isActive() {
            app.requestUserAttention(NSRequestUserAttentionType::InformationalRequest);
        }
    }

    fn play_alert_sound(&mut self) {
        NSBeep();
    }

    fn target_kind(&self, target: &str) -> Result<TargetKind, Error> {
        open::kind(target)
    }

    fn open(&mut self, target: &str) -> Result<(), Error> {
        open::open(target)
    }

    fn reveal(&mut self, target: &str) -> Result<(), Error> {
        open::reveal(target)
    }

    fn notify(&mut self, window: WindowId, title: &str, body: &str) {
        let waker = self.waker.clone();
        if let Some(notifier) = self.notifier.get_or_insert_with(|| Notifier::new(waker)) {
            notifier.notify(window, title, body);
        }
    }

    fn set_app_icon(&mut self, icon: Option<&Path>) -> Result<(), Error> {
        let image = match icon {
            Some(path) => {
                let file = NSString::from_str(&path.to_string_lossy());
                let image = NSImage::initWithContentsOfFile(NSImage::alloc(), &file)
                    .filter(|image| image.isValid())
                    .ok_or_else(|| Error::Icon(path.display().to_string()))?;
                Some(image)
            }
            None => None,
        };
        let app = NSApplication::sharedApplication(self.mtm);
        // SAFETY: AppKit takes an image, or nil, which gives the bundle's
        // own icon back, as its documentation says.
        unsafe { app.setApplicationIconImage(image.as_deref()) };
        Ok(())
    }

    fn set_translucent(&mut self, window: WindowId, backdrop: Option<Backdrop>) {
        if let Some(window) = self.windows.get(&window) {
            window.set_translucent(backdrop);
        }
    }

    fn set_window_appearance(&mut self, window: WindowId, appearance: Option<Appearance>) {
        if let Some(window) = self.windows.get(&window) {
            window
                .ns_window()
                .setAppearance(appearance::named(appearance).as_deref());
        }
    }
}
