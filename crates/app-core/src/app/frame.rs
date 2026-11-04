//! The windows' frames: the grid that fits each window, its padding and
//! its origin under the titlebar; the display list of its pane, its
//! cursor and its overlays, the notice's bar and the size's label among
//! them, drawn into its surface; the layers of the cursor's cells and of
//! the text that blinks, which the window server blinks; and the timers
//! of their blinking and of the labels.

use std::sync::{Arc, PoisonError};
use std::time::Instant;

use lokyr_kernel::commands::ErrorCategory;
use lokyr_kernel::diagnostics::About;
use lokyr_kernel::plugins;
use lokyr_kernel::workspace::{Direction, Grid, Layout, Pane, PaneId};
use lokyr_platform_core::{
    Appearance, Backdrop, Blink as PlatformBlink, Easing, Layer, LayerPlace, Platform, Point,
    PointerArea, PointerShape, Size, Titlebar, WindowId, WindowSize,
};
use lokyr_render_core::{
    CellBatch, Color, CursorShape, DisplayList, Item, Rect, Renderer, Surface as _,
};
use lokyr_text_core::{CellMetrics, FontBackend};
use lokyr_ui_core::focus::Target;
use lokyr_ui_core::font::Font;
use lokyr_ui_core::pane::{CursorLook, TextLook};
use lokyr_ui_core::text::TextConfig;
use lokyr_ui_core::theme::SelectionColors;
use lokyr_ui_core::theme::Variant;
use lokyr_ui_core::{cursor, pane as cells};
use lokyr_vt_core::terminal::MouseTracking;

use super::access;
use super::chrome;
use super::menus::menu_context;
use super::surfaces;
use super::{App, FLASH_OPACITY, GLYPHS_BUDGET, UNDRAWN_CELL};
use crate::drawing::{Backend, Drawing};
use crate::output_bench::Phases;
use crate::window::{PaneView, Shown, WindowLayer};
use crate::{notice, performance, size_label};

/// The room between the two parts of a split, in points, which their
/// divider takes.
const DIVIDER: f64 = 1.0;

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
        let Some(state) = self.windows.get(&window) else {
            return Vec::new();
        };
        let tab = self
            .workspace
            .window(state.window)
            .and_then(|shown| self.workspace.tab(shown.active_tab()));
        let rooms = match tab {
            Some(tab) => {
                let divider = (DIVIDER * size.scale).round() as u32;
                pane_rects(
                    tab.layout(),
                    tab.zoomed(),
                    content,
                    (width, height),
                    divider,
                )
            }
            None => Vec::new(),
        };
        // Each pane's header at its room's top, and its grid below it.
        let mut rooms = rooms;
        self.lay_out_headers(window, &mut rooms);
        let pixels = |cell: u32| u16::try_from(cell).unwrap_or(u16::MAX);
        let views: Vec<PaneView> = rooms
            .into_iter()
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
        }
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
    fn padding(&self, scale: f64) -> (u32, u32) {
        let window = &self.config.window;
        (
            window.padding_x.pixels(scale),
            window.padding_y.pixels(scale),
        )
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

    /// Draws `window`, of `size`: its program's screen, brought up to date,
    /// in the grid that fits its content within the padding, with the
    /// cursor.
    pub(super) fn draw(&mut self, platform: &mut dyn Platform, window: WindowId, size: WindowSize) {
        // The frame's time, which the performance's panel tells, the wait
        // for the display's drawable apart from the CPU's.
        let started =
            (self.performance.is_some() || self.output_bench.is_some()).then(Instant::now);
        let now = Instant::now();
        // The slots that show a view first ask their plugins for it, whose
        // calls' requests the turn's end takes.
        self.rendered_slots |= self.render_slots(platform, window);
        // Each pane of the active tab in its room, around the slots' views.
        let views = self.lay_out_panes(window, size);
        // The pointer's moves while Command is held, for the plugins that
        // follow it, as `links` does.
        let pointer_moves = self.command_held && self.plugins.subscribes(plugins::POINTER);
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
        // Each pane's program learns its grid, as a live resize pauses or
        // ends, and the others' output shows in this frame too.
        let mut due = None;
        let mut others = false;
        let live = state.live_resize;
        for view in &views {
            let Some(pane) = self.workspace.pane_mut(view.pane) else {
                continue;
            };
            let told = if live {
                pane.resize_live(view.grid, now)
            } else {
                pane.resize(view.grid, now)
            };
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
                others |= pane.update();
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
        let output = pane.update();
        let copied = started.map(|_| Instant::now());
        if output {
            state.blink.restart(now);
        }
        // The panes' screens, or the keyboard's pane, changed: the window's
        // accessibility tree goes within its rate, once a client asked.
        if (output || others || state.access_focus != focused)
            && platform.accessibility_asked(window)
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
                bench
                    .pending
                    .take()
                    .map(|time| (time, Arc::clone(&bench.samples)))
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
        let (x, y, width, height) = field_area.map_or(
            (
                f64::from(left) + f64::from(column) * width,
                f64::from(top) + f64::from(row) * height,
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
            .and_then(|_| self.workspace.pane(active))
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
        let dark = platform.appearance() == Appearance::Dark;
        let reduced = platform.reduce_motion();
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
        };
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
        let mut other_batches = other_cells(&others, &cell, selection_colors, hollow, text, font);
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
            other_batches = other_cells(&others, &cell, selection_colors, hollow, text, font);
        }
        // The plugins' decorations over the text, under the composition.
        cells::decorate(&mut batch, snapshot, &cell);
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
        items.extend(other_batches.into_iter().map(Item::Cells));
        // The slots' views over the panes, of the theme's tokens.
        let tokens = &self.theme.variant(dark).tokens;
        for view in &mut state.chrome {
            view.surface.animate(now, reduced);
        }
        let room = (size.pixels.width as f32, size.pixels.height as f32);
        let focused_node = match state.focus.target() {
            Target::Node { panel, node } => Some((panel, node)),
            Target::Pane => None,
        };
        items.extend(chrome::paint(
            &state.chrome,
            font,
            tokens,
            room,
            (dark, focused_node, now),
        ));
        // The notice, across the panes' top, under the titlebar and the tab
        // bar.
        state.notice = self.diagnostics.shown().map(|shown| {
            let label = match (&shown.about, shown.category()) {
                (About::Config, _) => "notice-config",
                (About::Font, _) => "notice-font",
                (About::GlobalKeys, _) => "notice-global-keys",
                (About::Term, _) => "notice-term",
                (About::Other, ErrorCategory::Fault) => "notice-fault",
                (About::Other, _) => "notice-failed",
            };
            let label = self.texts.text(label);
            let fault = shown.category() == ErrorCategory::Fault;
            let area = (size.pixels.width, state.panes_top.max(size.bar_pixels()));
            let glyph = |text: &cells::CellText<'_>| weighed(font, text);
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
        let mut areas: Vec<PointerArea> = state
            .notice
            .iter()
            .flat_map(|rects| {
                [
                    points(&rects.close, PointerShape::PointingHand),
                    points(&rects.bar, PointerShape::Arrow),
                ]
            })
            .collect();
        // The slots' views, under the notice and over the panes, the
        // topmost first.
        areas.extend(surfaces::pointer_areas(&state.chrome, size.scale));
        // The hand over the decorations that ask for it, as a link's that
        // Command underlines, of every pane, under the bar.
        for (pane, (left, top)) in &pointed {
            let Some(pane) = self.workspace.pane(*pane) else {
                continue;
            };
            let (width, height) = (cell.width, cell.height);
            for span in pane
                .snapshot()
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
                areas.push(points(&rect, PointerShape::PointingHand));
            }
        }
        if areas != state.pointer_areas {
            platform.set_pointer_areas(window, &areas);
            state.pointer_areas = areas;
        }
        // The titlebar of a tab bar while one shows, the system's otherwise;
        // a quick window has none.
        let titlebar = if chrome::has_tab_bar(&state.chrome) {
            Titlebar::Tabs
        } else {
            Titlebar::Titled
        };
        if titlebar != state.titlebar && self.quick.is_none_or(|(quick, _)| quick != window) {
            platform.set_titlebar(window, titlebar);
            state.titlebar = titlebar;
        }
        // The pointer's moves over the slots' views come to the app, for
        // their hover and their tooltips, and wake it over them alone; the
        // parts of a titlebar of tabs that are the bar's own take its
        // presses.
        let moving = surfaces::move_areas(&state.chrome, size.scale);
        if moving != state.move_areas {
            platform.set_move_areas(window, &moving);
            state.move_areas = moving;
        }
        let own = if state.titlebar == Titlebar::Tabs {
            surfaces::titlebar_areas(&state.chrome, size.scale)
        } else {
            Vec::new()
        };
        if own != state.titlebar_areas {
            platform.set_titlebar_areas(window, &own);
            state.titlebar_areas = own;
        }
        // The slots' indeterminate progresses, which the window server moves.
        let sweeps = chrome::sweeps(&state.chrome, tokens, (now, state.sweeps.1), size.scale);
        if sweeps != state.sweeps.0 {
            platform.set_sweeps(window, &sweeps);
            state.sweeps.0 = sweeps;
        }
        // A frame at each refresh while a slot's transition runs.
        if chrome::animating(&state.chrome, now) {
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

/// The cells of `panes`, the panes of a window but the one with the
/// keyboard, each at its origin, of `cell`: its selection in `colors`, its
/// cursor `cursor`, as an unfocused window's, and its text as `text` has it
/// look, steady, as the text that blinks blinks in the pane with the
/// keyboard alone.
fn other_cells(
    panes: &[(&Pane, (i32, i32))],
    cell: &CellMetrics,
    colors: SelectionColors,
    cursor: CursorLook,
    text: TextConfig,
    font: &mut dyn cells::Glyphs,
) -> Vec<CellBatch> {
    let look = TextLook {
        config: text,
        blink_shown: true,
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
            cells::decorate(&mut batch, snapshot, cell);
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

/// The rooms of the panes of `layout` in `area`, in pixels: a split's first
/// part takes its share of the area less `divider`, the pixels between the
/// parts, cut at a whole cell of `cell`, its width or its height, so that
/// its grid fills it, and the second the rest, past the divider; `zoomed`,
/// while one is, takes the whole area. In the layout's order.
pub(super) fn pane_rects(
    layout: &Layout,
    zoomed: Option<PaneId>,
    area: Rect,
    cell: (u32, u32),
    divider: u32,
) -> Vec<(PaneId, Rect)> {
    if let Some(zoomed) = zoomed.filter(|&pane| layout.contains(pane)) {
        return vec![(zoomed, area)];
    }
    let mut rooms = Vec::new();
    split_rooms(layout, area, cell, divider, &mut rooms);
    rooms
}

fn split_rooms(
    layout: &Layout,
    area: Rect,
    cell: (u32, u32),
    divider: u32,
    rooms: &mut Vec<(PaneId, Rect)>,
) {
    let share = layout.share();
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
    let share = share.unwrap_or(0.5);
    let (length, step) = match direction {
        Direction::Horizontal => (area.width, cell.0.max(1)),
        Direction::Vertical => (area.height, cell.1.max(1)),
    };
    let room = length.saturating_sub(divider);
    // Whole cells for the first part, of one cell at least, and as many as
    // leave the second one, if the room has two.
    let cells = (f64::from(room) * share / f64::from(step)).round() as u32;
    let most = (room / step).saturating_sub(1).max(1);
    let first_length = (cells.clamp(1, most) * step).min(room);
    let second_length = room - first_length;
    let past = i32::try_from(first_length + divider).unwrap_or(i32::MAX);
    let (first_area, second_area) = match direction {
        Direction::Horizontal => (
            Rect {
                width: first_length,
                ..area
            },
            Rect {
                x: area.x.saturating_add(past),
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
                y: area.y.saturating_add(past),
                height: second_length,
                ..area
            },
        ),
    };
    split_rooms(first, first_area, cell, divider, rooms);
    split_rooms(second, second_area, cell, divider, rooms);
}

/// The titlebar of a window of `size`, over its content's top, opaque in
/// `background`'s color; `None` without one.
pub(super) fn bar_item(size: WindowSize, background: Color) -> Option<Item> {
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
