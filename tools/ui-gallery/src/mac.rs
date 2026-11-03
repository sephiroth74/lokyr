//! The gallery's window on the Mac, of the platform's backend, the Metal
//! renderer and CoreText's fonts, as the app draws its chrome; and its
//! images, drawn offscreen.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::time::Instant;

use lokyr_font_coretext::CoreText;
use lokyr_platform_core::{
    AppEvent, Appearance, ContextMenu, Handler, Ime, KeyCode, KeyEvent, KeyState, MainMenu, Menu,
    MenuItem, Modifiers, MouseButton, Platform, Point, PointerArea, PointerShape, ScrollDelta,
    Shortcut, ShortcutKey, Size, StandardAction, Sweep, SweepShape, TimerId, WindowAttributes,
    WindowEvent, WindowId, WindowSize,
};
use lokyr_plugin_sdk::view::{self, UiEventKind};
use lokyr_render_core::{Color, DisplayList, Renderer, Surface as _};
use lokyr_render_metal::{Gpu, MetalRenderer, Surface as Drawable};
use lokyr_text_core::FontConfig;
use lokyr_ui_core::field::Clipboard;
use lokyr_ui_core::font::Font;
use lokyr_ui_core::panel::MenuEntry;
use lokyr_ui_core::pointer::Button;
use lokyr_ui_core::progress;
use lokyr_ui_core::theme::Theme;
use lokyr_ui_core::tree::{NodeId, Tree};
use lokyr_vt_core::input::{self as vt, KeyAction, Keypad, Mods};

use crate::gallery::Gallery;
use crate::view::{Keyed, MAIN, View};

/// The command of the menu's Quit.
const QUIT: &str = "gallery.quit";

/// The window's size, in points.
const SIZE: Size = Size {
    width: 1120.0,
    height: 860.0,
};

/// The points of a line of a wheel's scroll, the interface text's line.
const LINE: f64 = 16.0;

/// What draws the gallery: the GPU, its renderer and the fonts.
struct Drawing {
    gpu: Gpu,
    renderer: MetalRenderer,
    font: Font<CoreText>,
}

impl Drawing {
    fn new(scale: f32) -> Result<Self, String> {
        let gpu = Gpu::new().map_err(|error| error.to_string())?;
        let font = Font::new(CoreText::new(), FontConfig::default(), scale)
            .map_err(|error| error.to_string())?;
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

/// The gallery's window, and what draws it.
struct App {
    drawing: Option<Drawing>,
    view: Option<View>,
    window: Option<(WindowId, Drawable)>,
    size: Option<WindowSize>,
    areas: Vec<PointerArea>,
    dark: bool,
    /// The timer of the tooltip's delay, and when it fires.
    timer: Option<(TimerId, Instant)>,
    /// The indeterminate progresses' sweeps that the window shows, and
    /// when they started.
    sweeps: (Vec<Sweep>, Instant),
}

impl App {
    /// The window's room for the view: its width and height in pixels,
    /// below the titlebar, and the titlebar's height.
    fn room(&self) -> Option<(f32, f32, f32)> {
        let size = self.size?;
        let bar = size.bar_pixels() as f32;
        Some((
            size.pixels.width as f32,
            (size.pixels.height as f32 - bar).max(0.0),
            bar,
        ))
    }

    /// `position`, in points from the content's corner, in the view's
    /// pixels from its root's.
    fn at(&self, position: Point) -> Option<(f32, f32)> {
        let size = self.size?;
        Some((
            (position.x * size.scale) as f32,
            (position.y * size.scale) as f32 - size.bar_pixels() as f32,
        ))
    }

    fn open(&mut self, platform: &mut dyn Platform) -> Result<(), String> {
        let attributes = WindowAttributes {
            title: "Lokyr's UI gallery".to_owned(),
            size: SIZE,
            remember: false,
            restore: false,
            restore_size: false,
        };
        let window = platform
            .create_window(&attributes)
            .map_err(|error| error.to_string())?;
        let size = platform
            .window_size(window)
            .ok_or("the window has no size")?;
        let drawing = Drawing::new(size.scale as f32)?;
        let handle = platform
            .window_handle(window)
            .ok_or("the window has no view")?;
        let surface = Drawable::new(&drawing.gpu, handle).map_err(|error| error.to_string())?;
        platform.track_mouse_moves(window, true);
        platform.set_appearance(Some(if self.dark {
            Appearance::Dark
        } else {
            Appearance::Light
        }));
        let mut view = View::new(Gallery::new(self.dark), size.scale as f32);
        // A menu's panel of menu items shows as the system's own menu.
        view.native_menus = true;
        self.view = Some(view);
        self.drawing = Some(drawing);
        self.window = Some((window, surface));
        self.size = Some(size);
        platform.request_frame(window);
        Ok(())
    }

    fn resized(&mut self, platform: &mut dyn Platform, size: WindowSize) {
        let scale = size.scale as f32;
        if let (Some(view), Some(drawing)) = (&mut self.view, &mut self.drawing)
            && view.main.layout.scale() != scale
        {
            view.set_scale(scale);
            drawing.font.set_scale(scale);
        }
        self.size = Some(size);
        self.request_frame(platform);
    }

    fn request_frame(&mut self, platform: &mut dyn Platform) {
        if let Some((window, _)) = &self.window {
            platform.request_frame(*window);
        }
        self.show_native(platform);
    }

    /// Shows the native menu that a menu's panel became, if one waits, at
    /// its place: its items' choices come back as the window's context
    /// commands.
    fn show_native(&mut self, platform: &mut dyn Platform) {
        let (Some(view), Some((window, _)), Some(size)) = (&mut self.view, &self.window, self.size)
        else {
            return;
        };
        let Some((entries, at)) = view.take_native() else {
            return;
        };
        let mut menu = ContextMenu::default();
        menu.items = menu_items(&entries, &mut menu);
        let position = Point {
            x: f64::from(at.0) / size.scale,
            y: (f64::from(at.1) + size.bar_pixels() as f64) / size.scale,
        };
        platform.show_context_menu(*window, position, &menu);
    }

    fn frame(&mut self, platform: &mut dyn Platform) {
        let Some((width, height, bar)) = self.room() else {
            return;
        };
        let (Some(view), Some(drawing), Some((window, surface)), Some(size)) =
            (&mut self.view, &mut self.drawing, &self.window, self.size)
        else {
            return;
        };
        drawing.font.set_drawing(drawing.renderer.drawing_from());
        view.lay_out(&mut drawing.font, (width, Some(height)));
        // The transitions that the layout started, which end at once where
        // the system reduces motion.
        let now = Instant::now();
        view.animate(now, platform.reduce_motion());
        let room = (size.pixels.width as f32, size.pixels.height as f32);
        let theme = Theme::builtin();
        let variant = theme.variant(view.gallery.dark());
        drawing.font.begin_frame();
        let items = view.paint(
            &mut drawing.font,
            &variant.tokens,
            ((0.0, bar), room),
            (view.gallery.dark(), now),
        );
        let background = variant.colors.background;
        let list = DisplayList {
            background: Color::from_rgb8(background.r, background.g, background.b),
            items,
            frame: drawing.font.atlas_frame(),
        };
        let updates = drawing.font.texture_updates();
        surface.resize(size.pixels.width, size.pixels.height, size.scale);
        let drawn = drawing
            .renderer
            .update_textures(&updates)
            .and_then(|()| drawing.renderer.draw(surface, &list));
        if let Err(error) = drawn {
            eprintln!("ui-gallery: {error}");
        }
        // A frame at each refresh of the display while a transition runs,
        // and none once they end.
        if view.animating(now) {
            platform.request_frame(*window);
        }
        let scale = size.scale;
        let points = |pixels: f32| f64::from(pixels) / scale;
        let place = |rect: lokyr_render_core::Rect| {
            (
                Point {
                    x: points(rect.x as f32),
                    y: points(rect.y as f32 + bar),
                },
                Size {
                    width: points(rect.width as f32),
                    height: points(rect.height as f32),
                },
            )
        };
        // The pointer's shapes over the view, which the window server
        // shows as it moves.
        let areas: Vec<PointerArea> = view
            .areas()
            .into_iter()
            .map(|area| {
                let (origin, size) = place(area.rect);
                PointerArea {
                    origin,
                    size,
                    shape: match area.shape {
                        view::PointerShape::Arrow => PointerShape::Arrow,
                        view::PointerShape::Text => PointerShape::Text,
                        view::PointerShape::PointingHand => PointerShape::PointingHand,
                    },
                }
            })
            .collect();
        if areas != self.areas {
            platform.set_pointer_areas(*window, &areas);
            self.areas = areas;
        }
        // The indeterminate progresses, which the window server moves.
        let sweeps: Vec<Sweep> = view
            .sweeps(&variant.tokens)
            .into_iter()
            .map(|area| {
                let (origin, size) = place(area.rect);
                Sweep {
                    origin,
                    size,
                    within: place(area.visible),
                    radius: points(area.radius),
                    shape: match area.shape {
                        progress::SweepShape::Bar { width } => SweepShape::Bar {
                            width: points(width),
                        },
                        progress::SweepShape::Ring { span, thickness } => SweepShape::Ring {
                            span: f64::from(span),
                            thickness: points(thickness),
                        },
                    },
                    color: [
                        f64::from(area.color.red),
                        f64::from(area.color.green),
                        f64::from(area.color.blue),
                        f64::from(area.color.alpha),
                    ],
                    period: area.period,
                    start: self.sweeps.1,
                }
            })
            .collect();
        if sweeps != self.sweeps.0 {
            platform.set_sweeps(*window, &sweeps);
            self.sweeps.0 = sweeps;
        }
        // The input method's rectangle, of the field that has the focus,
        // by which it places its candidates and the palette of emoji.
        if let Some(area) = view.text_area(&mut drawing.font) {
            platform.set_text_area(
                *window,
                Point {
                    x: points(area.x),
                    y: points(area.y + bar),
                },
                Size {
                    width: points(area.width),
                    height: points(area.height),
                },
            );
        }
    }

    fn mouse(
        &mut self,
        platform: &mut dyn Platform,
        position: Point,
        press: Option<(MouseButton, bool, Modifiers, u32)>,
    ) {
        let Some(at) = self.at(position) else {
            return;
        };
        let (Some(view), Some(drawing)) = (&mut self.view, &mut self.drawing) else {
            return;
        };
        match press {
            None => view.moved(at, &mut drawing.font, Instant::now()),
            Some((button, pressed, mods, clicks)) => {
                let button = match button {
                    MouseButton::Left if mods.control => Button::Secondary,
                    MouseButton::Left => Button::Primary,
                    MouseButton::Right => Button::Secondary,
                    MouseButton::Middle => Button::Middle,
                    _ => return,
                };
                if pressed {
                    // A press keeps a field's composition as it stands,
                    // which the input method then forgets.
                    let kept = view.pressed(at, (button, clicks, mods.shift), &mut drawing.font);
                    if kept && let Some((window, _)) = &self.window {
                        platform.end_composition(*window);
                    }
                } else {
                    view.released(at, (button, clicks));
                }
            }
        }
        let deadline = self.view.as_ref().and_then(View::deadline);
        self.wake_at(platform, deadline);
        self.request_frame(platform);
    }

    /// Wakes at `deadline` for the tooltip that waits, by the one timer,
    /// moved or cancelled as the deadline changes.
    fn wake_at(&mut self, platform: &mut dyn Platform, deadline: Option<Instant>) {
        if self.timer.map(|(_, at)| at) == deadline {
            return;
        }
        if let Some((timer, _)) = self.timer.take() {
            platform.cancel_timer(timer);
        }
        if let Some(at) = deadline {
            self.timer = Some((platform.start_timer(at), at));
        }
    }

    /// The tooltip's timer fired: the tooltip shows, if its time came.
    fn timer(&mut self, platform: &mut dyn Platform, timer: TimerId) {
        if self.timer.is_none_or(|(waited, _)| waited != timer) {
            return;
        }
        self.timer = None;
        let Some(view) = &mut self.view else {
            return;
        };
        if view.tick(Instant::now()) {
            self.request_frame(platform);
        }
        let deadline = self.view.as_ref().and_then(View::deadline);
        self.wake_at(platform, deadline);
    }

    fn scrolled(&mut self, platform: &mut dyn Platform, position: Point, delta: (f32, f32)) {
        let Some(at) = self.at(position) else {
            return;
        };
        let Some(view) = &mut self.view else {
            return;
        };
        view.scrolled(at, delta);
        self.wake_at(platform, None);
        self.request_frame(platform);
    }

    fn key(&mut self, platform: &mut dyn Platform, event: &KeyEvent) {
        let pressed = event.state != KeyState::Released;
        let Some(key) = vt_key(event) else {
            return;
        };
        let Some(view) = &mut self.view else {
            return;
        };
        // A key hides the tooltips.
        if pressed {
            let shown = view.dismiss_tooltips();
            self.wake_at(platform, None);
            if shown {
                self.request_frame(platform);
            }
        }
        let Some(view) = &mut self.view else {
            return;
        };
        // Tab reaches the buttons and the toggles as the system's keyboard
        // navigation has it, which it reads at each key.
        let every_control = platform.keyboard_navigation();
        let keyed = view.key(&key, &mut Pasteboard(&mut *platform), every_control);
        if keyed == Keyed::Left && pressed && event.mods.super_key && event.code == KeyCode::KeyD {
            self.dark = !self.dark;
            platform.set_appearance(Some(if self.dark {
                Appearance::Dark
            } else {
                Appearance::Light
            }));
            view.gallery.toggle_dark();
            view.render();
        }
        self.request_frame(platform);
    }

    /// What the input method composes or types, into the field with the
    /// focus.
    fn ime(&mut self, platform: &mut dyn Platform, ime: Ime) {
        let Some(view) = &mut self.view else {
            return;
        };
        match ime {
            Ime::Preedit { text, selection } => view.ime(&text, selection, false),
            Ime::Commit(text) => view.ime(&text, 0..0, true),
        }
        self.request_frame(platform);
    }
}

/// The platform's items of a native menu's `entries`, and the items that
/// `menu` dims and ticks, of their nodes' keys as their commands.
fn menu_items(entries: &[MenuEntry], menu: &mut ContextMenu) -> Vec<MenuItem> {
    entries
        .iter()
        .map(|entry| match entry {
            MenuEntry::Item {
                key,
                title,
                ticked,
                event,
            } => {
                if event.is_none() {
                    menu.dimmed.push(key.0.clone());
                }
                if *ticked {
                    menu.ticked.push(key.0.clone());
                }
                MenuItem::Command {
                    title: title.clone(),
                    command: key.0.clone(),
                    shortcut: None,
                }
            }
            MenuEntry::Separator => MenuItem::Separator,
            MenuEntry::Submenu { title, entries } => MenuItem::Submenu(Menu {
                title: title.clone(),
                items: menu_items(entries, menu),
                role: None,
            }),
        })
        .collect()
}

/// The system's clipboard, as a field cuts, copies and pastes.
struct Pasteboard<'a>(&'a mut dyn Platform);

impl Clipboard for Pasteboard<'_> {
    fn read(&mut self) -> Option<String> {
        self.0.clipboard_text()
    }

    fn write(&mut self, text: &str) {
        self.0.set_clipboard_text(text);
    }
}

/// `event`, as the focus and the fields take it: the keys that move and
/// edit, and those that type a character; `None` for the others.
fn vt_key(event: &KeyEvent) -> Option<vt::KeyEvent<'_>> {
    let key = match event.code {
        KeyCode::Tab => vt::Key::Tab,
        KeyCode::Escape => vt::Key::Escape,
        KeyCode::Enter => vt::Key::Enter,
        KeyCode::NumpadEnter => vt::Key::Keypad(Keypad::Enter),
        KeyCode::Backspace => vt::Key::Backspace,
        KeyCode::Delete => vt::Key::Delete,
        KeyCode::Insert => vt::Key::Insert,
        KeyCode::Home => vt::Key::Home,
        KeyCode::End => vt::Key::End,
        KeyCode::PageUp => vt::Key::PageUp,
        KeyCode::PageDown => vt::Key::PageDown,
        KeyCode::ArrowUp => vt::Key::Up,
        KeyCode::ArrowDown => vt::Key::Down,
        KeyCode::ArrowLeft => vt::Key::Left,
        KeyCode::ArrowRight => vt::Key::Right,
        KeyCode::F(n) => vt::Key::F(n),
        _ => vt::Key::Char(event.plain.chars().next().filter(|c| !c.is_control())?),
    };
    Some(vt::KeyEvent {
        key,
        action: match event.state {
            KeyState::Pressed => KeyAction::Press,
            KeyState::Repeated => KeyAction::Repeat,
            KeyState::Released => KeyAction::Release,
        },
        mods: Mods {
            shift: event.mods.shift,
            control: event.mods.control,
            left_alt: event.mods.left_alt,
            right_alt: event.mods.right_alt,
            super_key: event.mods.super_key,
            ..Mods::default()
        },
        text: &event.text,
        unmodified: &event.unmodified,
        base_layout: None,
    })
}

impl Handler for App {
    fn started(&mut self, platform: &mut dyn Platform) {
        self.dark = platform.appearance() == Appearance::Dark;
        platform.set_main_menu(&main_menu());
        if let Err(error) = self.open(platform) {
            eprintln!("ui-gallery: {error}");
            platform.quit();
        }
    }

    fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
        match event {
            AppEvent::QuitRequested => platform.quit(),
            AppEvent::Command(command) if command == QUIT => platform.quit(),
            AppEvent::Timer(timer) => self.timer(platform, timer),
            _ => {}
        }
    }

    fn window_event(&mut self, platform: &mut dyn Platform, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::Resized(size) | WindowEvent::ScaleChanged(size) => {
                self.resized(platform, size);
            }
            WindowEvent::Frame => self.frame(platform),
            WindowEvent::CloseRequested => platform.quit(),
            WindowEvent::MouseMoved { position, .. } => self.mouse(platform, position, None),
            WindowEvent::MouseButton {
                button,
                pressed,
                position,
                mods,
                clicks,
            } => self.mouse(platform, position, Some((button, pressed, mods, clicks))),
            WindowEvent::Key(event) => self.key(platform, &event),
            WindowEvent::Ime(ime) => self.ime(platform, ime),
            WindowEvent::ContextCommand(command) => {
                if let Some(view) = &mut self.view {
                    view.chosen(&command);
                }
                self.request_frame(platform);
            }
            WindowEvent::Scroll {
                delta, position, ..
            } => {
                // A line of the wheel is a line of the interface's text.
                let (x, y) = match delta {
                    ScrollDelta::Lines { x, y } => (x * LINE, y * LINE),
                    ScrollDelta::Points { x, y } => (x, y),
                };
                self.scrolled(platform, position, (x as f32, y as f32));
            }
            WindowEvent::Focused(false) => {
                if let Some(view) = &mut self.view {
                    view.cancel();
                }
                self.wake_at(platform, None);
                self.request_frame(platform);
            }
            _ => {}
        }
    }
}

/// The gallery's menus: the application's, to hide it and quit, and the
/// Edit menu's palette of emoji and symbols, which types into a field.
fn main_menu() -> MainMenu {
    let keys = |key: char, control: bool| {
        Some(Shortcut {
            key: ShortcutKey::Char(key),
            command: true,
            shift: false,
            alt: false,
            control,
        })
    };
    MainMenu {
        menus: vec![
            Menu {
                title: "UI gallery".to_owned(),
                items: vec![
                    MenuItem::Standard {
                        title: "Hide UI gallery".to_owned(),
                        action: StandardAction::Hide,
                        shortcut: keys('h', false),
                    },
                    MenuItem::Separator,
                    MenuItem::Command {
                        title: "Quit UI gallery".to_owned(),
                        command: QUIT.to_owned(),
                        shortcut: keys('q', false),
                    },
                ],
                role: None,
            },
            Menu {
                title: "Edit".to_owned(),
                items: vec![MenuItem::Standard {
                    title: "Emoji & Symbols".to_owned(),
                    action: StandardAction::CharacterPalette,
                    shortcut: keys(' ', true),
                }],
                role: None,
            },
        ],
    }
}

/// Runs the gallery's window until it closes.
pub fn run() -> Result<(), String> {
    let app = App {
        drawing: None,
        view: None,
        window: None,
        size: None,
        areas: Vec::new(),
        dark: false,
        timer: None,
        sweeps: (Vec::new(), Instant::now()),
    };
    lokyr_platform_macos::run(app).map_err(|error| error.to_string())
}

/// Draws the gallery offscreen at `scale`, light and dark, the pointer
/// over a tab's close, the focus on a node, and the find bar and the note
/// open, into PNGs in `directory`.
pub fn images(directory: &Path, scale: f32) -> Result<(), String> {
    std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let mut drawing = Drawing::new(scale)?;
    for dark in [false, true] {
        let mut gallery = Gallery::new(dark);
        gallery.event("tab", UiEventKind::HoverEnter);
        gallery.event("tab.close", UiEventKind::HoverEnter);
        gallery.event("hand", UiEventKind::Click);
        // The find bar and the note open over the page.
        gallery.event("open-find", UiEventKind::Click);
        gallery.event("open-note", UiEventKind::Click);
        let mut view = View::new(gallery, scale);
        if let Some(second) = find(&view.main.tree, "second") {
            view.focus.focus(MAIN, &view.main.tree, second);
        }
        let width = SIZE.width as f32 * scale;
        view.lay_out(&mut drawing.font, (width, None));
        let frame = |view: &View| {
            view.main
                .tree
                .root()
                .and_then(|root| view.main.layout.frame(&view.main.tree, root))
                .ok_or("the gallery has no root")
        };
        // As high as the page that scrolls, all of it shown.
        let page = find(&view.main.tree, "page").ok_or("the gallery has no page")?;
        let room = view.main.layout.scroll_room(&view.main.tree, page);
        let height = frame(&view)?.height + room;
        view.lay_out(&mut drawing.font, (width, Some(height)));
        let root = frame(&view)?;
        let theme = Theme::builtin();
        let variant = theme.variant(dark);
        drawing.font.begin_frame();
        let items = view.paint(
            &mut drawing.font,
            &variant.tokens,
            ((0.0, 0.0), (width, f32::MAX)),
            (dark, Instant::now()),
        );
        let background = variant.colors.background;
        let list = DisplayList {
            background: Color::from_rgb8(background.r, background.g, background.b),
            items,
            frame: drawing.font.atlas_frame(),
        };
        let updates = drawing.font.texture_updates();
        drawing
            .renderer
            .update_textures(&updates)
            .map_err(|error| error.to_string())?;
        let (width, height) = (root.width as u32, root.height.ceil() as u32);
        let pixels = drawing
            .renderer
            .draw_offscreen(&list, width, height)
            .map_err(|error| error.to_string())?;
        let name = if dark {
            "gallery-dark.png"
        } else {
            "gallery-light.png"
        };
        write_png(&directory.join(name), (width, height), &pixels)?;
    }
    Ok(())
}

/// The node of `key` in `tree`, if it has one.
fn find(tree: &Tree, key: &str) -> Option<NodeId> {
    fn walk(tree: &Tree, id: NodeId, key: &str) -> Option<NodeId> {
        if tree.key(id).is_some_and(|found| found.0 == key) {
            return Some(id);
        }
        tree.children(id)
            .iter()
            .find_map(|&child| walk(tree, child, key))
    }
    walk(tree, tree.root()?, key)
}

/// Writes `pixels`, RGBA, of `width` by `height`, as a PNG at `path`.
fn write_png(path: &Path, (width, height): (u32, u32), pixels: &[u8]) -> Result<(), String> {
    let file = File::create(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(pixels))
        .map_err(|error| format!("{}: {error}", path.display()))
}
