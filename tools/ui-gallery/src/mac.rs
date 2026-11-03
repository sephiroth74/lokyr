//! The gallery's window on the Mac, of the platform's backend, the Metal
//! renderer and CoreText's fonts, as the app draws its chrome; and its
//! images, drawn offscreen.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use lokyr_font_coretext::CoreText;
use lokyr_platform_core::{
    AppEvent, Appearance, Handler, KeyCode, KeyEvent, KeyState, Modifiers, MouseButton, Platform,
    Point, PointerArea, PointerShape, Size, WindowAttributes, WindowEvent, WindowId, WindowSize,
};
use lokyr_plugin_sdk::view::{self, UiEventKind};
use lokyr_render_core::{Color, DisplayList, Renderer, Surface as _};
use lokyr_render_metal::{Gpu, MetalRenderer, Surface};
use lokyr_text_core::FontConfig;
use lokyr_ui_core::focus::{Focus, Routed, Target};
use lokyr_ui_core::font::Font;
use lokyr_ui_core::layout::Layout;
use lokyr_ui_core::paint::paint;
use lokyr_ui_core::pointer::{self, Button, NodeEvent, Pointer};
use lokyr_ui_core::theme::Theme;
use lokyr_ui_core::tree::{NodeId, Tree};
use lokyr_vt_core::input::{self as vt, KeyAction, Mods};

use crate::gallery::Gallery;

/// The gallery's one panel.
const PANEL: u8 = 0;

/// The window's size, in points.
const SIZE: Size = Size {
    width: 1120.0,
    height: 860.0,
};

/// The rounds of events that a change of the tree can bring, as a hover
/// that moves what is under the pointer, past which it waits for the next.
const ROUNDS: usize = 4;

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

/// The gallery's view, kept and laid out, and what the pointer and the
/// keyboard do over it.
struct View {
    gallery: Gallery,
    tree: Tree,
    layout: Layout,
    pointer: Pointer,
    focus: Focus<u8>,
}

impl View {
    fn new(gallery: Gallery, scale: f32) -> Self {
        let mut view = Self {
            gallery,
            tree: Tree::new(),
            layout: Layout::new(scale),
            pointer: Pointer::new(),
            focus: Focus::new(),
        };
        view.render();
        view
    }

    /// Takes the gallery's view anew, diffed against the last.
    fn render(&mut self) {
        let Ok(changes) = self.tree.update(&self.gallery.view()) else {
            return;
        };
        self.layout.update(&self.tree, &changes);
        self.focus.updated(PANEL, &self.tree);
    }

    /// Lays the view out in `width` pixels, and as high as the window
    /// leaves it, `height`.
    fn lay_out(&mut self, font: &mut Font<CoreText>, width: f32, height: Option<f32>) {
        self.layout.compute(&self.tree, Some(width), height, font);
    }

    /// Hands `events` to the gallery, and takes its view anew, with the
    /// events that the new layout brings, a few rounds at most.
    fn apply(
        &mut self,
        font: &mut Font<CoreText>,
        (width, height): (f32, f32),
        events: Vec<NodeEvent>,
    ) {
        let mut events = events;
        for _ in 0..ROUNDS {
            if events.is_empty() {
                return;
            }
            for event in &events {
                self.gallery.event(&event.key.0, event.kind);
            }
            self.render();
            self.lay_out(font, width, Some(height));
            events = self.pointer.laid_out(&self.tree, &self.layout);
        }
    }

    /// The node with the keyboard's focus.
    fn focused(&self) -> Option<NodeId> {
        match self.focus.target() {
            Target::Node { node, .. } => Some(node),
            Target::Pane => None,
        }
    }

    /// Tells the gallery where the focus is.
    fn tell_focus(&mut self) {
        let key = self
            .focused()
            .and_then(|node| self.tree.key(node))
            .map(|key| key.0.clone());
        self.gallery.focused(key.as_deref());
        self.render();
    }

    /// The display list of the view, from `origin`, over the theme's
    /// background.
    fn paint(&self, font: &mut Font<CoreText>, origin: (f32, f32)) -> DisplayList {
        let theme = Theme::builtin();
        let variant = theme.variant(self.gallery.dark());
        font.begin_frame();
        let items = paint(
            &self.tree,
            &self.layout,
            font,
            &variant.tokens,
            origin,
            (self.gallery.dark(), self.focused()),
        );
        let background = variant.colors.background;
        DisplayList {
            background: Color::from_rgb8(background.r, background.g, background.b),
            items,
            frame: font.atlas_frame(),
        }
    }
}

/// The gallery's window, and what draws it.
struct App {
    drawing: Option<Drawing>,
    view: Option<View>,
    window: Option<(WindowId, Surface)>,
    size: Option<WindowSize>,
    areas: Vec<PointerArea>,
    dark: bool,
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
        let surface = Surface::new(&drawing.gpu, handle).map_err(|error| error.to_string())?;
        platform.track_mouse_moves(window, true);
        platform.set_appearance(Some(if self.dark {
            Appearance::Dark
        } else {
            Appearance::Light
        }));
        self.view = Some(View::new(Gallery::new(self.dark), size.scale as f32));
        self.drawing = Some(drawing);
        self.window = Some((window, surface));
        self.size = Some(size);
        platform.request_frame(window);
        Ok(())
    }

    fn resized(&mut self, platform: &mut dyn Platform, size: WindowSize) {
        let scale = size.scale as f32;
        if let (Some(view), Some(drawing)) = (&mut self.view, &mut self.drawing)
            && view.layout.scale() != scale
        {
            view.layout.set_scale(&view.tree, scale);
            drawing.font.set_scale(scale);
        }
        self.size = Some(size);
        self.request_frame(platform);
    }

    fn request_frame(&self, platform: &mut dyn Platform) {
        if let Some((window, _)) = &self.window {
            platform.request_frame(*window);
        }
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
        view.lay_out(&mut drawing.font, width, Some(height));
        let list = view.paint(&mut drawing.font, (0.0, bar));
        let updates = drawing.font.texture_updates();
        surface.resize(size.pixels.width, size.pixels.height, size.scale);
        let drawn = drawing
            .renderer
            .update_textures(&updates)
            .and_then(|()| drawing.renderer.draw(surface, &list));
        if let Err(error) = drawn {
            eprintln!("ui-gallery: {error}");
        }
        // The pointer's shapes over the view, which the window server
        // shows as it moves.
        let scale = size.scale;
        let areas: Vec<PointerArea> = pointer::areas(&view.tree, &view.layout)
            .into_iter()
            .map(|area| PointerArea {
                origin: Point {
                    x: f64::from(area.rect.x) / scale,
                    y: (f64::from(area.rect.y) + f64::from(bar)) / scale,
                },
                size: Size {
                    width: f64::from(area.rect.width) / scale,
                    height: f64::from(area.rect.height) / scale,
                },
                shape: match area.shape {
                    view::PointerShape::Arrow => PointerShape::Arrow,
                    view::PointerShape::Text => PointerShape::Text,
                    view::PointerShape::PointingHand => PointerShape::PointingHand,
                },
            })
            .collect();
        if areas != self.areas {
            platform.set_pointer_areas(*window, &areas);
            self.areas = areas;
        }
    }

    fn mouse(
        &mut self,
        platform: &mut dyn Platform,
        position: Point,
        press: Option<(MouseButton, bool, Modifiers, u32)>,
    ) {
        let (Some(at), Some((width, height, _))) = (self.at(position), self.room()) else {
            return;
        };
        let (Some(view), Some(drawing)) = (&mut self.view, &mut self.drawing) else {
            return;
        };
        let events = match press {
            None => view.pointer.moved(&view.tree, &view.layout, at),
            Some((button, pressed, mods, clicks)) => {
                let button = match button {
                    MouseButton::Left if mods.control => Button::Secondary,
                    MouseButton::Left => Button::Primary,
                    MouseButton::Right => Button::Secondary,
                    MouseButton::Middle => Button::Middle,
                    _ => return,
                };
                if pressed {
                    let path = pointer::hit(&view.tree, &view.layout, at);
                    if view.focus.pressed(PANEL, &view.tree, &path) {
                        view.tell_focus();
                        view.lay_out(&mut drawing.font, width, Some(height));
                    }
                    view.pointer.pressed(&view.tree, &view.layout, at, button)
                } else {
                    view.pointer
                        .released(&view.tree, &view.layout, at, button, clicks)
                }
            }
        };
        view.apply(&mut drawing.font, (width, height), events);
        self.request_frame(platform);
    }

    fn key(&mut self, platform: &mut dyn Platform, event: &KeyEvent) {
        let pressed = event.state != KeyState::Released;
        if pressed && event.mods.super_key {
            match event.code {
                KeyCode::KeyD => {
                    self.dark = !self.dark;
                    platform.set_appearance(Some(if self.dark {
                        Appearance::Dark
                    } else {
                        Appearance::Light
                    }));
                    if let Some(view) = &mut self.view {
                        view.gallery.toggle_dark();
                        view.render();
                    }
                    self.request_frame(platform);
                }
                KeyCode::KeyQ => platform.quit(),
                _ => {}
            }
            return;
        }
        let Some(key) = vt_key(event) else {
            return;
        };
        let Some(view) = &mut self.view else {
            return;
        };
        let tree = &view.tree;
        match view.focus.key(|_| Some(tree), &key) {
            Routed::Focus => view.tell_focus(),
            Routed::Node { node, .. } if key.action == KeyAction::Press => {
                let name = match key.key {
                    vt::Key::Char(' ') => "Space",
                    vt::Key::Enter => "Return",
                    _ => return,
                };
                if let Some(pressed) = view.tree.key(node).map(|key| key.0.clone()) {
                    view.gallery.pressed(&pressed, name);
                    view.render();
                }
            }
            Routed::Node { .. } | Routed::Pane => return,
        }
        self.request_frame(platform);
    }
}

/// `event`, as the focus takes it: Tab, Escape, Return and the keys that
/// type a character; `None` for the others.
fn vt_key(event: &KeyEvent) -> Option<vt::KeyEvent<'_>> {
    let key = match event.code {
        KeyCode::Tab => vt::Key::Tab,
        KeyCode::Escape => vt::Key::Escape,
        KeyCode::Enter => vt::Key::Enter,
        _ => vt::Key::Char(event.plain.chars().next()?),
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
        if let Err(error) = self.open(platform) {
            eprintln!("ui-gallery: {error}");
            platform.quit();
        }
    }

    fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
        if event == AppEvent::QuitRequested {
            platform.quit();
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
            _ => {}
        }
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
    };
    lokyr_platform_macos::run(app).map_err(|error| error.to_string())
}

/// Draws the gallery offscreen at a scale of 2, light and dark, the
/// pointer over a tab's close and the focus on a node, into PNGs in
/// `directory`.
pub fn images(directory: &Path) -> Result<(), String> {
    std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let scale = 2.0;
    let mut drawing = Drawing::new(scale)?;
    for dark in [false, true] {
        let mut gallery = Gallery::new(dark);
        gallery.event("tab", UiEventKind::HoverEnter);
        gallery.event("tab.close", UiEventKind::HoverEnter);
        gallery.event("hand", UiEventKind::Click);
        let mut view = View::new(gallery, scale);
        if let Some(second) = find(&view.tree, "second") {
            view.focus.focus(PANEL, &view.tree, second);
            view.tell_focus();
        }
        let width = SIZE.width as f32 * scale;
        view.lay_out(&mut drawing.font, width, None);
        let root = view
            .tree
            .root()
            .and_then(|root| view.layout.frame(&view.tree, root))
            .ok_or("the gallery has no root")?;
        let list = view.paint(&mut drawing.font, (0.0, 0.0));
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
