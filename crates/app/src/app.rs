//! The application: its windows, what they draw, and its main menu.

use std::collections::BTreeMap;
use std::time::Instant;

use bolt_font_coretext::CoreText;
use bolt_platform_core::{
    AppEvent, Appearance, Handler, MainMenu, Menu, MenuItem, Platform, Shortcut, ShortcutKey, Size,
    TimerId, WindowAttributes, WindowEvent, WindowId, WindowSize,
};
use bolt_render_core::{CursorShape, DisplayList, Item, Renderer};
use bolt_render_metal::{Gpu, MetalRenderer, Surface};
use bolt_text_core::FontConfig;
use bolt_ui_core::cursor::{self, Blink, CursorConfig};
use bolt_ui_core::font::Font;
use bolt_ui_core::pane::{self, CursorLook};
use bolt_ui_core::theme::Theme;
use bolt_vt_core::parser::Parser;
use bolt_vt_core::terminal::{Colors, Options, Snapshot, Terminal};

/// The command that quits.
const QUIT: &str = "quit";

/// The size of a new window's content, in points.
const WINDOW_SIZE: Size = Size {
    width: 800.0,
    height: 500.0,
};

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
        let font = Font::new(CoreText::new(), config, 1.0).map_err(|error| error.to_string())?;
        if let Some(family) = font.missing_family() {
            eprintln!(
                "bolt: the font family `{family}` is not installed; the text takes `{}`",
                font.family()
            );
        }
        let (coverage, color) = font.texture_sizes();
        let renderer =
            MetalRenderer::new(&gpu, coverage, color).map_err(|error| error.to_string())?;
        Ok(Self {
            gpu,
            renderer,
            font,
        })
    }
}

/// A window that the app opened.
#[derive(Debug)]
struct Window {
    /// What it draws into, once the app draws and the platform has a layer.
    surface: Option<Surface>,
    /// What it shows, and the columns, rows and appearance that it was made
    /// for.
    sample: Option<(u16, u16, Appearance, Snapshot)>,
    /// Whether it has the keyboard's focus.
    focused: bool,
    /// The clock of its cursor's blinking.
    blink: Blink,
    /// Whether its cursor blinked at its last frame.
    blinking: bool,
}

impl Window {
    fn new(surface: Option<Surface>, focused: bool, now: Instant) -> Self {
        Self {
            surface,
            sample: None,
            focused,
            blink: Blink::new(now),
            blinking: false,
        }
    }

    /// The text that the window shows at `columns` by `rows`, in the
    /// theme's variant of `appearance`, made again when one of them changed.
    fn sample(
        &mut self,
        columns: u16,
        rows: u16,
        appearance: Appearance,
        theme: &Theme,
    ) -> &Snapshot {
        let stale = !matches!(&self.sample, Some((c, r, a, _)) if (*c, *r, *a) == (columns, rows, appearance));
        if stale {
            let colors = theme.variant(appearance == Appearance::Dark).colors.clone();
            self.sample = Some((columns, rows, appearance, sample(columns, rows, colors)));
        }
        &self.sample.as_ref().expect("made above").3
    }
}

/// The application, which opens a window when it starts, and quits with its
/// last window or from its menu.
#[derive(Debug, Default)]
pub struct App {
    drawing: Option<Drawing>,
    theme: Theme,
    cursor: CursorConfig,
    windows: BTreeMap<WindowId, Window>,
    /// The timer of the next change of a blinking cursor, and its time.
    blink_timer: Option<(TimerId, Instant)>,
}

impl App {
    /// An application that draws its windows with `drawing`, or nothing
    /// without it, with the cursor of `cursor`.
    pub fn new(drawing: Option<Drawing>, cursor: CursorConfig) -> Self {
        Self {
            drawing,
            theme: Theme::builtin(),
            cursor,
            windows: BTreeMap::new(),
            blink_timer: None,
        }
    }

    fn open_window(&mut self, platform: &mut dyn Platform) {
        let attributes = WindowAttributes {
            title: "Bolt".to_owned(),
            size: WINDOW_SIZE,
        };
        match platform.create_window(&attributes) {
            Ok(window) => {
                let surface = self.surface(platform, window);
                let focused = platform.focused_window() == Some(window);
                self.windows
                    .insert(window, Window::new(surface, focused, Instant::now()));
                if let Some(size) = platform.window_size(window) {
                    self.draw(platform, window, size);
                }
            }
            Err(error) => {
                eprintln!("bolt: {error}");
                if self.windows.is_empty() {
                    platform.quit();
                }
            }
        }
    }

    /// The surface of `window`, if the app draws and the platform has a
    /// layer for it.
    fn surface(&self, platform: &dyn Platform, window: WindowId) -> Option<Surface> {
        let drawing = self.drawing.as_ref()?;
        let handle = platform.window_handle(window)?;
        Surface::new(&drawing.gpu, handle)
            .inspect_err(|error| eprintln!("bolt: {error}"))
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
        let program = state
            .sample
            .as_ref()
            .map(|(.., snapshot)| snapshot.cursor())
            .unwrap_or_default();
        let shape = cursor::shape(&program, &self.cursor, state.focused);
        let (on, next) = if cursor::blinks(&program, &self.cursor, state.focused) {
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

    /// Draws `window`, of `size`: the grid of cells that fits its content,
    /// what the window shows in it, and the cursor.
    fn draw(&mut self, platform: &mut dyn Platform, window: WindowId, size: WindowSize) {
        let shape = self.cursor(platform, window, Instant::now());
        let config = self.cursor;
        let (Some(drawing), Some(state)) = (self.drawing.as_mut(), self.windows.get_mut(&window))
        else {
            return;
        };
        if state.surface.is_none() {
            return;
        }
        drawing.font.set_scale(size.scale as f32);
        drawing.font.begin_frame();
        let cell = *drawing.font.cell();
        let cursor = shape.map(|shape| CursorLook {
            shape,
            thickness: config.thickness(&cell, size.scale as f32),
        });
        let fit =
            |pixels: u32, cell: u32| (pixels / cell.max(1)).clamp(1, u32::from(u16::MAX)) as u16;
        let (columns, rows) = (
            fit(size.pixels.width, cell.width),
            fit(size.pixels.height, cell.height),
        );
        let snapshot = state.sample(columns, rows, platform.appearance(), &self.theme);
        let font = &mut drawing.font;
        let batch = pane::cells(snapshot, &cell, 0, 0, None, cursor, &mut |shown| {
            font.glyph(shown)
        });
        let list = DisplayList {
            background: pane::background(snapshot),
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
            eprintln!("bolt: {error}");
        }
    }
}

impl Handler for App {
    fn started(&mut self, platform: &mut dyn Platform) {
        platform.set_main_menu(&main_menu());
        self.open_window(platform);
    }

    fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
        match event {
            AppEvent::Command(command) if command == QUIT => platform.quit(),
            AppEvent::QuitRequested => platform.quit(),
            // What woke the application changed what a window shows.
            AppEvent::Wake => {
                for &window in self.windows.keys() {
                    platform.request_frame(window);
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
            AppEvent::AppearanceChanged(_) => {
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
            WindowEvent::Focused(focused) => {
                if let Some(state) = self.windows.get_mut(&window) {
                    state.focused = focused;
                    if focused {
                        state.blink.restart(Instant::now());
                    }
                    platform.request_frame(window);
                }
            }
            WindowEvent::Key(_) => {
                if let Some(state) = self.windows.get_mut(&window) {
                    state.blink.restart(Instant::now());
                    platform.request_frame(window);
                }
            }
            WindowEvent::CloseRequested => {
                // The surface goes before its window.
                self.windows.remove(&window);
                platform.close_window(window);
                if self.windows.is_empty() {
                    platform.quit();
                }
            }
            _ => {}
        }
    }
}

/// A snapshot of a terminal of `columns` by `rows` that shows the sample
/// text, in `colors`.
fn sample(columns: u16, rows: u16, colors: Colors) -> Snapshot {
    let mut terminal = Terminal::new(columns, rows, Options::default());
    terminal.set_default_colors(colors);
    Parser::new().advance(&mut terminal, sample_text().as_bytes());
    let mut snapshot = Snapshot::default();
    terminal.snapshot(&mut snapshot);
    snapshot
}

/// The text that a window shows: the palette's sixteen colors on the
/// default background and as backgrounds, the styles, the 256 colors, direct
/// color, and code.
fn sample_text() -> String {
    let mut text = String::from(
        "\x1b[1mBolt\x1b[0m: cells drawn by Metal, from a snapshot of a terminal.\r\n\r\n",
    );
    for index in 0..16 {
        text.push_str(&format!("\x1b[38;5;{index}m{index:>3} "));
    }
    text.push_str("\x1b[0m\r\n");
    for index in 0..16 {
        text.push_str(&format!("\x1b[48;5;{index}m{index:>3} "));
    }
    text.push_str("\x1b[0m\r\n\r\n");
    text.push_str("\x1b[1mbold\x1b[0m \x1b[3mitalic\x1b[0m \x1b[1;3mbold italic\x1b[0m \x1b[7minverse\x1b[0m\r\n\r\n");
    for index in 16..=231 {
        text.push_str(&format!("\x1b[48;5;{index}m "));
        if (index - 16) % 36 == 35 {
            text.push_str("\x1b[0m\r\n");
        }
    }
    for index in 232..=255 {
        text.push_str(&format!("\x1b[48;5;{index}m  "));
    }
    text.push_str("\x1b[0m\r\n");
    for step in 0..64 {
        let red = step * 4;
        text.push_str(&format!("\x1b[48;2;{red};128;{}m ", 255 - red));
    }
    text.push_str("\x1b[0m\r\n\r\n");
    text.push_str("fn main() { println!(\"Hello, {}!\", \"world\"); } // 0123456789 <>=!&|");
    text
}

/// The main menu: the application's menu, with Quit.
fn main_menu() -> MainMenu {
    MainMenu {
        menus: vec![Menu {
            title: "Bolt".to_owned(),
            items: vec![MenuItem::Command {
                title: "Quit Bolt".to_owned(),
                command: QUIT.to_owned(),
                shortcut: Some(Shortcut {
                    key: ShortcutKey::Char('q'),
                    command: true,
                    shift: false,
                    alt: false,
                    control: false,
                }),
            }],
        }],
    }
}

#[cfg(test)]
mod tests {
    use bolt_platform_core::fake::Fake;

    use super::*;

    fn started() -> (Fake, App) {
        let mut fake = Fake::default();
        let mut app = App::default();
        fake.start(&mut app);
        (fake, app)
    }

    #[test]
    fn starting_opens_a_window_under_a_menu_with_quit() {
        let (fake, _) = started();
        let windows: Vec<_> = fake.windows().collect();
        assert_eq!(windows.len(), 1);
        assert_eq!(fake.window(windows[0]).unwrap().size.points, WINDOW_SIZE);
        let menu = fake.main_menu().unwrap();
        assert!(matches!(
            &menu.menus[0].items[0],
            MenuItem::Command { command, shortcut: Some(Shortcut { key: ShortcutKey::Char('q'), command: true, .. }), .. }
                if command == QUIT
        ));
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

    #[test]
    fn the_sample_takes_the_appearance_s_variant() {
        let theme = Theme::builtin();
        let mut window = Window::new(None, false, Instant::now());
        let dark = window.sample(80, 24, Appearance::Dark, &theme);
        assert_eq!((dark.cols(), dark.rows()), (80, 24));
        assert_eq!(dark.colors().background, theme.dark.colors.background);
        let first: *const Snapshot = window.sample(80, 24, Appearance::Dark, &theme);
        assert!(std::ptr::eq(
            first,
            window.sample(80, 24, Appearance::Dark, &theme)
        ));
        let light = window.sample(80, 24, Appearance::Light, &theme);
        assert_eq!(light.colors().background, theme.light.colors.background);
        assert_eq!(window.sample(81, 24, Appearance::Light, &theme).cols(), 81);
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
        let steady_bar = CursorConfig {
            shape: cursor::Shape::Bar,
            blink: false,
            thickness: None,
        };
        let mut app = App::new(None, steady_bar);
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        fake.send(&mut app, window, WindowEvent::Focused(true));
        let half = Instant::now() + cursor::BLINK_INTERVAL + cursor::BLINK_INTERVAL / 2;
        assert_eq!(app.cursor(&mut fake, window, half), Some(CursorShape::Bar));
        assert_eq!(fake.timers().count(), 0);
    }

    #[test]
    fn other_commands_do_nothing() {
        let (mut fake, mut app) = started();
        fake.send_app(&mut app, AppEvent::Command("unknown".to_owned()));
        assert!(!fake.has_quit());
        assert_eq!(fake.windows().count(), 1);
    }
}
