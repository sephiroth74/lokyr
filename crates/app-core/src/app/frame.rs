//! The windows' frames: the grid that fits each window, its padding and
//! its origin under the titlebar; the display list of its pane, its
//! cursor and its overlays, the notice's bar and the size's label among
//! them, drawn into its surface; the layers of the cursor's cells and of
//! the text that blinks, which the window server blinks; and the timers
//! of their blinking and of the labels.

use std::sync::{Arc, PoisonError};
use std::time::Instant;

use lokyr_kernel::plugins;
use lokyr_kernel::plugins::host::BarPlace;
use lokyr_kernel::workspace::{self, Direction, Grid, Layout, Pane, PaneId};
use lokyr_platform_core::{
    Appearance, Backdrop, Blink as PlatformBlink, ContentRect, Easing, Layer, LayerPlace, Platform,
    Point, PointerArea, PointerShape, Size, Slide, Titlebar, WindowId, WindowSize,
};
use lokyr_render_core::{
    CellBatch, Color, CursorShape, DisplayList, Item, PictureId, Rect, Renderer, Shape,
    Surface as _,
};
use lokyr_text_core::{CellMetrics, FontBackend};
use lokyr_ui_core::focus::Target;
use lokyr_ui_core::font::Font;
use lokyr_ui_core::pane::{CursorLook, TextLook};
use lokyr_ui_core::theme::Variant;
use lokyr_ui_core::theme::{MatchColors, SelectionColors};
use lokyr_ui_core::{cursor, pane as cells};
use lokyr_vt_core::terminal::MouseTracking;

use super::access;
use super::chrome;
use super::menus::menu_context;
use super::surfaces;
use super::{App, FLASH_OPACITY, GLYPHS_BUDGET, PointerTold, UNDRAWN_CELL};
use crate::drawing::{Backend, Drawing};
use crate::output_bench::Phases;
use crate::window::{PaneView, Shown, WindowLayer};
use crate::{performance, size_label};

/// The room between the two parts of a split, in points, which their
/// divider takes.
const DIVIDER: f64 = 1.0;

/// The least room at the left of a pane's cells that its gutter takes, in
/// points: its mark, and a point on either side.
const GUTTER_ROOM: f64 = 4.0;

/// The tokens of the chrome's backgrounds, which take the window's
/// background's opacity: the tab bar's, of a pane's header too, the active
/// tab's and the status bar's. Over a translucent window, they take the
/// place of what is under them, the window's background that the frame
/// draws, so that they show the window's opacity rather than theirs over
/// it.
const TRANSLUCENT: [&str; 3] = ["tabs.bar.bg", "tabs.active.bg", "status-bar.bg"];

impl<B: Backend> App<B> {
    /// Asks every window for a frame.
    pub(super) fn request_frames(&self, platform: &mut dyn Platform) {
        for &window in self.windows.keys() {
            platform.request_frame(window);
        }
    }

    /// The theme's colors in `appearance`.
    pub(super) fn colors(&self, appearance: Appearance) -> lokyr_vt_core::terminal::Colors {
        self.theme
            .variant(appearance == Appearance::Dark)
            .colors
            .clone()
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

    /// The backdrop of `window`, as the config's opacity and blur make it:
    /// clear, of the blur of what is behind it, as the frame draws the
    /// window's background of its opacity, under the chrome that takes its
    /// place; `None` for an opaque one, as the settings' window always is,
    /// which is no terminal's, and one that shows opaque.
    fn backdrop(&self, window: WindowId) -> Option<Backdrop> {
        let shown_opaque = self.windows.get(&window).is_some_and(|state| state.opaque);
        (self.translucent_by_config(window) && !shown_opaque).then(|| Backdrop {
            color: [0.0; 4],
            blur: f64::from(self.config.window.background_blur.points()),
        })
    }

    /// The grid that fits a window of `size`, less its padding, in the
    /// cells of the font at its display's scale.
    pub(super) fn grid(&mut self, size: WindowSize) -> Grid {
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
    pub(super) fn size_of(&mut self, scale: f64, (columns, rows): (u32, u32)) -> Size {
        let (x, y) = self.padding(scale);
        let (width, height) = self.cell_pixels(scale);
        Size {
            width: f64::from(columns * width + 2 * x) / scale,
            height: f64::from(rows * height + 2 * y) / scale,
        }
    }

    /// Where a window of `size` has its grid's first cell, in pixels from
    /// its content's top left corner: past the padding, and below the
    /// titlebar.
    pub(super) fn origin(&self, size: WindowSize) -> (u32, u32) {
        let (x, y) = self.padding(size.scale);
        (x, y + size.bar_pixels())
    }

    /// The room of a window of `size` that its panes share, in pixels from
    /// its content's top left corner: past the padding, and below the
    /// titlebar.
    fn content(&self, size: WindowSize) -> Rect {
        let (x, y) = self.padding(size.scale);
        let below = size.pixels.height.saturating_sub(size.bar_pixels());
        Rect {
            x: i32::try_from(x).unwrap_or(i32::MAX),
            y: i32::try_from(y + size.bar_pixels()).unwrap_or(i32::MAX),
            width: size.pixels.width.saturating_sub(2 * x),
            height: below.saturating_sub(2 * y),
        }
    }

    /// Lays out the panes of `window`'s active tab in its content, of
    /// `size`: each its room and its grid, which the window keeps for its
    /// frame, its hits and its input method; the zoomed pane alone while
    /// one is.
    pub(super) fn lay_out_panes(&mut self, window: WindowId, size: WindowSize) -> Vec<PaneView> {
        // The slots' views take their regions, and the panes the rest.
        self.take_trees(window, size.scale as f32);
        // Their text measured anew of a new font, the terminal's or the
        // interface's, as their boxes take its new size.
        if let (Some(drawing), Some(state)) = (&self.drawing, self.windows.get_mut(&window)) {
            for view in &mut state.chrome {
                if view.faces != drawing.faces {
                    view.surface.layout.measure_again();
                    view.faces = drawing.faces;
                }
            }
        }
        let content = match self.lay_out_chrome(window, size) {
            Some(regions) => {
                let panes = chrome::rect_of(regions.panes);
                if let Some(state) = self.windows.get_mut(&window) {
                    state.panes_top = u32::try_from(panes.y).unwrap_or(0);
                }
                self.padded(panes, size.scale)
            }
            None => self.content(size),
        };
        let (width, height) = self.cell_pixels(size.scale);
        let padding = self.padding(size.scale);
        let Some(state) = self.windows.get_mut(&window) else {
            return Vec::new();
        };
        state.panes_room = Some(content);
        let tab = self
            .workspace
            .window(state.window)
            .and_then(|shown| self.workspace.tab(shown.active_tab()));
        let gap = Gap {
            line: (DIVIDER * size.scale).round() as u32,
            padding,
        };
        let (rooms, lines) = match tab {
            Some(tab) => (
                pane_rects(tab.layout(), tab.zoomed(), content, (width, height), gap),
                dividers(tab.layout(), tab.zoomed(), content, (width, height), gap),
            ),
            None => (Vec::new(), Vec::new()),
        };
        // The tab's floating panes over its layout that show, from the back
        // to the front, each in its frame of shares of the room.
        let floating: Vec<(PaneId, lokyr_ui_core::floating::Room)> = match tab {
            Some(tab) => tab
                .shown_floating()
                .map(|floating| {
                    let frame = floating.frame;
                    let shares = lokyr_ui_core::floating::Shares {
                        x: frame.x(),
                        y: frame.y(),
                        width: frame.width(),
                        height: frame.height(),
                    };
                    let room =
                        lokyr_ui_core::floating::room(shares, content, (width, height), size.scale);
                    (floating.pane, room)
                })
                .collect(),
            None => Vec::new(),
        };
        state.dividers = lines;
        // Each pane's header at its room's top, and its grid below it.
        let mut rooms = rooms;
        self.lay_out_headers(window, &mut rooms);
        let pixels = |cell: u32| u16::try_from(cell).unwrap_or(u16::MAX);
        // The floating panes' grids after the layout's, as they draw over
        // them.
        let floating_grids = floating.iter().map(|&(pane, room)| (pane, room.grid));
        let views: Vec<PaneView> = rooms
            .into_iter()
            .chain(floating_grids)
            .map(|(pane, rect)| PaneView {
                pane,
                rect,
                grid: Grid {
                    columns: fit(rect.width, width),
                    rows: fit(rect.height, height),
                    cell_width: pixels(width),
                    cell_height: pixels(height),
                },
            })
            .collect();
        if let Some(state) = self.windows.get_mut(&window) {
            state.views.clone_from(&views);
            state.floating_rooms = floating;
        }
        // The panels over them all, at their anchors.
        self.lay_out_panels(window, size);
        self.settle_views(window);
        views
    }

    /// Where `pane` of `window` has its grid's first cell, in pixels from
    /// the content's top left corner, as the last frame laid it out; the
    /// window's own grid's, of `size`, before it did.
    pub(super) fn pane_origin(
        &self,
        window: WindowId,
        pane: PaneId,
        size: WindowSize,
    ) -> (u32, u32) {
        self.windows
            .get(&window)
            .and_then(|state| state.views.iter().find(|view| view.pane == pane))
            .map_or_else(
                || self.origin(size),
                |view| {
                    let at = |pixels: i32| u32::try_from(pixels).unwrap_or(0);
                    (at(view.rect.x), at(view.rect.y))
                },
            )
    }

    /// `room`, in pixels, within the padding between the grid and the
    /// window's edges, on a display of `scale`.
    fn padded(&self, room: Rect, scale: f64) -> Rect {
        let (x, y) = self.padding(scale);
        let inset = |pixels: u32| i32::try_from(pixels).unwrap_or(i32::MAX);
        Rect {
            x: room.x.saturating_add(inset(x)),
            y: room.y.saturating_add(inset(y)),
            width: room.width.saturating_sub(2 * x),
            height: room.height.saturating_sub(2 * y),
        }
    }

    /// The room between the grid and the window's edges, in pixels on a
    /// display of `scale`: at the left and the right, and at the top and
    /// the bottom.
    /// The gutter at the cells' left takes the padding, which widens, at
    /// the right too, as far as the gutter needs.
    fn padding(&self, scale: f64) -> (u32, u32) {
        let window = &self.config.window;
        let x = window.padding_x.pixels(scale);
        let x = if window.gutter {
            // A cast from a float saturates.
            x.max((GUTTER_ROOM * scale).round() as u32)
        } else {
            x
        };
        (x, window.padding_y.pixels(scale))
    }

    /// The surface of `window`, if the app draws and the platform has a
    /// layer for it.
    pub(super) fn surface(&self, platform: &dyn Platform, window: WindowId) -> Option<B::Surface> {
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
    pub(super) fn layer_surface(
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
    pub(super) fn cursor(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        now: Instant,
    ) -> Option<(CursorShape, Option<PlatformBlink>)> {
        let state = self.windows.get_mut(&window)?;
        let program = self.workspace.focused(state.window)?.snapshot().cursor();
        let config = self.config.cursor;
        // The cursor of a pane whose window has the keyboard, but on a node
        // of a slot's view, shows as unfocused.
        let focused = state.focused && state.pane_has_focus();
        let shape = cursor::shape(&program, &config, focused);
        let blinking =
            cursor::blinks(&program, &config, focused) && state.blink.blinks(now, &config);
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
    pub(super) fn text_blink(
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
                .focused(state.window)
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
    pub(super) fn label_at(&mut self, platform: &mut dyn Platform, at: Instant) {
        if self.label_timer.is_some_and(|(_, current)| current <= at) {
            return;
        }
        if let Some((timer, _)) = self.label_timer.take() {
            platform.cancel_timer(timer);
        }
        self.label_timer = Some((platform.start_timer(at), at));
    }

    /// Draws the panes of `tab`, of `window`, a hidden tab's too, as the
    /// window would show them now, steady and of no focus, into a picture
    /// that the renderer holds, of the room that the window's panes share,
    /// its corner the picture's: the picture, its size in pixels and the
    /// background of the tab's pane with the keyboard; `None` where the app
    /// draws nothing, or before the window laid its panes out.
    pub(super) fn tab_picture(
        &mut self,
        platform: &dyn Platform,
        window: WindowId,
        tab: workspace::TabId,
    ) -> Option<(PictureId, (u32, u32), Color)> {
        let size = platform.window_size(window)?;
        let room = self.windows.get(&window)?.panes_room?;
        let (width, height) = self.cell_pixels(size.scale);
        let gap = Gap {
            line: (DIVIDER * size.scale).round() as u32,
            padding: self.padding(size.scale),
        };
        let variant = self.variant(platform, window);
        let colors = (variant.selection, variant.match_colors());
        let text = self.config.text;
        let minimum_contrast = self.config.theme.minimum_contrast.value();
        let thickness = self.config.cursor;
        let shown = self.workspace.tab(tab)?;
        let rooms = pane_rects(shown.layout(), shown.zoomed(), room, (width, height), gap);
        let background = cells::background(self.workspace.pane(shown.active_pane())?.snapshot());
        let drawing = self.drawing.as_mut()?;
        let oldest = drawing.renderer.drawing_from();
        drawing.font.set_drawing(oldest);
        drawing.font.begin_frame();
        drawing.font.set_text(text);
        let cell = *drawing.font.cell();
        let hollow = CursorLook {
            shape: CursorShape::Hollow,
            thickness: thickness.thickness(&cell, size.scale as f32),
            length: thickness.length.value(),
        };
        // Each pane at its room's corner, of the picture's.
        let panes: Vec<(&Pane, (i32, i32))> = rooms
            .iter()
            .filter_map(|(pane, rect)| {
                let corner = (rect.x.saturating_sub(room.x), rect.y.saturating_sub(room.y));
                Some((self.workspace.pane(*pane)?, corner))
            })
            .collect();
        let font = &mut drawing.font;
        font.collect_missing(GLYPHS_BUDGET.0, GLYPHS_BUDGET.1);
        let look = TextLook {
            config: text,
            minimum_contrast,
            ..TextLook::default()
        };
        let mut batches = other_cells(&panes, &cell, colors, hollow, look, 0.0, font);
        if font.draw_pending() {
            batches = other_cells(&panes, &cell, colors, hollow, look, 0.0, font);
        }
        let list = DisplayList {
            background,
            items: batches.into_iter().map(Item::Cells).collect(),
            frame: drawing.font.atlas_frame(),
        };
        let updates = drawing.font.texture_updates();
        let made = drawing.renderer.update_textures(&updates).and_then(|()| {
            drawing
                .renderer
                .make_picture(&list, room.width, room.height)
        });
        match made {
            Ok(picture) => picture.map(|picture| (picture, (room.width, room.height), background)),
            Err(error) => {
                eprintln!("lokyr: {error}");
                None
            }
        }
    }

    /// Draws `window`, of `size`, as [`App::draw_frame`] does; the plugins
    /// learn of the pointer again where the frame changed the text under
    /// it, still.
    pub(super) fn draw(&mut self, platform: &mut dyn Platform, window: WindowId, size: WindowSize) {
        self.draw_frame(platform, window, size);
        let shifted = std::mem::take(&mut self.pointer_shifted);
        if std::mem::take(&mut self.pointer_text_changed) {
            self.tell_pointer_over_changed_text(platform, window);
        } else if shifted {
            let position = platform.pointer_position(window);
            self.tell_pointer(platform, window, position, self.pointer_mods);
        }
    }

    /// Settles `window`'s titlebar: a tab bar's while one shows in it, the
    /// system's of no title while a bar's view shows the title in its
    /// place, the system's otherwise, of a bar below it, at the bottom or
    /// down a column; a quick window keeps its own. The platform keeps the
    /// room of the small buttons that show beside the window's, which the
    /// system's title leaves.
    fn settle_titlebar(&mut self, platform: &mut dyn Platform, window: WindowId) {
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        let titlebar = match chrome::tab_bar_place(&state.chrome, &self.plugins) {
            _ if !chrome::has_tab_bar(&state.chrome) => Titlebar::Titled,
            BarPlace::Titlebar | BarPlace::Sidebar { .. } => Titlebar::Tabs,
            BarPlace::Title => Titlebar::Untitled,
            _ => Titlebar::Titled,
        };
        if titlebar != state.titlebar && !self.quick.contains_key(&window) {
            platform.set_titlebar(window, titlebar);
            state.titlebar = titlebar;
        }
        let room = chrome::title_buttons_shown(&state.chrome).len() as f64
            * f64::from(chrome::TITLE_BUTTON_ROOM);
        if state.title_buttons_room != Some(room) && !self.quick.contains_key(&window) {
            platform.set_title_buttons_room(window, room);
            state.title_buttons_room = Some(room);
        }
    }

    /// Draws `window`, of `size`: its program's screen, brought up to date,
    /// in the grid that fits its content within the padding, with the
    /// cursor.
    fn draw_frame(&mut self, platform: &mut dyn Platform, window: WindowId, size: WindowSize) {
        // The frame's time, which the performance's panel tells, the wait
        // for the display's drawable apart from the CPU's.
        let started =
            (self.performance.is_some() || self.output_bench.is_some()).then(Instant::now);
        let now = Instant::now();
        // A wheel's glide and a trackpad's ease move their views first, so
        // that the frame shows them.
        self.glide_view(platform, window, now);
        self.ease_view(platform, window, now);
        // The slots that show a view first ask their plugins for it, whose
        // calls' requests the turn's end takes.
        self.rendered_slots |= self.render_slots(platform, window);
        // Each pane of the active tab in its room, around the slots' views,
        // under the panels; a menu's panel of menu items alone, the
        // platform's own.
        // The notice's card, opened or closed as a notice shows or not.
        self.settle_notice(platform, window);
        // The settings tab's view, as the window shows the tab or not.
        self.settle_settings(platform, window);
        let views = self.lay_out_panes(window, size);
        // The theme that a view's part newly names: its files watched with
        // those of the themes in force.
        if self.theme_parts_new {
            self.load_themes(platform);
        }
        self.keys_from_hidden(platform, window);
        self.show_native(platform, window, size);
        // The titlebar first, whose kind the window's keys follow.
        self.settle_titlebar(platform, window);
        self.show_window_keys(platform, window);
        // The pointer's moves while Command is held, for the plugins that
        // follow it, as `links` does.
        let pointer_moves = self.command_held && self.plugins.subscribes(plugins::POINTER);
        // The panes' views that changed, for the plugins that follow them.
        let tells_viewports = self.plugins.subscribes(plugins::VIEWPORT_CHANGED);
        let mut viewports = Vec::new();
        // The half of a pane that a tab's drag lights up.
        let lit = self.lit_half(window);
        // The rooms of the panes whose agents wait, in a tab of several.
        let waiting: Vec<(PaneId, Rect)> = if views.len() > 1 {
            views
                .iter()
                .filter(|view| {
                    self.workspace
                        .pane(view.pane)
                        .and_then(Pane::agent)
                        .and_then(workspace::Agent::state)
                        == Some(lokyr_kernel::plugins::host::AgentState::Waiting)
                })
                .map(|view| (view.pane, view.rect))
                .collect()
        } else {
            Vec::new()
        };
        // The rooms of the panes that take the window's broadcast of the
        // typed input, but those left out and those whose programs ended.
        let broadcasting: Vec<(PaneId, Rect)> = views
            .iter()
            .filter(|view| self.takes_broadcast(view.pane))
            .map(|view| (view.pane, view.rect))
            .collect();
        // A window of no pane, as of the settings tab, draws its chrome and
        // its views alone.
        if views.is_empty() {
            self.draw_without_panes(platform, window, size, now);
            return;
        }
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        // The pane with the keyboard, or the one zoomed over it.
        let focused = self.workspace.active_pane(state.window);
        let Some(&PaneView {
            pane: active,
            rect,
            grid,
        }) = views
            .iter()
            .find(|view| Some(view.pane) == focused)
            .or_else(|| views.first())
        else {
            return;
        };
        let at = |pixels: i32| u32::try_from(pixels).unwrap_or(0);
        let (left, top) = (at(rect.x), at(rect.y));
        // Each pane's terminal takes its grid with its program's signal, at
        // the resizes' pace, and the others' output shows in this frame too.
        let mut due = None;
        let mut others = false;
        // Where the panes' cursors show, for the tree of a client that
        // asked for one.
        let asked = platform.accessibility_asked(window);
        let mut cursors = Vec::new();
        // The block's prompt rows pinned at each view's top, as a plugin
        // asks, and a blank row before the prompts of every pane, those
        // that no window shows too, so that their commands' blocks have it
        // as they show.
        let looks = self.plugins.block_looks();
        let sticky = looks.prompt_rows;
        for pane in self.workspace.panes_mut() {
            pane.set_block_spacing(looks.spacing);
        }
        for view in &views {
            let Some(pane) = self.workspace.pane_mut(view.pane) else {
                continue;
            };
            let told = pane.resize(view.grid, now);
            // A frame shows its screen as the wait for its program's drawing
            // at a new size ends.
            let told = match (told, pane.answer_due()) {
                (Some(told), Some(answer)) => Some(told.min(answer)),
                (told, answer) => told.or(answer),
            };
            due = match (due, told) {
                (Some(due), Some(told)) => Some(std::cmp::min(due, told)),
                (due, told) => due.or(told),
            };
            if view.pane != active {
                pane.set_sticky_prompts(sticky);
                let share = pane.snapshot().fraction();
                let updated = pane.update();
                viewports.extend(pane.viewport_changed().map(|now| (view.pane, now)));
                others |= updated;
                self.pointer_text_changed |=
                    updated && text_changed_under(self.pointer_told, view.pane, pane);
                self.pointer_shifted |= shifted_under(self.pointer_told, view.pane, pane, share);
                if asked {
                    cursors.push(access::tree_cursor(pane));
                }
            }
        }
        let Some(pane) = self.workspace.pane_mut(active) else {
            return;
        };
        // A resize that changes the grid shows its size, a moment.
        let resized = state
            .grid_drawn
            .is_some_and(|drawn| drawn != (grid.columns, grid.rows));
        state.grid_drawn = Some((grid.columns, grid.rows));
        if resized {
            state.size_label = Some(now + size_label::HOLD);
        }
        // Output starts the cursor blinking again, on, as a key does.
        pane.set_sticky_prompts(sticky);
        let share = pane.snapshot().fraction();
        let output = pane.update();
        self.pointer_shifted |= shifted_under(self.pointer_told, active, pane, share);
        viewports.extend(pane.viewport_changed().map(|now| (active, now)));
        if tells_viewports {
            for (pane, viewport) in viewports {
                self.plugins.post(plugins::Event::ViewportChanged {
                    pane: plugins::host::PaneId(pane.get()),
                    viewport,
                });
            }
        }
        self.pointer_text_changed |= output && text_changed_under(self.pointer_told, active, pane);
        let copied = started.map(|_| Instant::now());
        if output {
            state.blink.restart(now);
        }
        // The panes' screens or cursors, or the keyboard's pane, changed:
        // the window's accessibility tree goes within its rate, once a
        // client asked.
        if asked {
            cursors.push(access::tree_cursor(pane));
            cursors.sort_unstable();
        }
        if asked
            && (output
                || others
                || state.access_focus != focused
                || cursors != state.access_cursors
                || state.views_changed)
        {
            state.access_changed = true;
            if self.access_timer.is_none() {
                self.access_timer = Some(platform.start_timer(now + access::RATE));
            }
        }
        // The press that this frame shows, which the latency's bench
        // measures as the frame shows.
        let shown_key = match &mut self.latency_bench {
            Some(bench) if output && bench.window == Some(window) => {
                bench.typed_at = None;
                let handled = bench.handled.take();
                bench.pending.take().map(|time| {
                    (
                        (time, handled.unwrap_or(time)),
                        (Arc::clone(&bench.samples), Arc::clone(&bench.phases)),
                    )
                })
            }
            _ => None,
        };
        // The pointer's moves without a button wake the app only for a
        // program that asked for them all.
        let moves = pointer_moves
            || pane
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
        // A render took a field's composition away, or set its text: the
        // input method forgets it.
        if std::mem::take(&mut state.composition_dropped) {
            platform.end_composition(window);
        }
        // The input method's candidates go at the cursor's cell, or at the
        // composition's start, which can be left of it, near the right edge;
        // at the field with the keyboard's focus, if a slot's view has it.
        let snapshot = pane.snapshot();
        let field_area = match state.focus.target() {
            Target::Node { panel, node } => state
                .chrome
                .iter()
                .find(|view| view.id == panel)
                .and_then(|view| {
                    let frame = view.frame?;
                    let area = view
                        .surface
                        .text_area(node, &mut self.drawing.as_mut()?.font)?;
                    Some((area.x + frame.x, area.y + frame.y, area.width, area.height))
                }),
            Target::Pane => None,
        };
        let at = snapshot.cursor().position;
        let (column, row) = state
            .preedit
            .as_ref()
            .and_then(|preedit| cells::composition_start(snapshot, preedit, snapshot.cols()))
            .map_or((at.x, at.y), |(row, column)| (column, row));
        let (width, height) = (f64::from(grid.cell_width), f64::from(grid.cell_height));
        // The rows a share of a row up, as the view is scrolled by points.
        let up = f64::from(cells::rows_up(snapshot, u32::from(grid.cell_height)));
        let (x, y, width, height) = field_area.map_or(
            (
                f64::from(left) + f64::from(column) * width,
                f64::from(top) + f64::from(row) * height - up,
                width,
                height,
            ),
            |(x, y, width, height)| {
                (
                    f64::from(x),
                    f64::from(y),
                    f64::from(width),
                    f64::from(height),
                )
            },
        );
        platform.set_text_area(
            window,
            Point {
                x: x / size.scale,
                y: y / size.scale,
            },
            Size {
                width: width / size.scale,
                height: height / size.scale,
            },
        );
        // A translucent window's background is the terminal's default
        // background, under the whole window, the titlebar too, which the
        // frame draws, and follows it, as the theme, the program or reverse
        // video change it; the platform's backdrop, clear, blurs what is
        // behind.
        let background = cells::background(pane.snapshot());
        let foreground = cells::foreground(pane.snapshot());
        let sent = state.backdrop;
        let backdrop = self.backdrop(window);
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
        // The menus' items as the focused pane's frame has it, or of no
        // pane, as of a window that shows the settings tab.
        let focused = self.windows.get(&window).filter(|state| state.focused);
        let context = focused.and_then(|state| match self.workspace.focused(state.window) {
            Some(_) => self.workspace.pane(active).map(|pane| {
                let mut context = menu_context(pane.snapshot());
                super::input::hold_selected_blocks(&self.plugins, pane, &mut context);
                context
            }),
            None => Some(lokyr_kernel::keymap::Context::new(Vec::<&str>::new())),
        });
        if let Some(context) = context {
            self.settle_menu(platform, &context);
        }
        let shown_cursor = self.cursor(platform, window, Instant::now());
        let (blink_shown, text_blink) = self.text_blink(platform, window, Instant::now());
        let text = self.config.text;
        let look = TextLook {
            config: text,
            blink_shown,
            minimum_contrast: self.config.theme.minimum_contrast.value(),
            cursor_guide: None,
        };
        // The cursor guide, of the pane that has the keyboard alone.
        let guided = TextLook {
            cursor_guide: self
                .cursor_guide
                .then(|| self.variant(platform, window).tokens.get("cursor.guide"))
                .flatten(),
            ..look
        };
        let config = self.config.cursor;
        let movement = config.movement;
        let selection_colors = self.variant(platform, window).selection;
        let match_colors = self.variant(platform, window).match_colors();
        let label_colors = self.variant(platform, window).label_colors();
        let colors = (selection_colors, match_colors);
        let dark = platform.appearance() == Appearance::Dark;
        // The chrome of the window's own appearance, as its title is, and
        // the terminal's colors of the pane's own, which a program, reverse
        // video or a man page's window change.
        let chrome_dark = own.map_or(dark, |own| own == Appearance::Dark);
        // The window's opacity, of 1 while it shows opaque.
        let opacity = if backdrop.is_some() {
            self.config.window.background_opacity.value()
        } else {
            1.0
        };
        let chrome_look = (chrome_dark, background, foreground, opacity);
        let reduced = platform.reduce_motion();
        // The views' scroll indicators show always where the system shows
        // its scroll bars so, else while their views scroll.
        let always_indicators = !platform.scrollers().overlay;
        let headroom = platform.screen_headroom(window);
        // The padding at the cells' left that the gutter takes, if it shows.
        let gutter = self
            .config
            .window
            .gutter
            .then(|| self.padding(size.scale).0);
        // The panes' scroll bars, as the config and the system show them,
        // over the padding at each pane's right too.
        let bar_mode = self.bar_mode(platform);
        let bar_padding = self.padding(size.scale).0;
        let (Some(drawing), Some(state)) = (self.drawing.as_mut(), self.windows.get_mut(&window))
        else {
            return;
        };
        let Some(pane) = self.workspace.pane(active) else {
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
            length: config.length.value(),
        });
        // A cursor of a layer of its own, which blinks apart from the grid.
        let apart = state.cursor_layer.surface.is_some();
        let snapshot = pane.snapshot();
        let font = &mut drawing.font;
        let (x, y) = (
            i32::try_from(left).unwrap_or(i32::MAX),
            i32::try_from(top).unwrap_or(i32::MAX),
        );
        // The cursor of a pane of the layout under a floating pane's box
        // shows not, as the box hides its cell.
        let floating_ids: Vec<PaneId> =
            state.floating_rooms.iter().map(|&(pane, _)| pane).collect();
        let cursor = cursor.filter(|_| {
            if floating_ids.contains(&active) {
                return true;
            }
            let at = snapshot.cursor().position;
            let cell_x = x + i32::from(at.x) * cell.width as i32;
            let cell_y = y + i32::from(at.y) * cell.height as i32;
            !state.floating_rooms.iter().any(|(_, room)| {
                let outer = room.outer;
                cell_x + cell.width as i32 > outer.x
                    && cell_x < outer.x + outer.width as i32
                    && cell_y + cell.height as i32 > outer.y
                    && cell_y < outer.y + outer.height as i32
            })
        });
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
        // Each pane's grid's corner, for the pointer's areas over it.
        let pointed: Vec<(PaneId, (i32, i32))> = views
            .iter()
            .map(|view| {
                let corner = if view.pane == active {
                    (x, y)
                } else {
                    (view.rect.x, view.rect.y)
                };
                (view.pane, corner)
            })
            .collect();
        // The other panes, each at its room's corner.
        let others: Vec<(&Pane, (i32, i32))> = views
            .iter()
            .filter(|view| view.pane != active)
            .filter_map(|view| Some((self.workspace.pane(view.pane)?, (view.rect.x, view.rect.y))))
            .collect();
        let hollow = CursorLook {
            shape: CursorShape::Hollow,
            thickness: config.thickness(&cell, size.scale as f32),
            length: config.length.value(),
        };
        // Each pane's rows, a share of a row up while its view is scrolled
        // by points, within the band of its grid's rows across its room,
        // its paddings too.
        let pad = i32::try_from(bar_padding).unwrap_or(0);
        let width = |pane: PaneId| {
            views
                .iter()
                .find(|view| view.pane == pane)
                .map_or(0, |view| view.rect.width)
        };
        let rows_of = |snapshot: &lokyr_vt_core::terminal::Snapshot, at: (i32, i32), pane| {
            cells::rows_at(
                snapshot,
                &cell,
                at,
                (at.0 - pad, width(pane) + 2 * bar_padding),
            )
        };
        let rows = rows_of(snapshot, (x, y), active);
        let others_drawn: Vec<(&Pane, (i32, i32))> = others
            .iter()
            .map(|&(pane, at)| (pane, rows_of(pane.snapshot(), at, pane.id()).corner(at)))
            .collect();
        // How much the panes that have not the keyboard fade, as the plugins
        // ask.
        let dim = self.plugins.inactive_dim();
        font.collect_missing(GLYPHS_BUDGET.0, GLYPHS_BUDGET.1);
        let mut batch = cells::cells_with(
            snapshot,
            &cell,
            rows.corner((x, y)),
            selection.as_ref(),
            grid_cursor,
            guided,
            font,
        );
        let mut other_batches = other_cells(&others_drawn, &cell, colors, hollow, look, dim, font);
        if font.draw_pending() {
            batch = cells::cells_with(
                snapshot,
                &cell,
                rows.corner((x, y)),
                selection.as_ref(),
                grid_cursor,
                guided,
                font,
            );
            other_batches = other_cells(&others_drawn, &cell, colors, hollow, look, dim, font);
        }
        // A search's matches over the cells, and the plugins' decorations
        // over the text and their labels, under the composition.
        cells::highlight(&mut batch, snapshot, match_colors);
        cells::decorate(&mut batch, snapshot, &cell);
        cells::labels(&mut batch, snapshot, &cell, label_colors, font);
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
                cells::blinking_cells(snapshot, &cell, selection.as_ref(), look, preedit, font)
            });
        // A translucent window's background, of its opacity, which the cells
        // leave showing, and the titlebar too, unless the config keeps it
        // opaque: past a sidebar, which runs through it, its view's own.
        let laid_out = started.map(|_| Instant::now());
        let opacity = chrome_look.3;
        let sidebar = chrome::has_tab_bar(&state.chrome)
            && matches!(
                chrome::tab_bar_place(&state.chrome, &self.plugins),
                BarPlace::Sidebar { .. }
            );
        let past = chrome::column_edge(&state.chrome, &self.plugins, size.scale)
            .filter(|_| sidebar)
            .map_or(0, |(_, _, zone)| {
                (zone.x + zone.width / 2.0).max(0.0) as u32
            });
        let opaque_bar = backdrop.is_some() && self.config.window.opaque_titlebar;
        let mut items: Vec<Item> = opaque_bar
            .then(|| bar_item(size, background, past))
            .flatten()
            .into_iter()
            .collect();
        // The panes' cells and their inline images: those under the cells'
        // backgrounds, the backgrounds, those under the text, the text, and
        // those over it after every pane's, each within its pane, and
        // within its rows' band while they draw a share of a row up.
        let mut over_text = Vec::new();
        let mut floating_drawn: Vec<(PaneId, Vec<Item>, Vec<Item>)> = Vec::new();
        {
            let renderer = &mut drawing.renderer;
            let cell_size = (cell.width, cell.height);
            let shown = std::iter::once((snapshot, (x, y), (grid.columns, grid.rows), rows)).chain(
                others.iter().map(|(pane, corner)| {
                    let size = pane.grid();
                    let snapshot = pane.snapshot();
                    let rows = rows_of(snapshot, *corner, pane.id());
                    (snapshot, *corner, (size.columns, size.rows), rows)
                }),
            );
            let batches = std::iter::once(batch).chain(other_batches);
            let ids = std::iter::once(active).chain(others.iter().map(|(pane, _)| pane.id()));
            for (((snapshot, corner, size, rows), batch), id) in shown.zip(batches).zip(ids) {
                let layers = self.inline_images.items(
                    renderer,
                    &*platform,
                    snapshot.images(),
                    super::inline_images::Grid {
                        corner,
                        up: rows.up,
                        cell: cell_size,
                        size,
                    },
                    now,
                );
                let mut drawn = layers.below;
                if layers.under.is_empty() {
                    drawn.push(Item::Cells(batch));
                } else {
                    let (backgrounds, text) = backgrounds_apart(batch);
                    drawn.push(Item::Cells(backgrounds));
                    drawn.extend(layers.under);
                    drawn.push(Item::Cells(text));
                }
                // A floating pane's, over the layout's, in its own layer.
                if floating_ids.contains(&id) {
                    floating_drawn.push((id, rows.clipped(drawn), rows.clipped(layers.over)));
                    continue;
                }
                items.extend(rows.clipped(drawn));
                over_text.extend(rows.clipped(layers.over));
            }
            self.inline_images.let_go(renderer, now);
        }
        items.extend(over_text);
        let bars = super::scrollbars::settle(
            &self.workspace,
            state,
            bar_mode,
            (size.scale, bar_padding),
            (now, reduced),
        );
        // The slots' views over the panes, of the theme's tokens of the
        // window's look.
        if state
            .chrome_tokens
            .as_ref()
            .is_none_or(|(kept, _)| *kept != chrome_look)
        {
            state.chrome_tokens = Some((chrome_look, chrome_tokens(&self.theme, chrome_look)));
        }
        let Some((_, tokens)) = state.chrome_tokens.as_ref() else {
            return;
        };
        // The commands' blocks as the plugins ask: a failed one's tint and
        // their dividers across each pane, its paddings too, then the marks
        // of the panes' gutters, in their paddings at the cells' left, along
        // the blocks where asked; each faded as its pane's cells are.
        let looks = self.plugins.block_looks();
        let faded = |dim: f32| {
            move |mut shape: Shape| {
                shape.fill = shape.fill.with_alpha(shape.fill.alpha * (1.0 - dim));
                shape
            }
        };
        let shown = || {
            std::iter::once((active, snapshot, (x, y), width(active), 0.0)).chain(
                others.iter().map(move |(pane, at)| {
                    (pane.id(), pane.snapshot(), *at, width(pane.id()), dim)
                }),
            )
        };
        for (pane, snapshot, at, width, dim) in shown() {
            let selected = self
                .plugins
                .selected_block(lokyr_kernel::plugins::host::PaneId(pane.get()));
            if !(looks.dividers || looks.failed_tint || selected.is_some()) {
                continue;
            }
            // Within the pane and its paddings, as a selected block's
            // outline goes past the edges of the view that cut it, and of
            // its rows, a share of a row up as they draw.
            let across = (at.0 - pad, width + 2 * bar_padding);
            let blocks: Vec<Shape> = cells::block_shapes(
                snapshot,
                rows_of(snapshot, at, pane).corner(at).1,
                cell.height,
                across,
                size.scale,
                tokens,
                (looks, selected),
            )
            .into_iter()
            .map(faded(dim))
            .collect();
            if !blocks.is_empty() {
                items.push(Item::PushClip(Rect {
                    x: across.0,
                    y: at.1,
                    width: across.1,
                    height: u32::from(snapshot.rows()) * cell.height,
                }));
                items.push(Item::Shapes(blocks));
                items.push(Item::PopClip);
            }
        }
        // The marks of the rows drawn a share of a row up, within their
        // band, each pane's apart.
        let mut shapes: Vec<Shape> = Vec::new();
        if let Some(padding) = gutter {
            for (pane, snapshot, at, _, dim) in shown() {
                let rows = rows_of(snapshot, at, pane);
                let marks = cells::gutter_marks(
                    snapshot,
                    rows.corner(at),
                    (padding, cell.height),
                    size.scale,
                    tokens,
                    looks.bars,
                )
                .map(faded(dim));
                if rows.clip.is_none() {
                    shapes.extend(marks);
                } else {
                    let marks: Vec<Shape> = marks.collect();
                    if !marks.is_empty() {
                        items.extend(rows.clipped(vec![Item::Shapes(marks)]));
                    }
                }
            }
        }
        if !shapes.is_empty() {
            items.push(Item::Shapes(shapes));
        }
        // The prompt rows of the block at each view's top, pinned over its
        // first rows, their tints and their bars: a background that takes
        // the window's place under them, of its opacity, their cells, the
        // selected block's tint, and a line under them.
        for (pane, snapshot, at, width, dim) in shown() {
            let Some(sticky) = snapshot.sticky() else {
                continue;
            };
            // Pushed up by their block's end, they move up with the rows by
            // the view's share of a row, within the rows' band; otherwise
            // they stay at the view's top.
            let rows = rows_of(snapshot, at, pane);
            let rows = if snapshot.sticky_pushed() {
                rows
            } else {
                cells::Rows::default()
            };
            let at = rows.corner(at);
            let mut pinned = Vec::new();
            let across = (at.0 - pad, width + 2 * bar_padding);
            let height = u32::from(sticky.rows()) * cell.height;
            let band = |y: i32, height: u32, fill: Color, knockout: bool| Shape {
                rect: Rect {
                    x: across.0,
                    y,
                    width: across.1,
                    height,
                },
                radius: 0.0,
                fill,
                border: 0.0,
                border_color: Color::CLEAR,
                shadow: None,
                arc: None,
                knockout,
                mark: None,
            };
            let background = cells::background(snapshot);
            pinned.push(Item::Shapes(vec![band(
                at.1,
                height,
                background.with_alpha(opacity),
                true,
            )]));
            let mut batch = cells::cells_with(sticky, &cell, at, None, None, look, font);
            cells::fade(&mut batch, background, dim);
            pinned.push(Item::Cells(batch));
            let mut over = Vec::new();
            let selected = self
                .plugins
                .selected_block(lokyr_kernel::plugins::host::PaneId(pane.get()));
            if let (Some(tint), Some((command, _))) =
                (tokens.get("blocks.selected.bg"), snapshot.sticky_prompt())
                && selected == Some(command)
            {
                over.push(faded(dim)(band(at.1, height, tint, false)));
            }
            if let Some(line) = tokens.get("blocks.divider") {
                // Casts from floats saturate.
                let thickness = (size.scale.round() as u32).clamp(1, cell.height);
                let below = at.1 + i32::try_from(height).unwrap_or(i32::MAX);
                over.push(faded(dim)(band(below, thickness, line, false)));
            }
            pinned.push(Item::Shapes(over));
            items.extend(rows.clipped(pinned));
        }
        // The dividers between the panes, under the slots' views.
        if let Some(line) = tokens.get("pane.divider") {
            items.extend(state.dividers.iter().map(|divider| Item::Rect {
                rect: divider.line,
                color: line,
            }));
        }
        // The panes' scroll bars, over their cells and under the slots'
        // views.
        let bar_shapes = super::scrollbars::shapes(&state.bars, tokens, size.scale);
        if !bar_shapes.is_empty() {
            items.push(Item::Shapes(bar_shapes));
        }
        let width = (RING * size.scale).round().max(1.0) as u32;
        let rings = |rooms: &[(PaneId, Rect)], floating: bool, color: Option<Color>| -> Vec<Item> {
            let Some(color) = color else {
                return Vec::new();
            };
            rooms
                .iter()
                .filter(|(pane, _)| floating_ids.contains(pane) == floating)
                .flat_map(|&(_, room)| ring(room, width))
                .map(|rect| Item::Rect { rect, color })
                .collect()
        };
        // A ring round each pane of the layout that takes the window's
        // broadcast, and round each whose agent waits, over it.
        items.extend(rings(&broadcasting, false, tokens.get("broadcast.ring")));
        items.extend(rings(&waiting, false, tokens.get("progress.paused")));
        // The floating panes over the layout's, from the back to the front:
        // each its frame, of its header and title, its cells, the images
        // over its text, and its rings.
        for &(floating, room) in &state.floating_rooms {
            let Some(at) = floating_drawn
                .iter()
                .position(|(pane, ..)| *pane == floating)
            else {
                continue;
            };
            let (_, drawn, over) = floating_drawn.swap_remove(at);
            let Some(shown) = self.workspace.pane(floating) else {
                continue;
            };
            let fill = shown.snapshot().colors().background;
            let look = lokyr_ui_core::floating::Look {
                fill: Color::from_rgb8(fill.r, fill.g, fill.b),
                border: tokens.get("panel.border").unwrap_or(Color::CLEAR),
                separator: tokens.get("separator").unwrap_or(Color::CLEAR),
                shadow: tokens.get("shadow"),
                title: tokens
                    .get(if floating == active {
                        "text"
                    } else {
                        "text.secondary"
                    })
                    .unwrap_or(Color::CLEAR),
                close: tokens.get("text.secondary").unwrap_or(Color::CLEAR),
                close_hover: tokens.get("button.hover.bg").unwrap_or(Color::CLEAR),
            };
            let hovers = state.floating_close_hover == Some(floating);
            let title = shown.header_title(true).unwrap_or_default();
            let glyph = |text: &cells::CellText<'_>| weighed(font, text);
            items.extend(lokyr_ui_core::floating::frame(
                &room, &title, look, &cell, size.scale, hovers, glyph,
            ));
            items.extend(drawn);
            items.extend(over);
            let own = [(floating, room.grid)];
            if broadcasting.iter().any(|&(pane, _)| pane == floating) {
                items.extend(rings(&own, true, tokens.get("broadcast.ring")));
            }
            if waiting.iter().any(|&(pane, _)| pane == floating) {
                items.extend(rings(&own, true, tokens.get("progress.paused")));
            }
        }
        // The half of a pane that a tab dragged over it lights up.
        if let (Some(half), Some(color)) = (lit, tokens.get("pane.drop")) {
            items.push(Item::Rect { rect: half, color });
        }
        // The progresses' fills, which the window server draws where it
        // can, so that a value's change slides with no frame of the app's.
        let fills_apart = platform.draws_fills();
        let room = (size.pixels.width as f32, size.pixels.height as f32);
        // The progresses that something drawn over them covers, a node
        // dragged among its siblings, a panel or a tooltip, whose fills and
        // moving parts the frame draws under it, as the window server's
        // would show over it.
        let tooltips: Vec<Rect> = state
            .chrome
            .iter()
            .filter_map(|view| {
                let frame = view.frame?;
                view.surface.tooltip_rect(font, ((frame.x, frame.y), room))
            })
            .collect();
        for view in &mut state.chrome {
            view.surface.animations.always_indicators = always_indicators;
            view.surface.animate(now, reduced);
            view.surface.fills_apart = fills_apart;
            view.surface.covered = if fills_apart {
                chrome::covered(
                    view,
                    &state.panels,
                    &tooltips,
                    (tokens, &self.previews),
                    now,
                )
            } else {
                std::collections::BTreeSet::new()
            };
        }
        let focused_node = match state.focus.target() {
            Target::Node { panel, node } => Some((panel, node)),
            Target::Pane => None,
        };
        let mut painted = chrome::paint(
            &state.chrome,
            font,
            tokens,
            room,
            (chrome_dark, focused_node, now),
            (&self.pictures, &self.previews),
        );
        // Under an opaque titlebar, the chrome in its band goes over it, of
        // its colors' own opacity, rather than knocking it out to the
        // window's.
        if opaque_bar && let Some(Item::Rect { rect: band, .. }) = bar_item(size, background, past)
        {
            over_opaque(&mut painted, band, opacity);
        }
        // The status bar's band across the window, under its plugins' views,
        // while one fills it.
        if let (Some(band), Some(fill)) = (state.status_bar, tokens.get("status-bar.bg")) {
            items.push(Item::Shapes(vec![Shape {
                knockout: tokens.knocks_out("status-bar.bg"),
                ..Shape::filled(chrome::rect_of(band), fill)
            }]));
        }
        items.extend(painted);
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
        let mut areas: Vec<PointerArea> = Vec::new();
        // A column's edge, which its drag resizes, then the slots' views,
        // under the notice and over the panes, the topmost first.
        if let Some((_, _, zone)) = chrome::column_edge(&state.chrome, &self.plugins, size.scale) {
            areas.push(points(&chrome::rect_of(zone), PointerShape::ResizeColumn));
        }
        areas.extend(surfaces::pointer_areas(&state.chrome, size.scale));
        // The floating panes' edges, headers and boxes, the one in front
        // first, over the panes and the dividers under them: inside, the
        // arrow over a program that takes the clicks, the I-beam otherwise.
        let workspace = &self.workspace;
        areas.extend(super::floating::pointer_areas(
            &state.floating_rooms,
            size.scale,
            |pane| {
                let clicks = workspace.pane(pane).is_some_and(|pane| {
                    !pane.is_kept()
                        && pane
                            .shown_modes()
                            .is_some_and(|modes| modes.mouse_tracking != MouseTracking::None)
                });
                if clicks {
                    PointerShape::Arrow
                } else {
                    PointerShape::Text
                }
            },
        ));
        // The dividers' grab areas, under the slots' views.
        areas.extend(
            state
                .dividers
                .iter()
                .map(|divider| super::dividers::grab_area(divider, size.scale)),
        );
        // The arrow over the scroll bars that show, under the slots' views.
        areas.extend(
            state
                .bars
                .iter()
                .filter(|bar| bar.look.opacity > 0.0)
                .map(|bar| points(&bar.area, PointerShape::Arrow)),
        );
        // The hand over the decorations that ask for it, as a link's that
        // Command underlines, of every pane, under the bar.
        for &(pane, corner) in &pointed {
            let Some(pane) = self.workspace.pane(pane) else {
                continue;
            };
            let snapshot = pane.snapshot();
            // Of the rows a share of a row up, within their band.
            let rows = rows_of(snapshot, corner, pane.id());
            let (left, top) = rows.corner(corner);
            let (width, height) = (cell.width, cell.height);
            for span in snapshot
                .decorations()
                .iter()
                .filter(|span| span.look.pointing)
            {
                let rect = Rect {
                    x: left + i32::try_from(u32::from(span.first) * width).unwrap_or(i32::MAX),
                    y: top + i32::try_from(u32::from(span.row) * height).unwrap_or(i32::MAX),
                    width: u32::from(span.last - span.first + 1) * width,
                    height,
                };
                let Some(rect) = rows.clip.map_or(Some(rect), |band| within(rect, band)) else {
                    continue;
                };
                areas.push(points(&rect, PointerShape::PointingHand));
            }
        }
        if areas != state.pointer_areas {
            platform.set_pointer_areas(window, &areas);
            state.pointer_areas = areas;
        }
        // The pointer's moves over the slots' views come to the app, for
        // their hover and their tooltips, and wake it over them alone; the
        // parts of a titlebar of tabs that are the bar's own take its
        // presses, and the panels drawn over a titlebar of either kind.
        let mut moving = surfaces::move_areas(&state.chrome, size.scale);
        // The moves over the scroll bars, shown or not, which the pointer
        // shows as it comes over them.
        moving.extend(state.bars.iter().map(|bar| ContentRect {
            origin: Point {
                x: f64::from(bar.area.x) / size.scale,
                y: f64::from(bar.area.y) / size.scale,
            },
            size: Size {
                width: f64::from(bar.area.width) / size.scale,
                height: f64::from(bar.area.height) / size.scale,
            },
        }));
        // While the keyboard follows the pointer, the moves over the panes
        // that have it not, which the first of gives it them; none wakes
        // the app over the pane that has it.
        if self.plugins.focus_follows_mouse() && state.views.len() > 1 {
            let active = self.workspace.active_pane(state.window);
            let points = |pixels: i32| f64::from(pixels) / size.scale;
            moving.extend(
                state
                    .views
                    .iter()
                    .filter(|view| Some(view.pane) != active)
                    .map(|view| ContentRect {
                        origin: Point {
                            x: points(view.rect.x),
                            y: points(view.rect.y),
                        },
                        size: Size {
                            width: f64::from(view.rect.width) / size.scale,
                            height: f64::from(view.rect.height) / size.scale,
                        },
                    }),
            );
        }
        if moving != state.move_areas {
            platform.set_move_areas(window, &moving);
            state.move_areas = moving;
        }
        let mut own =
            surfaces::titlebar_areas(&state.chrome, size.scale, state.titlebar == Titlebar::Tabs);
        let band = (size.pixels.width as f32, size.bar_pixels() as f32);
        own.extend(surfaces::panel_titlebar_areas(state, band, size.scale));
        if own != state.titlebar_areas {
            platform.set_titlebar_areas(window, &own);
            state.titlebar_areas = own;
        }
        // The slots' indeterminate progresses, which the window server moves.
        let sweeps = chrome::sweeps(
            &state.chrome,
            (tokens, &self.previews),
            (now, state.sweeps.1),
            size.scale,
        );
        if sweeps != state.sweeps.0 {
            platform.set_sweeps(window, &sweeps);
            state.sweeps.0 = sweeps;
        }
        if fills_apart {
            let fills = chrome::fills(&state.chrome, (tokens, &self.previews), now, size.scale);
            if fills != state.fills {
                platform.set_fills(window, &fills);
                state.fills = fills;
            }
        }
        // A frame at each refresh while a slot's transition runs, or a
        // scroll bar fades.
        if chrome::animating(&state.chrome, now) || bars.fading {
            platform.request_frame(window);
        }
        // The grid's size, at the content's centre, while the window
        // resizes.
        let labelled = state
            .size_label
            .is_some_and(|until| state.live_resize || Instant::now() < until);
        if labelled {
            let text = size_label::text(grid.columns, grid.rows);
            let area = (size.pixels.width, size.pixels.height, size.bar_pixels());
            let glyph = |shown: &cells::CellText<'_>| weighed(font, shown);
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
            let glyph = |shown: &cells::CellText<'_>| weighed(font, shown);
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
        if !same && let Some(((pressed, handled), (samples, phases))) = shown_key {
            let framed = Instant::now();
            drawing.renderer.on_next_presented(move |ago| {
                if let Some(shown) = ago.and_then(|ago| Instant::now().checked_sub(ago)) {
                    samples
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push(shown.saturating_duration_since(pressed));
                    phases.lock().unwrap_or_else(PoisonError::into_inner).push([
                        handled.saturating_duration_since(pressed),
                        framed.saturating_duration_since(handled),
                        shown.saturating_duration_since(framed),
                    ]);
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
        // An HDR cursor, past white, where the window's screen has the
        // headroom: its layer of extended range, its color brighter.
        let hdr = (config.hdr && headroom > 1.0).then(|| config.hdr_brightness.encoded());
        state.cursor_layer.extend(hdr.is_some());
        let cursor_cells = cursor_cells.map(|(column, row, mut cells)| {
            if let (Some(factor), Some(cursor)) = (hdr, cells.cursor.as_mut()) {
                cursor.color = brighter(cursor.color, factor);
            }
            (column, row, cells)
        });
        // Its rows a share of a row up, as the view is scrolled by points.
        let top = top - f64::from(rows.up);
        let cursor_shown = cursor_cells.map(|(column, row, cells)| {
            let at = (
                left + f64::from(column) * width,
                top + f64::from(row) * height,
            );
            (cells, at)
        });
        // A move of the cursor's own, in the pane that showed it, of the
        // same view, cells and place, of rows that did not scroll, slides,
        // as the config says, but where the system reduces motion.
        let steady = (
            active,
            rows.corner((x, y)),
            (cell.width, cell.height),
            snapshot.offset(),
        );
        let moved_alone =
            state.cursor_steady.replace(steady) == Some(steady) && snapshot.scrolled() == 0;
        let slide =
            movement
                .slide()
                .filter(|_| moved_alone && !reduced)
                .map(|(duration, curve)| Slide {
                    duration,
                    easing: easing_of(curve),
                });
        show_layer(
            platform,
            drawing,
            (window, Layer::Cursor, !state.occluded),
            &mut state.cursor_layer,
            (cursor_shown, rows),
            size.scale,
            (shown_cursor.and_then(|(_, blink)| blink), slide),
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
            (window, Layer::BlinkingText, !state.occluded),
            &mut state.text_layer,
            (text_shown, rows),
            size.scale,
            (text_blink, None),
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
        // The scroll bar that shows a while fades at its time.
        self.bar_timer_at(platform, bars.due);
    }
}

/// The cells of `panes`, the panes of a window but the one with the
/// keyboard, each at its origin, of `cell`: its selection and its search's
/// matches in `colors`, its cursor `cursor`, as an unfocused window's, and
/// its text as `text` has it look, steady, as the text that blinks blinks
/// in the pane with the keyboard alone.
fn other_cells(
    panes: &[(&Pane, (i32, i32))],
    cell: &CellMetrics,
    (colors, matches): (SelectionColors, MatchColors),
    cursor: CursorLook,
    text: TextLook,
    dim: f32,
    font: &mut dyn cells::Glyphs,
) -> Vec<CellBatch> {
    let look = TextLook {
        blink_shown: true,
        ..text
    };
    panes
        .iter()
        .map(|(pane, origin)| {
            let snapshot = pane.snapshot();
            let selected = snapshot.selection();
            let contains =
                |column: u16, row: u16| selected.is_some_and(|shown| shown.contains(column, row));
            let selection = selected.map(|_| cells::Selection {
                colors,
                contains: &contains,
            });
            let mut batch = cells::cells_with(
                snapshot,
                cell,
                *origin,
                selection.as_ref(),
                Some(cursor),
                look,
                font,
            );
            cells::highlight(&mut batch, snapshot, matches);
            cells::decorate(&mut batch, snapshot, cell);
            // Faded toward its background, as the plugins ask of a pane
            // that has not the keyboard.
            cells::fade(&mut batch, cells::background(snapshot), dim);
            batch
        })
        .collect()
}

/// The glyph of `shown`, of the terminal's font, for a text of the app's
/// own drawn in it, as the notice's: of the system's weight where its
/// coverage was smoothed and the terminal's text takes the weight.
fn weighed<B: FontBackend>(
    font: &mut Font<B>,
    shown: &cells::CellText<'_>,
) -> Option<cells::AtlasGlyph> {
    let system_weight = cells::Glyphs::system_weight(&*font);
    font.glyph(shown).map(|glyph| cells::AtlasGlyph {
        smoothed: glyph.smoothed && system_weight,
        ..glyph
    })
}

/// The cells of `cell` pixels that `pixels` hold, one at least.
fn fit(pixels: u32, cell: u32) -> u16 {
    (pixels / cell.max(1)).clamp(1, u32::from(u16::MAX)) as u16
}

/// The room between the two parts of a split, in pixels: the divider's
/// line, and each pane's padding on either side of it, across and down, as
/// every pane keeps its padding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Gap {
    /// The divider's line's thickness.
    pub(crate) line: u32,
    /// The panes' padding at their left and right, and at their top and
    /// bottom.
    pub(crate) padding: (u32, u32),
}

impl Gap {
    /// A gap of the line alone, of no padding.
    #[cfg(test)]
    pub(crate) const fn line(line: u32) -> Self {
        Self {
            line,
            padding: (0, 0),
        }
    }

    /// The padding along `direction`, a split's, and across it.
    fn paddings(self, direction: Direction) -> (u32, u32) {
        match direction {
            Direction::Horizontal => self.padding,
            Direction::Vertical => (self.padding.1, self.padding.0),
        }
    }
}

/// The width, in points, of the ring round a pane whose agent waits, and
/// round one that takes its window's broadcast.
const RING: f64 = 2.0;

/// The four sides of a ring of `width` pixels inside `room`.
pub(super) fn ring(room: Rect, width: u32) -> [Rect; 4] {
    let width = width.min(room.width / 2).min(room.height / 2);
    let side = i32::try_from(width).unwrap_or(0);
    let middle = room.height.saturating_sub(2 * width);
    [
        Rect {
            height: width,
            ..room
        },
        Rect {
            y: room.y + i32::try_from(room.height).unwrap_or(i32::MAX) - side,
            height: width,
            ..room
        },
        Rect {
            y: room.y + side,
            width,
            height: middle,
            ..room
        },
        Rect {
            x: room.x + i32::try_from(room.width).unwrap_or(i32::MAX) - side,
            y: room.y + side,
            width,
            height: middle,
        },
    ]
}

/// `batch` as two: its cells' backgrounds alone, and the rest of no
/// background, so that what draws between them, as an image under the
/// text, lies over the backgrounds and under the text and the cursor.
fn backgrounds_apart(mut batch: CellBatch) -> (CellBatch, CellBatch) {
    let backgrounds = CellBatch {
        backgrounds: std::mem::take(&mut batch.backgrounds),
        glyphs: Vec::new(),
        lines: Vec::new(),
        cursor: None,
        ..batch
    };
    (backgrounds, batch)
}
/// The rooms of the grids of the panes of `layout` in `area`, in pixels: a
/// split's first part takes its share of the area less `gap`, the divider's
/// line and the panes' padding on either side of it, cut at a whole cell of
/// `cell`, its width or its height, so that its grid fills it, and the
/// second the rest, past the gap; `zoomed`, while one is, takes the whole
/// area. In the layout's order.
pub(super) fn pane_rects(
    layout: &Layout,
    zoomed: Option<PaneId>,
    area: Rect,
    cell: (u32, u32),
    gap: Gap,
) -> Vec<(PaneId, Rect)> {
    if let Some(zoomed) = zoomed.filter(|&pane| layout.contains(pane)) {
        return vec![(zoomed, area)];
    }
    let mut rooms = Vec::new();
    split_rooms(layout, area, cell, gap, &mut rooms);
    rooms
}

fn split_rooms(
    layout: &Layout,
    area: Rect,
    cell: (u32, u32),
    gap: Gap,
    rooms: &mut Vec<(PaneId, Rect)>,
) {
    let Layout::Split {
        direction,
        first,
        second,
        ..
    } = layout
    else {
        if let Layout::Leaf(pane) = layout {
            rooms.push((*pane, area));
        }
        return;
    };
    let (first_area, _, second_area, _) = cut(layout, *direction, area, cell, gap);
    split_rooms(first, first_area, cell, gap, rooms);
    split_rooms(second, second_area, cell, gap, rooms);
}

/// The parts of `area`, in pixels, that `layout`, a split of `direction`,
/// gives its first part, its divider and its second part: the first its
/// share of the area less `gap`, cut at a whole cell of `cell`, so that its
/// grid fills it, of one cell at least, and of as many as leave the second
/// one, if the room has two; the divider's line in the gap's middle, past
/// the first part's padding, and as long as the area and the padding
/// across at either end, so that the lines meet and reach the panes' room's
/// edges; the second the rest, past the gap. And the room that the parts
/// share, the area less the gap.
fn cut(
    layout: &Layout,
    direction: Direction,
    area: Rect,
    cell: (u32, u32),
    gap: Gap,
) -> (Rect, Rect, Rect, u32) {
    let share = layout.share().unwrap_or(0.5);
    let (length, step) = match direction {
        Direction::Horizontal => (area.width, cell.0.max(1)),
        Direction::Vertical => (area.height, cell.1.max(1)),
    };
    let (along, across) = gap.paddings(direction);
    let between = gap.line.saturating_add(along.saturating_mul(2));
    let room = length.saturating_sub(between);
    let cells = (f64::from(room) * share / f64::from(step)).round() as u32;
    let most = (room / step).saturating_sub(1).max(1);
    let first_length = (cells.clamp(1, most) * step).min(room);
    let second_length = room - first_length;
    let at = |length: u32| i32::try_from(length).unwrap_or(i32::MAX);
    let parts = match direction {
        Direction::Horizontal => (
            Rect {
                width: first_length,
                ..area
            },
            Rect {
                x: area.x.saturating_add(at(first_length + along)),
                y: area.y.saturating_sub(at(across)),
                width: gap.line.min(length),
                height: area.height.saturating_add(across.saturating_mul(2)),
            },
            Rect {
                x: area.x.saturating_add(at(first_length + between)),
                width: second_length,
                ..area
            },
        ),
        Direction::Vertical => (
            Rect {
                height: first_length,
                ..area
            },
            Rect {
                x: area.x.saturating_sub(at(across)),
                y: area.y.saturating_add(at(first_length + along)),
                width: area.width.saturating_add(across.saturating_mul(2)),
                height: gap.line.min(length),
            },
            Rect {
                y: area.y.saturating_add(at(first_length + between)),
                height: second_length,
                ..area
            },
        ),
    };
    (parts.0, parts.1, parts.2, room)
}

/// A divider of a tab's layout: the split's path from the layout's top,
/// its direction, its line between its parts, and the split's whole area,
/// in pixels.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Divider {
    pub(crate) path: Vec<lokyr_kernel::workspace::Side>,
    pub(crate) direction: Direction,
    pub(crate) line: Rect,
    pub(crate) area: Rect,
    /// The room that the split's parts share, along its direction: its
    /// area less the gap between them.
    pub(crate) room: u32,
}

/// The dividers of `layout` in `area`, as [`pane_rects`] lays its panes
/// out: none while a pane is zoomed. In the layout's order, a split before
/// its parts'.
pub(super) fn dividers(
    layout: &Layout,
    zoomed: Option<PaneId>,
    area: Rect,
    cell: (u32, u32),
    gap: Gap,
) -> Vec<Divider> {
    let mut found = Vec::new();
    if zoomed.is_none_or(|pane| !layout.contains(pane)) {
        divider_rooms(layout, (area, Vec::new()), cell, gap, &mut found);
    }
    found
}

fn divider_rooms(
    layout: &Layout,
    (area, path): (Rect, Vec<lokyr_kernel::workspace::Side>),
    cell: (u32, u32),
    gap: Gap,
    found: &mut Vec<Divider>,
) {
    use lokyr_kernel::workspace::Side;

    let Layout::Split {
        direction,
        first,
        second,
        ..
    } = layout
    else {
        return;
    };
    let (first_area, line, second_area, room) = cut(layout, *direction, area, cell, gap);
    found.push(Divider {
        path: path.clone(),
        direction: *direction,
        line,
        area,
        room,
    });
    let deeper = |side| {
        let mut path = path.clone();
        path.push(side);
        path
    };
    divider_rooms(first, (first_area, deeper(Side::First)), cell, gap, found);
    divider_rooms(
        second,
        (second_area, deeper(Side::Second)),
        cell,
        gap,
        found,
    );
}

/// The titlebar of a window of `size`, over its content's top, opaque in
/// `background`'s color, from `left` pixels across, as a sidebar leaves
/// it; `None` without one.
pub(super) fn bar_item(size: WindowSize, background: Color, left: u32) -> Option<Item> {
    let height = size.bar_pixels();
    let left = left.min(size.pixels.width);
    (height > 0 && left < size.pixels.width).then_some(Item::Rect {
        rect: Rect {
            x: i32::try_from(left).unwrap_or(i32::MAX),
            y: 0,
            width: size.pixels.width - left,
            height,
        },
        color: background.with_alpha(1.0),
    })
}

/// `items`' shapes that knock out what is under them and meet `band`, an
/// opaque titlebar's, of a window of `opacity`, made to go over it, of
/// their colors' own opacity, which the window's took a share of.
pub(super) fn over_opaque(items: &mut [Item], band: Rect, opacity: f32) {
    let meets = |rect: &Rect| {
        let right = i64::from(rect.x) + i64::from(rect.width);
        let bottom = i64::from(rect.y) + i64::from(rect.height);
        i64::from(rect.x) < i64::from(band.x) + i64::from(band.width)
            && right > i64::from(band.x)
            && i64::from(rect.y) < i64::from(band.y) + i64::from(band.height)
            && bottom > i64::from(band.y)
    };
    for item in items {
        let Item::Shapes(shapes) = item else {
            continue;
        };
        for shape in shapes
            .iter_mut()
            .filter(|shape| shape.knockout && meets(&shape.rect))
        {
            shape.knockout = false;
            if opacity > 0.0 {
                shape.fill.alpha = (shape.fill.alpha / opacity).min(1.0);
            }
        }
    }
}

/// The appearance that a window whose default background is `background`
/// takes, in the app's `appearance`, for its title to read: its
/// background's, light or dark, when it is not the app's; `None` for the
/// app's. A background is light past WCAG's luminance where black and
/// white text contrast alike with it.
pub(super) fn appearance_over(background: Color, appearance: Appearance) -> Option<Appearance> {
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

/// Lokyr's own themes, Lokyr Light and Lokyr Dark, which the settings
/// window draws of.
static BUILT_IN: std::sync::LazyLock<lokyr_ui_core::theme::Theme> =
    std::sync::LazyLock::new(lokyr_ui_core::theme::Theme::builtin);

/// The chrome's tokens of `theme` in the window's look `look`: its
/// appearance, the terminal's background and foreground, and the window's
/// opacity, of which the chrome's backgrounds are as translucent, knocked
/// out to the window's where it is translucent.
fn chrome_tokens(
    theme: &lokyr_ui_core::theme::Theme,
    (dark, background, foreground, opacity): (bool, Color, Color, f32),
) -> lokyr_ui_core::theme::Tokens {
    let mut tokens = theme.variant(dark).tokens.clone();
    tokens.set("terminal.bg", background.with_alpha(opacity));
    tokens.set("terminal.fg", foreground.with_alpha(1.0));
    // The chrome's backgrounds, as the window's own is, as translucent as
    // the config makes the window, the active tab's too.
    for name in TRANSLUCENT {
        if let Some(color) = tokens.get(name) {
            tokens.set(name, color.with_alpha(color.alpha * opacity));
        }
    }
    if opacity < 1.0 {
        for name in TRANSLUCENT.into_iter().chain(["terminal.bg"]) {
            tokens.knock_out(name);
        }
    }
    tokens
}

impl<B: Backend> App<B> {
    /// Draws `window` of `size` that shows no pane, as of the settings tab:
    /// the window's background of the theme's terminal's colors, its
    /// slots' views and its panels, of their pointer's, moves' and
    /// titlebar's areas; no cursor nor blinking text; the menus of no
    /// pane's context.
    fn draw_without_panes(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        size: WindowSize,
        now: Instant,
    ) {
        // The settings window of Lokyr's own theme of the appearance,
        // whatever the theme in force.
        let own_theme = self.shows_settings(window);
        let colors = if own_theme {
            BUILT_IN
                .variant(platform.appearance() == Appearance::Dark)
                .colors
                .clone()
        } else {
            self.colors(platform.appearance())
        };
        let (bg, fg) = (colors.background, colors.foreground);
        let background = Color::from_rgb8(bg.r, bg.g, bg.b);
        let foreground = Color::from_rgb8(fg.r, fg.g, fg.b);
        let backdrop = self.backdrop(window);
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        if backdrop != state.backdrop {
            platform.set_translucent(window, backdrop);
            state.backdrop = backdrop;
        }
        let own = appearance_over(background, platform.appearance());
        if state.own_appearance != Some(own) {
            platform.set_window_appearance(window, own);
            state.own_appearance = Some(own);
        }
        let focused = state.focused;
        self.refresh_title(platform, window, false);
        if focused {
            self.settle_menu(
                platform,
                &lokyr_kernel::keymap::Context::new(Vec::<&str>::new()),
            );
        }
        let dark = platform.appearance() == Appearance::Dark;
        let chrome_dark = own.map_or(dark, |own| own == Appearance::Dark);
        let opacity = if backdrop.is_some() {
            self.config.window.background_opacity.value()
        } else {
            1.0
        };
        let chrome_look = (chrome_dark, background, foreground, opacity);
        let reduced = platform.reduce_motion();
        // The views' scroll indicators show always where the system shows
        // its scroll bars so, else while their views scroll.
        let always_indicators = !platform.scrollers().overlay;
        // The theme in force's specimen, which a view's terminal specimen
        // draws, as the panes would: a window of no panes, as the settings
        // window, alone takes it.
        self.previews.specimen = Some(self.specimen(self.theme.variant(dark)));
        let (Some(drawing), Some(state)) = (self.drawing.as_mut(), self.windows.get_mut(&window))
        else {
            return;
        };
        if state.surface.is_none() || state.occluded {
            return;
        }
        if state
            .chrome_tokens
            .as_ref()
            .is_none_or(|(kept, _)| *kept != chrome_look)
        {
            let theme = if own_theme { &*BUILT_IN } else { &self.theme };
            state.chrome_tokens = Some((chrome_look, chrome_tokens(theme, chrome_look)));
        }
        // The window's accessibility tree goes within its rate as its views
        // or its focus change, once a client asked.
        if platform.accessibility_asked(window)
            && (state.views_changed || state.access_focus.is_some())
        {
            state.access_changed = true;
            if self.access_timer.is_none() {
                self.access_timer = Some(platform.start_timer(now + access::RATE));
            }
        }
        let Some((_, tokens)) = state.chrome_tokens.as_ref() else {
            return;
        };
        let font = &mut drawing.font;
        let fills_apart = platform.draws_fills();
        let room = (size.pixels.width as f32, size.pixels.height as f32);
        let tooltips: Vec<Rect> = state
            .chrome
            .iter()
            .filter_map(|view| {
                let frame = view.frame?;
                view.surface.tooltip_rect(font, ((frame.x, frame.y), room))
            })
            .collect();
        for view in &mut state.chrome {
            view.surface.animations.always_indicators = always_indicators;
            view.surface.animate(now, reduced);
            view.surface.fills_apart = fills_apart;
            view.surface.covered = if fills_apart {
                chrome::covered(
                    view,
                    &state.panels,
                    &tooltips,
                    (tokens, &self.previews),
                    now,
                )
            } else {
                std::collections::BTreeSet::new()
            };
        }
        let focused_node = match state.focus.target() {
            Target::Node { panel, node } => Some((panel, node)),
            Target::Pane => None,
        };
        // A render took a field's composition away, or set its text: the
        // input method forgets it. Its candidates go at the field with the
        // keyboard's focus.
        if std::mem::take(&mut state.composition_dropped) {
            platform.end_composition(window);
        }
        let field_area = focused_node.and_then(|(panel, node)| {
            let view = state.chrome.iter().find(|view| view.id == panel)?;
            let frame = view.frame?;
            let area = view.surface.text_area(node, font)?;
            Some((area.x + frame.x, area.y + frame.y, area.width, area.height))
        });
        if let Some((x, y, width, height)) = field_area {
            platform.set_text_area(
                window,
                Point {
                    x: f64::from(x) / size.scale,
                    y: f64::from(y) / size.scale,
                },
                Size {
                    width: f64::from(width) / size.scale,
                    height: f64::from(height) / size.scale,
                },
            );
        }
        let items = chrome::paint(
            &state.chrome,
            font,
            tokens,
            room,
            (chrome_dark, focused_node, now),
            (&self.pictures, &self.previews),
        );
        // The slots' views' pointers, moves and titlebar's parts, as the
        // panes' frame gives them.
        let mut areas: Vec<PointerArea> = Vec::new();
        if let Some((_, _, zone)) = chrome::column_edge(&state.chrome, &self.plugins, size.scale) {
            let rect = chrome::rect_of(zone);
            areas.push(PointerArea {
                origin: Point {
                    x: f64::from(rect.x) / size.scale,
                    y: f64::from(rect.y) / size.scale,
                },
                size: Size {
                    width: f64::from(rect.width) / size.scale,
                    height: f64::from(rect.height) / size.scale,
                },
                shape: PointerShape::ResizeColumn,
            });
        }
        areas.extend(surfaces::pointer_areas(&state.chrome, size.scale));
        if areas != state.pointer_areas {
            platform.set_pointer_areas(window, &areas);
            state.pointer_areas = areas;
        }
        let moving = surfaces::move_areas(&state.chrome, size.scale);
        if moving != state.move_areas {
            platform.set_move_areas(window, &moving);
            state.move_areas = moving;
        }
        let mut own =
            surfaces::titlebar_areas(&state.chrome, size.scale, state.titlebar == Titlebar::Tabs);
        let band = (size.pixels.width as f32, size.bar_pixels() as f32);
        own.extend(surfaces::panel_titlebar_areas(state, band, size.scale));
        if own != state.titlebar_areas {
            platform.set_titlebar_areas(window, &own);
            state.titlebar_areas = own;
        }
        let sweeps = chrome::sweeps(
            &state.chrome,
            (tokens, &self.previews),
            (now, state.sweeps.1),
            size.scale,
        );
        if sweeps != state.sweeps.0 {
            platform.set_sweeps(window, &sweeps);
            state.sweeps.0 = sweeps;
        }
        if fills_apart {
            let fills = chrome::fills(&state.chrome, (tokens, &self.previews), now, size.scale);
            if fills != state.fills {
                platform.set_fills(window, &fills);
                state.fills = fills;
            }
        }
        if chrome::animating(&state.chrome, now) {
            platform.request_frame(window);
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
        match drawn {
            Ok(true) => {
                state.drawn_at = Some(Instant::now());
                state.shown = Some(Shown {
                    pixels,
                    scale: size.scale,
                    fonts: drawing.fonts,
                    background: list.background,
                    items: list.items,
                });
            }
            Ok(false) => {}
            Err(error) => eprintln!("lokyr: {error}"),
        }
        // No cursor nor blinking text of a pane.
        show_layer(
            platform,
            drawing,
            (window, Layer::Cursor, false),
            &mut state.cursor_layer,
            (None, cells::Rows::default()),
            size.scale,
            (None, None),
        );
        let was_shown = state.text_layer.place.is_some();
        show_layer(
            platform,
            drawing,
            (window, Layer::BlinkingText, false),
            &mut state.text_layer,
            (None, cells::Rows::default()),
            size.scale,
            (None, None),
        );
        if was_shown && state.text_layer.place.is_none() {
            state.text_layer.release();
        }
        if drawing.font.has_deferred() {
            platform.request_frame(window);
        }
    }
}

/// A layer's cells, and where their top left corner goes, in the window's
/// pixels.
type LayerCells = (CellBatch, (f64, f64));

/// Shows `shown` in `layer` of `window`, which `state` keeps: cells, which
/// `drawing` draws into the layer's surface as they change, as large as
/// they are and as their margin on each side, their batch's corner, at a
/// corner in pixels of the content, of `scale`, blinking as `blink` says,
/// and sliding there as `slide` says, from where it showed, of its size;
/// over `rows` drawn a share of a row up, the part of it within their band
/// alone, and none past it. `None` hides the layer.
fn show_layer<B: Backend>(
    platform: &mut dyn Platform,
    drawing: &mut Drawing<B>,
    (window, layer, visible): (WindowId, Layer, bool),
    state: &mut WindowLayer<B>,
    (shown, rows): (Option<LayerCells>, cells::Rows),
    scale: f64,
    (blink, slide): (Option<PlatformBlink>, Option<Slide>),
) {
    let mut undrawn = false;
    let place = shown.and_then(|(cells, (x, y))| {
        let surface = state.surface.as_ref()?;
        let margin = |pixels: i32| u32::try_from(pixels).unwrap_or(0) * 2;
        let width = u32::from(cells.columns) * cells.cell_width + margin(cells.x);
        let height = u32::from(cells.rows) * cells.cell_height + margin(cells.y);
        // Over rows drawn a share of a row up, the part of it within their
        // band alone, as the window server cuts no layer: none past it.
        let part = match rows.layer((x, y), (width, height)) {
            cells::Shows::All => None,
            cells::Shows::Part(part) => Some(part),
            cells::Shows::Nothing => return None,
        };
        let drawn = (cells, part, scale, drawing.fonts);
        if state.drawn.as_ref() != Some(&drawn) {
            let cells = Item::Cells(drawn.0.clone());
            let items = match part {
                Some(part) => vec![Item::PushClip(part), cells, Item::PopClip],
                None => vec![cells],
            };
            let list = DisplayList {
                background: Color::default().with_alpha(0.0),
                items,
                frame: drawing.font.atlas_frame(),
            };
            surface.resize(width, height, scale);
            match drawing.renderer.draw_untimed(surface, &list) {
                Ok(true) => state.drawn = Some(drawn),
                Ok(false) => undrawn = true,
                Err(error) => {
                    eprintln!("lokyr: {error}");
                    undrawn = true;
                }
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
            slide: None,
        })
    });
    // Cells that did not draw, as no drawable came, leave the layer where
    // its last cells show, of no other place, which would show them over
    // other cells; the next frame draws them again, while the window shows.
    if undrawn {
        if visible {
            platform.request_frame(window);
        }
        return;
    }
    if state.place != place {
        let sent = match (&state.place, place) {
            (Some(before), Some(after))
                if before.size == after.size && before.origin != after.origin =>
            {
                Some(LayerPlace { slide, ..after })
            }
            _ => place,
        };
        platform.place_layer(window, layer, sent.as_ref());
        state.place = place;
    }
}

/// The part of `rect` within `band`, from its top to its bottom; `None`
/// past it.
fn within(rect: Rect, band: Rect) -> Option<Rect> {
    let top = rect.y.max(band.y);
    let bottom = (i64::from(rect.y) + i64::from(rect.height))
        .min(i64::from(band.y) + i64::from(band.height));
    let height = u32::try_from(bottom - i64::from(top)).ok()?;
    (height > 0).then_some(Rect {
        y: top,
        height,
        ..rect
    })
}

/// `color` of its components, encoded as sRGB's are, times `factor`, past
/// white beyond 1, as a surface of extended range shows them; its opacity
/// as it is.
fn brighter(color: Color, factor: f32) -> Color {
    Color {
        red: color.red * factor,
        green: color.green * factor,
        blue: color.blue * factor,
        alpha: color.alpha,
    }
}

/// The platform's curve of `curve`.
fn easing_of(curve: cursor::Curve) -> Easing {
    match curve {
        cursor::Curve::Linear => Easing::Linear,
        cursor::Curve::EaseIn => Easing::EaseIn,
        cursor::Curve::EaseOut => Easing::EaseOut,
        cursor::Curve::EaseInOut => Easing::EaseInOut,
    }
}

/// The platform's blink of `pattern`, from `start`.
fn blink_of(pattern: cursor::Pattern, start: Instant) -> PlatformBlink {
    let easing = easing_of;
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

/// Whether `pane`, `id`, which the plugins were told the pointer is over,
/// shows its rows at another share of a row than `share`, its last frame's,
/// as they scroll by pixels.
fn shifted_under(told: Option<PointerTold>, id: PaneId, pane: &Pane, share: f32) -> bool {
    told.is_some_and(|(over, _, _)| over == id) && pane.snapshot().fraction() != share
}

/// Whether the last update of `pane`, of the id `id`, changed the text
/// under the pointer that the plugins were told of, `told`: its row, or
/// the rows that scrolled under it.
fn text_changed_under(told: Option<PointerTold>, id: PaneId, pane: &Pane) -> bool {
    let Some((over, Some((_, row)), _)) = told else {
        return false;
    };
    let snapshot = pane.snapshot();
    if over != id {
        return false;
    }
    if snapshot.scrolled() != 0 {
        return true;
    }
    snapshot.view_y(row).is_some_and(|y| snapshot.changed(y))
}
