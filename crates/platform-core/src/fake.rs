//! A platform without a window system, for tests. It keeps what the app
//! asks of it, and hands the app the events that a test sends, once it has
//! applied what they change, as a backend would.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

use crate::{
    AboutPanel, Alert, AlertId, AppEvent, Appearance, Awake, Backdrop, ContentRect, ContextMenu,
    DoubleTap, DoubleTaps, Error, Fill, GlobalKey, Handler, Held, Layer, LayerPlace, MainMenu,
    Modifier, Platform, Point, PointerArea, PointerShape, QuickPlace, SavePanelId, ScreenPoint,
    ScreenRect, Scrollers, Shortcut, Size, Sweep, Target, TargetKind, TimerId, Titlebar, ToolLink,
    Usage, Waker, WindowAttributes, WindowEvent, WindowHandle, WindowId, WindowSize,
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

/// Where the fake puts the first window's content's top left corner on the
/// screen, and how far down and to the right of it each next one goes.
const FIRST_ORIGIN: ScreenPoint = ScreenPoint { x: 100.0, y: 100.0 };
const CASCADE: f64 = 22.0;

/// A window that the app opened, as the fake keeps it.
#[derive(Clone, Debug, PartialEq)]
pub struct FakeWindow {
    /// Its title.
    pub title: String,
    /// The text at its titlebar's right, as the keys that bring it forward.
    pub title_accessory: Option<String>,
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
    /// The earliest time that the app asked for its frame at, with
    /// [`Platform::request_frame_at`], until the frame comes.
    pub frame_at: Option<Instant>,
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
    /// The fills that the app showed last.
    pub fills: Vec<Fill>,
    /// How many times the app showed fills.
    pub fills_shown: usize,
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
    /// The shape that the app holds the pointer at, while it holds it.
    pub held_pointer: Option<PointerShape>,
    /// Its own appearance, if the app gave it one.
    pub appearance: Option<Appearance>,
    /// Whether it is a quick window, and its place while it shows.
    pub quick: Option<Option<QuickPlace>>,
    /// Its titlebar, as the app gave it, which changes no size of the fake
    /// one's.
    pub titlebar: Titlebar,
    /// The room of its titlebar for the content's small buttons, in points,
    /// as the app gave it.
    pub title_buttons_room: f64,
    /// The parts of its titlebar that the app gave as its content's.
    pub titlebar_areas: Vec<ContentRect>,
    /// The parts of the content where the pointer's moves come, as the app
    /// gave them last.
    pub move_areas: Vec<ContentRect>,
    /// Whether an accessibility client asked for its tree, as a test says.
    pub accessibility_asked: bool,
    /// The updates of its accessibility tree that the app gave, once a
    /// client asked, in their order.
    pub accessibility: Vec<accesskit::TreeUpdate>,
    /// Where its content's top left corner is on the screen.
    pub origin: ScreenPoint,
    /// Whether it floats above the other applications' windows.
    pub floating: bool,
    /// Whether the app hid it, as the windows of a session that is not
    /// active hide.
    pub hidden: bool,
}

impl FakeWindow {
    /// Its content's frame on the screen.
    pub fn frame(&self) -> ScreenRect {
        ScreenRect {
            x: self.origin.x,
            y: self.origin.y,
            width: self.size.points.width,
            height: self.size.points.height,
        }
    }
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
    /// The app's number of it.
    pub id: u64,
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
    /// The screens' headroom past white, as [`Fake::set_screen_headroom`]
    /// sets it.
    screen_headroom: f32,
    scrollers: Scrollers,
    /// Whether the system's settings have an app's windows open again.
    keeps_windows: bool,
    /// Whether the system started the app of its own.
    default_launch: bool,
    /// The files that came from the internet, as a test says.
    downloaded: Vec<PathBuf>,
    /// Whether the app is the system's default terminal, and how many
    /// times it asked to be.
    default_terminal: bool,
    default_terminal_asks: usize,
    /// The links that the app asked for, each of the reason that it told.
    tool_links: Vec<(Vec<ToolLink>, String)>,
    /// Whether every place is one that goes, as a test says.
    temporary_place: bool,
    /// The screens' areas for windows, the menu bar's screen's first.
    screens: Vec<ScreenRect>,
    /// Whether it draws the progresses' fills, as a test says.
    draws_fills: bool,
    windows: BTreeMap<WindowId, FakeWindow>,
    /// The windows from the front one back, as they opened and came
    /// forward.
    order: Vec<WindowId>,
    /// The drag's image, while it shows, and the places that it slid back
    /// to as it hid, from the first, `None` of one hidden at once.
    drag_image: Option<ScreenRect>,
    drag_image_hides: Vec<Option<ScreenRect>>,
    focused: Option<WindowId>,
    menu: Option<MainMenu>,
    /// How many times the app set the main menu, and how many windows were
    /// open as it first did.
    menu_sets: usize,
    windows_at_first_menu: Option<usize>,
    /// The commands whose menu items are dimmed, and those whose items are
    /// ticked.
    disabled: std::collections::BTreeSet<String>,
    checked: std::collections::BTreeSet<String>,
    /// The context menus that the app opened, from the first, each over its
    /// window at its position.
    context_menus: Vec<(WindowId, Point, ContextMenu)>,
    closed_context_menus: Vec<WindowId>,
    /// The height of the titlebar of the windows that open, which their
    /// content runs under.
    bar: f64,
    /// The definitions that the app showed, from the first.
    definitions: Vec<FakeDefinition>,
    clipboard: Option<String>,
    /// The image on the clipboard, a PNG, which takes its text's place.
    clipboard_image: Option<Vec<u8>>,
    /// What the app keeps from idle sleep, and the assertion's name.
    awake: (Awake, String),
    /// The save panels that are open: each its id, its window and its
    /// file's name.
    save_panels: Vec<(SavePanelId, WindowId, String, String)>,
    /// Whether the resolutions of targets off the main thread wait, as of
    /// a file system that does not answer, and what lets them go.
    held_targets: Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>,
    /// The save panels whose windows closed, whose ends are to tell.
    ended_saves: Vec<SavePanelId>,
    /// The open panels shown, each its window, its extensions and its
    /// message, from the first.
    open_panels: Vec<(WindowId, Vec<String>, String)>,
    alerts: Vec<FakeAlert>,
    /// The timers that have not fired, with the time of each.
    timers: BTreeMap<TimerId, Instant>,
    /// Which Option keys act as Meta.
    option_as_meta: (bool, bool),
    /// Whether the keys are recorded, as the app last told.
    recording_keys: bool,
    /// Whether a field that edits text has the keyboard, as the app told.
    editing_text: bool,
    /// The icons that the app showed, from the first.
    app_icons: Vec<Option<PathBuf>>,
    /// The user's languages and regions, none unless a test sets them.
    locales: Vec<String>,
    /// How many times the app asked for the user's attention, and played
    /// the alert sound.
    attention_requests: usize,
    /// The badge on the application's icon.
    badge: Option<String>,
    alert_sounds: usize,
    /// The notifications that the app showed, from the first.
    notifications: Vec<FakeNotification>,
    /// The answers that the app gave the automation's requests, in their
    /// order.
    automation_answers: Vec<(u64, crate::automation::Answer)>,
    /// The windows that the app asked to bring forward, from the first.
    focus_requests: Vec<WindowId>,
    /// The URLs and the paths that the app opened, and those that it
    /// showed in the file manager, from the first.
    opened: Vec<String>,
    /// The URLs' schemes of no application, whose opening fails, as of an
    /// editor not installed.
    unhandled: Vec<String>,
    edited: Vec<PathBuf>,
    /// Why the text editor opens nothing, as the next open fails.
    editor_fails: Option<String>,
    /// The files moved to the Trash, from the first, which the fake
    /// removes.
    trashed: Vec<PathBuf>,
    /// Why a move to the Trash fails, as the next one does.
    trash_fails: Option<String>,
    /// The color panel that shows, of its color and whether it shows its
    /// opacity's slider.
    color_panel: Option<(crate::PanelColor, bool)>,
    /// Whether the secure entry of the keyboard is on, and how many times
    /// it changed.
    secure_input: (bool, usize),
    /// Whether the app can update itself, as a test says.
    can_update: bool,
    /// The updates' checks now and then, on or off, once started.
    update_checks: Option<bool>,
    /// The checks for updates that the app asked.
    updates_checked: usize,
    abouts: Vec<AboutPanel>,
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
    /// How many times the app gave its global keys.
    global_key_sets: usize,
    taken_keys: Vec<GlobalKey>,
    /// The modifiers whose double taps the app watches, whether the
    /// system's leave to watch them is given, and the finder of their
    /// taps, of the Mac's default double-click interval.
    double_taps: Vec<Modifier>,
    taps_allowed: bool,
    /// Whether the system gives the app no tap of the modifiers.
    taps_failing: bool,
    taps: DoubleTap,
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
            screen_headroom: 1.0,
            scrollers: Scrollers::default(),
            keeps_windows: false,
            default_launch: true,
            downloaded: Vec::new(),
            default_terminal: false,
            default_terminal_asks: 0,
            tool_links: Vec::new(),
            temporary_place: false,
            screens: vec![SCREEN_AREA],
            draws_fills: false,
            windows: BTreeMap::new(),
            order: Vec::new(),
            drag_image: None,
            drag_image_hides: Vec::new(),
            focused: None,
            menu: None,
            menu_sets: 0,
            windows_at_first_menu: None,
            disabled: std::collections::BTreeSet::new(),
            checked: std::collections::BTreeSet::new(),
            context_menus: Vec::new(),
            closed_context_menus: Vec::new(),
            bar: 0.0,
            definitions: Vec::new(),
            clipboard: None,
            clipboard_image: None,
            save_panels: Vec::new(),
            held_targets: Arc::default(),
            ended_saves: Vec::new(),
            open_panels: Vec::new(),
            awake: (Awake::Off, String::new()),
            alerts: Vec::new(),
            timers: BTreeMap::new(),
            option_as_meta: (false, false),
            recording_keys: false,
            editing_text: false,
            app_icons: Vec::new(),
            locales: Vec::new(),
            attention_requests: 0,
            badge: None,
            alert_sounds: 0,
            notifications: Vec::new(),
            automation_answers: Vec::new(),
            focus_requests: Vec::new(),
            opened: Vec::new(),
            unhandled: Vec::new(),
            edited: Vec::new(),
            editor_fails: None,
            trashed: Vec::new(),
            trash_fails: None,
            color_panel: None,
            secure_input: (false, 0),
            can_update: false,
            update_checks: None,
            updates_checked: 0,
            abouts: Vec::new(),
            usage: None,
            started: SystemTime::now(),
            posted_keys: Vec::new(),
            revealed: Vec::new(),
            global_keys: Vec::new(),
            global_key_sets: 0,
            taken_keys: Vec::new(),
            double_taps: Vec::new(),
            taps_allowed: true,
            taps_failing: false,
            taps: DoubleTap::new(std::time::Duration::from_millis(500)),
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

    /// Has every window's screen show colors as far as `headroom` times
    /// white.
    pub fn set_screen_headroom(&mut self, headroom: f32) {
        self.screen_headroom = headroom;
    }

    /// Turns the system's reduced motion on or off.
    pub fn set_reduce_motion(&mut self, on: bool) {
        self.reduce_motion = on;
    }

    /// Gives the system's settings of its scroll bars.
    pub fn set_scrollers(&mut self, scrollers: Scrollers) {
        self.scrollers = scrollers;
    }

    /// Gives whether the system's settings have an app's windows open
    /// again as it starts.
    pub fn set_keeps_windows(&mut self, keeps: bool) {
        self.keeps_windows = keeps;
    }

    /// Gives whether the system started the app of its own, rather than to
    /// open files or for a service.
    pub fn set_default_launch(&mut self, default: bool) {
        self.default_launch = default;
    }

    /// Marks the file at `path` as one that came from the internet.
    pub fn set_downloaded(&mut self, path: &Path) {
        self.downloaded.push(path.to_path_buf());
    }

    /// Gives whether the app is the system's default terminal.
    pub fn set_default_terminal(&mut self, default: bool) {
        self.default_terminal = default;
    }

    /// Gives whether every place is one that goes, as a disk image's.
    pub fn set_temporary_place(&mut self, temporary: bool) {
        self.temporary_place = temporary;
    }

    /// How many times the app asked to be the system's default terminal.
    pub fn default_terminal_asks(&self) -> usize {
        self.default_terminal_asks
    }

    /// The links of the command line tools that the app asked for, each of
    /// the reason that it told, which the fake does not make.
    pub fn tool_links(&self) -> &[(Vec<ToolLink>, String)] {
        &self.tool_links
    }

    /// Gives the screens' areas for windows, the menu bar's screen's
    /// first, in place of [`SCREEN_AREA`] alone, as a screen comes or goes.
    pub fn set_screens(&mut self, areas: Vec<ScreenRect>) {
        self.screens = areas;
    }

    /// Starts the application, as a backend's `run` does: its start, then
    /// the launch's end.
    pub fn start(&mut self, handler: &mut dyn Handler) {
        self.start_opening(handler, Vec::new());
    }

    /// Starts the application as the system does to open files, folders
    /// or links: its start, `events` that came before it, then the
    /// launch's end.
    pub fn start_opening(&mut self, handler: &mut dyn Handler, events: Vec<AppEvent>) {
        handler.started(self);
        for event in events {
            self.send_app(handler, event);
        }
        if !self.quit {
            handler.launched(self);
        }
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
            WindowEvent::Frame => {
                state.frame_requested = false;
                state.frame_at = None;
            }
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

    /// The save panels that are open, in the order that they opened: each
    /// its id, its window, its file's name and its message.
    pub fn save_panels(&self) -> &[(SavePanelId, WindowId, String, String)] {
        &self.save_panels
    }

    /// Holds the resolutions of targets off the main thread, as a file
    /// system that does not answer would, of `true`, or lets them go.
    pub fn hold_targets(&self, held: bool) {
        let (lock, released) = &*self.held_targets;
        *lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = held;
        released.notify_all();
    }

    /// The open panels that the app showed, each its window, its
    /// extensions and its message, from the first.
    pub fn open_panels(&self) -> &[(WindowId, Vec<String>, String)] {
        &self.open_panels
    }

    /// Closes the open panel as the user chose the files `paths`, none as
    /// they cancelled, and tells `handler`.
    pub fn choose(&mut self, handler: &mut dyn Handler, paths: Vec<PathBuf>) -> bool {
        self.send_app(handler, AppEvent::FilesChosen(paths))
    }

    /// Closes the save panel `panel` as the user chose `path`, or
    /// cancelled, of none, and tells `handler`; false if it is not open.
    pub fn close_save_panel(
        &mut self,
        handler: &mut dyn Handler,
        panel: SavePanelId,
        path: Option<PathBuf>,
    ) -> bool {
        let Some(index) = self.save_panels.iter().position(|open| open.0 == panel) else {
            return false;
        };
        self.save_panels.remove(index);
        self.send_app(handler, AppEvent::SavePanelClosed { panel, path })
    }

    /// Tells `handler` the ends of the save panels whose windows closed,
    /// of no path, as the platform ends a window's sheets with it: how
    /// many it told.
    pub fn deliver_ended_saves(&mut self, handler: &mut dyn Handler) -> usize {
        let ended = std::mem::take(&mut self.ended_saves);
        for &panel in &ended {
            self.send_app(handler, AppEvent::SavePanelClosed { panel, path: None });
        }
        ended.len()
    }

    /// What the app keeps from idle sleep, and its assertion's name.
    pub fn awake(&self) -> (Awake, &str) {
        (self.awake.0, &self.awake.1)
    }

    /// The image on the clipboard, a PNG, if one is there.
    pub fn clipboard_image(&self) -> Option<&[u8]> {
        self.clipboard_image.as_deref()
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

    /// Moves the pointer to `at` over `window` with no event, as the
    /// system's pointer goes where the app takes no moves.
    pub fn set_pointer(&mut self, window: WindowId, at: Point) {
        if let Some(state) = self.windows.get_mut(&window) {
            state.pointer_at = at;
        }
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

    /// Has the fake draw the progresses' fills, as a window server would,
    /// or leave them to the app, as by default.
    pub fn set_draws_fills(&mut self, draws: bool) {
        self.draws_fills = draws;
    }

    /// Brings `window` in front of the others with no event, as the user
    /// clicks it.
    pub fn raise(&mut self, window: WindowId) {
        if self.windows.contains_key(&window) {
            self.order.retain(|&open| open != window);
            self.order.insert(0, window);
        }
    }

    /// The drag's image's frame on the screen, while it shows.
    pub fn drag_image(&self) -> Option<ScreenRect> {
        self.drag_image
    }

    /// Where the drag's image slid as it hid, each time, from the first:
    /// `None` of one hidden at once.
    pub fn drag_image_hides(&self) -> &[Option<ScreenRect>] {
        &self.drag_image_hides
    }

    /// The main menu that the app set, if it set one.
    pub fn main_menu(&self) -> Option<&MainMenu> {
        self.menu.as_ref()
    }

    /// How many times the app set the main menu.
    pub fn main_menu_sets(&self) -> usize {
        self.menu_sets
    }

    /// How many windows were open as the app first set the main menu, if
    /// it set one.
    pub fn windows_at_first_menu(&self) -> Option<usize> {
        self.windows_at_first_menu
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

    /// The windows whose context menus the app closed, from the first,
    /// whose closings the tests send.
    pub fn closed_context_menus(&self) -> &[WindowId] {
        &self.closed_context_menus
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

    /// Whether the keys are recorded, as the app last told.
    pub fn recording_keys(&self) -> bool {
        self.recording_keys
    }

    /// Whether the app told that a field that edits text has the keyboard.
    pub fn editing_text(&self) -> bool {
        self.editing_text
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

    /// The badge on the application's icon, if one shows.
    pub fn badge(&self) -> Option<&str> {
        self.badge.as_deref()
    }

    /// How many times the app played the alert sound.
    pub fn alert_sounds(&self) -> usize {
        self.alert_sounds
    }

    /// The answers that the app gave the automation's requests, in their
    /// order.
    pub fn automation_answers(&self) -> &[(u64, crate::automation::Answer)] {
        &self.automation_answers
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

    /// Has no application open the URLs of `scheme`, as of an editor that
    /// is not installed: their opening fails.
    pub fn leave_unhandled(&mut self, scheme: &str) {
        self.unhandled.push(scheme.to_owned());
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

    /// The files that the app moved to the Trash, in order.
    pub fn trashed(&self) -> &[PathBuf] {
        &self.trashed
    }

    /// The color panel that shows, of its color as the app gave it and
    /// whether it shows its opacity's slider; none where it does not.
    pub fn color_panel(&self) -> Option<(crate::PanelColor, bool)> {
        self.color_panel
    }

    /// Has the next moves to the Trash fail, telling `why`, or succeed
    /// again of none.
    pub fn fail_trash(&mut self, why: Option<&str>) {
        self.trash_fails = why.map(str::to_owned);
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

    /// The About panels that the app opened, in their order.
    pub fn abouts(&self) -> &[AboutPanel] {
        &self.abouts
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

    /// How many times the app gave its global keys.
    pub fn global_key_sets(&self) -> usize {
        self.global_key_sets
    }

    /// Has another app take `key`, which the app cannot take from then on.
    pub fn take_global_key(&mut self, key: GlobalKey) {
        self.taken_keys.push(key);
    }

    /// The modifiers whose double taps the app watches, as it asked last.
    pub fn double_taps(&self) -> &[Modifier] {
        &self.double_taps
    }

    /// Gives or takes away the system's leave to see the modifiers in other
    /// apps, which the fake has given by default.
    pub fn allow_double_taps(&mut self, allowed: bool) {
        self.taps_allowed = allowed;
    }

    /// Has the system give the app no tap of the modifiers, with its leave,
    /// or give it again: the double taps' watch fails.
    pub fn fail_double_taps(&mut self, failing: bool) {
        self.taps_failing = failing;
    }

    /// Watches the double taps again, as the app becomes active, as the
    /// Mac's platform does where it watches some: the app hears its answer,
    /// [`AppEvent::DoubleTapsWatched`].
    pub fn watch_double_taps_again(&mut self, handler: &mut dyn Handler) {
        if !self.double_taps.is_empty() {
            let answer = self.taps_answer();
            self.send_app(handler, AppEvent::DoubleTapsWatched(answer));
        }
    }

    /// What a watch of the double taps answers now.
    fn taps_answer(&self) -> DoubleTaps {
        if self.double_taps.is_empty() {
            DoubleTaps::Off
        } else if !self.taps_allowed {
            DoubleTaps::NotAllowed
        } else if self.taps_failing {
            DoubleTaps::Failed
        } else {
            DoubleTaps::Watching
        }
    }

    /// Changes the modifiers held to `held` at `now`, of no key nor click
    /// for `idle`, as the system tells it: a double tap of a modifier that
    /// the app watches, with the system's leave, comes to it as
    /// [`AppEvent::DoubleTap`]. Whether one came.
    pub fn change_modifiers(
        &mut self,
        handler: &mut dyn Handler,
        held: Held,
        now: std::time::Instant,
        idle: std::time::Duration,
    ) -> bool {
        let tapped = self.taps.change(held, now, idle);
        match tapped {
            Some(modifier) if self.taps_allowed && self.double_taps.contains(&modifier) => {
                self.send_app(handler, AppEvent::DoubleTap(modifier));
                true
            }
            _ => false,
        }
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

    fn screen_headroom(&self, _window: WindowId) -> f32 {
        self.screen_headroom
    }

    fn scrollers(&self) -> Scrollers {
        self.scrollers
    }

    fn keeps_windows(&self) -> bool {
        self.keeps_windows
    }

    fn default_launch(&self) -> bool {
        self.default_launch
    }

    fn downloaded(&self, path: &Path) -> bool {
        self.downloaded.iter().any(|marked| marked == path) || quarantined(path)
    }

    fn quarantine(&self) -> crate::Quarantine {
        quarantine
    }

    fn is_default_terminal(&self) -> bool {
        self.default_terminal
    }

    fn make_default_terminal(&mut self) -> bool {
        self.default_terminal_asks += 1;
        true
    }

    fn link_tools(&mut self, links: Vec<ToolLink>, reason: &str) {
        self.tool_links.push((links, reason.to_owned()));
    }

    fn is_temporary_place(&self, _path: &Path) -> bool {
        self.temporary_place
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
        // Down and to the right of the front window, as the system cascades
        // them.
        let origin = self
            .order
            .iter()
            .find_map(|window| {
                self.windows
                    .get(window)
                    .filter(|state| state.quick.is_none())
            })
            .map_or(FIRST_ORIGIN, |front| ScreenPoint {
                x: front.origin.x + CASCADE,
                y: front.origin.y + CASCADE,
            });
        // Or at the frame asked for, on a screen that is there.
        let frame = attributes.frame.map(|frame| frame.onto(&self.screens));
        let origin = frame.map_or(origin, |frame| frame.origin());
        let points = frame.map_or(
            Size {
                width: attributes.size.width,
                height: attributes.size.height + self.bar,
            },
            |frame| Size {
                width: frame.width,
                height: frame.height,
            },
        );
        self.windows.insert(
            id,
            FakeWindow {
                title: attributes.title.clone(),
                title_accessory: None,
                size: WindowSize::new(points, self.scale).with_bar(self.bar),
                occluded: false,
                frame_requested: false,
                frames_asked: 0,
                frame_at: None,
                text_area: None,
                compositions_ended: 0,
                translucent: None,
                frame_held: false,
                remember: attributes.remember,
                layers: HashMap::new(),
                layer_places: 0,
                sweeps: Vec::new(),
                fills: Vec::new(),
                fills_shown: 0,
                restore: attributes.restore,
                restore_size: attributes.restore_size,
                mouse_moves: false,
                pointer: PointerShape::Arrow,
                pointer_at: Point::default(),
                pointer_areas: Vec::new(),
                held_pointer: None,
                appearance: None,
                quick: None,
                titlebar: Titlebar::default(),
                title_buttons_room: 0.0,
                titlebar_areas: Vec::new(),
                move_areas: Vec::new(),
                accessibility_asked: false,
                accessibility: Vec::new(),
                origin,
                floating: false,
                hidden: false,
            },
        );
        self.order.insert(0, id);
        Ok(id)
    }

    fn create_quick_window(&mut self, title: &str) -> Result<WindowId, Error> {
        let id = WindowId::new(self.next_id());
        self.windows.insert(
            id,
            FakeWindow {
                title: title.to_owned(),
                title_accessory: None,
                size: WindowSize::new(QUICK_SIZE, self.scale),
                occluded: true,
                frame_requested: false,
                frames_asked: 0,
                frame_at: None,
                text_area: None,
                compositions_ended: 0,
                translucent: None,
                frame_held: false,
                remember: false,
                layers: HashMap::new(),
                layer_places: 0,
                sweeps: Vec::new(),
                fills: Vec::new(),
                fills_shown: 0,
                restore: false,
                restore_size: false,
                mouse_moves: false,
                pointer: PointerShape::Arrow,
                pointer_at: Point::default(),
                pointer_areas: Vec::new(),
                held_pointer: None,
                appearance: None,
                quick: Some(None),
                titlebar: Titlebar::default(),
                title_buttons_room: 0.0,
                titlebar_areas: Vec::new(),
                move_areas: Vec::new(),
                accessibility_asked: false,
                accessibility: Vec::new(),
                origin: ScreenPoint::default(),
                floating: false,
                hidden: false,
            },
        );
        self.order.push(id);
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
                state.origin = frame.origin();
            }
            // Shown already, it keeps its place until it hides.
            Some(_) => return,
            None => {
                *shown = None;
                return;
            }
        }
        self.raise(window);
    }

    fn watch_double_taps(&mut self, modifiers: &[Modifier]) -> DoubleTaps {
        self.double_taps = modifiers.to_vec();
        self.taps_answer()
    }

    fn set_global_keys(&mut self, keys: &[GlobalKey]) -> Vec<usize> {
        self.global_keys = keys.to_vec();
        self.global_key_sets += 1;
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
        self.order.retain(|&open| open != window);
        if self.focused == Some(window) {
            self.focused = None;
        }
        // An alert on a window closes with it, unanswered; a save panel
        // ends, of no path, as its turn comes.
        self.alerts.retain(|open| open.window != Some(window));
        let (ended, kept) = std::mem::take(&mut self.save_panels)
            .into_iter()
            .partition::<Vec<_>, _>(|open| open.1 == window);
        self.save_panels = kept;
        self.ended_saves
            .extend(ended.into_iter().map(|open| open.0));
    }

    fn set_title(&mut self, window: WindowId, title: &str) {
        if let Some(state) = self.windows.get_mut(&window) {
            title.clone_into(&mut state.title);
        }
    }

    fn set_title_accessory(&mut self, window: WindowId, text: Option<&str>) {
        if let Some(state) = self.windows.get_mut(&window) {
            state.title_accessory = text.map(str::to_owned);
        }
    }

    fn shortcut_writer(&self) -> fn(Shortcut) -> String {
        |shortcut| shortcut.mac_text()
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

    fn draws_fills(&self) -> bool {
        self.draws_fills
    }

    fn set_fills(&mut self, window: WindowId, fills: &[Fill]) {
        if let Some(state) = self.windows.get_mut(&window) {
            state.fills = fills.to_vec();
            state.fills_shown += 1;
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

    fn request_frame_at(&mut self, window: WindowId, at: Instant) {
        self.request_frame(window);
        if let Some(state) = self.windows.get_mut(&window)
            && !state.occluded
        {
            state.frame_at = Some(state.frame_at.map_or(at, |asked| asked.min(at)));
        }
    }

    fn focus_window(&mut self, window: WindowId) {
        let hidden_quick = self
            .windows
            .get(&window)
            .is_some_and(|state| state.quick == Some(None));
        if !hidden_quick {
            // A window that the app hid shows again, as the system's.
            if self.windows.get(&window).is_some_and(|state| state.hidden) {
                self.set_window_hidden(window, false);
            }
            self.focus_requests.push(window);
            self.raise(window);
        }
    }

    fn set_window_hidden(&mut self, window: WindowId, hidden: bool) {
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        if state.quick.is_some() || state.hidden == hidden {
            return;
        }
        // Hidden, it does not show, as the backend tells at once; shown, it
        // shows again, with the frame that it asked for meanwhile, its other
        // events following as a test sends them.
        state.hidden = hidden;
        state.occluded = hidden;
        if hidden && std::mem::take(&mut state.frame_requested) {
            state.frame_held = true;
        }
        if !hidden && std::mem::take(&mut state.frame_held) {
            state.frame_requested = true;
        }
        if !hidden {
            self.raise(window);
        }
    }

    fn focused_window(&self) -> Option<WindowId> {
        self.focused
    }

    fn pointer_position(&self, window: WindowId) -> Option<Point> {
        self.windows.get(&window).map(|state| state.pointer_at)
    }

    fn window_frame(&self, window: WindowId) -> Option<ScreenRect> {
        self.windows.get(&window).map(FakeWindow::frame)
    }

    fn windowed_frame(&self, window: WindowId) -> Option<ScreenRect> {
        self.window_frame(window)
    }

    fn set_window_floating(&mut self, window: WindowId, floating: bool) {
        if let Some(state) = self.windows.get_mut(&window) {
            state.floating = floating;
        }
    }

    fn set_window_origin(&mut self, window: WindowId, origin: ScreenPoint) {
        if let Some(state) = self.windows.get_mut(&window) {
            state.origin = origin;
        }
    }

    fn window_at(&self, at: ScreenPoint) -> Option<WindowId> {
        self.order.iter().copied().find(|window| {
            self.windows
                .get(window)
                .is_some_and(|state| state.quick != Some(None) && state.frame().contains(at))
        })
    }

    fn show_drag_image(&mut self, frame: ScreenRect) {
        self.drag_image = Some(frame);
    }

    fn drag_image_handle(&self) -> Option<WindowHandle<'_>> {
        // Nothing draws into the fake's images.
        None
    }

    fn hide_drag_image(&mut self, to: Option<ScreenRect>) {
        if self.drag_image.take().is_some() {
            self.drag_image_hides.push(to);
        }
    }

    fn set_main_menu(&mut self, menu: &MainMenu) {
        self.menu = Some(menu.clone());
        self.menu_sets += 1;
        self.windows_at_first_menu.get_or_insert(self.windows.len());
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

    fn close_context_menu(&mut self, window: WindowId) {
        self.closed_context_menus.push(window);
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
        self.clipboard_image = None;
    }

    fn set_clipboard_image(&mut self, png: &[u8]) {
        self.clipboard = None;
        self.clipboard_image = Some(png.to_vec());
    }

    fn keep_awake(&mut self, awake: Awake, reason: &str) {
        self.awake = (awake, reason.to_owned());
    }

    fn show_save_panel(&mut self, window: WindowId, name: &str, message: &str) -> SavePanelId {
        let id = SavePanelId::new(self.next_id());
        self.save_panels
            .push((id, window, name.to_owned(), message.to_owned()));
        id
    }

    fn dismiss_save_panel(&mut self, panel: SavePanelId) {
        self.save_panels.retain(|open| open.0 != panel);
        self.ended_saves.retain(|&ended| ended != panel);
    }

    fn choose_files(&mut self, window: WindowId, extensions: &[&str], message: &str) {
        let extensions = extensions
            .iter()
            .map(|&extension| extension.to_owned())
            .collect();
        self.open_panels
            .push((window, extensions, message.to_owned()));
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

    fn dismiss_alert(&mut self, alert: AlertId) {
        self.alerts.retain(|open| open.id != alert);
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

    fn set_recording_keys(&mut self, on: bool) {
        self.recording_keys = on;
    }

    fn set_editing_text(&mut self, on: bool) {
        self.editing_text = on;
    }

    fn track_mouse_moves(&mut self, window: WindowId, track: bool) {
        if let Some(state) = self.windows.get_mut(&window) {
            state.mouse_moves = track;
        }
    }

    fn hold_pointer(&mut self, window: WindowId, shape: Option<PointerShape>) {
        if let Some(state) = self.windows.get_mut(&window) {
            state.held_pointer = shape;
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

    fn set_title_buttons_room(&mut self, window: WindowId, width: f64) {
        if let Some(state) = self.windows.get_mut(&window) {
            state.title_buttons_room = width;
        }
    }

    fn set_move_areas(&mut self, window: WindowId, areas: &[ContentRect]) {
        if let Some(state) = self.windows.get_mut(&window) {
            state.move_areas = areas.to_vec();
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

    fn show_about(&mut self, about: &AboutPanel) {
        self.abouts.push(about.clone());
    }

    fn set_pointer_shape(&mut self, window: WindowId, shape: PointerShape) {
        if let Some(state) = self.windows.get_mut(&window) {
            state.pointer = shape;
        }
    }

    /// Two by two opaque white pixels of any data but none.
    fn decode_image(&self, data: &[u8]) -> Option<crate::Pixels> {
        (!data.is_empty()).then(|| crate::Pixels {
            width: 2,
            height: 2,
            rgba: vec![255; 16],
        })
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

    fn set_badge(&mut self, badge: Option<&str>) {
        self.badge = badge.map(str::to_owned);
    }

    fn play_alert_sound(&mut self) {
        self.alert_sounds += 1;
    }

    fn answer_automation(&mut self, id: u64, answer: crate::automation::Answer) {
        self.automation_answers.push((id, answer));
    }

    fn notify(&mut self, window: WindowId, id: u64, title: &str, body: &str) {
        self.notifications.push(FakeNotification {
            window,
            id,
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
        resolve(target)
    }

    /// [`resolve`], once the targets that the test holds are let go.
    fn target_resolver(&self) -> crate::TargetResolver {
        let held = Arc::clone(&self.held_targets);
        Arc::new(move |target: &str| {
            let (lock, released) = &*held;
            let mut holding = lock
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            while *holding {
                holding = released
                    .wait(holding)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
            drop(holding);
            resolve(target)
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
        if target.kind == TargetKind::Url
            && target
                .location
                .split_once(':')
                .is_some_and(|(scheme, _)| self.unhandled.iter().any(|kept| kept == scheme))
        {
            return Err(Error::Open(
                target.location.clone(),
                "no application opens it".to_owned(),
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

    fn show_color_panel(&mut self, color: crate::PanelColor, alpha: bool) {
        self.color_panel = Some((color, alpha));
    }

    fn close_color_panel(&mut self) {
        self.color_panel = None;
    }

    fn trash(&mut self, path: &Path) -> Result<(), Error> {
        let failed = |why: String| Error::Trash(path.display().to_string(), why);
        if let Some(why) = &self.trash_fails {
            return Err(failed(why.clone()));
        }
        std::fs::remove_file(path).map_err(|error| failed(error.to_string()))?;
        self.trashed.push(path.to_owned());
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

/// The files that the fake's [`Platform::quarantine`] marked, by their
/// device and inode, which a rename keeps, as the Mac's attribute.
static QUARANTINED: std::sync::Mutex<Vec<(u64, u64)>> = std::sync::Mutex::new(Vec::new());

/// The fake's mark of a file come from elsewhere: its device and inode
/// kept, which [`quarantined`] finds again.
fn quarantine(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::metadata(path)?;
    QUARANTINED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push((metadata.dev(), metadata.ino()));
    Ok(())
}

/// Whether the fake's [`Platform::quarantine`] marked the file at `path`,
/// whatever its name since.
pub fn quarantined(path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    QUARANTINED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .contains(&(metadata.dev(), metadata.ino()))
}

/// A URL, unless of `file`; a directory, a program, as the shared rule
/// tells of the file that the target's links lead to, or a file, at the
/// path that its links lead to; a path that leads nowhere, a `file` URL
/// of another machine and a text that is no URL fail, as a backend's
/// do.
fn resolve(target: &str) -> Result<Target, Error> {
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
