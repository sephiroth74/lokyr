//! The [`Platform`] that the app's handler talks to.

use std::cell::Cell;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant, SystemTime};

use lokyr_platform_core::{
    AboutPanel, Alert, AlertId, AppEvent, Appearance, Awake, Backdrop, ContentRect, ContextMenu,
    Error, Fill, GlobalKey, Layer, LayerPlace, MainMenu, MenuRole, Platform, Point, PointerArea,
    PointerShape, QuickPlace, SavePanelId, ScreenPoint, ScreenRect, Scrollers, Shortcut, Size,
    Sweep, Target, TimerId, Titlebar, Usage, Waker, WindowAttributes, WindowEvent, WindowHandle,
    WindowId, WindowSize,
};
use objc2::rc::Retained;
use objc2::{AnyThread, MainThreadMarker};
use objc2_app_kit::{
    NSAlert, NSAppearanceCustomization, NSApplication, NSBeep, NSEvent, NSEventModifierFlags,
    NSEventType, NSImage, NSMenu, NSPasteboard, NSPasteboardTypePNG, NSPasteboardTypeString,
    NSRequestUserAttentionType, NSSavePanel, NSScreen, NSView, NSWindow,
};
use objc2_foundation::{NSData, NSLocale, NSPoint, NSString, NSTimer};
use raw_window_handle::{AppKitWindowHandle, RawWindowHandle};

use crate::awake::KeptAwake;
use crate::drag_image::DragImage;
use crate::global_keys::GlobalKeys;
use crate::menu::MenuTarget;
use crate::notify::Notifier;
use crate::save;
use crate::secure::SecureInput;
use crate::shared::{Pending, Shared};
use crate::updates::Updates;
use crate::window::Window;
use crate::{alert, appearance, definition, menu, open, screens, timer};

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
    /// The save panels that are open, each of its window: none of a
    /// window not there, whose end is queued already.
    save_panels: HashMap<SavePanelId, Option<(Retained<NSSavePanel>, WindowId)>>,
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
    /// Whether the system started the app of its own, as the launch told.
    default_launch: bool,
    quit: bool,
    stopped: bool,
    /// The notifications, once the app shows one: `None` before, and
    /// outside a bundle.
    notifier: Option<Option<Notifier>>,
    /// The secure entry of the keyboard, which turns off as the backend
    /// goes.
    secure: SecureInput,
    /// The power assertion that keeps the Mac awake, as the app asks.
    kept_awake: KeptAwake,
    /// The keys that the app takes from every app, which the system gets
    /// back as the backend goes.
    global_keys: GlobalKeys,
    /// The app's updates, where its bundle has the updater.
    updates: Updates,
    /// The drag's image, while it shows.
    drag_image: Option<DragImage>,
}

impl Backend {
    pub(crate) fn new(mtm: MainThreadMarker, shared: Weak<Shared>, waker: Waker) -> Self {
        Self {
            mtm,
            shared: shared.clone(),
            waker,
            windows: BTreeMap::new(),
            alerts: HashMap::new(),
            save_panels: HashMap::new(),
            timers: HashMap::new(),
            option_as_meta: Rc::new(Cell::new((false, false))),
            menu_target: None,
            disabled: Rc::default(),
            checked: Rc::default(),
            context_menu: None,
            last_id: 0,
            default_launch: true,
            quit: false,
            stopped: false,
            notifier: None,
            secure: SecureInput::default(),
            kept_awake: KeptAwake::default(),
            global_keys: GlobalKeys::new(shared),
            updates: Updates::default(),
            drag_image: None,
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

    /// Forgets the save panel `panel`, which closed, and returns whether it
    /// was open: one that the app withdrew is not.
    pub(crate) fn take_save_panel(&mut self, panel: SavePanelId) -> bool {
        self.save_panels.remove(&panel).is_some()
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

    /// Notes whether the system started the app of its own.
    pub(crate) fn set_default_launch(&mut self, default: bool) {
        self.default_launch = default;
    }

    /// Asks whether the app is the default terminal, out of the start's
    /// way, which an event tells once the system answered.
    pub(crate) fn look_at_default_terminal(&self) {
        crate::default_terminal::look(self.waker.clone());
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

    fn scrollers(&self) -> Scrollers {
        // AppKit reads the preferences at each call, cheaply.
        let defaults = objc2_foundation::NSUserDefaults::standardUserDefaults();
        Scrollers {
            overlay: objc2_app_kit::NSScroller::preferredScrollerStyle(self.mtm)
                == objc2_app_kit::NSScrollerStyle::Overlay,
            jump_to_spot: defaults.boolForKey(&objc2_foundation::NSString::from_str(
                "AppleScrollerPagingBehavior",
            )),
        }
    }

    fn default_launch(&self) -> bool {
        self.default_launch
    }

    fn downloaded(&self, path: &Path) -> bool {
        downloaded(path)
    }

    fn quarantine(&self) -> lokyr_platform_core::Quarantine {
        quarantine
    }

    fn is_default_terminal(&self) -> bool {
        crate::default_terminal::known()
    }

    fn make_default_terminal(&mut self) -> bool {
        crate::default_terminal::make_default(self.waker.clone())
    }

    fn link_tools(&mut self, links: Vec<lokyr_platform_core::ToolLink>, reason: &str) {
        crate::tools::link(links, reason.to_owned(), self.waker.clone());
    }

    fn is_temporary_place(&self, path: &std::path::Path) -> bool {
        crate::tools::is_temporary(path)
    }

    fn keeps_windows(&self) -> bool {
        // "Close windows when quitting an application" off, in the global
        // domain, which the standard defaults search; read at each call.
        objc2_foundation::NSUserDefaults::standardUserDefaults().boolForKey(
            &objc2_foundation::NSString::from_str("NSQuitAlwaysKeepsWindows"),
        )
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

    fn window_frame(&self, window: WindowId) -> Option<ScreenRect> {
        let open = self.windows.get(&window)?;
        Some(screens::on_screens(
            open.content_on_screen(),
            screens::top(self.mtm),
        ))
    }

    fn windowed_frame(&self, window: WindowId) -> Option<ScreenRect> {
        let open = self.windows.get(&window)?;
        Some(screens::on_screens(
            open.windowed_frame(),
            screens::top(self.mtm),
        ))
    }

    fn set_window_floating(&mut self, window: WindowId, floating: bool) {
        // A quick window floats of its own place.
        if let Some(open) = self.windows.get(&window).filter(|open| !open.is_quick()) {
            open.set_floating(floating);
        }
    }

    fn set_window_origin(&mut self, window: WindowId, origin: ScreenPoint) {
        if let Some(open) = self.windows.get(&window) {
            open.set_content_origin(origin);
        }
    }

    fn window_at(&self, at: ScreenPoint) -> Option<WindowId> {
        let point = screens::point_in_appkit(at, screens::top(self.mtm));
        // The front window that a press there would reach, of any app's:
        // the drag's image, which the pointer goes through, is none.
        let number = NSWindow::windowNumberAtPoint_belowWindowWithWindowNumber(point, 0, self.mtm);
        self.windows
            .iter()
            .find(|(_, open)| {
                let ns_window = open.ns_window();
                ns_window.windowNumber() == number && ns_window.isVisible()
            })
            .map(|(&id, _)| id)
    }

    fn show_drag_image(&mut self, frame: ScreenRect) {
        let frame = screens::in_appkit(frame, screens::top(self.mtm));
        match &self.drag_image {
            Some(image) => image.place(frame),
            None => self.drag_image = Some(DragImage::new(self.mtm, frame)),
        }
    }

    fn drag_image_handle(&self) -> Option<WindowHandle<'_>> {
        let view = self.drag_image.as_ref()?.address();
        let raw = RawWindowHandle::AppKit(AppKitWindowHandle::new(view));
        // SAFETY: the view is the image's, which lives as long as the image,
        // and no image hides while the backend is borrowed.
        Some(unsafe { WindowHandle::borrow_raw(raw) })
    }

    fn hide_drag_image(&mut self, to: Option<ScreenRect>) {
        if let Some(image) = self.drag_image.take() {
            image.hide(to.map(|to| screens::in_appkit(to, screens::top(self.mtm))));
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

    fn set_clipboard_image(&mut self, png: &[u8]) {
        // SAFETY: the type is AppKit's constant.
        let kind = unsafe { NSPasteboardTypePNG };
        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.clearContents();
        pasteboard.setData_forType(Some(&NSData::with_bytes(png)), kind);
    }

    fn keep_awake(&mut self, awake: Awake, reason: &str) {
        self.kept_awake.set(awake, reason);
    }

    fn show_save_panel(&mut self, window: WindowId, name: &str, message: &str) -> SavePanelId {
        let id = SavePanelId::new(self.next_id());
        match self.windows.get(&window) {
            Some(shown) => {
                let panel = save::begin(
                    self.mtm,
                    shown.ns_window(),
                    (name, message),
                    self.shared.clone(),
                    id,
                );
                self.save_panels.insert(id, Some((panel, window)));
            }
            // Of a window not there, a panel that the user never saw.
            None => {
                self.save_panels.insert(id, None);
                if let Some(shared) = self.shared.upgrade() {
                    shared.enqueue(Pending::App(AppEvent::SavePanelClosed {
                        panel: id,
                        path: None,
                    }));
                }
            }
        }
        id
    }

    fn dismiss_save_panel(&mut self, panel: SavePanelId) {
        // Forgotten first, so that its sheet's end tells nothing.
        let Some(Some((panel, window))) = self.save_panels.remove(&panel) else {
            return;
        };
        if let Some(window) = self.windows.get(&window) {
            window.ns_window().endSheet(&panel);
        }
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

    fn dismiss_alert(&mut self, alert: AlertId) {
        // Forgotten first, so that its sheet's end answers nothing; one on
        // its own whose loop has not run finds itself gone.
        let Some(open) = self.alerts.remove(&alert) else {
            return;
        };
        if let Some(window) = open.window.and_then(|window| self.windows.get(&window)) {
            window.ns_window().endSheet(&open.alert.window());
        }
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

    fn draws_fills(&self) -> bool {
        true
    }

    fn set_fills(&mut self, window: WindowId, fills: &[Fill]) {
        if let Some(window) = self.windows.get(&window) {
            window.set_fills(fills);
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

    fn hold_pointer(&mut self, window: WindowId, shape: Option<PointerShape>) {
        if let Some(window) = self.windows.get(&window) {
            window.hold_pointer(shape);
        }
    }

    fn set_title_accessory(&mut self, window: WindowId, text: Option<&str>) {
        if let Some(window) = self.windows.get(&window) {
            window.set_title_accessory(text);
        }
    }

    fn shortcut_writer(&self) -> fn(Shortcut) -> String {
        |shortcut| shortcut.mac_text()
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

    fn set_badge(&mut self, badge: Option<&str>) {
        let tile = NSApplication::sharedApplication(self.mtm).dockTile();
        tile.setBadgeLabel(badge.map(NSString::from_str).as_deref());
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

    fn decode_image(&self, data: &[u8]) -> Option<lokyr_platform_core::Pixels> {
        decoded(data)
    }

    fn image_decoder(&self) -> Option<lokyr_platform_core::ImageDecoder> {
        // A thread of no run loop drains no pool of its own: each decode
        // drains its own.
        Some(std::sync::Arc::new(|data: &[u8]| {
            objc2::rc::autoreleasepool(|_| decoded(data))
        }))
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
        keep_dock_tile_icon(icon);
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

/// Whether the file at `path` holds the quarantine's attribute of a
/// download, of no approval of Gatekeeper's.
fn downloaded(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    /// The flag of a quarantined file that the user let open, as
    /// Gatekeeper sets it.
    const USER_APPROVED: u32 = 0x0040;
    let Ok(path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // The attribute reads `flags;time;agent;id`, its flags in hex.
    let mut value = [0u8; 128];
    // SAFETY: both strings are NUL-terminated and live through the
    // call, which writes `value.len()` bytes at most into `value`.
    let size = unsafe {
        libc::getxattr(
            path.as_ptr(),
            c"com.apple.quarantine".as_ptr(),
            value.as_mut_ptr().cast(),
            value.len(),
            0,
            0,
        )
    };
    if size < 0 {
        // No attribute, or one longer than any that the system
        // writes, which asks all the same.
        return std::io::Error::last_os_error().raw_os_error() == Some(libc::ERANGE);
    }
    let value = &value[..size.unsigned_abs()];
    let flags = value
        .split(|&byte| byte == b';')
        .next()
        .and_then(|flags| std::str::from_utf8(flags).ok())
        .and_then(|flags| u32::from_str_radix(flags, 16).ok());
    flags.is_none_or(|flags| flags & USER_APPROVED == 0)
}

/// Marks the file at `path` as a download that the user has not let open,
/// as a browser marks it, of Lokyr the agent: Gatekeeper and [`downloaded`]
/// ask before it opens as a program.
fn quarantine(path: &Path) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    /// The flag of a file that came from elsewhere, of no approval.
    const FROM_ELSEWHERE: u32 = 0x0001;
    let path = std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "a path of a NUL"))?;
    let since = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    // `flags;time;agent;id`, its flags and time in hex.
    let value = format!("{FROM_ELSEWHERE:04x};{since:08x};Lokyr;");
    // SAFETY: both strings are NUL-terminated and `value`'s bytes live
    // through the call, which reads `value.len()` of them.
    let set = unsafe {
        libc::setxattr(
            path.as_ptr(),
            c"com.apple.quarantine".as_ptr(),
            value.as_ptr().cast(),
            value.len(),
            0,
            0,
        )
    };
    if set == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// The key of the app's defaults that holds the name of the icon's variant
/// shown, which the bundle's Dock tile plug-in draws of its own bundle while
/// the app is not running.
const DOCK_TILE_ICON: &str = "DockTileIcon";

/// The distributed notification by which the plug-in learns that the icon
/// changed.
const DOCK_TILE_ICON_CHANGED: &str = "io.github.sephiroth74.lokyr.DockTileIconChanged";

/// Keeps the icon's variant of the image at `icon`, or the bundle's own of
/// none, for the Dock tile plug-in: its name in the app's defaults, written
/// to the disk at once, as the app may quit next, and the plug-in told.
fn keep_dock_tile_icon(icon: Option<&Path>) {
    let defaults = objc2_foundation::NSUserDefaults::standardUserDefaults();
    let key = NSString::from_str(DOCK_TILE_ICON);
    match icon.and_then(variant_of) {
        Some(name) => {
            let name = NSString::from_str(&name);
            // SAFETY: a string is a property list's value, which the
            // defaults take.
            unsafe { defaults.setObject_forKey(Some(&name), &key) };
        }
        None => defaults.removeObjectForKey(&key),
    }
    defaults.synchronize();
    let center = objc2_foundation::NSDistributedNotificationCenter::defaultCenter();
    // SAFETY: a notification of a name alone, of no object nor user info.
    unsafe {
        center.postNotificationName_object_userInfo_deliverImmediately(
            &NSString::from_str(DOCK_TILE_ICON_CHANGED),
            None,
            None,
            true,
        );
    }
}

/// The variant's name of an icon's file, `lokyr-<name>.icns`.
fn variant_of(icon: &Path) -> Option<String> {
    let name = icon.file_name()?.to_str()?;
    let variant = name.strip_prefix("lokyr-")?.strip_suffix(".icns")?;
    (!variant.is_empty()).then(|| variant.to_owned())
}

/// The most pixels of an image that the platform decodes.
const MOST_DECODED: usize = 16_000_000;

/// The pixels of the image that `data` encodes, PNG, JPEG, GIF, TIFF or
/// BMP of its
/// first frame, as AppKit decodes it, drawn into a bitmap of sRGB of
/// premultiplied RGBA; none of data that does not decode, or of an image
/// of more pixels than [`MOST_DECODED`].
fn decoded(data: &[u8]) -> Option<lokyr_platform_core::Pixels> {
    use objc2_core_foundation::{CGPoint, CGRect, CGSize};
    use objc2_core_graphics::{
        CGBitmapContextCreate, CGColorSpace, CGContext, CGImage, CGImageAlphaInfo,
        CGImageByteOrderInfo, kCGColorSpaceSRGB,
    };
    let data = NSData::with_bytes(data);
    let rep = objc2_app_kit::NSBitmapImageRep::imageRepWithData(&data)?;
    let image = rep.CGImage()?;
    let (width, height) = (CGImage::width(Some(&image)), CGImage::height(Some(&image)));
    if width == 0 || height == 0 || width.saturating_mul(height) > MOST_DECODED {
        return None;
    }
    let mut rgba = vec![0_u8; width * height * 4];
    // SAFETY: the framework's constant, a name that it keeps.
    let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))?;
    let info = CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0;
    // SAFETY: the buffer holds `height` rows of `width` pixels of four
    // bytes, which the context writes while it lives, within this function.
    let context = unsafe {
        CGBitmapContextCreate(
            rgba.as_mut_ptr().cast(),
            width,
            height,
            8,
            width * 4,
            Some(&space),
            info,
        )
    }?;
    let whole = CGRect::new(
        CGPoint::new(0.0, 0.0),
        CGSize::new(width as f64, height as f64),
    );
    CGContext::draw_image(Some(&context), whole, Some(&image));
    drop(context);
    Some(lokyr_platform_core::Pixels {
        width: u32::try_from(width).ok()?,
        height: u32::try_from(height).ok()?,
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quarantined_file_is_downloaded_until_gatekeeper_approves_it() {
        let path = std::env::temp_dir().join(format!("lokyr-quarantined-{}", std::process::id()));
        std::fs::write(&path, b"#!/bin/sh\n").unwrap();
        assert!(!downloaded(&path));
        quarantine(&path).unwrap();
        assert!(downloaded(&path));
        std::fs::remove_file(&path).unwrap();
        assert!(quarantine(&path).is_err(), "of no file");
    }

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

    #[test]
    fn a_file_is_downloaded_of_its_quarantine_until_gatekeeper_approves_it() {
        let directory =
            std::env::temp_dir().join(format!("lokyr-quarantine-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let file = |name: &str, quarantine: Option<&str>| {
            let path = directory.join(name);
            std::fs::write(&path, "echo\n").unwrap();
            if let Some(value) = quarantine {
                let status = std::process::Command::new("/usr/bin/xattr")
                    .args(["-w", "com.apple.quarantine", value])
                    .arg(&path)
                    .status()
                    .unwrap();
                assert!(status.success());
            }
            path
        };
        assert!(downloaded(&file(
            "safari.command",
            Some("0083;6720a000;Safari;")
        )));
        assert!(!downloaded(&file(
            "approved.command",
            Some("00c3;6720a000;Safari;")
        )));
        assert!(downloaded(&file(
            "unreadable.command",
            Some("zz;6720a000;Safari;")
        )));
        assert!(!downloaded(&file("made.command", None)));
        assert!(!downloaded(&directory.join("gone")));
        let _ = std::fs::remove_dir_all(directory);
    }
}
