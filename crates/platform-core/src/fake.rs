//! A platform without a window system, for tests. It keeps what the app
//! asks of it, and hands the app the events that a test sends, once it has
//! applied what they change, as a backend would.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use crate::{
    Alert, AlertId, AppEvent, Appearance, Backdrop, ContextMenu, Error, Handler, MainMenu,
    Platform, Point, PointerShape, Size, TargetKind, TimerId, Waker, WindowAttributes, WindowEvent,
    WindowHandle, WindowId, WindowSize,
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
    /// Whether the app asked for a frame, which the next
    /// [`WindowEvent::Frame`] that a test sends gives.
    pub frame_requested: bool,
    /// Where the app said that the typed text goes, if it said.
    pub text_area: Option<(Point, Size)>,
    /// Its backdrop, when it is translucent.
    pub translucent: Option<Backdrop>,
    /// Whether the app asked for the pointer's moves without a button.
    pub mouse_moves: bool,
    /// How the pointer looks over its content, as the app asked.
    pub pointer: PointerShape,
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

/// A notification that the app showed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FakeNotification {
    /// The window that it is for.
    pub window: WindowId,
    /// Its title.
    pub title: String,
    /// Its text.
    pub body: String,
}

/// A definition that the app showed.
#[derive(Clone, Debug, PartialEq)]
pub struct FakeDefinition {
    /// The window that it showed over.
    pub window: WindowId,
    /// What it defines.
    pub text: String,
    /// Where the text's baseline starts.
    pub baseline: Point,
    /// The text's font, its family and its size, if the app gave one.
    pub font: Option<(String, f64)>,
}

/// The fake platform. Its focus moves only with the
/// [`WindowEvent::Focused`] events that a test sends, as a backend tells
/// the focus through them.
#[derive(Debug)]
pub struct Fake {
    scale: f64,
    /// The system's appearance, and the app's own.
    appearance: Appearance,
    own_appearance: Option<Appearance>,
    windows: BTreeMap<WindowId, FakeWindow>,
    focused: Option<WindowId>,
    menu: Option<MainMenu>,
    /// The commands whose menu items are dimmed, and those whose items are
    /// ticked.
    disabled: std::collections::BTreeSet<String>,
    checked: std::collections::BTreeSet<String>,
    /// The context menus that the app opened, from the first, each over its
    /// window at its position.
    context_menus: Vec<(WindowId, Point, ContextMenu)>,
    /// The definitions that the app showed, from the first.
    definitions: Vec<FakeDefinition>,
    clipboard: Option<String>,
    alerts: Vec<FakeAlert>,
    /// The timers that have not fired, with the time of each.
    timers: BTreeMap<TimerId, Instant>,
    /// Which Option keys act as Meta.
    option_as_meta: (bool, bool),
    /// The icons that the app showed, from the first.
    app_icons: Vec<Option<PathBuf>>,
    /// The user's languages and regions, none unless a test sets them.
    locales: Vec<String>,
    /// How many times the app asked for the user's attention, and played
    /// the alert sound.
    attention_requests: usize,
    alert_sounds: usize,
    /// The notifications that the app showed, from the first.
    notifications: Vec<FakeNotification>,
    /// The windows that the app asked to bring forward, from the first.
    focus_requests: Vec<WindowId>,
    /// The URLs and the paths that the app opened, and those that it
    /// showed in the file manager, from the first.
    opened: Vec<String>,
    revealed: Vec<String>,
    last_id: u64,
    woken: Arc<AtomicBool>,
    quit: bool,
    /// How many requests to quit the app refused.
    quit_refusals: usize,
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
            own_appearance: None,
            windows: BTreeMap::new(),
            focused: None,
            menu: None,
            disabled: std::collections::BTreeSet::new(),
            checked: std::collections::BTreeSet::new(),
            context_menus: Vec::new(),
            definitions: Vec::new(),
            clipboard: None,
            alerts: Vec::new(),
            timers: BTreeMap::new(),
            option_as_meta: (false, false),
            app_icons: Vec::new(),
            locales: Vec::new(),
            attention_requests: 0,
            alert_sounds: 0,
            notifications: Vec::new(),
            focus_requests: Vec::new(),
            opened: Vec::new(),
            revealed: Vec::new(),
            last_id: 0,
            woken: Arc::new(AtomicBool::new(false)),
            quit: false,
            quit_refusals: 0,
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
            WindowEvent::Frame => state.frame_requested = false,
            WindowEvent::Focused(true) => self.focused = Some(window),
            WindowEvent::Focused(false) if self.focused == Some(window) => self.focused = None,
            _ => {}
        }
        handler.window_event(self, window, event);
        true
    }

    /// Hands `handler` an event of the application, once the fake has
    /// applied the appearance that it brings, as the system's. Returns
    /// whether it handed it: not once the app has quit, nor a change of the
    /// system's appearance while the app has its own, which it does not see.
    pub fn send_app(&mut self, handler: &mut dyn Handler, event: AppEvent) -> bool {
        if self.quit {
            return false;
        }
        if let AppEvent::AppearanceChanged(appearance) = event {
            self.appearance = appearance;
            if self.own_appearance.is_some() {
                return false;
            }
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
        self.answer_alert_with(handler, alert, button, false)
    }

    /// Closes `alert` with `button` as [`Fake::answer_alert`] does, its
    /// checkbox ticked when `suppressed`, if it has one.
    pub fn answer_alert_with(
        &mut self,
        handler: &mut dyn Handler,
        alert: AlertId,
        button: usize,
        suppressed: bool,
    ) -> bool {
        let Some(index) = self.alerts.iter().position(|open| open.id == alert) else {
            return false;
        };
        let open = self.alerts.remove(index);
        let suppressed = suppressed && open.alert.suppression.is_some();
        self.send_app(
            handler,
            AppEvent::AlertClosed {
                alert,
                button,
                suppressed,
            },
        )
    }

    /// Fires the timers whose time has come by `now`, from the earliest,
    /// as the clock reaches it: those that the app starts meanwhile fire
    /// too, if their time has come. Returns how many fired.
    pub fn fire_timers(&mut self, handler: &mut dyn Handler, now: Instant) -> usize {
        let mut fired = 0;
        loop {
            let due = self
                .timers
                .iter()
                .filter(|&(_, &at)| at <= now)
                .min_by_key(|&(&timer, &at)| (at, timer))
                .map(|(&timer, _)| timer);
            let Some(timer) = due else {
                return fired;
            };
            self.timers.remove(&timer);
            if !self.send_app(handler, AppEvent::Timer(timer)) {
                return fired;
            }
            fired += 1;
        }
    }

    /// The timers that have not fired, with the time of each, from the
    /// first started.
    pub fn timers(&self) -> impl Iterator<Item = (TimerId, Instant)> + '_ {
        self.timers.iter().map(|(&timer, &at)| (timer, at))
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

    /// Whether the menu items of `command` are enabled.
    pub fn command_enabled(&self, command: &str) -> bool {
        !self.disabled.contains(command)
    }

    /// Whether the menu items of `command` are ticked.
    pub fn command_checked(&self, command: &str) -> bool {
        self.checked.contains(command)
    }

    /// The appearance that the app gave itself, if it gave one.
    pub fn own_appearance(&self) -> Option<Appearance> {
        self.own_appearance
    }

    /// The context menus that the app opened, from the first, each over its
    /// window at its position.
    pub fn context_menus(&self) -> &[(WindowId, Point, ContextMenu)] {
        &self.context_menus
    }

    /// The definitions that the app showed, from the first.
    pub fn definitions(&self) -> &[FakeDefinition] {
        &self.definitions
    }

    /// The alerts that are open, from the first opened.
    pub fn alerts(&self) -> &[FakeAlert] {
        &self.alerts
    }

    /// Which Option keys act as Meta, as the app said: the left one, and
    /// the right one.
    pub fn option_as_meta(&self) -> (bool, bool) {
        self.option_as_meta
    }

    /// Gives the user the languages and regions of `locales`, as
    /// [`Platform::locales`] names them.
    pub fn set_locales(&mut self, locales: &[&str]) {
        self.locales = locales.iter().map(|&locale| locale.to_owned()).collect();
    }

    /// The icons that the app showed, from the first: a file's, or `None`
    /// for the application's own.
    pub fn app_icons(&self) -> &[Option<PathBuf>] {
        &self.app_icons
    }

    /// How many times the app asked for the user's attention.
    pub fn attention_requests(&self) -> usize {
        self.attention_requests
    }

    /// How many times the app played the alert sound.
    pub fn alert_sounds(&self) -> usize {
        self.alert_sounds
    }

    /// The notifications that the app showed, from the first.
    pub fn notifications(&self) -> &[FakeNotification] {
        &self.notifications
    }

    /// The windows that the app asked to bring forward, from the first,
    /// which take no focus until a test sends it.
    pub fn focus_requests(&self) -> &[WindowId] {
        &self.focus_requests
    }

    /// The URLs and the paths that the app opened, from the first.
    pub fn opened(&self) -> &[String] {
        &self.opened
    }

    /// The paths that the app showed in the file manager, from the first.
    pub fn revealed(&self) -> &[String] {
        &self.revealed
    }

    /// Whether the app has quit.
    pub fn has_quit(&self) -> bool {
        self.quit
    }

    /// How many of the system's requests to quit the app refused.
    pub fn quit_refusals(&self) -> usize {
        self.quit_refusals
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

    fn refuse_quit(&mut self) {
        self.quit_refusals += 1;
    }

    fn appearance(&self) -> Appearance {
        self.own_appearance.unwrap_or(self.appearance)
    }

    fn set_appearance(&mut self, appearance: Option<Appearance>) {
        self.own_appearance = appearance;
    }

    fn locales(&self) -> Vec<String> {
        self.locales.clone()
    }

    fn create_window(&mut self, attributes: &WindowAttributes) -> Result<WindowId, Error> {
        let id = WindowId::new(self.next_id());
        self.windows.insert(
            id,
            FakeWindow {
                title: attributes.title.clone(),
                size: WindowSize::new(attributes.size, self.scale),
                occluded: false,
                frame_requested: false,
                text_area: None,
                translucent: None,
                mouse_moves: false,
                pointer: PointerShape::Arrow,
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

    fn window_handle(&self, _window: WindowId) -> Option<WindowHandle<'_>> {
        // Nothing draws into the fake's windows.
        None
    }

    fn request_frame(&mut self, window: WindowId) {
        if let Some(state) = self.windows.get_mut(&window) {
            state.frame_requested = true;
        }
    }

    fn focus_window(&mut self, window: WindowId) {
        self.focus_requests.push(window);
    }

    fn focused_window(&self) -> Option<WindowId> {
        self.focused
    }

    fn set_main_menu(&mut self, menu: &MainMenu) {
        self.menu = Some(menu.clone());
    }

    fn set_command_enabled(&mut self, command: &str, enabled: bool) {
        if enabled {
            self.disabled.remove(command);
        } else {
            self.disabled.insert(command.to_owned());
        }
    }

    fn set_command_checked(&mut self, command: &str, checked: bool) {
        if checked {
            self.checked.insert(command.to_owned());
        } else {
            self.checked.remove(command);
        }
    }

    fn show_context_menu(&mut self, window: WindowId, position: Point, menu: &ContextMenu) {
        self.context_menus.push((window, position, menu.clone()));
    }

    fn show_definition(
        &mut self,
        window: WindowId,
        text: &str,
        baseline: Point,
        font: Option<(&str, f64)>,
    ) {
        self.definitions.push(FakeDefinition {
            window,
            text: text.to_owned(),
            baseline,
            font: font.map(|(family, size)| (family.to_owned(), size)),
        });
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

    fn start_timer(&mut self, at: Instant) -> TimerId {
        let id = TimerId::new(self.next_id());
        self.timers.insert(id, at);
        id
    }

    fn cancel_timer(&mut self, timer: TimerId) {
        self.timers.remove(&timer);
    }

    fn set_text_area(&mut self, window: WindowId, origin: Point, size: Size) {
        if let Some(state) = self.windows.get_mut(&window) {
            state.text_area = Some((origin, size));
        }
    }

    fn set_option_as_meta(&mut self, left: bool, right: bool) {
        self.option_as_meta = (left, right);
    }

    fn track_mouse_moves(&mut self, window: WindowId, track: bool) {
        if let Some(state) = self.windows.get_mut(&window) {
            state.mouse_moves = track;
        }
    }

    fn set_pointer_shape(&mut self, window: WindowId, shape: PointerShape) {
        if let Some(state) = self.windows.get_mut(&window) {
            state.pointer = shape;
        }
    }

    /// Fails for a file that does not exist, as a backend fails to read it.
    fn set_app_icon(&mut self, icon: Option<&Path>) -> Result<(), Error> {
        if let Some(path) = icon
            && !path.is_file()
        {
            return Err(Error::Icon(path.display().to_string()));
        }
        self.app_icons.push(icon.map(Path::to_owned));
        Ok(())
    }

    fn request_attention(&mut self) {
        self.attention_requests += 1;
    }

    fn play_alert_sound(&mut self) {
        self.alert_sounds += 1;
    }

    fn notify(&mut self, window: WindowId, title: &str, body: &str) {
        self.notifications.push(FakeNotification {
            window,
            title: title.to_owned(),
            body: body.to_owned(),
        });
    }

    fn set_translucent(&mut self, window: WindowId, backdrop: Option<Backdrop>) {
        if let Some(state) = self.windows.get_mut(&window) {
            state.translucent = backdrop;
        }
    }

    /// A URL, unless of `file`; a directory, a program, as the shared rule
    /// tells of the file that the target's links lead to, or a file; a
    /// path that does not exist, a `file` URL of another machine and a text
    /// that is no URL fail, as a backend's do.
    fn target_kind(&self, target: &str) -> Result<TargetKind, Error> {
        let failed = |why: String| Error::Open(target.to_owned(), why);
        let path = match fake_path(target) {
            Target::Url => return Ok(TargetKind::Url),
            Target::Path(path) => path,
            Target::Neither(why) => return Err(failed(why.to_owned())),
        };
        let path = std::fs::canonicalize(path).map_err(|error| failed(error.to_string()))?;
        let metadata = std::fs::metadata(&path).map_err(|error| failed(error.to_string()))?;
        Ok(if crate::runs_by_itself(&path, &metadata) {
            TargetKind::Program
        } else if metadata.is_dir() {
            TargetKind::Directory
        } else {
            TargetKind::File
        })
    }

    /// Opens nothing, and records `target`, or, for a program, reveals it.
    fn open(&mut self, target: &str) -> Result<(), Error> {
        if self.target_kind(target)? == TargetKind::Program {
            return self.reveal(target);
        }
        self.opened.push(target.to_owned());
        Ok(())
    }

    /// Shows nothing, and records `target`; a URL that is not of `file`
    /// fails.
    fn reveal(&mut self, target: &str) -> Result<(), Error> {
        if self.target_kind(target)? == TargetKind::Url {
            return Err(Error::Open(target.to_owned(), "not a file".to_owned()));
        }
        self.revealed.push(target.to_owned());
        Ok(())
    }
}

/// What a target is, as the fake reads it.
enum Target {
    /// A URL of another scheme than `file`.
    Url,
    /// The path of an absolute path, or of a `file` URL of this machine.
    Path(PathBuf),
    /// Neither, for the reason.
    Neither(&'static str),
}

/// What `target` is: an absolute path; a `file` URL, of any case, with no
/// host or `localhost`, its escapes decoded; or another URL, of a scheme
/// that starts with a letter.
fn fake_path(target: &str) -> Target {
    if target.starts_with('/') {
        return Target::Path(PathBuf::from(target));
    }
    let Some((scheme, rest)) = target.split_once(':') else {
        return Target::Neither("it is not a URL");
    };
    let scheme_ok = scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c));
    if !scheme_ok {
        return Target::Neither("it is not a URL");
    }
    if !scheme.eq_ignore_ascii_case("file") {
        return Target::Url;
    }
    let path = match rest.strip_prefix("//") {
        Some(rest) => {
            let (host, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
            if !host.is_empty() && !host.eq_ignore_ascii_case("localhost") {
                return Target::Neither("it is on another machine");
            }
            path
        }
        None => rest,
    };
    match decoded(path) {
        Some(path) if path.starts_with('/') => Target::Path(PathBuf::from(path)),
        _ => Target::Neither("it has no path"),
    }
}

/// `text` with its `%` escapes decoded, if they are escapes of UTF-8.
fn decoded(text: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(text.len());
    let mut rest = text.as_bytes();
    while let Some((&byte, after)) = rest.split_first() {
        if byte == b'%' {
            let hex = std::str::from_utf8(after.get(..2)?).ok()?;
            bytes.push(u8::from_str_radix(hex, 16).ok()?);
            rest = &after[2..];
        } else {
            bytes.push(byte);
            rest = after;
        }
    }
    String::from_utf8(bytes).ok()
}
