//! The input of the windows: their keys, the input method's composition,
//! the mouse's buttons, moves and wheel, the selection that it makes, and
//! what is dropped on them, each routed to its window's pane, or to its
//! slots' views, which take the pointer over them and the keys while one of
//! their nodes has the keyboard's focus.

use std::time::Instant;

use lokyr_kernel::commands::Value;
use lokyr_kernel::keymap::{Invocation, Pressed, Resolved, Resolver};
use lokyr_kernel::plugins;
use lokyr_kernel::workspace::{Pane, PaneId};
use lokyr_kernel::{keymap, paste};
use lokyr_platform_core::{
    Dropped, Ime, KeyEvent, KeyState, Modifiers, MouseButton, Platform, Point, ScrollDelta,
    ScrollPhase, WindowId,
};
use lokyr_ui_core::pane::Preedit;
use lokyr_vt_core::input::{Key, KeyOptions, MouseAction, MouseEvent, Wheel};
use lokyr_vt_core::terminal::{
    MouseEncoding, MouseTracking, Scroll, SelectionShape, SelectionUnit,
};

use super::{App, SELECTION_SCROLL, SELECTION_SCROLL_ROWS, chrome, context};
use crate::drawing::Backend;
use crate::mouse::{Pointer, Spot};
use crate::window::{ColumnDrag, Selecting};
use crate::{keys, mouse};

impl<B: Backend> App<B> {
    /// A key of `window`: the press that the latency's bench waits for;
    /// a man page's keys, which scroll it; a key of a window whose program
    /// ended, which closes it; and the others, which the keymap runs as
    /// commands, holds back as a sequence under way, or gives the program.
    pub(super) fn key(&mut self, platform: &mut dyn Platform, window: WindowId, key: &KeyEvent) {
        // The press that the latency's bench waits to see shown.
        if let Some(bench) = &mut self.latency_bench
            && bench.window == Some(window)
            && bench.pending.is_none()
            && key.state == KeyState::Pressed
            && !key.mods.super_key
            && !key.text.is_empty()
        {
            bench.pending = key.time;
            bench.handled = Some(Instant::now());
        }
        // Command, down or up, with the pointer still: the plugins learn
        // where the pointer is, and its moves wake the app while it is held.
        if key.mods.super_key != self.command_held {
            self.command_held = key.mods.super_key;
            let position = platform.pointer_position(window);
            self.tell_pointer(platform, window, position, key.mods);
            platform.request_frame(window);
        }
        // The key's own prompt, which no output told, as after a
        // silent `stty -echo`, is secure from the next key.
        self.settle_secure_input(platform);
        // A panel that hid since the last frame, as another tab came in
        // front, takes no key.
        self.keys_from_hidden(platform, window);
        let pressed = key.state != KeyState::Released;
        // A key hides the slots' tooltips.
        if pressed {
            self.dismiss_tooltips(platform, window);
        }
        let options = keys::options(self.config.keyboard.option_as_meta);
        // A mode of the keyboard's takes the keys of a pane whose program
        // ended too, which a key would close.
        let held_back = self.keys_held_back(window);
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        let Some(pane) = self.workspace.focused(state.window) else {
            return;
        };
        let pane_keys = state.pane_has_focus();
        // A man page's keys scroll it, and close nothing, but in a mode of
        // the keyboard's, whose keys they are.
        if pane_keys && pane.is_kept() && state.page.is_some() && !held_back {
            if pressed && let Some(scroll) = page_scroll(key) {
                pane.scroll(scroll);
                platform.request_frame(window);
            }
            return;
        }
        if pane_keys && pane.is_kept() && !held_back {
            if pressed && closes(key) {
                let id = pane.id();
                self.close_pane(platform, window, id);
            }
            return;
        }
        if pressed {
            state.blink.restart(Instant::now());
            platform.request_frame(window);
        }
        match self.resolve_key(window, key) {
            Taken::Run(invocation) => {
                self.run(platform, &invocation.command, invocation.args);
            }
            Taken::Held => {}
            Taken::Program(keys) => {
                // A node of a slot's view that has the focus takes them,
                // and a mode of the keyboard's holds them back.
                let keys = self.surface_keys(platform, window, keys);
                if keys.is_empty() || self.keys_held_back(window) {
                    return;
                }
                if pressed {
                    self.close_typing_panels(platform, window);
                }
                let Some(pane) = self
                    .windows
                    .get(&window)
                    .and_then(|state| self.workspace.focused(state.window))
                else {
                    return;
                };
                give(pane, &keys, options);
            }
        }
    }

    /// Closes `window`'s panels that a key typed into a pane closes, as
    /// their plugins hear.
    fn close_typing_panels(&mut self, platform: &mut dyn Platform, window: WindowId) {
        let Some(state) = self.windows.get(&window) else {
            return;
        };
        let typing: Vec<u32> = state
            .chrome
            .iter()
            .filter(|shown| {
                shown
                    .placed
                    .as_ref()
                    .is_some_and(|placed| placed.options.close_typing)
            })
            .map(|shown| shown.id)
            .collect();
        for view in typing {
            self.close_panel(platform, window, view, true);
        }
    }

    /// The input method's text: composed, shown at the cursor, or
    /// typed, sent to the program, after the keys of a sequence under
    /// way, which it ends. A composition that starts brings the view
    /// back to the screen, and the cursor's blinking on, as typing
    /// does.
    pub(super) fn ime(&mut self, platform: &mut dyn Platform, window: WindowId, ime: Ime) {
        // A field of a slot's view that has the focus takes it, but of a
        // panel that hid, and a mode of the keyboard's holds it back.
        self.keys_from_hidden(platform, window);
        if self.surface_ime(platform, window, &ime) || self.keys_held_back(window) {
            return;
        }
        let options = keys::options(self.config.keyboard.option_as_meta);
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        let Some(pane) = self.workspace.focused(state.window) else {
            return;
        };
        // On a window whose program ended, what the input method
        // composes does not show, and its commit closes it, as a key
        // does.
        if pane.is_kept() {
            if matches!(ime, Ime::Commit(_)) {
                let id = pane.id();
                self.close_pane(platform, window, id);
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
                self.close_typing_panels(platform, window);
            }
        }
        platform.request_frame(window);
    }

    /// A drop on `window`: files paste their paths, as a shell's
    /// arguments; text pastes as a paste does.
    pub(super) fn dropped(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        dropped: Dropped,
    ) {
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
        // A node of a slot's view that has the focus leaves out the pane's
        // contexts.
        let mut context = if state.pane_has_focus() {
            context(self.workspace.focused(state.window))
        } else {
            keymap::Context::new(Vec::<&str>::new())
        };
        // The plugins' panels: `<namespace>.open` of each that shows, not
        // of one whose pane another tab or a zoomed pane hides, and
        // `<namespace>.focused` of the one whose node has the keyboard.
        let focused_view = match state.focus.target() {
            lokyr_ui_core::focus::Target::Node { panel, .. } => Some(panel),
            lokyr_ui_core::focus::Target::Pane => None,
        };
        for view in &state.chrome {
            let namespace = view.plugin.rsplit('/').next().unwrap_or_default();
            let shows = view
                .placed
                .as_ref()
                .is_none_or(|placed| super::panels::anchor_shows(state, &placed.options.anchor));
            if matches!(view.view, lokyr_kernel::plugins::ui::ViewId::Panel(_)) && shows {
                context.hold(format!("{namespace}.open"));
            }
            if focused_view == Some(view.id) {
                context.hold(format!("{namespace}.focused"));
            }
        }
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

    /// Where `position` is in the cells of the pane that has `window`'s
    /// keyboard.
    pub(super) fn pointer(
        &self,
        platform: &dyn Platform,
        window: WindowId,
        position: Point,
    ) -> Option<Pointer> {
        self.pointer_in(platform, window, self.focused(window)?, position)
    }

    /// Where `position` is in the cells of `pane`, one of `window`'s.
    fn pointer_in(
        &self,
        platform: &dyn Platform,
        window: WindowId,
        id: PaneId,
        position: Point,
    ) -> Option<Pointer> {
        let pane = self.workspace.pane(id)?;
        let size = platform.window_size(window)?;
        Some(mouse::pointer(
            position,
            size.scale,
            self.pane_origin(window, id, size),
            pane.grid(),
        ))
    }

    /// The pane of `window` under `position`, as its last frame laid its
    /// panes out; none over a divider, the padding or the titlebar.
    pub(super) fn pane_at(
        &self,
        platform: &dyn Platform,
        window: WindowId,
        position: Point,
    ) -> Option<PaneId> {
        let scale = platform.window_size(window)?.scale;
        let (x, y) = (position.x * scale, position.y * scale);
        let state = self.windows.get(&window)?;
        state
            .views
            .iter()
            .find(|view| {
                let (left, top) = (f64::from(view.rect.x), f64::from(view.rect.y));
                (left..left + f64::from(view.rect.width)).contains(&x)
                    && (top..top + f64::from(view.rect.height)).contains(&y)
            })
            .map(|view| view.pane)
    }

    /// The pane that the pointer's moves and the wheel at `position` go
    /// to: the one under it, or, over none, the one with the keyboard.
    fn pane_under(
        &self,
        platform: &dyn Platform,
        window: WindowId,
        position: Point,
    ) -> Option<PaneId> {
        self.pane_at(platform, window, position)
            .or_else(|| self.focused(window))
    }

    /// Gives the keyboard to the pane of `window` at `position`, if another
    /// has it.
    fn follow_pointer(&mut self, platform: &mut dyn Platform, window: WindowId, position: Point) {
        let Some(under) = self.pane_at(platform, window, position) else {
            return;
        };
        let shown = self.windows.get(&window).map(|state| state.window);
        if shown.and_then(|shown| self.workspace.active_pane(shown)) != Some(under) {
            self.focus_pane(platform, window, under);
            platform.request_frame(window);
        }
    }

    /// A button of the mouse went down or up over `window`: the program
    /// learns of it, as its mouse modes have it, at the screen's row, and
    /// not of a press above the screen, in a view scrolled up. Shift keeps
    /// a press for the terminal, which selects with it; a release goes to
    /// the program whenever it learned of the press, so that no button stays
    /// down for it. While a program holds a button, its pane takes the
    /// presses and the releases, over another pane too, at its nearest
    /// cell.
    pub(super) fn mouse_button(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        (button, pressed, clicks): (MouseButton, bool, u32),
        position: Point,
        mods: Modifiers,
    ) {
        let platform_button = button;
        let Some(button) = mouse::button(button) else {
            return;
        };
        // A press at a column's edge drags it, which takes its release, as
        // a divider's drag takes its own.
        if self.column_button(platform, window, (platform_button, pressed), position) {
            return;
        }
        let divider = (platform_button, pressed, clicks);
        if !pressed && self.divider_button(platform, window, divider, position) {
            return;
        }
        // A knob's drag takes its release, wherever it is.
        let bar = (platform_button, pressed);
        if !pressed && self.bar_button(platform, window, bar, (position, mods)) {
            return;
        }
        // A slot's view under the press takes it, and keeps the pointer
        // until the button goes up.
        if self.surface_button(
            platform,
            window,
            (platform_button, pressed, clicks),
            (position, mods),
        ) {
            return;
        }
        // A press over a divider's grab area drags it, or shares its split
        // half and half of a double click, the panes beside it taking none.
        let holds = self
            .windows
            .get(&window)
            .is_some_and(|state| state.mouse.holder().is_some());
        if pressed && !holds && self.divider_button(platform, window, divider, position) {
            return;
        }
        // A press over a scroll bar that shows drags its knob, or pages or
        // jumps over its track, the pane under it taking none.
        if pressed && !holds && self.bar_button(platform, window, bar, (position, mods)) {
            return;
        }
        // A press gives the pane under it the keyboard, then acts there;
        // while a program holds a button, its pane takes the others' presses
        // and releases, wherever they are.
        let holder = self
            .windows
            .get(&window)
            .and_then(|state| state.mouse.holder());
        if pressed
            && holder.is_none()
            && let Some(under) = self.pane_at(platform, window, position)
            && self.focused(window) != Some(under)
        {
            self.focus_pane(platform, window, under);
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
        let Some(target) = holder.or_else(|| self.focused(window)) else {
            return;
        };
        // Past the pane's edges, at its nearest cell.
        let Some(pointer) = self.pointer_in(platform, window, target, position) else {
            return;
        };
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        let Some(pane) = self.workspace.pane(target).filter(|pane| !pane.is_kept()) else {
            return;
        };
        let on_screen =
            mouse::on_screen(pointer, pane.snapshot().offset(), pane.grid().cell_height);
        if pressed {
            let Some(pointer) = on_screen else {
                return;
            };
            state.mouse.moved(target, pointer, true);
            if pane.mouse(&mouse_event(MouseAction::Press(button), mods, pointer)) {
                state.mouse.press(target, button);
            }
        } else if state.mouse.release(button) {
            let pointer = on_screen.unwrap_or(mouse::at_top(pointer));
            state.mouse.moved(target, pointer, true);
            pane.mouse(&mouse_event(MouseAction::Release(button), mods, pointer));
        }
    }

    /// The left button's press at the edge of `window`'s tab bar's column,
    /// at `position`, which starts the edge's drag, or a release that ends
    /// one, as the bar's plugin learns of the column's width then: whether
    /// it was one.
    fn column_button(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        (button, pressed): (lokyr_platform_core::MouseButton, bool),
        position: Point,
    ) -> bool {
        let Some(scale) = platform.window_size(window).map(|size| size.scale) else {
            return false;
        };
        let Some(state) = self.windows.get_mut(&window) else {
            return false;
        };
        if !pressed {
            let Some(drag) = state.column_drag.take() else {
                return false;
            };
            let width = self
                .plugins
                .tab_bar_place(&drag.plugin)
                .width()
                .unwrap_or(drag.width);
            self.plugins.tab_bar_resized(&drag.plugin, width);
            return true;
        }
        if button != lokyr_platform_core::MouseButton::Left {
            return false;
        }
        let Some((plugin, left, zone)) = chrome::column_edge(&state.chrome, &self.plugins, scale)
        else {
            return false;
        };
        let (x, y) = ((position.x * scale) as f32, (position.y * scale) as f32);
        let within = (zone.x..zone.x + zone.width).contains(&x)
            && (zone.y..zone.y + zone.height).contains(&y);
        let Some(width) = self.plugins.tab_bar_place(&plugin).width() else {
            return false;
        };
        if !within {
            return false;
        }
        state.column_drag = Some(ColumnDrag {
            plugin,
            left,
            from: position.x,
            width,
        });
        true
    }

    /// Runs the command that the keymap binds the left button's click with
    /// `mods` to, as it goes down at `position` over `window`, which has the
    /// focus, before the selection and the program: the column of the cell
    /// under the pointer and its row's number, as the drawn frame showed
    /// it, become its `column` and `row` arguments, if it takes them.
    /// Whether a command was bound.
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
        let pane = self.workspace.focused(state.window);
        let context = context(pane);
        let row = pane.map_or(0, shown_top) + i64::from(pointer.row);
        let Resolved::Run(mut invocation) =
            Resolver::new().press(&self.keymap, Pressed::click(binding_mods(mods)), &context)
        else {
            return false;
        };
        if let Some(command) = self.commands.get(&invocation.command) {
            for (name, value) in [("column", i64::from(pointer.col)), ("row", row)] {
                if command.params.iter().any(|param| param.name == name) {
                    invocation
                        .args
                        .entry(name.to_owned())
                        .or_insert(Value::Integer(value));
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
            || self.workspace.focused(state.window).is_none_or(|pane| {
                pane.is_kept()
                    || pane
                        .input_modes()
                        .is_none_or(|modes| modes.mouse_tracking == MouseTracking::None)
            })
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
        let Some(pane) = self.workspace.focused(state.window) else {
            return;
        };
        let grid = pane.grid();
        let spot = mouse::spot(pointer, shown_top(pane), grid.cell_width, grid.columns);
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
                pane.anchor_selection((spot.col, spot.row));
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
        let Some(id) = self.focused(window) else {
            return;
        };
        let (_, top) = self.pane_origin(window, id, size);
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        let Some(pane) = self.workspace.focused(state.window) else {
            return;
        };
        let Some(selecting) = &mut state.selecting else {
            return;
        };
        let grid = pane.grid();
        selecting.last = mouse::spot(pointer, shown_top(pane), grid.cell_width, grid.columns);
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
                .focused(state.window)
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
    pub(super) fn scroll_selections(&mut self, platform: &mut dyn Platform) {
        self.scroll_timer = None;
        let mut scrolled = false;
        for (&window, state) in &mut self.windows {
            let Some(selecting) = &mut state.selecting else {
                continue;
            };
            if !selecting.held || selecting.beyond == 0 {
                continue;
            }
            let Some(pane) = self.workspace.focused(state.window) else {
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
                pane.grid().rows.saturating_sub(1)
            };
            selecting.last.row = pane.origin().map_or(0, |origin| origin.row(edge));
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
    pub(super) fn mouse_moved(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        position: Point,
        mods: Modifiers,
    ) {
        // A divider that a drag holds moves its split.
        if self.divider_moved(platform, window, position) {
            return;
        }
        // A knob that a drag holds moves the view; a scroll bar under the
        // pointer shows.
        if self.bar_dragged(platform, window, position) {
            return;
        }
        self.bar_hovered(platform, window, Some(position));
        // A column's edge that a drag holds follows the pointer.
        if let Some(drag) = self
            .windows
            .get(&window)
            .and_then(|state| state.column_drag.clone())
        {
            let moved = position.x - drag.from;
            let moved = if drag.left { moved } else { -moved };
            self.plugins
                .set_tab_bar_width(&drag.plugin, drag.width + moved as f32);
            platform.request_frame(window);
            return;
        }
        let dragging = self
            .windows
            .get(&window)
            .and_then(|state| state.selecting)
            .is_some_and(|selecting| selecting.held);
        let held = self
            .windows
            .get(&window)
            .is_some_and(|state| state.mouse.held().is_some());
        // A slot's view under the pointer takes its move, unless a press
        // over the panes holds it, as does a panel over them, and a view
        // whose press holds the pointer, as a tab's drag over the panes.
        let taken = !dragging && !held && self.surface_moved(platform, window, position);
        // The keyboard follows the pointer to the pane that it comes over,
        // as a plugin asks, but in a drag, and where a view takes the move.
        if !dragging && !held && !taken && self.plugins.focus_follows_mouse() {
            self.follow_pointer(platform, window, position);
        }
        // The plugins that follow the pointer learn that it left the panes.
        if taken {
            self.tell_pointer(platform, window, None, mods);
            return;
        }
        self.tell_pointer(platform, window, Some(position), mods);
        if dragging {
            self.select_drag(platform, window, position);
            return;
        }
        if mods.shift {
            return;
        }
        // A drag that a program holds goes to its pane alone, wherever the
        // pointer goes, at the pane's nearest cell past its edges.
        let holder = self
            .windows
            .get(&window)
            .and_then(|state| state.mouse.holder());
        let Some(target) = holder.or_else(|| self.pane_under(platform, window, position)) else {
            return;
        };
        let Some(pointer) = self.pointer_in(platform, window, target, position) else {
            return;
        };
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        let Some(pane) = self.workspace.pane(target).filter(|pane| !pane.is_kept()) else {
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
        if state.mouse.moved(target, pointer, pixels) {
            let action = MouseAction::Motion(state.mouse.held());
            pane.mouse(&mouse_event(action, mods, pointer));
        }
    }

    /// Tells the plugins that subscribed to the pointer's events where it
    /// is over `window`, at `position`, with `mods`, if it went over another
    /// cell, or out of the panes, or Command went down or up: its pane and
    /// its cell, by its row's number as the drawn frame showed it, which
    /// takes no lock of the pane.
    pub(super) fn tell_pointer(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        position: Option<Point>,
        mods: Modifiers,
    ) {
        self.tell_pointer_again(platform, window, position, mods, false);
    }

    /// Tells the plugins of the pointer again, still at its cell over
    /// `window`, as its frame changed the text under it, its row or its
    /// pane's rows that scrolled under it: a link there may have gone.
    pub(super) fn tell_pointer_over_changed_text(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
    ) {
        let position = platform.pointer_position(window);
        self.tell_pointer_again(platform, window, position, self.pointer_mods, true);
    }

    /// Tells the plugins where the pointer is, as [`App::tell_pointer`],
    /// even at the cell that they were told of with `again`.
    fn tell_pointer_again(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        position: Option<Point>,
        mods: Modifiers,
        again: bool,
    ) {
        // The modifiers of a pointer's event are those held now.
        self.command_held = mods.super_key;
        self.pointer_mods = mods;
        if !self.plugins.subscribes(plugins::POINTER) {
            return;
        }
        let command = mods.super_key;
        let over = position.and_then(|position| {
            let pane = self.pane_under(platform, window, position)?;
            let pointer = self.pointer_in(platform, window, pane, position)?;
            // The drawn frame's numbers, which take no lock of the pane.
            let top = self.workspace.pane(pane)?.snapshot().origin()?.top;
            Some((
                pane,
                Some((pointer.col, top + i64::from(pointer.row))),
                command,
            ))
        });
        // Out of the panes, the pane that it left learns so.
        let told = over.or_else(|| self.pointer_told.map(|(pane, _, _)| (pane, None, command)));
        if told.is_none() || (told == self.pointer_told && !again) {
            return;
        }
        self.pointer_told = told;
        let Some((pane, at, _)) = told else {
            return;
        };
        self.plugins.post(plugins::Event::Pointer {
            pane: plugins::host::PaneId(pane.get()),
            at,
            mods: plugins::Mods {
                shift: mods.shift,
                control: mods.control,
                option: mods.left_alt || mods.right_alt,
                command,
            },
        });
    }

    /// The wheel or the trackpad scrolled over `window`, by `scroll`'s delta
    /// in its phase: each whole step goes to a program that takes the
    /// wheel, as the cursor keys on the alternate screen; otherwise, and
    /// with Shift, it moves the view through the scrollback.
    pub(super) fn scroll(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        (delta, phase): (ScrollDelta, Option<ScrollPhase>),
        position: Point,
        mods: Modifiers,
    ) {
        // A slot's view under the pointer takes it.
        if self.surface_scrolled(platform, window, delta, position) {
            return;
        }
        let Some(scale) = platform.window_size(window).map(|size| size.scale) else {
            return;
        };
        let Some(target) = self.pane_under(platform, window, position) else {
            return;
        };
        let Some(pointer) = self.pointer_in(platform, window, target, position) else {
            return;
        };
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        let Some(pane) = self.workspace.pane(target) else {
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

/// The number of the first row of `pane`'s view as the drawn frame showed
/// it, which the pointer's cells take, though output scrolled the view
/// since; as the terminal has it now, when a resize laid the rows out
/// again since, or the other screen became the active one.
pub(super) fn shown_top(pane: &Pane) -> i64 {
    let now = pane.origin();
    match (pane.snapshot().origin(), now) {
        (Some(drawn), Some(now)) if drawn.epoch == now.epoch => drawn.top,
        (_, now) => now.map_or(0, |now| now.top),
    }
}

/// Shows `selecting`'s selection in `pane`: the cells between its spots,
/// or the words or the lines that they are in; none, as after a click. Its
/// anchor is where the terminal keeps it, which output moves with its text.
pub(super) fn show_selection(pane: &Pane, selecting: &mut Selecting) {
    // The anchor where its text is now, which a reflow can have moved; its
    // side of its cell kept.
    if let Some((col, row)) = pane.selection_anchor() {
        let side = selecting
            .anchor
            .boundary
            .saturating_sub(selecting.anchor.col);
        selecting.anchor = Spot {
            row,
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
    match cells {
        Some((from, to)) => pane.select(from, to, selecting.shape, selecting.unit),
        None => pane.clear_selection(),
    }
}
