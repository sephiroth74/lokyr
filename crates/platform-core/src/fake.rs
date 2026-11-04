//! A platform without a window system, for tests. It keeps what the app
//! asks of it, and hands the app the events that a test sends, once it has
//! applied what they change, as a backend would.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

use crate::{
    Alert, AlertId, AppEvent, Appearance, Backdrop, ContentRect, ContextMenu, Error, GlobalKey,
    Handler, Layer, LayerPlace, MainMenu, Platform, Point, PointerArea, PointerShape, QuickPlace,
    ScreenRect, Size, Sweep, Target, TargetKind, TimerId, Titlebar, Usage, Waker, WindowAttributes,
    WindowEvent, WindowHandle, WindowId, WindowSize,
};

/// The scale of the fake's display when a test sets none: a Retina one.
pub const DEFAULT_SCALE: f64 = 2.0;

/// The fake's one screen.
pub const SCREEN: ScreenRect = ScreenRect {
    x: 0.0,
    y: 0.0,
    width: 1440.0,
    height: 900.0,
};

/// The screen's area for windows, under a menu bar of 25 points.
pub const SCREEN_AREA: ScreenRect = ScreenRect {
    x: 0.0,
    y: 25.0,
    width: 1440.0,
    height: 875.0,
};

/// The size of a quick window's content before it first shows.
const QUICK_SIZE: Size = Size {
    width: 800.0,
    height: 400.0,
};

/// A window that the app opened, as the fake keeps it.
#[derive(Clone, Debug, PartialEq)]
pub struct FakeWindow {
    /// Its title.
    pub title: String,
    /// Where the pointer is over its content, as the events sent to it
    /// last told.
    pub pointer_at: Point,
    /// The size of its content.
    pub size: WindowSize,
    /// Whether it is hidden or covered.
    pub occluded: bool,
    /// Whether the app asked for a frame, which the next
    /// [`WindowEvent::Frame`] that a test sends gives.
    pub frame_requested: bool,
    /// How many times the app asked for a frame.
    pub frames_asked: usize,
    /// Where the app said that the typed text goes, if it said.
    pub text_area: Option<(Point, Size)>,
    /// How many times the app ended the input method's composition.
    pub compositions_ended: usize,
    /// Its backdrop, when it is translucent.
    pub translucent: Option<Backdrop>,
    /// Whether it asked for a frame while it was hidden or covered, which
    /// comes once it shows.
    pub frame_held: bool,
    /// Whether it keeps its frame.
    pub remember: bool,
    /// Where the app placed its layers, and how they blink: none while
    /// hidden.
    pub layers: HashMap<Layer, LayerPlace>,
    /// How many times the app placed a layer, or hid it.
    pub layer_places: usize,
    /// The sweeps that the app showed last.
    pub sweeps: Vec<Sweep>,
    /// Whether it opened at the frame kept last.
    pub restore: bool,
    /// Whether it opened of the size of the frame kept last.
    pub restore_size: bool,
    /// Whether the app asked for the pointer's moves without a button.
    pub mouse_moves: bool,
    /// How the pointer looks over its content, as the app asked.
    pub pointer: PointerShape,
    /// The areas of its content of their own pointers, as the app gave
    /// them.
    pub pointer_areas: Vec<PointerArea>,
    /// Its own appearance, if the app gave it one.
    pub appearance: Option<Appearance>,
    /// Whether it is a quick window, and its place while it shows.
    pub quick: Option<Option<QuickPlace>>,
    /// Its titlebar, as the app gave it, which changes no size of the fake
    /// one's.
    pub titlebar: Titlebar,
    /// The parts of its titlebar that the app gave as its content's.
    pub titlebar_areas: Vec<ContentRect>,
    /// Whether an accessibility client asked for its tree, as a test says.
    pub accessibility_asked: bool,
    /// The updates of its accessibility tree that the app gave, once a
    /// client asked, in their order.
    pub accessibility: Vec<accesskit::TreeUpdate>,
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
    /// Whether the system's keyboard navigation reaches every control.
    keyboard_navigation: bool,
    /// Whether the system reduces motion.
    reduce_motion: bool,
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
    /// The height of the titlebar of the windows that open, which their
    /// content runs under.
    bar: f64,
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
    edited: Vec<PathBuf>,
    /// Why the text editor opens nothing, as the next open fails.
    editor_fails: Option<String>,
    /// Whether the secure entry of the keyboard is on, and how many times
    /// it changed.
    secure_input: (bool, usize),
    /// Whether the app can update itself, as a test says.
    can_update: bool,
    /// The updates' checks now and then, on or off, once started.
    update_checks: Option<bool>,
    /// The checks for updates that the app asked.
    updates_checked: usize,
    usage: Option<Usage>,
    /// When the fake was made, as the process's start.
    started: SystemTime,
    /// The keys that the app posted, from the first, which a test sends
    /// back as the keyboard's.
    posted_keys: Vec<(WindowId, crate::KeyCode, String)>,
    revealed: Vec<String>,
    /// The keys that the app takes from every app, and those that another
    /// app takes, which the fake refuses.
    global_keys: Vec<GlobalKey>,
    taken_keys: Vec<GlobalKey>,
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
            keyboard_navigation: false,
            reduce_motion: false,
            windows: BTreeMap::new(),
            focused: None,
            menu: None,
            disabled: std::collections::BTreeSet::new(),
            checked: std::collections::BTreeSet::new(),
            context_menus: Vec::new(),
            bar: 0.0,
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
            edited: Vec::new(),
            editor_fails: None,
            secure_input: (false, 0),
            can_update: false,
            update_checks: None,
            updates_checked: 0,
            usage: None,
            started: SystemTime::now(),
            posted_keys: Vec::new(),
            revealed: Vec::new(),
            global_keys: Vec::new(),
            taken_keys: Vec::new(),
            last_id: 0,
            woken: Arc::new(AtomicBool::new(false)),
            quit: false,
            quit_refusals: 0,
        }
    }

    /// Turns the system's keyboard navigation, of every control, on or off.
    pub fn set_keyboard_navigation(&mut self, on: bool) {
        self.keyboard_navigation = on;
    }

    /// Turns the system's reduced motion on or off.
    pub fn set_reduce_motion(&mut self, on: bool) {
        self.reduce_motion = on;
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
            WindowEvent::Occluded(occluded) => {
                state.occluded = *occluded;
                // The frame asked for meanwhile comes as it shows.
                if !occluded && std::mem::take(&mut state.frame_held) {
                    state.frame_requested = true;
                }
            }
            WindowEvent::Frame => state.frame_requested = false,
            WindowEvent::MouseMoved { position, .. }
            | WindowEvent::MouseButton { position, .. }
            | WindowEvent::Scroll { position, .. } => state.pointer_at = *position,
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
        self.answer_alert_typed(handler, alert, (button, suppressed), None)
    }

    /// Closes `alert` with `button`, its checkbox ticked when `suppressed`,
    /// as [`Fake::answer_alert_with`] does, its field holding `text`, or the
    /// text that it opened with, if it has one.
    pub fn answer_alert_typed(
        &mut self,
        handler: &mut dyn Handler,
        alert: AlertId,
        (button, suppressed): (usize, bool),
        text: Option<&str>,
    ) -> bool {
        let Some(index) = self.alerts.iter().position(|open| open.id == alert) else {
            return false;
        };
        let open = self.alerts.remove(index);
        let suppressed = suppressed && open.alert.suppression.is_some();
        let text = open
            .alert
            .input
            .map(|input| text.map_or(input, str::to_owned));
        self.send_app(
            handler,
            AppEvent::AlertClosed {
                alert,
                button,
                suppressed,
                text,
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

    /// An accessibility client asks for the tree of `window`, as a
    /// platform tells it: from then on, the updates of its tree come; and
    /// whether `handler` was told.
    pub fn ask_accessibility(&mut self, handler: &mut dyn Handler, window: WindowId) -> bool {
        match self.windows.get_mut(&window) {
            Some(state) => state.accessibility_asked = true,
            None => return false,
        }
        self.send(handler, window, WindowEvent::AccessibilityAsked)
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

    /// Gives the windows that open from now on a titlebar `bar` points high
    /// over their content, which runs under it, as a backend's: their
    /// content is that much taller than the size asked for, which is the one
    /// below the bar.
    pub fn set_bar(&mut self, bar: f64) {
        self.bar = bar;
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

    /// The URLs and the paths that the app opened, from the first, as the
    /// targets' locations tell them.
    pub fn opened(&self) -> &[String] {
        &self.opened
    }

    /// The paths that the app showed in the file manager, from the first,
    /// as the targets' locations tell them.
    pub fn revealed(&self) -> &[String] {
        &self.revealed
    }

    /// Gives what the process has used of the machine, which `usage` tells.
    pub fn set_usage(&mut self, usage: Usage) {
        self.usage = Some(usage);
    }

    /// The files that the app opened in the text editor, in order.
    pub fn edited(&self) -> &[PathBuf] {
        &self.edited
    }

    /// Whether the secure entry of the keyboard is on, as the app last set
    /// it, and how many times it changed.
    pub fn secure_input(&self) -> (bool, usize) {
        self.secure_input
    }

    /// Has the app able to update itself, or not, as it is by default.
    pub fn set_can_update(&mut self, can: bool) {
        self.can_update = can;
    }

    /// The updates' checks now and then, on or off, as the app last set
    /// them; `None` before it started them, or where it cannot update
    /// itself.
    pub fn update_checks(&self) -> Option<bool> {
        self.update_checks
    }

    /// How many checks for updates the app asked.
    pub fn updates_checked(&self) -> usize {
        self.updates_checked
    }

    /// Has the text editor open nothing, for `why`, until it is `None`.
    pub fn fail_editing(&mut self, why: Option<&str>) {
        self.editor_fails = why.map(str::to_owned);
    }

    /// The keys that the app posted, from the first, each with its window
    /// and its text.
    pub fn posted_keys(&self) -> &[(WindowId, crate::KeyCode, String)] {
        &self.posted_keys
    }

    /// The keys that the app takes from every app, as it gave them last.
    pub fn global_keys(&self) -> &[GlobalKey] {
        &self.global_keys
    }

    /// Has another app take `key`, which the app cannot take from then on.
    pub fn take_global_key(&mut self, key: GlobalKey) {
        self.taken_keys.push(key);
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

    fn keyboard_navigation(&self) -> bool {
        self.keyboard_navigation
    }

    fn reduce_motion(&self) -> bool {
        self.reduce_motion
    }

    fn set_appearance(&mut self, appearance: Option<Appearance>) {
        self.own_appearance = appearance;
    }

    fn screen_scale(&self) -> f64 {
        self.scale
    }

    fn process_started(&self) -> Option<SystemTime> {
        Some(self.started)
    }

    fn post_key(&mut self, window: WindowId, code: crate::KeyCode, text: &str) {
        self.posted_keys.push((window, code, text.to_owned()));
    }

    fn usage(&self) -> Option<Usage> {
        self.usage
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
                size: WindowSize::new(
                    Size {
                        width: attributes.size.width,
                        height: attributes.size.height + self.bar,
                    },
                    self.scale,
                )
                .with_bar(self.bar),
                occluded: false,
                frame_requested: false,
                frames_asked: 0,
                text_area: None,
                compositions_ended: 0,
                translucent: None,
                frame_held: false,
                remember: attributes.remember,
                layers: HashMap::new(),
                layer_places: 0,
                sweeps: Vec::new(),
                restore: attributes.restore,
                restore_size: attributes.restore_size,
                mouse_moves: false,
                pointer: PointerShape::Arrow,
                pointer_at: Point::default(),
                pointer_areas: Vec::new(),
                appearance: None,
                quick: None,
                titlebar: Titlebar::default(),
                titlebar_areas: Vec::new(),
                accessibility_asked: false,
                accessibility: Vec::new(),
            },
        );
        Ok(id)
    }

    fn create_quick_window(&mut self, title: &str) -> Result<WindowId, Error> {
        let id = WindowId::new(self.next_id());
        self.windows.insert(
            id,
            FakeWindow {
                title: title.to_owned(),
                size: WindowSize::new(QUICK_SIZE, self.scale),
                occluded: true,
                frame_requested: false,
                frames_asked: 0,
                text_area: None,
                compositions_ended: 0,
                translucent: None,
                frame_held: false,
                remember: false,
                layers: HashMap::new(),
                layer_places: 0,
                sweeps: Vec::new(),
                restore: false,
                restore_size: false,
                mouse_moves: false,
                pointer: PointerShape::Arrow,
                pointer_at: Point::default(),
                pointer_areas: Vec::new(),
                appearance: None,
                quick: Some(None),
                titlebar: Titlebar::default(),
                titlebar_areas: Vec::new(),
                accessibility_asked: false,
                accessibility: Vec::new(),
            },
        );
        Ok(id)
    }

    fn show_quick_window(&mut self, window: WindowId, place: Option<&QuickPlace>) {
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        let Some(shown) = &mut state.quick else {
            return;
        };
        match place {
            // Shown, it takes the place's frame, as the system's does at
            // once, and is no longer hidden, with the frame that it asked
            // for, as the backend tells at once; its other events follow as
            // a test sends them.
            Some(place) if shown.is_none() => {
                *shown = Some(*place);
                state.occluded = false;
                if std::mem::take(&mut state.frame_held) {
                    state.frame_requested = true;
                }
                let (frame, _) = place.frames(SCREEN, SCREEN_AREA);
                state.size = WindowSize::new(
                    Size {
                        width: frame.width,
                        height: frame.height,
                    },
                    self.scale,
                );
            }
            // Shown already, it keeps its place until it hides.
            Some(_) => {}
            None => *shown = None,
        }
    }

    fn set_global_keys(&mut self, keys: &[GlobalKey]) -> Vec<usize> {
        self.global_keys = keys.to_vec();
        // Another app's, or a function key past F20, which the Mac's
        // keyboards lack, as the backend refuses it.
        (0..keys.len())
            .filter(|&index| {
                self.taken_keys.contains(&keys[index])
                    || matches!(keys[index].code, crate::KeyCode::F(number) if number > 20)
            })
            .collect()
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

    fn set_window_remember(&mut self, window: WindowId, remember: bool) {
        if let Some(state) = self.windows.get_mut(&window) {
            state.remember = remember;
        }
    }

    fn window_size(&self, window: WindowId) -> Option<WindowSize> {
        self.windows.get(&window).map(|state| state.size)
    }

    fn refresh_interval(&self, window: WindowId) -> Option<Duration> {
        // A display of 60 Hz.
        self.windows
            .contains_key(&window)
            .then(|| Duration::from_secs(1) / 60)
    }

    fn window_handle(&self, _window: WindowId) -> Option<WindowHandle<'_>> {
        // Nothing draws into the fake's windows.
        None
    }

    fn layer_handle(&self, _window: WindowId, _layer: Layer) -> Option<WindowHandle<'_>> {
        None
    }

    fn place_layer(&mut self, window: WindowId, layer: Layer, place: Option<&LayerPlace>) {
        if let Some(state) = self.windows.get_mut(&window) {
            match place {
                Some(&place) => state.layers.insert(layer, place),
                None => state.layers.remove(&layer),
            };
            state.layer_places += 1;
        }
    }

    fn set_sweeps(&mut self, window: WindowId, sweeps: &[Sweep]) {
        if let Some(state) = self.windows.get_mut(&window) {
            state.sweeps = sweeps.to_vec();
        }
    }

    fn request_frame(&mut self, window: WindowId) {
        if let Some(state) = self.windows.get_mut(&window) {
            state.frames_asked += 1;
            if state.occluded {
                state.frame_held = true;
            } else {
                state.frame_requested = true;
            }
        }
    }

    fn focus_window(&mut self, window: WindowId) {
        let hidden_quick = self
            .windows
            .get(&window)
            .is_some_and(|state| state.quick == Some(None));
        if !hidden_quick {
            self.focus_requests.push(window);
        }
    }

    fn focused_window(&self) -> Option<WindowId> {
        self.focused
    }

    fn pointer_position(&self, window: WindowId) -> Option<Point> {
        self.windows.get(&window).map(|state| state.pointer_at)
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

    fn end_composition(&mut self, window: WindowId) {
        if let Some(state) = self.windows.get_mut(&window) {
            state.compositions_ended += 1;
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

    fn set_pointer_areas(&mut self, window: WindowId, areas: &[PointerArea]) {
        if let Some(state) = self.windows.get_mut(&window) {
            state.pointer_areas = areas.to_vec();
        }
    }

    fn set_titlebar(&mut self, window: WindowId, titlebar: Titlebar) {
        if let Some(state) = self.windows.get_mut(&window) {
            state.titlebar = titlebar;
        }
    }

    fn set_titlebar_areas(&mut self, window: WindowId, areas: &[ContentRect]) {
        if let Some(state) = self.windows.get_mut(&window) {
            state.titlebar_areas = areas.to_vec();
        }
    }

    fn accessibility_asked(&self, window: WindowId) -> bool {
        self.windows
            .get(&window)
            .is_some_and(|state| state.accessibility_asked)
    }

    fn update_accessibility(&mut self, window: WindowId, update: accesskit::TreeUpdate) {
        if let Some(state) = self.windows.get_mut(&window)
            && state.accessibility_asked
        {
            state.accessibility.push(update);
        }
    }

    fn set_secure_input(&mut self, on: bool) {
        if self.secure_input.0 != on {
            self.secure_input = (on, self.secure_input.1 + 1);
        }
    }

    fn start_updates(&mut self, check: bool) -> bool {
        if self.can_update && self.update_checks.is_none() {
            self.update_checks = Some(check);
        }
        self.can_update
    }

    fn set_update_checks(&mut self, check: bool) {
        if self.update_checks.is_some() {
            self.update_checks = Some(check);
        }
    }

    fn check_for_updates(&mut self) {
        if self.can_update {
            self.updates_checked += 1;
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

    fn set_window_appearance(&mut self, window: WindowId, appearance: Option<Appearance>) {
        if let Some(window) = self.windows.get_mut(&window) {
            window.appearance = appearance;
        }
    }

    /// A URL, unless of `file`; a directory, a program, as the shared rule
    /// tells of the file that the target's links lead to, or a file, at the
    /// path that its links lead to; a path that leads nowhere, a `file` URL
    /// of another machine and a text that is no URL fail, as a backend's
    /// do.
    fn resolve_target(&self, target: &str) -> Result<Target, Error> {
        let failed = |why: String| Error::Open(target.to_owned(), why);
        let path = match given(target) {
            Given::Url => {
                return Ok(Target {
                    kind: TargetKind::Url,
                    location: target.to_owned(),
                });
            }
            Given::Path(path) => path,
            Given::Neither(why) => return Err(failed(why.to_owned())),
        };
        let path = std::fs::canonicalize(path).map_err(|error| failed(error.to_string()))?;
        let metadata = std::fs::metadata(&path).map_err(|error| failed(error.to_string()))?;
        let kind = if crate::runs_by_itself(&path, &metadata) {
            TargetKind::Program
        } else if metadata.is_dir() {
            TargetKind::Directory
        } else {
            TargetKind::File
        };
        Ok(Target {
            kind,
            location: path.display().to_string(),
        })
    }

    /// Opens nothing, and records `target`'s location; a program, and a
    /// file or a directory of no absolute path, fail.
    fn system_version(&self) -> String {
        "Fake 1.0".to_owned()
    }

    fn open(&mut self, target: &Target) -> Result<(), Error> {
        if target.kind != TargetKind::Url && !Path::new(&target.location).is_absolute() {
            return Err(Error::Open(
                target.location.clone(),
                "it has no path".to_owned(),
            ));
        }
        if target.kind == TargetKind::Program {
            return Err(Error::Open(
                target.location.clone(),
                "it would run".to_owned(),
            ));
        }
        self.opened.push(target.location.clone());
        Ok(())
    }

    /// Shows nothing, and records `target`'s location; a URL, and a target
    /// of no absolute path, fail.
    fn reveal(&mut self, target: &Target) -> Result<(), Error> {
        if target.kind != TargetKind::Url && !Path::new(&target.location).is_absolute() {
            return Err(Error::Open(
                target.location.clone(),
                "it has no path".to_owned(),
            ));
        }
        if target.kind == TargetKind::Url {
            return Err(Error::Open(
                target.location.clone(),
                "it is no file".to_owned(),
            ));
        }
        self.revealed.push(target.location.clone());
        Ok(())
    }

    fn open_in_text_editor(&mut self, path: &Path) -> Result<(), Error> {
        if let Some(why) = &self.editor_fails {
            return Err(Error::Open(path.display().to_string(), why.clone()));
        }
        self.edited.push(path.to_owned());
        Ok(())
    }
}

/// What a target is, as the fake reads it.
enum Given {
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
fn given(target: &str) -> Given {
    if target.starts_with('/') {
        return Given::Path(PathBuf::from(target));
    }
    let Some((scheme, rest)) = target.split_once(':') else {
        return Given::Neither("it is not a URL");
    };
    let scheme_ok = scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c));
    if !scheme_ok {
        return Given::Neither("it is not a URL");
    }
    if !scheme.eq_ignore_ascii_case("file") {
        return Given::Url;
    }
    let path = match rest.strip_prefix("//") {
        Some(rest) => {
            let (host, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
            if !host.is_empty() && !host.eq_ignore_ascii_case("localhost") {
                return Given::Neither("it is on another machine");
            }
            path
        }
        None => rest,
    };
    match decoded(path) {
        Some(path) if path.starts_with('/') => Given::Path(PathBuf::from(path)),
        _ => Given::Neither("it has no path"),
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
