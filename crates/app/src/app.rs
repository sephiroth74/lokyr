//! The application: its windows, each with the pane of its program, what
//! they draw, its main menu, and its config, which it reads again when the
//! file changes.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use lokyr_font_coretext::CoreText;
use lokyr_kernel::commands::{
    Args, Category, Code, Command, Error, ErrorCategory, Kind, Registry, Value,
};
use lokyr_kernel::config::{
    self, AppConfig, AppearanceMode, ClipboardRead, CloseOnExit, Config, ConfirmPaste, LinksConfig,
    NewDirectory, OpenFile, OpenUrl, StartDirectory,
};
use lokyr_kernel::diagnostics::{About, Diagnostic, Diagnostics};
use lokyr_kernel::keymap::{
    self, Binding, Context, Invocation, Keymap, Pressed, Resolved, Resolver, When,
};
use lokyr_kernel::l10n::{self, Arg, Texts};
use lokyr_kernel::paste;
use lokyr_kernel::plugins::{self, Plugins, Request};
use lokyr_kernel::state;
#[cfg(test)]
use lokyr_kernel::workspace::RESIZE_PACE;
#[cfg(test)]
use lokyr_kernel::workspace::TOLD_PACE;
use lokyr_kernel::workspace::{
    self, Environment, Event, Exited, Grid, Pane, PaneId, Settings, Workspace,
};
use lokyr_platform_core::{
    Alert, AlertId, AlertStyle, AppEvent, Appearance, Backdrop, ContextMenu, Dropped, GlobalKey,
    Handler, Ime, KeyCode, KeyEvent, KeyState, MainMenu, Menu, MenuItem, MenuRole, Modifiers,
    MouseButton, Platform, Point, PointerArea, PointerShape, ScrollDelta, ScrollPhase, Shortcut,
    ShortcutKey, Size, StandardAction, TargetKind, TimerId, WindowAttributes, WindowEvent,
    WindowId, WindowSize,
};
use lokyr_platform_core::{Blink as PlatformBlink, Easing, Layer, LayerPlace};
use lokyr_pty::Program;
use lokyr_render_core::{CellBatch, Color, CursorShape, DisplayList, Item, Rect, Renderer};
use lokyr_render_metal::{Gpu, MetalRenderer, Surface};
use lokyr_text_core::FontConfig;
use lokyr_ui_core::cursor::{self, Blink};
use lokyr_ui_core::font::Font;
use lokyr_ui_core::pane::{self as cells, CursorLook, Preedit, TextLook};
use lokyr_ui_core::theme::{Theme, Variant};
use lokyr_vt_core::input::{Key, KeyOptions, MouseAction, MouseEvent, Wheel};
use lokyr_vt_core::page::{Budget, Scrollback};
use lokyr_vt_core::terminal::{
    self as vt, ClipboardRequest, MouseEncoding, MouseTracking, Scroll, SelectionShape,
    SelectionUnit,
};

use crate::environment;
use crate::icon::Icons;
use crate::keys;
use crate::mouse::{self, Mouse, Pointer, Spot};
use crate::notice;
use crate::performance::{self, Performance};
use crate::quick;
use crate::size_label;

use crate::watch::Watcher;

/// The command that quits.
const QUIT: &str = "app.quit";

/// The command that opens the config file in the text editor, made first
/// when it is not there.
const SETTINGS: &str = "app.settings";

/// The command that shows the performance's panel over the windows, or
/// hides it.
const PERFORMANCE: &str = "app.performance";

/// The command that turns Secure Keyboard Entry on for good, until the app
/// quits, or back to its own way, on at a password's prompt.
const SECURE_INPUT: &str = "app.secure-keyboard-entry";

/// What stands before the title of the window whose keys Secure Keyboard
/// Entry keeps from other apps.
const SECURE_MARK: &str = "🔒 ";

/// The command that opens a window.
const NEW_WINDOW: &str = "window.new";

/// The command that closes the focused window.
const CLOSE_WINDOW: &str = "window.close";

/// The command that asks for a title of the user's for the focused window.
const SET_TITLE: &str = "window.set-title";

/// The command that shows the quick terminal, or hides it.
const TOGGLE_QUICK: &str = "window.toggle-quick-terminal";

/// The command that shows the system's definition of the selection, of
/// one line, which the context menu alone shows, naming it.
const LOOK_UP: &str = "pane.look-up";

/// The characters of the selection that Look Up's item names, past which
/// it shortens it.
const LOOK_UP_TITLE: usize = 30;

/// The command that opens a window whose pane shows the man page that the
/// selection names, which the context menu alone shows, naming it.
const MAN_PAGE: &str = "pane.man-page";

/// The longest name of a man page, and of its section, that the man page's
/// command takes.
const MAN_PAGE_NAME: usize = 100;
const MAN_SECTION: usize = 8;

/// The commands that the context menu alone shows, as they name the
/// selection.
const CONTEXT_ONLY: [&str; 2] = [LOOK_UP, MAN_PAGE];

/// The commands that give the app an appearance, as the View menu's
/// Appearance chooses it, each with its own and its title's message.
const APPEARANCES: [(&str, AppearanceMode, &str); 3] = [
    (
        "app.appearance-system",
        AppearanceMode::System,
        "appearance-system",
    ),
    (
        "app.appearance-light",
        AppearanceMode::Light,
        "appearance-light",
    ),
    (
        "app.appearance-dark",
        AppearanceMode::Dark,
        "appearance-dark",
    ),
];

/// The lines of a pasted text that its confirmation shows, and the
/// characters of each.
const PREVIEW: (usize, usize) = (3, 60);

/// How deep the commands that plugins run from their commands go, past
/// which the next does not run, as a plugin that runs its own command
/// would go on forever.
const COMMANDS_DEEP: usize = 16;

/// The catalogs of the texts that the app shows, the kernel's and its own,
/// by language.
pub(crate) const CATALOGS: [(&str, &str); 4] = [
    l10n::KERNEL[0],
    l10n::KERNEL[1],
    ("en", include_str!("../resources/locales/en/app.ftl")),
    ("it", include_str!("../resources/locales/it/app.ftl")),
];

/// The size of a new window's content, in points.
const WINDOW_SIZE: Size = Size {
    width: 800.0,
    height: 500.0,
};

/// How long after a window's last frame of output its title looks once
/// more at the program in its foreground, for one that started without a
/// word.
const TITLE_LOOK: Duration = Duration::from_millis(250);

/// What parts a title's parts.
const TITLE_DASH: &str = " — ";

/// The columns and the rows of a man page's window: `man` lays its pages
/// out for 80 columns.
const MAN_PAGE_CELLS: (u32, u32) = (80, 48);

/// How long the app waits after the last change of the config file, for
/// the changes that come with it, as an editor's write and rename do, before
/// it reads the file once.
const SETTLE: Duration = Duration::from_millis(100);

/// How often the app looks for the status of a program whose output ended.
const EXIT_POLL: Duration = Duration::from_millis(20);

/// The cell of a window that the app does not draw, in pixels.
const UNDRAWN_CELL: (u32, u32) = (8, 16);

/// How long a window's flash for its bell lasts.
const FLASH: Duration = Duration::from_millis(100);

/// How often the view scrolls under a selection that the pointer drags past
/// the grid's top or bottom.
const SELECTION_SCROLL: Duration = Duration::from_millis(50);

/// The most rows that the view scrolls a step under a selection, as the
/// pointer goes farther past the grid.
const SELECTION_SCROLL_ROWS: usize = 8;

/// The opacity of the default text's color over a window as it flashes.
const FLASH_OPACITY: f32 = 0.2;

/// How long after a bell that acted another in the same window does
/// nothing, as a program can ring many at once.
const BELL_PAUSE: Duration = Duration::from_millis(200);

/// How long after a notification that showed another of the same window
/// shows nothing, as a program can send many at once.
const NOTIFICATION_PAUSE: Duration = Duration::from_secs(2);

/// The characters of a notification's title and of its text that show.
const NOTIFICATION_TITLE: usize = 256;
const NOTIFICATION_BODY: usize = 2048;

/// The `TERM` that a window takes in place of the terminal's own when the
/// app has no entry of it to give its programs.
const FALLBACK_TERM: &str = "xterm-256color";

/// The title of a window whose program gives none, and what stands before
/// the title of a window whose bell rang in the background.
const TITLE: &str = "Lokyr";
const BELL_MARK: &str = "🔔 ";

/// What draws the windows: the GPU, its renderer, and the font of the text.
#[derive(Debug)]
pub struct Drawing {
    gpu: Gpu,
    renderer: MetalRenderer,
    font: Font<CoreText>,
    /// How many fonts it took, the first one too: a layer draws its cells
    /// again of a new one, whose atlas gives their regions to other pixels.
    fonts: u64,
}

impl Drawing {
    /// The system's GPU, and the font that `config` gives, which tells of a
    /// family that is not installed.
    pub fn new(config: FontConfig) -> Result<Self, String> {
        let gpu = Gpu::new().map_err(|error| error.to_string())?;
        let font = font(config)?;
        let (coverage, color) = font.texture_sizes();
        let renderer =
            MetalRenderer::new(&gpu, coverage, color).map_err(|error| error.to_string())?;
        Ok(Self {
            gpu,
            renderer,
            font,
            fonts: 1,
        })
    }

    /// Takes the font that `config` gives, with an atlas of its own, and a
    /// renderer of the atlas's textures, in place of those before; or, if
    /// it fails, keeps them.
    fn set_font(&mut self, config: FontConfig) -> Result<(), String> {
        let font = font(config)?;
        let (coverage, color) = font.texture_sizes();
        let renderer =
            MetalRenderer::new(&self.gpu, coverage, color).map_err(|error| error.to_string())?;
        self.font = font;
        self.renderer = renderer;
        self.fonts += 1;
        Ok(())
    }

    /// What the font lacks of the config's: a family that is not
    /// installed, of the text or of the text past ASCII, and the one that
    /// it takes instead.
    fn missing(&self) -> Option<String> {
        let taken = self.font.family();
        let missing = |family: &str, of: &str| {
            format!("the font family `{family}` is not installed; the {of} takes `{taken}`")
        };
        let text = self
            .font
            .missing_family()
            .map(|family| missing(family, "text"));
        let past_ascii = self
            .font
            .missing_non_ascii_family()
            .map(|family| missing(family, "text past ASCII"));
        match (text, past_ascii) {
            (Some(text), Some(past)) => Some(format!("{text}; {past}")),
            (text, past) => text.or(past),
        }
    }
}

/// The font that `config` gives.
fn font(config: FontConfig) -> Result<Font<CoreText>, String> {
    Font::new(CoreText::new(), config, 1.0).map_err(|error| error.to_string())
}

/// Checks `config` as the app takes it, without applying it: its font,
/// whose failure fails the whole config, and its icon and its language,
/// which the log tells of when they are not Lokyr's, and whose defaults
/// then show.
pub(crate) fn check(config: &Config) -> Result<(), String> {
    font(config.font.clone())?;
    texts(&config.app.language, &[]);
    if let Some(icons) = Icons::find() {
        variant(&icons.variants(), &config.app.icon);
    }
    Ok(())
}

/// The variant of `variants` that `wanted` names; for a name that is none
/// of theirs, the default one, as the log tells.
fn variant<'a>(variants: &[String], wanted: &'a str) -> &'a str {
    if variants.iter().any(|variant| variant == wanted) {
        wanted
    } else {
        eprintln!(
            "lokyr: the icon `{wanted}` is not one of Lokyr's: {}; the default one shows",
            variants.join(", ")
        );
        AppConfig::DEFAULT_ICON
    }
}

/// What a pane's session wakes the main thread for: that it changed, which
/// the main thread takes, and how many times it woke it.
#[derive(Debug, Default)]
struct PaneWake {
    woken: AtomicBool,
    wakes: AtomicU64,
}

impl PaneWake {
    /// The session changed, from its threads.
    fn wake(&self) {
        self.woken.store(true, Ordering::Release);
        self.wakes.fetch_add(1, Ordering::Relaxed);
    }

    /// Whether it changed since the last take.
    fn take(&self) -> bool {
        self.woken.swap(false, Ordering::AcqRel)
    }
}

/// The config file, and its watch.
#[derive(Debug)]
struct ConfigFile {
    path: PathBuf,
    watcher: Option<Watcher>,
    /// Whether the file may have changed, which the watch's thread tells.
    changed: Arc<AtomicBool>,
    /// The timer of the reading, which waits for the changes that come
    /// together.
    reading: Option<TimerId>,
}

impl ConfigFile {
    /// Watches the file, or its directory's parent while its directory does
    /// not exist, so that its changes wake `platform`.
    fn watch(&mut self, platform: &dyn Platform) {
        let (changed, waker) = (Arc::clone(&self.changed), platform.waker());
        self.watcher = Watcher::new(&self.path, move || {
            changed.store(true, Ordering::Release);
            waker.wake();
        })
        .inspect_err(|error| eprintln!("lokyr: {error}"))
        .ok();
    }
}

/// A layer of a window, over its grid, which the window server blinks.
#[derive(Debug, Default)]
struct WindowLayer {
    /// What the app draws into, where the platform has the layer.
    surface: Option<Surface>,
    /// Where the app placed it last, and how it blinks: `None` while it is
    /// hidden.
    place: Option<LayerPlace>,
    /// The cells that it shows, at what scale, and of which of the
    /// drawing's fonts, which a frame draws again only as they change.
    drawn: Option<(CellBatch, f64, u64)>,
}

impl WindowLayer {
    fn new(surface: Option<Surface>) -> Self {
        Self {
            surface,
            ..Self::default()
        }
    }

    /// Lets the layer's drawables go, all of them, as a frame of a pixel
    /// takes the place of the one that it shows; the next cells that it
    /// shows draw again.
    fn release(&mut self) {
        let Some(surface) = &self.surface else {
            return;
        };
        surface.release();
        if let Err(error) = surface.clear(Color::default().with_alpha(0.0)) {
            eprintln!("lokyr: {error}");
        }
        self.drawn = None;
    }
}

/// A window that the app opened.
#[derive(Debug)]
struct Window {
    /// What it draws into, once the app draws and the platform has a layer.
    surface: Option<Surface>,
    /// The layers of its text that blinks, over the grid, and of its
    /// cursor's cells, over everything.
    text_layer: WindowLayer,
    cursor_layer: WindowLayer,
    /// The workspace's window that it shows.
    window: workspace::WindowId,
    /// Its pane, which runs its program, and what that shows.
    pane: PaneId,
    /// The text that the input method composes, shown at the cursor.
    preedit: Option<Preedit>,
    /// Whether it has the keyboard's focus.
    focused: bool,
    /// The clock of its cursor's blinking.
    blink: Blink,
    /// Whether its cursor blinked at its last frame.
    blinking: bool,
    /// Until when it flashes for its bell.
    flash_until: Option<Instant>,
    /// When its last bell acted.
    rang_at: Option<Instant>,
    /// When its program's last notification showed.
    notified_at: Option<Instant>,
    /// Whether its title shows that its bell rang while it had no focus.
    bell_marked: bool,
    /// The backdrop that the platform has for it, while it is translucent.
    backdrop: Option<Backdrop>,
    /// The mouse over it.
    mouse: Mouse,
    /// Whether the platform tells the pointer's moves without a button, as
    /// a program asked for them.
    mouse_moves: bool,
    /// How the pointer looks over it, as the platform has it.
    pointer: PointerShape,
    /// The sequence of keys under way, as the keymap resolves it.
    resolver: Resolver,
    /// The keys of the sequence under way, which go to the program if the
    /// next key does not go on with it.
    held_back: Vec<KeyEvent>,
    /// The keys whose presses the keymap took, whose releases go nowhere,
    /// each once.
    taken: Vec<KeyCode>,
    /// The selection that the mouse makes, or made last, which Shift's click
    /// extends.
    selecting: Option<Selecting>,
    /// The title of the man page that it shows, as `man ls`, when it shows
    /// one: it takes the man page's colors, and keeps the page once `man`
    /// has ended.
    page: Option<String>,
    /// The last part of the directory of the program in its foreground, as
    /// `lokyr`, and that program's name, as `zsh`, which its title shows.
    place: (String, String),
    /// That directory, whole, as the title last looked, which stays once
    /// the program ended.
    directory: Option<PathBuf>,
    /// Whether the user resizes it, dragging its edge; when it drew last;
    /// the grid's size that it drew last, its columns and rows; and until
    /// when the label of its grid's size shows, after the grid changed.
    live_resize: bool,
    drawn_at: Option<Instant>,
    grid_drawn: Option<(u16, u16)>,
    size_label: Option<Instant>,
    /// Where its last frame drew the notice's bar and its close, in pixels,
    /// and the pointer's areas that the platform has of them, in points.
    notice: Option<notice::Rects>,
    pointer_areas: Vec<PointerArea>,
    /// The title that the platform shows, once the app gave one.
    title_shown: Option<String>,
    /// When its last frame of output came, since its title last looked, for
    /// the titles' timer to look once more [`TITLE_LOOK`] after it.
    output_at: Option<Instant>,
    /// The title that the user gave it, which takes the place of its
    /// program's and of its own.
    user_title: Option<String>,
    /// The appearance of its own that the platform has for it, once the
    /// app gave one: `None` for the app's.
    own_appearance: Option<Option<Appearance>>,
    /// Whether it is hidden or covered, which draws nothing.
    occluded: bool,
    /// What its surface shows, as its last frame drew it: its size in
    /// pixels and its scale, the drawing's fonts, the background and the
    /// items, which a frame of the same presents again for nothing.
    shown: Option<Shown>,
}

/// What a window's surface shows.
#[derive(Debug, PartialEq)]
struct Shown {
    pixels: (u32, u32),
    scale: f64,
    fonts: u64,
    background: Color,
    items: Vec<Item>,
}

/// `target` as a question shows it: each character that shows nothing,
/// or that turns the text's direction, as `\u{202E}`, so that the target
/// reads as it is.
fn shown(target: &str) -> String {
    let mut shown = String::with_capacity(target.len());
    for c in target.chars() {
        if invisible(c) {
            shown.push_str(&format!("\\u{{{:X}}}", u32::from(c)));
        } else {
            shown.push(c);
        }
    }
    shown
}

/// Whether `c` shows nothing, or turns the text's direction: the controls,
/// the separators of lines and paragraphs, and Unicode's characters that
/// are ignorable by default, as the bidirectional overrides and isolates,
/// the zero-width ones, the variation selectors and the tags.
fn invisible(c: char) -> bool {
    c.is_control()
        || matches!(
            u32::from(c),
            0xAD | 0x34F
                | 0x61C
                | 0x115F..=0x1160
                | 0x17B4..=0x17B5
                | 0x180B..=0x180F
                | 0x200B..=0x200F
                | 0x2028..=0x202E
                | 0x2060..=0x206F
                | 0x3164
                | 0xFE00..=0xFE0F
                | 0xFEFF
                | 0xFFA0
                | 0xFFF9..=0xFFFB
                | 0x1BCA0..=0x1BCA3
                | 0x1D173..=0x1D17A
                | 0xE0000..=0xE0FFF
        )
}

/// What the user chose that links do, as the config writes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Chosen {
    /// What a URL does.
    Url(OpenUrl),
    /// What a file does.
    File(OpenFile),
}

/// A selection that the mouse's left button makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Selecting {
    /// Where the press started it.
    anchor: Spot,
    /// Where the pointer is, or was as the button went up.
    last: Spot,
    shape: SelectionShape,
    unit: SelectionUnit,
    /// Whether the button is down.
    held: bool,
    /// How many rows past the grid's top, negative, or bottom, positive,
    /// the pointer is, which scrolls the view.
    beyond: i64,
}

impl Window {
    fn new(
        (surface, text_surface, cursor_surface): (
            Option<Surface>,
            Option<Surface>,
            Option<Surface>,
        ),
        (window, pane): (workspace::WindowId, PaneId),
        focused: bool,
        now: Instant,
    ) -> Self {
        Self {
            surface,
            text_layer: WindowLayer::new(text_surface),
            cursor_layer: WindowLayer::new(cursor_surface),
            window,
            pane,
            preedit: None,
            focused,
            blink: Blink::new(now),
            blinking: false,
            flash_until: None,
            rang_at: None,
            notified_at: None,
            bell_marked: false,
            backdrop: None,
            mouse: Mouse::default(),
            mouse_moves: false,
            pointer: PointerShape::Arrow,
            resolver: Resolver::new(),
            held_back: Vec::new(),
            taken: Vec::new(),
            selecting: None,
            page: None,
            place: (String::new(), String::new()),
            directory: None,
            notice: None,
            live_resize: false,
            drawn_at: None,
            grid_drawn: None,
            size_label: None,
            pointer_areas: Vec::new(),
            title_shown: None,
            output_at: None,
            user_title: None,
            own_appearance: None,
            occluded: false,
            shown: None,
        }
    }

    /// Takes the press of the key at `code` for the keymap, so that its
    /// release goes nowhere.
    fn take(&mut self, code: KeyCode) {
        if !self.taken.contains(&code) {
            self.taken.push(code);
        }
    }

    /// Ends the sequence under way, and gives back its keys, for the
    /// program, whose releases go to it too, as the keymap no longer takes
    /// their presses.
    fn end_sequence(&mut self) -> Vec<KeyEvent> {
        self.resolver = Resolver::new();
        let keys = std::mem::take(&mut self.held_back);
        for key in &keys {
            self.taken.retain(|&code| code != key.code);
        }
        keys
    }
}

/// The application, which opens a window when it starts, and quits with its
/// last window or from its menu.
#[derive(Debug)]
pub struct App {
    drawing: Option<Drawing>,
    theme: Theme,
    /// The program that a new window runs.
    program: Program,
    /// The program that shows man pages, in place of the login shell's
    /// `man`, as the tests give one.
    man: Option<PathBuf>,
    /// The config in force.
    config: Config,
    /// The file of the config, if it has one.
    config_file: Option<ConfigFile>,
    /// The texts that the app shows, in the language shown.
    texts: Texts,
    /// The user's languages, as the system has them, from the first.
    preferred: Vec<String>,
    /// The commands, which the app and the plugins run.
    commands: Registry,
    /// The built-in plugins.
    plugins: Plugins,
    /// The bindings of keys to commands: the app's, the plugins', and the
    /// config's.
    keymap: Keymap,
    /// The size of the text that a plugin gave, in points, in place of the
    /// config's.
    font_size: Option<f32>,
    /// The windows, their tabs and panes.
    workspace: Workspace,
    /// The platform's windows, each with what the app keeps of it.
    windows: BTreeMap<WindowId, Window>,
    /// The timer of the next change of a blinking cursor, and its time.
    blink_timer: Option<(TimerId, Instant)>,
    /// The timer of the end of the first label of a grid's size that ends,
    /// and its time.
    label_timer: Option<(TimerId, Instant)>,
    /// The timer of the next look for the status of a program whose output
    /// ended.
    exit_timer: Option<TimerId>,
    /// The timer of the turn of the panes whose sessions told something
    /// within the pace of their last look, and its time.
    told_timer: Option<(TimerId, Instant)>,
    /// What each pane's session wakes the main thread for, which tells the
    /// panes that changed.
    pane_wakes: BTreeMap<PaneId, Arc<PaneWake>>,
    /// The timer of the next size that a program learns, at its pace, and
    /// its time.
    resize_timer: Option<(TimerId, Instant)>,
    /// The timer of the end of the windows' flashes.
    flash_timer: Option<TimerId>,
    /// The timer of the next step of the views that scroll under a
    /// selection dragged past their grid.
    scroll_timer: Option<TimerId>,
    /// Whether a window opened since the app started, after which none
    /// restores the frame that a window kept, nor starts in the first
    /// window's directory.
    opened_one: bool,
    /// The directory of the app's state, which keeps the directory of the
    /// last window that closed; `None` keeps nothing, as in the tests.
    state: Option<PathBuf>,
    /// The window that took the focus last, which is the one in front as
    /// the app quits, though the app is not active.
    front: Option<WindowId>,
    /// What went wrong that the user is to learn of: the log, and the
    /// notices that the windows show.
    diagnostics: Diagnostics,
    /// Whether the user turned Secure Keyboard Entry on for good, from the
    /// menu, and whether it is on, as the platform has it.
    secure_chosen: bool,
    secure_on: bool,
    /// The timer of the windows' titles' last look at the programs in
    /// their foreground, after the frames of a burst of output, with the
    /// time that it is for; and how long after a window's last frame of
    /// output that look comes, `None` for none.
    title_timer: Option<(TimerId, Instant)>,
    title_look: Option<Duration>,
    /// The icon's variants, where the app finds them.
    icons: Option<Icons>,
    /// The variant that the app made the icon, once it made one.
    icon_shown: Option<String>,
    /// The programs' reads of the clipboard that the user is asked about,
    /// by their alerts.
    clipboard_asks: BTreeMap<AlertId, (PaneId, ClipboardRequest)>,
    /// The pastes that the user is asked to confirm, by their alerts.
    paste_asks: BTreeMap<AlertId, (PaneId, String)>,
    /// Whether the user asked that pastes be not confirmed, where the
    /// config could not keep it: until the app quits.
    pastes_unconfirmed: bool,
    /// The links that the user is asked about before they open, by their
    /// alerts: the window that the question is on, the URL or the path,
    /// and what it is.
    open_asks: BTreeMap<AlertId, (Option<WindowId>, (String, TargetKind))>,
    /// The contexts in which the menus' commands that tell one can run, and
    /// whether the platform has each one's items enabled, as the focused
    /// pane's last frame had it.
    menu_contexts: Vec<(String, When)>,
    menu_enabled: BTreeMap<String, bool>,
    /// The window that the last context menu opened on, whose pane marks
    /// the cell that it opened on, which its commands of a cell take.
    context_window: Option<WindowId>,
    /// The window whose pane the commands run in, in place of the focused
    /// one, while a context menu's do.
    acting: Option<WindowId>,
    /// What the user chose that links do, where the config could not keep
    /// it: until the app quits.
    links_chosen: (Option<OpenUrl>, Option<OpenFile>),
    /// The appearance that the user chose from the menu, where the config
    /// could not keep it: until the app quits, or the config's key changes.
    appearance_chosen: Option<AppearanceMode>,
    /// The performance's panel, while it shows, with what it measures; and
    /// whether the user chose from the menu to show it, until the app quits
    /// or the config's key changes.
    performance: Option<Performance>,
    performance_chosen: Option<bool>,
    /// Whether the cursors and the text that blinks blink as on layers of
    /// their own without them, as the tests' fake platform, which draws
    /// nothing, shows them.
    layers: bool,
    /// The windows whose closing the user is asked to confirm, by their
    /// alerts.
    close_asks: BTreeMap<AlertId, WindowId>,
    /// The windows whose title the user is asked for, by their alerts.
    title_asks: BTreeMap<AlertId, WindowId>,
    /// The question before quitting, while it waits for its answer.
    quit_ask: Option<AlertId>,
    /// Whether the system asked to quit, and waits for the answer.
    system_quit: bool,
    /// The environment of the programs that the app starts, which the
    /// start completes with the user's locale.
    environment: Environment,
    /// The memory that the windows' scrollbacks share, which the config
    /// sizes.
    budget: Arc<Budget>,
    /// The quick terminal's window, once it opened, and whether it shows.
    quick: Option<(WindowId, bool)>,
    /// The config's global keys that the platform took, by the indices
    /// that their presses tell, with the commands that they run; and
    /// whether the system gave the app one of them at least, which keeps
    /// the app open without windows.
    global_keys: Vec<(keymap::Combo, Invocation)>,
    global_held: bool,
}

impl App {
    /// An application that draws its windows with `drawing`, or nothing
    /// without it, with `config`, which it reads again from `path`, its
    /// file, when that changes.
    pub fn new(drawing: Option<Drawing>, config: Config, path: Option<PathBuf>) -> Self {
        let mut plugins = Plugins::new();
        plugins.add("core-actions", Box::new(lokyr_core_actions::CoreActions));
        plugins.add("links", Box::new(lokyr_links::Links));
        let keymap = Keymap::with_config(bindings(&plugins), &config.keys);
        let budget = Arc::new(Budget::new(config.scrollback.budget.bytes()));
        Self {
            drawing,
            theme: Theme::builtin(),
            program: Program::LoginShell,
            man: None,
            config,
            config_file: path.map(|path| ConfigFile {
                path,
                watcher: None,
                changed: Arc::new(AtomicBool::new(false)),
                reading: None,
            }),
            commands: commands(&Texts::new(l10n::SOURCE, &CATALOGS), &plugins),
            texts: Texts::new(l10n::SOURCE, &CATALOGS),
            preferred: Vec::new(),
            plugins,
            keymap,
            font_size: None,
            workspace: Workspace::new(),
            windows: BTreeMap::new(),
            blink_timer: None,
            label_timer: None,
            exit_timer: None,
            told_timer: None,
            pane_wakes: BTreeMap::new(),
            resize_timer: None,
            flash_timer: None,
            scroll_timer: None,
            title_timer: None,
            title_look: Some(TITLE_LOOK),
            opened_one: false,
            state: None,
            front: None,
            diagnostics: Diagnostics::default(),
            secure_chosen: false,
            secure_on: false,
            icons: Icons::find(),
            icon_shown: None,
            clipboard_asks: BTreeMap::new(),
            paste_asks: BTreeMap::new(),
            pastes_unconfirmed: false,
            open_asks: BTreeMap::new(),
            links_chosen: (None, None),
            appearance_chosen: None,
            performance: None,
            performance_chosen: None,
            layers: false,
            menu_contexts: Vec::new(),
            menu_enabled: BTreeMap::new(),
            context_window: None,
            acting: None,
            close_asks: BTreeMap::new(),
            title_asks: BTreeMap::new(),
            quit_ask: None,
            system_quit: false,
            environment: {
                let resources = environment::resources();
                Environment {
                    terminfo: environment::terminfo(resources.as_deref()),
                    resources,
                    binaries: environment::binaries(),
                    ..Environment::default()
                }
            },
            budget,
            quick: None,
            global_keys: Vec::new(),
            global_held: false,
        }
    }

    /// The app, with `plugin`, by its `name`, after the built-in ones.
    #[cfg(test)]
    fn with_plugin(mut self, name: &str, plugin: Box<dyn plugins::Plugin>) -> Self {
        self.plugins.add(name, plugin);
        self
    }

    /// The app, whose notices start with those of `diagnostics`, as a
    /// config that does not read as the app starts.
    pub fn with_diagnostics(mut self, diagnostics: Diagnostics) -> Self {
        self.diagnostics = diagnostics;
        self
    }

    /// The app, which keeps the directory of the last window that closed in
    /// `state`, its state's directory, for the next start.
    pub fn with_state(mut self, state: Option<PathBuf>) -> Self {
        self.state = state;
        self
    }

    /// The app, whose windows run `program` rather than the login shell;
    /// its titles take no last look after a burst of output, whose timer
    /// the tests of the other timers would count, but where a test gives
    /// them one.
    #[cfg(test)]
    fn with_program(mut self, program: Program) -> Self {
        self.program = program;
        self.title_look = None;
        self
    }

    /// The app, whose programs find its resources in `resources`.
    #[cfg(test)]
    fn with_resources(mut self, resources: Option<PathBuf>) -> Self {
        self.environment.resources = resources;
        self
    }

    /// The pane of `window`.
    #[cfg(test)]
    fn pane(&self, window: WindowId) -> &Pane {
        let pane = self.windows[&window].pane;
        self.workspace.pane(pane).expect("the window's pane")
    }

    /// The app, whose icon's variants are `icons`.
    #[cfg(test)]
    fn with_icons(mut self, icons: Option<Icons>) -> Self {
        self.icons = icons;
        self
    }

    /// Reads the config file again: a valid config applies whole, and an
    /// invalid one changes nothing, as the log tells.
    fn reload(&mut self, platform: &mut dyn Platform) {
        let Some(file) = &mut self.config_file else {
            return;
        };
        if file
            .watcher
            .as_ref()
            .is_none_or(|watcher| watcher.is_stale(&file.path))
        {
            file.watch(platform);
        }
        match Config::read(&file.path) {
            Ok(config) => self.apply(platform, config),
            Err(error) => {
                let text =
                    format!("{error}; none of the config applies, and the one in force stays");
                let diagnostic = Diagnostic::new(Code::InvalidConfig, text, About::Config);
                self.report(platform, diagnostic);
            }
        }
    }

    /// Tells `diagnostic`, in the log and, where it is one, in the notice
    /// of every window.
    fn report(&mut self, platform: &mut dyn Platform, diagnostic: Diagnostic) {
        if self.diagnostics.report(diagnostic) {
            self.request_frames(platform);
        }
    }

    /// Takes the notice about `about` away, as a fix does.
    fn resolve(&mut self, platform: &mut dyn Platform, about: &About) {
        if self.diagnostics.resolve(about) {
            self.request_frames(platform);
        }
    }

    /// Asks every window for a frame.
    fn request_frames(&self, platform: &mut dyn Platform) {
        for &window in self.windows.keys() {
            platform.request_frame(window);
        }
    }

    /// Tells what the font lacks of the config's, or takes the notice of
    /// what it lacked away.
    fn tell_font(&mut self, platform: &mut dyn Platform) {
        let Some(drawing) = &self.drawing else {
            return;
        };
        match drawing.missing() {
            Some(text) => self.report(
                platform,
                Diagnostic::new(Code::Unavailable, text, About::Font),
            ),
            None => self.resolve(platform, &About::Font),
        }
    }

    /// Takes `config` in place of the one in force, whole, in every window;
    /// or nothing of it, if its font fails.
    fn apply(&mut self, platform: &mut dyn Platform, config: Config) {
        if config == self.config {
            self.resolve(platform, &About::Config);
            return;
        }
        let font_changed = config.font != self.config.font;
        if font_changed
            && let Some(drawing) = &mut self.drawing
            && let Err(error) = drawing.set_font(sized(&config.font, self.font_size))
        {
            let text = format!("{error}; none of the config applies, and the one in force stays");
            let diagnostic = Diagnostic::new(Code::InvalidConfig, text, About::Config);
            self.report(platform, diagnostic);
            return;
        }
        self.resolve(platform, &About::Config);
        if font_changed {
            self.tell_font(platform);
        }
        let icon_changed = config.app.icon != self.config.app.icon;
        let language_changed = config.app.language != self.config.app.language;
        let appearance_changed = config.app.appearance != self.config.app.appearance;
        let keys_changed = config.keys != self.config.keys;
        let remember_changed = config.window.remember_frame != self.config.window.remember_frame;
        let performance_changed =
            config.debug.performance_overlay != self.config.debug.performance_overlay;
        if keys_changed {
            self.keymap = Keymap::with_config(bindings(&self.plugins), &config.keys);
        }
        self.config = config;
        // The open windows keep their frames, or not, as the user moves or
        // resizes them from now; a man page's never does.
        if remember_changed {
            for (&window, state) in &self.windows {
                if self.quick.is_some_and(|(quick, _)| quick == window) {
                    continue;
                }
                let remember = self.config.window.remember_frame && state.page.is_none();
                platform.set_window_remember(window, remember);
            }
        }
        let (left, right) = self.config.keyboard.option_as_meta.sides();
        platform.set_option_as_meta(left, right);
        // The open windows take the settings for their output to come;
        // their programs keep the `TERM` that they started with.
        let terminal = &self.config.terminal;
        let widths = terminal.widths();
        self.budget.set_limit(self.config.scrollback.budget.bytes());
        for pane in self.workspace.panes() {
            pane.set_scroll_on_output(self.config.scrollback.scroll_on_output);
            pane.set_clipboard_reads(self.config.clipboard.read == ClipboardRead::Ask);
            pane.set_width_tables(widths.tables, widths.ambiguous_wide);
            pane.set_normalization(terminal.normalization.form());
            pane.set_scrollback_limit(self.config.scrollback.limit.bytes());
        }
        if icon_changed {
            self.show_icon(platform);
        }
        if appearance_changed {
            self.appearance_chosen = None;
            self.show_appearance(platform);
        }
        if performance_changed {
            self.performance_chosen = None;
            self.show_performance(platform);
        }
        if language_changed {
            self.texts = texts(&self.config.app.language, &self.preferred);
            self.commands = commands(&self.texts, &self.plugins);
            self.set_menu(platform);
        } else if keys_changed {
            self.set_menu(platform);
        }
        if keys_changed {
            self.take_global_keys(platform);
        }
        for &window in self.windows.keys() {
            platform.request_frame(window);
        }
    }

    /// Shows the icon's variant that the config names, or the default one
    /// for a name that is no variant's, as the log tells; nothing when that
    /// variant shows already, as the bundle's own does at the start, or
    /// when the app found no variants, which the log tells too.
    fn show_icon(&mut self, platform: &mut dyn Platform) {
        let Some(icons) = &self.icons else {
            eprintln!(
                "lokyr: no icon's variants found, neither in the bundle nor in the source tree; the icon `{}` cannot show",
                self.config.app.icon
            );
            return;
        };
        let variant = variant(&icons.variants(), &self.config.app.icon);
        let shown = self
            .icon_shown
            .as_deref()
            .or_else(|| icons.bundled().then_some(AppConfig::DEFAULT_ICON));
        if shown == Some(variant) {
            return;
        }
        match platform.set_app_icon(icons.file(variant).as_deref()) {
            Ok(()) => self.icon_shown = Some(variant.to_owned()),
            Err(error) => eprintln!("lokyr: {error}"),
        }
    }

    /// Where `position` is in the cells of `window`.
    fn pointer(
        &self,
        platform: &dyn Platform,
        window: WindowId,
        position: Point,
    ) -> Option<Pointer> {
        let pane = self.workspace.pane(self.windows.get(&window)?.pane)?;
        let size = platform.window_size(window)?;
        Some(mouse::pointer(
            position,
            size.scale,
            self.origin(size),
            pane.grid(),
        ))
    }

    /// A button of the mouse went down or up over `window`: the program
    /// learns of it, as its mouse modes have it, at the screen's row, and
    /// not of a press above the screen, in a view scrolled up. Shift keeps
    /// a press for the terminal, which selects with it; a release goes to
    /// the program whenever it learned of the press, so that no button stays
    /// down for it.
    fn mouse_button(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        (button, pressed, clicks): (MouseButton, bool, u32),
        position: Point,
        mods: Modifiers,
    ) {
        let Some(button) = mouse::button(button) else {
            return;
        };
        // A press on the notice's close closes the notice; one on the rest
        // of its bar does nothing, as the bar covers the text under it.
        match self.on_notice(platform, window, position) {
            Some(close) if pressed => {
                if close {
                    self.diagnostics.dismiss();
                    self.request_frames(platform);
                }
                return;
            }
            _ => {}
        }
        // A press that a binding takes starts no selection, and reaches no
        // program, which then learns of no release.
        if button == lokyr_vt_core::input::MouseButton::Left
            && pressed
            && clicks <= 1
            && self.click(platform, window, position, mods)
        {
            return;
        }
        // The right button, or the left one with Control, opens the context
        // menu where the terminal keeps the click.
        let menu_button = match button {
            lokyr_vt_core::input::MouseButton::Right => true,
            lokyr_vt_core::input::MouseButton::Left => mods.control,
            _ => false,
        };
        if pressed && menu_button && self.keeps_click(window, mods) {
            self.context_menu(platform, window, position);
            return;
        }
        if button == lokyr_vt_core::input::MouseButton::Left && self.selects(window, pressed, mods)
        {
            if pressed {
                self.select_press(platform, window, position, (mods, clicks));
            } else {
                self.select_release(platform, window);
            }
            return;
        }
        if pressed && mods.shift {
            return;
        }
        let Some(pointer) = self.pointer(platform, window, position) else {
            return;
        };
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        let Some(pane) = self
            .workspace
            .pane(state.pane)
            .filter(|pane| !pane.is_kept())
        else {
            return;
        };
        let on_screen =
            mouse::on_screen(pointer, pane.snapshot().offset(), pane.grid().cell_height);
        if pressed {
            let Some(pointer) = on_screen else {
                return;
            };
            state.mouse.moved(pointer, true);
            if pane.mouse(&mouse_event(MouseAction::Press(button), mods, pointer)) {
                state.mouse.press(button);
            }
        } else if state.mouse.release(button) {
            let pointer = on_screen.unwrap_or(mouse::at_top(pointer));
            state.mouse.moved(pointer, true);
            pane.mouse(&mouse_event(MouseAction::Release(button), mods, pointer));
        }
    }

    /// Runs the command that the keymap binds the left button's click with
    /// `mods` to, as it goes down at `position` over `window`, which has the
    /// focus, before the selection and the program: the column and the row
    /// of the cell under the pointer, in the view, become its `column` and
    /// `row` arguments, if it takes them. Whether a command was bound.
    fn click(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        position: Point,
        mods: Modifiers,
    ) -> bool {
        if platform.focused_window() != Some(window) {
            return false;
        }
        let Some(pointer) = self.pointer(platform, window, position) else {
            return false;
        };
        let Some(state) = self.windows.get(&window) else {
            return false;
        };
        let context = context(self.workspace.pane(state.pane));
        let Resolved::Run(mut invocation) =
            Resolver::new().press(&self.keymap, Pressed::click(binding_mods(mods)), &context)
        else {
            return false;
        };
        if let Some(command) = self.commands.get(&invocation.command) {
            for (name, value) in [("column", pointer.col), ("row", pointer.row)] {
                if command.params.iter().any(|param| param.name == name) {
                    invocation
                        .args
                        .entry(name.to_owned())
                        .or_insert(Value::Integer(i64::from(value)));
                }
            }
        }
        self.run(platform, &invocation.command, invocation.args);
        true
    }

    /// Whether the left button's press, `pressed`, or its release, with
    /// `mods`, over `window` selects: a press that the terminal keeps; a
    /// release that ends a selection.
    fn selects(&self, window: WindowId, pressed: bool, mods: Modifiers) -> bool {
        if pressed {
            return self.keeps_click(window, mods);
        }
        self.windows
            .get(&window)
            .and_then(|state| state.selecting)
            .is_some_and(|selecting| selecting.held)
    }

    /// Whether a press with `mods` over `window` stays with the terminal:
    /// where no program takes the mouse, or with Shift.
    fn keeps_click(&self, window: WindowId, mods: Modifiers) -> bool {
        let Some(state) = self.windows.get(&window) else {
            return false;
        };
        mods.shift
            || self.workspace.pane(state.pane).is_none_or(|pane| {
                pane.is_kept()
                    || pane
                        .input_modes()
                        .is_none_or(|modes| modes.mouse_tracking == MouseTracking::None)
            })
    }

    /// Opens `window`'s context menu at `position`, over the cell there,
    /// once the click has selected, as macOS's Terminal does: the
    /// selection stays when it holds the cell; else the cell's word is
    /// selected, as by a double click, or nothing on a blank cell. Its
    /// items are the commands of a cell, which take its `column` and `row`,
    /// then the clipboard's and the editing's that run without arguments;
    /// an item is dimmed where its command's context does not hold, or
    /// where its plugin tells that it would do nothing.
    fn context_menu(&mut self, platform: &mut dyn Platform, window: WindowId, position: Point) {
        let Some(pointer) = self.pointer(platform, window, position) else {
            return;
        };
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        let Some(pane) = self.workspace.pane(state.pane) else {
            return;
        };
        // Pinned, so that output that scrolls while the menu is open leaves
        // its commands on the clicked text.
        pane.mark_cell((pointer.col, i64::from(pointer.row)));
        let within = pane
            .selection()
            .is_some_and(|selected| selected.contains(pointer.col, pointer.row));
        if !within {
            let grid = pane.grid();
            let offset = pane.view_offset();
            let spot = mouse::spot(pointer, offset, grid.cell_width, grid.columns);
            pane.anchor_selection((spot.col, spot.row + offset as i64));
            let mut selecting = Selecting {
                anchor: spot,
                last: spot,
                shape: SelectionShape::Cells,
                unit: SelectionUnit::Word,
                held: false,
                beyond: 0,
            };
            show_selection(pane, &mut selecting);
            let blank = pane
                .selection_text()
                .is_none_or(|text| text.trim().is_empty());
            if blank {
                pane.clear_selection();
            }
            state.selecting = (!blank).then_some(selecting);
            platform.request_frame(window);
        }
        let context = context(Some(pane));
        let cell = (pointer.col, pointer.row);
        let edits = |category: Category| -> Vec<Command> {
            self.commands
                .in_order()
                .filter(|command| {
                    command.category == category
                        && command.params.iter().all(|param| param.default.is_some())
                        && !of_cell(command)
                        && !CONTEXT_ONLY.contains(&command.name.as_str())
                })
                .cloned()
                .collect()
        };
        // As in Terminal: what opens, then Look Up, then the clipboard's,
        // then the editing's.
        let groups: [Vec<Command>; 4] = [
            self.commands
                .in_order()
                .filter(|command| of_cell(command))
                .chain(self.commands.get(MAN_PAGE))
                .cloned()
                .collect(),
            self.commands.get(LOOK_UP).cloned().into_iter().collect(),
            edits(Category::Clipboard),
            edits(Category::Edit),
        ];
        self.acting = Some(window);
        let dimmed = groups
            .iter()
            .flatten()
            .filter(|command| !self.applies(platform, command, &context, cell))
            .map(|command| command.name.clone())
            .collect();
        self.acting = None;
        self.context_window = Some(window);
        // Look Up and the man page name what they take.
        let selection = self.looked_up(window).map(|(text, _)| text);
        let named = |name: &str| {
            let text = selection.as_deref()?;
            let (message, variable) = match name {
                LOOK_UP => ("menu-look-up", "word"),
                MAN_PAGE if man_page(text).is_some() => ("menu-man-page", "page"),
                _ => return None,
            };
            let text = shortened(text, LOOK_UP_TITLE);
            Some(
                self.texts
                    .text_with(message, &[(variable, Arg::Text(&text))]),
            )
        };
        let mut items = Vec::new();
        for group in groups.iter().filter(|group| !group.is_empty()) {
            if !items.is_empty() {
                items.push(MenuItem::Separator);
            }
            items.extend(group.iter().map(|command| MenuItem::Command {
                title: named(&command.name).unwrap_or_else(|| command.title.clone()),
                command: command.name.clone(),
                shortcut: None,
            }));
        }
        let menu = ContextMenu { items, dimmed };
        platform.show_context_menu(window, position, &menu);
    }

    /// Whether `command` would do something in `context`, at the view's
    /// cell `cell` for a command of a cell: its context holds, and its
    /// plugin, if one runs it, tells that it would.
    fn applies(
        &mut self,
        platform: &mut dyn Platform,
        command: &Command,
        context: &Context,
        (column, row): (u16, u16),
    ) -> bool {
        let when = command.when.as_deref().map(When::parse);
        if when.is_some_and(|when| when.is_ok_and(|when| !when.holds(context))) {
            return false;
        }
        if command.name == LOOK_UP {
            return self
                .acting
                .and_then(|window| self.looked_up(window))
                .is_some();
        }
        if command.name == MAN_PAGE {
            return self
                .acting
                .and_then(|window| self.looked_up(window))
                .is_some_and(|(text, _)| man_page(&text).is_some());
        }
        if !self.plugins.owns(&command.name) {
            return true;
        }
        let args = if of_cell(command) {
            cell_args(column, i64::from(row))
        } else {
            Args::new()
        };
        let applies = self.commands.check(&command.name, args).and_then(|args| {
            self.with_plugins(platform, |plugins, context| {
                plugins.applies(&command.name, &args, context)
            })
        });
        applies.unwrap_or_else(|error| {
            eprintln!("lokyr: {error}");
            false
        })
    }

    /// What Look Up looks up in `window`: the selection's text, of one line,
    /// without the spaces around it, and the cell where that text starts,
    /// in the view; `None` without one.
    fn looked_up(&self, window: WindowId) -> Option<(String, (u16, i64))> {
        let pane = self.workspace.pane(self.windows.get(&window)?.pane)?;
        let (column, row) = pane.selection()?.start;
        let selected = pane.selection_text()?;
        let text = selected.trim();
        if text.is_empty() || text.contains('\n') {
            return None;
        }
        // Each space before the text takes a cell.
        let spaces = selected[..selected.len() - selected.trim_start().len()]
            .chars()
            .count();
        let column = column.saturating_add(u16::try_from(spaces).unwrap_or(u16::MAX));
        Some((text.to_owned(), (column, row)))
    }

    /// Shows the system's definition of the selection of the window that
    /// the commands run in, over the selection's first cell, in the text's
    /// font.
    fn look_up(&mut self, platform: &mut dyn Platform) {
        let Some(window) = self.acting.or_else(|| platform.focused_window()) else {
            return;
        };
        let Some((text, (column, row))) = self.looked_up(window) else {
            return;
        };
        let (Some(size), Some(state)) = (platform.window_size(window), self.windows.get(&window))
        else {
            return;
        };
        let Some(pane) = self.workspace.pane(state.pane) else {
            return;
        };
        let grid = pane.grid();
        // A selection that starts above the view, or below it, shows at its
        // edge.
        let row = row.clamp(0, i64::from(grid.rows.saturating_sub(1))) as f64;
        let (left, top) = self.origin(size);
        let baseline = self
            .drawing
            .as_ref()
            .map_or(f64::from(grid.cell_height) * 0.75, |drawing| {
                f64::from(drawing.font.cell().baseline)
            });
        let at = Point {
            x: (f64::from(left) + f64::from(column) * f64::from(grid.cell_width)) / size.scale,
            y: (f64::from(top) + row * f64::from(grid.cell_height) + baseline) / size.scale,
        };
        let points = f64::from(self.font_size.unwrap_or(self.config.font.size.points()));
        let family = self
            .drawing
            .as_ref()
            .map(|drawing| drawing.font.family().to_owned());
        platform.show_definition(
            window,
            &text,
            at,
            family.as_deref().map(|family| (family, points)),
        );
    }

    /// Shows the performance's panel over every window, or hides it, as the
    /// menu chose, else as the config says, and ticks the menu's item: while
    /// it shows, its timer measures each second, which wakes the app, and
    /// draws every window once more.
    fn show_performance(&mut self, platform: &mut dyn Platform) {
        let shown = self
            .performance_chosen
            .unwrap_or(self.config.debug.performance_overlay);
        platform.set_command_checked(PERFORMANCE, shown);
        match (shown, self.performance.take()) {
            (true, None) => {
                let now = Instant::now();
                // The GPU's times from before it shows are not its own.
                if let Some(drawing) = &mut self.drawing {
                    drawing.renderer.take_gpu_times();
                }
                let windows: Vec<WindowId> = self.windows.keys().copied().collect();
                let bytes = self.output_bytes();
                let mut shown = Performance::new(now, &windows, platform.usage(), bytes);
                shown.timer = Some(platform.start_timer(now + performance::INTERVAL));
                self.performance = Some(shown);
            }
            (true, Some(shown)) => self.performance = Some(shown),
            (false, Some(hidden)) => {
                if let Some(timer) = hidden.timer {
                    platform.cancel_timer(timer);
                }
            }
            (false, None) => {}
        }
        for &window in self.windows.keys() {
            platform.request_frame(window);
        }
    }

    /// The performance's timer fired: it measures the second that went, a
    /// window's lines each, and asks every window for the frame that shows
    /// them.
    fn measure_performance(&mut self, platform: &mut dyn Platform) {
        let now = Instant::now();
        let bytes = self.output_bytes();
        let gpu = self
            .drawing
            .as_mut()
            .map(|drawing| drawing.renderer.take_gpu_times())
            .unwrap_or_default();
        let windows: Vec<WindowId> = self.windows.keys().copied().collect();
        let Some(shown) = &mut self.performance else {
            return;
        };
        let measures = shown.measure(now, &windows, gpu, platform.usage(), bytes);
        shown.keep(&windows);
        for (window, measure) in measures {
            eprintln!("{}", performance::log_line(window.get(), &measure));
            shown.set_lines(window, performance::lines(&measure, &self.texts));
            platform.request_frame(window);
        }
        shown.timer = Some(platform.start_timer(now + performance::INTERVAL));
    }

    /// The bytes that each pane's program wrote, by the pane's number.
    fn output_bytes(&self) -> BTreeMap<u64, u64> {
        self.workspace
            .panes()
            .map(|pane| (pane.id().get(), pane.output_bytes()))
            .collect()
    }

    /// Opens the config file in the system's text editor, made first, when
    /// it is not there, of every key commented out with its default and its
    /// text; an alert tells why it cannot, as without a home directory,
    /// which gives it no path.
    fn open_settings(&mut self, platform: &mut dyn Platform) {
        let failed = match &self.config_file {
            None => Some(self.texts.text("settings-no-home")),
            Some(file) => config::make(&file.path)
                .err()
                .map(|error| error.to_string())
                .or_else(|| {
                    platform
                        .open_in_text_editor(&file.path)
                        .err()
                        .map(|error| error.to_string())
                }),
        };
        if let Some(why) = failed {
            self.tell_settings_failed(platform, why);
        }
    }

    /// Tells, in an alert of the window that the commands run in, or of the
    /// one that has the focus, that Settings… opens nothing, and `why`, as
    /// the log does.
    fn tell_settings_failed(&mut self, platform: &mut dyn Platform, why: String) {
        eprintln!("lokyr: {why}");
        let alert = Alert {
            style: AlertStyle::Warning,
            message: self.texts.text("settings-failed-message"),
            detail: why,
            buttons: Vec::new(),
            suppression: None,
            input: None,
        };
        let window = self.acting.or_else(|| platform.focused_window());
        platform.show_alert(window, &alert);
    }

    /// Opens a window whose pane shows the man page that the selection of
    /// the window that the commands run in names, rather than the shell:
    /// `man`, which the user's login shell runs, as their `PATH` finds the
    /// pages of their own programs.
    fn open_man_page(&mut self, platform: &mut dyn Platform) {
        let Some(window) = self.acting.or_else(|| platform.focused_window()) else {
            return;
        };
        let Some((text, args)) = self
            .looked_up(window)
            .and_then(|(text, _)| Some((text.clone(), man_page(&text)?)))
        else {
            return;
        };
        let program = man_program(self.man.as_deref(), &args);
        self.open_window_running(platform, &program, Some(format!("man {text}")));
    }

    /// Runs `name`, which `window`'s context menu chose, in its pane, as
    /// though the window had the focus: a command of a cell takes the cell
    /// that the menu opened on, wherever output moved its text, and does
    /// not run once its text left the scrollback.
    fn context_command(&mut self, platform: &mut dyn Platform, window: WindowId, name: &str) {
        let Some(state) = self.windows.get(&window) else {
            return;
        };
        let id = state.pane;
        let pane = self.workspace.pane(id);
        // The cell's line and link are read with it, which the command
        // reads, until the mark goes after it.
        let marked = pane
            .filter(|_| self.context_window == Some(window))
            .and_then(Pane::marked_cell);
        let args = match self.commands.get(name) {
            Some(command) if of_cell(command) => marked.map(|(column, row)| cell_args(column, row)),
            _ => Some(Args::new()),
        };
        if let Some(args) = args {
            self.acting = Some(window);
            self.run(platform, name, args);
            self.acting = None;
        }
        if let Some(pane) = self.workspace.pane(id) {
            pane.unmark_cell();
        }
    }

    /// Whether `position`, in points of `window`'s content, is on the bar
    /// of the notice that its last frame drew, and on its close; `None` off
    /// the bar.
    fn on_notice(
        &self,
        platform: &dyn Platform,
        window: WindowId,
        position: Point,
    ) -> Option<bool> {
        let (state, size) = (self.windows.get(&window)?, platform.window_size(window)?);
        let pixels = (position.x * size.scale, position.y * size.scale);
        let rects = state
            .notice
            .filter(|rects| notice::contains(&rects.bar, pixels))?;
        Some(notice::contains(&rects.close, pixels))
    }

    /// Starts a selection at `position`, as the left button goes down with
    /// `mods`, of its `clicks` in a row: a click starts one of cells, which
    /// selects nothing until the pointer moves, and takes the selection that
    /// was there away; a double click selects the word, a triple click the
    /// line; Option makes a rectangle; Shift, where no program takes it,
    /// extends the last selection to the pointer.
    fn select_press(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        position: Point,
        (mods, clicks): (Modifiers, u32),
    ) {
        let Some(pointer) = self.pointer(platform, window, position) else {
            return;
        };
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        let Some(pane) = self.workspace.pane(state.pane) else {
            return;
        };
        let grid = pane.grid();
        let spot = mouse::spot(pointer, pane.view_offset(), grid.cell_width, grid.columns);
        let tracking = !pane.is_kept()
            && pane
                .input_modes()
                .is_some_and(|modes| modes.mouse_tracking != MouseTracking::None);
        match &mut state.selecting {
            Some(selecting) if mods.shift && !tracking && clicks <= 1 => {
                selecting.last = spot;
                selecting.held = true;
            }
            slot => {
                let unit = match clicks {
                    2 => SelectionUnit::Word,
                    3.. => SelectionUnit::Line,
                    _ => SelectionUnit::Cell,
                };
                let rectangle = (mods.left_alt || mods.right_alt) && unit == SelectionUnit::Cell;
                // The terminal keeps the anchor, which moves with its text.
                pane.anchor_selection((spot.col, spot.row + pane.view_offset() as i64));
                *slot = Some(Selecting {
                    anchor: spot,
                    last: spot,
                    shape: if rectangle {
                        SelectionShape::Rectangle
                    } else {
                        SelectionShape::Cells
                    },
                    unit,
                    held: true,
                    beyond: 0,
                });
            }
        }
        if let Some(selecting) = &mut state.selecting {
            show_selection(pane, selecting);
        }
        platform.request_frame(window);
    }

    /// The pointer dragged a selection to `position`: the selection takes
    /// the cells to it, and the view scrolls while it is past the grid's
    /// top or bottom.
    fn select_drag(&mut self, platform: &mut dyn Platform, window: WindowId, position: Point) {
        let Some(size) = platform.window_size(window) else {
            return;
        };
        let scale = size.scale;
        let Some(pointer) = self.pointer(platform, window, position) else {
            return;
        };
        let (_, top) = self.origin(size);
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        let Some(pane) = self.workspace.pane(state.pane) else {
            return;
        };
        let Some(selecting) = &mut state.selecting else {
            return;
        };
        let grid = pane.grid();
        selecting.last = mouse::spot(pointer, pane.view_offset(), grid.cell_width, grid.columns);
        selecting.beyond = mouse::beyond(position.y, scale, top, grid);
        show_selection(pane, selecting);
        platform.request_frame(window);
        if selecting.beyond != 0 && self.scroll_timer.is_none() {
            self.scroll_timer = Some(platform.start_timer(Instant::now() + SELECTION_SCROLL));
        }
    }

    /// The left button went up, which ends the selection that it made:
    /// with `copy-on-select`, its text goes to the clipboard.
    fn select_release(&mut self, platform: &mut dyn Platform, window: WindowId) {
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        if let Some(selecting) = &mut state.selecting {
            selecting.held = false;
            selecting.beyond = 0;
        }
        if self.config.clipboard.copy_on_select
            && let Some(text) = self
                .workspace
                .pane(state.pane)
                .and_then(Pane::selection_text)
            && !text.is_empty()
        {
            platform.set_clipboard_text(&text);
        }
    }

    /// The views under selections dragged past their grid scroll a step,
    /// of a row for each row that the pointer is past it, and the
    /// selections take the rows that come; the timer goes on while a view
    /// still scrolls.
    fn scroll_selections(&mut self, platform: &mut dyn Platform) {
        self.scroll_timer = None;
        let mut scrolled = false;
        for (&window, state) in &mut self.windows {
            let Some(selecting) = &mut state.selecting else {
                continue;
            };
            if !selecting.held || selecting.beyond == 0 {
                continue;
            }
            let Some(pane) = self.workspace.pane(state.pane) else {
                continue;
            };
            let rows = usize::try_from(selecting.beyond.unsigned_abs())
                .unwrap_or(usize::MAX)
                .min(SELECTION_SCROLL_ROWS);
            let before = pane.view_offset();
            pane.scroll(if selecting.beyond < 0 {
                Scroll::Up(rows)
            } else {
                Scroll::Down(rows)
            });
            let offset = pane.view_offset();
            // The pointer stays past the edge, on the row that the view
            // brings under it.
            let edge = if selecting.beyond < 0 {
                0
            } else {
                i64::from(pane.grid().rows) - 1
            };
            selecting.last.row = edge - offset as i64;
            show_selection(pane, selecting);
            platform.request_frame(window);
            scrolled |= offset != before;
        }
        if scrolled {
            self.scroll_timer = Some(platform.start_timer(Instant::now() + SELECTION_SCROLL));
        }
    }

    /// The pointer moved over `window`: a program that asked learns of it,
    /// once a cell, or a pixel for reports in pixels, at the screen's row,
    /// and not above the screen.
    fn mouse_moved(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        position: Point,
        mods: Modifiers,
    ) {
        let dragging = self
            .windows
            .get(&window)
            .and_then(|state| state.selecting)
            .is_some_and(|selecting| selecting.held);
        if dragging {
            self.select_drag(platform, window, position);
            return;
        }
        if mods.shift {
            return;
        }
        let Some(pointer) = self.pointer(platform, window, position) else {
            return;
        };
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        let Some(pane) = self
            .workspace
            .pane(state.pane)
            .filter(|pane| !pane.is_kept())
        else {
            return;
        };
        let Some(modes) = pane.input_modes() else {
            return;
        };
        if modes.mouse_tracking == MouseTracking::None {
            return;
        }
        let offset = pane.snapshot().offset();
        let Some(pointer) = mouse::on_screen(pointer, offset, pane.grid().cell_height) else {
            return;
        };
        let pixels = modes.mouse_encoding == MouseEncoding::SgrPixels;
        if state.mouse.moved(pointer, pixels) {
            let action = MouseAction::Motion(state.mouse.held());
            pane.mouse(&mouse_event(action, mods, pointer));
        }
    }

    /// The wheel or the trackpad scrolled over `window`, by `scroll`'s delta
    /// in its phase: each whole step goes to a program that takes the
    /// wheel, as the cursor keys on the alternate screen; otherwise, and
    /// with Shift, it moves the view through the scrollback.
    fn scroll(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        (delta, phase): (ScrollDelta, Option<ScrollPhase>),
        position: Point,
        mods: Modifiers,
    ) {
        let Some(scale) = platform.window_size(window).map(|size| size.scale) else {
            return;
        };
        let Some(pointer) = self.pointer(platform, window, position) else {
            return;
        };
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        let Some(pane) = self.workspace.pane(state.pane) else {
            return;
        };
        let row_points = f64::from(pane.grid().cell_height) / scale;
        let (across, up) = state.mouse.scroll(delta, phase, row_points, mods.shift);
        // A step that the program takes over the scrollback goes to it at
        // the screen's top row.
        let pointer = mouse::on_screen(pointer, pane.snapshot().offset(), pane.grid().cell_height)
            .unwrap_or(mouse::at_top(pointer));
        let to_program = !mods.shift && !pane.is_kept();
        // A binding of a step takes it first, before the program and the
        // view.
        let context = context(Some(pane));
        let bound = |wheel| match Resolver::new().press(
            &self.keymap,
            Pressed::wheel(wheel, binding_mods(mods)),
            &context,
        ) {
            Resolved::Run(invocation) => Some(invocation),
            _ => None,
        };
        let mut runs = Vec::new();
        let mut view: i64 = 0;
        let wheel = if up > 0 { Wheel::Up } else { Wheel::Down };
        let up_bound = bound(wheel);
        for _ in 0..up.unsigned_abs() {
            if let Some(invocation) = &up_bound {
                runs.push(invocation.clone());
                continue;
            }
            let event = mouse_event(MouseAction::Wheel(wheel), mods, pointer);
            if !(to_program && pane.mouse(&event)) {
                view += i64::from(up.signum());
            }
        }
        let wheel = if across > 0 {
            Wheel::Left
        } else {
            Wheel::Right
        };
        let across_bound = bound(wheel);
        for _ in 0..across.unsigned_abs() {
            if let Some(invocation) = &across_bound {
                runs.push(invocation.clone());
            } else if to_program {
                pane.mouse(&mouse_event(MouseAction::Wheel(wheel), mods, pointer));
            }
        }
        if view != 0 {
            let rows = usize::try_from(view.unsigned_abs()).unwrap_or(usize::MAX);
            pane.scroll(if view > 0 {
                Scroll::Up(rows)
            } else {
                Scroll::Down(rows)
            });
            platform.request_frame(window);
        }
        for invocation in runs {
            self.run(platform, &invocation.command, invocation.args);
        }
    }

    fn open_window(&mut self, platform: &mut dyn Platform) {
        let program = self.program.clone();
        self.open_window_running(platform, &program, None);
    }

    /// Opens a window, whose pane runs `program`, and quits if it cannot
    /// while no other window is open: of the man page of the title `page`,
    /// as `man ls`, of its size and colors, when it shows one.
    fn open_window_running(
        &mut self,
        platform: &mut dyn Platform,
        program: &Program,
        page: Option<String>,
    ) {
        // Of its cells, at the scale of the screen that it opens on; a man
        // page's of its own, which it neither keeps nor restores.
        let window_config = &self.config.window;
        let cells = match page {
            Some(_) => MAN_PAGE_CELLS,
            None => (
                u32::from(window_config.columns.count()),
                u32::from(window_config.rows.count()),
            ),
        };
        let remember = window_config.remember_frame && page.is_none();
        let size = self.size_of(platform.screen_scale(), cells);
        let environment = Environment {
            directory: self.start_directory(platform),
            ..self.environment.clone()
        };
        let attributes = WindowAttributes {
            title: page.clone().unwrap_or_else(|| TITLE.to_owned()),
            size,
            remember,
            // The first window as the app starts; the others take the size
            // that a window kept, cascaded.
            restore: remember && !self.opened_one,
            restore_size: remember && self.opened_one,
        };
        self.opened_one = true;
        match platform.create_window(&attributes) {
            Ok(window) => self.run_in(platform, window, (program, page), &environment),
            Err(error) => {
                eprintln!("lokyr: {error}");
                if self.windows.is_empty() && !self.global_held {
                    platform.quit();
                }
            }
        }
    }

    /// Gives the platform's new `window` a pane that runs `program` in
    /// `environment`, of the man page of the title `page`, as `man ls`, of
    /// its colors, when it shows one; and draws it.
    fn run_in(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        (program, page): (&Program, Option<String>),
        environment: &Environment,
    ) {
        let surfaces = (
            self.surface(platform, window),
            self.layer_surface(platform, window, Layer::BlinkingText),
            self.layer_surface(platform, window, Layer::Cursor),
        );
        let focused = platform.focused_window() == Some(window);
        let size = platform
            .window_size(window)
            .unwrap_or_else(|| WindowSize::new(WINDOW_SIZE, 1.0));
        let grid = self.grid(size);
        let colors = &match page {
            Some(_) => self.theme.man_page.colors.clone(),
            None => self.colors(platform.appearance()),
        };
        let waker = platform.waker();
        let wake = Arc::new(PaneWake::default());
        let woken = Arc::clone(&wake);
        let pane = Pane::spawn(
            program,
            environment,
            &self.settings(),
            grid,
            colors,
            &self.texts,
            move || {
                woken.wake();
                waker.wake();
            },
        );
        self.pane_wakes.insert(pane.id(), wake);
        pane.set_scroll_on_output(self.config.scrollback.scroll_on_output);
        pane.set_clipboard_reads(self.config.clipboard.read == ClipboardRead::Ask);
        let id = pane.id();
        let ids = (self.workspace.open_window(pane), id);
        let mut state = Window::new(surfaces, ids, focused, Instant::now());
        state.page = page;
        self.windows.insert(window, state);
        self.draw(platform, window, size);
    }

    /// Shows the quick terminal, at the place that the config gives, and
    /// opens it first, in the directory that `new-directory` gives; or
    /// hides it, if it shows with the focus. Shown without the focus, it
    /// takes the focus.
    fn toggle_quick(&mut self, platform: &mut dyn Platform) {
        let place = quick::place(&self.config.quick_terminal);
        match self.quick {
            Some((window, true))
                if self.windows.get(&window).is_some_and(|state| state.focused) =>
            {
                platform.show_quick_window(window, None);
                self.quick = Some((window, false));
            }
            Some((window, _)) => {
                platform.show_quick_window(window, Some(&place));
                self.quick = Some((window, true));
            }
            None => {
                let environment = Environment {
                    directory: self.start_directory(platform),
                    ..self.environment.clone()
                };
                match platform.create_quick_window(TITLE) {
                    Ok(window) => {
                        // Shown first, so that its pane starts of its size.
                        platform.show_quick_window(window, Some(&place));
                        let program = self.program.clone();
                        self.run_in(platform, window, (&program, None), &environment);
                        self.quick = Some((window, true));
                    }
                    Err(error) => eprintln!("lokyr: {error}"),
                }
            }
        }
    }

    /// Hides the quick terminal as `window`, which it is, lost the focus,
    /// if `autohide` says so: not for a question on it, which takes the
    /// focus.
    fn autohide(&mut self, platform: &mut dyn Platform, window: WindowId) {
        if self.quick != Some((window, true))
            || !self.config.quick_terminal.autohide
            || self.asks_on(window)
        {
            return;
        }
        platform.show_quick_window(window, None);
        self.quick = Some((window, false));
    }

    /// Whether a question of the app's is open on `window`.
    fn asks_on(&self, window: WindowId) -> bool {
        let pane = self.windows.get(&window).map(|state| state.pane);
        self.close_asks.values().any(|&asked| asked == window)
            || self.title_asks.values().any(|&asked| asked == window)
            || self
                .open_asks
                .values()
                .any(|(asked, _)| *asked == Some(window))
            || self
                .clipboard_asks
                .values()
                .any(|(asked, _)| Some(*asked) == pane)
            || self
                .paste_asks
                .values()
                .any(|(asked, _)| Some(*asked) == pane)
    }

    /// Takes the config's global keys from every app, in place of those
    /// before, and tells those that the platform refused, as another app
    /// takes them, and those of a character that the US layout has no key
    /// for, by which they go.
    fn take_global_keys(&mut self, platform: &mut dyn Platform) {
        let mut refused = Vec::new();
        let mut keys: Vec<GlobalKey> = Vec::new();
        self.global_keys.clear();
        for (combo, invocation) in keymap::global_bindings(&self.config.keys) {
            match quick::global_key(combo) {
                Some(key) => {
                    keys.push(key);
                    self.global_keys.push((combo, invocation));
                }
                None => refused.push(combo),
            }
        }
        let refused_keys = platform.set_global_keys(&keys);
        self.global_held = refused_keys.len() < keys.len();
        for index in refused_keys {
            if let Some(&(combo, _)) = self.global_keys.get(index) {
                refused.push(combo);
            }
        }
        if refused.is_empty() {
            self.resolve(platform, &About::GlobalKeys);
            return;
        }
        let named: Vec<String> = refused
            .iter()
            .map(|combo| format!("`{}{combo}`", keymap::GLOBAL))
            .collect();
        let text = format!(
            "{} cannot be taken from every app: another app holds it, or the Mac's keyboards have no such key",
            named.join(", ")
        );
        let diagnostic = Diagnostic::new(Code::Unavailable, text, About::GlobalKeys);
        self.report(platform, diagnostic);
    }

    /// The directory that a window that opens now starts in: the first
    /// window's, as the app starts, `start-directory` says; another's,
    /// `new-directory`, of the window that the commands run in, or the one
    /// that has the focus, or, while the app is not active, the one that
    /// had it last. `None` for the home directory, as a directory that is
    /// not there, or cannot be read, takes too, which the log tells.
    fn start_directory(&self, platform: &dyn Platform) -> Option<PathBuf> {
        let home = std::env::home_dir();
        let (key, chosen) = if self.opened_one {
            let chosen = match &self.config.window.new_directory {
                NewDirectory::Home => None,
                NewDirectory::Inherit => self
                    .acting
                    .or_else(|| platform.focused_window())
                    .or(self.front)
                    .and_then(|window| self.directory_of(window)),
                NewDirectory::Path(path) => config::expanded(path, home.as_deref()),
            };
            ("new-directory", chosen)
        } else {
            let chosen = match &self.config.window.start_directory {
                StartDirectory::Home => None,
                StartDirectory::Last => self.state.as_deref().and_then(state::last_directory),
                StartDirectory::Path(path) => config::expanded(path, home.as_deref()),
            };
            ("start-directory", chosen)
        };
        let chosen = chosen?;
        if !chosen.is_dir() {
            eprintln!(
                "lokyr: the window starts in the home directory, as `{key}`'s {} is not a directory that it can enter",
                chosen.display()
            );
            return None;
        }
        Some(chosen)
    }

    /// The directory of the program in `window`'s foreground, or, once it
    /// ended or where it cannot be read, the one that its title saw last.
    fn directory_of(&self, window: WindowId) -> Option<PathBuf> {
        let state = self.windows.get(&window)?;
        self.workspace
            .pane(state.pane)
            .and_then(Pane::working_directory)
            .or_else(|| state.directory.clone())
    }

    /// Keeps the directory of `window`, unless it shows a man page, as the
    /// last window's, for a next start in it.
    fn keep_directory(&self, window: WindowId) {
        let Some(kept) = &self.state else {
            return;
        };
        if self
            .windows
            .get(&window)
            .is_none_or(|shown| shown.page.is_some())
        {
            return;
        }
        if let Some(directory) = self.directory_of(window)
            && let Err(error) = state::keep_last_directory(kept, &directory)
        {
            eprintln!("lokyr: the last window's directory cannot be kept: {error}");
        }
    }

    /// Quits, as the directory of the window in front is kept as the last
    /// window's.
    fn quit(&mut self, platform: &mut dyn Platform) {
        if let Some(front) = self.front.or_else(|| platform.focused_window()) {
            self.keep_directory(front);
        }
        platform.quit();
    }

    /// Closes `window`, and quits with the last one.
    fn close(&mut self, platform: &mut dyn Platform, window: WindowId) {
        self.keep_directory(window);
        if self.front == Some(window) {
            self.front = None;
        }
        // The surface and the session go before the window, and the asks of
        // its program's reads with it.
        if let Some(state) = self.windows.remove(&window) {
            drop(state.surface);
            self.clipboard_asks
                .retain(|_, (pane, _)| *pane != state.pane);
            self.paste_asks.retain(|_, (pane, _)| *pane != state.pane);
            self.open_asks
                .retain(|_, (asked, _)| *asked != Some(window));
            self.close_asks.retain(|_, asked| *asked != window);
            self.title_asks.retain(|_, asked| *asked != window);
            self.pane_wakes.remove(&state.pane);
            self.workspace.close_window(state.window);
        }
        if self.quick.is_some_and(|(quick, _)| quick == window) {
            self.quick = None;
        }
        platform.close_window(window);
        // The secure entry goes with the window that had the focus, whose
        // loss of it the platform tells no more, unless another takes it.
        self.settle_secure_input(platform);
        // A global key goes on showing the quick terminal, which needs the
        // app. Without one, the last window quits, the hidden quick
        // terminal with it, after the question for a program in it.
        if self.global_held {
            return;
        }
        if self.windows.is_empty() {
            platform.quit();
        } else if self.windows.len() == 1 && self.quick.is_some_and(|(_, shown)| !shown) {
            self.ask_quit(platform, false);
        }
    }

    /// Does what `close-on-exit` says with the windows whose program has
    /// ended, and looks again soon for the status of those whose output
    /// ended before it.
    fn exits(&mut self, platform: &mut dyn Platform) {
        let now = Instant::now();
        let close = self.config.window.close_on_exit;
        let mut closing = Vec::new();
        let mut waiting = false;
        for (&window, state) in &self.windows {
            let Some(pane) = self.workspace.pane_mut(state.pane) else {
                continue;
            };
            // A man page's window keeps the page, from its top.
            let page = state.page.is_some();
            let close = if page { CloseOnExit::Never } else { close };
            match pane.exited(close, now, &self.texts) {
                Some(Exited::Close) => closing.push(window),
                Some(Exited::Keep) => {
                    if page {
                        pane.scroll(Scroll::Top);
                    }
                    platform.request_frame(window);
                }
                None => waiting |= pane.is_waiting(),
            }
        }
        for window in closing {
            self.close(platform, window);
        }
        if waiting && self.exit_timer.is_none() {
            self.exit_timer = Some(platform.start_timer(now + EXIT_POLL));
        }
    }

    /// What the keymap does with `key`, in `window`: runs a command, holds
    /// the key back as a sequence goes on, or gives it to the program,
    /// after the keys of a sequence that it did not go on with. A release
    /// of a key that the keymap took goes nowhere, but for a key of the
    /// sequence under way, whose release waits with it.
    fn resolve_key(&mut self, window: WindowId, key: &KeyEvent) -> Taken {
        let Some(state) = self.windows.get_mut(&window) else {
            return Taken::Held;
        };
        if key.state == KeyState::Released {
            if let Some(at) = state.taken.iter().position(|&code| code == key.code) {
                state.taken.remove(at);
                if state.held_back.iter().any(|held| held.code == key.code) {
                    state.held_back.push(key.clone());
                }
                return Taken::Held;
            }
            return Taken::Program(vec![key.clone()]);
        }
        // A key of the sequence under way, held, waits for the next.
        if key.state == KeyState::Repeated
            && state.resolver.is_pending()
            && state.taken.contains(&key.code)
        {
            return Taken::Held;
        }
        let event = keys::terminal_key(key);
        let pressed = event.as_ref().and_then(|event| {
            Pressed::key(event.key, binding_mods(key.mods), key.code.base_layout())
        });
        // A modifier or a lock alone neither takes a binding nor ends a
        // sequence; another key that the keymap does not name, as the
        // keypad's, ends it, and goes to the program after its keys.
        let Some(pressed) = pressed else {
            let unnamed = event.is_some_and(|event| !is_modifier(event.key));
            let mut keys = if unnamed {
                state.end_sequence()
            } else {
                Vec::new()
            };
            keys.push(key.clone());
            return Taken::Program(keys);
        };
        let context = context(self.workspace.pane(state.pane));
        match state.resolver.press(&self.keymap, pressed, &context) {
            Resolved::Run(invocation) => {
                state.held_back.clear();
                state.take(key.code);
                Taken::Run(invocation)
            }
            Resolved::Pending => {
                state.held_back.push(key.clone());
                state.take(key.code);
                Taken::Held
            }
            Resolved::Program { .. } => {
                let mut keys = state.end_sequence();
                keys.push(key.clone());
                Taken::Program(keys)
            }
        }
    }

    /// Runs the command `name` with `args`, as the menus and the keys ask,
    /// once the registry took them: in the plugin that registered it, or
    /// in the app; an error goes to the log.
    fn run(&mut self, platform: &mut dyn Platform, name: &str, args: Args) {
        self.run_deep(platform, name, args, 0);
    }

    /// Runs `name`, which commands of plugins `depth` deep ran.
    fn run_deep(&mut self, platform: &mut dyn Platform, name: &str, args: Args, depth: usize) {
        let ran = match self.commands.check(name, args) {
            Err(error) => Err(error),
            Ok(args) if self.plugins.owns(name) => {
                let ran = self.with_plugins(platform, |plugins, context| {
                    plugins.run(name, args, context)
                });
                self.after_plugins(platform, depth);
                ran
            }
            Ok(_) => match name {
                QUIT => {
                    self.ask_quit(platform, false);
                    Ok(())
                }
                SETTINGS => {
                    self.open_settings(platform);
                    Ok(())
                }
                PERFORMANCE => {
                    self.performance_chosen = Some(self.performance.is_none());
                    self.show_performance(platform);
                    Ok(())
                }
                SECURE_INPUT => {
                    self.secure_chosen = !self.secure_chosen;
                    platform.set_command_checked(SECURE_INPUT, self.secure_chosen);
                    self.settle_secure_input(platform);
                    Ok(())
                }
                NEW_WINDOW => {
                    self.open_window(platform);
                    Ok(())
                }
                TOGGLE_QUICK => {
                    self.toggle_quick(platform);
                    Ok(())
                }
                CLOSE_WINDOW => {
                    if let Some(window) = platform.focused_window() {
                        self.ask_close(platform, window);
                    }
                    Ok(())
                }
                SET_TITLE => {
                    if let Some(window) = self.acting.or_else(|| platform.focused_window()) {
                        self.ask_title(platform, window);
                    }
                    Ok(())
                }
                LOOK_UP => {
                    self.look_up(platform);
                    Ok(())
                }
                MAN_PAGE => {
                    self.open_man_page(platform);
                    Ok(())
                }
                _ if let Some(&(_, mode, _)) =
                    APPEARANCES.iter().find(|(command, ..)| *command == name) =>
                {
                    self.choose_appearance(platform, mode);
                    Ok(())
                }
                _ => Err(Error::new(
                    Code::Failed,
                    format!("`{name}` has no action in the app"),
                )),
            },
        };
        if let Err(error) = ran {
            self.report(
                platform,
                Diagnostic::new(error.code, error.message, About::Other),
            );
        }
    }

    /// Activates the plugins, which register their commands and bind their
    /// keys; one that fails is disabled, as the log tells.
    fn activate_plugins(&mut self, platform: &mut dyn Platform) {
        let errors = self.with_plugins(platform, Plugins::activate);
        for error in errors {
            self.report(
                platform,
                Diagnostic::new(error.code, error.message, About::Other),
            );
        }
        self.after_plugins(platform, 0);
    }

    /// Calls `call` with the plugins and what their host reaches of the
    /// app: the registry, the workspace, the platform's clipboard, the
    /// focused window, or the one that a context menu's commands run in,
    /// and the text's size.
    fn with_plugins<T>(
        &mut self,
        platform: &mut dyn Platform,
        call: impl FnOnce(&mut Plugins, &mut plugins::Context<'_>) -> T,
    ) -> T {
        let focused = self
            .acting
            .or_else(|| platform.focused_window())
            .and_then(|window| self.windows.get(&window))
            .map(|state| state.window);
        let font_size = self.font_size.unwrap_or(self.config.font.size.points());
        let confirm_paste = if self.pastes_unconfirmed {
            ConfirmPaste::Never
        } else {
            self.config.clipboard.confirm_paste
        };
        let mut clipboard = PlatformClipboard(platform);
        let mut context = plugins::Context {
            registry: &mut self.commands,
            workspace: &mut self.workspace,
            clipboard: &mut clipboard,
            focused,
            font_size,
            confirm_paste,
        };
        call(&mut self.plugins, &mut context)
    }

    /// Gives the platform the main menu of the commands and their keys,
    /// in place of the one before; the items that it dimmed are enabled
    /// again, until the next frame of the focused pane settles them.
    fn set_menu(&mut self, platform: &mut dyn Platform) {
        platform.set_main_menu(&main_menu(&self.commands, &self.keymap, &self.texts));
        for (command, enabled) in std::mem::take(&mut self.menu_enabled) {
            if !enabled {
                platform.set_command_enabled(&command, true);
            }
        }
        self.menu_contexts = menu_contexts(&self.commands);
    }

    /// Enables the menus' items of the commands that can run in `context`,
    /// and dims the others, as each command's own context says: only those
    /// whose state changes reach the platform.
    fn settle_menu(&mut self, platform: &mut dyn Platform, context: &Context) {
        for (command, when) in &self.menu_contexts {
            let enabled = when.holds(context);
            if self.menu_enabled.get(command) != Some(&enabled) {
                platform.set_command_enabled(command, enabled);
                self.menu_enabled.insert(command.clone(), enabled);
            }
        }
    }

    /// Takes what the plugins' calls changed and asked, once they
    /// returned: their bindings into the keymap, and their requests, of
    /// which a command runs `depth` deep; the windows draw again, as a
    /// call can change what a pane shows.
    fn after_plugins(&mut self, platform: &mut dyn Platform, depth: usize) {
        if self.plugins.take_bindings_changed() {
            self.keymap = Keymap::with_config(bindings(&self.plugins), &self.config.keys);
            self.set_menu(platform);
        }
        for request in self.plugins.take_requests() {
            match request {
                Request::Run(invocation) if depth < COMMANDS_DEEP => {
                    self.run_deep(platform, &invocation.command, invocation.args, depth + 1);
                }
                Request::Run(invocation) => eprintln!(
                    "lokyr: `{}` does not run, as commands that ran commands took it {COMMANDS_DEEP} deep",
                    invocation.command
                ),
                Request::OpenWindow => self.open_window(platform),
                Request::CloseWindow(window) => {
                    let shown = self
                        .windows
                        .iter()
                        .find(|(_, state)| state.window == window)
                        .map(|(&shown, _)| shown);
                    if let Some(shown) = shown {
                        self.ask_close(platform, shown);
                    }
                }
                Request::Quit => self.ask_quit(platform, false),
                Request::FontSize(size) => self.set_font_size(platform, size),
                Request::ConfirmPaste { pane, text } => self.ask_paste(platform, pane, text),
                Request::Open(target) => self.open_target(platform, target),
            }
        }
        for &window in self.windows.keys() {
            platform.request_frame(window);
        }
    }

    /// Gives the text `size`, in points, or the config's with `None`, in
    /// every window; a font that fails leaves the one in force, as the log
    /// tells.
    fn set_font_size(&mut self, platform: &mut dyn Platform, size: Option<f32>) {
        let font = sized(&self.config.font, size);
        if let Some(drawing) = &mut self.drawing
            && let Err(error) = drawing.set_font(font)
        {
            eprintln!("lokyr: {error}");
            return;
        }
        self.font_size = size;
        for &window in self.windows.keys() {
            platform.request_frame(window);
        }
    }

    /// Takes the workspace's events. A program's write to the clipboard
    /// puts its text there, and the primary selection, which macOS lacks,
    /// takes nothing; its read asks the user, once at a time for each pane.
    /// A window takes the title of its active tab, which its program's
    /// gives; a bell does what `[bell]` says, and a program's notification
    /// shows as the system's. The tabs keep their programs' colors, which
    /// the windows do not show, and a program's end waits for its status,
    /// which the exits take.
    /// Gathers what the sessions of `panes` told, and of those that waited
    /// for their turn, and acts on it; a timer brings the turn of those
    /// that wait still.
    fn collect(&mut self, platform: &mut dyn Platform, panes: &[PaneId]) {
        let due = self.workspace.collect(panes, Instant::now());
        self.events(platform);
        if let Some(due) = due
            && self.told_timer.is_none_or(|(_, at)| at > due)
        {
            if let Some((timer, _)) = self.told_timer.take() {
                platform.cancel_timer(timer);
            }
            self.told_timer = Some((platform.start_timer(due), due));
        }
    }

    fn events(&mut self, platform: &mut dyn Platform) {
        for event in self.workspace.take_events() {
            match event {
                Event::ClipboardWrite { target, data, .. } => {
                    if target.clipboard {
                        platform.set_clipboard_text(&String::from_utf8_lossy(&data));
                    }
                }
                Event::Terminal {
                    pane,
                    event: vt::Event::ClipboardRead(request),
                } => self.ask_clipboard_read(platform, pane, request),
                Event::TitleChanged(pane) => {
                    if let Some(window) = self.window_of(pane) {
                        self.show_title(platform, window);
                    }
                }
                Event::Bell(pane) => self.ring(platform, pane),
                Event::Terminal {
                    pane,
                    event: vt::Event::Notification { title, body },
                } => self.notify(platform, pane, &title, &body),
                Event::WindowOpened(_)
                | Event::WindowClosed(_)
                | Event::Ended(_)
                | Event::Terminal { .. } => {}
            }
        }
    }

    /// What a new window's terminal takes of the config: its `TERM`, its
    /// widths and normal form, and its scrollback's limit, within the
    /// budget that the windows share. The terminal's own `TERM`, without
    /// its entry, as outside the bundle before `cargo xtask terminfo`,
    /// gives way to `xterm-256color`, as the log tells, rather than leave
    /// the programs a terminal that they cannot know.
    fn settings(&self) -> Settings {
        let terminal = &self.config.terminal;
        let mut term = terminal.term.as_str();
        if matches!(term, "xterm-lokyr" | "lokyr") && self.environment.terminfo.is_none() {
            eprintln!(
                "lokyr: `{term}` has no terminfo entry here, outside the bundle: the window takes `{FALLBACK_TERM}`; `cargo xtask terminfo` compiles the entry for `cargo run`"
            );
            term = FALLBACK_TERM;
        }
        Settings {
            term: term.to_owned(),
            widths: terminal.widths(),
            normalization: terminal.normalization.form(),
            scrollback: Scrollback {
                limit: self.config.scrollback.limit.bytes(),
                budget: Some(Arc::clone(&self.budget)),
            },
        }
    }

    /// The platform's window that shows `pane`.
    fn window_of(&self, pane: PaneId) -> Option<WindowId> {
        self.windows
            .iter()
            .find(|(_, state)| state.pane == pane)
            .map(|(&window, _)| window)
    }

    /// Gives `window` the title of its active tab, or Lokyr's when its
    /// program gives none, after 🔔 when its bell rang while it had no
    /// focus.
    fn show_title(&mut self, platform: &mut dyn Platform, window: WindowId) {
        let title = self.title_of(window);
        let secure = self.secure_on;
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        let lock = if secure && state.focused {
            SECURE_MARK
        } else {
            ""
        };
        let mark = if state.bell_marked { BELL_MARK } else { "" };
        let title = format!("{lock}{mark}{title}");
        if state.title_shown.as_ref() != Some(&title) {
            platform.set_title(window, &title);
            state.title_shown = Some(title);
        }
    }

    /// Turns Secure Keyboard Entry on while the user chose it, or while the
    /// program in the window that has the focus hides what is typed, as at
    /// a password's prompt, which its terminal tells as output comes and
    /// before each key, never by a timer; off otherwise, and without a
    /// window that has the focus, as while another app is active. The
    /// focused window's title shows a lock while it is on.
    fn settle_secure_input(&mut self, platform: &mut dyn Platform) {
        let focused = self
            .windows
            .iter()
            .find(|(_, state)| state.focused)
            .map(|(&window, state)| (window, state.pane));
        let hides = focused
            .and_then(|(_, pane)| self.workspace.pane(pane))
            .is_some_and(Pane::hides_input);
        let on = focused.is_some() && (self.secure_chosen || hides);
        if on == self.secure_on {
            return;
        }
        platform.set_secure_input(on);
        self.secure_on = on;
        let windows: Vec<WindowId> = self.windows.keys().copied().collect();
        for window in windows {
            self.show_title(platform, window);
        }
    }

    /// The title of `window`, as [`App::base_title`] gives it, then the
    /// grid's size, as `— 100×33`, unless the config leaves it out.
    fn title_of(&self, window: WindowId) -> String {
        let title = self.base_title(window);
        let grid = self
            .windows
            .get(&window)
            .and_then(|state| self.workspace.pane(state.pane))
            .map(Pane::grid);
        match grid {
            Some(grid) if self.config.window.title_size => {
                format!("{title}{TITLE_DASH}{}×{}", grid.columns, grid.rows)
            }
            _ => title,
        }
    }

    /// The title of `window` before its size: its name, as
    /// [`App::window_name`] gives it, then the program in its foreground,
    /// as `lokyr — vim`, which a shell's title of its own, set at its
    /// prompt, would otherwise hide; but a man page's, and a program whose
    /// name is the title's.
    fn base_title(&self, window: WindowId) -> String {
        // The user's, in place of the rest.
        if let Some(title) = self
            .windows
            .get(&window)
            .and_then(|state| state.user_title.clone())
        {
            return title;
        }
        let name = self.window_name(window);
        let program = self
            .windows
            .get(&window)
            .filter(|state| state.page.is_none())
            .map(|state| state.place.1.as_str())
            .filter(|program| !program.is_empty() && *program != name);
        match program {
            Some(program) => format!("{name}{TITLE_DASH}{program}"),
            None => name,
        }
    }

    /// The name of `window`: its active tab's title, which its program
    /// gives; or its man page's; or the last part of its program's
    /// directory, as `lokyr`; or Lokyr's.
    fn window_name(&self, window: WindowId) -> String {
        let state = self.windows.get(&window);
        state
            .and_then(|state| self.workspace.window(state.window))
            .and_then(|shown| self.workspace.tab(shown.active_tab()))
            .map(workspace::Tab::title)
            .filter(|title| !title.is_empty())
            .or_else(|| state.and_then(|state| state.page.as_deref()))
            .or_else(|| {
                state
                    .map(|state| state.place.0.as_str())
                    .filter(|directory| !directory.is_empty())
            })
            .unwrap_or(TITLE)
            .to_owned()
    }

    /// Shows `window`'s title, if it changed, after a frame, which brought
    /// `output` or not: one of output looks again at the directory and the
    /// program in its foreground, while its program runs; a look reads the
    /// terminal's foreground group and its first process's path and
    /// directory, some microseconds. A frame of output comes after what a
    /// program wrote last, as `cd`'s prompt; the titles' timer looks once
    /// more [`TITLE_LOOK`] after a window's last one, for a program that
    /// started without a word after the echo of its command, as `sleep`:
    /// a wakeup or a few a burst, and none for the cursor's blinking.
    fn refresh_title(&mut self, platform: &mut dyn Platform, window: WindowId, output: bool) {
        if output && self.look_at_place(window) {
            let now = Instant::now();
            if let Some(state) = self.windows.get_mut(&window) {
                state.output_at = Some(now);
            }
            if let Some(after) = self.title_look
                && self.title_timer.is_none()
            {
                self.title_timer = Some((platform.start_timer(now + after), now + after));
            }
        }
        self.show_title(platform, window);
    }

    /// Looks at the directory and the program in `window`'s foreground,
    /// while its program runs: whether it did.
    fn look_at_place(&mut self, window: WindowId) -> bool {
        let Some(state) = self.windows.get_mut(&window) else {
            return false;
        };
        let Some(pane) = self.workspace.pane(state.pane) else {
            return false;
        };
        if pane.is_kept() {
            return false;
        }
        // A directory that cannot be read, as a program's of another user,
        // as `sudo`, keeps the one before.
        if let Some(directory) = pane.working_directory() {
            state.place.0 = directory_name(&directory, std::env::home_dir().as_deref());
            state.directory = Some(directory);
        }
        state.place.1 = pane.foreground_program().unwrap_or_default();
        true
    }

    /// The titles' timer fired: the windows whose last frame of output came
    /// [`TITLE_LOOK`] or more before the time that it was for look once
    /// more; the timer starts again for the others, at the first of their
    /// times.
    fn look_at_places_again(&mut self, platform: &mut dyn Platform) {
        let (Some((_, at)), Some(after)) = (self.title_timer.take(), self.title_look) else {
            return;
        };
        let mut next: Option<Instant> = None;
        let mut due = Vec::new();
        for (&window, state) in &mut self.windows {
            let Some(output) = state.output_at else {
                continue;
            };
            if output + after <= at {
                state.output_at = None;
                due.push(window);
            } else {
                next = Some(next.map_or(output + after, |next| next.min(output + after)));
            }
        }
        for window in due {
            self.look_at_place(window);
            self.show_title(platform, window);
        }
        if let Some(next) = next {
            self.title_timer = Some((platform.start_timer(next), next));
        }
    }

    /// Rings `pane`'s bell, as `[bell]` says: the system's sound; a flash
    /// of its window with the focus; for one without, a bounce of the
    /// Dock's icon and 🔔 in its title. A bell within [`BELL_PAUSE`] of the
    /// last that acted in the window does nothing.
    fn ring(&mut self, platform: &mut dyn Platform, pane: PaneId) {
        let Some(window) = self.window_of(pane) else {
            return;
        };
        let bell = self.config.bell;
        let now = Instant::now();
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        if state.rang_at.is_some_and(|at| now < at + BELL_PAUSE) {
            return;
        }
        state.rang_at = Some(now);
        if bell.sound {
            platform.play_alert_sound();
        }
        if state.focused {
            if bell.flash {
                state.flash_until = Some(now + FLASH);
                if let Some(timer) = self.flash_timer.take() {
                    platform.cancel_timer(timer);
                }
                self.flash_timer = Some(platform.start_timer(now + FLASH));
                platform.request_frame(window);
            }
            return;
        }
        if bell.bounce {
            platform.request_attention();
        }
        if bell.mark_title && !state.bell_marked {
            state.bell_marked = true;
            self.show_title(platform, window);
        }
    }

    /// Shows `pane`'s program's notification, of `title`, or, when that is
    /// empty, of the title that the user gave its window, or its window's
    /// name, and `body`, as the system's, while its window has no focus:
    /// one within [`NOTIFICATION_PAUSE`] of the last of the window shows
    /// nothing, and a long title or text shows its start.
    fn notify(&mut self, platform: &mut dyn Platform, pane: PaneId, title: &str, body: &str) {
        let Some(window) = self.window_of(pane) else {
            return;
        };
        let now = Instant::now();
        let title = if title.is_empty() {
            self.windows
                .get(&window)
                .and_then(|state| state.user_title.clone())
                .unwrap_or_else(|| self.window_name(window))
        } else {
            title.to_owned()
        };
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        if state.focused
            || state
                .notified_at
                .is_some_and(|at| now < at + NOTIFICATION_PAUSE)
        {
            return;
        }
        state.notified_at = Some(now);
        let title: String = title.chars().take(NOTIFICATION_TITLE).collect();
        let body: String = body.chars().take(NOTIFICATION_BODY).collect();
        platform.notify(window, &title, &body);
    }

    /// Asks the user, with an alert on its window, whether `pane`'s
    /// program can read the clipboard, as `request` asks: the default
    /// button refuses. A read that comes while the pane's ask is open gets
    /// no answer.
    fn ask_clipboard_read(
        &mut self,
        platform: &mut dyn Platform,
        pane: PaneId,
        request: ClipboardRequest,
    ) {
        if self
            .clipboard_asks
            .values()
            .any(|(asking, _)| *asking == pane)
        {
            return;
        }
        let Some((&window, _)) = self.windows.iter().find(|(_, state)| state.pane == pane) else {
            return;
        };
        let alert = Alert {
            style: AlertStyle::Warning,
            message: self.texts.text("clipboard-read-message"),
            detail: self.texts.text("clipboard-read-detail"),
            buttons: vec![
                self.texts.text("clipboard-read-deny"),
                self.texts.text("clipboard-read-allow"),
            ],
            suppression: None,
            input: None,
        };
        let alert = platform.show_alert(Some(window), &alert);
        self.clipboard_asks.insert(alert, (pane, request));
    }

    /// Pastes `text` into `window`'s program as a paste of the clipboard
    /// does: bracketed where the program asks, and asking first where
    /// `confirm-paste` says so.
    fn paste_into(&mut self, platform: &mut dyn Platform, window: WindowId, text: String) {
        let Some(id) = self.windows.get(&window).map(|state| state.pane) else {
            return;
        };
        let Some(pane) = self.workspace.pane(id) else {
            return;
        };
        let bracketed = pane
            .input_modes()
            .is_some_and(|modes| modes.bracketed_paste);
        let confirm = if self.pastes_unconfirmed {
            ConfirmPaste::Never
        } else {
            self.config.clipboard.confirm_paste
        };
        match paste::decide(&text, bracketed, confirm) {
            paste::Paste::Now(text) => pane.paste(&text),
            paste::Paste::Ask => self.ask_paste(platform, id, text),
        }
    }

    /// Asks the user to confirm a paste of `text` into `pane`, whose
    /// program takes its line breaks as Return: an alert in its window,
    /// with the text's first lines as they would reach the program. A
    /// paste while one waits for its answer goes.
    fn ask_paste(&mut self, platform: &mut dyn Platform, pane: PaneId, text: String) {
        if self.paste_asks.values().any(|(asking, _)| *asking == pane) {
            return;
        }
        let Some((&window, _)) = self.windows.iter().find(|(_, state)| state.pane == pane) else {
            return;
        };
        let lines = i64::try_from(paste::lines(&text)).unwrap_or(i64::MAX);
        let (most, width) = PREVIEW;
        let alert = Alert {
            style: AlertStyle::Warning,
            message: self
                .texts
                .text_with("paste-lines-message", &[("lines", Arg::Number(lines))]),
            detail: format!(
                "{}\n\n{}",
                self.texts.text("paste-lines-detail"),
                paste::preview(&text, most, width)
            ),
            buttons: vec![
                self.texts.text("paste-lines-paste"),
                self.texts.text("paste-lines-one-line"),
                self.texts.text("paste-lines-cancel"),
            ],
            suppression: Some(self.texts.text("paste-lines-dont-ask")),
            input: None,
        };
        let alert = platform.show_alert(Some(window), &alert);
        self.paste_asks.insert(alert, (pane, text));
    }

    /// Opens `target`, a URL or an absolute path, as a plugin asked, and as
    /// `[links]` says: a URL or a file asks the user first, by default,
    /// showing it in full; a directory opens, and a program shows in the
    /// file manager, never running.
    fn open_target(&mut self, platform: &mut dyn Platform, target: String) {
        let links = self.links();
        let opened = match platform.target_kind(&target) {
            Ok(kind @ TargetKind::Url) if links.urls == OpenUrl::Ask => {
                self.ask_open(platform, target, kind);
                return;
            }
            Ok(kind @ TargetKind::File) if links.files == OpenFile::Ask => {
                self.ask_open(platform, target, kind);
                return;
            }
            Ok(TargetKind::File) if links.files == OpenFile::Reveal => platform.reveal(&target),
            // A program shows in the file manager.
            Ok(_) => platform.open(&target),
            Err(error) => Err(error),
        };
        if let Err(error) = opened {
            eprintln!("lokyr: {error}");
        }
    }

    /// What links do: the config's `[links]`, or what the user chose where
    /// the config could not keep it.
    fn links(&self) -> LinksConfig {
        let (urls, files) = self.links_chosen;
        LinksConfig {
            urls: urls.unwrap_or(self.config.links.urls),
            files: files.unwrap_or(self.config.links.files),
        }
    }

    /// Asks the user whether to open `target`, a URL or a file, of `kind`,
    /// on the window that the commands run in, or the one that has the
    /// focus: a file can show in the file manager instead.
    fn ask_open(&mut self, platform: &mut dyn Platform, target: String, kind: TargetKind) {
        let file = kind == TargetKind::File;
        let (message, dont_ask) = if file {
            ("open-file-message", "open-file-dont-ask")
        } else {
            ("open-url-message", "open-url-dont-ask")
        };
        let mut buttons = vec![self.texts.text("open-open")];
        if file {
            buttons.push(self.texts.text("open-reveal"));
        }
        buttons.push(self.texts.text("open-cancel"));
        let alert = Alert {
            style: AlertStyle::Informational,
            message: self.texts.text(message),
            detail: shown(&target),
            buttons,
            suppression: Some(self.texts.text(dont_ask)),
            input: None,
        };
        let window = self.acting.or_else(|| platform.focused_window());
        let alert = platform.show_alert(window, &alert);
        self.open_asks.insert(alert, (window, (target, kind)));
    }

    /// The user answered the question before opening `target`, of `kind`,
    /// with `button`, its checkbox ticked when `suppressed`: it opens, or
    /// shows in the file manager, and, ticked, the config keeps the answer.
    fn answer_open(
        &mut self,
        platform: &mut dyn Platform,
        (target, kind): (String, TargetKind),
        button: usize,
        suppressed: bool,
    ) {
        let (done, chosen) = match (kind, button) {
            (TargetKind::File, 0) => (platform.open(&target), Chosen::File(OpenFile::Open)),
            (TargetKind::File, 1) => (platform.reveal(&target), Chosen::File(OpenFile::Reveal)),
            (_, 0) => (platform.open(&target), Chosen::Url(OpenUrl::Open)),
            _ => return,
        };
        if let Err(error) = done {
            eprintln!("lokyr: {error}");
        }
        if suppressed {
            self.remember_opening(chosen);
        }
    }

    /// Keeps `chosen`, what links do, as the user answered without wanting
    /// to be asked again: the config file takes it, or, if it cannot, it
    /// lasts until the app quits.
    fn remember_opening(&mut self, chosen: Chosen) {
        let (key, value) = match chosen {
            Chosen::Url(urls) => {
                self.config.links.urls = urls;
                ("urls", "open")
            }
            Chosen::File(files) => {
                self.config.links.files = files;
                (
                    "files",
                    if files == OpenFile::Reveal {
                        "reveal"
                    } else {
                        "open"
                    },
                )
            }
        };
        let written = match &self.config_file {
            Some(file) => config::set_string(&file.path, "links", key, value)
                .map_err(|error| error.to_string()),
            None => Err("there is no config file, without a home directory".to_owned()),
        };
        if let Err(error) = written {
            eprintln!("lokyr: {error}");
            eprintln!("lokyr: links {key} {value} until Lokyr quits");
            match chosen {
                Chosen::Url(urls) => self.links_chosen.0 = Some(urls),
                Chosen::File(files) => self.links_chosen.1 = Some(files),
            }
        }
    }

    /// Asks the user for a title of `window`, which takes the place of its
    /// program's and of its own, in a field that holds the one that the
    /// user gave before; an empty one gives the window's own back. A window
    /// that waits for the answer asks nothing more.
    fn ask_title(&mut self, platform: &mut dyn Platform, window: WindowId) {
        if self.title_asks.values().any(|&asked| asked == window) {
            return;
        }
        let Some(state) = self.windows.get(&window) else {
            return;
        };
        let alert = Alert {
            style: AlertStyle::Informational,
            message: self.texts.text("set-title-message"),
            detail: self.texts.text("set-title-detail"),
            buttons: vec![
                self.texts.text("set-title-set"),
                self.texts.text("set-title-cancel"),
            ],
            suppression: None,
            input: Some(state.user_title.clone().unwrap_or_default()),
        };
        let asked = platform.show_alert(Some(window), &alert);
        self.title_asks.insert(asked, window);
    }

    /// Closes `window`, once the user confirms it while a program other
    /// than a shell runs in it, which closing ends; a window that waits for
    /// the answer asks nothing more.
    fn ask_close(&mut self, platform: &mut dyn Platform, window: WindowId) {
        if self.close_asks.values().any(|&asked| asked == window) {
            return;
        }
        let Some(state) = self.windows.get(&window) else {
            return;
        };
        let Some(program) = self.workspace.pane(state.pane).and_then(Pane::running) else {
            self.close(platform, window);
            return;
        };
        let program = self.program_name(program);
        let alert = Alert {
            style: AlertStyle::Warning,
            message: self.texts.text("close-running-message"),
            detail: self
                .texts
                .text_with("close-running-detail", &[("program", Arg::Text(&program))]),
            buttons: vec![
                self.texts.text("close-running-close"),
                self.texts.text("close-running-cancel"),
            ],
            suppression: None,
            input: None,
        };
        let alert = platform.show_alert(Some(window), &alert);
        self.close_asks.insert(alert, window);
    }

    /// Quits, once the user confirms it while programs other than shells
    /// run, which quitting ends: the system's request, when `system`, waits
    /// for the answer, and gives up when the user cancels. A question that
    /// waits for its answer takes the request too.
    fn ask_quit(&mut self, platform: &mut dyn Platform, system: bool) {
        self.system_quit |= system;
        if self.quit_ask.is_some() {
            return;
        }
        let mut programs = Vec::new();
        let mut windows = 0;
        for state in self.windows.values() {
            if let Some(program) = self.workspace.pane(state.pane).and_then(Pane::running) {
                windows += 1;
                let program = self.program_name(program);
                if !programs.contains(&program) {
                    programs.push(program);
                }
            }
        }
        if windows == 0 {
            self.system_quit = false;
            self.quit(platform);
            return;
        }
        let programs = programs.join(", ");
        let alert = Alert {
            style: AlertStyle::Warning,
            message: self.texts.text("quit-running-message"),
            detail: self.texts.text_with(
                "quit-running-detail",
                &[
                    ("windows", Arg::Number(windows)),
                    ("programs", Arg::Text(&programs)),
                ],
            ),
            buttons: vec![
                self.texts.text("quit-running-quit"),
                self.texts.text("quit-running-cancel"),
            ],
            suppression: None,
            input: None,
        };
        self.quit_ask = Some(platform.show_alert(None, &alert));
    }

    /// The name of a program that runs, or "a program" for one whose name
    /// could not be read.
    fn program_name(&self, program: String) -> String {
        if program.is_empty() {
            self.texts.text("program-unknown")
        } else {
            program
        }
    }

    /// Pastes no longer ask, as the user said with the question's
    /// checkbox: the config file takes `confirm-paste = "never"`, or, if it
    /// cannot, as the log tells, the app keeps it until it quits.
    fn stop_confirming_pastes(&mut self) {
        self.config.clipboard.confirm_paste = ConfirmPaste::Never;
        let written = match &self.config_file {
            Some(file) => config::set_string(&file.path, "clipboard", "confirm-paste", "never")
                .map_err(|error| error.to_string()),
            None => Err("there is no config file, without a home directory".to_owned()),
        };
        if let Err(error) = written {
            eprintln!("lokyr: {error}");
            eprintln!("lokyr: pastes are not confirmed until Lokyr quits");
            self.pastes_unconfirmed = true;
        }
    }

    /// Gives the app the appearance `mode`, as the user chose it from the
    /// menu: the config file takes it, or, if it cannot, as the log tells,
    /// the app keeps it until it quits, or until the config's key changes.
    fn choose_appearance(&mut self, platform: &mut dyn Platform, mode: AppearanceMode) {
        let written = match &self.config_file {
            Some(file) => config::set_string(&file.path, "app", "appearance", mode.name())
                .map_err(|error| error.to_string()),
            None => Err("there is no config file, without a home directory".to_owned()),
        };
        match written {
            Ok(()) => {
                self.config.app.appearance = mode;
                self.appearance_chosen = None;
            }
            Err(error) => {
                eprintln!("lokyr: {error}");
                eprintln!("lokyr: the appearance is {} until Lokyr quits", mode.name());
                self.appearance_chosen = Some(mode);
            }
        }
        self.show_appearance(platform);
    }

    /// Gives the platform the appearance that the user chose, or the
    /// system's, ticks its item in the View menu, and draws the panes in
    /// its theme's variant.
    fn show_appearance(&mut self, platform: &mut dyn Platform) {
        let mode = self.appearance_chosen.unwrap_or(self.config.app.appearance);
        platform.set_appearance(match mode {
            AppearanceMode::System => None,
            AppearanceMode::Light => Some(Appearance::Light),
            AppearanceMode::Dark => Some(Appearance::Dark),
        });
        for (command, choice, _) in APPEARANCES {
            platform.set_command_checked(command, choice == mode);
        }
        let appearance = platform.appearance();
        self.appearance_changed(platform, appearance);
    }

    /// The app's appearance is `appearance`: the panes take its theme's
    /// variant, and the windows draw again.
    fn appearance_changed(&mut self, platform: &mut dyn Platform, appearance: Appearance) {
        let colors = self.colors(appearance);
        for state in self.windows.values().filter(|state| state.page.is_none()) {
            if let Some(pane) = self.workspace.pane(state.pane) {
                pane.set_colors(&colors);
            }
        }
        let windows: Vec<WindowId> = self.windows.keys().copied().collect();
        for window in windows {
            if let Some(size) = platform.window_size(window) {
                self.draw(platform, window, size);
            }
        }
    }

    /// The theme's colors in `appearance`.
    fn colors(&self, appearance: Appearance) -> lokyr_vt_core::terminal::Colors {
        self.theme
            .variant(appearance == Appearance::Dark)
            .colors
            .clone()
    }

    /// The grid that fits a window of `size`, less its padding, in the
    /// cells of the font at its display's scale.
    fn grid(&mut self, size: WindowSize) -> Grid {
        let (x, y) = self.padding(size.scale);
        let (width, height) = self.cell_pixels(size.scale);
        let fit = |pixels: u32, padding: u32, cell: u32| {
            (pixels.saturating_sub(2 * padding) / cell.max(1)).clamp(1, u32::from(u16::MAX)) as u16
        };
        let pixels = |cell: u32| u16::try_from(cell).unwrap_or(u16::MAX);
        // The titlebar is over the content's top.
        let below = size.pixels.height.saturating_sub(size.bar_pixels());
        Grid {
            columns: fit(size.pixels.width, x, width),
            rows: fit(below, y, height),
            cell_width: pixels(width),
            cell_height: pixels(height),
        }
    }

    /// The size of a cell, in pixels, of the font at a display's `scale`.
    fn cell_pixels(&mut self, scale: f64) -> (u32, u32) {
        match &mut self.drawing {
            Some(drawing) => {
                drawing.font.set_scale(scale as f32);
                let cell = drawing.font.cell();
                (cell.width, cell.height)
            }
            None => UNDRAWN_CELL,
        }
    }

    /// The size, in points, of a window's content of `cells`, its columns
    /// and its rows, with its padding, at a display's `scale`.
    fn size_of(&mut self, scale: f64, (columns, rows): (u32, u32)) -> Size {
        let (x, y) = self.padding(scale);
        let (width, height) = self.cell_pixels(scale);
        Size {
            width: f64::from(columns * width + 2 * x) / scale,
            height: f64::from(rows * height + 2 * y) / scale,
        }
    }

    /// The variant of the theme that `window` takes: a man page's, or the
    /// app's appearance's.
    fn variant(&self, platform: &dyn Platform, window: WindowId) -> &Variant {
        if self
            .windows
            .get(&window)
            .is_some_and(|state| state.page.is_some())
        {
            return &self.theme.man_page;
        }
        self.theme
            .variant(platform.appearance() == Appearance::Dark)
    }

    /// The backdrop of a window whose terminal's default background is
    /// `background`, as the config's opacity and blur make it: `None` for
    /// an opaque one.
    fn backdrop(&self, background: Color) -> Option<Backdrop> {
        let window = &self.config.window;
        window
            .background_opacity
            .is_translucent()
            .then(|| Backdrop {
                color: [
                    f64::from(background.red),
                    f64::from(background.green),
                    f64::from(background.blue),
                    f64::from(window.background_opacity.value()),
                ],
                blur: f64::from(window.background_blur.points()),
            })
    }

    /// Where a window of `size` has its grid's first cell, in pixels from
    /// its content's top left corner: past the padding, and below the
    /// titlebar.
    fn origin(&self, size: WindowSize) -> (u32, u32) {
        let (x, y) = self.padding(size.scale);
        (x, y + size.bar_pixels())
    }

    /// The room between the grid and the window's edges, in pixels on a
    /// display of `scale`: at the left and the right, and at the top and
    /// the bottom.
    fn padding(&self, scale: f64) -> (u32, u32) {
        let window = &self.config.window;
        (
            window.padding_x.pixels(scale),
            window.padding_y.pixels(scale),
        )
    }

    /// The surface of `window`, if the app draws and the platform has a
    /// layer for it.
    fn surface(&self, platform: &dyn Platform, window: WindowId) -> Option<Surface> {
        let drawing = self.drawing.as_ref()?;
        let handle = platform.window_handle(window)?;
        Surface::new(&drawing.gpu, handle)
            .inspect_err(|error| eprintln!("lokyr: {error}"))
            .ok()
    }

    /// The surface of `layer` of `window`, if the app draws and the
    /// platform has the layer.
    fn layer_surface(
        &self,
        platform: &dyn Platform,
        window: WindowId,
        layer: Layer,
    ) -> Option<Surface> {
        let drawing = self.drawing.as_ref()?;
        let handle = platform.layer_handle(window, layer)?;
        Surface::new(&drawing.gpu, handle)
            .inspect_err(|error| eprintln!("lokyr: {error}"))
            .ok()
    }

    /// The shape that the cursor of `window` takes at `now`, and how its
    /// layer blinks, which the window server runs; or, for a window without
    /// that layer, `None` while its blinking hides it. A cursor that blinks
    /// keeps the timer of its next change running: of the blinking's end
    /// alone, on a layer.
    fn cursor(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        now: Instant,
    ) -> Option<(CursorShape, Option<PlatformBlink>)> {
        let state = self.windows.get_mut(&window)?;
        let program = self.workspace.pane(state.pane)?.snapshot().cursor();
        let config = self.config.cursor;
        let shape = cursor::shape(&program, &config, state.focused);
        let blinking =
            cursor::blinks(&program, &config, state.focused) && state.blink.blinks(now, &config);
        // A cursor that stopped blinking stays on, and takes no timer.
        state.blinking = blinking;
        if !blinking {
            return Some((shape, None));
        }
        if state.cursor_layer.surface.is_some() || self.layers {
            let (since, stops) = (state.blink.since(), state.blink.stops(&config));
            if let Some(stops) = stops {
                self.blink_at(platform, stops);
            }
            return Some((shape, Some(blink_of(config.pattern(), since))));
        }
        let (on, next) = state.blink.at(now, &config);
        if let Some(next) = next {
            self.blink_at(platform, next);
        }
        on.then_some((shape, None))
    }

    /// Whether the text that blinks in `window` shows in its grid at `now`,
    /// and how its layer blinks, which the window server runs. It blinks in
    /// the window that has the focus, on the cursor's clock, switched at the
    /// cursor's interval until its timeout, and stops shown; on a layer, the
    /// grid hides it, and the layer blinks it, with the timer of the
    /// blinking's end alone; without, the frames show and hide it, each with
    /// the timer of the next change.
    fn text_blink(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        now: Instant,
    ) -> (bool, Option<PlatformBlink>) {
        let (text, config) = (self.config.text, self.config.cursor);
        let Some(state) = self.windows.get_mut(&window) else {
            return (true, None);
        };
        let blinking = text.blink
            && state.focused
            && self
                .workspace
                .pane(state.pane)
                .is_some_and(|pane| cells::has_blinking_text(pane.snapshot()));
        if !blinking {
            return (true, None);
        }
        if state.text_layer.surface.is_some() || self.layers {
            if !state.blink.blinks(now, &config) {
                return (true, None);
            }
            let switched = cursor::CursorConfig {
                smooth_blink: cursor::SmoothBlink::Off,
                ..config
            };
            let blink = blink_of(switched.pattern(), state.blink.since());
            if let Some(stops) = state.blink.stops(&config) {
                state.blinking = true;
                self.blink_at(platform, stops);
            }
            return (false, Some(blink));
        }
        let (shown, next) = state.blink.at(now, &config);
        if let Some(next) = next {
            state.blinking = true;
            self.blink_at(platform, next);
        }
        (shown, None)
    }

    /// Keeps the timer of the blinking cursors and text at `at`, unless
    /// one is due sooner.
    fn blink_at(&mut self, platform: &mut dyn Platform, at: Instant) {
        if self
            .blink_timer
            .is_some_and(|(_, current)| current <= at && Instant::now() < current)
        {
            return;
        }
        if let Some((timer, _)) = self.blink_timer.take() {
            platform.cancel_timer(timer);
        }
        self.blink_timer = Some((platform.start_timer(at), at));
    }

    /// Keeps the timer of the labels of the grids' sizes at `at`, unless one
    /// is due sooner.
    fn label_at(&mut self, platform: &mut dyn Platform, at: Instant) {
        if self.label_timer.is_some_and(|(_, current)| current <= at) {
            return;
        }
        if let Some((timer, _)) = self.label_timer.take() {
            platform.cancel_timer(timer);
        }
        self.label_timer = Some((platform.start_timer(at), at));
    }

    /// Keeps the timer of the programs' sizes at `at`, unless it comes
    /// sooner.
    fn pace_at(&mut self, platform: &mut dyn Platform, at: Instant) {
        if self.resize_timer.is_some_and(|(_, current)| current <= at) {
            return;
        }
        if let Some((timer, _)) = self.resize_timer.take() {
            platform.cancel_timer(timer);
        }
        self.resize_timer = Some((platform.start_timer(at), at));
    }

    /// Draws `window`, of `size`: its program's screen, brought up to date,
    /// in the grid that fits its content within the padding, with the
    /// cursor.
    fn draw(&mut self, platform: &mut dyn Platform, window: WindowId, size: WindowSize) {
        // The frame's time, which the performance's panel tells, the wait
        // for the display's drawable apart from the CPU's.
        let started = self.performance.is_some().then(Instant::now);
        let grid = self.grid(size);
        let (left, top) = self.origin(size);
        let now = Instant::now();
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        let Some(pane) = self.workspace.pane_mut(state.pane) else {
            return;
        };
        let due = pane.resize(grid, now);
        // A resize that changes the grid shows its size, a moment.
        let resized = state
            .grid_drawn
            .is_some_and(|drawn| drawn != (grid.columns, grid.rows));
        state.grid_drawn = Some((grid.columns, grid.rows));
        if resized {
            state.size_label = Some(now + size_label::HOLD);
        }
        // Output starts the cursor blinking again, on, as a key does.
        let output = pane.update();
        if output {
            state.blink.restart(now);
        }
        // The pointer's moves without a button wake the app only for a
        // program that asked for them all.
        let moves = pane
            .input_modes()
            .is_some_and(|modes| modes.mouse_tracking == MouseTracking::AnyEvent);
        if moves != state.mouse_moves {
            platform.track_mouse_moves(window, moves);
            state.mouse_moves = moves;
        }
        // The pointer is the text's I-beam, as the text selects; the arrow
        // over a program that takes the mouse's clicks.
        let takes_clicks = !pane.is_kept()
            && pane
                .input_modes()
                .is_some_and(|modes| modes.mouse_tracking != MouseTracking::None);
        let pointer = if takes_clicks {
            PointerShape::Arrow
        } else {
            PointerShape::Text
        };
        if pointer != state.pointer {
            platform.set_pointer_shape(window, pointer);
            state.pointer = pointer;
        }
        // The input method's candidates go at the cursor's cell, or at the
        // composition's start, which can be left of it, near the right edge.
        let snapshot = pane.snapshot();
        let at = snapshot.cursor().position;
        let (column, row) = state
            .preedit
            .as_ref()
            .and_then(|preedit| cells::composition_start(snapshot, preedit, snapshot.cols()))
            .map_or((at.x, at.y), |(row, column)| (column, row));
        let (width, height) = (f64::from(grid.cell_width), f64::from(grid.cell_height));
        platform.set_text_area(
            window,
            Point {
                x: (f64::from(left) + f64::from(column) * width) / size.scale,
                y: (f64::from(top) + f64::from(row) * height) / size.scale,
            },
            Size {
                width: width / size.scale,
                height: height / size.scale,
            },
        );
        // A translucent window's backdrop is the terminal's default
        // background, under the whole window, the titlebar too, and follows
        // it, as the theme, the program or reverse video change it.
        let background = cells::background(pane.snapshot());
        let sent = state.backdrop;
        let backdrop = self.backdrop(background);
        if backdrop != sent {
            platform.set_translucent(window, backdrop);
            if let Some(state) = self.windows.get_mut(&window) {
                state.backdrop = backdrop;
            }
        }
        // The title reads over the background, as the window takes the
        // appearance of its background where it is not the app's.
        let own = appearance_over(background, platform.appearance());
        if let Some(state) = self.windows.get_mut(&window)
            && state.own_appearance != Some(own)
        {
            platform.set_window_appearance(window, own);
            state.own_appearance = Some(own);
        }
        self.refresh_title(platform, window, output);
        if resized {
            self.label_at(platform, now + size_label::HOLD);
        }
        if output && self.windows.get(&window).is_some_and(|state| state.focused) {
            self.settle_secure_input(platform);
        }
        if let Some(due) = due {
            self.pace_at(platform, due);
        }
        // The menus' items as the focused pane's frame has it.
        let context = self
            .windows
            .get(&window)
            .filter(|state| state.focused)
            .and_then(|state| self.workspace.pane(state.pane))
            .map(|pane| menu_context(pane.snapshot()));
        if let Some(context) = context {
            self.settle_menu(platform, &context);
        }
        let shown_cursor = self.cursor(platform, window, Instant::now());
        let (blink_shown, text_blink) = self.text_blink(platform, window, Instant::now());
        let text = self.config.text;
        let look = TextLook {
            config: text,
            blink_shown,
        };
        let config = self.config.cursor;
        let selection_colors = self.variant(platform, window).selection;
        let (Some(drawing), Some(state)) = (self.drawing.as_mut(), self.windows.get_mut(&window))
        else {
            return;
        };
        let Some(pane) = self.workspace.pane(state.pane) else {
            return;
        };
        if state.surface.is_none() || state.occluded {
            return;
        }
        // The glyphs of the frames that the GPU still draws, of any window,
        // stay in the atlas.
        let oldest = drawing.renderer.drawing_from();
        drawing.font.set_drawing(oldest);
        drawing.font.begin_frame();
        drawing.font.set_text(self.config.text);
        let cell = *drawing.font.cell();
        let cursor = shown_cursor.map(|(shape, _)| CursorLook {
            shape,
            thickness: config.thickness(&cell, size.scale as f32),
        });
        // A cursor of a layer of its own, which blinks apart from the grid.
        let apart = state.cursor_layer.surface.is_some();
        let snapshot = pane.snapshot();
        let font = &mut drawing.font;
        let (x, y) = (
            i32::try_from(left).unwrap_or(i32::MAX),
            i32::try_from(top).unwrap_or(i32::MAX),
        );
        let selected = snapshot.selection();
        let contains =
            |column: u16, row: u16| selected.is_some_and(|shown| shown.contains(column, row));
        let selection = selected.map(|_| cells::Selection {
            colors: selection_colors,
            contains: &contains,
        });
        let mut batch = cells::cells_with(
            snapshot,
            &cell,
            (x, y),
            selection.as_ref(),
            cursor.filter(|_| !apart),
            look,
            font,
        );
        if let Some(preedit) = &state.preedit {
            cells::preedit(&mut batch, snapshot, &cell, preedit, font);
        }
        let cursor_cells = cursor.filter(|_| apart).and_then(|look| {
            cells::cursor_cells(&batch, snapshot, look, &cell, state.preedit.as_ref())
        });
        // The text that blinks, which its layer shows, as the grid hides it.
        let preedit = state.preedit.as_ref();
        let text_cells = text_blink
            .filter(|_| state.text_layer.surface.is_some())
            .and_then(|_| {
                cells::blinking_cells(snapshot, &cell, selection.as_ref(), text, preedit, font)
            });
        // Over a backdrop, what the cells leave shows it, and the titlebar
        // too, unless the config keeps it opaque.
        let opacity = if backdrop.is_some() { 0.0 } else { 1.0 };
        let opaque_bar = backdrop.is_some() && self.config.window.opaque_titlebar;
        let mut items: Vec<Item> = opaque_bar
            .then(|| bar_item(size, background))
            .flatten()
            .into_iter()
            .collect();
        items.push(Item::Cells(batch));
        // The notice, across the content's top, under the titlebar.
        state.notice = self.diagnostics.shown().map(|shown| {
            let label = match (&shown.about, shown.category()) {
                (About::Config, _) => "notice-config",
                (About::Font, _) => "notice-font",
                (About::GlobalKeys, _) => "notice-global-keys",
                (About::Other, ErrorCategory::Fault) => "notice-fault",
                (About::Other, _) => "notice-failed",
            };
            let label = self.texts.text(label);
            let fault = shown.category() == ErrorCategory::Fault;
            let area = (size.pixels.width, size.bar_pixels());
            let glyph = |text: &cells::CellText<'_>| font.glyph(text);
            let (rects, drawn) = notice::bar((&label, &shown.text, fault), &cell, area, glyph);
            items.extend(drawn);
            rects
        });
        // The hand over its close, the arrow over the rest of the bar,
        // which the platform shows without the app.
        let points = |rect: &Rect, shape| PointerArea {
            origin: Point {
                x: f64::from(rect.x) / size.scale,
                y: f64::from(rect.y) / size.scale,
            },
            size: Size {
                width: f64::from(rect.width) / size.scale,
                height: f64::from(rect.height) / size.scale,
            },
            shape,
        };
        let areas: Vec<PointerArea> = state
            .notice
            .iter()
            .flat_map(|rects| {
                [
                    points(&rects.close, PointerShape::PointingHand),
                    points(&rects.bar, PointerShape::Arrow),
                ]
            })
            .collect();
        if areas != state.pointer_areas {
            platform.set_pointer_areas(window, &areas);
            state.pointer_areas = areas;
        }
        // The grid's size, at the content's centre, while the window
        // resizes.
        let labelled = state
            .size_label
            .is_some_and(|until| state.live_resize || Instant::now() < until);
        if labelled {
            let text = size_label::text(grid.columns, grid.rows);
            let area = (size.pixels.width, size.pixels.height, size.bar_pixels());
            let glyph = |shown: &cells::CellText<'_>| font.glyph(shown);
            items.extend(size_label::label(&text, &cell, area, glyph));
        }
        // A flash of the bell, the default text's color over the window.
        if state
            .flash_until
            .is_some_and(|until| Instant::now() < until)
        {
            let text = snapshot.colors().foreground;
            items.push(Item::Rect {
                rect: Rect {
                    x: 0,
                    y: 0,
                    width: size.pixels.width,
                    height: size.pixels.height,
                },
                color: Color::from_rgb8(text.r, text.g, text.b).with_alpha(FLASH_OPACITY),
            });
        }
        // The performance's panel, over everything.
        if let Some(shown) = &self.performance {
            let measuring;
            let lines = match shown.lines(window) {
                Some(lines) => lines,
                None => {
                    measuring = [self.texts.text("performance-measuring")];
                    &measuring[..]
                }
            };
            let area = (size.pixels.width, size.pixels.height, size.bar_pixels());
            let corner = self.config.debug.performance_corner;
            let glyph = |shown: &cells::CellText<'_>| font.glyph(shown);
            items.extend(performance::panel(lines, &cell, area, corner, glyph));
        }
        let list = DisplayList {
            background: background.with_alpha(opacity),
            items,
            frame: drawing.font.atlas_frame(),
        };
        let updates = drawing.font.texture_updates();
        let Some(surface) = &state.surface else {
            return;
        };
        surface.resize(size.pixels.width, size.pixels.height, size.scale);
        // A frame that would show what the surface shows already presents
        // nothing, and keeps it.
        let pixels = (size.pixels.width, size.pixels.height);
        let same = state.shown.as_ref().is_some_and(|shown| {
            shown.pixels == pixels
                && shown.scale == size.scale
                && shown.fonts == drawing.fonts
                && shown.background == list.background
                && shown.items == list.items
        });
        let drawn = drawing.renderer.update_textures(&updates).and_then(|()| {
            if same {
                Ok(false)
            } else {
                drawing.renderer.draw(surface, &list)
            }
        });
        let presented = match drawn {
            Ok(true) => {
                state.drawn_at = Some(Instant::now());
                state.shown = Some(Shown {
                    pixels,
                    scale: size.scale,
                    fonts: drawing.fonts,
                    background: list.background,
                    items: list.items,
                });
                Some(drawing.renderer.drawable_wait())
            }
            Ok(false) => None,
            Err(error) => {
                eprintln!("lokyr: {error}");
                None
            }
        };
        // The frame's time ends here: the layers draw seldom, apart.
        let spent = started.map(|started| started.elapsed());
        // The cursor's cells, into their layer, which the platform places
        // over the grid's cell, and blinks.
        let (left, top) = (f64::from(x), f64::from(y));
        let (width, height) = (f64::from(cell.width), f64::from(cell.height));
        let cursor_shown = cursor_cells.map(|(column, row, cells)| {
            let at = (
                left + f64::from(column) * width,
                top + f64::from(row) * height,
            );
            (cells, at)
        });
        show_layer(
            platform,
            drawing,
            (window, Layer::Cursor),
            &mut state.cursor_layer,
            cursor_shown,
            size.scale,
            shown_cursor.and_then(|(_, blink)| blink),
        );
        // The text that blinks, into its layer, over its rows, with a cell's
        // margin on each side, which glyphs that go past their cells take.
        let text_shown = text_cells.map(|(row, mut cells)| {
            cells.x = i32::try_from(cell.width).unwrap_or(i32::MAX);
            cells.y = i32::try_from(cell.height).unwrap_or(i32::MAX);
            let at = (left - width, top + (f64::from(row) - 1.0) * height);
            (cells, at)
        });
        let was_shown = state.text_layer.place.is_some();
        show_layer(
            platform,
            drawing,
            (window, Layer::BlinkingText),
            &mut state.text_layer,
            text_shown,
            size.scale,
            text_blink,
        );
        // Hidden, it lets its drawables go, as blinking text seldom shows.
        if was_shown && state.text_layer.place.is_none() {
            state.text_layer.release();
        }
        if let (Some(wait), Some(spent), Some(shown)) = (presented, spent, &mut self.performance) {
            shown.frame(window, spent.saturating_sub(wait), wait);
        }
    }
}

impl Handler for App {
    fn started(&mut self, platform: &mut dyn Platform) {
        self.preferred = platform.locales();
        self.texts = texts(&self.config.app.language, &self.preferred);
        self.commands = commands(&self.texts, &self.plugins);
        self.activate_plugins(platform);
        // The plugins' commands registered in English, which their catalogs
        // title in the language shown.
        self.commands = commands(&self.texts, &self.plugins);
        self.set_menu(platform);
        self.show_appearance(platform);
        self.show_performance(platform);
        self.tell_font(platform);
        let (left, right) = self.config.keyboard.option_as_meta.sides();
        platform.set_option_as_meta(left, right);
        self.show_icon(platform);
        self.take_global_keys(platform);
        self.environment.locale =
            environment::utf8_locale(&self.preferred, &environment::system_locales());
        if let Some(file) = &mut self.config_file {
            if let Err(error) = crate::watch::make_directory(&file.path) {
                eprintln!(
                    "lokyr: the config's directory cannot be made, for {}: {error}",
                    file.path.display()
                );
            }
            file.watch(platform);
        }
        self.open_window(platform);
    }

    fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
        match event {
            AppEvent::Command(command) => self.run(platform, &command, Args::new()),
            AppEvent::GlobalKey(index) => {
                if let Some((_, invocation)) = self.global_keys.get(index).cloned() {
                    self.run(platform, &invocation.command, invocation.args);
                }
            }
            AppEvent::Reopen => self.open_window(platform),
            AppEvent::QuitRequested => self.ask_quit(platform, true),
            // What woke the application changed what a window shows, ended
            // its program, or changed the config file.
            AppEvent::Wake => {
                // The panes whose sessions woke it, alone, and their
                // windows' frames.
                let changed: Vec<PaneId> = self
                    .pane_wakes
                    .iter()
                    .filter(|(_, wake)| wake.take())
                    .map(|(&pane, _)| pane)
                    .collect();
                self.collect(platform, &changed);
                self.exits(platform);
                // The reading waits for the changes to settle: each change
                // puts it off.
                if let Some(file) = &mut self.config_file
                    && file.changed.swap(false, Ordering::AcqRel)
                {
                    if let Some(timer) = file.reading.take() {
                        platform.cancel_timer(timer);
                    }
                    file.reading = Some(platform.start_timer(Instant::now() + SETTLE));
                }
                for pane in changed {
                    if let Some(window) = self.window_of(pane) {
                        platform.request_frame(window);
                    }
                }
            }
            AppEvent::Timer(timer) if self.told_timer.is_some_and(|(told, _)| told == timer) => {
                self.told_timer = None;
                self.collect(platform, &[]);
            }
            AppEvent::Timer(timer)
                if self
                    .config_file
                    .as_ref()
                    .is_some_and(|file| file.reading == Some(timer)) =>
            {
                if let Some(file) = &mut self.config_file {
                    file.reading = None;
                }
                self.reload(platform);
            }
            // The programs whose grid changed within their pace learn it.
            AppEvent::Timer(timer)
                if self.resize_timer.is_some_and(|(resize, _)| resize == timer) =>
            {
                let Some((_, at)) = self.resize_timer.take() else {
                    return;
                };
                // The timer fires once its time has come, or later.
                let now = Instant::now().max(at);
                let next = self
                    .workspace
                    .panes_mut()
                    .filter_map(|pane| pane.pace(now))
                    .min();
                if let Some(next) = next {
                    self.pace_at(platform, next);
                }
            }
            // The performance's panel measures again.
            AppEvent::Timer(timer)
                if self
                    .performance
                    .as_ref()
                    .is_some_and(|shown| shown.timer == Some(timer)) =>
            {
                self.measure_performance(platform);
            }
            // The titles look once more after a burst of output.
            AppEvent::Timer(timer) if self.title_timer.is_some_and(|(title, _)| title == timer) => {
                self.look_at_places_again(platform);
            }
            // A selection dragged past a window's edge scrolls its view.
            AppEvent::Timer(timer) if self.scroll_timer == Some(timer) => {
                self.scroll_selections(platform);
            }
            // The windows' flashes end.
            AppEvent::Timer(timer) if self.flash_timer == Some(timer) => {
                self.flash_timer = None;
                for (&window, state) in &mut self.windows {
                    if state.flash_until.take().is_some() {
                        platform.request_frame(window);
                    }
                }
            }
            AppEvent::Timer(timer) if self.exit_timer == Some(timer) => {
                self.exit_timer = None;
                self.exits(platform);
            }
            // The labels of the grids' sizes that end go.
            AppEvent::Timer(timer) if self.label_timer.is_some_and(|(label, _)| label == timer) => {
                // Its time, which has come, ends the labels of that time.
                let now = self
                    .label_timer
                    .take()
                    .map_or_else(Instant::now, |(_, at)| at.max(Instant::now()));
                let mut next = None;
                for (&window, state) in &mut self.windows {
                    match state.size_label {
                        // The end of its live resize starts its time.
                        Some(_) if state.live_resize => {}
                        Some(until) if until <= now => {
                            state.size_label = None;
                            platform.request_frame(window);
                        }
                        Some(until) => {
                            next = Some(next.map_or(until, |next: Instant| next.min(until)));
                        }
                        None => {}
                    }
                }
                if let Some(next) = next {
                    self.label_at(platform, next);
                }
            }
            // A blinking cursor changes.
            AppEvent::Timer(timer) if self.blink_timer.is_some_and(|(blink, _)| blink == timer) => {
                self.blink_timer = None;
                for (&window, state) in &self.windows {
                    if state.blinking {
                        platform.request_frame(window);
                    }
                }
            }
            // A click on a program's notification brings its window forward.
            // The text editor did not open the config file, as the system
            // told once it had taken the open on.
            AppEvent::OpenFailed { target, error } => {
                let config = self
                    .config_file
                    .as_ref()
                    .is_some_and(|file| file.path.display().to_string() == target);
                if config {
                    self.tell_settings_failed(platform, error);
                } else {
                    eprintln!("lokyr: {error}");
                }
            }
            // The quick terminal, hidden, shows at its place.
            AppEvent::NotificationClicked(window) => {
                if self.quick.is_some_and(|(quick, _)| quick == window) {
                    let place = quick::place(&self.config.quick_terminal);
                    platform.show_quick_window(window, Some(&place));
                    self.quick = Some((window, true));
                } else if self.windows.contains_key(&window) {
                    platform.focus_window(window);
                }
            }
            // The user answered a program's read of the clipboard: its
            // second button allows it.
            AppEvent::AlertClosed {
                alert,
                button,
                suppressed,
                text,
            } => {
                if let Some((pane, request)) = self.clipboard_asks.remove(&alert)
                    && button == 1
                    && let Some(pane) = self.workspace.pane(pane)
                {
                    pane.reply_clipboard(request, &platform.clipboard_text().unwrap_or_default());
                }
                if let Some(window) = self.close_asks.remove(&alert)
                    && button == 0
                {
                    self.close(platform, window);
                }
                // The title given, or the window's own, for none.
                if let Some(window) = self.title_asks.remove(&alert)
                    && button == 0
                    && let Some(state) = self.windows.get_mut(&window)
                {
                    state.user_title = text
                        .as_deref()
                        .map(str::trim)
                        .filter(|title| !title.is_empty())
                        .map(str::to_owned);
                    self.show_title(platform, window);
                }
                if self.quit_ask == Some(alert) {
                    self.quit_ask = None;
                    let system = std::mem::take(&mut self.system_quit);
                    if button == 0 {
                        self.quit(platform);
                    } else if system {
                        platform.refuse_quit();
                    }
                }
                if let Some((_, asked)) = self.open_asks.remove(&alert) {
                    self.answer_open(platform, asked, button, suppressed);
                }
                // Paste, as one line, or cancel.
                if let Some((pane, text)) = self.paste_asks.remove(&alert)
                    && button < 2
                {
                    if let Some(pane) = self.workspace.pane(pane) {
                        match button {
                            0 => pane.paste(&text),
                            _ => pane.paste(&paste::one_line(&text)),
                        }
                    }
                    if suppressed {
                        self.stop_confirming_pastes();
                    }
                }
            }
            AppEvent::AppearanceChanged(appearance) => {
                self.appearance_changed(platform, appearance);
            }
            _ => {}
        }
    }

    fn window_event(&mut self, platform: &mut dyn Platform, window: WindowId, event: WindowEvent) {
        match event {
            // A step of a live resize draws at once, so that the content
            // follows the edge, but once a refresh: a second draw within
            // one would wait a refresh for a drawable, which the display's
            // next frame draws instead.
            WindowEvent::Resized(size) | WindowEvent::ScaleChanged(size) => {
                let refresh = platform.refresh_interval(window).unwrap_or_default();
                let drew_within = self.windows.get(&window).is_some_and(|state| {
                    state.live_resize
                        && state
                            .drawn_at
                            .is_some_and(|drawn| drawn.elapsed() < refresh)
                });
                if drew_within {
                    platform.request_frame(window);
                } else {
                    self.draw(platform, window, size);
                }
            }
            // A live resize presents its frames with Core Animation's
            // transaction, which moves the window's edge; its label of the
            // grid's size stays a moment after it ends.
            WindowEvent::LiveResize(resizing) => {
                if let Some(state) = self.windows.get_mut(&window) {
                    state.live_resize = resizing;
                    if let Some(surface) = &state.surface {
                        surface.set_presents_with_transaction(resizing);
                    }
                    if !resizing && state.size_label.is_some() {
                        let until = Instant::now() + size_label::HOLD;
                        state.size_label = Some(until);
                        self.label_at(platform, until);
                    }
                    platform.request_frame(window);
                }
            }
            WindowEvent::Frame => {
                if let Some(size) = platform.window_size(window) {
                    self.draw(platform, window, size);
                }
            }
            // The focus, and a key, start the cursor blinking again, on.
            // A window that loses the focus forgets the keys that the keymap
            // took, whose releases go elsewhere.
            // A window that takes the focus loses its bell's mark.
            WindowEvent::Focused(focused) => {
                if let Some(state) = self.windows.get_mut(&window) {
                    // A program that asked learns of it.
                    if let Some(pane) = self.workspace.pane(state.pane) {
                        pane.focus(focused);
                    }
                    state.focused = focused;
                    if focused {
                        self.front = Some(window);
                        state.blink.restart(Instant::now());
                    } else {
                        state.taken.clear();
                    }
                    if focused {
                        state.bell_marked = false;
                    }
                    platform.request_frame(window);
                    // Its title takes the lock, or leaves it, and its bell's
                    // mark goes with the focus.
                    self.settle_secure_input(platform);
                    self.show_title(platform, window);
                }
                if !focused {
                    self.autohide(platform, window);
                }
            }
            // A key goes to the program, and starts the cursor blinking
            // again, on; a key that types or acts, not a modifier, a lock,
            // Fn or a Command key that no menu took, closes a window whose
            // program ended.
            WindowEvent::Key(key) => {
                // The key's own prompt, which no output told, as after a
                // silent `stty -echo`, is secure from the next key.
                self.settle_secure_input(platform);
                let options = keys::options(self.config.keyboard.option_as_meta);
                let Some(state) = self.windows.get_mut(&window) else {
                    return;
                };
                let Some(pane) = self.workspace.pane(state.pane) else {
                    return;
                };
                let pressed = key.state != KeyState::Released;
                // A man page's keys scroll it, and close nothing.
                if pane.is_kept() && state.page.is_some() {
                    if pressed && let Some(scroll) = page_scroll(&key) {
                        pane.scroll(scroll);
                        platform.request_frame(window);
                    }
                    return;
                }
                if pane.is_kept() {
                    if pressed && closes(&key) {
                        self.close(platform, window);
                    }
                    return;
                }
                if pressed {
                    state.blink.restart(Instant::now());
                    platform.request_frame(window);
                }
                match self.resolve_key(window, &key) {
                    Taken::Run(invocation) => {
                        self.run(platform, &invocation.command, invocation.args);
                    }
                    Taken::Held => {}
                    Taken::Program(keys) => {
                        let Some(pane) = self
                            .windows
                            .get(&window)
                            .and_then(|state| self.workspace.pane(state.pane))
                        else {
                            return;
                        };
                        give(pane, &keys, options);
                    }
                }
            }
            // The input method's text: composed, shown at the cursor, or
            // typed, sent to the program, after the keys of a sequence under
            // way, which it ends. A composition that starts brings the view
            // back to the screen, and the cursor's blinking on, as typing
            // does.
            WindowEvent::Ime(ime) => {
                let options = keys::options(self.config.keyboard.option_as_meta);
                let Some(state) = self.windows.get_mut(&window) else {
                    return;
                };
                let Some(pane) = self.workspace.pane(state.pane) else {
                    return;
                };
                // On a window whose program ended, what the input method
                // composes does not show, and its commit closes it, as a key
                // does.
                if pane.is_kept() {
                    if matches!(ime, Ime::Commit(_)) {
                        self.close(platform, window);
                    }
                    return;
                }
                let composes = match &ime {
                    Ime::Preedit { text, .. } => !text.is_empty(),
                    Ime::Commit(_) => true,
                };
                if composes && state.resolver.is_pending() {
                    give(pane, &state.end_sequence(), options);
                }
                match ime {
                    Ime::Preedit { text, selection } => {
                        if !text.is_empty() && state.preedit.is_none() {
                            pane.scroll(Scroll::Bottom);
                            state.blink.restart(Instant::now());
                        }
                        state.preedit = (!text.is_empty()).then_some(Preedit {
                            text,
                            cursor: selection.start,
                        });
                    }
                    Ime::Commit(text) => {
                        state.preedit = None;
                        pane.text(&text);
                        state.blink.restart(Instant::now());
                    }
                }
                platform.request_frame(window);
            }
            WindowEvent::MouseButton {
                button,
                pressed,
                position,
                mods,
                clicks,
            } => self.mouse_button(platform, window, (button, pressed, clicks), position, mods),
            WindowEvent::MouseMoved { position, mods } => {
                self.mouse_moved(platform, window, position, mods);
            }
            WindowEvent::Scroll {
                delta,
                phase,
                position,
                mods,
                ..
            } => self.scroll(platform, window, (delta, phase), position, mods),
            WindowEvent::CloseRequested => self.ask_close(platform, window),
            // Files paste their paths, as a shell's arguments; text pastes
            // as a paste does.
            WindowEvent::Dropped(dropped) => {
                let text = match dropped {
                    Dropped::Files(paths) => {
                        let paths: Vec<String> = paths
                            .iter()
                            .map(|path| path.to_string_lossy().into_owned())
                            .collect();
                        paste::dropped_paths(paths.iter().map(String::as_str))
                    }
                    Dropped::Text(text) => text,
                };
                self.paste_into(platform, window, text);
            }
            WindowEvent::ContextCommand(command) => {
                self.context_command(platform, window, &command);
            }
            // A window that does not show lets its drawables go, and draws
            // again as it shows.
            WindowEvent::Occluded(occluded) => {
                if let Some(state) = self.windows.get_mut(&window) {
                    state.occluded = occluded;
                    if occluded {
                        // The next frame draws whole, as the surface lets
                        // its drawables go.
                        state.shown = None;
                        let surfaces = [
                            &state.surface,
                            &state.text_layer.surface,
                            &state.cursor_layer.surface,
                        ];
                        for surface in surfaces.into_iter().flatten() {
                            surface.release();
                        }
                    } else {
                        platform.request_frame(window);
                    }
                }
            }
        }
    }
}

/// The titlebar of a window of `size`, over its content's top, opaque in
/// `background`'s color; `None` without one.
fn bar_item(size: WindowSize, background: Color) -> Option<Item> {
    let height = size.bar_pixels();
    (height > 0).then_some(Item::Rect {
        rect: Rect {
            x: 0,
            y: 0,
            width: size.pixels.width,
            height,
        },
        color: background.with_alpha(1.0),
    })
}

/// A title's name of `directory`: its last part, `~` for `home`, and `/`
/// for the root.
fn directory_name(directory: &std::path::Path, home: Option<&std::path::Path>) -> String {
    if Some(directory) == home {
        return "~".to_owned();
    }
    directory.file_name().map_or_else(
        || "/".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// The appearance that a window whose default background is `background`
/// takes, in the app's `appearance`, for its title to read: its
/// background's, light or dark, when it is not the app's; `None` for the
/// app's. A background is light past WCAG's luminance where black and
/// white text contrast alike with it.
fn appearance_over(background: Color, appearance: Appearance) -> Option<Appearance> {
    let linear = |channel: f32| {
        let channel = f64::from(channel);
        if channel <= 0.040_45 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance = 0.2126 * linear(background.red)
        + 0.7152 * linear(background.green)
        + 0.0722 * linear(background.blue);
    let own = if luminance > 0.179 {
        Appearance::Light
    } else {
        Appearance::Dark
    };
    (own != appearance).then_some(own)
}

/// The arguments of `man` for the page that `text` names: its name, and
/// its section before it when `text` ends with one in brackets, as
/// `ls(1)`, after `--`; `None` for a text that is no page's name, which
/// only letters, digits and `.`, `_`, `+`, `:` and `-` make, not from `-`
/// or `.`, as `man` would take an option, or a path.
fn man_page(text: &str) -> Option<Vec<String>> {
    let (name, section) = match text.strip_suffix(')').and_then(|text| text.split_once('(')) {
        Some((name, section)) => (name, Some(section)),
        None => (text, None),
    };
    let named = !name.is_empty()
        && name.len() <= MAN_PAGE_NAME
        && !name.starts_with(['-', '.'])
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._+:-".contains(c));
    // A section starts with its number, or `n`, as `3pm` and `n`.
    let sectioned = section.is_none_or(|section| {
        section.len() <= MAN_SECTION
            && section.starts_with(|c: char| c.is_ascii_digit() || c == 'n')
            && section.chars().all(|c| c.is_ascii_alphanumeric())
    });
    (named && sectioned).then(|| {
        let mut args = vec!["--".to_owned()];
        args.extend(section.map(str::to_owned));
        args.push(name.to_owned());
        args
    })
}

/// The program that shows the man page of `args`: `man`, the path, or the
/// login shell's `man`, with them in its command, which they need no
/// quotes in, whatever the shell. Its pager is `ul`, which prints the whole
/// page, its bold and its underlined text as the terminal shows them, for
/// the window to keep.
fn man_program(man: Option<&std::path::Path>, args: &[String]) -> Program {
    let mut all = vec!["-P".to_owned(), "ul".to_owned()];
    all.extend(args.iter().cloned());
    match man {
        Some(man) => Program::command(man, all),
        None => Program::LoginCommand {
            command: format!("exec man {}", all.join(" ")).into(),
        },
    }
}

/// `text`, shortened to its first `most` characters and `…` when it has
/// more.
fn shortened(text: &str, most: usize) -> String {
    match text.char_indices().nth(most) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_owned(),
    }
}

/// Whether `command` is one of a cell: it takes the cell's `column` and
/// `row`, whole numbers, and needs no other argument.
fn of_cell(command: &Command) -> bool {
    let cell = |name: &str| {
        command
            .params
            .iter()
            .any(|param| param.name == name && param.kind == Kind::Integer)
    };
    cell("column")
        && cell("row")
        && command
            .params
            .iter()
            .all(|param| param.name == "column" || param.name == "row" || param.default.is_some())
}

/// The arguments of a command of a cell for the view's cell at `column` of
/// `row`.
fn cell_args(column: u16, row: i64) -> Args {
    Args::from([
        ("column".to_owned(), Value::Integer(i64::from(column))),
        ("row".to_owned(), Value::Integer(row)),
    ])
}

/// Shows `selecting`'s selection in `pane`: the cells between its spots,
/// or the words or the lines that they are in; none, as after a click. Its
/// anchor is where the terminal keeps it, which output moves with its text.
fn show_selection(pane: &Pane, selecting: &mut Selecting) {
    let offset = pane.view_offset() as i64;
    // The anchor where its text is now, which output, or a reflow, can
    // have moved; its side of its cell kept.
    if let Some((col, row)) = pane.selection_anchor() {
        let side = selecting
            .anchor
            .boundary
            .saturating_sub(selecting.anchor.col);
        selecting.anchor = Spot {
            row: row - offset,
            col,
            boundary: col + side,
        };
    }
    let (anchor, last) = (selecting.anchor, selecting.last);
    let cells = match (selecting.shape, selecting.unit) {
        (SelectionShape::Rectangle, _) => mouse::rectangle_cells(anchor, last),
        (SelectionShape::Cells, SelectionUnit::Cell) => {
            mouse::dragged_cells(anchor, last, pane.grid().columns)
        }
        (SelectionShape::Cells, _) => Some(((anchor.col, anchor.row), (last.col, last.row))),
    };
    let view = |(col, row): (u16, i64)| (col, row + offset);
    match cells {
        Some((from, to)) => pane.select(view(from), view(to), selecting.shape, selecting.unit),
        None => pane.clear_selection(),
    }
}

/// The terminal's mouse event of `action`, with `mods`, at `pointer`.
fn mouse_event(action: MouseAction, mods: Modifiers, pointer: Pointer) -> MouseEvent {
    MouseEvent {
        action,
        mods: keys::mods(mods),
        col: pointer.col,
        row: pointer.row,
        x: pointer.x,
        y: pointer.y,
    }
}

/// How `key` scrolls a man page that its window keeps, as a pager's keys
/// do: the arrows by a row, Space and the page keys by a page, Home and
/// End to the page's ends.
fn page_scroll(key: &lokyr_platform_core::KeyEvent) -> Option<Scroll> {
    use lokyr_vt_core::input::Key;
    if key.mods.super_key {
        return None;
    }
    Some(match keys::terminal_key(key)?.key {
        Key::Up => Scroll::Up(1),
        Key::Down => Scroll::Down(1),
        Key::PageUp => Scroll::PageUp,
        Key::PageDown | Key::Char(' ') => Scroll::PageDown,
        Key::Home => Scroll::Top,
        Key::End => Scroll::Bottom,
        _ => return None,
    })
}

/// Whether `key` closes a window whose program ended: a key that types or
/// acts, not a modifier, a lock, Fn, nor a key with Command, which the
/// app's shortcuts have.
fn closes(key: &lokyr_platform_core::KeyEvent) -> bool {
    use lokyr_vt_core::input::Key;
    !key.mods.super_key
        && keys::terminal_key(key).is_some_and(|key| {
            !matches!(
                key.key,
                Key::Modifier(_) | Key::CapsLock | Key::NumLock | Key::ScrollLock
            )
        })
}

/// Gives `keys` to the program of `pane`, in their order, with the
/// keyboard's `options`.
fn give(pane: &Pane, keys: &[KeyEvent], options: KeyOptions) {
    for key in keys {
        if let Some(event) = keys::terminal_key(key) {
            pane.key(&event, options);
        }
    }
}

/// Whether `key` is a modifier or a lock, which no binding takes.
fn is_modifier(key: Key) -> bool {
    matches!(
        key,
        Key::Modifier(_) | Key::CapsLock | Key::ScrollLock | Key::NumLock
    )
}

/// What the keymap does with a key.
enum Taken {
    /// It runs a command.
    Run(Invocation),
    /// It holds the key back, or takes its release.
    Held,
    /// It gives these keys to the program, in their order.
    Program(Vec<KeyEvent>),
}

/// The modifiers of `mods`, as bindings name them.
fn binding_mods(mods: Modifiers) -> keymap::Mods {
    keymap::Mods {
        cmd: mods.super_key,
        ctrl: mods.control,
        alt: mods.left_alt || mods.right_alt,
        shift: mods.shift,
    }
}

/// The context of the keys in a window of `pane`: its pane has the
/// keyboard, and holds the pane's contexts, the alternate screen and the
/// selection.
fn context(pane: Option<&Pane>) -> Context {
    let mut names = vec!["pane.focused"];
    names.extend(pane.map(Pane::contexts).unwrap_or_default());
    Context::new(names)
}

/// The clipboard of the platform, for the plugins' host.
struct PlatformClipboard<'a>(&'a mut dyn Platform);

impl plugins::Clipboard for PlatformClipboard<'_> {
    fn text(&self) -> Option<String> {
        self.0.clipboard_text()
    }

    fn set_text(&mut self, text: &str) {
        self.0.set_clipboard_text(text);
    }
}

/// `font`, at `size` in points when it is one, rather than its own.
fn sized(font: &FontConfig, size: Option<f32>) -> FontConfig {
    let size = size.and_then(|size| lokyr_text_core::Size::try_from(f64::from(size)).ok());
    FontConfig {
        size: size.unwrap_or(font.size),
        ..font.clone()
    }
}

/// Shows `shown` in `layer` of `window`, which `state` keeps: cells, which
/// `drawing` draws into the layer's surface as they change, as large as
/// they are and as their margin on each side, their batch's corner, at a
/// corner in pixels of the content, of `scale`, blinking as `blink` says.
/// `None` hides the layer.
fn show_layer(
    platform: &mut dyn Platform,
    drawing: &mut Drawing,
    (window, layer): (WindowId, Layer),
    state: &mut WindowLayer,
    shown: Option<(CellBatch, (f64, f64))>,
    scale: f64,
    blink: Option<PlatformBlink>,
) {
    let place = shown.and_then(|(cells, (x, y))| {
        let surface = state.surface.as_ref()?;
        let margin = |pixels: i32| u32::try_from(pixels).unwrap_or(0) * 2;
        let width = u32::from(cells.columns) * cells.cell_width + margin(cells.x);
        let height = u32::from(cells.rows) * cells.cell_height + margin(cells.y);
        let drawn = (cells, scale, drawing.fonts);
        if state.drawn.as_ref() != Some(&drawn) {
            let list = DisplayList {
                background: Color::default().with_alpha(0.0),
                items: vec![Item::Cells(drawn.0.clone())],
                frame: drawing.font.atlas_frame(),
            };
            surface.resize(width, height, scale);
            match drawing.renderer.draw_untimed(surface, &list) {
                Ok(true) => state.drawn = Some(drawn),
                Ok(false) => {}
                Err(error) => eprintln!("lokyr: {error}"),
            }
        }
        let points = |pixels: f64| pixels / scale;
        Some(LayerPlace {
            origin: Point {
                x: points(x),
                y: points(y),
            },
            size: Size {
                width: points(f64::from(width)),
                height: points(f64::from(height)),
            },
            blink,
        })
    });
    if state.place != place {
        platform.place_layer(window, layer, place.as_ref());
        state.place = place;
    }
}

/// The platform's blink of `pattern`, from `start`.
fn blink_of(pattern: cursor::Pattern, start: Instant) -> PlatformBlink {
    let easing = |curve: cursor::Curve| match curve {
        cursor::Curve::Linear => Easing::Linear,
        cursor::Curve::EaseIn => Easing::EaseIn,
        cursor::Curve::EaseOut => Easing::EaseOut,
        cursor::Curve::EaseInOut => Easing::EaseInOut,
    };
    PlatformBlink {
        start,
        visible: pattern.visible,
        fade_out: pattern.fade_out,
        fade_out_curve: easing(pattern.fade_out_curve),
        hidden: pattern.hidden,
        fade_in: pattern.fade_in,
        fade_in_curve: easing(pattern.fade_in_curve),
    }
}

/// The default bindings: the app's, then the active plugins'.
fn bindings(plugins: &Plugins) -> Vec<Binding> {
    let mut bindings = default_bindings();
    bindings.extend(plugins.bindings().cloned());
    bindings
}

/// The default bindings of the app's commands.
fn default_bindings() -> Vec<Binding> {
    [
        ("cmd+,", SETTINGS),
        ("cmd+n", NEW_WINDOW),
        ("cmd+w", CLOSE_WINDOW),
        ("cmd+shift+i", SET_TITLE),
        ("cmd+q", QUIT),
    ]
    .into_iter()
    .filter_map(|(keys, command)| {
        Some(Binding {
            keys: keymap::parse_keys(keys).ok()?,
            invocation: Invocation {
                command: command.to_owned(),
                args: Args::new(),
            },
            when: None,
        })
    })
    .collect()
}

/// The texts of the language `chosen`, as the config names it, or of the
/// first of the user's `preferred` languages that the catalogs have; a
/// language that they lack goes to the log, with those that they have.
fn texts(chosen: &str, preferred: &[String]) -> Texts {
    let available = l10n::languages(&CATALOGS);
    let (language, unknown) = l10n::choose(chosen, preferred, &available);
    if unknown {
        eprintln!(
            "lokyr: the language `{chosen}` is not one of Lokyr's: {}; the system's shows",
            available.join(", ")
        );
    }
    Texts::new(&language, &CATALOGS)
}

/// The app's commands, which it runs itself, with their titles in `texts`,
/// and the active plugins'.
fn commands(texts: &Texts, plugins: &Plugins) -> Registry {
    let mut registry = Registry::new();
    let commands = [
        (SETTINGS, "app-settings", Category::App),
        (SECURE_INPUT, "app-secure-keyboard-entry", Category::App),
        (PERFORMANCE, "app-performance", Category::View),
        (NEW_WINDOW, "window-new", Category::Windows),
        (CLOSE_WINDOW, "window-close", Category::Windows),
        (SET_TITLE, "window-set-title", Category::Windows),
        (
            TOGGLE_QUICK,
            "window-toggle-quick-terminal",
            Category::Windows,
        ),
        (QUIT, "app-quit", Category::App),
        (LOOK_UP, "pane-look-up", Category::Edit),
        (MAN_PAGE, "pane-man-page", Category::Windows),
    ]
    .into_iter()
    .chain(
        APPEARANCES
            .into_iter()
            .map(|(name, _, title)| (name, title, Category::View)),
    );
    for (name, title, category) in commands {
        let command = Command {
            name: name.to_owned(),
            title: texts.text(&format!("command-{title}")),
            category,
            capabilities: Vec::new(),
            params: Vec::new(),
            when: CONTEXT_ONLY.contains(&name).then(|| "selection".to_owned()),
        };
        if let Err(error) = registry.register(command) {
            eprintln!("lokyr: {error}");
        }
    }
    for command in plugins.commands_in(texts.language()) {
        if let Err(error) = registry.register(command) {
            eprintln!("lokyr: {error}");
        }
    }
    registry
}

/// The main menu, of the commands of `commands` that run without
/// arguments, by their categories, in the order of their registrations,
/// each with the keys that `keymap` binds it to: the application's menu,
/// with the system's items to hide it; the shell's, of the windows', the
/// tabs', the panes' and the input's; the edit's, of the clipboard's and the
/// editing's; the view's; the windows', with the system's items; and the
/// help's, which the system gives a search of the menus. A line parts the
/// commands of a category or a namespace from the next. Its titles are
/// `texts`'.
fn main_menu(commands: &Registry, keymap: &Keymap, texts: &Texts) -> MainMenu {
    let items = |categories: &[Category]| {
        let shown: Vec<&Command> = commands
            .in_order()
            .filter(|command| {
                categories.contains(&command.category)
                    && command.params.iter().all(|param| param.default.is_some())
                    && APPEARANCES.iter().all(|(name, ..)| *name != command.name)
                    && command.name != PERFORMANCE
                    && !CONTEXT_ONLY.contains(&command.name.as_str())
            })
            .collect();
        command_items(shown, |name| keymap.shortcut(name).and_then(shortcut))
    };
    // The appearances, in a menu of their own, by their short titles.
    let appearances = APPEARANCES
        .into_iter()
        .filter(|(name, ..)| commands.get(name).is_some())
        .map(|(name, _, title)| MenuItem::Command {
            title: texts.text(&format!("menu-{title}")),
            command: name.to_owned(),
            shortcut: keymap.shortcut(name).and_then(shortcut),
        })
        .collect();
    let mut view = items(&[Category::View]);
    if !view.is_empty() {
        view.push(MenuItem::Separator);
    }
    view.push(MenuItem::Submenu(Menu {
        title: texts.text("menu-appearance"),
        items: appearances,
        role: None,
    }));
    // The performance's panel, a tool to see how Lokyr does, last.
    if let Some(command) = commands.get(PERFORMANCE) {
        view.push(MenuItem::Separator);
        view.push(MenuItem::Command {
            title: command.title.clone(),
            command: PERFORMANCE.to_owned(),
            shortcut: keymap.shortcut(PERFORMANCE).and_then(shortcut),
        });
    }
    let standard = |key: &str, action, shortcut: Option<Shortcut>| MenuItem::Standard {
        title: texts.text(key),
        action,
        shortcut,
    };
    let cmd = |key: char, alt: bool| Shortcut {
        key: ShortcutKey::Char(key),
        command: true,
        shift: false,
        alt,
        control: false,
    };
    let menu = |key: &str, items, role| Menu {
        title: texts.text(key),
        items,
        role,
    };
    // Settings first, as macOS's apps have it, and Secure Keyboard Entry,
    // then the system's items.
    let first = |item: &MenuItem| matches!(item, MenuItem::Command { command, .. } if command == SETTINGS || command == SECURE_INPUT);
    let mut application = items(&[Category::App])
        .into_iter()
        .filter(first)
        .collect::<Vec<_>>();
    if !application.is_empty() {
        application.push(MenuItem::Separator);
    }
    application.extend([
        standard("menu-hide", StandardAction::Hide, Some(cmd('h', false))),
        standard(
            "menu-hide-others",
            StandardAction::HideOthers,
            Some(cmd('h', true)),
        ),
        standard("menu-show-all", StandardAction::ShowAll, None),
    ]);
    let app_commands: Vec<MenuItem> = items(&[Category::App])
        .into_iter()
        .filter(|item| !first(item))
        .collect();
    if !app_commands.is_empty() {
        application.push(MenuItem::Separator);
        application.extend(app_commands);
    }
    MainMenu {
        menus: vec![
            Menu {
                title: "Lokyr".to_owned(),
                items: application,
                role: None,
            },
            menu(
                "menu-shell",
                items(&[
                    Category::Windows,
                    Category::Tabs,
                    Category::Panes,
                    Category::Input,
                ]),
                None,
            ),
            menu(
                "menu-edit",
                items(&[Category::Clipboard, Category::Edit]),
                None,
            ),
            menu("menu-view", view, None),
            menu(
                "menu-window",
                vec![
                    standard(
                        "menu-minimize",
                        StandardAction::Minimize,
                        Some(cmd('m', false)),
                    ),
                    standard("menu-zoom", StandardAction::Zoom, None),
                    MenuItem::Separator,
                    standard(
                        "menu-bring-all-to-front",
                        StandardAction::BringAllToFront,
                        None,
                    ),
                ],
                Some(MenuRole::Window),
            ),
            menu("menu-help", Vec::new(), Some(MenuRole::Help)),
        ],
    }
}

/// The items of `commands`, in their order, each with the keys that
/// `shortcut` gives its name: a line parts the commands of a category or a
/// namespace from the next.
fn command_items<'a>(
    commands: impl IntoIterator<Item = &'a Command>,
    shortcut: impl Fn(&str) -> Option<Shortcut>,
) -> Vec<MenuItem> {
    let mut items = Vec::new();
    let mut group = None;
    for command in commands {
        let namespace = command.name.split('.').next().unwrap_or_default();
        if group.is_some_and(|group| group != (command.category, namespace)) {
            items.push(MenuItem::Separator);
        }
        group = Some((command.category, namespace));
        items.push(MenuItem::Command {
            title: command.title.clone(),
            command: command.name.clone(),
            shortcut: shortcut(&command.name),
        });
    }
    items
}

/// The contexts in which the commands of `commands` that tell one can run,
/// as the menus enable their items.
fn menu_contexts(commands: &Registry) -> Vec<(String, When)> {
    commands
        .in_order()
        .filter_map(|command| {
            let when = When::parse(command.when.as_deref()?).ok()?;
            Some((command.name.clone(), when))
        })
        .collect()
}

/// The keys of `combo` as a menu's item shows them: a key that types a
/// character, or a named one, with its modifiers.
fn shortcut(combo: keymap::Combo) -> Option<Shortcut> {
    use keymap::{Named, Press};
    let key = match combo.press {
        Press::Char(character) => ShortcutKey::Char(character),
        Press::Named(named) => ShortcutKey::Code(match named {
            Named::Enter => KeyCode::Enter,
            Named::Tab => KeyCode::Tab,
            Named::Backspace => KeyCode::Backspace,
            Named::Escape => KeyCode::Escape,
            Named::Space => KeyCode::Space,
            Named::Delete => KeyCode::Delete,
            Named::Insert => KeyCode::Insert,
            Named::Home => KeyCode::Home,
            Named::End => KeyCode::End,
            Named::PageUp => KeyCode::PageUp,
            Named::PageDown => KeyCode::PageDown,
            Named::Up => KeyCode::ArrowUp,
            Named::Down => KeyCode::ArrowDown,
            Named::Left => KeyCode::ArrowLeft,
            Named::Right => KeyCode::ArrowRight,
            Named::F(n) => KeyCode::F(n),
        }),
        Press::Wheel(_) | Press::Click => return None,
    };
    Some(Shortcut {
        key,
        command: combo.mods.cmd,
        shift: combo.mods.shift,
        alt: combo.mods.alt,
        control: combo.mods.ctrl,
    })
}

/// The context in which the menus' commands run, of a pane whose frame
/// shows `snapshot`: the pane has the focus, and may show the alternate
/// screen and a selection.
fn menu_context(snapshot: &vt::Snapshot) -> Context {
    let mut names = vec!["pane.focused"];
    if snapshot.alternate_screen() {
        names.push("alternate-screen");
    }
    if snapshot.selection().is_some() {
        names.push("selection");
    }
    Context::new(names)
}

#[cfg(test)]
mod tests {
    use lokyr_platform_core::fake::{Fake, FakeDefinition};
    use lokyr_ui_core::cursor::CursorConfig;

    use super::*;

    /// A program that runs until its terminal hangs up, and writes nothing.
    fn quiet() -> Program {
        Program::command("/bin/cat", [] as [&str; 0])
    }

    /// The app, of `config`, whose windows run `program`.
    fn app(config: Config, program: Program) -> App {
        App::new(None, config, None).with_program(program)
    }

    fn started() -> (Fake, App) {
        let mut fake = Fake::default();
        let mut app = app(Config::default(), quiet());
        fake.start(&mut app);
        (fake, app)
    }

    #[test]
    fn starting_opens_a_window_under_the_menus_of_the_commands() {
        let (fake, _) = started();
        let windows: Vec<_> = fake.windows().collect();
        assert_eq!(windows.len(), 1);
        // Of 100 columns of 4 points and 30 rows of 8, with 4 of padding at
        // each edge.
        assert_eq!(
            fake.window(windows[0]).unwrap().size.points,
            Size {
                width: 408.0,
                height: 248.0
            }
        );
        let menu = fake.main_menu().unwrap();
        let titles: Vec<&str> = menu.menus.iter().map(|menu| menu.title.as_str()).collect();
        assert_eq!(titles, ["Lokyr", "Shell", "Edit", "View", "Window", "Help"]);
        let roles: Vec<Option<MenuRole>> = menu.menus.iter().map(|menu| menu.role).collect();
        assert_eq!(
            roles,
            [
                None,
                None,
                None,
                None,
                Some(MenuRole::Window),
                Some(MenuRole::Help)
            ]
        );
        assert_eq!(
            menu_items(&fake, 0),
            [
                "Settings… cmd+,",
                "Secure Keyboard Entry",
                "-",
                "Hide Lokyr cmd+h",
                "Hide Others alt+cmd+h",
                "Show All",
                "-",
                "Quit Lokyr cmd+q"
            ]
        );
        assert_eq!(
            menu_items(&fake, 1),
            [
                "New Window cmd+n",
                "Close Window cmd+w",
                "Set Title… shift+cmd+i",
                "Quick Terminal"
            ]
        );
        assert_eq!(
            menu_items(&fake, 2),
            [
                "Copy cmd+c",
                "Paste cmd+v",
                "-",
                "Select All cmd+a",
                "Clear cmd+k"
            ]
        );
        // The commands that take an argument that they need have no item,
        // as `links.open` and `pane.send-text`.
        assert_eq!(
            menu_items(&fake, 3),
            [
                "Bigger cmd+=",
                "Smaller cmd+-",
                "Default Size cmd+0",
                "-",
                "Scroll Up a Line cmd+ArrowUp",
                "Scroll Down a Line cmd+ArrowDown",
                "Scroll Up a Page cmd+PageUp",
                "Scroll Down a Page cmd+PageDown",
                "Scroll to the Top cmd+Home",
                "Scroll to the Bottom cmd+End",
                "-",
                "Appearance",
                "-",
                "Show Performance"
            ]
        );
        assert_eq!(
            menu_items(&fake, 4),
            ["Minimize cmd+m", "Zoom", "-", "Bring All to Front"]
        );
        assert!(menu_items(&fake, 5).is_empty());
    }

    /// The items of the main menu's menu `index` that `fake` has: each
    /// one's title and its keys, as `cmd+n`, or a line, `-`.
    fn menu_items(fake: &Fake, index: usize) -> Vec<String> {
        let keys = |shortcut: &Shortcut| {
            let mut keys = String::new();
            for (held, name) in [
                (shortcut.control, "ctrl+"),
                (shortcut.alt, "alt+"),
                (shortcut.shift, "shift+"),
                (shortcut.command, "cmd+"),
            ] {
                if held {
                    keys.push_str(name);
                }
            }
            match shortcut.key {
                ShortcutKey::Char(key) => keys.push(key),
                ShortcutKey::Code(code) => keys.push_str(&format!("{code:?}")),
            }
            keys
        };
        fake.main_menu().unwrap().menus[index]
            .items
            .iter()
            .map(|item| match item {
                MenuItem::Command {
                    title, shortcut, ..
                }
                | MenuItem::Standard {
                    title, shortcut, ..
                } => match shortcut {
                    Some(shortcut) => format!("{title} {}", keys(shortcut)),
                    None => title.clone(),
                },
                MenuItem::Separator => "-".to_owned(),
                MenuItem::Submenu(menu) => menu.title.clone(),
            })
            .collect()
    }

    #[test]
    fn the_menus_follow_the_keys_of_the_config() {
        let keys = concat!(
            "\"cmd+n\" = false\n",
            "\"cmd+y\" = \"window.new\"\n",
            "\"cmd+j\" = { command = \"pane.clear\", when = \"selection\" }\n",
            "\"cmd+w\" = \"pane.select-all\"\n",
        );
        let (fake, _, _) = bound(keys, reader(""));
        // A binding's key, the one that a key bound later takes away, and
        // one that holds in a context alone, which no item shows.
        assert_eq!(
            menu_items(&fake, 1),
            [
                "New Window cmd+y",
                "Close Window",
                "Set Title… shift+cmd+i",
                "Quick Terminal"
            ]
        );
        assert_eq!(
            menu_items(&fake, 2),
            [
                "Copy cmd+c",
                "Paste cmd+v",
                "-",
                "Select All cmd+a",
                "Clear cmd+k"
            ]
        );
    }

    #[test]
    fn settings_opens_the_config_in_the_text_editor_made_first() {
        let (mut fake, mut app, path) = with_file("settings", "");
        std::fs::remove_file(&path).unwrap();
        // The application's menu starts with it, of its key.
        assert_eq!(
            menu_items(&fake, 0)[..3],
            ["Settings… cmd+,", "Secure Keyboard Entry", "-"]
        );
        assert_eq!(menu_items(&fake, 0).last().unwrap(), "Quit Lokyr cmd+q");
        // A file that is not there is made of the template, then opens.
        fake.send_app(&mut app, AppEvent::Command(SETTINGS.to_owned()));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), Config::template());
        assert_eq!(fake.edited(), std::slice::from_ref(&path));
        // One that is there stays as it is.
        std::fs::write(&path, "[font]\nsize = 14\n").unwrap();
        fake.send_app(&mut app, AppEvent::Command(SETTINGS.to_owned()));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "[font]\nsize = 14\n"
        );
        assert_eq!(fake.edited(), [path.clone(), path.clone()]);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        // Without a file's path, nothing opens.
        let (mut fake, mut app) = started();
        fake.send_app(&mut app, AppEvent::Command(SETTINGS.to_owned()));
        assert!(fake.edited().is_empty());
    }

    #[test]
    fn settings_tells_why_it_opens_nothing() {
        let settings = |fake: &mut Fake, app: &mut App| {
            fake.send_app(app, AppEvent::Command(SETTINGS.to_owned()));
        };
        let told = |fake: &Fake| {
            fake.alerts()
                .iter()
                .map(|shown| (shown.alert.message.clone(), shown.alert.detail.clone()))
                .collect::<Vec<_>>()
        };
        let message = "Lokyr cannot open its config file".to_owned();
        // A directory that cannot be made: a link to one that is gone.
        let (mut fake, mut app, path) = with_file("settings-failed", "");
        let window = fake.windows().next().unwrap();
        fake.send(&mut app, window, WindowEvent::Focused(true));
        let directory = path.parent().unwrap().to_owned();
        std::fs::remove_dir_all(&directory).unwrap();
        std::os::unix::fs::symlink(directory.with_extension("gone"), &directory).unwrap();
        settings(&mut fake, &mut app);
        let shown = told(&fake);
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].0, message);
        assert!(
            shown[0].1.contains(&path.display().to_string()),
            "{shown:?}"
        );
        assert!(fake.edited().is_empty());
        // On the window that has the focus.
        assert_eq!(fake.alerts()[0].window, Some(window));
        std::fs::remove_file(&directory).unwrap();
        // An editor that the platform does not find.
        std::fs::create_dir_all(&directory).unwrap();
        fake.fail_editing(Some("no application opens text"));
        settings(&mut fake, &mut app);
        let error = format!(
            "{} could not be opened: no application opens text",
            path.display()
        );
        assert_eq!(told(&fake)[1], (message.clone(), error));
        // One that the system tells of after the open, for the config file;
        // another target's goes to the log alone.
        fake.fail_editing(None);
        settings(&mut fake, &mut app);
        let late = AppEvent::OpenFailed {
            target: path.display().to_string(),
            error: "it is damaged".to_owned(),
        };
        fake.send_app(&mut app, late);
        assert_eq!(
            told(&fake)[2],
            (message.clone(), "it is damaged".to_owned())
        );
        let other = AppEvent::OpenFailed {
            target: "/elsewhere".to_owned(),
            error: "it is damaged".to_owned(),
        };
        fake.send_app(&mut app, other);
        assert_eq!(told(&fake).len(), 3);
        let _ = std::fs::remove_dir_all(&directory);
        // Without a home directory.
        let (mut fake, mut app) = started();
        settings(&mut fake, &mut app);
        let no_home =
            "Without a home directory, there is no config file: the defaults apply.".to_owned();
        assert_eq!(told(&fake), [(message, no_home)]);
    }

    #[test]
    fn the_performance_s_panel_shows_from_the_menu_and_the_config() {
        let (mut fake, mut app, path) = with_file("performance", "");
        let window = fake.windows().next().unwrap();
        // Hidden: no timer of its own, and the View menu's last item.
        assert!(app.performance.is_none());
        assert!(!fake.command_checked(PERFORMANCE));
        assert_eq!(menu_items(&fake, 3).last().unwrap(), "Show Performance");
        let timers = fake.timers().count();
        // The menu shows it, ticked, and its timer measures each second.
        fake.set_usage(lokyr_platform_core::Usage {
            idle_wakeups: 0,
            interrupt_wakeups: 0,
            footprint: 1_000_000,
        });
        fake.send_app(&mut app, AppEvent::Command(PERFORMANCE.to_owned()));
        assert!(fake.command_checked(PERFORMANCE));
        assert_eq!(fake.timers().count(), timers + 1);
        let shown = app.performance.as_ref().unwrap();
        assert_eq!(shown.lines(window), None);
        let (timer, at) = fake
            .timers()
            .find(|&(timer, _)| shown.timer == Some(timer))
            .unwrap();
        fake.set_usage(lokyr_platform_core::Usage {
            idle_wakeups: 3,
            interrupt_wakeups: 4,
            footprint: 2_000_000,
        });
        fake.send(&mut app, window, WindowEvent::Frame);
        fake.send_app(&mut app, AppEvent::Timer(timer));
        assert!(fake.window(window).unwrap().frame_requested);
        let lines = app.performance.as_ref().unwrap().lines(window).unwrap();
        assert_eq!(lines.len(), 7);
        assert_eq!(lines[5], "Memory 2.0 MB");
        let next = app.performance.as_ref().unwrap().timer.unwrap();
        assert!(
            fake.timers()
                .any(|(timer, when)| timer == next && when > at)
        );
        // A window that opens shows its lines once it had a whole second.
        fake.send_app(&mut app, AppEvent::Command("window.new".to_owned()));
        let opened = fake.windows().find(|&other| other != window).unwrap();
        fake.send_app(&mut app, AppEvent::Timer(next));
        let shown = app.performance.as_ref().unwrap();
        assert!(shown.lines(window).is_some() && shown.lines(opened).is_none());
        let next = shown.timer.unwrap();
        fake.send_app(&mut app, AppEvent::Timer(next));
        assert!(app.performance.as_ref().unwrap().lines(opened).is_some());
        let next = app.performance.as_ref().unwrap().timer.unwrap();
        // Again, it hides, and its timer goes.
        fake.send_app(&mut app, AppEvent::Command(PERFORMANCE.to_owned()));
        assert!(app.performance.is_none() && !fake.command_checked(PERFORMANCE));
        assert!(!fake.timers().any(|(timer, _)| timer == next));
        // The config's key shows it, and a change of it takes the menu's
        // choice away.
        fake.send_app(&mut app, AppEvent::Command(PERFORMANCE.to_owned()));
        let mut config = app.config.clone();
        config.debug.performance_overlay = true;
        app.apply(&mut fake, config.clone());
        assert!(app.performance.is_some());
        config.debug.performance_overlay = false;
        app.apply(&mut fake, config);
        assert!(app.performance.is_none() && !fake.command_checked(PERFORMANCE));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn the_menus_follow_a_plugin_s_binding_and_a_reloaded_config() {
        let (mut fake, mut app, _, _) = with_tester();
        assert_eq!(menu_items(&fake, 1)[0], "New Window cmd+n");
        app.run(&mut fake, "test.bind", Args::new());
        assert_eq!(menu_items(&fake, 1)[0], "New Window");
        assert!(menu_items(&fake, 3).contains(&"test.size cmd+n".to_owned()));
        // A config that takes Command-W away, as it reloads.
        let (mut fake, mut app, path) = with_file("menu-keys", "");
        assert_eq!(menu_items(&fake, 1)[1], "Close Window cmd+w");
        std::fs::write(&path, "[keys]\n\"cmd+w\" = false\n").unwrap();
        change(&mut fake, &mut app);
        assert_eq!(menu_items(&fake, 1)[1], "Close Window");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn copy_s_item_is_enabled_with_a_selection_alone() {
        let (mut fake, mut app, window) = focused_with(reader(""));
        fake.send(&mut app, window, WindowEvent::Frame);
        assert!(!fake.command_enabled("pane.copy"));
        assert!(fake.command_enabled("pane.paste"));
        fake.send(&mut app, window, command_key(KeyCode::KeyA, "a"));
        fake.send(&mut app, window, WindowEvent::Frame);
        assert!(fake.command_enabled("pane.copy"));
        // A click takes the selection away.
        let none = Modifiers::NONE;
        fake.send(&mut app, window, click(true, cell(2, 1), none, 1));
        fake.send(&mut app, window, click(false, cell(2, 1), none, 1));
        fake.send(&mut app, window, WindowEvent::Frame);
        assert!(!fake.command_enabled("pane.copy"));
    }

    #[test]
    fn the_app_s_commands_register_with_their_categories_and_titles() {
        let registry = commands(&Texts::new("en", &CATALOGS), &Plugins::new());
        let registered: Vec<(&str, &str, Category)> = registry
            .commands()
            .map(|command| {
                (
                    command.name.as_str(),
                    command.title.as_str(),
                    command.category,
                )
            })
            .collect();
        assert_eq!(
            registered,
            [
                ("app.appearance-dark", "Dark Appearance", Category::View),
                ("app.appearance-light", "Light Appearance", Category::View),
                (
                    "app.appearance-system",
                    "Automatic Appearance",
                    Category::View
                ),
                ("app.performance", "Show Performance", Category::View),
                ("app.quit", "Quit Lokyr", Category::App),
                (
                    "app.secure-keyboard-entry",
                    "Secure Keyboard Entry",
                    Category::App
                ),
                ("app.settings", "Settings…", Category::App),
                ("pane.look-up", "Look Up", Category::Edit),
                ("pane.man-page", "Open man Page", Category::Windows),
                ("window.close", "Close Window", Category::Windows),
                ("window.new", "New Window", Category::Windows),
                ("window.set-title", "Set Title…", Category::Windows),
                (
                    "window.toggle-quick-terminal",
                    "Quick Terminal",
                    Category::Windows
                ),
            ]
        );
        let texts = Texts::new("en", &CATALOGS);
        let menu = main_menu(&registry, &Keymap::default(), &texts);
        let titles: Vec<&str> = menu.menus[1]
            .items
            .iter()
            .filter_map(|item| match item {
                MenuItem::Command { title, .. } => Some(title.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            titles,
            ["New Window", "Close Window", "Set Title…", "Quick Terminal"]
        );
        // A command that takes no argument rejects one, and does nothing.
        let mut args = Args::new();
        args.insert(
            "now".to_owned(),
            lokyr_kernel::commands::Value::Boolean(true),
        );
        let error = registry.check(QUIT, args.clone()).unwrap_err();
        assert_eq!(error.code.number(), 102);
        let (mut fake, mut app, _) = started_running(idle());
        app.run(&mut fake, QUIT, args);
        assert!(!fake.has_quit());
        app.run(&mut fake, QUIT, Args::new());
        assert!(fake.has_quit());
    }

    /// The titles of the main menu that `fake` has that the app's catalogs
    /// give: the shell's first item, the application's first and last, and
    /// the edit's menu.
    fn menu_titles(fake: &Fake) -> Vec<String> {
        let menus = &fake.main_menu().unwrap().menus;
        let title = |item: &MenuItem| match item {
            MenuItem::Command { title, .. } | MenuItem::Standard { title, .. } => title.clone(),
            _ => String::new(),
        };
        // The first of the system's items of the application's menu.
        let standard = menus[0]
            .items
            .iter()
            .find(|item| matches!(item, MenuItem::Standard { .. }))
            .unwrap();
        vec![
            title(&menus[1].items[0]),
            title(standard),
            title(menus[0].items.last().unwrap()),
            menus[2].title.clone(),
        ]
    }

    #[test]
    fn the_menus_take_the_config_s_language_and_change_with_it() {
        let (mut fake, mut app, path) = with_file("language", "[app]\nlanguage = \"it\"\n");
        assert_eq!(
            menu_titles(&fake),
            [
                "Nuova finestra",
                "Nascondi Lokyr",
                "Esci da Lokyr",
                "Composizione"
            ]
        );
        assert_eq!(menu_items(&fake, 0)[0], "Impostazioni… cmd+,");
        // The plugins' commands too, from their own catalogs.
        assert_eq!(
            menu_items(&fake, 2),
            [
                "Copia cmd+c",
                "Incolla cmd+v",
                "-",
                "Seleziona tutto cmd+a",
                "Cancella cmd+k"
            ]
        );
        assert_eq!(menu_items(&fake, 3)[0], "Aumenta cmd+=");
        std::fs::write(&path, "[app]\nlanguage = \"en\"\n").unwrap();
        change(&mut fake, &mut app);
        assert_eq!(
            menu_titles(&fake),
            ["New Window", "Hide Lokyr", "Quit Lokyr", "Edit"]
        );
        assert_eq!(menu_items(&fake, 2)[0], "Copy cmd+c");
        assert_eq!(app.texts.language(), "en");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// The commands of the View menu's Appearance, and whether each one's
    /// item is ticked.
    fn appearances(fake: &Fake) -> Vec<(String, bool)> {
        let view = &fake.main_menu().unwrap().menus[3];
        let Some(menu) = view.items.iter().find_map(|item| match item {
            MenuItem::Submenu(menu) => Some(menu),
            _ => None,
        }) else {
            panic!("{view:?}");
        };
        menu.items
            .iter()
            .map(|item| match item {
                MenuItem::Command { title, command, .. } => {
                    (title.clone(), fake.command_checked(command))
                }
                other => panic!("{other:?}"),
            })
            .collect()
    }

    /// The default background of `window`'s pane, once a frame shows it.
    fn background(fake: &mut Fake, app: &mut App, window: WindowId) -> lokyr_vt_core::page::Rgb {
        fake.send(app, window, WindowEvent::Frame);
        app.pane(window).snapshot().colors().background
    }

    #[test]
    fn the_appearance_follows_the_config_and_the_menu() {
        let (mut fake, mut app, path) = with_file(
            "appearance",
            "# Mine.\n[app]\nappearance = \"dark\" # at night\n",
        );
        let window = fake.windows().next().unwrap();
        let (light, dark) = (
            app.colors(Appearance::Light).background,
            app.colors(Appearance::Dark).background,
        );
        assert_eq!(fake.own_appearance(), Some(Appearance::Dark));
        assert_eq!(
            appearances(&fake),
            [
                ("Automatic".to_owned(), false),
                ("Light".to_owned(), false),
                ("Dark".to_owned(), true)
            ]
        );
        assert_eq!(background(&mut fake, &mut app, window), dark);
        // The system's change does not reach it.
        assert!(!fake.send_app(&mut app, AppEvent::AppearanceChanged(Appearance::Light)));
        assert_eq!(background(&mut fake, &mut app, window), dark);
        // The menu's choice applies, and goes into the file.
        let choose = |fake: &mut Fake, app: &mut App, command: &str| {
            fake.send_app(app, AppEvent::Command(command.to_owned()));
        };
        choose(&mut fake, &mut app, "app.appearance-light");
        assert_eq!(fake.own_appearance(), Some(Appearance::Light));
        assert_eq!(background(&mut fake, &mut app, window), light);
        assert_eq!(
            appearances(&fake)
                .iter()
                .map(|(_, ticked)| *ticked)
                .collect::<Vec<_>>(),
            [false, true, false]
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "# Mine.\n[app]\nappearance = \"light\" # at night\n"
        );
        // The system's, which is light, then dark.
        choose(&mut fake, &mut app, "app.appearance-system");
        assert_eq!(fake.own_appearance(), None);
        assert!(fake.send_app(&mut app, AppEvent::AppearanceChanged(Appearance::Dark)));
        assert_eq!(background(&mut fake, &mut app, window), dark);
        assert!(fake.command_checked("app.appearance-system"));
        // The file that the menu wrote changes nothing as it reloads; set by
        // hand, it applies.
        change(&mut fake, &mut app);
        assert_eq!(fake.own_appearance(), None);
        std::fs::write(&path, "[app]\nappearance = \"light\"\n").unwrap();
        change(&mut fake, &mut app);
        assert_eq!(fake.own_appearance(), Some(Appearance::Light));
        assert!(fake.command_checked("app.appearance-light"));
        assert!(!fake.command_checked("app.appearance-system"));
        // A file that does not read keeps the choice until the key changes.
        std::fs::write(&path, "[app\n").unwrap();
        choose(&mut fake, &mut app, "app.appearance-dark");
        assert_eq!(fake.own_appearance(), Some(Appearance::Dark));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[app\n");
        std::fs::write(&path, "[app]\nappearance = \"light\"\n").unwrap();
        change(&mut fake, &mut app);
        assert_eq!(fake.own_appearance(), Some(Appearance::Dark));
        std::fs::write(&path, "[app]\nappearance = \"system\"\n").unwrap();
        change(&mut fake, &mut app);
        assert_eq!(fake.own_appearance(), None);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn the_appearance_s_menu_is_in_the_config_s_language() {
        let (fake, _app, path) = with_file("appearance-it", "[app]\nlanguage = \"it\"\n");
        let view = &fake.main_menu().unwrap().menus[3];
        let Some(menu) = view.items.iter().find_map(|item| match item {
            MenuItem::Submenu(menu) => Some(menu),
            _ => None,
        }) else {
            panic!("{view:?}");
        };
        assert_eq!(menu.title, "Aspetto");
        assert_eq!(
            appearances(&fake),
            [
                ("Automatico".to_owned(), true),
                ("Chiaro".to_owned(), false),
                ("Scuro".to_owned(), false)
            ]
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn the_system_s_language_shows_unless_the_config_names_one() {
        let mut fake = Fake::default();
        fake.set_locales(&["de_CH", "it_CH"]);
        let mut first = app(Config::default(), quiet());
        fake.start(&mut first);
        assert_eq!(
            menu_titles(&fake),
            [
                "Nuova finestra",
                "Nascondi Lokyr",
                "Esci da Lokyr",
                "Composizione"
            ]
        );
        // A language that Lokyr lacks takes the system's.
        let mut fake = Fake::default();
        fake.set_locales(&["it_IT"]);
        let mut config = Config::default();
        config.app.language = "fr".to_owned();
        let mut second = app(config, quiet());
        fake.start(&mut second);
        assert_eq!(second.texts.language(), "it");
        // Without a language of Lokyr's, English.
        let (fake, _) = started();
        assert_eq!(
            menu_titles(&fake),
            ["New Window", "Hide Lokyr", "Quit Lokyr", "Edit"]
        );
    }

    #[test]
    fn the_app_s_catalogs_match_their_source() {
        let pairs = [
            (
                include_str!("../resources/locales/en/app.ftl"),
                include_str!("../resources/locales/it/app.ftl"),
            ),
            (
                include_str!("../resources/locales/en/infoplist.ftl"),
                include_str!("../resources/locales/it/infoplist.ftl"),
            ),
        ];
        for (source, italian) in pairs {
            assert_eq!(l10n::check(source, italian), Ok(()));
        }
        // Every command's title is a message of the catalogs.
        let texts = Texts::new("it", &CATALOGS);
        for command in commands(&texts, &Plugins::new()).commands() {
            assert!(!command.title.starts_with("command-"), "{}", command.title);
        }
        // The built-in plugins' catalogs, each language's against English's,
        // whose titles are those that the commands register.
        let builtins: [Box<dyn plugins::Plugin>; 2] = [
            Box::new(lokyr_core_actions::CoreActions),
            Box::new(lokyr_links::Links),
        ];
        for plugin in &builtins {
            let catalogs = plugin.catalogs();
            let source = catalogs
                .iter()
                .find(|(language, _)| *language == l10n::SOURCE)
                .expect("an English catalog")
                .1;
            assert!(catalogs.len() > 1, "{catalogs:?}");
            for (_, other) in catalogs {
                assert_eq!(l10n::check(source, other), Ok(()));
            }
        }
        let (_, app) = started();
        let registered: Vec<(String, String)> = app
            .plugins
            .commands()
            .map(|command| (command.name.clone(), command.title.clone()))
            .collect();
        let english: Vec<(String, String)> = app
            .plugins
            .commands_in("en")
            .into_iter()
            .map(|command| (command.name, command.title))
            .collect();
        assert_eq!(english, registered);
        assert!(registered.iter().any(|(name, _)| name == "links.open"));
    }

    #[test]
    fn two_windows_run_two_shells() {
        let mut fake = Fake::default();
        let mut app = app(Config::default(), shell("echo pid:$$; read line"));
        fake.start(&mut app);
        fake.send_app(&mut app, AppEvent::Command(NEW_WINDOW.to_owned()));
        let windows: Vec<WindowId> = fake.windows().collect();
        assert_eq!(windows.len(), 2);
        assert_eq!(app.workspace.windows().count(), 2);
        let pid = |app: &App, window| {
            let line = row(app, window, 0);
            line.strip_prefix("pid:").map(str::to_owned)
        };
        run_until(&mut fake, &mut app, |_, app| {
            windows.iter().all(|&window| pid(app, window).is_some())
        });
        assert_ne!(pid(&app, windows[0]), pid(&app, windows[1]));
        // Closing one leaves the other, with its pane.
        fake.send(&mut app, windows[0], WindowEvent::CloseRequested);
        assert_eq!(fake.windows().collect::<Vec<_>>(), [windows[1]]);
        assert_eq!(app.workspace.windows().count(), 1);
        assert_eq!(app.workspace.panes().count(), 1);
        assert!(!fake.has_quit());
    }

    /// A shell that waits for a line, as at its prompt: no program runs.
    fn idle() -> Program {
        shell("echo ready; read line")
    }

    /// The app started, its window running `program`, once it shows
    /// `ready`, focused.
    fn started_running(program: Program) -> (Fake, App, WindowId) {
        let (mut fake, mut app, window) = started_with(program);
        fake.send(&mut app, window, WindowEvent::Focused(true));
        (fake, app, window)
    }

    #[test]
    fn quit_from_the_menu_or_from_the_system_quits() {
        let (mut fake, mut app, _) = started_running(idle());
        fake.send_app(&mut app, AppEvent::Command(QUIT.to_owned()));
        assert!(fake.has_quit());
        assert!(fake.alerts().is_empty());

        let (mut fake, mut app, _) = started_running(idle());
        fake.send_app(&mut app, AppEvent::QuitRequested);
        assert!(fake.has_quit());
        assert_eq!(fake.quit_refusals(), 0);
    }

    #[test]
    fn closing_the_last_window_quits() {
        let (mut fake, mut app, window) = started_running(idle());
        fake.send(&mut app, window, WindowEvent::CloseRequested);
        assert_eq!(fake.windows().count(), 0);
        assert!(fake.has_quit());
    }

    #[test]
    fn closing_a_window_whose_program_runs_asks_first() {
        let (mut fake, mut app, window) = started_running(shell("echo ready; exec sleep 30"));
        run_until(&mut fake, &mut app, |_, app| {
            app.pane(window).running().as_deref() == Some("sleep")
        });
        // The close button, then ⌘W: one question, cancelled.
        fake.send(&mut app, window, WindowEvent::CloseRequested);
        fake.send(&mut app, window, command_key(KeyCode::KeyW, "w"));
        assert_eq!(fake.alerts().len(), 1);
        let open = &fake.alerts()[0];
        assert_eq!(open.window, Some(window));
        assert_eq!(open.alert.message, app.texts.text("close-running-message"));
        assert_eq!(
            open.alert.detail,
            app.texts
                .text_with("close-running-detail", &[("program", Arg::Text("sleep"))])
        );
        assert_eq!(
            open.alert.buttons,
            [
                app.texts.text("close-running-close"),
                app.texts.text("close-running-cancel")
            ]
        );
        let id = open.id;
        fake.answer_alert(&mut app, id, 1);
        assert_eq!(fake.windows().count(), 1);
        // Confirmed, it closes, and the app quits with its last window.
        fake.send(&mut app, window, command_key(KeyCode::KeyW, "w"));
        let id = fake.alerts()[0].id;
        fake.answer_alert(&mut app, id, 0);
        assert_eq!(fake.windows().count(), 0);
        assert!(fake.has_quit());
    }

    #[test]
    fn closing_a_window_asks_for_a_job_behind_the_shell() {
        // A job in the background, behind a shell that reads, as at its
        // prompt.
        let (mut fake, mut app, window) =
            started_running(shell("sleep 30 & echo ready; read line"));
        run_until(&mut fake, &mut app, |_, app| {
            app.pane(window).running().as_deref() == Some("sleep")
        });
        fake.send(&mut app, window, command_key(KeyCode::KeyW, "w"));
        assert_eq!(fake.alerts().len(), 1);
        assert_eq!(
            fake.alerts()[0].alert.detail,
            app.texts
                .text_with("close-running-detail", &[("program", Arg::Text("sleep"))])
        );
    }

    #[test]
    fn quitting_while_programs_run_asks_and_the_system_waits_for_the_answer() {
        let (mut fake, mut app, busy) = started_running(shell("echo ready; exec sleep 30"));
        run_until(&mut fake, &mut app, |_, app| {
            app.pane(busy).running().as_deref() == Some("sleep")
        });
        // Another window, whose shell waits: it runs nothing.
        app.program = idle();
        app.run(&mut fake, NEW_WINDOW, Args::new());
        // ⌘Q: a question without a window, cancelled.
        fake.send(&mut app, busy, command_key(KeyCode::KeyQ, "q"));
        assert_eq!(fake.alerts().len(), 1);
        let open = &fake.alerts()[0];
        assert_eq!(open.window, None);
        assert_eq!(
            open.alert.detail,
            app.texts.text_with(
                "quit-running-detail",
                &[
                    ("windows", Arg::Number(1)),
                    ("programs", Arg::Text("sleep"))
                ]
            )
        );
        let id = open.id;
        fake.answer_alert(&mut app, id, 1);
        assert!(!fake.has_quit());
        assert_eq!(fake.quit_refusals(), 0);
        // The system's request: cancelled, it is refused; a second one
        // while the question is open waits for the same answer.
        fake.send_app(&mut app, AppEvent::QuitRequested);
        fake.send_app(&mut app, AppEvent::QuitRequested);
        assert_eq!(fake.alerts().len(), 1);
        let id = fake.alerts()[0].id;
        fake.answer_alert(&mut app, id, 1);
        assert_eq!(fake.quit_refusals(), 1);
        assert!(!fake.has_quit());
        // The system's request while ⌘Q's question is open takes its
        // answer.
        fake.send(&mut app, busy, command_key(KeyCode::KeyQ, "q"));
        fake.send_app(&mut app, AppEvent::QuitRequested);
        assert_eq!(fake.alerts().len(), 1);
        let id = fake.alerts()[0].id;
        fake.answer_alert(&mut app, id, 1);
        assert_eq!(fake.quit_refusals(), 2);
        // Confirmed, the app quits.
        fake.send_app(&mut app, AppEvent::QuitRequested);
        let id = fake.alerts()[0].id;
        fake.answer_alert(&mut app, id, 0);
        assert!(fake.has_quit());
        assert_eq!(fake.quit_refusals(), 2);
    }

    #[test]
    fn programs_of_several_windows_take_one_question() {
        let (mut fake, mut app, first) = started_running(shell("echo ready; exec sleep 30"));
        app.run(&mut fake, NEW_WINDOW, Args::new());
        app.program = shell("echo ready; exec cat");
        app.run(&mut fake, NEW_WINDOW, Args::new());
        run_until(&mut fake, &mut app, |_, app| {
            app.windows
                .values()
                .filter(|state| {
                    app.workspace
                        .pane(state.pane)
                        .and_then(Pane::running)
                        .is_some()
                })
                .count()
                == 3
        });
        fake.send(&mut app, first, command_key(KeyCode::KeyQ, "q"));
        assert_eq!(
            fake.alerts()[0].alert.detail,
            app.texts.text_with(
                "quit-running-detail",
                &[
                    ("windows", Arg::Number(3)),
                    ("programs", Arg::Text("sleep, cat"))
                ]
            )
        );
        // A program whose name is not known is "a program".
        assert_eq!(
            app.program_name(String::new()),
            app.texts.text("program-unknown")
        );
    }

    #[test]
    fn a_window_without_a_layer_draws_nothing() {
        let (mut fake, mut app) = started();
        let window = fake.windows().next().unwrap();
        assert!(app.windows[&window].surface.is_none());
        let size = WindowSize::new(
            Size {
                width: 900.0,
                height: 600.0,
            },
            1.0,
        );
        assert!(fake.send(&mut app, window, WindowEvent::Resized(size)));
        assert!(fake.send(&mut app, window, WindowEvent::ScaleChanged(size)));
        fake.send_app(&mut app, AppEvent::AppearanceChanged(Appearance::Dark));
        assert_eq!(fake.windows().count(), 1);
    }

    #[test]
    fn a_wake_asks_for_the_frames_of_the_panes_that_woke_it() {
        let (mut fake, mut app) = started();
        let window = fake.windows().next().unwrap();
        if fake.window(window).unwrap().frame_requested {
            fake.send(&mut app, window, WindowEvent::Frame);
        }
        // A wake of nothing that changed asks for nothing.
        fake.send_app(&mut app, AppEvent::Wake);
        assert!(!fake.window(window).unwrap().frame_requested);
        let pane = app.windows[&window].pane;
        app.pane_wakes[&pane].wake();
        fake.send_app(&mut app, AppEvent::Wake);
        assert!(fake.window(window).unwrap().frame_requested);
        assert!(fake.send(&mut app, window, WindowEvent::Frame));
        assert!(!fake.window(window).unwrap().frame_requested);
    }

    /// The app started, its window focused and drawn once.
    fn focused() -> (Fake, App, WindowId) {
        let (mut fake, mut app) = started();
        let window = fake.windows().next().unwrap();
        assert!(fake.send(&mut app, window, WindowEvent::Focused(true)));
        assert!(fake.window(window).unwrap().frame_requested);
        assert!(fake.send(&mut app, window, WindowEvent::Frame));
        (fake, app, window)
    }

    #[test]
    fn the_focused_window_s_cursor_blinks_on_its_timer() {
        let (mut fake, mut app, window) = focused();
        let timers: Vec<_> = fake.timers().collect();
        assert_eq!(timers.len(), 1);
        let (timer, at) = timers[0];
        assert_eq!(app.blink_timer, Some((timer, at)));
        // The timer asks the window for the frame of the change.
        assert_eq!(fake.fire_timers(&mut app, at), 1);
        assert_eq!(app.blink_timer, None);
        assert!(fake.window(window).unwrap().frame_requested);
        // Off, then on, as the clock goes.
        let now = at;
        assert_eq!(
            app.cursor(&mut fake, window, now).map(|(shape, _)| shape),
            None
        );
        assert_eq!(
            app.cursor(&mut fake, window, now + cursor::BLINK_INTERVAL)
                .map(|(shape, _)| shape),
            Some(CursorShape::Block)
        );
        assert_eq!(fake.timers().count(), 1);
    }

    #[test]
    fn blinking_stops_on_after_its_timeout() {
        let (mut fake, mut app, window) = focused();
        let later = Instant::now() + cursor::BLINK_TIMEOUT;
        assert_eq!(
            app.cursor(&mut fake, window, later).map(|(shape, _)| shape),
            Some(CursorShape::Block)
        );
        assert!(!app.windows[&window].blinking);
        // Its last timer asks for no frame.
        let fired = fake.fire_timers(&mut app, later + cursor::BLINK_TIMEOUT);
        assert_eq!(fired, 1);
        assert!(!fake.window(window).unwrap().frame_requested);
        assert_eq!(fake.timers().count(), 0);
    }

    #[test]
    fn a_window_without_the_focus_shows_a_hollow_block_that_does_not_blink() {
        let (mut fake, mut app, window) = focused();
        assert!(fake.send(&mut app, window, WindowEvent::Focused(false)));
        let now = Instant::now();
        for millis in [0, 600, 1200] {
            let at = now + std::time::Duration::from_millis(millis);
            assert_eq!(
                app.cursor(&mut fake, window, at).map(|(shape, _)| shape),
                Some(CursorShape::Hollow)
            );
        }
        assert!(!app.windows[&window].blinking);
    }

    #[test]
    fn a_cursor_on_its_layer_blinks_without_a_timer_but_its_end() {
        let mut fake = Fake::default();
        let mut config = Config::default();
        config.cursor.smooth_blink = cursor::SmoothBlink::Breathing;
        let mut app = app(config, quiet());
        app.layers = true;
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        fake.send(&mut app, window, WindowEvent::Focused(true));
        fake.send(&mut app, window, WindowEvent::Frame);
        let now = Instant::now();
        let (shape, blink) = app.cursor(&mut fake, window, now).unwrap();
        assert_eq!(shape, CursorShape::Block);
        // Breathing's, from the clock's start, which the layer runs.
        let blink = blink.expect("a blink");
        let millis = Duration::from_millis;
        assert_eq!(
            (blink.visible, blink.fade_out, blink.hidden, blink.fade_in),
            (millis(500), millis(900), millis(500), millis(900))
        );
        assert_eq!(
            (blink.fade_out_curve, blink.fade_in_curve),
            (Easing::EaseInOut, Easing::EaseInOut)
        );
        let since = app.windows[&window].blink.since();
        assert_eq!(blink.start, since);
        // One timer alone, of the blinking's end.
        let timers: Vec<Instant> = fake.timers().map(|(_, at)| at).collect();
        assert_eq!(timers, [since + cursor::BLINK_TIMEOUT]);
        // At its end, a frame, and the cursor steady, without a timer.
        assert_eq!(fake.fire_timers(&mut app, timers[0]), 1);
        assert!(fake.window(window).unwrap().frame_requested);
        let (_, blink) = app.cursor(&mut fake, window, timers[0]).unwrap();
        assert_eq!(blink, None);
        assert_eq!(fake.timers().count(), 0);
        // Off switches, on and off for the interval each.
        app.config.cursor.smooth_blink = cursor::SmoothBlink::Off;
        app.windows
            .get_mut(&window)
            .unwrap()
            .blink
            .restart(Instant::now());
        let (_, blink) = app.cursor(&mut fake, window, Instant::now()).unwrap();
        let blink = blink.expect("a blink");
        assert_eq!((blink.visible, blink.hidden), (millis(500), millis(500)));
        assert_eq!(
            (blink.fade_out, blink.fade_in),
            (Duration::ZERO, Duration::ZERO)
        );
        // Without the focus, a hollow block, steady.
        fake.send(&mut app, window, WindowEvent::Focused(false));
        let (shape, blink) = app.cursor(&mut fake, window, Instant::now()).unwrap();
        assert_eq!((shape, blink), (CursorShape::Hollow, None));
    }

    #[test]
    fn a_new_font_has_the_layers_draw_their_cells_again() {
        // A machine without a GPU, as some CI's, draws nothing.
        let Ok(mut drawing) = Drawing::new(FontConfig::default()) else {
            return;
        };
        assert_eq!(drawing.fonts, 1);
        drawing.set_font(FontConfig::default()).unwrap();
        assert_eq!(drawing.fonts, 2);
    }

    #[test]
    fn blinking_text_on_its_layer_blinks_without_a_timer_but_its_end() {
        // A cursor that does not blink, and a smooth blink, which the text
        // does not take.
        let mut config = Config::default();
        config.cursor.blink = false;
        config.cursor.smooth_blink = cursor::SmoothBlink::Breathing;
        let mut fake = Fake::default();
        let mut app = app(config, shell("printf '\\033[5mblink\\033[0m'; sleep 10"));
        app.layers = true;
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        fake.send(&mut app, window, WindowEvent::Focused(true));
        run_until(&mut fake, &mut app, |_, app| row(app, window, 0) == "blink");
        fake.send(&mut app, window, WindowEvent::Frame);
        // The grid hides it, and its layer switches it at the interval, from
        // the clock's start.
        let (shown, blink) = app.text_blink(&mut fake, window, Instant::now());
        assert!(!shown);
        let blink = blink.expect("a blink");
        let since = app.windows[&window].blink.since();
        assert_eq!(blink.start, since);
        let interval = cursor::BLINK_INTERVAL;
        assert_eq!((blink.visible, blink.hidden), (interval, interval));
        assert_eq!(
            (blink.fade_out, blink.fade_in),
            (Duration::ZERO, Duration::ZERO)
        );
        // One timer alone, of the blinking's end; at its end, a frame, and
        // the text in the grid, steady.
        let (timer, at) = app.blink_timer.expect("the blinking's timer");
        assert_eq!(at, since + cursor::BLINK_TIMEOUT);
        fake.send_app(&mut app, AppEvent::Timer(timer));
        assert!(fake.window(window).unwrap().frame_requested);
        assert_eq!(app.text_blink(&mut fake, window, at), (true, None));
        assert_eq!(app.blink_timer, None);
        // Without the focus, steady.
        app.windows
            .get_mut(&window)
            .unwrap()
            .blink
            .restart(Instant::now());
        fake.send(&mut app, window, WindowEvent::Focused(false));
        let unfocused = app.text_blink(&mut fake, window, Instant::now());
        assert_eq!(unfocused, (true, None));
    }

    #[test]
    fn the_configuration_gives_the_cursor_s_shape_and_blinking() {
        let mut fake = Fake::default();
        let mut config = Config::default();
        config.cursor = CursorConfig {
            shape: cursor::Shape::Bar,
            blink: false,
            ..CursorConfig::default()
        };
        let mut app = app(config, quiet());
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        fake.send(&mut app, window, WindowEvent::Focused(true));
        let half = Instant::now() + cursor::BLINK_INTERVAL + cursor::BLINK_INTERVAL / 2;
        assert_eq!(
            app.cursor(&mut fake, window, half).map(|(shape, _)| shape),
            Some(CursorShape::Bar)
        );
        assert_eq!(fake.timers().count(), 0);
    }

    /// An app started with its config file, in a directory of its own for
    /// `test`, which holds `text`.
    fn with_file(test: &str, text: &str) -> (Fake, App, PathBuf) {
        let directory =
            std::env::temp_dir().join(format!("lokyr-app-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("config.toml");
        std::fs::write(&path, text).unwrap();
        let config = Config::read(&path).unwrap();
        let mut fake = Fake::default();
        let mut app = App::new(None, config, Some(path.clone())).with_program(quiet());
        fake.start(&mut app);
        assert!(app.config_file.as_ref().unwrap().watcher.is_some());
        (fake, app, path)
    }

    /// Tells the app that its file changed, as its watch does, and lets
    /// the changes settle.
    fn change(fake: &mut Fake, app: &mut App) {
        let file = app.config_file.as_ref().unwrap();
        file.changed.store(true, Ordering::Release);
        fake.send_app(app, AppEvent::Wake);
        // Another change meanwhile reads the file once.
        app.config_file
            .as_ref()
            .unwrap()
            .changed
            .store(true, Ordering::Release);
        fake.send_app(app, AppEvent::Wake);
        let reading = app.config_file.as_ref().unwrap().reading;
        assert!(reading.is_some());
        // The one timer, the reading's, as the window has no focus to blink.
        let timers: Vec<_> = fake.timers().collect();
        assert_eq!(timers.len(), 1);
        assert_eq!(Some(timers[0].0), reading);
        fake.fire_timers(app, timers[0].1);
        assert_eq!(app.config_file.as_ref().unwrap().reading, None);
    }

    #[test]
    fn a_changed_config_applies_whole_once_its_changes_settle() {
        let (mut fake, mut app, path) = with_file("valid", "[cursor]\nshape = \"bar\"\n");
        assert_eq!(app.config.cursor.shape, cursor::Shape::Bar);
        let window = fake.windows().next().unwrap();
        fake.send(&mut app, window, WindowEvent::Frame);
        assert_eq!(fake.option_as_meta(), (false, false));
        std::fs::write(
            &path,
            "[cursor]\nshape = \"hollow\"\nblink = false\n[keyboard]\noption-as-meta = \"right\"\n",
        )
        .unwrap();
        change(&mut fake, &mut app);
        assert_eq!(app.config.cursor.shape, cursor::Shape::Hollow);
        assert!(!app.config.cursor.blink);
        assert!(fake.window(window).unwrap().frame_requested);
        // The platform learns which Option acts as Meta.
        assert_eq!(fake.option_as_meta(), (false, true));
        // A file that goes gives the defaults.
        std::fs::remove_file(&path).unwrap();
        change(&mut fake, &mut app);
        assert_eq!(app.config, Config::default());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_saved_config_reloads_through_the_watch() {
        let (mut fake, mut app, path) = with_file("watched", "[cursor]\nshape = \"bar\"\n");
        std::fs::write(&path, "[cursor]\nshape = \"underline\"\n").unwrap();
        // The watch's thread wakes the app, and the reading follows.
        run_until(&mut fake, &mut app, |_, app| {
            app.config.cursor.shape == cursor::Shape::Underline
        });
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// A directory of its own for `test`, with two icon's variants: the
    /// default one and `laguz`.
    fn icons(test: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("lokyr-app-icons-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        for variant in [AppConfig::DEFAULT_ICON, "laguz"] {
            std::fs::write(directory.join(format!("lokyr-{variant}.icns")), "").unwrap();
        }
        directory
    }

    /// The default config, with the icon's variant `icon`.
    fn with_icon(icon: &str) -> Config {
        let mut config = Config::default();
        config.app.icon = icon.to_owned();
        config
    }

    #[test]
    fn the_config_s_icon_shows_as_it_applies() {
        let directory = icons("bundled");
        let mut fake = Fake::default();
        let mut app =
            app(Config::default(), quiet()).with_icons(Some(Icons::new(directory.clone(), true)));
        fake.start(&mut app);
        // The bundle's own icon is the default variant's.
        assert_eq!(fake.app_icons(), []);
        let laguz = Some(directory.join("lokyr-laguz.icns"));
        app.apply(&mut fake, with_icon("laguz"));
        assert_eq!(fake.app_icons(), std::slice::from_ref(&laguz));
        // Another key leaves it.
        let mut other = with_icon("laguz");
        other.cursor.blink = false;
        app.apply(&mut fake, other);
        assert_eq!(fake.app_icons().len(), 1);
        // A name that is no variant's gives the default, the bundle's own.
        app.apply(&mut fake, with_icon("lagus"));
        assert_eq!(fake.app_icons(), [laguz.clone(), None]);
        // The default, which shows already, changes nothing.
        app.apply(&mut fake, with_icon(AppConfig::DEFAULT_ICON));
        assert_eq!(fake.app_icons().len(), 2);
        // A variant whose file cannot be read keeps the icon that shows.
        std::os::unix::fs::symlink(directory.join("gone"), directory.join("lokyr-broken.icns"))
            .unwrap();
        app.apply(&mut fake, with_icon("broken"));
        assert_eq!(fake.app_icons().len(), 2);
        app.apply(&mut fake, with_icon("laguz"));
        assert_eq!(fake.app_icons(), [laguz.clone(), None, laguz]);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn outside_the_bundle_the_icon_shows_from_its_file_at_the_start() {
        let directory = icons("tree");
        for (config, variant) in [
            (Config::default(), AppConfig::DEFAULT_ICON),
            (with_icon("laguz"), "laguz"),
        ] {
            let mut fake = Fake::default();
            let mut app =
                app(config, quiet()).with_icons(Some(Icons::new(directory.clone(), false)));
            fake.start(&mut app);
            let file = directory.join(format!("lokyr-{variant}.icns"));
            assert_eq!(fake.app_icons(), [Some(file)]);
        }
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn a_link_s_target_that_goes_and_comes_back_reloads() {
        use std::os::unix::fs::symlink;

        let home = std::env::temp_dir().join(format!("lokyr-app-checkout-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let (dotfiles, directory) = (home.join("dotfiles"), home.join("lokyr"));
        std::fs::create_dir_all(&dotfiles).unwrap();
        std::fs::create_dir_all(&directory).unwrap();
        let target = dotfiles.join("lokyr.toml");
        std::fs::write(&target, "[cursor]\nshape = \"bar\"\n").unwrap();
        let path = directory.join("config.toml");
        symlink(&target, &path).unwrap();
        let config = Config::read(&path).unwrap();
        let mut fake = Fake::default();
        let mut app = App::new(None, config, Some(path.clone())).with_program(quiet());
        fake.start(&mut app);
        assert_eq!(app.config.cursor.shape, cursor::Shape::Bar);
        // As a checkout of the dotfiles removes the file, and writes it
        // again, changed.
        std::fs::remove_file(&target).unwrap();
        run_until(&mut fake, &mut app, |_, app| {
            app.config.cursor.shape == cursor::Shape::Block
        });
        std::fs::write(&target, "[cursor]\nshape = \"underline\"\n").unwrap();
        run_until(&mut fake, &mut app, |_, app| {
            app.config.cursor.shape == cursor::Shape::Underline
        });
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_link_pointed_elsewhere_reloads_at_its_new_target() {
        use std::os::unix::fs::symlink;

        let home = std::env::temp_dir().join(format!("lokyr-app-links-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        // The config's link to a file of the dotfiles, pointed to another
        // of the same directory; then its directory's link, as GNU Stow
        // folds one, pointed to another package.
        for folded in [false, true] {
            let root = home.join(if folded { "folded" } else { "file" });
            let (one, two) = if folded {
                (root.join("one"), root.join("two"))
            } else {
                (root.join("dotfiles"), root.join("dotfiles"))
            };
            let target = |package: &std::path::Path, name: &str| {
                if folded {
                    package.to_owned()
                } else {
                    package.join(name)
                }
            };
            let file = |package: &std::path::Path, name: &str| {
                if folded {
                    package.join("config.toml")
                } else {
                    package.join(name)
                }
            };
            for package in [&one, &two] {
                std::fs::create_dir_all(package).unwrap();
            }
            std::fs::write(file(&one, "one.toml"), "[cursor]\nshape = \"bar\"\n").unwrap();
            std::fs::write(file(&two, "two.toml"), "[cursor]\nshape = \"underline\"\n").unwrap();
            let directory = root.join("lokyr");
            let path = directory.join("config.toml");
            let linked = if folded {
                directory.clone()
            } else {
                std::fs::create_dir_all(&directory).unwrap();
                path.clone()
            };
            symlink(target(&one, "one.toml"), &linked).unwrap();
            let config = Config::read(&path).unwrap();
            let mut fake = Fake::default();
            let mut app = App::new(None, config, Some(path.clone())).with_program(quiet());
            fake.start(&mut app);
            assert_eq!(app.config.cursor.shape, cursor::Shape::Bar);
            std::fs::remove_file(&linked).unwrap();
            symlink(target(&two, "two.toml"), &linked).unwrap();
            run_until(&mut fake, &mut app, |_, app| {
                app.config.cursor.shape == cursor::Shape::Underline
            });
            // A later edit of the new target reloads too.
            std::fs::write(file(&two, "two.toml"), "[cursor]\nshape = \"hollow\"\n").unwrap();
            run_until(&mut fake, &mut app, |_, app| {
                app.config.cursor.shape == cursor::Shape::Hollow
            });
        }
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_config_s_directory_is_made_at_the_start_and_watched() {
        let home = std::env::temp_dir().join(format!("lokyr-app-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        // A home without `.config`.
        let path = home.join(".config").join("lokyr").join("config.toml");
        let mut fake = Fake::default();
        let mut app = App::new(None, Config::default(), Some(path.clone())).with_program(quiet());
        fake.start(&mut app);
        assert!(path.parent().unwrap().is_dir());
        assert!(app.config_file.as_ref().unwrap().watcher.is_some());
        std::fs::write(&path, "[cursor]\nshape = \"hollow\"\n").unwrap();
        run_until(&mut fake, &mut app, |_, app| {
            app.config.cursor.shape == cursor::Shape::Hollow
        });
        // A directory that goes gives the defaults.
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
        run_until(&mut fake, &mut app, |_, app| {
            app.config == Config::default()
        });
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn each_change_puts_the_reading_off() {
        let (mut fake, mut app, path) = with_file("settle", "");
        let changed = |fake: &mut Fake, app: &mut App| {
            let file = app.config_file.as_ref().unwrap();
            file.changed.store(true, Ordering::Release);
            fake.send_app(app, AppEvent::Wake);
            app.config_file.as_ref().unwrap().reading.unwrap()
        };
        let first = changed(&mut fake, &mut app);
        let second = changed(&mut fake, &mut app);
        // The first timer is gone, and the one left counts from the last.
        assert_ne!(first, second);
        let timers: Vec<_> = fake.timers().map(|(timer, _)| timer).collect();
        assert_eq!(timers, [second]);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn an_invalid_config_changes_nothing() {
        let (mut fake, mut app, path) = with_file("invalid", "[cursor]\nshape = \"bar\"\n");
        std::fs::write(&path, "[cursor]\nshape = \"beam\"\n").unwrap();
        change(&mut fake, &mut app);
        assert_eq!(app.config.cursor.shape, cursor::Shape::Bar);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// Runs the app, as the main thread's loop would, handing it the wakes
    /// of its programs, the timers that come due and the frames that it asks
    /// for, until `done` holds; fails after a while.
    fn run_until(fake: &mut Fake, app: &mut App, mut done: impl FnMut(&Fake, &App) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done(fake, app) {
            assert!(Instant::now() < deadline, "not done in time");
            fake.deliver_wake(app);
            fake.fire_timers(app, Instant::now());
            let asked: Vec<WindowId> = fake
                .windows()
                .filter(|&window| fake.window(window).unwrap().frame_requested)
                .collect();
            for window in asked {
                fake.send(app, window, WindowEvent::Frame);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// The text of row `row` of a window's screen.
    fn row(app: &App, window: WindowId, row: u16) -> String {
        let snapshot = app.pane(window).snapshot();
        let line = snapshot.line(row);
        line.cells()
            .iter()
            // A wide character's second cell holds nothing of it.
            .filter(|cell| cell.width() != lokyr_vt_core::page::Width::SpacerTail)
            .map(|cell| match cell.content() {
                lokyr_vt_core::page::Content::Char(c)
                | lokyr_vt_core::page::Content::Grapheme(c) => c,
                _ => ' ',
            })
            .map(|c| if c == '\0' { ' ' } else { c })
            .collect::<String>()
            .trim_end()
            .to_owned()
    }

    fn shell(script: &str) -> Program {
        Program::command("/bin/sh", ["-c", script])
    }

    /// A directory of the test's own, made empty, as the system names it,
    /// which `pwd` tells, its links resolved.
    fn real_directory(test: &str) -> PathBuf {
        std::fs::canonicalize(directory(test)).unwrap()
    }

    /// The directory that `window`'s shell told as it started, with `pwd`.
    fn started_in(fake: &mut Fake, app: &mut App, window: WindowId) -> String {
        run_until(fake, app, |_, app| !row(app, window, 0).is_empty());
        row(app, window, 0)
    }

    #[test]
    fn the_first_window_starts_where_start_directory_says() {
        let base = real_directory("start-directory");
        let (last, path, state) = (base.join("last"), base.join("path"), base.join("state"));
        for made in [&last, &path] {
            std::fs::create_dir(made).unwrap();
        }
        let home = std::env::home_dir().unwrap().display().to_string();
        let shown = |at: &std::path::Path| at.display().to_string();
        let gone = base.join("gone").display().to_string();
        for (directory, kept, expected) in [
            (StartDirectory::Home, true, home.clone()),
            (StartDirectory::Last, true, shown(&last)),
            (StartDirectory::Last, false, home.clone()),
            (StartDirectory::Path(shown(&path)), true, shown(&path)),
            (StartDirectory::Path(gone), true, home.clone()),
        ] {
            let _ = std::fs::remove_dir_all(&state);
            if kept {
                state::keep_last_directory(&state, &last).unwrap();
            }
            let mut config = Config::default();
            config.window.start_directory = directory.clone();
            let mut fake = Fake::default();
            let mut app = app(config, shell("pwd; exec sleep 10")).with_state(Some(state.clone()));
            fake.start(&mut app);
            let window = fake.windows().next().unwrap();
            assert_eq!(
                started_in(&mut fake, &mut app, window),
                expected,
                "{directory:?}"
            );
        }
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn a_new_window_starts_where_new_directory_says_and_the_last_is_kept() {
        let base = real_directory("new-directory");
        let (inherited, path, state) =
            (base.join("inherit"), base.join("path"), base.join("state"));
        for made in [&inherited, &path] {
            std::fs::create_dir(made).unwrap();
        }
        let shown = |at: &std::path::Path| at.display().to_string();
        let home = std::env::home_dir().unwrap().display().to_string();
        // Each window's shell tells where it started, then goes on in its
        // own directory, or the first one's, of its program, `sleep`.
        let script = format!(
            "pwd; [ \"$PWD\" = {home:?} ] && cd {:?}; exec sleep 10",
            shown(&inherited)
        );
        let mut fake = Fake::default();
        let mut app = app(Config::default(), shell(&script)).with_state(Some(state.clone()));
        fake.start(&mut app);
        let first = fake.windows().next().unwrap();
        assert_eq!(started_in(&mut fake, &mut app, first), home);
        run_until(&mut fake, &mut app, |_, app| {
            app.pane(first).working_directory().as_deref() == Some(inherited.as_path())
        });
        // The window that had the focus last, as the app is not active.
        fake.send(&mut app, first, WindowEvent::Focused(true));
        fake.send(&mut app, first, WindowEvent::Focused(false));
        let opened = |fake: &mut Fake, app: &mut App| {
            let before: Vec<WindowId> = fake.windows().collect();
            fake.send_app(app, AppEvent::Command(NEW_WINDOW.to_owned()));
            let window = fake
                .windows()
                .find(|window| !before.contains(window))
                .unwrap();
            (window, started_in(fake, app, window))
        };
        // Of the program in the window that had the focus, by default.
        let (second, directory) = opened(&mut fake, &mut app);
        assert_eq!(directory, shown(&inherited));
        app.config.window.new_directory = NewDirectory::Home;
        assert_eq!(opened(&mut fake, &mut app).1, home);
        app.config.window.new_directory = NewDirectory::Path(shown(&path));
        let (third, directory) = opened(&mut fake, &mut app);
        assert_eq!(directory, shown(&path));
        // A window that closes keeps its directory as the last one's; the
        // one in front, as the app quits, though no window has the focus,
        // as the app is not active.
        app.close(&mut fake, second);
        assert_eq!(state::last_directory(&state), Some(inherited.clone()));
        fake.send(&mut app, third, WindowEvent::Focused(true));
        fake.send(&mut app, third, WindowEvent::Focused(false));
        assert_eq!(fake.focused_window(), None);
        app.quit(&mut fake);
        assert!(fake.has_quit());
        assert_eq!(state::last_directory(&state), Some(path));
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn a_window_whose_program_ended_keeps_the_directory_that_its_title_saw() {
        let base = real_directory("ended-directory");
        let state = base.join("state");
        let script = format!("cd {:?}; echo here; sleep 0.5; exit 1", base.display());
        let mut fake = Fake::default();
        let mut app = app(Config::default(), shell(&script)).with_state(Some(state.clone()));
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        run_until(&mut fake, &mut app, |_, app| {
            app.windows[&window].directory.as_deref() == Some(base.as_path())
        });
        run_until(&mut fake, &mut app, |_, app| app.pane(window).is_kept());
        assert_eq!(app.pane(window).working_directory(), None);
        app.close(&mut fake, window);
        assert_eq!(state::last_directory(&state), Some(base.clone()));
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn secure_keyboard_entry_follows_a_password_s_prompt_and_the_menu() {
        let script = "stty -echo; echo ready; read a; stty echo; stty -echo; printf 'Password:'; \
            read secret; stty echo; echo done; read b; stty -echo; read c; stty echo; read d";
        let (mut fake, mut app, window) = started_with(shell(script));
        fake.send(&mut app, window, WindowEvent::Focused(true));
        run_until(&mut fake, &mut app, |_, app| row(app, window, 0) == "ready");
        let on = |fake: &Fake| fake.secure_input().0;
        let title = |fake: &Fake| fake.window(window).unwrap().title.clone();
        // Before the prompt, `read` with its echo off, in canonical mode, is
        // a password's too.
        assert!(on(&fake));
        // At the prompt, which its output tells, on, of a lock in the title.
        fake.send(&mut app, window, typed(KeyCode::Enter, "\r", false));
        run_until(&mut fake, &mut app, |_, app| {
            row(app, window, 1).starts_with("Password:")
        });
        assert!(on(&fake));
        assert!(title(&fake).starts_with(SECURE_MARK), "{}", title(&fake));
        // Without the focus, off, and the lock goes; back, on again.
        fake.send(&mut app, window, WindowEvent::Focused(false));
        assert!(!on(&fake) && !title(&fake).starts_with(SECURE_MARK));
        fake.send(&mut app, window, WindowEvent::Focused(true));
        assert!(on(&fake) && title(&fake).starts_with(SECURE_MARK));
        // After it, as its output tells, off.
        fake.send(&mut app, window, typed(KeyCode::Enter, "\r", false));
        run_until(&mut fake, &mut app, |fake, app| {
            (0..4).any(|r| row(app, window, r).ends_with("done")) && !on(fake)
        });
        assert!(!title(&fake).starts_with(SECURE_MARK));
        // A prompt that no output tells, as after a silent `stty -echo`, is
        // seen before the next key.
        fake.send(&mut app, window, typed(KeyCode::Enter, "\r", false));
        run_until(&mut fake, &mut app, |_, app| app.pane(window).hides_input());
        assert!(!on(&fake));
        fake.send(&mut app, window, typed(KeyCode::KeyX, "x", false));
        assert!(on(&fake));
        fake.send(&mut app, window, WindowEvent::Focused(false));
        fake.send(&mut app, window, WindowEvent::Focused(true));
        fake.send(&mut app, window, WindowEvent::Focused(false));
        // The menu's item turns it on for good, ticked, and off again.
        fake.send(&mut app, window, WindowEvent::Focused(true));
        fake.send(&mut app, window, typed(KeyCode::Enter, "\r", false));
        run_until(&mut fake, &mut app, |_, app| {
            !app.pane(window).hides_input()
        });
        fake.send(&mut app, window, typed(KeyCode::KeyX, "x", false));
        assert!(!on(&fake));
        assert!(!fake.command_checked(SECURE_INPUT));
        fake.send_app(&mut app, AppEvent::Command(SECURE_INPUT.to_owned()));
        assert!(on(&fake) && fake.command_checked(SECURE_INPUT));
        // Never while another app is active, as no window has the focus.
        fake.send(&mut app, window, WindowEvent::Focused(false));
        assert!(!on(&fake));
        fake.send(&mut app, window, WindowEvent::Focused(true));
        assert!(on(&fake));
        fake.send_app(&mut app, AppEvent::Command(SECURE_INPUT.to_owned()));
        assert!(!on(&fake) && !fake.command_checked(SECURE_INPUT));
        // Each change once.
        assert_eq!(fake.secure_input().1, 14);
    }

    #[test]
    fn secure_keyboard_entry_goes_with_the_focused_window_that_closes() {
        let config = Config::parse(
            "[keys]\n\"global:ctrl+`\" = \"window.toggle-quick-terminal\"\n",
            std::path::Path::new("config.toml"),
        )
        .unwrap();
        let mut fake = Fake::default();
        let mut app = app(config, quiet());
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        fake.send(&mut app, window, WindowEvent::Focused(true));
        fake.send_app(&mut app, AppEvent::Command(SECURE_INPUT.to_owned()));
        assert!(fake.secure_input().0);
        // The last window, which a global key keeps the app open without:
        // no window takes the focus.
        app.close(&mut fake, window);
        assert!(!fake.has_quit());
        assert_eq!(fake.secure_input(), (false, 2));
    }

    #[test]
    fn a_program_that_asks_learns_of_the_focus() {
        let (mut fake, mut app, window) = started_with(shell(
            "stty raw -echo; printf '\\033[?1004h'; echo ready; cat -v",
        ));
        fake.send(&mut app, window, WindowEvent::Focused(true));
        fake.send(&mut app, window, WindowEvent::Focused(false));
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "^[[I^[[O"));
    }

    #[test]
    fn a_window_that_does_not_show_draws_nothing_until_it_shows() {
        let (mut fake, mut app, window) = started_with(shell(
            "stty raw -echo; echo ready; head -c 1 >/dev/null; echo more; cat",
        ));
        fake.send(&mut app, window, WindowEvent::Frame);
        // Hidden and shown again without a frame asked meanwhile, it draws
        // anew, as it let its drawables go.
        fake.send(&mut app, window, WindowEvent::Occluded(true));
        fake.send(&mut app, window, WindowEvent::Occluded(false));
        assert!(fake.send(&mut app, window, WindowEvent::Frame));
        assert!(!fake.window(window).unwrap().frame_requested);
        fake.send(&mut app, window, WindowEvent::Occluded(true));
        fake.send(&mut app, window, WindowEvent::Occluded(false));
        assert!(fake.window(window).unwrap().frame_requested);
        fake.send(&mut app, window, WindowEvent::Frame);
        fake.send(&mut app, window, WindowEvent::Occluded(true));
        assert!(app.windows[&window].occluded);
        // Output asks for a frame, which waits.
        fake.send(&mut app, window, key());
        let pause = Instant::now() + Duration::from_millis(300);
        run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
        let shown = fake.window(window).unwrap();
        assert!(!shown.frame_requested && shown.frame_held);
        // Shown again, it draws.
        fake.send(&mut app, window, WindowEvent::Occluded(false));
        assert!(!app.windows[&window].occluded);
        assert!(fake.window(window).unwrap().frame_requested);
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "more"));
    }

    #[test]
    fn a_window_opens_of_its_cells_and_the_first_where_the_last_was() {
        let mut config = unpadded();
        config.window.columns = 80.try_into().unwrap();
        config.window.rows = 24.try_into().unwrap();
        let mut fake = Fake::default();
        let mut app = app(config, quiet());
        fake.start(&mut app);
        let first = fake.windows().next().unwrap();
        let shown = fake.window(first).unwrap();
        assert_eq!(
            shown.size.points,
            Size {
                width: 320.0,
                height: 192.0
            }
        );
        assert!(shown.remember && shown.restore && !shown.restore_size);
        // The next keeps its frame, and opens of the kept size, or of the
        // cells without one.
        fake.send_app(&mut app, AppEvent::Command(NEW_WINDOW.to_owned()));
        let second = fake.windows().find(|&window| window != first).unwrap();
        let shown = fake.window(second).unwrap();
        assert!(shown.remember && !shown.restore && shown.restore_size);
        // Without the key, none keeps it.
        let mut config = unpadded();
        config.window.remember_frame = false;
        let mut fake = Fake::default();
        let mut forgetful = App::new(None, config, None).with_program(quiet());
        fake.start(&mut forgetful);
        let first = fake.windows().next().unwrap();
        let shown = fake.window(first).unwrap();
        assert!(!shown.remember && !shown.restore);
        fake.send_app(&mut forgetful, AppEvent::Command(NEW_WINDOW.to_owned()));
        let second = fake.windows().find(|&window| window != first).unwrap();
        assert!(!fake.window(second).unwrap().restore_size);
        // The key, turned on, reaches the window open.
        let mut config = forgetful.config.clone();
        config.window.remember_frame = true;
        forgetful.apply(&mut fake, config);
        assert!(fake.window(first).unwrap().remember);
    }

    #[test]
    fn a_window_shows_its_program_s_output() {
        let mut fake = Fake::default();
        let mut app = app(Config::default(), shell("printf 'hello'; sleep 10"));
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        run_until(&mut fake, &mut app, |_, app| row(app, window, 0) == "hello");
        // Of the config's columns and rows, within the padding.
        let snapshot = app.pane(window).snapshot();
        assert_eq!((snapshot.cols(), snapshot.rows()), (100, 30));
    }

    #[test]
    fn a_program_finds_the_bundle_s_resources_and_the_user_s_locale() {
        let resources =
            std::env::temp_dir().join(format!("lokyr-app-resources-{}", std::process::id()));
        std::fs::create_dir_all(&resources).unwrap();
        let mut fake = Fake::default();
        // A language that the system has no locale of, then one that it has.
        fake.set_locales(&["xx_XX", "it_CH"]);
        let script = "printf '%s' \"$LOKYR_RESOURCES_DIR\"; sleep 10";
        let mut app = app(Config::default(), shell(script)).with_resources(Some(resources.clone()));
        fake.start(&mut app);
        assert_eq!(app.environment.locale, "it_CH.UTF-8");
        let window = fake.windows().next().unwrap();
        let shown = resources.display().to_string();
        run_until(&mut fake, &mut app, |_, app| row(app, window, 0) == shown);
        let _ = std::fs::remove_dir_all(resources);
    }

    /// The point inside the cell at `col` and `row` of a window without
    /// padding, whose columns are 4 points wide and rows 8 high.
    fn cell(col: u16, row: u16) -> Point {
        Point {
            x: 4.0 * f64::from(col) + 1.0,
            y: 8.0 * f64::from(row) + 1.0,
        }
    }

    fn press(button: MouseButton, pressed: bool, at: Point, mods: Modifiers) -> WindowEvent {
        WindowEvent::MouseButton {
            button,
            pressed,
            position: at,
            mods,
            clicks: 1,
        }
    }

    /// The left button's press, or release, at `at`, with `mods`, of
    /// `clicks` in a row.
    fn click(pressed: bool, at: Point, mods: Modifiers, clicks: u32) -> WindowEvent {
        WindowEvent::MouseButton {
            button: MouseButton::Left,
            pressed,
            position: at,
            mods,
            clicks,
        }
    }

    /// The pointer's move to `at`, with `mods`.
    fn moved(at: Point, mods: Modifiers) -> WindowEvent {
        WindowEvent::MouseMoved { position: at, mods }
    }

    /// A drag of the left button from `from` to `to`, with `mods`.
    fn select_drag(
        fake: &mut Fake,
        app: &mut App,
        window: WindowId,
        (from, to): (Point, Point),
        mods: Modifiers,
    ) {
        fake.send(app, window, click(true, from, mods, 1));
        fake.send(app, window, moved(to, mods));
        fake.send(app, window, click(false, to, mods, 1));
    }

    /// The text that `window`'s pane selects, as the terminal has it.
    fn selected_text(app: &App, window: WindowId) -> Option<String> {
        app.pane(window).selection_text()
    }

    /// The app without padding, its window running a program that shows two
    /// lines, `hello world` and `second line`, then `ready`, of `config`'s
    /// `[clipboard]`.
    fn selecting(clipboard: &str, modes: &str) -> (Fake, App, WindowId) {
        let mut config = Config::parse(
            &format!("[clipboard]\n{clipboard}"),
            std::path::Path::new("config.toml"),
        )
        .unwrap();
        config.window = unpadded().window;
        let program = shell(&format!(
            "stty raw -echo; printf 'hello world\\r\\nsecond line\\r\\n{modes}'; echo ready; cat -v"
        ));
        let mut fake = Fake::default();
        let mut app = app(config, program);
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
        (fake, app, window)
    }

    #[test]
    fn the_pointer_is_the_text_s_but_over_a_program_that_takes_the_mouse() {
        // A program that takes the mouse's clicks at a key, and gives them
        // back at the next.
        let (mut fake, mut app, window) = started_with(shell(concat!(
            "stty raw -echo; echo ready; head -c1 >/dev/null; ",
            "printf '\\033[?1000h'; head -c1 >/dev/null; printf '\\033[?1000l'; cat"
        )));
        let pointer = |fake: &Fake| fake.window(window).unwrap().pointer;
        run_until(&mut fake, &mut app, |fake, _| {
            pointer(fake) == PointerShape::Text
        });
        let a = key_event(KeyCode::KeyA, "a", Modifiers::NONE, true);
        fake.send(&mut app, window, a.clone());
        run_until(&mut fake, &mut app, |fake, _| {
            pointer(fake) == PointerShape::Arrow
        });
        fake.send(&mut app, window, a);
        run_until(&mut fake, &mut app, |fake, _| {
            pointer(fake) == PointerShape::Text
        });
    }

    #[test]
    fn a_drag_selects_the_cells_between_its_ends_and_a_click_selects_none() {
        let (mut fake, mut app, window) = selecting("", "");
        let none = Modifiers::NONE;
        select_drag(&mut fake, &mut app, window, (cell(0, 0), cell(5, 0)), none);
        assert_eq!(selected_text(&app, window).as_deref(), Some("hello"));
        // Backwards, across rows.
        select_drag(&mut fake, &mut app, window, (cell(6, 1), cell(6, 0)), none);
        assert_eq!(
            selected_text(&app, window).as_deref(),
            Some("world\nsecond")
        );
        // The view shows it at its next frame.
        run_until(&mut fake, &mut app, |_, app| {
            app.pane(window).snapshot().selection().is_some()
        });
        assert!(fake.window(window).is_some());
        // A click takes it away.
        fake.send(&mut app, window, click(true, cell(2, 1), none, 1));
        fake.send(&mut app, window, click(false, cell(2, 1), none, 1));
        assert_eq!(selected_text(&app, window), None);
    }

    #[test]
    fn clicks_select_a_word_and_a_line_and_option_a_rectangle() {
        let (mut fake, mut app, window) = selecting("", "");
        let none = Modifiers::NONE;
        fake.send(&mut app, window, click(true, cell(8, 0), none, 2));
        assert_eq!(selected_text(&app, window).as_deref(), Some("world"));
        // Dragged, a word's selection goes on by words.
        fake.send(&mut app, window, moved(cell(1, 1), none));
        assert_eq!(
            selected_text(&app, window).as_deref(),
            Some("world\nsecond")
        );
        fake.send(&mut app, window, click(false, cell(1, 1), none, 2));
        fake.send(&mut app, window, click(true, cell(3, 1), none, 3));
        fake.send(&mut app, window, click(false, cell(3, 1), none, 3));
        assert_eq!(selected_text(&app, window).as_deref(), Some("second line"));
        let option = Modifiers {
            left_alt: true,
            ..Modifiers::NONE
        };
        select_drag(
            &mut fake,
            &mut app,
            window,
            (cell(1, 0), cell(4, 1)),
            option,
        );
        assert_eq!(selected_text(&app, window).as_deref(), Some("ell\neco"));
    }

    #[test]
    fn shift_s_click_extends_the_selection() {
        let (mut fake, mut app, window) = selecting("", "");
        select_drag(
            &mut fake,
            &mut app,
            window,
            (cell(0, 0), cell(5, 0)),
            Modifiers::NONE,
        );
        fake.send(&mut app, window, click(true, cell(3, 1), SHIFT, 1));
        fake.send(&mut app, window, click(false, cell(3, 1), SHIFT, 1));
        assert_eq!(
            selected_text(&app, window).as_deref(),
            Some("hello world\nsec")
        );
    }

    #[test]
    fn a_selection_copies_its_text_as_it_ends_when_the_config_says() {
        let (mut fake, mut app, window) = selecting("", "");
        select_drag(
            &mut fake,
            &mut app,
            window,
            (cell(0, 0), cell(5, 0)),
            Modifiers::NONE,
        );
        assert_eq!(fake.clipboard_text(), None);
        let (mut fake, mut app, window) = selecting("copy-on-select = true\n", "");
        fake.send(
            &mut app,
            window,
            click(true, cell(0, 0), Modifiers::NONE, 1),
        );
        fake.send(&mut app, window, moved(cell(5, 0), Modifiers::NONE));
        // Not before the button goes up.
        assert_eq!(fake.clipboard_text(), None);
        fake.send(
            &mut app,
            window,
            click(false, cell(5, 0), Modifiers::NONE, 1),
        );
        assert_eq!(fake.clipboard_text().as_deref(), Some("hello"));
        // A click, which selects nothing, copies nothing.
        fake.send(
            &mut app,
            window,
            click(true, cell(2, 1), Modifiers::NONE, 1),
        );
        fake.send(
            &mut app,
            window,
            click(false, cell(2, 1), Modifiers::NONE, 1),
        );
        assert_eq!(fake.clipboard_text().as_deref(), Some("hello"));
        // Nor do empty cells, whose text is empty.
        select_drag(
            &mut fake,
            &mut app,
            window,
            (cell(0, 6), cell(5, 6)),
            Modifiers::NONE,
        );
        assert_eq!(selected_text(&app, window).as_deref(), Some(""));
        assert_eq!(fake.clipboard_text().as_deref(), Some("hello"));
    }

    #[test]
    fn a_program_that_takes_the_mouse_has_the_clicks_but_shift_s() {
        let (mut fake, mut app, window) = selecting("", "\\033[?1000h\\033[?1006h");
        select_drag(
            &mut fake,
            &mut app,
            window,
            (cell(0, 0), cell(5, 0)),
            Modifiers::NONE,
        );
        assert_eq!(selected_text(&app, window), None);
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "[<0;1;1M"));
        select_drag(&mut fake, &mut app, window, (cell(0, 0), cell(5, 0)), SHIFT);
        assert_eq!(selected_text(&app, window).as_deref(), Some("hello"));
    }

    /// The last context menu that the app opened, over `window`: where, its
    /// items' titles, `-` for a line, and its dimmed commands.
    fn context_menu(fake: &Fake, window: WindowId) -> (Point, Vec<String>, Vec<String>) {
        let (over, position, menu) = fake.context_menus().last().expect("a context menu");
        assert_eq!(*over, window);
        let items = menu
            .items
            .iter()
            .map(|item| match item {
                MenuItem::Command {
                    title, shortcut, ..
                } => {
                    assert_eq!(*shortcut, None, "{title}");
                    title.clone()
                }
                MenuItem::Separator => "-".to_owned(),
                other => panic!("{other:?}"),
            })
            .collect();
        (*position, items, menu.dimmed.clone())
    }

    /// A press and a release of the right button at `at`, with `mods`.
    fn right_click(fake: &mut Fake, app: &mut App, window: WindowId, at: Point, mods: Modifiers) {
        fake.send(app, window, press(MouseButton::Right, true, at, mods));
        fake.send(app, window, press(MouseButton::Right, false, at, mods));
    }

    #[test]
    fn a_right_click_selects_and_opens_the_context_menu_at_the_pointer() {
        let (mut fake, mut app, window) = selecting("", "");
        let none = Modifiers::NONE;
        // On a word, which it selects; its program has no link there.
        right_click(&mut fake, &mut app, window, cell(8, 0), none);
        assert_eq!(selected_text(&app, window).as_deref(), Some("world"));
        let (position, items, dimmed) = context_menu(&fake, window);
        assert_eq!(position, cell(8, 0));
        assert_eq!(
            items,
            [
                "Open Link",
                "Open man Page for “world”",
                "-",
                "Look Up “world”",
                "-",
                "Copy",
                "Paste",
                "-",
                "Select All",
                "Clear"
            ]
        );
        assert_eq!(dimmed, ["links.open"]);
        // Within the selection, which stays.
        select_drag(&mut fake, &mut app, window, (cell(0, 0), cell(3, 0)), none);
        right_click(&mut fake, &mut app, window, cell(1, 0), none);
        assert_eq!(selected_text(&app, window).as_deref(), Some("hel"));
        assert_eq!(context_menu(&fake, window).1[3], "Look Up “hel”");
        assert_eq!(context_menu(&fake, window).2, ["links.open"]);
        // On a blank cell, which takes it away, and dims Copy and Look Up.
        right_click(&mut fake, &mut app, window, cell(5, 0), none);
        assert_eq!(selected_text(&app, window), None);
        assert!(app.windows[&window].selecting.is_none());
        assert_eq!(
            context_menu(&fake, window).1[1..4],
            ["Open man Page", "-", "Look Up"]
        );
        assert_eq!(
            context_menu(&fake, window).2,
            ["links.open", "pane.man-page", "pane.look-up", "pane.copy"]
        );
        // A Control click opens it too, and starts no selection that the
        // pointer would drag.
        let control = Modifiers {
            control: true,
            ..none
        };
        fake.send(&mut app, window, click(true, cell(2, 1), control, 1));
        fake.send(&mut app, window, moved(cell(9, 1), control));
        fake.send(&mut app, window, click(false, cell(9, 1), control, 1));
        assert_eq!(selected_text(&app, window).as_deref(), Some("second"));
        assert_eq!(context_menu(&fake, window).0, cell(2, 1));
        assert_eq!(fake.context_menus().len(), 4);
        // Its items run in its window's pane, which has no focus.
        assert_eq!(fake.focused_window(), None);
        let chosen = |name: &str| WindowEvent::ContextCommand(name.to_owned());
        fake.send(&mut app, window, chosen("pane.copy"));
        assert_eq!(fake.clipboard_text().as_deref(), Some("second"));
        fake.send(&mut app, window, chosen("pane.select-all"));
        assert!(
            selected_text(&app, window).is_some_and(|text| text.starts_with("hello world\n")),
            "{:?}",
            selected_text(&app, window)
        );
    }

    #[test]
    fn look_up_shows_the_definition_of_the_selection_of_one_line() {
        let (mut fake, mut app, window) = selecting("", "");
        let none = Modifiers::NONE;
        let look_up = |fake: &mut Fake, app: &mut App| {
            fake.send(app, window, WindowEvent::ContextCommand(LOOK_UP.to_owned()));
        };
        right_click(&mut fake, &mut app, window, cell(8, 0), none);
        look_up(&mut fake, &mut app);
        // Over the word's first cell, at its baseline, in a window of
        // cells 4 points wide and 8 high, without the drawing's font.
        assert_eq!(
            fake.definitions(),
            [FakeDefinition {
                window,
                text: "world".to_owned(),
                baseline: Point { x: 24.0, y: 6.0 },
                font: None,
            }]
        );
        // A selection of two lines has none, and its item names nothing.
        select_drag(&mut fake, &mut app, window, (cell(0, 0), cell(5, 1)), none);
        right_click(&mut fake, &mut app, window, cell(2, 0), none);
        let (_, items, dimmed) = context_menu(&fake, window);
        assert_eq!(items[3], "Look Up");
        assert!(dimmed.contains(&LOOK_UP.to_owned()), "{dimmed:?}");
        look_up(&mut fake, &mut app);
        assert_eq!(fake.definitions().len(), 1);
        // On the second row, without the space before it, at the cell after
        // the space.
        select_drag(&mut fake, &mut app, window, (cell(6, 1), cell(12, 1)), none);
        right_click(&mut fake, &mut app, window, cell(8, 1), none);
        look_up(&mut fake, &mut app);
        let last = fake.definitions().last().unwrap();
        assert_eq!(
            (last.text.as_str(), last.baseline),
            ("line", Point { x: 28.0, y: 14.0 })
        );
    }

    #[test]
    fn the_man_page_of_the_selection_opens_in_a_window_of_its_own() {
        let (mut fake, mut app, window) = selecting("", "");
        // A man that tells its arguments, gives an empty title, prints a
        // page of 100 lines, and ends.
        let directory = std::env::temp_dir().join(format!("lokyr-man-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let man = directory.join("man");
        std::fs::write(
            &man,
            "#!/bin/sh\necho \"man $*\"\nprintf '\\033]2;\\007'\ni=1\nwhile [ $i -le 100 ]; do echo \"line $i\"; i=$((i+1)); done\n",
        )
        .unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&man, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        app.man = Some(man);
        let none = Modifiers::NONE;
        let open = |fake: &mut Fake, app: &mut App| {
            fake.send(
                app,
                window,
                WindowEvent::ContextCommand(MAN_PAGE.to_owned()),
            );
        };
        right_click(&mut fake, &mut app, window, cell(8, 0), none);
        open(&mut fake, &mut app);
        let opened: Vec<WindowId> = fake.windows().filter(|&other| other != window).collect();
        let [page] = opened[..] else {
            panic!("one window more: {opened:?}");
        };
        // Of 80 columns and 48 rows, cells 4 points wide and 8 high, its
        // title the page's, in the man page's colors.
        let shown = fake.window(page).unwrap();
        assert_eq!(shown.title, "man world — 80×48");
        // Light, as the app is; it neither keeps its frame nor restores
        // one.
        assert_eq!(shown.appearance, None);
        assert!(!shown.remember && !shown.restore && !shown.restore_size);
        assert_eq!(
            shown.size.points,
            Size {
                width: 320.0,
                height: 384.0
            }
        );
        let grid = app.pane(page).grid();
        assert_eq!((grid.columns, grid.rows), (80, 48));
        let yellow = app.theme.man_page.colors.background;
        assert_eq!(background(&mut fake, &mut app, page), yellow);
        // Once `man` has ended, the window keeps the page, from its top.
        run_until(&mut fake, &mut app, |_, app| {
            app.pane(page).is_kept() && row(app, page, 0) == "man -P ul -- world"
        });
        assert!(fake.windows().any(|other| other == page));
        assert_eq!(row(&app, page, 1), "line 1");
        // An empty title of the program's leaves the page's.
        assert_eq!(fake.window(page).unwrap().title, "man world — 80×48");
        // A pager's keys scroll it, and others do nothing.
        fake.send(&mut app, page, typed(KeyCode::ArrowDown, "", false));
        fake.send(&mut app, page, WindowEvent::Frame);
        assert_eq!(row(&app, page, 0), "line 1");
        fake.send(&mut app, page, typed(KeyCode::KeyQ, "q", false));
        fake.send(&mut app, page, typed(KeyCode::End, "", false));
        fake.send(&mut app, page, WindowEvent::Frame);
        assert!(fake.windows().any(|other| other == page));
        assert_eq!(app.pane(page).view_offset(), 0);
        // The appearance leaves its colors, and its title reads over them:
        // dark, in the light appearance of its own.
        fake.send_app(&mut app, AppEvent::AppearanceChanged(Appearance::Dark));
        assert_eq!(background(&mut fake, &mut app, page), yellow);
        assert_ne!(background(&mut fake, &mut app, window), yellow);
        assert_eq!(
            fake.window(page).unwrap().appearance,
            Some(Appearance::Light)
        );
        assert_eq!(fake.window(window).unwrap().appearance, None);
        // Two words name no page: the item is dimmed, and names nothing.
        select_drag(&mut fake, &mut app, window, (cell(0, 0), cell(11, 0)), none);
        right_click(&mut fake, &mut app, window, cell(2, 0), none);
        let (_, items, dimmed) = context_menu(&fake, window);
        assert_eq!(items[1], "Open man Page");
        assert!(dimmed.contains(&MAN_PAGE.to_owned()), "{dimmed:?}");
        assert_eq!(items[3], "Look Up “hello world”");
        open(&mut fake, &mut app);
        assert_eq!(fake.windows().count(), 2);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn a_man_page_s_name_takes_its_section_and_no_option() {
        let args = |text: &str| man_page(text).map(|args| args.join(" "));
        for (text, wanted) in [
            ("ls", "-- ls"),
            ("ls(1)", "-- 1 ls"),
            ("File::Spec(3pm)", "-- 3pm File::Spec"),
            ("c++", "-- c++"),
            ("git-commit", "-- git-commit"),
            ("pthread_create(3)", "-- 3 pthread_create"),
            ("printf(n)", "-- n printf"),
        ] {
            assert_eq!(args(text).as_deref(), Some(wanted), "{text}");
        }
        for text in [
            "-P",
            ".",
            "..",
            "./x.1",
            "a/b",
            "a b",
            "x;rm",
            "$(x)",
            "ls(",
            "(1)",
            "ls()",
            "ls(x)",
            "ls(123456789)",
            "",
            &"a".repeat(MAN_PAGE_NAME + 1),
        ] {
            assert_eq!(args(text), None, "{text}");
        }
        assert!(man_page(&"a".repeat(MAN_PAGE_NAME)).is_some());
        // Through the login shell, which takes them as they are.
        let args = man_page("ls(1)").unwrap();
        assert_eq!(
            man_program(None, &args),
            Program::LoginCommand {
                command: "exec man -P ul -- 1 ls".into()
            }
        );
        assert_eq!(
            man_program(Some(std::path::Path::new("/bin/echo")), &args),
            Program::command("/bin/echo", ["-P", "ul", "--", "1", "ls"])
        );
    }

    #[test]
    fn a_title_shortens_past_its_characters() {
        assert_eq!(shortened("abcdef", 3), "abc…");
        assert_eq!(shortened("abc", 3), "abc");
        assert_eq!(shortened("àèìòù", 2), "àè…");
        assert_eq!(shortened("", 2), "");
    }

    #[test]
    fn a_program_that_takes_the_clicks_has_the_right_button_but_with_shift() {
        let (mut fake, mut app, window) = selecting("", "\\033[?1000h\\033[?1006h");
        right_click(&mut fake, &mut app, window, cell(2, 0), Modifiers::NONE);
        let control = Modifiers {
            control: true,
            ..Modifiers::NONE
        };
        fake.send(&mut app, window, click(true, cell(4, 0), control, 1));
        fake.send(&mut app, window, click(false, cell(4, 0), control, 1));
        run_until(&mut fake, &mut app, |_, app| {
            shows(app, window, "^[[<2;3;1M^[[<2;3;1m^[[<16;5;1M^[[<16;5;1m")
        });
        assert!(fake.context_menus().is_empty());
        // With Shift, the menu opens, and the program learns of nothing.
        right_click(&mut fake, &mut app, window, cell(8, 0), SHIFT);
        assert_eq!(selected_text(&app, window).as_deref(), Some("world"));
        assert_eq!(context_menu(&fake, window).0, cell(8, 0));
        fake.send(&mut app, window, typed(KeyCode::KeyX, "x", false));
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "1mx"));
    }

    /// The app without padding, its window running `seq 1 100`, which
    /// prints `101` to `103` at each Return: and the screen's row of `90`.
    fn numbers() -> (Fake, App, WindowId, u16) {
        // The echo off before `ready`, so that no Return typed after it
        // echoes a line of its own.
        let program = shell(concat!(
            "seq 1 100; stty -echo; echo ready; ",
            "read line; seq 101 103; read line; seq 104 106; read line"
        ));
        let (fake, app, window) = started_with(program);
        let rows = app.pane(window).grid().rows;
        let ninety = (0..rows)
            .find(|&r| row(&app, window, r) == "90")
            .expect("90 on the screen");
        (fake, app, window, ninety)
    }

    /// Return, for the program's next numbers, and their drawing.
    fn more_output(fake: &mut Fake, app: &mut App, window: WindowId, last: &str) {
        fake.send(app, window, typed(KeyCode::Enter, "\r", false));
        run_until(fake, app, |_, app| shows(app, window, last));
    }

    #[test]
    fn output_during_a_drag_leaves_its_anchor_on_its_text() {
        let (mut fake, mut app, window, ninety) = numbers();
        let none = Modifiers::NONE;
        fake.send(&mut app, window, click(true, cell(0, ninety), none, 1));
        fake.send(&mut app, window, moved(cell(2, ninety), none));
        assert_eq!(selected_text(&app, window).as_deref(), Some("90"));
        // Three lines scroll `90` up; the pointer, on the same row of the
        // screen, now over `93`.
        more_output(&mut fake, &mut app, window, "103");
        fake.send(&mut app, window, moved(cell(2, ninety), none));
        assert_eq!(
            selected_text(&app, window).as_deref(),
            Some("90\n91\n92\n93")
        );
        fake.send(&mut app, window, click(false, cell(2, ninety), none, 1));
        // Shift's click after more output extends from `90` too.
        more_output(&mut fake, &mut app, window, "106");
        fake.send(&mut app, window, click(true, cell(2, ninety), SHIFT, 1));
        fake.send(&mut app, window, click(false, cell(2, ninety), SHIFT, 1));
        let text = selected_text(&app, window).unwrap();
        assert!(
            text.starts_with("90\n91") && text.ends_with("\n96"),
            "{text:?}"
        );
    }

    #[test]
    fn a_drag_past_the_bottom_scrolls_the_view_down_and_a_release_stops_it() {
        let mut fake = Fake::default();
        let mut app = app(
            unpadded(),
            shell("seq 1 200; echo ready; stty raw -echo; cat -v"),
        );
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
        app.pane(window).scroll(Scroll::Up(10));
        let none = Modifiers::NONE;
        fake.send(&mut app, window, click(true, cell(3, 2), none, 1));
        // Two rows below the window's bottom.
        let rows = f64::from(app.pane(window).grid().rows);
        let below = Point {
            x: 1.0,
            y: 8.0 * (rows + 2.0),
        };
        fake.send(&mut app, window, moved(below, none));
        assert!(app.scroll_timer.is_some());
        fake.fire_timers(&mut app, Instant::now() + SELECTION_SCROLL);
        assert!(app.pane(window).view_offset() < 10);
        // The release, past the edge, stops it.
        fake.send(&mut app, window, click(false, below, none, 1));
        fake.fire_timers(&mut app, Instant::now() + SELECTION_SCROLL);
        assert!(app.scroll_timer.is_none());
        let offset = app.pane(window).view_offset();
        fake.fire_timers(&mut app, Instant::now() + SELECTION_SCROLL * 2);
        assert_eq!(app.pane(window).view_offset(), offset);
    }

    #[test]
    fn a_double_click_copies_its_word_on_select() {
        let (mut fake, mut app, window) = selecting("copy-on-select = true\n", "");
        let none = Modifiers::NONE;
        fake.send(&mut app, window, click(true, cell(7, 0), none, 2));
        fake.send(&mut app, window, click(false, cell(7, 0), none, 2));
        assert_eq!(fake.clipboard_text().as_deref(), Some("world"));
    }

    #[test]
    fn a_drag_past_the_top_scrolls_the_view_under_the_selection() {
        let mut fake = Fake::default();
        let mut app = app(
            unpadded(),
            shell("seq 1 200; echo ready; stty raw -echo; cat -v"),
        );
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
        let none = Modifiers::NONE;
        fake.send(&mut app, window, click(true, cell(3, 2), none, 1));
        // Two rows above the window's top.
        let above = Point { x: 1.0, y: -12.0 };
        fake.send(&mut app, window, moved(above, none));
        assert!(app.scroll_timer.is_some());
        for _ in 0..3 {
            fake.fire_timers(&mut app, Instant::now() + SELECTION_SCROLL);
        }
        // Three steps of two rows; the selection goes up with the view.
        assert_eq!(app.pane(window).view_offset(), 6);
        // From where the view brought the pointer, six rows above the
        // screen's top row, `141`, to the press's, after `143`.
        let text = selected_text(&app, window).unwrap();
        assert_eq!(text.lines().count(), 9, "{text:?}");
        assert!(text.starts_with("135\n136"), "{text:?}");
        assert!(text.ends_with("142\n143"), "{text:?}");
        // Back within the grid, the scrolling stops.
        fake.send(&mut app, window, moved(cell(0, 0), none));
        fake.fire_timers(&mut app, Instant::now() + SELECTION_SCROLL);
        assert_eq!(app.pane(window).view_offset(), 6);
        assert!(app.scroll_timer.is_none());
        fake.send(&mut app, window, click(false, cell(0, 0), none, 1));
    }

    fn wheel(delta: ScrollDelta, mods: Modifiers) -> WindowEvent {
        WindowEvent::Scroll {
            delta,
            phase: None,
            momentum: false,
            position: cell(0, 0),
            mods,
        }
    }

    const SHIFT: Modifiers = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };

    /// A program that turns the mouse's tracking on with `modes`, says
    /// `ready`, and shows what it reads, in raw mode.
    fn reader(modes: &str) -> Program {
        shell(&format!(
            "stty raw -echo; printf '{modes}'; echo ready; cat -v"
        ))
    }

    /// The app without padding, its window running `program`, once it
    /// shows `ready`.
    fn started_with(program: Program) -> (Fake, App, WindowId) {
        let mut fake = Fake::default();
        let mut app = app(unpadded(), program);
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        run_until(&mut fake, &mut app, |_, app| {
            screen(app, window)
                .iter()
                .any(|line| line.contains("ready"))
        });
        (fake, app, window)
    }

    fn shows(app: &App, window: WindowId, text: &str) -> bool {
        screen(app, window).iter().any(|line| line.contains(text))
    }

    #[test]
    fn a_click_reaches_a_program_that_tracks_the_mouse_but_with_shift() {
        let (mut fake, mut app, window) = started_with(reader("\\033[?1000h\\033[?1006h"));
        // Shift keeps the click for the terminal.
        fake.send(
            &mut app,
            window,
            press(MouseButton::Left, true, cell(2, 1), SHIFT),
        );
        fake.send(
            &mut app,
            window,
            press(MouseButton::Left, false, cell(2, 1), SHIFT),
        );
        fake.send(
            &mut app,
            window,
            press(MouseButton::Left, true, cell(4, 2), Modifiers::NONE),
        );
        fake.send(
            &mut app,
            window,
            press(MouseButton::Left, false, cell(4, 2), Modifiers::NONE),
        );
        run_until(&mut fake, &mut app, |_, app| {
            shows(app, window, "^[[<0;5;3M^[[<0;5;3m")
        });
        assert!(!shows(&app, window, ";3;2"), "{:?}", screen(&app, window));
    }

    #[test]
    fn a_release_with_shift_reaches_a_program_that_learned_of_the_press() {
        let (mut fake, mut app, window) = started_with(reader("\\033[?1000h\\033[?1006h"));
        fake.send(
            &mut app,
            window,
            press(MouseButton::Left, true, cell(4, 2), Modifiers::NONE),
        );
        fake.send(
            &mut app,
            window,
            press(MouseButton::Left, false, cell(4, 2), SHIFT),
        );
        run_until(&mut fake, &mut app, |_, app| {
            shows(app, window, "^[[<0;5;3M^[[<4;5;3m")
        });
    }

    #[test]
    fn a_release_goes_nowhere_when_the_program_did_not_learn_of_the_press() {
        // The mouse's tracking from the first key on.
        let program = shell(
            "stty raw -echo; echo ready; head -c 1 >/dev/null; printf '\\033[?1000h\\033[?1006h'; echo on; cat -v",
        );
        let (mut fake, mut app, window) = started_with(program);
        let at = cell(4, 2);
        fake.send(
            &mut app,
            window,
            press(MouseButton::Left, true, at, Modifiers::NONE),
        );
        fake.send(&mut app, window, key());
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "on"));
        for pressed in [false, true, false] {
            let event = press(MouseButton::Left, pressed, at, Modifiers::NONE);
            fake.send(&mut app, window, event);
        }
        run_until(&mut fake, &mut app, |_, app| {
            screen(app, window)
                .iter()
                .any(|line| line.trim() == "^[[<0;5;3M^[[<0;5;3m")
        });
    }

    #[test]
    fn a_click_in_a_scrolled_view_reaches_the_program_at_the_screen_s_row() {
        let program = shell(
            "seq 1 100; printf '\\033[?1003h\\033[?1006h'; stty raw -echo; echo ready; cat -v",
        );
        let (mut fake, mut app, window) = started_with(program);
        let offset = |app: &App| app.pane(window).snapshot().offset();
        // Shift's wheel moves the view, which the program takes otherwise.
        let up = ScrollDelta::Lines { x: 0.0, y: 3.0 };
        fake.send(&mut app, window, wheel(up, SHIFT));
        run_until(&mut fake, &mut app, |_, app| offset(app) == 3);
        // A move and a click above the screen go nowhere; those on its
        // fifth row, the view's eighth, at its row.
        for at in [cell(3, 2), cell(3, 7)] {
            let moved = WindowEvent::MouseMoved {
                position: at,
                mods: Modifiers::NONE,
            };
            fake.send(&mut app, window, moved);
        }
        for row in [2, 7] {
            for pressed in [true, false] {
                let event = press(MouseButton::Left, pressed, cell(1, row), Modifiers::NONE);
                fake.send(&mut app, window, event);
            }
        }
        // The view stays up as the program writes: down to read it.
        let down = ScrollDelta::Lines { x: 0.0, y: -3.0 };
        fake.send(&mut app, window, wheel(down, SHIFT));
        run_until(&mut fake, &mut app, |_, app| {
            screen(app, window)
                .iter()
                .any(|line| line.trim() == "^[[<35;4;5M^[[<0;2;5M^[[<0;2;5m")
        });
    }

    #[test]
    fn a_composition_brings_the_view_back_to_the_screen() {
        let mut fake = Fake::default();
        let mut app = app(unpadded(), shell("seq 1 100; echo ready; sleep 10"));
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
        let offset = |app: &App| app.pane(window).snapshot().offset();
        let up = ScrollDelta::Lines { x: 0.0, y: 3.0 };
        fake.send(&mut app, window, wheel(up, Modifiers::NONE));
        run_until(&mut fake, &mut app, |_, app| offset(app) == 3);
        let composing = Ime::Preedit {
            text: "に".to_owned(),
            selection: 3..3,
        };
        fake.send(&mut app, window, WindowEvent::Ime(composing));
        run_until(&mut fake, &mut app, |_, app| offset(app) == 0);
    }

    #[test]
    fn the_candidates_go_at_a_composition_moved_left_of_the_cursor() {
        // The cursor in the last column.
        let mut fake = Fake::default();
        let mut app = app(unpadded(), shell("printf 'ready\\033[999G'; sleep 10"));
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
        let columns = app.pane(window).snapshot().cols();
        let area = |fake: &mut Fake, app: &mut App| {
            fake.send(app, window, WindowEvent::Frame);
            fake.window(window).unwrap().text_area.unwrap().0.x
        };
        run_until(&mut fake, &mut app, |_, app| {
            app.pane(window).snapshot().cursor().position.x == columns - 1
        });
        // A column is 4 points: the cursor's, then the composition's, of
        // four cells, as far left as it takes.
        assert_eq!(area(&mut fake, &mut app), f64::from(columns - 1) * 4.0);
        let composing = Ime::Preedit {
            text: "日本".to_owned(),
            selection: 6..6,
        };
        fake.send(&mut app, window, WindowEvent::Ime(composing));
        assert_eq!(area(&mut fake, &mut app), f64::from(columns - 4) * 4.0);
    }

    /// The app without padding, of a config whose `[clipboard]` reads
    /// `read`, its window running `program`, once it shows `ready`.
    fn with_clipboard(read: &str, program: Program) -> (Fake, App, WindowId) {
        let mut config = Config::parse(
            &format!("[clipboard]\nread = \"{read}\"\n"),
            std::path::Path::new("config.toml"),
        )
        .unwrap();
        config.window = unpadded().window;
        let mut fake = Fake::default();
        let mut app = app(config, program);
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
        (fake, app, window)
    }

    #[test]
    fn a_program_s_write_reaches_the_clipboard() {
        // `hello` to the clipboard, then `other` to the primary selection
        // alone, which macOS lacks.
        let program = shell(
            "printf '\\033]52;c;aGVsbG8=\\a'; printf '\\033]52;p;b3RoZXI=\\a'; echo ready; sleep 10",
        );
        let (mut fake, mut app, _) = with_clipboard("never", program);
        run_until(&mut fake, &mut app, |fake, _| {
            fake.clipboard_text().as_deref() == Some("hello")
        });
        let pause = Instant::now() + Duration::from_millis(300);
        run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
        assert_eq!(fake.clipboard_text().as_deref(), Some("hello"));
    }

    /// A program that reads the clipboard once it reads a byte, and shows
    /// the answer.
    fn clipboard_reader() -> Program {
        shell(
            "stty raw -echo; echo ready; while :; do head -c 1 >/dev/null; printf '\\033]52;c;?\\a'; done & cat -v",
        )
    }

    #[test]
    fn a_read_gets_no_answer_unless_the_user_allows_it() {
        // Off by default: no alert, no answer.
        let (mut fake, mut app, window) = with_clipboard("never", clipboard_reader());
        fake.set_clipboard_text("secret");
        fake.send(&mut app, window, key());
        let pause = Instant::now() + Duration::from_millis(400);
        run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
        assert!(fake.alerts().is_empty());
        assert!(!shows(&app, window, "52;"), "{:?}", screen(&app, window));
        // Asked: refused, then allowed; a read while the alert is open asks
        // nothing more.
        let (mut fake, mut app, window) = with_clipboard("ask", clipboard_reader());
        fake.set_clipboard_text("secret");
        for allowed in [false, true] {
            fake.send(&mut app, window, key());
            run_until(&mut fake, &mut app, |fake, _| !fake.alerts().is_empty());
            fake.send(&mut app, window, key());
            let pause = Instant::now() + Duration::from_millis(300);
            run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
            assert_eq!(fake.alerts().len(), 1);
            let open = &fake.alerts()[0];
            assert_eq!(open.window, Some(window));
            assert_eq!(
                open.alert.buttons,
                [
                    app.texts.text("clipboard-read-deny"),
                    app.texts.text("clipboard-read-allow")
                ]
            );
            let id = open.id;
            fake.answer_alert(&mut app, id, usize::from(allowed));
            if !allowed {
                let pause = Instant::now() + Duration::from_millis(300);
                run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
                assert!(!shows(&app, window, "52;"), "{:?}", screen(&app, window));
            }
        }
        run_until(&mut fake, &mut app, |_, app| {
            shows(app, window, "^[]52;c;c2VjcmV0^G")
        });
        let shown = screen(&app, window).concat();
        assert_eq!(shown.matches("52;").count(), 1, "{shown:?}");
    }

    #[test]
    fn a_config_that_asks_applies_to_the_open_windows() {
        let (mut fake, mut app, window) = with_clipboard("never", clipboard_reader());
        let mut asking = app.config.clone();
        asking.clipboard.read = ClipboardRead::Ask;
        app.apply(&mut fake, asking);
        fake.send(&mut app, window, key());
        run_until(&mut fake, &mut app, |fake, _| fake.alerts().len() == 1);
        // A window that closes takes its ask with it, once its program's
        // question, as `cat` runs, is answered.
        fake.send(&mut app, window, WindowEvent::CloseRequested);
        let close = fake.alerts().last().unwrap().id;
        fake.answer_alert(&mut app, close, 0);
        assert!(app.clipboard_asks.is_empty());
    }

    /// The app without padding, of the config `text`, its window running
    /// `program`, once it shows `ready`.
    fn configured(text: &str, program: Program) -> (Fake, App, WindowId) {
        let mut config = Config::parse(text, std::path::Path::new("config.toml")).unwrap();
        config.window = unpadded().window;
        let mut fake = Fake::default();
        let mut app = app(config, program);
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
        (fake, app, window)
    }

    #[test]
    fn the_grid_starts_below_the_titlebar() {
        let mut fake = Fake::default();
        fake.set_bar(14.0);
        let mut app = app(unpadded(), selecting_program());
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
        // The size asked for is below the bar, whose height the grid leaves.
        let size = fake.window(window).unwrap().size;
        assert_eq!(size.points.height, 510.0);
        let grid = app.pane(window).grid();
        assert_eq!((grid.columns, grid.rows), (200, 62));
        // The cells start below it.
        let below = |col: u16, row: u16| Point {
            y: cell(col, row).y + 14.0,
            ..cell(col, row)
        };
        fake.send(
            &mut app,
            window,
            click(true, below(8, 0), Modifiers::NONE, 2),
        );
        fake.send(
            &mut app,
            window,
            click(false, below(8, 0), Modifiers::NONE, 2),
        );
        assert_eq!(selected_text(&app, window).as_deref(), Some("world"));
        fake.send(
            &mut app,
            window,
            click(true, below(3, 1), Modifiers::NONE, 2),
        );
        fake.send(
            &mut app,
            window,
            click(false, below(3, 1), Modifiers::NONE, 2),
        );
        assert_eq!(selected_text(&app, window).as_deref(), Some("second"));
        // Over the bar, the first row.
        fake.send(
            &mut app,
            window,
            click(true, cell(8, 0), Modifiers::NONE, 2),
        );
        fake.send(
            &mut app,
            window,
            click(false, cell(8, 0), Modifiers::NONE, 2),
        );
        assert_eq!(selected_text(&app, window).as_deref(), Some("world"));
        // An opaque bar covers the bar's pixels, of the background.
        let background = Color::from_rgb8(1, 2, 3);
        let Some(Item::Rect { rect, color }) = bar_item(size, background) else {
            panic!("a bar");
        };
        assert_eq!((rect.x, rect.y, rect.width, rect.height), (0, 0, 1600, 28));
        assert_eq!(color, background.with_alpha(1.0));
        assert!(bar_item(points(400.0, 300.0), background).is_none());
    }

    /// A program that shows `hello world` and `second line`, then `ready`.
    fn selecting_program() -> Program {
        shell("stty raw -echo; printf 'hello world\\r\\nsecond line\\r\\n'; echo ready; cat -v")
    }

    #[test]
    fn the_title_tells_the_directory_the_program_and_the_size() {
        // A program that starts without a word: the last look after the
        // output's frames finds it.
        let mut fake = Fake::default();
        let mut app = app(
            unpadded(),
            shell("cd /usr; stty raw -echo; echo ready; sleep 0.1; exec cat"),
        );
        app.title_look = Some(TITLE_LOOK);
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        let title = |fake: &Fake| fake.window(window).unwrap().title.clone();
        run_until(&mut fake, &mut app, |fake, _| {
            title(fake) == "usr — cat — 200×62"
        });
        // Without the size.
        app.config.window.title_size = false;
        app.show_title(&mut fake, window);
        assert_eq!(title(&fake), "usr — cat");
        // A title of the program's, as a shell's prompt sets, and the
        // program in the foreground after it; once, when they are one.
        app.config.window.title_size = true;
        app.pane(window).text("\u{1b}]2;prompt\u{7}");
        run_until(&mut fake, &mut app, |fake, _| {
            title(fake) == "prompt — cat — 200×62"
        });
        app.pane(window).text("\u{1b}]2;cat\u{7}");
        run_until(&mut fake, &mut app, |fake, _| title(fake) == "cat — 200×62");
        // The directory's names.
        let home = Some(std::path::Path::new("/Users/me"));
        let named = |directory: &str| directory_name(std::path::Path::new(directory), home);
        assert_eq!(named("/Users/me/src/lokyr"), "lokyr");
        assert_eq!(named("/Users/me"), "~");
        assert_eq!(named("/"), "/");
        assert_eq!(directory_name(std::path::Path::new("/tmp"), None), "tmp");
    }

    #[test]
    fn the_cursor_s_blinking_takes_no_look_at_the_title() {
        let (mut fake, mut app, window) = focused();
        app.title_look = Some(TITLE_LOOK);
        // The blink's frames, which no output brings: its timer alone.
        for _ in 0..4 {
            let (_, at) = app.blink_timer.unwrap();
            fake.fire_timers(&mut app, at);
            assert!(fake.send(&mut app, window, WindowEvent::Frame));
            assert_eq!(fake.timers().count(), 1);
            assert!(app.title_timer.is_none());
        }
        // A frame of output starts the titles' timer.
        app.pane(window).text("x");
        run_until(&mut fake, &mut app, |_, app| app.title_timer.is_some());
    }

    #[test]
    fn the_title_s_last_look_comes_after_the_window_s_last_output() {
        let (mut fake, mut app, window) = focused();
        app.title_look = Some(TITLE_LOOK);
        app.pane(window).text("x");
        run_until(&mut fake, &mut app, |_, app| app.title_timer.is_some());
        let (_, at) = app.title_timer.unwrap();
        // A frame of output came after the timer started, and the program
        // changed since.
        let later = at - TITLE_LOOK / 2;
        let state = app.windows.get_mut(&window).unwrap();
        state.output_at = Some(later);
        state.place.1 = "stale".to_owned();
        // The timer looks not yet, and starts again for that frame.
        fake.fire_timers(&mut app, at);
        assert_eq!(app.windows[&window].place.1, "stale");
        assert_eq!(app.title_timer.map(|(_, at)| at), Some(later + TITLE_LOOK));
        fake.fire_timers(&mut app, later + TITLE_LOOK);
        assert_eq!(app.windows[&window].place.1, "cat");
        assert!(app.title_timer.is_none());
        assert_eq!(app.windows[&window].output_at, None);
    }

    #[test]
    fn a_directory_that_cannot_be_read_keeps_the_title_s_place() {
        // A program of another user's, as `login` runs, whose directory the
        // system does not tell, waiting for a name.
        let mut fake = Fake::default();
        let mut app = app(
            unpadded(),
            shell("cd /usr; stty -echo; echo ready; read line; echo gone; exec /usr/bin/login"),
        );
        app.title_look = Some(TITLE_LOOK);
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        let title = |fake: &Fake| fake.window(window).unwrap().title.clone();
        run_until(&mut fake, &mut app, |fake, _| {
            title(fake).starts_with("usr — ")
        });
        fake.send(&mut app, window, typed(KeyCode::Enter, "\r", false));
        run_until(&mut fake, &mut app, |fake, _| {
            title(fake).ends_with(" — login — 200×62")
        });
        assert_eq!(title(&fake), "usr — login — 200×62");
    }

    #[test]
    fn a_kept_screen_keeps_its_title_s_place() {
        let mut config = unpadded();
        config.window.close_on_exit = CloseOnExit::Never;
        let mut fake = Fake::default();
        let mut app = app(
            config,
            shell("cd /usr; stty raw -echo; echo ready; head -c 1 >/dev/null; exit 3"),
        );
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        let title = |fake: &Fake| fake.window(window).unwrap().title.clone();
        run_until(&mut fake, &mut app, |fake, _| {
            title(fake).starts_with("usr — ")
        });
        fake.send(&mut app, window, key());
        run_until(&mut fake, &mut app, |_, app| app.pane(window).is_kept());
        fake.send(&mut app, window, WindowEvent::Frame);
        assert!(title(&fake).starts_with("usr — "), "{}", title(&fake));
    }

    #[test]
    fn a_window_takes_the_appearance_of_its_background_where_it_is_not_the_app_s() {
        let gray = |level: u8| Color::from_rgb8(level, level, level);
        assert_eq!(appearance_over(gray(250), Appearance::Light), None);
        assert_eq!(
            appearance_over(gray(250), Appearance::Dark),
            Some(Appearance::Light)
        );
        assert_eq!(
            appearance_over(gray(30), Appearance::Light),
            Some(Appearance::Dark)
        );
        assert_eq!(appearance_over(gray(30), Appearance::Dark), None);
        // Past the luminance where black and white contrast alike, about
        // 118 of 255.
        assert_eq!(
            appearance_over(gray(124), Appearance::Dark),
            Some(Appearance::Light)
        );
        assert_eq!(
            appearance_over(gray(112), Appearance::Light),
            Some(Appearance::Dark)
        );
    }

    #[test]
    fn a_title_of_the_user_s_takes_the_place_of_the_window_s_own() {
        let program = shell("stty raw -echo; echo ready; cat");
        let (mut fake, mut app, window) = configured("", program);
        fake.send(&mut app, window, WindowEvent::Focused(true));
        let title = |fake: &Fake| fake.window(window).unwrap().title.clone();
        let own = app.title_of(window);
        let ask = |fake: &mut Fake, app: &mut App| {
            fake.send(app, window, command_key_shift(KeyCode::KeyI, "i"));
            let open = fake.alerts().last().expect("the question").clone();
            assert_eq!(open.window, Some(window));
            open
        };
        // The question holds nothing at first; its title, trimmed.
        let open = ask(&mut fake, &mut app);
        assert_eq!(open.alert.input.as_deref(), Some(""));
        assert_eq!(open.alert.buttons, ["Set", "Cancel"]);
        // A second ask waits for the first's answer.
        fake.send_app(&mut app, AppEvent::Command(SET_TITLE.to_owned()));
        assert_eq!(fake.alerts().len(), 1);
        fake.answer_alert_typed(&mut app, open.id, (0, false), Some("  build  "));
        assert_eq!(title(&fake), "build — 200×62");
        // A program's title stays behind it.
        app.pane(window).text("\u{1b}]2;prompt\u{7}");
        let pause = Instant::now() + Duration::from_millis(200);
        run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
        assert_eq!(title(&fake), "build — 200×62");
        // Asked again, it holds the user's; cancelled, it stays.
        let open = ask(&mut fake, &mut app);
        assert_eq!(open.alert.input.as_deref(), Some("build"));
        fake.answer_alert_typed(&mut app, open.id, (1, false), Some("other"));
        assert_eq!(title(&fake), "build — 200×62");
        // Emptied, the window's own comes back, of the program's title.
        let open = ask(&mut fake, &mut app);
        fake.answer_alert_typed(&mut app, open.id, (0, false), Some(" "));
        assert_eq!(title(&fake), "prompt — bash — 200×62");
        assert_ne!(own, title(&fake));
    }

    /// The press of Command and Shift and the key of `letter`.
    fn command_key_shift(code: KeyCode, letter: &str) -> WindowEvent {
        let mods = Modifiers { shift: true, ..CMD };
        key_event(code, letter, mods, true)
    }

    #[test]
    fn a_window_takes_its_program_s_title() {
        let program = shell(
            "stty raw -echo; printf '\\033]2;x\\a'; echo ready; head -c 1 >/dev/null; printf '\\033]2;\\a'; sleep 10",
        );
        let (mut fake, mut app, window) = configured("", program);
        let title = |fake: &Fake| fake.window(window).unwrap().title.clone();
        let size = |app: &App| {
            let grid = app.pane(window).grid();
            format!("{TITLE_DASH}{}×{}", grid.columns, grid.rows)
        };
        // The program's title, then the program in the foreground: the
        // shell that runs the script.
        run_until(&mut fake, &mut app, |fake, app| {
            title(fake) == format!("x{TITLE_DASH}bash{}", size(app))
        });
        // A title of nothing gives the window's own.
        fake.send(&mut app, window, key());
        run_until(&mut fake, &mut app, |fake, app| {
            !title(fake).starts_with('x') && title(fake) == app.title_of(window)
        });
    }

    /// A program that rings its bell at each key, twice the second time
    /// within a fifth of a second.
    fn ringer() -> Program {
        shell(
            "stty raw -echo; echo ready; while :; do head -c 1 >/dev/null; printf '\\a'; sleep 0.05; printf '\\a'; done",
        )
    }

    #[test]
    fn a_bell_flashes_a_window_with_the_focus_and_marks_one_without() {
        let (mut fake, mut app, window) = configured("", ringer());
        fake.send(&mut app, window, WindowEvent::Focused(true));
        fake.send(&mut app, window, key());
        run_until(&mut fake, &mut app, |_, app| {
            app.windows[&window].flash_until.is_some()
        });
        // The flash ends with its timer.
        run_until(&mut fake, &mut app, |_, app| {
            app.windows[&window].flash_until.is_none()
        });
        assert_eq!((fake.alert_sounds(), fake.attention_requests()), (0, 0));
        assert!(!fake.window(window).unwrap().title.starts_with(BELL_MARK));
        // Without the focus: a bounce and the mark, once for the two bells,
        // until the window takes the focus. The bells of the first key are
        // a fifth of a second behind.
        let pause = Instant::now() + Duration::from_millis(250);
        run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
        fake.send(&mut app, window, WindowEvent::Focused(false));
        fake.send(&mut app, window, key());
        run_until(&mut fake, &mut app, |fake, app| {
            fake.window(window).unwrap().title == format!("🔔 {}", app.title_of(window))
        });
        let pause = Instant::now() + Duration::from_millis(300);
        run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
        assert_eq!(fake.attention_requests(), 1);
        assert!(app.windows[&window].flash_until.is_none());
        fake.send(&mut app, window, WindowEvent::Focused(true));
        assert_eq!(fake.window(window).unwrap().title, app.title_of(window));
    }

    #[test]
    fn the_bell_s_config_chooses_what_it_does() {
        let config = "[bell]\nsound = true\nflash = false\nbounce = false\nmark-title = false\n";
        let (mut fake, mut app, window) = configured(config, ringer());
        fake.send(&mut app, window, WindowEvent::Focused(true));
        fake.send(&mut app, window, key());
        run_until(&mut fake, &mut app, |fake, _| fake.alert_sounds() == 1);
        assert!(app.windows[&window].flash_until.is_none());
        fake.send(&mut app, window, WindowEvent::Focused(false));
        let pause = Instant::now() + Duration::from_millis(250);
        run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
        fake.send(&mut app, window, key());
        run_until(&mut fake, &mut app, |fake, _| fake.alert_sounds() == 2);
        let pause = Instant::now() + Duration::from_millis(300);
        run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
        assert_eq!(fake.alert_sounds(), 2, "one sound for the two bells");
        assert_eq!(fake.attention_requests(), 0);
        assert!(!fake.window(window).unwrap().title.starts_with(BELL_MARK));
        assert!(app.windows[&window].flash_until.is_none());
    }

    /// A program that sends a notification of OSC 9 and one of OSC 777 at
    /// each key.
    fn notifier() -> Program {
        shell(
            "stty raw -echo; printf '\\033]2;agent\\a'; echo ready; while :; do head -c 1 >/dev/null; printf '\\033]9;waiting\\a'; printf '\\033]777;notify;Done;all tests pass\\a'; done",
        )
    }

    #[test]
    fn a_notification_shows_for_a_window_without_the_focus() {
        let (mut fake, mut app, window) = configured("", notifier());
        // Not while the window has the focus.
        fake.send(&mut app, window, WindowEvent::Focused(true));
        fake.send(&mut app, window, key());
        let pause = Instant::now() + Duration::from_millis(300);
        run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
        assert!(fake.notifications().is_empty());
        // Without it, the first of the two, its title the window's, as
        // OSC 9 gives none; the second within the pause shows nothing.
        fake.send(&mut app, window, WindowEvent::Focused(false));
        fake.send(&mut app, window, key());
        run_until(&mut fake, &mut app, |fake, _| {
            !fake.notifications().is_empty()
        });
        let pause = Instant::now() + Duration::from_millis(300);
        run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
        let shown = fake.notifications();
        assert_eq!(shown.len(), 1, "{shown:?}");
        assert_eq!(
            (
                shown[0].window,
                shown[0].title.as_str(),
                shown[0].body.as_str()
            ),
            (window, "agent", "waiting")
        );
        // A click brings the window forward.
        fake.send_app(&mut app, AppEvent::NotificationClicked(window));
        assert_eq!(fake.focus_requests(), [window]);
        // A window that closed is not asked for.
        fake.send_app(&mut app, AppEvent::NotificationClicked(WindowId::new(999)));
        assert_eq!(fake.focus_requests(), [window]);
    }

    #[test]
    fn a_notification_without_a_title_takes_the_user_s() {
        let (mut fake, mut app, window) = configured("", notifier());
        app.windows.get_mut(&window).unwrap().user_title = Some("build".to_owned());
        fake.send(&mut app, window, WindowEvent::Focused(false));
        fake.send(&mut app, window, key());
        run_until(&mut fake, &mut app, |fake, _| {
            !fake.notifications().is_empty()
        });
        assert_eq!(fake.notifications()[0].title, "build");
    }

    #[test]
    fn a_notification_s_title_and_text_show_their_start() {
        let long = "x".repeat(5000);
        let program = shell(&format!(
            "stty raw -echo; echo ready; head -c 1 >/dev/null; printf '\\033]777;notify;{long};{long}\\a'; sleep 10"
        ));
        let (mut fake, mut app, window) = configured("", program);
        fake.send(&mut app, window, key());
        run_until(&mut fake, &mut app, |fake, _| {
            !fake.notifications().is_empty()
        });
        let shown = &fake.notifications()[0];
        assert_eq!(
            (shown.title.chars().count(), shown.body.chars().count()),
            (NOTIFICATION_TITLE, NOTIFICATION_BODY)
        );
    }

    /// The characters of the view's row `row` of `window`, each by its
    /// column, and whether it is a cluster of several codepoints.
    fn cells_of(app: &App, window: WindowId, row: u16) -> Vec<(u16, char, bool)> {
        use lokyr_vt_core::page::Content;

        let snapshot = app.pane(window).snapshot();
        (0..)
            .zip(snapshot.line(row).cells())
            .filter_map(|(col, cell)| match cell.content() {
                Content::Char(character) if character != '\0' => Some((col, character, false)),
                Content::Grapheme(character) => Some((col, character, true)),
                _ => None,
            })
            .collect()
    }

    /// A program that prints its `TERM`, then an emoji, an ambiguous `α`
    /// and an accent that combines, each before a letter, at the start and
    /// at each key, a row each time.
    fn printer() -> Program {
        shell(
            "stty raw -echo; printf '%s\\r\\nready\\r\\n' \"$TERM\"; while :; do printf '\\360\\237\\230\\200x\\316\\261ye\\314\\201z\\r\\n'; head -c 1 >/dev/null; done",
        )
    }

    /// The columns of `x`, `y` and `z` on the view's row `row`, and whether
    /// the accent's character before `z` is a cluster.
    fn placed(app: &App, window: WindowId, row: u16) -> Option<(u16, u16, u16, bool)> {
        let cells = cells_of(app, window, row);
        let at = |wanted: char| {
            cells
                .iter()
                .find(|&&(_, c, _)| c == wanted)
                .map(|&(col, ..)| col)
        };
        let z = at('z')?;
        let accent = cells.iter().rev().find(|&&(col, ..)| col < z)?.2;
        Some((at('x')?, at('y')?, z, accent))
    }

    #[test]
    fn a_new_window_takes_the_terminal_s_settings() {
        let (mut fake, mut app, window) = configured("", printer());
        run_until(&mut fake, &mut app, |_, app| {
            placed(app, window, 2).is_some()
        });
        assert_eq!(row(&app, window, 0).trim(), "xterm-256color");
        // An emoji of two cells, `α` of one, and the accent kept apart.
        assert_eq!(placed(&app, window, 2), Some((2, 4, 6, true)));
        let config = "[terminal]\nterm = \"vt220\"\nunicode-widths = \"8\"\nambiguous-width = 2\nnormalization = \"nfc\"\n";
        let (mut fake, mut app, window) = configured(config, printer());
        run_until(&mut fake, &mut app, |_, app| {
            placed(app, window, 2).is_some()
        });
        assert_eq!(row(&app, window, 0).trim(), "vt220");
        // The emoji of one cell, `α` of two, and `é` composed, of two too,
        // as it is ambiguous.
        assert_eq!(placed(&app, window, 2), Some((1, 4, 7, false)));
        // XTGETTCAP tells the same name, in hex.
        let asking =
            shell("stty raw -echo; printf '\\033P+q544E\\033\\\\'; printf 'ready\\r\\n'; cat -v");
        let (mut fake, mut app, window) = configured(config, asking);
        run_until(&mut fake, &mut app, |_, app| {
            shows(app, window, "+r544E=7674323230")
        });
    }

    #[test]
    fn the_terminal_s_own_term_needs_its_entry() {
        let program =
            || shell("printf '[%s] [%s]\\n' \"$TERM\" \"$TERMINFO\"; echo ready; sleep 10");
        let config = || {
            let mut config = Config::parse(
                "[terminal]\nterm = \"xterm-lokyr\"\n",
                std::path::Path::new("config.toml"),
            )
            .unwrap();
            config.window = unpadded().window;
            config
        };
        for (terminfo, shown) in [
            (None, "[xterm-256color] []"),
            (
                Some("/tmp/lokyr-terminfo"),
                "[xterm-lokyr] [/tmp/lokyr-terminfo]",
            ),
        ] {
            let mut fake = Fake::default();
            let mut app = app(config(), program());
            app.environment.terminfo = terminfo.map(PathBuf::from);
            fake.start(&mut app);
            let window = fake.windows().next().unwrap();
            run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
            assert_eq!(row(&app, window, 0).trim(), shown);
        }
    }

    #[test]
    fn a_reloaded_config_s_settings_apply_to_the_text_to_come() {
        let (mut fake, mut app, window) = configured("", printer());
        let mut changed = app.config.clone();
        changed.terminal = Config::parse(
            "[terminal]\nterm = \"vt220\"\nunicode-widths = \"8\"\nambiguous-width = 2\nnormalization = \"nfc\"\n",
            std::path::Path::new("config.toml"),
        )
        .unwrap()
        .terminal;
        app.apply(&mut fake, changed);
        run_until(&mut fake, &mut app, |_, app| {
            placed(app, window, 2).is_some()
        });
        fake.send(&mut app, window, key());
        run_until(&mut fake, &mut app, |_, app| {
            placed(app, window, 3).is_some()
        });
        // The row before keeps its cells; the new one takes the settings,
        // and the program keeps its `TERM`.
        assert_eq!(placed(&app, window, 2), Some((2, 4, 6, true)));
        assert_eq!(placed(&app, window, 3), Some((1, 4, 7, false)));
        assert_eq!(row(&app, window, 0).trim(), "xterm-256color");
    }

    #[test]
    fn the_scrollback_keeps_within_its_limit_and_the_budget_follows_the_config() {
        let program = shell(
            "stty raw -echo; seq 1 20000; printf 'ready\\r\\n'; head -c 1 >/dev/null; seq 1 20000; printf 'done\\r\\n'; sleep 10",
        );
        let (mut fake, mut app, window) =
            configured("[scrollback]\nlimit = 2\nbudget = 50\n", program);
        assert_eq!(app.budget.limit(), 50 * 1024 * 1024);
        let kept = |fake: &mut Fake, app: &mut App| {
            app.pane(window).scroll(Scroll::Top);
            fake.send(app, window, WindowEvent::Frame);
            app.pane(window).snapshot().offset()
        };
        // Two MiB keep a thousand rows or so of these columns, short of the
        // 20,000.
        let two = kept(&mut fake, &mut app);
        assert!((500..4000).contains(&two), "{two} rows kept");
        // Smaller, for the output to come, and a larger budget.
        let changed = Config::parse(
            "[scrollback]\nlimit = 1\nbudget = 200\n",
            std::path::Path::new("config.toml"),
        )
        .unwrap();
        let mut config = app.config.clone();
        config.scrollback = changed.scrollback;
        app.apply(&mut fake, config);
        assert_eq!(app.budget.limit(), 200 * 1024 * 1024);
        fake.send(&mut app, window, key());
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "done"));
        let one = kept(&mut fake, &mut app);
        assert!(one < two * 3 / 4, "{one} rows kept, from {two}");
    }

    #[test]
    fn the_wheel_moves_the_view_through_the_scrollback() {
        let mut fake = Fake::default();
        let mut app = app(unpadded(), shell("seq 1 100; echo ready; sleep 10"));
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
        let offset = |app: &App| app.pane(window).snapshot().offset();
        fake.send(
            &mut app,
            window,
            wheel(ScrollDelta::Lines { x: 0.0, y: 3.0 }, Modifiers::NONE),
        );
        run_until(&mut fake, &mut app, |_, app| offset(app) == 3);
        // A trackpad's points, by rows of 8: a row and a half down, and the
        // half that makes a row with the next.
        let points = |y| wheel(ScrollDelta::Points { x: 0.0, y }, Modifiers::NONE);
        fake.send(&mut app, window, points(-12.0));
        run_until(&mut fake, &mut app, |_, app| offset(app) == 2);
        fake.send(&mut app, window, points(-4.0));
        run_until(&mut fake, &mut app, |_, app| offset(app) == 1);
        // Shift turns a mouse's wheel sideways, which still scrolls the view.
        fake.send(
            &mut app,
            window,
            wheel(ScrollDelta::Lines { x: 2.0, y: 0.0 }, SHIFT),
        );
        run_until(&mut fake, &mut app, |_, app| offset(app) == 3);
    }

    #[test]
    fn the_wheel_goes_to_a_program_that_takes_it_but_with_shift() {
        let (mut fake, mut app, window) = started_with(reader("\\033[?1000h\\033[?1006h"));
        let up = ScrollDelta::Lines { x: 0.0, y: 1.0 };
        fake.send(&mut app, window, wheel(up, Modifiers::NONE));
        run_until(&mut fake, &mut app, |_, app| {
            shows(app, window, "^[[<64;1;1M")
        });
        // With Shift, the view moves, without the program.
        fake.send(&mut app, window, wheel(up, SHIFT));
        let offset = |app: &App| app.pane(window).snapshot().offset();
        // The screen has no scrollback yet: the view stays, and the program
        // learns nothing more, a while after.
        let pause = Instant::now() + Duration::from_millis(300);
        run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
        assert_eq!(offset(&app), 0);
        assert_eq!(
            screen(&app, window).concat().matches("^[[<").count(),
            1,
            "{:?}",
            screen(&app, window)
        );
    }

    #[test]
    fn the_wheel_sends_the_cursor_keys_on_the_alternate_screen() {
        let (mut fake, mut app, window) = started_with(reader("\\033[?1049h"));
        let down = ScrollDelta::Lines { x: 0.0, y: -2.0 };
        fake.send(&mut app, window, wheel(down, Modifiers::NONE));
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "^[[B^[[B"));
    }

    #[test]
    fn moves_come_for_a_program_that_asked_for_them_once_a_cell() {
        let (mut fake, mut app, window) = started_with(reader("\\033[?1003h\\033[?1006h"));
        run_until(&mut fake, &mut app, |fake, _| {
            fake.window(window).unwrap().mouse_moves
        });
        for at in [cell(1, 0), Point { x: 6.0, y: 2.0 }, cell(3, 0)] {
            let event = WindowEvent::MouseMoved {
                position: at,
                mods: Modifiers::NONE,
            };
            fake.send(&mut app, window, event);
        }
        run_until(&mut fake, &mut app, |_, app| {
            shows(app, window, "^[[<35;4;1M")
        });
        // The second move, in the first's cell, reported nothing.
        let shown = screen(&app, window).concat();
        assert_eq!(shown.matches("^[[<35;").count(), 2, "{shown:?}");
        assert!(shown.contains("^[[<35;2;1M^[[<35;4;1M"), "{shown:?}");
    }

    #[test]
    fn the_config_s_scroll_on_output_reaches_the_panes() {
        let (mut fake, mut app, path) =
            with_file("output", "[scrollback]\nscroll-on-output = true\n");
        let window = fake.windows().next().unwrap();
        assert!(app.pane(window).scroll_on_output());
        std::fs::write(&path, "").unwrap();
        change(&mut fake, &mut app);
        assert!(!app.pane(window).scroll_on_output());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// The config of a window without padding.
    /// The config of no padding, of windows of 200 columns and 62 rows:
    /// 800 points by 496, of cells 4 points wide and 8 high.
    fn unpadded() -> Config {
        let mut config = Config::default();
        config.window.padding_x = 0.0.try_into().unwrap();
        config.window.padding_y = 0.0.try_into().unwrap();
        config.window.columns = 200.try_into().unwrap();
        config.window.rows = 62.try_into().unwrap();
        config
    }

    /// A size of a window's content, in points, at a scale of 2: cells of 8
    /// by 16 pixels make a column of 4 points and a row of 8.
    fn points(width: f64, height: f64) -> WindowSize {
        WindowSize::new(Size { width, height }, 2.0)
    }

    /// The text of every row of a window's screen.
    fn screen(app: &App, window: WindowId) -> Vec<String> {
        let rows = app.pane(window).snapshot().rows();
        (0..rows).map(|at| row(app, window, at)).collect()
    }

    #[test]
    fn the_grid_fits_the_window_less_its_padding() {
        let (mut fake, mut app, path) = with_file("padding", "[window]\npadding-x = 10\n");
        // 1600 by 1000 pixels, less 20 at the left and the right and 8 at
        // the top and the bottom.
        let grid = app.grid(points(800.0, 500.0));
        assert_eq!((grid.columns, grid.rows), (195, 61));
        // A window smaller than its padding keeps a cell.
        let grid = app.grid(points(15.0, 5.0));
        assert_eq!((grid.columns, grid.rows), (1, 1));
        // The input method's candidates go at the cursor's cell, within the
        // padding.
        let window = fake.windows().next().unwrap();
        fake.send(&mut app, window, WindowEvent::Frame);
        let (origin, _) = fake.window(window).unwrap().text_area.unwrap();
        assert_eq!((origin.x, origin.y), (10.0, 4.0));
        // A reloaded padding gives the grid that it leaves: of a window of
        // 100 columns and 30 rows with their padding, 420 points by 248.
        std::fs::write(&path, "[window]\npadding-x = 0\npadding-y = 0\n").unwrap();
        change(&mut fake, &mut app);
        assert!(fake.window(window).unwrap().frame_requested);
        fake.send(&mut app, window, WindowEvent::Frame);
        let snapshot = app.pane(window).snapshot();
        assert_eq!((snapshot.cols(), snapshot.rows()), (105, 31));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_translucent_background_makes_the_window_translucent() {
        let (mut fake, mut reloaded, path) = with_file("translucent", "");
        let window = fake.windows().next().unwrap();
        let backdrop = |fake: &mut Fake, app: &mut App| {
            fake.send(app, window, WindowEvent::Frame);
            fake.window(window)
                .unwrap()
                .translucent
                .map(|backdrop| (backdrop.color, backdrop.blur))
        };
        assert_eq!(backdrop(&mut fake, &mut reloaded), None);
        std::fs::write(
            &path,
            "[window]\nbackground-opacity = 0.9\nbackground-blur = 20\n",
        )
        .unwrap();
        change(&mut fake, &mut reloaded);
        // The terminal's default background, of the light variant, with the
        // opacity.
        let light = reloaded.colors(Appearance::Light).background;
        let rgb = |rgb: lokyr_vt_core::page::Rgb| {
            [rgb.r, rgb.g, rgb.b].map(|component| f64::from(f32::from(component) / 255.0))
        };
        let [red, green, blue] = rgb(light);
        let opacity = f64::from(0.9_f32);
        assert_eq!(
            backdrop(&mut fake, &mut reloaded),
            Some(([red, green, blue, opacity], 20.0))
        );
        // The dark variant's background takes its place.
        fake.send_app(&mut reloaded, AppEvent::AppearanceChanged(Appearance::Dark));
        let [red, green, blue] = rgb(reloaded.colors(Appearance::Dark).background);
        assert_eq!(
            backdrop(&mut fake, &mut reloaded),
            Some(([red, green, blue, opacity], 20.0))
        );
        // A blur alone, of an opaque background, leaves the window opaque.
        std::fs::write(&path, "[window]\nbackground-blur = 20\n").unwrap();
        change(&mut fake, &mut reloaded);
        assert_eq!(backdrop(&mut fake, &mut reloaded), None);
        std::fs::write(&path, "[window]\nbackground-opacity = 0.5\n").unwrap();
        change(&mut fake, &mut reloaded);
        let blur = backdrop(&mut fake, &mut reloaded).map(|(_, blur)| blur);
        assert_eq!(blur, Some(0.0));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        // A window opens as the config says.
        let mut config = Config::default();
        config.window.background_opacity = 0.8.try_into().unwrap();
        config.window.background_blur = 12.0.try_into().unwrap();
        let mut fake = Fake::default();
        let mut app = app(config, quiet());
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        let opened = fake.window(window).unwrap().translucent;
        assert_eq!(opened.map(|backdrop| backdrop.blur), Some(12.0));
    }

    #[test]
    fn a_resize_storm_tells_the_program_its_size_at_its_pace() {
        let mut fake = Fake::default();
        // The program prints its size at each SIGWINCH.
        let script = "trap 'stty size' WINCH; echo ready; while :; do sleep 0.02; done";
        let mut app = app(unpadded(), shell(script));
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        run_until(&mut fake, &mut app, |_, app| row(app, window, 0) == "ready");
        // A live resize's steps, as fast as they come.
        let start = Instant::now();
        let mut told = Vec::new();
        let grids: Vec<Grid> = (0..50)
            .map(|step| {
                let size = points(400.0 + f64::from(step) * 4.0, 300.0);
                assert!(fake.send(&mut app, window, WindowEvent::Resized(size)));
                told.push(app.pane(window).told());
                app.grid(size)
            })
            .collect();
        let elapsed = start.elapsed();
        let last = grids[49];
        assert_eq!((last.columns, last.rows), (149, 37));
        // The terminal takes each step at once, and the program the first,
        // then one every 50 ms at most.
        let pane = app.pane(window);
        assert_eq!(pane.snapshot().cols(), last.columns);
        assert_eq!(told[0], grids[0]);
        told.dedup();
        let paced = elapsed.as_millis() / RESIZE_PACE.as_millis() + 1;
        assert!(
            told.len() as u128 <= paced,
            "{} sizes in {elapsed:?}",
            told.len()
        );
        // Then the last step, once, as its timer comes: the timer of a size
        // told since it started comes first, and starts it again.
        for _ in 0..2 {
            let Some((timer, at)) = app.resize_timer else {
                break;
            };
            // The label of the grid's size has its own, a second later.
            let timers: Vec<_> = fake.timers().collect();
            assert!(timers.contains(&(timer, at)), "{timers:?}");
            assert_eq!(fake.fire_timers(&mut app, at), 1);
        }
        assert_eq!(app.resize_timer, None);
        assert_eq!(app.pane(window).told(), last);
        // The program saw few sizes, and the last one.
        run_until(&mut fake, &mut app, |_, app| {
            screen(app, window).iter().any(|line| line == "37 149")
        });
        let sizes: Vec<String> = screen(&app, window)
            .into_iter()
            .skip(1)
            .filter(|line| !line.is_empty())
            .collect();
        assert!(sizes.len() <= told.len() + 1, "{sizes:?}");
        assert_eq!(sizes.last().map(String::as_str), Some("37 149"));
    }

    #[test]
    fn a_resize_shows_the_grid_s_size_until_a_moment_after_it_ends() {
        let (mut fake, mut app) = started();
        let window = fake.windows().next().unwrap();
        fake.send(&mut app, window, WindowEvent::Frame);
        let label = |app: &App| app.windows[&window].size_label;
        // The first frame shows none.
        assert_eq!(label(&app), None);
        // A live resize that changes the grid shows it, while it lasts.
        fake.send(&mut app, window, WindowEvent::LiveResize(true));
        assert!(app.windows[&window].live_resize);
        fake.send(&mut app, window, WindowEvent::Resized(points(400.0, 300.0)));
        let until = label(&app).expect("the label");
        let (timer, at) = app.label_timer.expect("its timer");
        assert_eq!(at, until);
        // Its timer, within the live resize, leaves it.
        fake.send_app(&mut app, AppEvent::Timer(timer));
        assert_eq!(label(&app), Some(until));
        // Its end keeps it a moment more, then takes it away, with a frame.
        let ended = Instant::now();
        fake.send(&mut app, window, WindowEvent::LiveResize(false));
        let until = label(&app).expect("the label");
        assert!(until >= ended + size_label::HOLD);
        let (_, at) = app.label_timer.expect("its timer");
        fake.send(&mut app, window, WindowEvent::Frame);
        assert!(!fake.window(window).unwrap().frame_requested);
        // The cursor's blinking has a timer of its own before it.
        assert!(fake.fire_timers(&mut app, at) >= 1);
        assert_eq!(label(&app), None);
        assert_eq!(app.label_timer, None);
        assert!(fake.window(window).unwrap().frame_requested);
        // A resize of the same grid shows none.
        fake.send(&mut app, window, WindowEvent::Resized(points(401.0, 301.0)));
        assert_eq!(label(&app), None);
    }

    #[test]
    fn a_live_resize_draws_once_a_refresh() {
        let (mut fake, mut app) = started();
        let window = fake.windows().next().unwrap();
        fake.send(&mut app, window, WindowEvent::LiveResize(true));
        // A frame drawn within this refresh: the step waits for the next.
        app.windows.get_mut(&window).unwrap().drawn_at = Some(Instant::now());
        let before = app.pane(window).grid();
        fake.send(&mut app, window, WindowEvent::Frame);
        assert!(!fake.window(window).unwrap().frame_requested);
        fake.send(&mut app, window, WindowEvent::Resized(points(400.0, 300.0)));
        assert_eq!(app.pane(window).grid(), before);
        assert!(fake.window(window).unwrap().frame_requested);
        // A refresh later, it draws at once.
        let earlier = Instant::now() - Duration::from_millis(50);
        app.windows.get_mut(&window).unwrap().drawn_at = Some(earlier);
        fake.send(&mut app, window, WindowEvent::Resized(points(400.0, 300.0)));
        assert_ne!(app.pane(window).grid(), before);
    }

    #[test]
    fn the_screen_and_its_scrollback_reflow_with_the_window() {
        let mut fake = Fake::default();
        let script = "seq 1 3000; printf '%150s' '' | tr ' ' x; sleep 10";
        let mut app = app(unpadded(), shell(script));
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        let line = "x".repeat(150);
        run_until(&mut fake, &mut app, |_, app| row(app, window, 61) == line);
        let before = screen(&app, window);
        assert_eq!(before[60], "3000");
        // Half as wide, the line takes two rows, and the screen keeps its
        // height.
        fake.send(&mut app, window, WindowEvent::Resized(points(400.0, 500.0)));
        let narrow = screen(&app, window);
        assert_eq!(narrow.len(), 62);
        assert_eq!(narrow[59..], ["3000", &line[..100], &line[100..]]);
        // Through other sizes, taller and shorter too, and back, the screen
        // is as it was: no row lost, none doubled.
        for (width, height) in [
            (52.0, 500.0),
            (244.0, 200.0),
            (1000.0, 900.0),
            (148.0, 500.0),
        ] {
            fake.send(
                &mut app,
                window,
                WindowEvent::Resized(points(width, height)),
            );
        }
        fake.send(&mut app, window, WindowEvent::Resized(points(800.0, 500.0)));
        assert_eq!(screen(&app, window), before);
    }

    #[test]
    fn a_synchronized_update_shows_whole() {
        let mut fake = Fake::default();
        let script = "printf 'before\\r\\n'; sleep 0.3; printf '\\033[?2026hhalf'; sleep 0.5; \
                      printf ' done\\033[?2026l'; sleep 10";
        let mut app = app(Config::default(), shell(script));
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        let mut seen = Vec::new();
        run_until(&mut fake, &mut app, |_, app| {
            let shown = row(app, window, 1);
            if seen.last() != Some(&shown) {
                seen.push(shown.clone());
            }
            shown == "half done"
        });
        assert!(!seen.iter().any(|shown| shown == "half"), "{seen:?}");
        assert_eq!(row(&app, window, 0), "before");
    }

    /// The app with `close-on-exit` as `close`, whose window runs `script`,
    /// once its program has ended and the window has done what `close`
    /// says.
    fn ended(close: CloseOnExit, script: &str) -> (Fake, App, WindowId) {
        let mut config = Config::default();
        config.window.close_on_exit = close;
        let mut fake = Fake::default();
        let mut app = app(config, shell(script));
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        run_until(&mut fake, &mut app, |fake, app| {
            fake.windows().count() == 0 || app.pane(window).is_kept()
        });
        (fake, app, window)
    }

    fn key() -> WindowEvent {
        typed(lokyr_platform_core::KeyCode::KeyA, "a", false)
    }

    /// A key, of `code`, that types `text`, with Command held or not.
    fn typed(code: lokyr_platform_core::KeyCode, text: &str, command: bool) -> WindowEvent {
        WindowEvent::Key(lokyr_platform_core::KeyEvent {
            code,
            state: KeyState::Pressed,
            mods: lokyr_platform_core::Modifiers {
                super_key: command,
                ..lokyr_platform_core::Modifiers::default()
            },
            text: text.to_owned(),
            unmodified: text.to_owned(),
            plain: text.to_owned(),
        })
    }

    /// The press, or the release, of the key at `code`, which types `plain`
    /// without modifiers, with `mods`.
    fn key_event(code: KeyCode, plain: &str, mods: Modifiers, pressed: bool) -> WindowEvent {
        let control = mods.control && plain.len() == 1;
        let text = if control {
            // Control types the letter's control code.
            let byte = plain.as_bytes()[0].to_ascii_uppercase() & 0x1f;
            char::from(byte).to_string()
        } else {
            plain.to_owned()
        };
        WindowEvent::Key(KeyEvent {
            code,
            state: if pressed {
                KeyState::Pressed
            } else {
                KeyState::Released
            },
            mods,
            text,
            unmodified: plain.to_owned(),
            plain: plain.to_owned(),
        })
    }

    const CMD: Modifiers = Modifiers {
        super_key: true,
        ..Modifiers::NONE
    };

    const CTRL: Modifiers = Modifiers {
        control: true,
        ..Modifiers::NONE
    };

    /// The app without padding, of a config whose `[keys]` are `keys`, its
    /// window running `program`, once it shows `ready`.
    fn bound(keys: &str, program: Program) -> (Fake, App, WindowId) {
        let mut config = Config::parse(
            &format!("[keys]\n{keys}"),
            std::path::Path::new("config.toml"),
        )
        .unwrap();
        config.window = unpadded().window;
        let mut fake = Fake::default();
        let mut app = app(config, program);
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
        (fake, app, window)
    }

    #[test]
    fn a_bound_key_runs_its_command_and_its_release_goes_nowhere() {
        let keys = "\"cmd+shift+n\" = \"window.new\"\n";
        let (mut fake, mut app, window) = bound(keys, reader("\\033[>3u"));
        let shifted = Modifiers { shift: true, ..CMD };
        fake.send(
            &mut app,
            window,
            key_event(KeyCode::KeyN, "n", shifted, true),
        );
        assert_eq!(fake.windows().count(), 2);
        // The release reaches nothing, though the program asked for
        // releases, as the kitty protocol's flags have it.
        fake.send(
            &mut app,
            window,
            key_event(KeyCode::KeyN, "n", shifted, false),
        );
        // The default too.
        fake.send(&mut app, window, key_event(KeyCode::KeyN, "n", CMD, true));
        assert_eq!(fake.windows().count(), 3);
        let pause = Instant::now() + Duration::from_millis(300);
        run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
        let shown = screen(&app, window).concat();
        assert!(!shown.contains("^["), "{shown:?}");
    }

    #[test]
    fn a_binding_of_the_config_sends_its_text_as_it_is() {
        // A shell's line editing: the word before, and the line's start.
        let keys = concat!(
            "\"alt+backspace\" = { command = \"pane.send-text\", args = { text = \"\\u0017\" } }\n",
            "\"cmd+left\" = { command = \"pane.send-text\", args = { text = \"\\u0001\" } }\n",
        );
        // More lines than the window's rows, for a scrollback.
        let program = shell("seq 1 200; echo ready; stty raw -echo; cat -v");
        let (mut fake, mut app, window) = bound(keys, program);
        fake.send(&mut app, window, WindowEvent::Focused(true));
        // A view up the scrollback comes back down, as typing brings it.
        let offset = |app: &App| app.pane(window).snapshot().offset();
        fake.send(&mut app, window, key_event(KeyCode::ArrowUp, "", CMD, true));
        run_until(&mut fake, &mut app, |_, app| offset(app) == 1);
        let alt = Modifiers {
            left_alt: true,
            ..Modifiers::NONE
        };
        fake.send(
            &mut app,
            window,
            key_event(KeyCode::Backspace, "", alt, true),
        );
        fake.send(
            &mut app,
            window,
            key_event(KeyCode::ArrowLeft, "", CMD, true),
        );
        run_until(&mut fake, &mut app, |_, app| {
            offset(app) == 0 && shows(app, window, "^W^A")
        });
        // Its text is required.
        let error = app
            .commands
            .check("pane.send-text", Args::new())
            .unwrap_err();
        assert_eq!(error.code, Code::MissingArgument);
    }

    #[test]
    fn a_sequence_holds_its_keys_back_until_it_runs_or_gives_them_to_the_program() {
        let keys = "\"ctrl+a>n\" = \"window.new\"\n";
        let (mut fake, mut app, window) = bound(keys, reader(""));
        fake.send(&mut app, window, key_event(KeyCode::KeyA, "a", CTRL, true));
        fake.send(&mut app, window, key_event(KeyCode::KeyA, "a", CTRL, false));
        fake.send(
            &mut app,
            window,
            key_event(KeyCode::KeyN, "n", Modifiers::NONE, true),
        );
        assert_eq!(fake.windows().count(), 2);
        // A key that goes on with no sequence: both reach the program.
        fake.send(&mut app, window, key_event(KeyCode::KeyA, "a", CTRL, true));
        fake.send(
            &mut app,
            window,
            key_event(KeyCode::KeyE, "e", Modifiers::NONE, true),
        );
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "^Ae"));
        let shown = screen(&app, window).concat();
        assert_eq!(shown.matches("^A").count(), 1, "{shown:?}");
        assert!(!shown.contains('n'), "{shown:?}");
    }

    /// A plugin of the tests: `test.copy`, on Control-T, puts the first
    /// line of the focused pane's text on the clipboard, and opens a
    /// window through `window.new`; `test.boom`, on Control-B, panics;
    /// `test.size` gives the text 18 points; `test.close` closes the
    /// focused window; and `test.again` runs itself, counting its runs.
    struct Tester {
        runs: std::rc::Rc<std::cell::Cell<usize>>,
    }

    impl plugins::Plugin for Tester {
        fn activate(&mut self, cx: &mut plugins::Cx<'_>) -> Result<(), Error> {
            let names = [
                "test.copy",
                "test.boom",
                "test.size",
                "test.close",
                "test.again",
                "test.bind",
            ];
            for name in names {
                cx.register_command(Command {
                    name: name.to_owned(),
                    title: name.to_owned(),
                    category: Category::View,
                    capabilities: Vec::new(),
                    params: Vec::new(),
                    when: None,
                })?;
            }
            for (keys, command) in [("ctrl+t", "test.copy"), ("ctrl+b", "test.boom")] {
                cx.bind(plugins::host::Binding {
                    keys: keys.to_owned(),
                    command: command.to_owned(),
                    args: Args::new(),
                    when: Some("pane.focused".to_owned()),
                })?;
            }
            Ok(())
        }

        fn on_command(
            &mut self,
            cx: &mut plugins::Cx<'_>,
            name: &str,
            _args: Args,
        ) -> Result<(), Error> {
            match name {
                "test.boom" => panic!("on purpose"),
                "test.size" => cx.set_font_size(Some(18.0)),
                "test.close" => {
                    let window = cx.focused_window().expect("a window");
                    cx.close_window(window)
                }
                "test.again" => {
                    self.runs.set(self.runs.get() + 1);
                    cx.run_command("test.again", Args::new())
                }
                // A binding that takes a key of the app's own.
                "test.bind" => cx.bind(plugins::host::Binding {
                    keys: "cmd+n".to_owned(),
                    command: "test.size".to_owned(),
                    args: Args::new(),
                    when: None,
                }),
                _ => {
                    let pane = cx.focused_pane().expect("a focused pane");
                    cx.select_all(pane)?;
                    let text = cx.selection_text(pane)?.unwrap_or_default();
                    cx.set_clipboard_text(text.lines().next().unwrap_or_default())?;
                    cx.run_command(NEW_WINDOW, Args::new())
                }
            }
        }
    }

    /// The app with the tests' plugin, whose runs of `test.again` the
    /// counter counts, its window running `reader`, focused, once it shows
    /// `ready`.
    fn with_tester() -> (Fake, App, WindowId, std::rc::Rc<std::cell::Cell<usize>>) {
        let runs = std::rc::Rc::default();
        let tester = Tester {
            runs: std::rc::Rc::clone(&runs),
        };
        let mut fake = Fake::default();
        let mut app = app(unpadded(), reader("")).with_plugin("tester", Box::new(tester));
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        fake.send(&mut app, window, WindowEvent::Focused(true));
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
        (fake, app, window, runs)
    }

    #[test]
    fn a_plugin_s_binding_runs_its_command_and_what_it_asks() {
        let (mut fake, mut app, window, _) = with_tester();
        // The plugins started with the app, the built-in one too.
        assert_eq!(
            app.plugins.status("core-actions"),
            Some(plugins::Status::Active)
        );
        assert!(app.commands.get("test.copy").is_some());
        fake.send(&mut app, window, key_event(KeyCode::KeyT, "t", CTRL, true));
        assert_eq!(fake.clipboard_text().as_deref(), Some("ready"));
        // The window that its command asked for, once the call returned.
        assert_eq!(fake.windows().count(), 2);
        // The key went nowhere else.
        let pause = Instant::now() + Duration::from_millis(300);
        run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
        assert!(!shows(&app, window, "^T"));
        // Another language makes the registry anew, with the plugins'
        // commands.
        let mut config = app.config.clone();
        config.app.language = "it".to_owned();
        app.apply(&mut fake, config);
        assert!(app.commands.get("test.copy").is_some());
        fake.send(&mut app, window, key_event(KeyCode::KeyT, "t", CTRL, true));
        assert_eq!(fake.windows().count(), 3);
    }

    #[test]
    fn a_plugin_that_panics_is_disabled_and_its_keys_go_to_the_program() {
        let (mut fake, mut app, window, _) = with_tester();
        fake.send(&mut app, window, key_event(KeyCode::KeyB, "b", CTRL, true));
        assert_eq!(
            app.plugins.status("tester"),
            Some(plugins::Status::Disabled)
        );
        assert!(app.commands.get("test.copy").is_none());
        // Its bindings went with it: its keys are the program's, and the
        // app goes on.
        fake.send(&mut app, window, key_event(KeyCode::KeyT, "t", CTRL, true));
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "^T"));
        assert_eq!(fake.clipboard_text(), None);
        assert_eq!(fake.windows().count(), 1);
    }

    #[test]
    fn a_plugin_s_requests_change_the_app_after_its_call() {
        let (mut fake, mut app, window, runs) = with_tester();
        app.run(&mut fake, "test.size", Args::new());
        assert_eq!(app.font_size, Some(18.0));
        assert_eq!(sized(&app.config.font, app.font_size).size.points(), 18.0);
        assert_eq!(sized(&app.config.font, None).size.points(), 13.0);
        // The focused window closes; the other stays.
        app.run(&mut fake, NEW_WINDOW, Args::new());
        assert_eq!(fake.windows().count(), 2);
        app.run(&mut fake, "test.close", Args::new());
        // Its program, `cat`, runs: the question, answered.
        let close = fake.alerts().last().unwrap().id;
        fake.answer_alert(&mut app, close, 0);
        assert_eq!(fake.windows().collect::<Vec<_>>().len(), 1);
        assert!(!fake.windows().any(|shown| shown == window));
        // A command that runs itself stops, its first run and as many
        // again as the depth allows.
        app.run(&mut fake, "test.again", Args::new());
        assert_eq!(runs.get(), COMMANDS_DEEP + 1);
    }

    /// The press of Command and the key of `letter`.
    fn command_key(code: KeyCode, letter: &str) -> WindowEvent {
        key_event(code, letter, CMD, true)
    }

    /// The app without padding, its window running `program`, focused, once
    /// it shows `ready`.
    fn focused_with(program: Program) -> (Fake, App, WindowId) {
        let (mut fake, mut app, window) = started_with(program);
        fake.send(&mut app, window, WindowEvent::Focused(true));
        (fake, app, window)
    }

    #[test]
    fn a_cmd_click_opens_the_link_under_it_rather_than_selecting() {
        let (mut fake, mut app, window, _) = pasting(
            "click",
            "[links]\nurls = \"open\"\n",
            shell(concat!(
                "stty raw -echo; cd /usr; printf 'https://lokyr.dev/x bin/\\r\\n",
                "\\033]8;;https://lokyr.dev/osc\\033\\\\here\\033]8;;\\033\\\\\\r\\n'; ",
                "echo ready; cat -v"
            )),
        );
        let cmd_click = |fake: &mut Fake, app: &mut App, col, row| {
            fake.send(app, window, click(true, cell(col, row), CMD, 1));
            fake.send(app, window, click(false, cell(col, row), CMD, 1));
        };
        // A URL, a path from the program's directory, an OSC 8 link, and
        // nothing on a space.
        for (col, row) in [(2, 0), (21, 0), (1, 1), (10, 2)] {
            cmd_click(&mut fake, &mut app, col, row);
        }
        assert_eq!(
            fake.opened(),
            ["https://lokyr.dev/x", "/usr/bin", "https://lokyr.dev/osc"]
        );
        assert_eq!(selected_text(&app, window), None);
        assert!(app.windows[&window].selecting.is_none());
        // A window without the focus takes the click as a click.
        fake.send(&mut app, window, WindowEvent::Focused(false));
        cmd_click(&mut fake, &mut app, 2, 0);
        assert_eq!(fake.opened().len(), 3);
        assert!(app.windows[&window].selecting.is_some());
    }

    #[test]
    fn the_context_menu_opens_the_link_at_its_cell() {
        let (mut fake, mut app, window, _) = pasting(
            "context",
            "[links]\nurls = \"open\"\n",
            shell(concat!(
                "stty raw -echo; printf 'see https://lokyr.dev/x here\\r\\n'; ",
                "echo ready; cat -v"
            )),
        );
        fake.send(&mut app, window, WindowEvent::Focused(false));
        // On the URL, which names no man page.
        right_click(&mut fake, &mut app, window, cell(10, 0), Modifiers::NONE);
        assert_eq!(context_menu(&fake, window).2, [MAN_PAGE]);
        right_click(&mut fake, &mut app, window, cell(1, 0), Modifiers::NONE);
        assert_eq!(context_menu(&fake, window).2, ["links.open"]);
        // The item takes the cell that the menu opened on.
        right_click(&mut fake, &mut app, window, cell(10, 0), Modifiers::NONE);
        let chosen = WindowEvent::ContextCommand("links.open".to_owned());
        fake.send(&mut app, window, chosen);
        assert_eq!(fake.opened(), ["https://lokyr.dev/x"]);
    }

    #[test]
    fn the_context_menu_s_link_is_its_text_s_where_output_moved_it() {
        let (mut fake, mut app, window, _) = pasting(
            "context-moved",
            "[links]\nurls = \"ask\"\n",
            shell(concat!(
                "stty -echo; printf 'see https://lokyr.dev/x here\\n'; echo ready; ",
                "read line; seq 1 70; echo https://lokyr.dev/y; echo done; read line; ",
                "yes https://lokyr.dev/z | head -100; echo https://lokyr.dev/z/end; read line"
            )),
        );
        let chosen = || WindowEvent::ContextCommand("links.open".to_owned());
        // The program's last line written whole, its newline too, which a
        // read of the terminal can bring after the line.
        let ended = |app: &App, last: &str| {
            let at = app.pane(window).snapshot().cursor().position;
            at.x == 0 && at.y > 0 && row(app, window, at.y - 1) == last
        };
        fake.send(&mut app, window, WindowEvent::Focused(false));
        right_click(&mut fake, &mut app, window, cell(10, 0), Modifiers::NONE);
        assert!(
            !context_menu(&fake, window)
                .2
                .contains(&"links.open".to_owned())
        );
        // Output scrolls the URL out of the view while the menu is open.
        fake.send(&mut app, window, typed(KeyCode::Enter, "\r", false));
        run_until(&mut fake, &mut app, |_, app| ended(app, "done"));
        assert_ne!(row(&app, window, 0), "see https://lokyr.dev/x here");
        fake.send(&mut app, window, chosen());
        // The question is the menu's window's, which has no focus.
        let [ask] = fake.alerts() else {
            panic!("one question: {:?}", fake.alerts());
        };
        assert_eq!(
            (ask.window, ask.alert.detail.as_str()),
            (Some(window), "https://lokyr.dev/x")
        );
        fake.answer_alert_with(&mut app, ask.id, 1, false);
        // The menu's item runs once.
        fake.send(&mut app, window, chosen());
        assert!(fake.alerts().is_empty());
        // Its text scrolled into the scrollback, which a clear takes away,
        // it runs nothing, though links came to every row around its own,
        // which a resize that a pane puts off can move by one.
        let rows = app.pane(window).grid().rows;
        let y = (0..rows)
            .find(|&r| row(&app, window, r) == "https://lokyr.dev/y")
            .unwrap();
        right_click(&mut fake, &mut app, window, cell(10, y), Modifiers::NONE);
        fake.send(&mut app, window, typed(KeyCode::Enter, "\r", false));
        run_until(&mut fake, &mut app, |_, app| {
            ended(app, "https://lokyr.dev/z/end")
        });
        app.pane(window).clear_scrollback();
        let around: Vec<String> = (y.saturating_sub(2)..rows)
            .map(|r| row(&app, window, r))
            .collect();
        assert!(
            row(&app, window, y).starts_with("https://lokyr.dev/z"),
            "row {y} of {rows}, the cursor at {:?}: {around:?}",
            app.pane(window).snapshot().cursor().position
        );
        fake.send(&mut app, window, chosen());
        assert!(fake.alerts().is_empty() && fake.opened().is_empty());
    }

    /// Opens with Command-click what `window` shows at `col` of its first
    /// row.
    fn cmd_click(fake: &mut Fake, app: &mut App, window: WindowId, col: u16) {
        fake.send(app, window, click(true, cell(col, 0), CMD, 1));
        fake.send(app, window, click(false, cell(col, 0), CMD, 1));
    }

    /// A program that shows a URL at column 0, a file at 20, a program at
    /// 31 and a directory at 45.
    fn links() -> Program {
        shell(concat!(
            "stty raw -echo; ",
            "printf 'https://lokyr.dev/x /etc/hosts /usr/bin/true /usr\\r\\n'; ",
            "echo ready; cat -v"
        ))
    }

    #[test]
    fn a_link_asks_first_and_the_config_keeps_the_answer() {
        let (mut fake, mut app, window, path) = pasting("links", "# mine\n", links());
        let shown = |fake: &Fake| {
            let open = &fake.alerts()[0];
            assert_eq!(open.window, Some(window));
            (
                open.alert.message.clone(),
                open.alert.detail.clone(),
                open.alert.buttons.clone(),
                open.alert.suppression.clone().unwrap(),
            )
        };
        // A URL: Open or Cancel.
        cmd_click(&mut fake, &mut app, window, 2);
        let url = (
            "Open this link?".to_owned(),
            "https://lokyr.dev/x".to_owned(),
            vec!["Open".to_owned(), "Cancel".to_owned()],
            "Don't ask again for links".to_owned(),
        );
        assert_eq!(shown(&fake), url);
        // Ticked, but cancelled: nothing changes.
        fake.answer_alert_with(&mut app, fake.alerts()[0].id, 1, true);
        // A file: Open, Show in Finder, or Cancel.
        cmd_click(&mut fake, &mut app, window, 22);
        let file = (
            "Open this file?".to_owned(),
            "/etc/hosts".to_owned(),
            vec![
                "Open".to_owned(),
                "Show in Finder".to_owned(),
                "Cancel".to_owned(),
            ],
            "Don't ask again for files".to_owned(),
        );
        assert_eq!(shown(&fake), file);
        fake.answer_alert(&mut app, fake.alerts()[0].id, 1);
        // A program shows in the Finder, and a directory opens, without a
        // question.
        cmd_click(&mut fake, &mut app, window, 33);
        cmd_click(&mut fake, &mut app, window, 46);
        assert!(fake.alerts().is_empty());
        assert_eq!(fake.opened(), ["/usr"]);
        assert_eq!(fake.revealed(), ["/etc/hosts", "/usr/bin/true"]);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# mine\n");
        // Ticked, the answer goes into the config, and the next one opens
        // without a question, before the file's reload too.
        cmd_click(&mut fake, &mut app, window, 2);
        fake.answer_alert_with(&mut app, fake.alerts()[0].id, 0, true);
        cmd_click(&mut fake, &mut app, window, 22);
        fake.answer_alert_with(&mut app, fake.alerts()[0].id, 1, true);
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.starts_with("# mine\n"), "{written}");
        let links = Config::read(&path).unwrap().links;
        assert_eq!((links.urls, links.files), (OpenUrl::Open, OpenFile::Reveal));
        cmd_click(&mut fake, &mut app, window, 2);
        cmd_click(&mut fake, &mut app, window, 22);
        assert!(fake.alerts().is_empty());
        assert_eq!(
            fake.opened()[1..],
            ["https://lokyr.dev/x", "https://lokyr.dev/x"]
        );
        assert_eq!(fake.revealed()[2..], ["/etc/hosts", "/etc/hosts"]);
        // Opening a file, ticked, keeps `open`.
        std::fs::write(&path, "").unwrap();
        app.apply(&mut fake, Config::default());
        cmd_click(&mut fake, &mut app, window, 22);
        fake.answer_alert_with(&mut app, fake.alerts()[0].id, 0, true);
        assert_eq!(fake.opened().last().map(String::as_str), Some("/etc/hosts"));
        assert_eq!(Config::read(&path).unwrap().links.files, OpenFile::Open);
        assert!(app.links_chosen == (None, None));
    }

    #[test]
    fn a_window_that_closes_drops_its_link_s_question() {
        let (mut fake, mut app, window, _) = pasting("link-close", "", links());
        cmd_click(&mut fake, &mut app, window, 2);
        assert_eq!(app.open_asks.len(), 1);
        app.run(&mut fake, NEW_WINDOW, Args::new());
        fake.send(&mut app, window, WindowEvent::CloseRequested);
        let close = fake.alerts().last().unwrap().id;
        fake.answer_alert(&mut app, close, 0);
        assert!(app.open_asks.is_empty());
    }

    #[test]
    fn a_link_s_question_shows_its_invisible_characters_escaped() {
        assert_eq!(
            shown("https://lokyr.dev/\u{202E}gpj.exe\u{200B}x\u{7}é"),
            "https://lokyr.dev/\\u{202E}gpj.exe\\u{200B}x\\u{7}é"
        );
        assert_eq!(shown("/Users/x/日本 文書.txt"), "/Users/x/日本 文書.txt");
        for c in [
            '\u{2066}',
            '\u{2069}',
            '\u{2028}',
            '\u{FEFF}',
            '\u{E0041}',
            '\u{FE0F}',
        ] {
            assert!(invisible(c), "{c:?}");
        }
        for c in ['a', ' ', '/', 'é', '日', '😀'] {
            assert!(!invisible(c), "{c:?}");
        }
    }

    #[test]
    fn a_link_s_answer_lasts_until_the_app_quits_where_the_config_cannot_keep_it() {
        let text = "[font]\nsise = 14\n";
        let (mut fake, mut app, window, path) = pasting("bad-links", text, links());
        cmd_click(&mut fake, &mut app, window, 2);
        fake.answer_alert_with(&mut app, fake.alerts()[0].id, 0, true);
        cmd_click(&mut fake, &mut app, window, 22);
        fake.answer_alert_with(&mut app, fake.alerts()[0].id, 0, true);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        assert_eq!(
            app.links_chosen,
            (Some(OpenUrl::Open), Some(OpenFile::Open))
        );
        // A reload of the file does not bring the questions back.
        app.apply(&mut fake, Config::default());
        cmd_click(&mut fake, &mut app, window, 2);
        cmd_click(&mut fake, &mut app, window, 22);
        assert!(fake.alerts().is_empty());
        assert_eq!(fake.opened().len(), 4);
    }

    #[test]
    fn a_click_runs_the_command_that_the_config_binds_it_to() {
        let mut config = Config::parse(
            "[keys]\n\"alt+click\" = \"window.new\"\n",
            std::path::Path::new("config.toml"),
        )
        .unwrap();
        config.window = unpadded().window;
        let mut fake = Fake::default();
        let mut app = app(config, shell("echo ready; sleep 10"));
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        fake.send(&mut app, window, WindowEvent::Focused(true));
        let alt = Modifiers {
            left_alt: true,
            ..Modifiers::NONE
        };
        // Without the arguments that it does not take; a double click is
        // not one.
        fake.send(&mut app, window, click(true, cell(1, 0), alt, 1));
        assert_eq!(fake.windows().count(), 2);
        fake.send(&mut app, window, click(false, cell(1, 0), alt, 1));
        fake.send(&mut app, window, click(true, cell(1, 0), alt, 2));
        fake.send(&mut app, window, click(false, cell(1, 0), alt, 2));
        assert_eq!(fake.windows().count(), 2);
    }

    /// The app without padding, of the config file `text`, its window
    /// running `program`, focused, once it shows `ready`; a file that does
    /// not read gives the defaults.
    fn pasting(test: &str, text: &str, program: Program) -> (Fake, App, WindowId, PathBuf) {
        let directory =
            std::env::temp_dir().join(format!("lokyr-paste-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("config.toml");
        std::fs::write(&path, text).unwrap();
        let mut config = Config::read(&path).unwrap_or_default();
        config.window = unpadded().window;
        let mut fake = Fake::default();
        let mut app = App::new(None, config, Some(path.clone())).with_program(program);
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
        fake.send(&mut app, window, WindowEvent::Focused(true));
        (fake, app, window, path)
    }

    /// Pastes `text` with Command-V into `window`.
    fn paste_key(fake: &mut Fake, app: &mut App, window: WindowId, text: &str) {
        fake.set_clipboard_text(text);
        fake.send(app, window, command_key(KeyCode::KeyV, "v"));
    }

    #[test]
    fn a_drop_pastes_its_files_paths_or_its_text() {
        let (mut fake, mut app, window) = focused_with(reader(""));
        let paths = ["/tmp/a file.txt", "/usr/bin"].map(PathBuf::from).to_vec();
        fake.send(
            &mut app,
            window,
            WindowEvent::Dropped(Dropped::Files(paths)),
        );
        run_until(&mut fake, &mut app, |_, app| {
            shows(app, window, "'/tmp/a file.txt' /usr/bin")
        });
        // Text pastes as a paste does: several lines ask first.
        fake.send(
            &mut app,
            window,
            WindowEvent::Dropped(Dropped::Text("x".to_owned())),
        );
        run_until(&mut fake, &mut app, |_, app| {
            shows(app, window, "/usr/bin x")
        });
        assert!(fake.alerts().is_empty());
        let lines = Dropped::Text("a\nb\n".to_owned());
        fake.send(&mut app, window, WindowEvent::Dropped(lines));
        assert_eq!(fake.alerts().len(), 1);
    }

    #[test]
    fn copy_and_paste_go_through_the_clipboard_and_several_lines_ask() {
        let (mut fake, mut app, window) = focused_with(reader(""));
        // The pane's text, to its cursor's row.
        fake.send(&mut app, window, command_key(KeyCode::KeyA, "a"));
        fake.send(&mut app, window, command_key(KeyCode::KeyC, "c"));
        assert_eq!(fake.clipboard_text().as_deref(), Some("ready\n"));
        // A line pastes at once, and a line with its line break without it.
        paste_key(&mut fake, &mut app, window, "hello");
        paste_key(&mut fake, &mut app, window, "ls\n");
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "hellols"));
        assert!(fake.alerts().is_empty());
        // Several lines ask first, once at a time: cancelled, pasted, and
        // pasted as one line.
        for button in [2, 0, 1] {
            paste_key(&mut fake, &mut app, window, "a\nb\n");
            paste_key(&mut fake, &mut app, window, "a\nb\n");
            assert_eq!(fake.alerts().len(), 1);
            let open = &fake.alerts()[0];
            assert_eq!(open.window, Some(window));
            assert_eq!(
                open.alert.message,
                app.texts
                    .text_with("paste-lines-message", &[("lines", Arg::Number(2))])
            );
            assert!(
                open.alert.detail.ends_with("\n\na\nb"),
                "{}",
                open.alert.detail
            );
            assert_eq!(
                open.alert.buttons,
                [
                    app.texts.text("paste-lines-paste"),
                    app.texts.text("paste-lines-one-line"),
                    app.texts.text("paste-lines-cancel")
                ]
            );
            assert_eq!(
                open.alert.suppression,
                Some(app.texts.text("paste-lines-dont-ask"))
            );
            let id = open.id;
            fake.answer_alert(&mut app, id, button);
            if button == 2 {
                let pause = Instant::now() + Duration::from_millis(300);
                run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
                assert!(
                    !shows(&app, window, "hellolsa"),
                    "{:?}",
                    screen(&app, window)
                );
            }
        }
        // The line breaks reach the program as Return; as one line, a
        // space for each, but the last.
        run_until(&mut fake, &mut app, |_, app| {
            shows(app, window, "hellolsa^Mb^Ma b")
        });
        // The question asked nothing of the config.
        assert_eq!(app.config.clipboard.confirm_paste, ConfirmPaste::Auto);
    }

    #[test]
    fn a_program_that_asks_for_bracketed_paste_takes_line_breaks_at_once() {
        let (mut fake, mut app, window) = focused_with(reader("\\033[?2004h"));
        paste_key(&mut fake, &mut app, window, "a\nb");
        assert!(fake.alerts().is_empty());
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "^[[200~a"));
        // Always asks, there too, and for a line with its line break.
        let (mut fake, mut app, window, _) = pasting(
            "always",
            "[clipboard]\nconfirm-paste = \"always\"\n",
            reader("\\033[?2004h"),
        );
        paste_key(&mut fake, &mut app, window, "ls\n");
        assert_eq!(fake.alerts().len(), 1);
        // Never asks nothing.
        let (mut fake, mut app, window, _) = pasting(
            "never",
            "[clipboard]\nconfirm-paste = \"never\"\n",
            reader(""),
        );
        paste_key(&mut fake, &mut app, window, "a\nb");
        assert!(fake.alerts().is_empty());
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "a^Mb"));
    }

    #[test]
    fn don_t_ask_again_writes_the_config() {
        let (mut fake, mut app, window, path) =
            pasting("again", "# mine\n[font]\nsize = 14\n", reader(""));
        paste_key(&mut fake, &mut app, window, "a\nb");
        let id = fake.alerts()[0].id;
        // Ticked, but cancelled: nothing changes.
        fake.answer_alert_with(&mut app, id, 2, true);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "# mine\n[font]\nsize = 14\n"
        );
        paste_key(&mut fake, &mut app, window, "a\nb");
        let id = fake.alerts()[0].id;
        fake.answer_alert_with(&mut app, id, 0, true);
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(
            written.starts_with("# mine\n[font]\nsize = 14\n"),
            "{written}"
        );
        assert_eq!(
            Config::read(&path).unwrap().clipboard.confirm_paste,
            ConfirmPaste::Never
        );
        // The next paste pastes, before the file's reload too.
        paste_key(&mut fake, &mut app, window, "c\nd");
        assert!(fake.alerts().is_empty());
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "a^Mbc^Md"));
        assert!(!app.pastes_unconfirmed);
    }

    #[test]
    fn don_t_ask_again_lasts_until_the_app_quits_where_the_config_cannot_keep_it() {
        let text = "[font]\nsise = 14\n";
        let (mut fake, mut app, window, path) = pasting("bad", text, reader(""));
        paste_key(&mut fake, &mut app, window, "a\nb");
        let id = fake.alerts()[0].id;
        fake.answer_alert_with(&mut app, id, 1, true);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        assert!(app.pastes_unconfirmed);
        // A reload of the file does not bring the question back.
        app.apply(&mut fake, Config::default());
        paste_key(&mut fake, &mut app, window, "c\nd");
        assert!(fake.alerts().is_empty());
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "a bc^Md"));
    }

    #[test]
    fn a_window_that_closes_drops_its_paste_s_question() {
        let (mut fake, mut app, window) = focused_with(reader(""));
        paste_key(&mut fake, &mut app, window, "a\nb");
        let id = fake.alerts()[0].id;
        app.run(&mut fake, NEW_WINDOW, Args::new());
        // Its program, `cat`, runs: the question, answered.
        fake.send(&mut app, window, WindowEvent::CloseRequested);
        let close = fake.alerts().last().unwrap().id;
        fake.answer_alert(&mut app, close, 0);
        assert!(app.paste_asks.is_empty());
        // A late answer reaches nothing.
        fake.answer_alert(&mut app, id, 0);
        assert_eq!(fake.windows().count(), 1);
    }

    #[test]
    fn the_font_s_size_and_the_view_take_their_keys() {
        // More lines than the window's rows, for a scrollback.
        let (mut fake, mut app, window) =
            focused_with(shell("seq 1 200; echo ready; stty raw -echo; cat -v"));
        let named = |code, plain: &str, mods| key_event(code, plain, mods, true);
        fake.send(&mut app, window, named(KeyCode::Equal, "=", CMD));
        assert_eq!(app.font_size, Some(14.0));
        fake.send(&mut app, window, named(KeyCode::Minus, "-", CMD));
        fake.send(&mut app, window, named(KeyCode::Minus, "-", CMD));
        assert_eq!(app.font_size, Some(12.0));
        fake.send(&mut app, window, named(KeyCode::Digit0, "0", CMD));
        assert_eq!(app.font_size, None);
        // A line up, a page down, to the top.
        let offset = |app: &App| app.pane(window).snapshot().offset();
        fake.send(&mut app, window, named(KeyCode::ArrowUp, "", CMD));
        run_until(&mut fake, &mut app, |_, app| offset(app) == 1);
        fake.send(&mut app, window, named(KeyCode::PageDown, "", SHIFT));
        run_until(&mut fake, &mut app, |_, app| offset(app) == 0);
        fake.send(&mut app, window, named(KeyCode::Home, "", CMD));
        run_until(&mut fake, &mut app, |_, app| offset(app) > 1);
    }

    #[test]
    fn clearing_keeps_the_prompt_at_the_top() {
        let (mut fake, mut app, window) =
            focused_with(shell("seq 1 60; printf 'ready$'; stty raw -echo; cat -v"));
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready$"));
        fake.send(&mut app, window, command_key(KeyCode::KeyK, "k"));
        run_until(&mut fake, &mut app, |_, app| {
            row(app, window, 0) == "ready$"
        });
        let shown = screen(&app, window);
        assert!(shown[1..].iter().all(String::is_empty), "{shown:?}");
        // Nothing is left above it either, and typing goes on at the prompt.
        fake.send(&mut app, window, command_key(KeyCode::KeyA, "a"));
        fake.send(&mut app, window, command_key(KeyCode::KeyC, "c"));
        assert_eq!(fake.clipboard_text().as_deref(), Some("ready$"));
        fake.send(
            &mut app,
            window,
            key_event(KeyCode::KeyX, "x", Modifiers::NONE, true),
        );
        run_until(&mut fake, &mut app, |_, app| {
            row(app, window, 0) == "ready$x"
        });
    }

    /// `event`, a key's press, as the system repeats it.
    fn repeated(event: WindowEvent) -> WindowEvent {
        let WindowEvent::Key(key) = event else {
            unreachable!("a key");
        };
        WindowEvent::Key(KeyEvent {
            state: KeyState::Repeated,
            ..key
        })
    }

    #[test]
    fn a_key_s_release_reaches_the_program_after_its_binding_repeated() {
        let keys = "\"ctrl+a\" = \"window.new\"\n";
        let (mut fake, mut app, window) = bound(keys, reader("\\033[>3u"));
        let bound = key_event(KeyCode::KeyA, "a", CTRL, true);
        fake.send(&mut app, window, bound.clone());
        fake.send(&mut app, window, repeated(bound));
        fake.send(&mut app, window, key_event(KeyCode::KeyA, "a", CTRL, false));
        assert_eq!(fake.windows().count(), 3);
        // The key without Control is the program's, its release too.
        let plain = Modifiers::NONE;
        fake.send(&mut app, window, key_event(KeyCode::KeyA, "a", plain, true));
        fake.send(
            &mut app,
            window,
            key_event(KeyCode::KeyA, "a", plain, false),
        );
        run_until(&mut fake, &mut app, |_, app| {
            shows(app, window, "a^[[97;1:3u")
        });
    }

    #[test]
    fn a_sequence_s_keys_go_back_to_the_program_with_their_releases() {
        let keys = "\"ctrl+a>n\" = \"window.new\"\n";
        let (mut fake, mut app, window) = bound(keys, reader("\\033[>3u"));
        // Released while the sequence waits, and held while it repeats.
        let first = key_event(KeyCode::KeyA, "a", CTRL, true);
        fake.send(&mut app, window, first.clone());
        fake.send(&mut app, window, repeated(first));
        fake.send(&mut app, window, key_event(KeyCode::KeyA, "a", CTRL, false));
        let plain = Modifiers::NONE;
        fake.send(&mut app, window, key_event(KeyCode::KeyE, "e", plain, true));
        run_until(&mut fake, &mut app, |_, app| {
            shows(app, window, "^[[97;5u^[[97;5:3ue")
        });
        // Pressed, then given back, then released: the release follows.
        fake.send(&mut app, window, key_event(KeyCode::KeyA, "a", CTRL, true));
        fake.send(&mut app, window, key_event(KeyCode::KeyE, "e", plain, true));
        fake.send(&mut app, window, key_event(KeyCode::KeyA, "a", CTRL, false));
        run_until(&mut fake, &mut app, |_, app| {
            shows(app, window, "e^[[97;5ue^[[97;5:3u")
        });
        assert_eq!(fake.windows().count(), 1);
    }

    #[test]
    fn a_commit_and_a_keypad_key_end_a_sequence() {
        let keys = "\"ctrl+a>n\" = \"window.new\"\n";
        let (mut fake, mut app, window) = bound(keys, reader(""));
        fake.send(&mut app, window, key_event(KeyCode::KeyA, "a", CTRL, true));
        fake.send(
            &mut app,
            window,
            WindowEvent::Ime(Ime::Commit("x".to_owned())),
        );
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "^Ax"));
        fake.send(&mut app, window, key_event(KeyCode::KeyA, "a", CTRL, true));
        fake.send(
            &mut app,
            window,
            key_event(KeyCode::Numpad1, "1", Modifiers::NONE, true),
        );
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "^Ax^A1"));
        // The sequence ended: `n` types.
        fake.send(
            &mut app,
            window,
            key_event(KeyCode::KeyN, "n", Modifiers::NONE, true),
        );
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "^Ax^A1n"));
        assert_eq!(fake.windows().count(), 1);
    }

    #[test]
    fn losing_the_focus_forgets_the_keys_that_the_keymap_took() {
        let keys = "\"ctrl+a\" = \"window.new\"\n";
        let (mut fake, mut app, window) = bound(keys, reader("\\033[>3u"));
        fake.send(&mut app, window, key_event(KeyCode::KeyA, "a", CTRL, true));
        // The release comes to another window.
        fake.send(&mut app, window, WindowEvent::Focused(false));
        fake.send(&mut app, window, WindowEvent::Focused(true));
        let plain = Modifiers::NONE;
        fake.send(&mut app, window, key_event(KeyCode::KeyA, "a", plain, true));
        fake.send(
            &mut app,
            window,
            key_event(KeyCode::KeyA, "a", plain, false),
        );
        run_until(&mut fake, &mut app, |_, app| {
            shows(app, window, "a^[[97;1:3u")
        });
    }

    #[test]
    fn a_binding_holds_in_its_context() {
        let keys = "\"ctrl+o\" = { command = \"window.new\", when = \"alternate-screen\" }\n";
        let program = shell(
            "stty raw -echo; echo ready; head -c 1 >/dev/null; printf '\\033[?1049h'; echo alt; cat -v",
        );
        let (mut fake, mut app, window) = bound(keys, program);
        // On the primary screen, the key is the program's.
        fake.send(&mut app, window, key_event(KeyCode::KeyO, "o", CTRL, true));
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "alt"));
        assert_eq!(fake.windows().count(), 1);
        fake.send(&mut app, window, key_event(KeyCode::KeyO, "o", CTRL, true));
        assert_eq!(fake.windows().count(), 2);
    }

    #[test]
    fn a_binding_holds_with_a_selection_or_without() {
        let keys = concat!(
            "\"cmd+e\" = [\n",
            "  { command = \"pane.send-text\", args = { text = \"S\" }, when = \"selection\" },\n",
            "  { command = \"pane.send-text\", args = { text = \"N\" }, when = \"!selection\" },\n",
            "]\n",
        );
        let (mut fake, mut app, window) = bound(keys, reader(""));
        fake.send(&mut app, window, WindowEvent::Focused(true));
        let cmd_e = || key_event(KeyCode::KeyE, "e", CMD, true);
        fake.send(&mut app, window, cmd_e());
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "N"));
        // Command-A selects everything.
        fake.send(&mut app, window, command_key(KeyCode::KeyA, "a"));
        fake.send(&mut app, window, cmd_e());
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "NS"));
    }

    #[test]
    fn a_step_of_the_wheel_runs_its_binding() {
        let keys = "\"cmd+wheel-up\" = \"window.new\"\n";
        let (mut fake, mut app, window) = bound(keys, reader(""));
        let up = ScrollDelta::Lines { x: 0.0, y: 2.0 };
        fake.send(&mut app, window, wheel(up, CMD));
        assert_eq!(fake.windows().count(), 3, "a window a step");
        // Without Command, the wheel is the view's.
        fake.send(&mut app, window, wheel(up, Modifiers::NONE));
        assert_eq!(fake.windows().count(), 3);
    }

    #[test]
    fn a_reloaded_config_binds_its_keys_anew() {
        let (mut fake, mut app, path) = with_file("keys", "[keys]\n\"cmd+n\" = false\n");
        let window = fake.windows().next().unwrap();
        fake.send(&mut app, window, key_event(KeyCode::KeyN, "n", CMD, true));
        assert_eq!(fake.windows().count(), 1, "the default taken away");
        std::fs::write(&path, "").unwrap();
        change(&mut fake, &mut app);
        fake.send(&mut app, window, key_event(KeyCode::KeyN, "n", CMD, true));
        assert_eq!(fake.windows().count(), 2);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn an_invalid_config_shows_its_notice_until_one_that_reads() {
        let (mut fake, mut app, path) = with_file("notice", "[font]\nsize = 14\n");
        let window = fake.windows().next().unwrap();
        assert_eq!(app.diagnostics.shown(), None);
        // The error, of its line, and nothing of the file applies.
        std::fs::write(&path, "[font]\nsize = 15\nbogus = 1\n").unwrap();
        fake.send(&mut app, window, WindowEvent::Frame);
        change(&mut fake, &mut app);
        let shown = app.diagnostics.shown().cloned().expect("a notice");
        assert_eq!(
            (shown.code, &shown.about),
            (Code::InvalidConfig, &About::Config)
        );
        assert!(shown.text.contains("config.toml:3:1: "), "{}", shown.text);
        assert!(shown.text.ends_with("and the one in force stays"));
        assert_eq!(app.config.font.size.points(), 14.0);
        assert!(fake.window(window).unwrap().frame_requested);
        // Another error takes its place; a config that reads takes it away.
        std::fs::write(&path, "[font]\nsize = 1000\n").unwrap();
        change(&mut fake, &mut app);
        let again = app.diagnostics.shown().cloned().expect("a notice");
        assert_ne!(again.text, shown.text);
        std::fs::write(&path, "[font]\nsize = 16\n").unwrap();
        change(&mut fake, &mut app);
        assert_eq!(app.diagnostics.shown(), None);
        assert_eq!(app.config.font.size.points(), 16.0);
        // Back to the config in force, as after an error, takes it away too.
        std::fs::write(&path, "bogus = 1\n").unwrap();
        change(&mut fake, &mut app);
        std::fs::write(&path, "[font]\nsize = 16\n").unwrap();
        change(&mut fake, &mut app);
        assert_eq!(app.diagnostics.shown(), None);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_font_that_is_not_installed_shows_its_notice_until_one_that_is() {
        let family = |name: &str| Some(name.to_owned().try_into().unwrap());
        let mut config = Config::default();
        config.font.family = family("No Such Family 42");
        // A machine without a GPU, as some CI's, draws nothing.
        let Ok(drawing) = Drawing::new(config.font.clone()) else {
            return;
        };
        let mut fake = Fake::default();
        let mut app = App::new(Some(drawing), config, None).with_program(quiet());
        fake.start(&mut app);
        let shown = app.diagnostics.shown().cloned().expect("a notice");
        assert_eq!(
            (shown.code, &shown.about),
            (Code::Unavailable, &About::Font)
        );
        assert!(
            shown.text.starts_with(
                "the font family `No Such Family 42` is not installed; the text takes `"
            )
        );
        // A family that is there takes it away.
        let mut config = app.config.clone();
        config.font.family = family("Menlo");
        app.apply(&mut fake, config);
        assert_eq!(app.diagnostics.shown(), None);
    }

    #[test]
    fn a_failure_shows_its_notice_and_a_click_on_the_bar_closes_it() {
        let (mut fake, mut app, window, _) = with_tester();
        // A command that the caller named wrongly goes to the log alone.
        app.run(&mut fake, "test.nothing", Args::new());
        assert_eq!(app.diagnostics.shown(), None);
        // A plugin that panics: a fault, which disables it.
        app.run(&mut fake, "test.boom", Args::new());
        let shown = app.diagnostics.shown().cloned().expect("a notice");
        assert_eq!(shown.code, Code::Fault);
        assert!(
            shown.text.contains("panicked: on purpose"),
            "{}",
            shown.text
        );
        // Where the bar was drawn, a press on its close closes it; on the
        // rest of it, nothing; neither reaches the program, nor starts a
        // selection.
        let rect = |x, width| Rect {
            x,
            y: 56,
            width,
            height: 40,
        };
        let rects = notice::Rects {
            bar: rect(0, 800),
            close: rect(780, 20),
        };
        app.windows.get_mut(&window).unwrap().notice = Some(rects);
        let scale = fake.window(window).unwrap().size.scale;
        let at = |x: f64, y: f64| Point {
            x: x / scale,
            y: y / scale,
        };
        let press = |position| WindowEvent::MouseButton {
            button: lokyr_platform_core::MouseButton::Left,
            pressed: true,
            clicks: 1,
            position,
            mods: Modifiers::NONE,
        };
        fake.send(&mut app, window, press(at(10.0, 70.0)));
        assert!(app.diagnostics.shown().is_some());
        assert!(app.windows[&window].selecting.is_none());
        fake.send(&mut app, window, press(at(790.0, 70.0)));
        assert_eq!(app.diagnostics.shown(), None);
        assert!(app.windows[&window].selecting.is_none());
        // A failure of a command shows too; a press below the bar leaves it.
        let failed = Diagnostic::new(Code::Failed, "it failed", About::Other);
        app.report(&mut fake, failed);
        fake.send(&mut app, window, press(at(10.0, 120.0)));
        assert!(app.diagnostics.shown().is_some());
        assert!(app.windows[&window].selecting.is_some());
    }

    #[test]
    fn keys_reach_the_program_but_command_s_do_not() {
        use lokyr_platform_core::KeyCode;

        let (mut fake, mut app) = started();
        let window = fake.windows().next().unwrap();
        // `cat`, which the terminal echoes, and which writes back each line.
        fake.send(&mut app, window, typed(KeyCode::KeyH, "h", true));
        fake.send(&mut app, window, typed(KeyCode::KeyO, "o", false));
        fake.send(&mut app, window, typed(KeyCode::KeyK, "k", false));
        fake.send(&mut app, window, typed(KeyCode::Enter, "\r", false));
        run_until(&mut fake, &mut app, |_, app| row(app, window, 1) == "ok");
        assert_eq!(row(&app, window, 0), "ok");
    }

    #[test]
    fn a_clean_exit_closes_the_window_and_another_keeps_it_until_a_key() {
        let (fake, _, _) = ended(CloseOnExit::Clean, "exit 0");
        assert_eq!(fake.windows().count(), 0);
        assert!(fake.has_quit());

        let (mut fake, mut app, window) = ended(CloseOnExit::Clean, "printf 'oops'; exit 3");
        assert_eq!(fake.windows().count(), 1);
        run_until(&mut fake, &mut app, |_, app| {
            row(app, window, 1) == "[the program exited with code 3; a key closes the window]"
        });
        assert_eq!(row(&app, window, 0), "oops");
        fake.send(&mut app, window, key());
        assert_eq!(fake.windows().count(), 0);
        assert!(fake.has_quit());
    }

    #[test]
    fn a_kept_window_closes_with_a_key_that_types_or_acts_alone() {
        use lokyr_platform_core::KeyCode;

        let (mut fake, mut app, window) = ended(CloseOnExit::Clean, "exit 3");
        // A lock, Fn, a modifier and a Command key leave it open.
        for key in [
            typed(KeyCode::CapsLock, "", false),
            typed(KeyCode::Fn, "", false),
            typed(KeyCode::ShiftLeft, "", false),
            typed(KeyCode::KeyH, "h", true),
        ] {
            fake.send(&mut app, window, key);
            assert_eq!(fake.windows().count(), 1);
        }
        // What an input method composes does not show; its commit closes.
        let composing = Ime::Preedit {
            text: "に".to_owned(),
            selection: 3..3,
        };
        fake.send(&mut app, window, WindowEvent::Ime(composing));
        assert_eq!(app.windows[&window].preedit, None);
        fake.send(
            &mut app,
            window,
            WindowEvent::Ime(Ime::Commit("日".to_owned())),
        );
        assert_eq!(fake.windows().count(), 0);
    }

    #[test]
    fn a_program_that_cannot_start_leaves_a_window_that_a_key_closes() {
        let mut fake = Fake::default();
        let missing = Program::command("/nonexistent/lokyr-program", [] as [&str; 0]);
        let mut app = app(Config::default(), missing);
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        assert!(row(&app, window, 0).starts_with("[the program cannot start"));
        fake.send(&mut app, window, key());
        assert_eq!(fake.windows().count(), 0);
    }

    #[test]
    fn the_line_of_a_program_that_ended_reads_whatever_it_left_set() {
        // The line drawing character set, insert mode and no autowrap.
        let script = "printf '\\033(0\\033[4h\\033[?7l'; exit 3";
        let (mut fake, mut app, window) = ended(CloseOnExit::Clean, script);
        run_until(&mut fake, &mut app, |_, app| {
            row(app, window, 1) == "[the program exited with code 3; a key closes the window]"
        });
    }

    #[test]
    fn always_closes_and_never_keeps() {
        let (fake, _, _) = ended(CloseOnExit::Always, "exit 3");
        assert_eq!(fake.windows().count(), 0);
        let (fake, app, window) = ended(CloseOnExit::Never, "exit 0");
        assert_eq!(fake.windows().count(), 1);
        assert!(app.pane(window).is_kept());
        // A program that a signal ends.
        let (mut fake, mut app, window) = ended(CloseOnExit::Clean, "kill -9 $$");
        run_until(&mut fake, &mut app, |_, app| {
            row(app, window, 1) == "[the program ended by signal 9; a key closes the window]"
        });
    }

    #[test]
    fn the_input_method_s_text_shows_then_reaches_the_program() {
        use lokyr_platform_core::KeyCode;

        let (mut fake, mut app) = started();
        let window = fake.windows().next().unwrap();
        fake.send(&mut app, window, WindowEvent::Frame);
        // The candidates' place: the cursor's cell, at the top left within
        // the padding of 4 points, in the points of cells of 8 by 16 pixels
        // at a scale of 2.
        let (origin, size) = fake.window(window).unwrap().text_area.unwrap();
        assert_eq!((origin.x, origin.y), (4.0, 4.0));
        assert_eq!((size.width, size.height), (4.0, 8.0));
        let composing = Ime::Preedit {
            text: "にほ".to_owned(),
            selection: 6..6,
        };
        fake.send(&mut app, window, WindowEvent::Ime(composing));
        assert_eq!(
            app.windows[&window].preedit,
            Some(Preedit {
                text: "にほ".to_owned(),
                cursor: 6
            })
        );
        assert!(fake.window(window).unwrap().frame_requested);
        fake.send(
            &mut app,
            window,
            WindowEvent::Ime(Ime::Commit("日本".to_owned())),
        );
        assert_eq!(app.windows[&window].preedit, None);
        fake.send(&mut app, window, typed(KeyCode::Enter, "\r", false));
        // `cat` writes back the line.
        run_until(&mut fake, &mut app, |_, app| row(app, window, 1) == "日本");
        // An empty preedit ends the composition with nothing typed.
        let composing = Ime::Preedit {
            text: "k".to_owned(),
            selection: 1..1,
        };
        fake.send(&mut app, window, WindowEvent::Ime(composing));
        let ended = Ime::Preedit {
            text: String::new(),
            selection: 0..0,
        };
        fake.send(&mut app, window, WindowEvent::Ime(ended));
        assert_eq!(app.windows[&window].preedit, None);
    }

    #[test]
    fn output_starts_the_blinking_again() {
        let mut fake = Fake::default();
        let mut app = app(Config::default(), shell("sleep 0.3; printf x; sleep 10"));
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        let focused = Instant::now();
        fake.send(&mut app, window, WindowEvent::Focused(true));
        run_until(&mut fake, &mut app, |_, app| row(app, window, 0) == "x");
        // The output came at least 0.3 s after the focus: 15.1 s after the
        // focus, the cursor still blinks, as its timeout counts from it.
        let later = focused + cursor::BLINK_TIMEOUT + Duration::from_millis(100);
        app.cursor(&mut fake, window, later).map(|(shape, _)| shape);
        assert!(app.windows[&window].blinking);
    }

    /// A directory of the test's own, made empty.
    fn directory(test: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("lokyr-app-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        directory
    }

    /// The startup lines of a shell whose prompt is as oh-my-posh draws it
    /// with its shell integration: three lines of output above it, then a
    /// first line computed once, as wide as the terminal, `left` and
    /// `right` at its edges, and the input line, marked with OSC 133.
    const PROMPT_OUTPUT: &str = "for n in 1 2 3; do echo \"output $n\"; done\n\
        cols=$(stty size | cut -d' ' -f2)\n\
        first=\"left$(printf '%*s' $((cols - 9)) '')right\"\n";

    /// Drags the window of `app` through `widths`, in points, a step every
    /// 8 ms, as a live resize does.
    fn drag(fake: &mut Fake, app: &mut App, window: WindowId, widths: &[f64]) {
        for pair in widths.windows(2) {
            for step in 0..=20 {
                let width = pair[0] + (pair[1] - pair[0]) * f64::from(step) / 20.0;
                fake.send(app, window, WindowEvent::Resized(points(width, 500.0)));
                let until = Instant::now() + Duration::from_millis(8);
                run_until(fake, app, |_, _| Instant::now() > until);
            }
        }
    }

    /// Runs the shell that `command` starts through a drag from 200
    /// columns to 75, 175 and 100, and waits for its screen to show
    /// `expected`, its rows without their spaces at either end, and to keep
    /// it a while.
    fn prompt_through_a_drag(command: &str, expected: &[&str]) {
        let mut fake = Fake::default();
        let mut app = app(unpadded(), shell(command));
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        let trimmed = |app: &App| -> Vec<String> {
            screen(app, window)
                .iter()
                .take(expected.len())
                .map(|line| line.trim().to_owned())
                .collect()
        };
        run_until(&mut fake, &mut app, |_, app| {
            screen(app, window).iter().any(|line| line.starts_with('>'))
        });
        drag(&mut fake, &mut app, window, &[800.0, 300.0, 700.0, 400.0]);
        run_until(&mut fake, &mut app, |_, app| trimmed(app) == expected);
        let until = Instant::now() + Duration::from_millis(300);
        run_until(&mut fake, &mut app, |_, _| Instant::now() > until);
        assert_eq!(trimmed(&app), expected);
    }

    #[test]
    fn a_zsh_prompt_draws_once_through_a_drag() {
        let directory = directory("zsh-prompt");
        let rc = format!(
            r#"{PROMPT_OUTPUT}PROMPT=$'%{{\e]133;D;0\a\e]133;A\a%}}'"$first"$'\n> %{{\e]133;B\a%}}'
preexec() {{ print -n '\e]133;C\a' }}
print -z 'echo typed'
"#
        );
        std::fs::write(directory.join(".zshrc"), rc).unwrap();
        let zsh = format!("ZDOTDIR={} exec /bin/zsh -d -i", directory.display());
        // The first line, of 200 columns, takes two rows at 100; zsh draws
        // the prompt again, with what was typed, where the cleared one was.
        let expected = [
            "output 1",
            "output 2",
            "output 3",
            "left",
            "right",
            "> echo typed",
            "",
        ];
        prompt_through_a_drag(&zsh, &expected);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn a_bash_prompt_keeps_the_lines_that_bash_does_not_draw_again() {
        let directory = directory("bash-prompt");
        let rc = directory.join("rc");
        let prompt = format!(
            r#"{PROMPT_OUTPUT}PS1='\[\e]133;D;0\a\e]133;A;redraw=last\a\]'"$first"'\n> \[\e]133;B\a\]'
"#
        );
        std::fs::write(&rc, prompt).unwrap();
        let bash = format!(
            "BASH_SILENCE_DEPRECATION_WARNING=1 exec /bin/bash --noprofile --rcfile {} -i",
            rc.display()
        );
        // bash draws the last line again, and the first reflows.
        let expected = ["output 1", "output 2", "output 3", "left", "right", ">", ""];
        prompt_through_a_drag(&bash, &expected);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn blinking_text_keeps_the_blinking_s_timer_in_the_focused_window() {
        // A cursor that does not blink, over text that does.
        let config = |text_blink: bool| {
            let mut config = Config::default();
            config.cursor.blink = false;
            config.text.blink = text_blink;
            config
        };
        let script = "printf '\\033[5mblink\\033[0m'; sleep 10";
        for (text_blink, focused, timer) in [
            (true, true, true),
            (false, true, false),
            (true, false, false),
        ] {
            let mut fake = Fake::default();
            let mut app = app(config(text_blink), shell(script));
            fake.start(&mut app);
            let window = fake.windows().next().unwrap();
            fake.send(&mut app, window, WindowEvent::Focused(focused));
            run_until(&mut fake, &mut app, |_, app| row(app, window, 0) == "blink");
            fake.send(&mut app, window, WindowEvent::Frame);
            assert_eq!(app.blink_timer.is_some(), timer, "{text_blink} {focused}");
            assert_eq!(app.windows[&window].blinking, timer);
        }
    }

    #[test]
    fn other_commands_do_nothing() {
        let (mut fake, mut app) = started();
        fake.send_app(&mut app, AppEvent::Command("unknown".to_owned()));
        assert!(!fake.has_quit());
        assert_eq!(fake.windows().count(), 1);
    }

    /// The quick terminal's window, if the app opened it.
    fn quick_window(fake: &Fake) -> Option<WindowId> {
        fake.windows().find(|&window| {
            fake.window(window)
                .is_some_and(|state| state.quick.is_some())
        })
    }

    fn toggle(fake: &mut Fake, app: &mut App) {
        fake.send_app(app, AppEvent::Command(TOGGLE_QUICK.to_owned()));
    }

    #[test]
    fn the_quick_terminal_shows_and_hides_keeping_its_pane() {
        let (mut fake, mut app) = started();
        assert_eq!(quick_window(&fake), None);
        toggle(&mut fake, &mut app);
        let quick = quick_window(&fake).unwrap();
        let shown = |fake: &Fake| fake.window(quick).unwrap().quick.unwrap();
        // At the top, of half the area under the menu bar, its pane of
        // that size from the start.
        assert_eq!(
            shown(&fake),
            Some(lokyr_platform_core::QuickPlace::default())
        );
        let size = fake.window(quick).unwrap().size;
        assert_eq!((size.points.width, size.points.height), (1440.0, 438.0));
        let pane = app.windows[&quick].pane;
        assert_eq!(app.grid(size), app.pane(quick).grid());
        // With the focus, it hides; its pane stays.
        fake.send(&mut app, quick, WindowEvent::Focused(true));
        toggle(&mut fake, &mut app);
        assert_eq!(shown(&fake), None);
        fake.send(&mut app, quick, WindowEvent::Focused(false));
        assert_eq!(app.windows[&quick].pane, pane);
        // It shows again; without the focus, it takes it rather than
        // hiding.
        toggle(&mut fake, &mut app);
        assert!(shown(&fake).is_some());
        toggle(&mut fake, &mut app);
        assert!(shown(&fake).is_some());
        assert_eq!(quick_window(&fake), Some(quick));
        // Losing the focus hides it, as `autohide` says.
        fake.send(&mut app, quick, WindowEvent::Focused(true));
        fake.send(&mut app, quick, WindowEvent::Focused(false));
        assert_eq!(shown(&fake), None);
        // Closed, the next shows a new one.
        app.close(&mut fake, quick);
        assert_eq!(quick_window(&fake), None);
        assert!(!fake.has_quit());
        toggle(&mut fake, &mut app);
        assert_ne!(quick_window(&fake), Some(quick));
    }

    #[test]
    fn the_quick_terminal_stays_as_it_loses_the_focus_without_autohide_or_to_a_question() {
        let mut config = Config::default();
        config.quick_terminal.autohide = false;
        config.quick_terminal.position = config::QuickPosition::Left;
        let mut fake = Fake::default();
        let mut app = app(config, quiet());
        fake.start(&mut app);
        toggle(&mut fake, &mut app);
        let quick = quick_window(&fake).unwrap();
        let place = fake.window(quick).unwrap().quick.unwrap().unwrap();
        assert_eq!(place.edge, lokyr_platform_core::QuickEdge::Left);
        fake.send(&mut app, quick, WindowEvent::Focused(true));
        fake.send(&mut app, quick, WindowEvent::Focused(false));
        assert!(fake.window(quick).unwrap().quick.unwrap().is_some());
        // With autohide, a question on it, which takes the focus, keeps it.
        app.config.quick_terminal.autohide = true;
        fake.send(&mut app, quick, WindowEvent::Focused(true));
        app.ask_title(&mut fake, quick);
        fake.send(&mut app, quick, WindowEvent::Focused(false));
        assert!(fake.window(quick).unwrap().quick.unwrap().is_some());
    }

    #[test]
    fn a_global_key_runs_its_command_from_every_app_and_keeps_the_app_open() {
        let config = Config::parse(
            "[keys]\n\"global:ctrl+`\" = \"window.toggle-quick-terminal\"\n\"global:ctrl+f24\" = \"window.new\"\n",
            std::path::Path::new("config.toml"),
        )
        .unwrap();
        let mut fake = Fake::default();
        let mut app = app(config, quiet());
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        let key = GlobalKey {
            code: KeyCode::Backquote,
            command: false,
            control: true,
            alt: false,
            shift: false,
        };
        let f24 = GlobalKey {
            code: KeyCode::F(24),
            ..key
        };
        assert_eq!(fake.global_keys(), [key, f24]);
        // The one that the system refused, of a key that the Mac lacks, is
        // told.
        let shown = app.diagnostics.shown().unwrap();
        assert_eq!(shown.about, About::GlobalKeys);
        assert!(shown.text.contains("`global:ctrl+f24`"), "{}", shown.text);
        fake.send_app(&mut app, AppEvent::GlobalKey(0));
        assert!(quick_window(&fake).is_some());
        // Without its windows, the app waits for the key, and opens a
        // window as the Dock's icon asks.
        let quick = quick_window(&fake).unwrap();
        app.close(&mut fake, quick);
        app.close(&mut fake, window);
        assert_eq!(fake.windows().count(), 0);
        assert!(!fake.has_quit());
        fake.send_app(&mut app, AppEvent::Reopen);
        assert_eq!(fake.windows().count(), 1);
        // A key that another app takes is told too; once the config takes
        // it away, the notice goes, and the last window quits again.
        fake.take_global_key(key);
        app.apply(
            &mut fake,
            Config::parse(
                "[keys]\n\"global:ctrl+`\" = \"window.toggle-quick-terminal\"\n",
                std::path::Path::new("config.toml"),
            )
            .unwrap(),
        );
        let shown = app.diagnostics.shown().unwrap();
        assert!(
            shown.text.contains("`global:ctrl+`` cannot"),
            "{}",
            shown.text
        );
        app.apply(&mut fake, Config::default());
        assert!(app.diagnostics.shown().is_none());
        assert!(fake.global_keys().is_empty());
        // A key that the system refused keeps nothing open.
        let refused = Config::parse(
            "[keys]\n\"global:ctrl+`\" = \"window.toggle-quick-terminal\"\n",
            std::path::Path::new("config.toml"),
        )
        .unwrap();
        app.apply(&mut fake, refused);
        assert_eq!(fake.global_keys(), [key]);
        let window = fake.windows().next().unwrap();
        app.close(&mut fake, window);
        assert!(fake.has_quit());
    }

    #[test]
    fn without_a_global_key_the_last_window_quits_with_the_hidden_quick_terminal() {
        // Its program asked about first.
        let (mut fake, mut app) = started();
        let window = fake.windows().next().unwrap();
        toggle(&mut fake, &mut app);
        let quick = quick_window(&fake).unwrap();
        fake.send(&mut app, quick, WindowEvent::Focused(true));
        toggle(&mut fake, &mut app);
        assert_eq!(fake.window(quick).unwrap().quick, Some(None));
        // A notification's click shows it, rather than focusing it hidden.
        fake.send_app(&mut app, AppEvent::NotificationClicked(quick));
        assert!(fake.window(quick).unwrap().quick.unwrap().is_some());
        assert!(fake.focus_requests().is_empty());
        fake.send(&mut app, quick, WindowEvent::Focused(false));
        assert_eq!(fake.window(quick).unwrap().quick, Some(None));
        app.close(&mut fake, window);
        assert!(!fake.has_quit());
        let [question] = fake.alerts() else {
            panic!("the question before quitting: {:?}", fake.alerts());
        };
        let question = question.id;
        fake.answer_alert(&mut app, question, 0);
        assert!(fake.has_quit());
        // A shell at its prompt quits at once; a quick terminal that shows
        // keeps the app.
        let (mut fake, mut app, window) = started_running(idle());
        toggle(&mut fake, &mut app);
        let quick = quick_window(&fake).unwrap();
        run_until(&mut fake, &mut app, |_, app| {
            screen(app, quick).iter().any(|line| line.contains("ready"))
        });
        app.close(&mut fake, window);
        assert!(!fake.has_quit());
        fake.send(&mut app, quick, WindowEvent::Focused(true));
        toggle(&mut fake, &mut app);
        fake.send_app(&mut app, AppEvent::Command(NEW_WINDOW.to_owned()));
        let window = fake.windows().find(|&opened| opened != quick).unwrap();
        app.close(&mut fake, window);
        assert!(fake.has_quit());
    }

    #[test]
    fn a_pane_s_output_asks_for_its_own_window_s_frame_alone() {
        let (mut fake, mut app, first) = started_running(idle());
        fake.send_app(&mut app, AppEvent::Command(NEW_WINDOW.to_owned()));
        let second = fake.windows().find(|&window| window != first).unwrap();
        run_until(&mut fake, &mut app, |_, app| {
            screen(app, second)
                .iter()
                .any(|line| line.contains("ready"))
        });
        for window in [first, second] {
            if fake.window(window).unwrap().frame_requested {
                fake.send(&mut app, window, WindowEvent::Frame);
            }
        }
        // The second's shell echoes a key: its window's frame, not the
        // first's.
        fake.send(&mut app, second, typed(KeyCode::KeyX, "x", false));
        let start = Instant::now();
        while !screen(&app, second).iter().any(|line| line.ends_with('x')) {
            assert!(start.elapsed() < Duration::from_secs(10), "no echo");
            fake.deliver_wake(&mut app);
            assert!(!fake.window(first).unwrap().frame_requested);
            if fake.window(second).unwrap().frame_requested {
                fake.send(&mut app, second, WindowEvent::Frame);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn a_stream_of_bells_wakes_the_main_thread_at_the_frame_s_pace() {
        let script = "echo ready; read line; while :; do printf '\\a\\a\\a\\a'; done";
        let (mut fake, mut app, window) = started_running(shell(script));
        let pane = app.windows[&window].pane;
        let wake = Arc::clone(&app.pane_wakes[&pane]);
        fake.send(&mut app, window, typed(KeyCode::Enter, "\r", false));
        // The main thread as it runs, which takes each wake at once, and a
        // display of 60 Hz, which draws the window while it shows.
        let run = |fake: &mut Fake, app: &mut App, drawn: bool| {
            let before = wake.wakes.load(Ordering::Relaxed);
            let start = Instant::now();
            let mut frames = 0;
            let mut frame_at = start;
            while start.elapsed() < Duration::from_millis(500) {
                fake.deliver_wake(app);
                fake.fire_timers(app, Instant::now());
                if drawn
                    && frame_at.elapsed() >= TOLD_PACE
                    && fake.window(window).unwrap().frame_requested
                {
                    fake.send(app, window, WindowEvent::Frame);
                    frames += 1;
                    frame_at = Instant::now();
                }
            }
            (wake.wakes.load(Ordering::Relaxed) - before, frames)
        };
        // Drawn: a wake a frame for the grid, and one a pace for the bells,
        // at most.
        let (wakes, frames) = run(&mut fake, &mut app, true);
        assert!(app.windows[&window].rang_at.is_some(), "the bell rang");
        assert!(wakes <= frames + 35, "{wakes} wakes for {frames} frames");
        // Not drawn: one a pace for the bells.
        fake.send(&mut app, window, WindowEvent::Occluded(true));
        let (wakes, _) = run(&mut fake, &mut app, false);
        assert!(wakes <= 35, "{wakes} wakes hidden");
    }

    #[test]
    fn a_frame_with_nothing_new_presents_nothing() {
        let Ok(drawing) = Drawing::new(FontConfig::default()) else {
            eprintln!("no Metal device: no frame to present");
            return;
        };
        let mut fake = Fake::default();
        let mut app = App::new(Some(drawing), unpadded(), None).with_program(idle());
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        let surface = Surface::detached(&app.drawing.as_ref().unwrap().gpu);
        app.windows.get_mut(&window).unwrap().surface = Some(surface);
        run_until(&mut fake, &mut app, |_, app| {
            screen(app, window)
                .iter()
                .any(|line| line.contains("ready"))
        });
        let drawn = |app: &App| app.windows[&window].drawn_at;
        fake.send(&mut app, window, WindowEvent::Frame);
        let first = drawn(&app);
        assert!(first.is_some());
        // The same again: nothing presents, and the surface keeps it.
        fake.send(&mut app, window, WindowEvent::Frame);
        assert_eq!(drawn(&app), first);
        // The focus's cursor is new.
        fake.send(&mut app, window, WindowEvent::Focused(true));
        fake.send(&mut app, window, WindowEvent::Frame);
        assert_ne!(drawn(&app), first);
    }
}
