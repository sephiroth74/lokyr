//! The application: its windows, each with the pane of its program, what
//! they draw, its main menu, and its config, which it reads again when the
//! file changes.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use lokyr_font_coretext::CoreText;
use lokyr_kernel::commands::{Args, Category, Code, Command, Error, Registry};
use lokyr_kernel::config::{AppConfig, ClipboardRead, Config};
use lokyr_kernel::keymap::{
    self, Binding, Context, Invocation, Keymap, Pressed, Resolved, Resolver,
};
use lokyr_kernel::l10n::{self, Texts};
#[cfg(test)]
use lokyr_kernel::workspace::RESIZE_PACE;
use lokyr_kernel::workspace::{self, Environment, Event, Exited, Grid, Pane, PaneId, Workspace};
use lokyr_platform_core::{
    Alert, AlertId, AlertStyle, AppEvent, Appearance, Backdrop, Handler, Ime, KeyCode, KeyEvent,
    KeyState, MainMenu, Menu, MenuItem, Modifiers, MouseButton, Platform, Point, ScrollDelta,
    ScrollPhase, Shortcut, ShortcutKey, Size, TimerId, WindowAttributes, WindowEvent, WindowId,
    WindowSize,
};
use lokyr_pty::Program;
use lokyr_render_core::{Color, CursorShape, DisplayList, Item, Renderer};
use lokyr_render_metal::{Gpu, MetalRenderer, Surface};
use lokyr_text_core::FontConfig;
use lokyr_ui_core::cursor::{self, Blink};
use lokyr_ui_core::font::Font;
use lokyr_ui_core::pane::{self as cells, CursorLook, Preedit, TextLook};
use lokyr_ui_core::theme::Theme;
use lokyr_vt_core::input::{Key, KeyOptions, MouseAction, MouseEvent, Wheel};
use lokyr_vt_core::terminal::{self as vt, ClipboardRequest, MouseEncoding, MouseTracking, Scroll};

use crate::environment;
use crate::icon::Icons;
use crate::keys;
use crate::mouse::{self, Mouse, Pointer};

use crate::watch::Watcher;

/// The command that quits.
const QUIT: &str = "app.quit";

/// The command that opens a window.
const NEW_WINDOW: &str = "window.new";

/// The catalogs of the texts that the app shows, the kernel's and its own,
/// by language.
const CATALOGS: [(&str, &str); 4] = [
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

/// How long the app waits after the last change of the config file, for
/// the changes that come with it, as an editor's write and rename do, before
/// it reads the file once.
const SETTLE: Duration = Duration::from_millis(100);

/// How often the app looks for the status of a program whose output ended.
const EXIT_POLL: Duration = Duration::from_millis(20);

/// The cell of a window that the app does not draw, in pixels.
const UNDRAWN_CELL: (u32, u32) = (8, 16);

/// What draws the windows: the GPU, its renderer, and the font of the text.
#[derive(Debug)]
pub struct Drawing {
    gpu: Gpu,
    renderer: MetalRenderer,
    font: Font<CoreText>,
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
        Ok(())
    }
}

/// The font that `config` gives, which tells of a family that is not
/// installed.
fn font(config: FontConfig) -> Result<Font<CoreText>, String> {
    let font = Font::new(CoreText::new(), config, 1.0).map_err(|error| error.to_string())?;
    if let Some(family) = font.missing_family() {
        eprintln!(
            "lokyr: the font family `{family}` is not installed; the text takes `{}`",
            font.family()
        );
    }
    if let Some(family) = font.missing_non_ascii_family() {
        eprintln!(
            "lokyr: the font family `{family}` is not installed; the text past ASCII takes `{}`",
            font.family()
        );
    }
    Ok(font)
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

/// A window that the app opened.
#[derive(Debug)]
struct Window {
    /// What it draws into, once the app draws and the platform has a layer.
    surface: Option<Surface>,
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
    /// The backdrop that the platform has for it, while it is translucent.
    backdrop: Option<Backdrop>,
    /// The mouse over it.
    mouse: Mouse,
    /// Whether the platform tells the pointer's moves without a button, as
    /// a program asked for them.
    mouse_moves: bool,
    /// The sequence of keys under way, as the keymap resolves it.
    resolver: Resolver,
    /// The keys of the sequence under way, which go to the program if the
    /// next key does not go on with it.
    held_back: Vec<KeyEvent>,
    /// The keys whose presses the keymap took, whose releases go nowhere,
    /// each once.
    taken: Vec<KeyCode>,
}

impl Window {
    fn new(
        surface: Option<Surface>,
        (window, pane): (workspace::WindowId, PaneId),
        focused: bool,
        now: Instant,
    ) -> Self {
        Self {
            surface,
            window,
            pane,
            preedit: None,
            focused,
            blink: Blink::new(now),
            blinking: false,
            backdrop: None,
            mouse: Mouse::default(),
            mouse_moves: false,
            resolver: Resolver::new(),
            held_back: Vec::new(),
            taken: Vec::new(),
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
    /// The config in force.
    config: Config,
    /// The file of the config, if it has one.
    config_file: Option<ConfigFile>,
    /// The texts that the app shows, in the language shown.
    texts: Texts,
    /// The user's languages, as the system has them, from the first.
    preferred: Vec<String>,
    /// The commands, which the app runs.
    commands: Registry,
    /// The bindings of keys to commands: the defaults, and the config's.
    keymap: Keymap,
    /// The windows, their tabs and panes.
    workspace: Workspace,
    /// The platform's windows, each with what the app keeps of it.
    windows: BTreeMap<WindowId, Window>,
    /// The timer of the next change of a blinking cursor, and its time.
    blink_timer: Option<(TimerId, Instant)>,
    /// The timer of the next look for the status of a program whose output
    /// ended.
    exit_timer: Option<TimerId>,
    /// The timer of the next size that a program learns, at its pace, and
    /// its time.
    resize_timer: Option<(TimerId, Instant)>,
    /// The icon's variants, where the app finds them.
    icons: Option<Icons>,
    /// The variant that the app made the icon, once it made one.
    icon_shown: Option<String>,
    /// The programs' reads of the clipboard that the user is asked about,
    /// by their alerts.
    clipboard_asks: BTreeMap<AlertId, (PaneId, ClipboardRequest)>,
    /// The environment of the programs that the app starts, which the
    /// start completes with the user's locale.
    environment: Environment,
}

impl App {
    /// An application that draws its windows with `drawing`, or nothing
    /// without it, with `config`, which it reads again from `path`, its
    /// file, when that changes.
    pub fn new(drawing: Option<Drawing>, config: Config, path: Option<PathBuf>) -> Self {
        let keymap = Keymap::with_config(default_bindings(), &config.keys);
        Self {
            drawing,
            theme: Theme::builtin(),
            program: Program::LoginShell,
            config,
            config_file: path.map(|path| ConfigFile {
                path,
                watcher: None,
                changed: Arc::new(AtomicBool::new(false)),
                reading: None,
            }),
            commands: commands(&Texts::new(l10n::SOURCE, &CATALOGS)),
            texts: Texts::new(l10n::SOURCE, &CATALOGS),
            preferred: Vec::new(),
            keymap,
            workspace: Workspace::new(),
            windows: BTreeMap::new(),
            blink_timer: None,
            exit_timer: None,
            resize_timer: None,
            icons: Icons::find(),
            icon_shown: None,
            clipboard_asks: BTreeMap::new(),
            environment: Environment {
                resources: environment::resources(),
                ..Environment::default()
            },
        }
    }

    /// The app, whose windows run `program` rather than the login shell.
    #[cfg(test)]
    fn with_program(mut self, program: Program) -> Self {
        self.program = program;
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
                eprintln!("lokyr: {error}");
                eprintln!("lokyr: none of the config applies, and the one in force stays");
            }
        }
    }

    /// Takes `config` in place of the one in force, whole, in every window;
    /// or nothing of it, if its font fails.
    fn apply(&mut self, platform: &mut dyn Platform, config: Config) {
        if config == self.config {
            return;
        }
        if config.font != self.config.font
            && let Some(drawing) = &mut self.drawing
            && let Err(error) = drawing.set_font(config.font.clone())
        {
            eprintln!("lokyr: {error}");
            eprintln!("lokyr: none of the config applies, and the one in force stays");
            return;
        }
        let icon_changed = config.app.icon != self.config.app.icon;
        let language_changed = config.app.language != self.config.app.language;
        if config.keys != self.config.keys {
            self.keymap = Keymap::with_config(default_bindings(), &config.keys);
        }
        self.config = config;
        let (left, right) = self.config.keyboard.option_as_meta.sides();
        platform.set_option_as_meta(left, right);
        for pane in self.workspace.panes() {
            pane.set_scroll_on_output(self.config.scrollback.scroll_on_output);
            pane.set_clipboard_reads(self.config.clipboard.read == ClipboardRead::Ask);
        }
        if icon_changed {
            self.show_icon(platform);
        }
        if language_changed {
            self.texts = texts(&self.config.app.language, &self.preferred);
            self.commands = commands(&self.texts);
            platform.set_main_menu(&main_menu(&self.commands));
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
        let variants = icons.variants();
        let wanted = &self.config.app.icon;
        let variant = if variants.contains(wanted) {
            wanted.as_str()
        } else {
            eprintln!(
                "lokyr: the icon `{wanted}` is not one of Lokyr's: {}; the default one shows",
                variants.join(", ")
            );
            AppConfig::DEFAULT_ICON
        };
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
        let scale = platform.window_size(window)?.scale;
        let padding = self.padding(scale);
        Some(mouse::pointer(position, scale, padding, pane.grid()))
    }

    /// A button of the mouse went down or up over `window`: the program
    /// learns of it, as its mouse modes have it, at the screen's row, and
    /// not of a press above the screen, in a view scrolled up. Shift keeps
    /// a press for the terminal, which selects with it; a release goes to
    /// the program whenever it learned of the press, so that no button stays
    /// down for it.
    fn mouse_button(
        &mut self,
        platform: &dyn Platform,
        window: WindowId,
        button: MouseButton,
        pressed: bool,
        position: Point,
        mods: Modifiers,
    ) {
        let Some(button) = mouse::button(button) else {
            return;
        };
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

    /// The pointer moved over `window`: a program that asked learns of it,
    /// once a cell, or a pixel for reports in pixels, at the screen's row,
    /// and not above the screen.
    fn mouse_moved(
        &mut self,
        platform: &dyn Platform,
        window: WindowId,
        position: Point,
        mods: Modifiers,
    ) {
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
        let attributes = WindowAttributes {
            title: "Lokyr".to_owned(),
            size: WINDOW_SIZE,
        };
        match platform.create_window(&attributes) {
            Ok(window) => {
                let surface = self.surface(platform, window);
                let focused = platform.focused_window() == Some(window);
                let size = platform
                    .window_size(window)
                    .unwrap_or_else(|| WindowSize::new(WINDOW_SIZE, 1.0));
                let grid = self.grid(size);
                let colors = &self.colors(platform.appearance());
                let waker = platform.waker();
                let pane = Pane::spawn(
                    &self.program,
                    &self.environment,
                    grid,
                    colors,
                    &self.texts,
                    move || {
                        waker.wake();
                    },
                );
                pane.set_scroll_on_output(self.config.scrollback.scroll_on_output);
                pane.set_clipboard_reads(self.config.clipboard.read == ClipboardRead::Ask);
                let id = pane.id();
                let ids = (self.workspace.open_window(pane), id);
                self.windows
                    .insert(window, Window::new(surface, ids, focused, Instant::now()));
                self.draw(platform, window, size);
            }
            Err(error) => {
                eprintln!("lokyr: {error}");
                if self.windows.is_empty() {
                    platform.quit();
                }
            }
        }
    }

    /// Closes `window`, and quits with the last one.
    fn close(&mut self, platform: &mut dyn Platform, window: WindowId) {
        // The surface and the session go before the window, and the asks of
        // its program's reads with it.
        if let Some(state) = self.windows.remove(&window) {
            drop(state.surface);
            self.clipboard_asks
                .retain(|_, (pane, _)| *pane != state.pane);
            self.workspace.close_window(state.window);
        }
        platform.close_window(window);
        if self.windows.is_empty() {
            platform.quit();
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
            match pane.exited(close, now, &self.texts) {
                Some(Exited::Close) => closing.push(window),
                Some(Exited::Keep) => platform.request_frame(window),
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
    /// once the registry took them; an error goes to the log.
    fn run(&mut self, platform: &mut dyn Platform, name: &str, args: Args) {
        let ran = self.commands.check(name, args).and_then(|_| match name {
            QUIT => {
                platform.quit();
                Ok(())
            }
            NEW_WINDOW => {
                self.open_window(platform);
                Ok(())
            }
            _ => Err(Error::new(
                Code::Failed,
                format!("`{name}` has no action in the app"),
            )),
        });
        if let Err(error) = ran {
            eprintln!("lokyr: {error}");
        }
    }

    /// Takes the workspace's events. A program's write to the clipboard
    /// puts its text there, and the primary selection, which macOS lacks,
    /// takes nothing; its read asks the user, once at a time for each pane.
    /// The tabs keep their programs' titles and colors, which the windows do
    /// not show; the bells go unanswered, and a program's end waits for its
    /// status, which the exits take.
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
                Event::WindowOpened(_)
                | Event::WindowClosed(_)
                | Event::TitleChanged(_)
                | Event::Bell(_)
                | Event::Ended(_)
                | Event::Terminal { .. } => {}
            }
        }
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
        };
        let alert = platform.show_alert(Some(window), &alert);
        self.clipboard_asks.insert(alert, (pane, request));
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
        let (width, height) = match &mut self.drawing {
            Some(drawing) => {
                drawing.font.set_scale(size.scale as f32);
                let cell = drawing.font.cell();
                (cell.width, cell.height)
            }
            None => UNDRAWN_CELL,
        };
        let fit = |pixels: u32, padding: u32, cell: u32| {
            (pixels.saturating_sub(2 * padding) / cell.max(1)).clamp(1, u32::from(u16::MAX)) as u16
        };
        let pixels = |cell: u32| u16::try_from(cell).unwrap_or(u16::MAX);
        Grid {
            columns: fit(size.pixels.width, x, width),
            rows: fit(size.pixels.height, y, height),
            cell_width: pixels(width),
            cell_height: pixels(height),
        }
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

    /// The shape that the cursor of `window` takes at `now`, or `None` while
    /// its blinking hides it; a cursor that blinks keeps the timer of its
    /// next change running.
    fn cursor(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        now: Instant,
    ) -> Option<CursorShape> {
        let state = self.windows.get_mut(&window)?;
        let program = self.workspace.pane(state.pane)?.snapshot().cursor();
        let config = &self.config.cursor;
        let shape = cursor::shape(&program, config, state.focused);
        let (on, next) = if cursor::blinks(&program, config, state.focused) {
            state.blink.at(now)
        } else {
            (true, None)
        };
        // A cursor that stopped blinking stays on, and takes no timer.
        state.blinking = next.is_some();
        if let Some(next) = next {
            self.blink_at(platform, next);
        }
        on.then_some(shape)
    }

    /// Keeps the timer of the blinking cursors at `at`.
    fn blink_at(&mut self, platform: &mut dyn Platform, at: Instant) {
        if self.blink_timer.is_some_and(|(_, current)| current == at) {
            return;
        }
        if let Some((timer, _)) = self.blink_timer.take() {
            platform.cancel_timer(timer);
        }
        self.blink_timer = Some((platform.start_timer(at), at));
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
        let grid = self.grid(size);
        let (left, top) = self.padding(size.scale);
        let now = Instant::now();
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        let Some(pane) = self.workspace.pane_mut(state.pane) else {
            return;
        };
        let due = pane.resize(grid, now);
        // Output starts the cursor blinking again, on, as a key does.
        if pane.update() {
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
        // The input method's candidates go at the cursor's cell.
        let at = pane.snapshot().cursor().position;
        let (width, height) = (f64::from(grid.cell_width), f64::from(grid.cell_height));
        platform.set_text_area(
            window,
            Point {
                x: (f64::from(left) + f64::from(at.x) * width) / size.scale,
                y: (f64::from(top) + f64::from(at.y) * height) / size.scale,
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
        if let Some(due) = due {
            self.pace_at(platform, due);
        }
        let shape = self.cursor(platform, window, Instant::now());
        // Text that blinks takes the cursor's clock, in the window that has
        // the focus, and stops shown with it.
        let text = self.config.text;
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        let Some(pane) = self.workspace.pane(state.pane) else {
            return;
        };
        let blinking = text.blink && state.focused && cells::has_blinking_text(pane.snapshot());
        let (blink_shown, next) = if blinking {
            state.blink.at(Instant::now())
        } else {
            (true, None)
        };
        if let Some(next) = next {
            state.blinking = true;
            self.blink_at(platform, next);
        }
        let look = TextLook {
            config: text,
            blink_shown,
        };
        let config = self.config.cursor;
        let (Some(drawing), Some(state)) = (self.drawing.as_mut(), self.windows.get_mut(&window))
        else {
            return;
        };
        let Some(pane) = self.workspace.pane(state.pane) else {
            return;
        };
        if state.surface.is_none() {
            return;
        }
        drawing.font.begin_frame();
        drawing.font.set_text(self.config.text);
        let cell = *drawing.font.cell();
        let cursor = shape.map(|shape| CursorLook {
            shape,
            thickness: config.thickness(&cell, size.scale as f32),
        });
        let snapshot = pane.snapshot();
        let font = &mut drawing.font;
        let (x, y) = (
            i32::try_from(left).unwrap_or(i32::MAX),
            i32::try_from(top).unwrap_or(i32::MAX),
        );
        let mut batch = cells::cells_with(snapshot, &cell, (x, y), None, cursor, look, font);
        if let Some(preedit) = &state.preedit {
            cells::preedit(&mut batch, snapshot, &cell, preedit, font);
        }
        // Over a backdrop, what the cells leave shows it.
        let opacity = if backdrop.is_some() { 0.0 } else { 1.0 };
        let list = DisplayList {
            background: background.with_alpha(opacity),
            items: vec![Item::Cells(batch)],
        };
        let updates = drawing.font.texture_updates();
        let Some(surface) = &state.surface else {
            return;
        };
        surface.resize(size.pixels.width, size.pixels.height, size.scale);
        let drawn = drawing
            .renderer
            .update_textures(&updates)
            .and_then(|()| drawing.renderer.draw(surface, &list));
        if let Err(error) = drawn {
            eprintln!("lokyr: {error}");
        }
    }
}

impl Handler for App {
    fn started(&mut self, platform: &mut dyn Platform) {
        self.preferred = platform.locales();
        self.texts = texts(&self.config.app.language, &self.preferred);
        self.commands = commands(&self.texts);
        platform.set_main_menu(&main_menu(&self.commands));
        let (left, right) = self.config.keyboard.option_as_meta.sides();
        platform.set_option_as_meta(left, right);
        self.show_icon(platform);
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
            AppEvent::QuitRequested => platform.quit(),
            // What woke the application changed what a window shows, ended
            // its program, or changed the config file.
            AppEvent::Wake => {
                self.workspace.collect();
                self.events(platform);
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
                for &window in self.windows.keys() {
                    platform.request_frame(window);
                }
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
            AppEvent::Timer(timer) if self.exit_timer == Some(timer) => {
                self.exit_timer = None;
                self.exits(platform);
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
            // The user answered a program's read of the clipboard: its
            // second button allows it.
            AppEvent::AlertClosed { alert, button } => {
                if let Some((pane, request)) = self.clipboard_asks.remove(&alert)
                    && button == 1
                    && let Some(pane) = self.workspace.pane(pane)
                {
                    pane.reply_clipboard(request, &platform.clipboard_text().unwrap_or_default());
                }
            }
            AppEvent::AppearanceChanged(appearance) => {
                let colors = self.colors(appearance);
                for pane in self.workspace.panes() {
                    pane.set_colors(&colors);
                }
                let windows: Vec<WindowId> = self.windows.keys().copied().collect();
                for window in windows {
                    if let Some(size) = platform.window_size(window) {
                        self.draw(platform, window, size);
                    }
                }
            }
            _ => {}
        }
    }

    fn window_event(&mut self, platform: &mut dyn Platform, window: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::Resized(size) | WindowEvent::ScaleChanged(size) => {
                self.draw(platform, window, size);
            }
            WindowEvent::Frame => {
                if let Some(size) = platform.window_size(window) {
                    self.draw(platform, window, size);
                }
            }
            // The focus, and a key, start the cursor blinking again, on.
            // A window that loses the focus forgets the keys that the keymap
            // took, whose releases go elsewhere.
            WindowEvent::Focused(focused) => {
                if let Some(state) = self.windows.get_mut(&window) {
                    state.focused = focused;
                    if focused {
                        state.blink.restart(Instant::now());
                    } else {
                        state.taken.clear();
                    }
                    platform.request_frame(window);
                }
            }
            // A key goes to the program, and starts the cursor blinking
            // again, on; a key that types or acts, not a modifier, a lock,
            // Fn or a Command key that no menu took, closes a window whose
            // program ended.
            WindowEvent::Key(key) => {
                let options = keys::options(self.config.keyboard.option_as_meta);
                let Some(state) = self.windows.get_mut(&window) else {
                    return;
                };
                let Some(pane) = self.workspace.pane(state.pane) else {
                    return;
                };
                let pressed = key.state != KeyState::Released;
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
                ..
            } => self.mouse_button(platform, window, button, pressed, position, mods),
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
            WindowEvent::CloseRequested => self.close(platform, window),
            _ => {}
        }
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
/// keyboard, and its program may show the alternate screen.
fn context(pane: Option<&Pane>) -> Context {
    let mut names = vec!["pane.focused"];
    if pane.is_some_and(Pane::is_alternate_screen) {
        names.push("alternate-screen");
    }
    Context::new(names)
}

/// The default bindings of the app's commands.
fn default_bindings() -> Vec<Binding> {
    [("cmd+n", NEW_WINDOW), ("cmd+q", QUIT)]
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

/// The app's commands, which it runs itself, with their titles in `texts`.
fn commands(texts: &Texts) -> Registry {
    let mut registry = Registry::new();
    let commands = [
        (NEW_WINDOW, "command-window-new", Category::Windows),
        (QUIT, "command-app-quit", Category::App),
    ];
    for (name, title, category) in commands {
        let command = Command {
            name: name.to_owned(),
            title: texts.text(title),
            category,
            capabilities: Vec::new(),
            params: Vec::new(),
        };
        if let Err(error) = registry.register(command) {
            eprintln!("lokyr: {error}");
        }
    }
    registry
}

/// The main menu: the application's menu, with New Window and Quit, by the
/// titles of their commands in `commands`.
fn main_menu(commands: &Registry) -> MainMenu {
    let title = |name: &str| {
        commands
            .get(name)
            .map_or_else(|| name.to_owned(), |command| command.title.clone())
    };
    MainMenu {
        menus: vec![Menu {
            title: "Lokyr".to_owned(),
            items: vec![
                MenuItem::Command {
                    title: title(NEW_WINDOW),
                    command: NEW_WINDOW.to_owned(),
                    shortcut: Some(Shortcut {
                        key: ShortcutKey::Char('n'),
                        command: true,
                        shift: false,
                        alt: false,
                        control: false,
                    }),
                },
                MenuItem::Command {
                    title: title(QUIT),
                    command: QUIT.to_owned(),
                    shortcut: Some(Shortcut {
                        key: ShortcutKey::Char('q'),
                        command: true,
                        shift: false,
                        alt: false,
                        control: false,
                    }),
                },
            ],
        }],
    }
}

#[cfg(test)]
mod tests {
    use lokyr_kernel::config::CloseOnExit;
    use lokyr_platform_core::fake::Fake;
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
    fn starting_opens_a_window_under_a_menu_with_new_window_and_quit() {
        let (fake, _) = started();
        let windows: Vec<_> = fake.windows().collect();
        assert_eq!(windows.len(), 1);
        assert_eq!(fake.window(windows[0]).unwrap().size.points, WINDOW_SIZE);
        let menu = fake.main_menu().unwrap();
        let shortcuts: Vec<(String, char)> = menu.menus[0]
            .items
            .iter()
            .filter_map(|item| match item {
                MenuItem::Command {
                    command,
                    shortcut:
                        Some(Shortcut {
                            key: ShortcutKey::Char(key),
                            command: true,
                            ..
                        }),
                    ..
                } => Some((command.clone(), *key)),
                _ => None,
            })
            .collect();
        assert_eq!(
            shortcuts,
            [(NEW_WINDOW.to_owned(), 'n'), (QUIT.to_owned(), 'q')]
        );
    }

    #[test]
    fn the_app_s_commands_register_with_their_categories_and_titles() {
        let registry = commands(&Texts::new("en", &CATALOGS));
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
                ("app.quit", "Quit Lokyr", Category::App),
                ("window.new", "New Window", Category::Windows),
            ]
        );
        let menu = main_menu(&registry);
        let titles: Vec<&str> = menu.menus[0]
            .items
            .iter()
            .filter_map(|item| match item {
                MenuItem::Command { title, .. } => Some(title.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(titles, ["New Window", "Quit Lokyr"]);
        // A command that takes no argument rejects one, and does nothing.
        let mut args = Args::new();
        args.insert(
            "now".to_owned(),
            lokyr_kernel::commands::Value::Boolean(true),
        );
        let error = registry.check(QUIT, args.clone()).unwrap_err();
        assert_eq!(error.code.number(), 102);
        let (mut fake, mut app) = started();
        app.run(&mut fake, QUIT, args);
        assert!(!fake.has_quit());
        app.run(&mut fake, QUIT, Args::new());
        assert!(fake.has_quit());
    }

    /// The titles of the items of the main menu that `fake` has.
    fn menu_titles(fake: &Fake) -> Vec<String> {
        fake.main_menu().unwrap().menus[0]
            .items
            .iter()
            .filter_map(|item| match item {
                MenuItem::Command { title, .. } => Some(title.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_menus_take_the_config_s_language_and_change_with_it() {
        let (mut fake, mut app, path) = with_file("language", "[app]\nlanguage = \"it\"\n");
        assert_eq!(menu_titles(&fake), ["Nuova finestra", "Esci da Lokyr"]);
        std::fs::write(&path, "[app]\nlanguage = \"en\"\n").unwrap();
        change(&mut fake, &mut app);
        assert_eq!(menu_titles(&fake), ["New Window", "Quit Lokyr"]);
        assert_eq!(app.texts.language(), "en");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn the_system_s_language_shows_unless_the_config_names_one() {
        let mut fake = Fake::default();
        fake.set_locales(&["de_CH", "it_CH"]);
        let mut first = app(Config::default(), quiet());
        fake.start(&mut first);
        assert_eq!(menu_titles(&fake), ["Nuova finestra", "Esci da Lokyr"]);
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
        assert_eq!(menu_titles(&fake), ["New Window", "Quit Lokyr"]);
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
        for command in commands(&texts).commands() {
            assert!(!command.title.starts_with("command-"), "{}", command.title);
        }
    }

    #[test]
    fn two_windows_run_two_shells() {
        let mut fake = Fake::default();
        let mut app = app(Config::default(), shell("echo pid:$$; sleep 10"));
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

    #[test]
    fn quit_from_the_menu_or_from_the_system_quits() {
        let (mut fake, mut app) = started();
        fake.send_app(&mut app, AppEvent::Command(QUIT.to_owned()));
        assert!(fake.has_quit());

        let (mut fake, mut app) = started();
        fake.send_app(&mut app, AppEvent::QuitRequested);
        assert!(fake.has_quit());
    }

    #[test]
    fn closing_the_last_window_quits() {
        let (mut fake, mut app) = started();
        let window = fake.windows().next().unwrap();
        fake.send(&mut app, window, WindowEvent::CloseRequested);
        assert_eq!(fake.windows().count(), 0);
        assert!(fake.has_quit());
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
    fn a_wake_asks_each_window_for_a_frame() {
        let (mut fake, mut app) = started();
        let window = fake.windows().next().unwrap();
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
        assert_eq!(app.cursor(&mut fake, window, now), None);
        assert_eq!(
            app.cursor(&mut fake, window, now + cursor::BLINK_INTERVAL),
            Some(CursorShape::Block)
        );
        assert_eq!(fake.timers().count(), 1);
    }

    #[test]
    fn blinking_stops_on_after_its_timeout() {
        let (mut fake, mut app, window) = focused();
        let later = Instant::now() + cursor::BLINK_TIMEOUT;
        assert_eq!(
            app.cursor(&mut fake, window, later),
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
            assert_eq!(app.cursor(&mut fake, window, at), Some(CursorShape::Hollow));
        }
        assert!(!app.windows[&window].blinking);
    }

    #[test]
    fn the_configuration_gives_the_cursor_s_shape_and_blinking() {
        let mut fake = Fake::default();
        let mut config = Config::default();
        config.cursor = CursorConfig {
            shape: cursor::Shape::Bar,
            blink: false,
            thickness: None,
        };
        let mut app = app(config, quiet());
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        fake.send(&mut app, window, WindowEvent::Focused(true));
        let half = Instant::now() + cursor::BLINK_INTERVAL + cursor::BLINK_INTERVAL / 2;
        assert_eq!(app.cursor(&mut fake, window, half), Some(CursorShape::Bar));
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

    #[test]
    fn a_window_shows_its_program_s_output() {
        let mut fake = Fake::default();
        let mut app = app(Config::default(), shell("printf 'hello'; sleep 10"));
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        run_until(&mut fake, &mut app, |_, app| row(app, window, 0) == "hello");
        // At the window's grid: 1600 by 1000 pixels, less 8 of padding at
        // each edge, of cells of 8 by 16.
        let snapshot = app.pane(window).snapshot();
        assert_eq!((snapshot.cols(), snapshot.rows()), (198, 61));
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
        // A window that closes takes its ask with it.
        fake.send(&mut app, window, WindowEvent::CloseRequested);
        assert!(app.clipboard_asks.is_empty());
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
    fn unpadded() -> Config {
        let mut config = Config::default();
        config.window.padding_x = 0.0.try_into().unwrap();
        config.window.padding_y = 0.0.try_into().unwrap();
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
        // A reloaded padding gives the grid that it leaves.
        std::fs::write(&path, "[window]\npadding-x = 0\npadding-y = 0\n").unwrap();
        change(&mut fake, &mut app);
        assert!(fake.window(window).unwrap().frame_requested);
        fake.send(&mut app, window, WindowEvent::Frame);
        let snapshot = app.pane(window).snapshot();
        assert_eq!((snapshot.cols(), snapshot.rows()), (200, 62));
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
            assert_eq!(fake.timers().collect::<Vec<_>>(), [(timer, at)]);
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
        app.cursor(&mut fake, window, later);
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
}
