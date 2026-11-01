//! The application: its windows, each with the pane of its program, what
//! they draw, its main menu, and its config, which it reads again when the
//! file changes.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use lokyr_kernel::commands::{Args, Category, Code, Command, Error, ErrorCategory, Registry};
use lokyr_kernel::config::{
    self, AppConfig, AppearanceMode, ClipboardRead, CloseOnExit, Config, ConfirmPaste,
    NewDirectory, OpenFile, OpenUrl, StartDirectory,
};
use lokyr_kernel::diagnostics::{About, Diagnostic, Diagnostics};
use lokyr_kernel::keymap::{self, Binding, Context, Invocation, Keymap, When};
use lokyr_kernel::l10n::{self, Texts};
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
    Alert, AlertId, AlertStyle, AppEvent, Appearance, Backdrop, GlobalKey, Handler, Platform,
    Point, PointerArea, PointerShape, Size, TargetKind, TimerId, WindowAttributes, WindowEvent,
    WindowId, WindowSize,
};
use lokyr_platform_core::{Blink as PlatformBlink, Easing, Layer, LayerPlace};
use lokyr_pty::Program;
use lokyr_render_core::{
    CellBatch, Color, CursorShape, DisplayList, Item, Rect, Renderer, Surface as _,
};
use lokyr_text_core::FontConfig;
use lokyr_ui_core::cursor;
use lokyr_ui_core::pane::{self as cells, CursorLook, TextLook};
use lokyr_ui_core::theme::{Theme, Variant};
use lokyr_vt_core::page::{Budget, Scrollback};
use lokyr_vt_core::terminal::{self as vt, ClipboardRequest, MouseTracking, Scroll};

use crate::drawing::{self, Backend, Drawing};
use crate::environment;
use crate::icon::Icons;
use crate::latency::{self, LatencyBench};
use crate::notice;
use crate::output_bench::{self, OutputBench, Phases};
use crate::performance::{self, Performance};
use crate::quick;
use crate::size_label;

use crate::watch::Watcher;
use crate::window::{Shown, Surfaces, Window, WindowLayer};

mod asks;
mod input;
mod menus;
mod notices;
#[cfg(test)]
mod tests;

use input::show_selection;
use menus::menu_context;

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

/// How long the startup's and the latency's benches wait for a frame on
/// the display, as one that sleeps, or a machine without one, shows none.
const BENCH_DEADLINE: Duration = Duration::from_secs(15);

/// How long a frame starts looking up the faces of characters that it has
/// not met, and how long it draws the glyphs that the atlas lacks, on the
/// font's threads: a frame of new glyphs takes up to about 10 ms, within a
/// refresh at 60 Hz, and a page of them shows whole in a few frames.
const GLYPHS_BUDGET: (Duration, Duration) = (Duration::from_millis(2), Duration::from_millis(2));

/// Checks `config` as the app takes it, without applying it: its font,
/// whose failure fails the whole config, and its icon and its language,
/// which the log tells of when they are not Lokyr's, and whose defaults
/// then show.
pub fn check<B: Backend>(config: &Config) -> Result<(), String> {
    drawing::font::<B>(config.font.clone())?;
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

/// The application, which opens a window when it starts, and quits with its
/// last window or from its menu.
#[derive(Debug)]
pub struct App<B: Backend> {
    drawing: Option<Drawing<B>>,
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
    windows: BTreeMap<WindowId, Window<B>>,
    /// The timer of the next change of a blinking cursor, and its time.
    blink_timer: Option<(TimerId, Instant)>,
    /// The timer of the end of the first label of a grid's size that ends,
    /// and its time.
    label_timer: Option<(TimerId, Instant)>,
    /// The timer of the next look for the status of a program whose output
    /// ended.
    exit_timer: Option<TimerId>,
    /// The timer past which the startup's and the latency's benches give up
    /// waiting for a frame on the display.
    bench_deadline: Option<TimerId>,
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
    /// The config's global keys that the Mac's keyboards have, by the
    /// indices that their presses tell, with the commands that they run,
    /// those that another app holds too.
    global_keys: Vec<(keymap::Combo, Invocation)>,
    /// Whether the system gave the app one of the global keys at least,
    /// which keeps the app open without windows.
    global_held: bool,
    /// The measure of the startup, with `--bench startup`: when the first
    /// window's first frame showed, once it did, which the app prints
    /// before it quits.
    startup_bench: Option<Arc<Mutex<StartupBench>>>,
    /// The latency's bench, with `--bench latency`.
    latency_bench: Option<LatencyBench>,
    /// The output's bench, with `--bench output`.
    output_bench: Option<OutputBench>,
    /// The size of the output bench's window's content, in points, that
    /// `--window` gives.
    bench_size: Option<Size>,
}

/// The measure of the startup.
#[derive(Debug, Default)]
struct StartupBench {
    /// Whether a frame will tell when it shows.
    asked: bool,
    /// When the first frame showed.
    shown: Option<std::time::SystemTime>,
}

impl<B: Backend> App<B> {
    /// An application that draws its windows with `drawing`, or nothing
    /// without it, with `config`, which it reads again from `path`, its
    /// file, when that changes.
    pub fn new(drawing: Option<Drawing<B>>, config: Config, path: Option<PathBuf>) -> Self {
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
            bench_deadline: None,
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
                    version: env!("CARGO_PKG_VERSION").to_owned(),
                    ..Environment::default()
                }
            },
            budget,
            quick: None,
            global_keys: Vec::new(),
            global_held: false,
            startup_bench: None,
            latency_bench: None,
            output_bench: None,
            bench_size: None,
        }
    }

    /// The app whose programs learn `version` as Lokyr's, in place of the
    /// crate's: the app's own, with the date of its commit.
    pub fn with_version(mut self, version: &str) -> Self {
        self.environment.version = version.to_owned();
        self
    }

    /// The app that measures its startup, from the process's start to its
    /// first window's first frame on the display, prints it as JSON, and
    /// quits.
    pub fn with_startup_bench(mut self) -> Self {
        self.startup_bench = Some(Arc::default());
        self
    }

    /// The app that measures the latency of its keys, from each press to
    /// the frame that shows what it typed, in a window whose program
    /// echoes them, as a shell does, and prints it as JSON as it quits:
    /// of `keys` that it types itself, and then quits, or of the user's;
    /// with `flood`, beside a window that floods its output.
    pub fn with_latency_bench(mut self, keys: usize, flood: bool) -> Self {
        self.program = Program::command("/bin/sh", ["-c", "stty raw -echo; exec cat"]);
        self.latency_bench = Some(LatencyBench {
            keys,
            flood,
            ..LatencyBench::default()
        });
        self
    }

    /// The app that measures its frames' time on the CPU, by their phases,
    /// as `program`'s output draws in a window of `size` points, or of the
    /// config's columns and rows without one, and prints it as JSON, and
    /// quits, as the program ends.
    pub fn with_output_bench(mut self, program: Program, size: Option<Size>) -> Self {
        self.program = program;
        self.output_bench = Some(OutputBench::default());
        self.bench_size = size;
        self
    }

    /// Ends the output's bench, and the app, failing, when its program
    /// could not start, which then tells no exit.
    fn end_output_bench_unstarted(&mut self, platform: &mut dyn Platform) {
        if self.output_bench.is_none() {
            return;
        }
        let started = self.windows.values().any(|state| {
            self.workspace
                .pane(state.pane)
                .is_some_and(lokyr_kernel::workspace::Pane::started)
        });
        if !started {
            eprintln!("lokyr: the output's program cannot start, and so no frames to measure");
            crate::BENCH_FAILED.store(true, Ordering::Relaxed);
            self.output_bench = None;
            platform.quit();
        }
    }

    /// Prints the output's report, of `window`'s pixels, once.
    fn end_output_bench(&mut self, platform: &mut dyn Platform, window: WindowId) {
        let Some(bench) = self.output_bench.take() else {
            return;
        };
        let pixels = platform
            .window_size(window)
            .map_or((0, 0), |size| (size.pixels.width, size.pixels.height));
        println!("{}", output_bench::report(&bench.frames, pixels));
    }

    /// Starts the latency's bench, as its window opened: the window that
    /// floods, if it has one, and the first key a moment after, for the
    /// programs to start.
    fn start_latency_bench(&mut self, platform: &mut dyn Platform) {
        let measured = self.windows.keys().next().copied();
        let Some(bench) = &mut self.latency_bench else {
            return;
        };
        bench.window = measured;
        let flood = bench.flood;
        if flood {
            let program = Program::command("/bin/sh", ["-c", "exec yes 'flood flood flood flood'"]);
            self.open_window_running(platform, &program, None);
            if let Some(measured) = measured {
                platform.focus_window(measured);
            }
        }
        if let Some(bench) = &mut self.latency_bench
            && bench.keys > 0
        {
            bench.timer = Some(platform.start_timer(Instant::now() + Duration::from_millis(1500)));
        }
    }

    /// The bench's next key, when the last one showed or was given up; or
    /// its end, once it typed them all.
    fn latency_step(&mut self, platform: &mut dyn Platform) {
        let now = Instant::now();
        let Some(bench) = &mut self.latency_bench else {
            return;
        };
        bench.timer = None;
        if bench
            .typed_at
            .is_some_and(|typed| now.duration_since(typed) > latency::GIVE_UP)
        {
            bench.pending = None;
            bench.typed_at = None;
        }
        let waiting = bench.pending.is_some() || bench.typed_at.is_some();
        if bench.sent < bench.keys {
            if !waiting && let Some(window) = bench.window {
                let (code, letter) = latency::key_of(bench.sent);
                platform.post_key(window, code, &letter.to_string());
                bench.typed_at = Some(now);
                bench.sent += 1;
            }
            let pause = latency::pause(bench.sent);
            bench.timer = Some(platform.start_timer(now + pause));
            return;
        }
        if waiting {
            bench.timer = Some(platform.start_timer(now + Duration::from_millis(50)));
            return;
        }
        self.end_latency_bench(platform);
        platform.quit();
    }

    /// Prints the latency's report, once; of keys that it typed and none
    /// showed in a frame whose time the display told, as on a display that
    /// tells none, it says so.
    fn end_latency_bench(&mut self, platform: &mut dyn Platform) {
        let Some(bench) = self.latency_bench.take() else {
            return;
        };
        let refresh = bench
            .window
            .and_then(|window| platform.refresh_interval(window))
            .unwrap_or(Duration::from_micros(16_667));
        let samples = bench
            .samples
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if bench.keys > 0 && samples.is_empty() {
            eprintln!("lokyr: no frame showed on the display, and so no latency to measure");
        }
        println!(
            "{}",
            latency::report(&samples, refresh, bench.flood, bench.keys)
        );
    }

    /// Ends the startup's bench, and the latency's of the keys that it types,
    /// as the display showed no frame of them in [`BENCH_DEADLINE`], as on
    /// a display asleep or a machine without one: each says so, the
    /// latency's report of no samples, and the app quits. The latency of
    /// the user's own keys, or of keys that showed, goes on.
    fn give_up_benches(&mut self, platform: &mut dyn Platform) {
        let seconds = BENCH_DEADLINE.as_secs();
        if self.startup_bench.take().is_some() {
            eprintln!(
                "lokyr: no frame showed on the display in {seconds} s, and so no startup to measure"
            );
            platform.quit();
            return;
        }
        let shown = self.latency_bench.as_ref().is_none_or(|bench| {
            bench.keys == 0
                || !bench
                    .samples
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .is_empty()
        });
        if !shown {
            eprintln!("lokyr: no frame showed on the display in {seconds} s");
            self.end_latency_bench(platform);
            platform.quit();
        }
    }

    /// Prints the startup's measure and quits, once the first frame
    /// showed; quits at once without a drawing, which shows no frame.
    fn end_startup_bench(&mut self, platform: &mut dyn Platform) {
        let Some(bench) = &self.startup_bench else {
            return;
        };
        if self.drawing.is_none() {
            eprintln!("lokyr: no frame shows without a GPU, and so no startup to measure");
            self.startup_bench = None;
            platform.quit();
            return;
        }
        let (asked, shown) = {
            let bench = bench.lock().unwrap_or_else(PoisonError::into_inner);
            (bench.asked, bench.shown)
        };
        let Some(shown) = shown else {
            // The display skipped the frame: the windows draw again, whole.
            if !asked {
                for (&window, state) in &mut self.windows {
                    state.shown = None;
                    platform.request_frame(window);
                }
            }
            return;
        };
        let started = platform.process_started();
        let Some(elapsed) = started.and_then(|started| shown.duration_since(started).ok()) else {
            eprintln!("lokyr: the process's start cannot be read, and so the startup");
            self.startup_bench = None;
            platform.quit();
            return;
        };
        println!(
            "{{\"pid\":{},\"first_frame_ms\":{:.1}}}",
            std::process::id(),
            elapsed.as_secs_f64() * 1000.0
        );
        self.startup_bench = None;
        platform.quit();
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
        // The output bench's window takes its own size, or the config's
        // grid, never the frame that the last window left.
        let remember =
            window_config.remember_frame && page.is_none() && self.output_bench.is_none();
        let size = self
            .bench_size
            .unwrap_or_else(|| self.size_of(platform.screen_scale(), cells));
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
        let surfaces = Surfaces {
            window: self.surface(platform, window),
            text: self.layer_surface(platform, window, Layer::BlinkingText),
            cursor: self.layer_surface(platform, window, Layer::Cursor),
        };
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
        self.end_latency_bench(platform);
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
            // The latency's bench of the user's keys tells them as its
            // window closes.
            self.end_latency_bench(platform);
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
        let mut ended = Vec::new();
        let mut waiting = false;
        for (&window, state) in &self.windows {
            let Some(pane) = self.workspace.pane_mut(state.pane) else {
                continue;
            };
            // A man page's window keeps the page, from its top.
            let page = state.page.is_some();
            let close = if page { CloseOnExit::Never } else { close };
            match pane.exited(close, now, &self.texts) {
                Some(Exited::Close) => {
                    closing.push(window);
                    ended.push(window);
                }
                Some(Exited::Keep) => {
                    if page {
                        pane.scroll(Scroll::Top);
                    }
                    platform.request_frame(window);
                    ended.push(window);
                }
                None => waiting |= pane.is_waiting(),
            }
        }
        // The output's bench ends with its program.
        if self.output_bench.is_some()
            && let Some(&window) = ended.first()
        {
            self.end_output_bench(platform, window);
            platform.quit();
            return;
        }
        for window in closing {
            self.close(platform, window);
        }
        if waiting && self.exit_timer.is_none() {
            self.exit_timer = Some(platform.start_timer(now + EXIT_POLL));
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

    /// Takes the workspace's events. A program's write to the clipboard
    /// puts its text there, and the primary selection, which macOS lacks,
    /// takes nothing; its read asks the user, once at a time for each pane.
    /// A window takes the title of its active tab, which its program's
    /// gives; a bell does what `[bell]` says, and a program's notification
    /// shows as the system's. The tabs keep their programs' colors, which
    /// the windows do not show, and a program's end waits for its status,
    /// which the exits take.
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
    fn surface(&self, platform: &dyn Platform, window: WindowId) -> Option<B::Surface> {
        let drawing = self.drawing.as_ref()?;
        let handle = platform.window_handle(window)?;
        drawing
            .backend
            .surface(handle)
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
    ) -> Option<B::Surface> {
        let drawing = self.drawing.as_ref()?;
        let handle = platform.layer_handle(window, layer)?;
        drawing
            .backend
            .surface(handle)
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
        let started =
            (self.performance.is_some() || self.output_bench.is_some()).then(Instant::now);
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
        let copied = started.map(|_| Instant::now());
        if output {
            state.blink.restart(now);
        }
        // The press that this frame shows, which the latency's bench
        // measures as the frame shows.
        let shown_key = match &mut self.latency_bench {
            Some(bench) if output && bench.window == Some(window) => {
                bench.typed_at = None;
                bench
                    .pending
                    .take()
                    .map(|time| (time, Arc::clone(&bench.samples)))
            }
            _ => None,
        };
        // The pointer's moves without a button wake the app only for a
        // program that asked for them all.
        let moves = pane
            .shown_modes()
            .is_some_and(|modes| modes.mouse_tracking == MouseTracking::AnyEvent);
        if moves != state.mouse_moves {
            platform.track_mouse_moves(window, moves);
            state.mouse_moves = moves;
        }
        // The pointer is the text's I-beam, as the text selects; the arrow
        // over a program that takes the mouse's clicks.
        let takes_clicks = !pane.is_kept()
            && pane
                .shown_modes()
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
        // The layout finds the glyphs that the atlas lacks, which the font
        // draws together, on its threads, for a part of the frame's time at
        // most, and is done again with them; the cells of those left show
        // empty until the frames after draw them.
        let grid_cursor = cursor.filter(|_| !apart);
        font.collect_missing(GLYPHS_BUDGET.0, GLYPHS_BUDGET.1);
        let mut batch = cells::cells_with(
            snapshot,
            &cell,
            (x, y),
            selection.as_ref(),
            grid_cursor,
            look,
            font,
        );
        if font.draw_pending() {
            batch = cells::cells_with(
                snapshot,
                &cell,
                (x, y),
                selection.as_ref(),
                grid_cursor,
                look,
                font,
            );
        }
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
        let laid_out = started.map(|_| Instant::now());
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
        let listed = started.map(|_| Instant::now());
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
        if !same && let Some((pressed, samples)) = shown_key {
            drawing.renderer.on_next_presented(move |ago| {
                if let Some(shown) = ago.and_then(|ago| Instant::now().checked_sub(ago)) {
                    samples
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push(shown.saturating_duration_since(pressed));
                }
            });
        }
        // The startup's measure asks the first frame to tell as it shows.
        if !same
            && let Some(bench) = &self.startup_bench
            && !std::mem::replace(
                &mut bench.lock().unwrap_or_else(PoisonError::into_inner).asked,
                true,
            )
        {
            let (bench, waker) = (Arc::clone(bench), platform.waker());
            drawing.renderer.on_next_presented(move |ago| {
                let mut bench = bench.lock().unwrap_or_else(PoisonError::into_inner);
                match ago {
                    Some(ago) => bench.shown = std::time::SystemTime::now().checked_sub(ago),
                    // A frame that the display skipped: the next tells.
                    None => bench.asked = false,
                }
                waker.wake();
            });
        }
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
        // A frame that drew nothing leaves its benches' call to no other
        // surface's drawable, and the startup's asks the next frame.
        if presented.is_none() && !same {
            drawing.renderer.forget_next_presented();
            if let Some(bench) = &self.startup_bench {
                bench.lock().unwrap_or_else(PoisonError::into_inner).asked = false;
            }
        }
        // The frame's time ends here: the layers draw seldom, apart.
        let spent = started.map(|started| started.elapsed());
        // The output's bench: the CPU's time, but the wait for a drawable.
        if let (
            Some(bench),
            Some(started),
            Some(copied),
            Some(laid_out),
            Some(listed),
            Some(wait),
        ) = (
            &mut self.output_bench,
            started,
            copied,
            laid_out,
            listed,
            presented,
        ) {
            let ended = Instant::now();
            bench.frames.push(Phases {
                snapshot: copied.duration_since(started),
                layout: laid_out.duration_since(copied),
                list: listed.duration_since(laid_out),
                encode: ended.duration_since(listed).saturating_sub(wait),
                total: ended.duration_since(started).saturating_sub(wait),
            });
        }
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
        // The glyphs that the frame had no time for, in the next.
        if drawing.font.has_deferred() {
            platform.request_frame(window);
        }
        // Once the first frame is drawn, a thread readies the system's
        // fallback of fonts, whose first looks would hold up the first
        // frame of Chinese, Japanese, Korean or emoji by 10 to 20 ms.
        if !std::mem::replace(&mut drawing.warmed_up, true) {
            let family = drawing.font.family().to_owned();
            let _ = std::thread::Builder::new()
                .name("font-warm-up".into())
                .spawn(move || B::warm_up(&family));
        }
    }
}

impl<B: Backend> Handler for App<B> {
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
        // The benches leave the user's global keys to the user's Lokyr.
        if self.startup_bench.is_none()
            && self.latency_bench.is_none()
            && self.output_bench.is_none()
        {
            self.take_global_keys(platform);
        }
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
        self.end_output_bench_unstarted(platform);
        self.end_startup_bench(platform);
        self.start_latency_bench(platform);
        if self.startup_bench.is_some() || self.latency_bench.is_some() {
            self.bench_deadline = Some(platform.start_timer(Instant::now() + BENCH_DEADLINE));
        }
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
                self.end_startup_bench(platform);
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
            AppEvent::Timer(timer)
                if self
                    .latency_bench
                    .as_ref()
                    .is_some_and(|bench| bench.timer == Some(timer)) =>
            {
                self.latency_step(platform);
            }
            AppEvent::Timer(timer) if self.bench_deadline == Some(timer) => {
                self.bench_deadline = None;
                self.give_up_benches(platform);
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
            AppEvent::NotificationClicked(window) => self.notification_clicked(platform, window),
            AppEvent::AlertClosed {
                alert,
                button,
                suppressed,
                text,
            } => self.alert_closed(platform, alert, (button, suppressed), text.as_deref()),
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
            WindowEvent::Key(key) => self.key(platform, window, &key),
            WindowEvent::Ime(ime) => self.ime(platform, window, ime),
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
            WindowEvent::Dropped(dropped) => self.dropped(platform, window, dropped),
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
fn show_layer<B: Backend>(
    platform: &mut dyn Platform,
    drawing: &mut Drawing<B>,
    (window, layer): (WindowId, Layer),
    state: &mut WindowLayer<B>,
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
