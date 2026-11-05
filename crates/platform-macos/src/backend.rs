//! The [`Platform`] that the app's handler talks to.

use std::cell::Cell;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant, SystemTime};

use lokyr_platform_core::{
    AboutPanel, Alert, AlertId, Appearance, Backdrop, ContentRect, ContextMenu, Error, GlobalKey,
    Layer, LayerPlace, MainMenu, MenuRole, Platform, Point, PointerArea, PointerShape, QuickPlace,
    Size, Sweep, Target, TimerId, Titlebar, Usage, Waker, WindowAttributes, WindowEvent,
    WindowHandle, WindowId, WindowSize,
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

use crate::global_keys::GlobalKeys;
use crate::menu::MenuTarget;
use crate::notify::Notifier;
use crate::secure::SecureInput;
use crate::shared::{Pending, Shared};
use crate::updates::Updates;
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
    /// The secure entry of the keyboard, which turns off as the backend
    /// goes.
    secure: SecureInput,
    /// The keys that the app takes from every app, which the system gets
    /// back as the backend goes.
    global_keys: GlobalKeys,
    /// The app's updates, where its bundle has the updater.
    updates: Updates,
}

impl Backend {
    pub(crate) fn new(mtm: MainThreadMarker, shared: Weak<Shared>, waker: Waker) -> Self {
        Self {
            mtm,
            shared: shared.clone(),
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
            secure: SecureInput::default(),
            global_keys: GlobalKeys::new(shared),
            updates: Updates::default(),
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
    ) -> Option<(Retained<NSMenu>, Retained<NSView>, NSPoint, WindowId)> {
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
        Some((waiting.menu.clone(), view, location, waiting.window))
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

    /// Forgets the context menu that waits to open, if one does, whose
    /// closing its window then hears, as it never opened; one that is
    /// open stays, whose loop tells its own closing.
    fn drop_waiting_context_menu(&mut self) {
        if let Some(waiting) = self.context_menu.take_if(|menu| !menu.open)
            && let Some(shared) = self.shared.upgrade()
        {
            shared.enqueue(Pending::Window(
                waiting.window,
                WindowEvent::ContextMenuClosed,
            ));
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

    fn reduce_motion(&self) -> bool {
        objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
    }

    fn keyboard_navigation(&self) -> bool {
        // AppKit reads the preference at each call, cheaply, and tells no
        // change of it.
        NSApplication::sharedApplication(self.mtm).isFullKeyboardAccessEnabled()
    }

    fn set_appearance(&mut self, appearance: Option<Appearance>) {
        appearance::set(self.mtm, appearance);
    }

    fn screen_scale(&self) -> f64 {
        NSScreen::mainScreen(self.mtm).map_or(1.0, |screen| screen.backingScaleFactor())
    }

    fn usage(&self) -> Option<Usage> {
        let pid = libc::pid_t::try_from(std::process::id()).ok()?;
        // SAFETY: the struct is plain data, which zeros make valid.
        let mut info: libc::rusage_info_v4 = unsafe { std::mem::zeroed() };
        // SAFETY: `proc_pid_rusage` fills in the struct of the flavor that
        // it is asked for, `rusage_info_v4`, which `info` is.
        let status =
            unsafe { libc::proc_pid_rusage(pid, libc::RUSAGE_INFO_V4, (&raw mut info).cast()) };
        (status == 0).then_some(Usage {
            idle_wakeups: info.ri_pkg_idle_wkups,
            interrupt_wakeups: info.ri_interrupt_wkups,
            footprint: info.ri_phys_footprint,
        })
    }

    fn process_started(&self) -> Option<SystemTime> {
        let pid = libc::pid_t::try_from(std::process::id()).ok()?;
        // SAFETY: the struct is plain data, which zeros make valid.
        let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
        let size = i32::try_from(size_of::<libc::proc_bsdinfo>()).ok()?;
        // SAFETY: `proc_pidinfo` fills in at most `size` bytes of the
        // flavor's struct, `proc_bsdinfo`, which `info` is.
        let written = unsafe {
            libc::proc_pidinfo(pid, libc::PROC_PIDTBSDINFO, 0, (&raw mut info).cast(), size)
        };
        (written == size).then(|| {
            SystemTime::UNIX_EPOCH
                + Duration::from_secs(info.pbi_start_tvsec)
                + Duration::from_micros(info.pbi_start_tvusec)
        })
    }

    fn post_key(&mut self, window: WindowId, code: lokyr_platform_core::KeyCode, text: &str) {
        let Some(window) = self.windows.get(&window) else {
            return;
        };
        let Some(code) = crate::keys::virtual_code(code) else {
            return;
        };
        let app = NSApplication::sharedApplication(self.mtm);
        let number = window.ns_window().windowNumber();
        let characters = NSString::from_str(text);
        for kind in [NSEventType::KeyDown, NSEventType::KeyUp] {
            let event = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
                kind,
                NSPoint::ZERO,
                NSEventModifierFlags::empty(),
                objc2_quartz_core::CACurrentMediaTime(),
                number,
                None,
                &characters,
                &characters,
                false,
                code,
            );
            if let Some(event) = event {
                app.postEvent_atStart(&event, false);
            }
        }
        // The main run loop takes the keys at once, as a key of the
        // keyboard's wakes it, where a posted event waits for its next wake.
        if let Some(main) = objc2_core_foundation::CFRunLoop::main() {
            main.wake_up();
        }
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
        // Down and to the right of the window with the focus, else of the
        // front one, as that of a sheet, which has the focus, or any while
        // the app is not active, as the system cascades windows; centred
        // without one.
        // A quick window is none of them.
        let ordered = NSApplication::sharedApplication(self.mtm).orderedWindows();
        let front = (0..ordered.count()).find_map(|index| {
            let ns_window = ordered.objectAtIndex(index);
            self.windows
                .values()
                .find(|window| !window.is_quick() && std::ptr::eq(window.ns_window(), &*ns_window))
        });
        let after = self
            .windows
            .values()
            .find(|window| !window.is_quick() && window.is_focused())
            .or(front)
            .map(|window| window.ns_window().frame());
        if !window.is_restored() {
            window.place(after);
        }
        // Once placed, so that its own placing keeps nothing.
        window.set_remember(attributes.remember);
        // The window is in the map before it takes the focus, whose event
        // follows this call.
        self.windows.insert(id, window);
        self.windows[&id].show();
        Ok(id)
    }

    fn create_quick_window(&mut self, title: &str) -> Result<WindowId, Error> {
        let id = WindowId::new(self.next_id());
        let window = Window::new_quick(
            self.mtm,
            self.shared.clone(),
            id,
            title,
            Rc::clone(&self.option_as_meta),
        );
        self.windows.insert(id, window);
        Ok(id)
    }

    fn show_quick_window(&mut self, window: WindowId, place: Option<&QuickPlace>) {
        if let Some(window) = self.windows.get(&window) {
            window.show_quick(place);
        }
    }

    fn set_global_keys(&mut self, keys: &[GlobalKey]) -> Vec<usize> {
        self.global_keys.set(keys)
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

    fn set_window_remember(&mut self, window: WindowId, remember: bool) {
        if let Some(window) = self.windows.get(&window) {
            window.set_remember(remember);
        }
    }

    fn window_size(&self, window: WindowId) -> Option<WindowSize> {
        self.windows.get(&window).map(Window::size)
    }

    fn refresh_interval(&self, window: WindowId) -> Option<Duration> {
        self.windows.get(&window).map(Window::refresh_interval)
    }

    fn window_handle(&self, window: WindowId) -> Option<WindowHandle<'_>> {
        let view = self.windows.get(&window)?.view_address();
        let raw = RawWindowHandle::AppKit(AppKitWindowHandle::new(view));
        // SAFETY: the view is the window's content view, which lives as long
        // as the window, and no window closes while the backend is borrowed.
        Some(unsafe { WindowHandle::borrow_raw(raw) })
    }

    fn layer_handle(&self, window: WindowId, layer: Layer) -> Option<WindowHandle<'_>> {
        let view = self.windows.get(&window)?.layer_address(layer);
        let raw = RawWindowHandle::AppKit(AppKitWindowHandle::new(view));
        // SAFETY: the view is the window's layer's, which lives as long as
        // the window, and no window closes while the backend is borrowed.
        Some(unsafe { WindowHandle::borrow_raw(raw) })
    }

    fn place_layer(&mut self, window: WindowId, layer: Layer, place: Option<&LayerPlace>) {
        if let Some(window) = self.windows.get(&window) {
            window.place_layer(layer, place);
        }
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
        // A hidden quick window would take the keys off the screen.
        if let Some(window) = self.windows.get(&window)
            && !window.is_hidden_quick()
        {
            NSApplication::sharedApplication(self.mtm).activate();
            window.show();
        }
    }

    fn pointer_position(&self, window: WindowId) -> Option<Point> {
        self.windows.get(&window).map(Window::pointer)
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
        self.drop_waiting_context_menu();
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

    fn close_context_menu(&mut self, window: WindowId) {
        if self
            .context_menu
            .as_ref()
            .is_some_and(|menu| menu.window == window)
        {
            self.drop_waiting_context_menu();
            self.cancel_context_menu();
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

    fn set_sweeps(&mut self, window: WindowId, sweeps: &[Sweep]) {
        if let Some(window) = self.windows.get(&window) {
            window.set_sweeps(sweeps);
        }
    }

    fn end_composition(&mut self, window: WindowId) {
        if let Some(window) = self.windows.get(&window) {
            window.end_composition();
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

    fn set_secure_input(&mut self, on: bool) {
        self.secure.set(on);
    }

    fn start_updates(&mut self, check: bool) -> bool {
        self.updates.start(check)
    }

    fn set_update_checks(&mut self, check: bool) {
        self.updates.set_checks(check);
    }

    fn check_for_updates(&mut self) {
        self.updates.check();
    }

    fn show_about(&mut self, about: &AboutPanel) {
        crate::about::show(self.mtm, about);
    }

    fn set_pointer_areas(&mut self, window: WindowId, areas: &[PointerArea]) {
        if let Some(window) = self.windows.get(&window) {
            window.set_pointer_areas(areas);
        }
    }

    fn set_titlebar(&mut self, window: WindowId, titlebar: Titlebar) {
        if let Some(window) = self.windows.get(&window) {
            window.set_titlebar(titlebar);
        }
    }

    fn set_move_areas(&mut self, window: WindowId, areas: &[ContentRect]) {
        if let Some(window) = self.windows.get(&window) {
            window.set_move_areas(areas);
        }
    }

    fn set_titlebar_areas(&mut self, window: WindowId, areas: &[ContentRect]) {
        if let Some(window) = self.windows.get(&window) {
            window.set_titlebar_areas(areas);
        }
    }

    fn accessibility_asked(&self, window: WindowId) -> bool {
        self.windows
            .get(&window)
            .is_some_and(Window::accessibility_asked)
    }

    fn update_accessibility(&mut self, window: WindowId, update: accesskit::TreeUpdate) {
        if let Some(window) = self.windows.get(&window) {
            window.update_accessibility(update);
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

    fn resolve_target(&self, target: &str) -> Result<Target, Error> {
        open::resolve(target)
    }

    fn open(&mut self, target: &Target) -> Result<(), Error> {
        open::open(target)
    }

    fn system_version(&self) -> String {
        let version = sysctl_string(c"kern.osproductversion").unwrap_or_else(|| "?".to_owned());
        match sysctl_string(c"kern.osversion") {
            Some(build) => format!("macOS {version} ({build})"),
            None => format!("macOS {version}"),
        }
    }

    fn reveal(&mut self, target: &Target) -> Result<(), Error> {
        open::reveal(target)
    }

    fn open_in_text_editor(&mut self, path: &Path) -> Result<(), Error> {
        open::in_text_editor(path, self.waker.clone())
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

/// The string of the system's control `name`, as `sysctl` tells it.
fn sysctl_string(name: &std::ffi::CStr) -> Option<String> {
    let mut size = 0;
    // SAFETY: a null buffer asks for the value's size alone.
    if unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    } != 0
    {
        return None;
    }
    let mut buffer = vec![0_u8; size];
    // SAFETY: the buffer holds `size` bytes, which `sysctlbyname` fills.
    if unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            buffer.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    } != 0
    {
        return None;
    }
    buffer.truncate(size);
    let text = std::ffi::CStr::from_bytes_until_nul(&buffer).ok()?;
    Some(text.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_s_version_is_macos_s_with_its_build() {
        let version = sysctl_string(c"kern.osproductversion").unwrap();
        assert!(
            version.split('.').all(|part| part.parse::<u32>().is_ok()),
            "{version}"
        );
        assert!(sysctl_string(c"kern.osversion").is_some_and(|build| !build.is_empty()));
        assert_eq!(sysctl_string(c"kern.no-such-control"), None);
    }
}
