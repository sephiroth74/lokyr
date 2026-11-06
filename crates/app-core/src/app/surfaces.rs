//! The input of a window's slots' views, each a surface of its own: the
//! pointer over them before the panes, its presses, its drags with the
//! capture, its scrolls and its tooltips; the keyboard's focus across the
//! window's pane and their nodes, Tab and Escape, a field's keys, its
//! clipboard and the input method; and the texts and the selections that
//! their plugins set.

use std::time::Instant;

use lokyr_kernel::plugins::ui::{UiEvent, ViewId};
use lokyr_kernel::plugins::view::{Key as NodeKey, Kind};
use lokyr_kernel::workspace::Workspace;
use lokyr_platform_core::{
    ContentRect, Ime, KeyEvent, Modifiers, MouseButton, Platform, Point, PointerArea, PointerShape,
    ScrollDelta, Size, WindowId,
};
use lokyr_ui_core::chrome;
use lokyr_ui_core::field::Clipboard;
use lokyr_ui_core::focus::{Routed, Target};
use lokyr_ui_core::layout::Frame;
use lokyr_ui_core::panel::Role;
use lokyr_ui_core::pointer::{self, Button, NodeEvent};
use lokyr_ui_core::tree::Tree;
use lokyr_vt_core::input::{self as vt, KeyAction, Mods};

use super::App;
use super::chrome::{SlotView, ui_event};
use crate::drawing::Backend;
use crate::keys;
use crate::window::Window;

/// A line of the wheel over a slot's view, in points: a line of the
/// interface's text.
const LINE: f64 = 16.0;

/// The commands of the Edit menu that a field with the focus takes as its
/// own, each of its key under Command.
const FIELD_COMMANDS: [(&str, char); 3] = [
    ("pane.copy", 'c'),
    ("pane.paste", 'v'),
    ("pane.select-all", 'a'),
];

impl<B: Backend> App<B> {
    /// The pointer moved to `position` over `window`: the slot's view under
    /// it, or the one that its button went down over while it is held,
    /// takes the move, its hover's, its drag's and its tooltip's, and the
    /// view that it left learns of it. Whether a view took it, which the
    /// panes then do not.
    pub(super) fn surface_moved(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        position: Point,
    ) -> bool {
        let Some(scale) = platform.window_size(window).map(|size| size.scale) else {
            return false;
        };
        let (Some(drawing), Some(state)) = (self.drawing.as_mut(), self.windows.get_mut(&window))
        else {
            return false;
        };
        let at = pixels(position, scale);
        let now = Instant::now();
        let target = state.captured.or_else(|| under(&state.chrome, at));
        let mut events = Vec::new();
        let mut changed = false;
        if state.over != target {
            if let Some(left) = state.over.and_then(|id| view_mut(&mut state.chrome, id)) {
                changed |= left.surface.tooltip.shown(&left.surface.tree).is_some();
                let gone = left.surface.left(now);
                events.extend(told(left, gone));
            }
            state.over = target;
        }
        let view = target.and_then(|id| view_mut(&mut state.chrome, id));
        let taken = view.is_some();
        if let Some(view) = view {
            let shown = view.surface.tooltip.shown(&view.surface.tree).is_some();
            let (moved, _) = view.surface.moved(local(view, at), &mut drawing.font, now);
            changed |= shown != view.surface.tooltip.shown(&view.surface.tree).is_some();
            events.extend(told(view, moved));
        }
        changed |= !events.is_empty();
        self.ui_events.extend(events);
        if changed {
            platform.request_frame(window);
        }
        self.wake_for_tooltips(platform);
        taken
    }

    /// The pointer left the parts of `window` where its moves come: the
    /// view that it was over learns of it, unless its button is down over
    /// one, which keeps the pointer, or it went over another view.
    pub(super) fn surface_exited(&mut self, platform: &mut dyn Platform, window: WindowId) {
        let Some(state) = self.windows.get(&window) else {
            return;
        };
        if state.captured.is_some() {
            return;
        }
        // From a view to the one beside it, the move into it.
        let scale = platform.window_size(window).map(|size| size.scale);
        if let (Some(position), Some(scale)) = (platform.pointer_position(window), scale)
            && under(&state.chrome, pixels(position, scale)).is_some()
        {
            self.surface_moved(platform, window, position);
            return;
        }
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        let Some(view) = state
            .over
            .take()
            .and_then(|id| view_mut(&mut state.chrome, id))
        else {
            return;
        };
        let shown = view.surface.tooltip.shown(&view.surface.tree).is_some();
        let gone = view.surface.left(Instant::now());
        let changed = shown || !gone.is_empty();
        let events: Vec<_> = told(view, gone).collect();
        self.ui_events.extend(events);
        if changed {
            platform.request_frame(window);
        }
        self.wake_for_tooltips(platform);
    }

    /// A button went down or up at `position` over `window`, with `mods`,
    /// of `clicks` in a row: a press over a slot's view goes to it, which
    /// keeps the pointer until the button goes up, the compositions under
    /// way ending as they stand, and gives the keyboard's focus to its node
    /// under the pointer that takes it; a press over the panes gives it
    /// back to the pane. Whether a view took it, which the panes then do
    /// not.
    pub(super) fn surface_button(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        (button, pressed, clicks): (MouseButton, bool, u32),
        (position, mods): (Point, Modifiers),
    ) -> bool {
        let Some(scale) = platform.window_size(window).map(|size| size.scale) else {
            return false;
        };
        let at = pixels(position, scale);
        // A press closes the panels that a click outside them closes, which
        // a menu's closing takes.
        if pressed
            && let Some(state) = self.windows.get_mut(&window)
            && state.captured.is_none()
        {
            let outside = state.panels.pressed(at);
            for panel in outside.closed {
                self.close_panel(platform, window, panel, true);
            }
            if outside.taken {
                return true;
            }
        }
        let Some(state) = self.windows.get(&window) else {
            return false;
        };
        // A release goes where its press went; a press under way over the
        // panes, a selection's or a program's, keeps them the pointer.
        let panes_hold =
            state.selecting.is_some_and(|selecting| selecting.held) || state.mouse.held().is_some();
        let target = match (pressed, state.captured) {
            (_, Some(captured)) => Some(captured),
            (true, None) if !panes_hold => under(&state.chrome, at),
            _ => None,
        };
        let Some(id) = target else {
            if pressed && !state.pane_has_focus() {
                self.keys_to_pane(platform, window);
            }
            return false;
        };
        let button = match button {
            MouseButton::Left if mods.control => Button::Secondary,
            MouseButton::Left => Button::Primary,
            MouseButton::Right => Button::Secondary,
            MouseButton::Middle => Button::Middle,
            _ => return true,
        };
        let before = state.focus.target();
        let (Some(drawing), Some(state)) = (self.drawing.as_mut(), self.windows.get_mut(&window))
        else {
            return true;
        };
        let mut events = Vec::new();
        let mut ended = false;
        if pressed {
            // A composition under way, of a field of another view or of the
            // pane, ends as it stands.
            for view in state.chrome.iter_mut().filter(|view| view.id != id) {
                if let Some(node) = view.surface.fields.composing(&view.surface.tree) {
                    let kept = view
                        .surface
                        .fields
                        .keep_composition(&view.surface.tree, node)
                        .unwrap_or_default();
                    events.extend(told(view, kept));
                    ended = true;
                }
            }
            ended |= state.preedit.take().is_some();
            let Some(view) = view_mut(&mut state.chrome, id) else {
                return true;
            };
            let at = local(view, at);
            let pressed = view
                .surface
                .pressed(at, (button, clicks, mods.shift), &mut drawing.font);
            ended |= pressed.composition_kept;
            events.extend(told(view, pressed.events));
            state.captured = Some(id);
            state.focus.pressed(id, &view.surface.tree, &pressed.path);
        } else {
            state.captured = None;
            let Some(view) = view_mut(&mut state.chrome, id) else {
                return true;
            };
            let released = view.surface.released(local(view, at), (button, clicks));
            events.extend(told(view, released));
        }
        self.ui_events.extend(events);
        if ended {
            platform.end_composition(window);
        }
        self.focus_moved(platform, window, before);
        self.views_changed(window);
        platform.request_frame(window);
        self.wake_for_tooltips(platform);
        true
    }

    /// A wheel or a trackpad scrolled by `delta` at `position` over
    /// `window`: the slot's view under it takes it, its list or its scroll
    /// area under the pointer scrolling. Whether a view took it, which the
    /// panes then do not.
    pub(super) fn surface_scrolled(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        delta: ScrollDelta,
        position: Point,
    ) -> bool {
        let Some(scale) = platform.window_size(window).map(|size| size.scale) else {
            return false;
        };
        let Some(state) = self.windows.get_mut(&window) else {
            return false;
        };
        let at = pixels(position, scale);
        let Some(view) = under(&state.chrome, at).and_then(|id| view_mut(&mut state.chrome, id))
        else {
            return false;
        };
        let (x, y) = match delta {
            ScrollDelta::Lines { x, y } => (x * LINE, y * LINE),
            ScrollDelta::Points { x, y } => (x, y),
        };
        let scrolled = view.surface.scrolled(local(view, at), (x as f32, y as f32));
        let events: Vec<_> = told(view, scrolled).collect();
        self.ui_events.extend(events);
        self.views_changed(window);
        platform.request_frame(window);
        self.wake_for_tooltips(platform);
        true
    }

    /// The keys of `keys`, of `window`, which the keymap left: where a
    /// node of a slot's view has the keyboard's focus, or had it as their
    /// presses went, Tab and Shift-Tab move it among the view's nodes and
    /// Escape gives it back to the pane, the node takes the others, a field
    /// cutting, copying and pasting of the clipboard, and those that no
    /// node takes go nowhere; the keys of the pane, which the caller gives
    /// its program.
    pub(super) fn surface_keys(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        keys: Vec<KeyEvent>,
    ) -> Vec<KeyEvent> {
        let every_control = platform.keyboard_navigation();
        let mut pane = Vec::new();
        for key in keys {
            let Some(event) = keys::terminal_key(&key) else {
                pane.push(key);
                continue;
            };
            let Some(state) = self.windows.get_mut(&window) else {
                break;
            };
            let before = state.focus.target();
            state.focus.set_every_control(every_control);
            let routed = {
                let (focus, chrome) = (&mut state.focus, &state.chrome);
                focus.key(|id| tree_of(chrome, id), &event)
            };
            let escape = event.key == vt::Key::Escape && event.action == KeyAction::Press;
            match routed {
                // Escape over no node closes the topmost menu that asks so,
                // as a menu takes it wherever the focus is.
                Routed::Pane if escape => match state.panels.escaped_menu() {
                    Some(menu) => self.close_panel(platform, window, menu, true),
                    None => pane.push(key.clone()),
                },
                Routed::Pane => pane.push(key.clone()),
                Routed::Node { panel, node } => {
                    let Some(view) = view_mut(&mut state.chrome, panel) else {
                        continue;
                    };
                    let taken = view.surface.key(node, &event, &mut Board(&mut *platform));
                    if let Some(events) = taken {
                        let events: Vec<_> = told(view, events).collect();
                        self.ui_events.extend(events);
                        self.views_changed(window);
                        platform.request_frame(window);
                    }
                }
                // The release of a key that moved the focus, as Tab's after a
                // key typed before it went up, does nothing.
                Routed::Focus if event.action == KeyAction::Release => {}
                Routed::Focus => {
                    // A key that moves the focus takes the pointer back, as
                    // of a drag.
                    let mut events = Vec::new();
                    for view in &mut state.chrome {
                        let cancelled = view.surface.cancel();
                        events.extend(told(view, cancelled));
                    }
                    state.captured = None;
                    // A field that Tab gives the focus selects all of its
                    // text.
                    if let Target::Node { panel, node } = state.focus.target()
                        && let Some(view) = view_mut(&mut state.chrome, panel)
                    {
                        view.surface.fields.select_all(&view.surface.tree, node);
                    }
                    self.ui_events.extend(events);
                    self.focus_moved(platform, window, before);
                    // Escape in a panel that asks so closes it, its focus
                    // gone back to where it took it from.
                    let panel = match before {
                        Target::Node { panel, .. } if escape => Some(panel),
                        _ => None,
                    };
                    if let Some(panel) = panel
                        && self
                            .windows
                            .get_mut(&window)
                            .is_some_and(|state| state.panels.escaped(panel))
                    {
                        self.close_panel(platform, window, panel, true);
                    }
                }
            }
        }
        pane
    }

    /// The command `name`, which the Edit menu or its keys run, copy, paste
    /// or select all: a node with the keyboard's focus in the front window
    /// takes it, so that no paste goes to a pane whose keyboard a node has,
    /// a field as its key under Command, any other doing nothing. Whether
    /// one took it, which the command then does not run.
    pub(super) fn field_command(&mut self, platform: &mut dyn Platform, name: &str) -> bool {
        let Some(&(_, letter)) = FIELD_COMMANDS.iter().find(|(command, _)| *command == name) else {
            return false;
        };
        let Some(window) = platform.focused_window() else {
            return false;
        };
        let Some(state) = self.windows.get_mut(&window) else {
            return false;
        };
        let Target::Node { panel, node } = state.focus.target() else {
            return false;
        };
        let Some(view) = view_mut(&mut state.chrome, panel) else {
            return false;
        };
        if !matches!(view.surface.tree.kind(node), Some(Kind::Field(_))) {
            return true;
        }
        let text = letter.to_string();
        let key = vt::KeyEvent {
            key: vt::Key::Char(letter),
            action: KeyAction::Press,
            mods: Mods {
                super_key: true,
                ..Mods::NONE
            },
            text: &text,
            unmodified: &text,
            base_layout: None,
        };
        if let Some(events) = view.surface.key(node, &key, &mut Board(&mut *platform)) {
            let events: Vec<_> = told(view, events).collect();
            self.ui_events.extend(events);
            self.views_changed(window);
            platform.request_frame(window);
        }
        true
    }

    /// The input method's text in `window`, which the field with the
    /// keyboard's focus takes, if a node of a slot's view has it, as the
    /// pane does otherwise. Whether a node had it.
    pub(super) fn surface_ime(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        ime: &Ime,
    ) -> bool {
        let Some(state) = self.windows.get_mut(&window) else {
            return false;
        };
        let Target::Node { panel, node } = state.focus.target() else {
            return false;
        };
        let Some(view) = view_mut(&mut state.chrome, panel) else {
            return true;
        };
        let events = match ime {
            Ime::Preedit { text, selection } => view.surface.compose(node, text, selection.clone()),
            Ime::Commit(text) => view.surface.commit(node, text),
        };
        if let Some(events) = events {
            let events: Vec<_> = told(view, events).collect();
            self.ui_events.extend(events);
        }
        self.views_changed(window);
        platform.request_frame(window);
        true
    }

    /// Gives `window`'s keyboard back to its pane, from a node of a slot's
    /// view, as a press over the panes does.
    pub(super) fn keys_to_pane(&mut self, platform: &mut dyn Platform, window: WindowId) {
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        let before = state.focus.target();
        state.focus.pane();
        self.focus_moved(platform, window, before);
    }

    /// The keyboard's focus of `window` moved from `before`, or did not: a
    /// composition of the element that had it ends as it stands, which the
    /// platform's input method forgets; the pane's program learns whether
    /// its pane has it, if it asked, while the window has the focus; the
    /// focus's and the blur's events go to the views' plugins.
    pub(super) fn focus_moved(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        before: Target<u32>,
    ) {
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        let now = state.focus.target();
        let mut events = Vec::new();
        if now != before {
            if end_composition(state, before, &mut events) {
                platform.end_composition(window);
            }
            tell_pane(&self.workspace, state, before);
            state.views_changed = true;
            platform.request_frame(window);
        }
        events.extend(focus_events(state));
        self.ui_events.extend(events);
    }

    /// `window` lost the keyboard's focus: the pointer's drags over its
    /// slots' views cancel, and their tooltips hide.
    pub(super) fn surface_cancel(&mut self, platform: &mut dyn Platform, window: WindowId) {
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        let mut events = Vec::new();
        for view in &mut state.chrome {
            let cancelled = view.surface.cancel();
            events.extend(told(view, cancelled));
            view.surface.tooltip.dismiss();
        }
        state.captured = None;
        self.ui_events.extend(events);
        self.wake_for_tooltips(platform);
    }

    /// A key hides the tooltips of `window`'s slots' views.
    pub(super) fn dismiss_tooltips(&mut self, platform: &mut dyn Platform, window: WindowId) {
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        let mut shown = false;
        for view in &mut state.chrome {
            shown |= view.surface.tooltip.dismiss();
        }
        if shown {
            platform.request_frame(window);
        }
        self.wake_for_tooltips(platform);
    }

    /// Keeps the tooltips' timer at the soonest time that a tooltip of the
    /// windows' slots' views waits for, or none.
    pub(super) fn wake_for_tooltips(&mut self, platform: &mut dyn Platform) {
        let deadline = self
            .windows
            .values()
            .flat_map(|state| &state.chrome)
            .filter_map(|view| view.surface.tooltip.deadline())
            .min();
        if self.tooltip_timer.map(|(_, at)| at) == deadline {
            return;
        }
        if let Some((timer, _)) = self.tooltip_timer.take() {
            platform.cancel_timer(timer);
        }
        if let Some(at) = deadline {
            self.tooltip_timer = Some((platform.start_timer(at), at));
        }
    }

    /// The tooltips' timer fired: those whose time came show.
    pub(super) fn tooltips_due(&mut self, platform: &mut dyn Platform) {
        self.tooltip_timer = None;
        let now = Instant::now();
        for (&window, state) in &mut self.windows {
            let mut shown = false;
            for view in &mut state.chrome {
                shown |= view.surface.tooltip.tick(now);
            }
            if shown {
                platform.request_frame(window);
            }
        }
        self.wake_for_tooltips(platform);
    }

    /// The text `text` that `plugin` sets in the field of `key` of its view
    /// `view`, as an answer to the field's revision `revision`: set unless
    /// keys typed since changed the field, or the field holds it already,
    /// whose caret and undo stay; a composition of the field ends.
    pub(super) fn set_field_text(
        &mut self,
        platform: &mut dyn Platform,
        (plugin, view, key): (&str, &ViewId, &NodeKey),
        text: &str,
        revision: u64,
    ) {
        let Some((window, shown)) = self.shown_view(plugin, view) else {
            return;
        };
        let surface = &mut shown.surface;
        let composing = surface
            .fields
            .composing(&surface.tree)
            .and_then(|node| surface.tree.key(node))
            == Some(key);
        if surface.fields.answer(key, text, revision) {
            if composing {
                platform.end_composition(window);
            }
            platform.request_frame(window);
            self.views_changed(window);
        }
    }

    /// The item `item` that `plugin` selects in the list of `key` of its
    /// view `view`, which scrolls to it.
    pub(super) fn select_item(
        &mut self,
        platform: &mut dyn Platform,
        (plugin, view, key): (&str, &ViewId, &NodeKey),
        item: usize,
    ) {
        let Some((window, shown)) = self.shown_view(plugin, view) else {
            return;
        };
        let surface = &mut shown.surface;
        if let Some(node) = surface.tree.by_key(key) {
            surface.lists.select(&surface.tree, node, item);
            platform.request_frame(window);
            self.views_changed(window);
        }
    }

    /// The window that shows `plugin`'s view `view`, and its surface.
    fn shown_view(&mut self, plugin: &str, view: &ViewId) -> Option<(WindowId, &mut SlotView)> {
        self.windows.iter_mut().find_map(|(&window, state)| {
            state
                .chrome
                .iter_mut()
                .find(|shown| shown.plugin == plugin && &shown.view == view)
                .map(|shown| (window, shown))
        })
    }
}

/// `state`'s focus moved from `before`: the program of its pane learns
/// whether the pane has it, if it asked, while the window has the
/// keyboard, and the cursor starts blinking, on.
pub(super) fn tell_pane<B: Backend>(
    workspace: &Workspace,
    state: &mut Window<B>,
    before: Target<u32>,
) {
    let to_pane = state.focus.target() == Target::Pane;
    if state.focused
        && (before == Target::Pane) != to_pane
        && let Some(pane) = workspace.focused(state.window)
    {
        pane.focus(to_pane);
    }
    state.blink.restart(Instant::now());
}

/// The focus's and the blur's events of the moves of `state`'s focus
/// since it told them last, each to its view's plugin.
/// Ends the composition of where `state`'s keyboard's focus was, `before`,
/// as the focus moved: a field's, as it stands, whose events its plugin
/// hears, into `events`, or the pane's; whether one ended, which the input
/// method is to forget.
pub(super) fn end_composition<B: Backend>(
    state: &mut Window<B>,
    before: Target<u32>,
    events: &mut Vec<(String, UiEvent)>,
) -> bool {
    match before {
        Target::Node { panel, node } => match view_mut(&mut state.chrome, panel) {
            Some(view) if view.surface.fields.composing(&view.surface.tree) == Some(node) => {
                let kept = view
                    .surface
                    .fields
                    .keep_composition(&view.surface.tree, node)
                    .unwrap_or_default();
                events.extend(told(view, kept));
                true
            }
            _ => false,
        },
        Target::Pane => state.preedit.take().is_some(),
    }
}

pub(super) fn focus_events<B: Backend>(state: &mut Window<B>) -> Vec<(String, UiEvent)> {
    let (focus, chrome) = (&mut state.focus, &state.chrome);
    focus
        .events(|id| tree_of(chrome, id))
        .into_iter()
        .filter_map(|(id, event)| {
            let view = chrome.iter().find(|view| view.id == id)?;
            Some((view.plugin.clone(), ui_event(view.view.clone(), event)))
        })
        .collect()
}

/// The slots' views' areas of the pointer's shapes over `chrome`, in
/// points of a display of `scale`, the topmost first.
pub(super) fn pointer_areas(chrome: &[SlotView], scale: f64) -> Vec<PointerArea> {
    chrome
        .iter()
        .rev()
        .filter(|view| !view.is_tooltip())
        .filter_map(|view| Some((view, view.frame?)))
        .flat_map(|(view, frame)| {
            pointer::areas(&view.surface.tree, &view.surface.layout)
                .into_iter()
                .map(move |area| {
                    let rect = content_rect(
                        Frame {
                            x: area.rect.x as f32 + frame.x,
                            y: area.rect.y as f32 + frame.y,
                            width: area.rect.width as f32,
                            height: area.rect.height as f32,
                        },
                        scale,
                    );
                    PointerArea {
                        origin: rect.origin,
                        size: rect.size,
                        shape: match area.shape {
                            lokyr_kernel::plugins::view::PointerShape::Arrow => PointerShape::Arrow,
                            lokyr_kernel::plugins::view::PointerShape::Text => PointerShape::Text,
                            lokyr_kernel::plugins::view::PointerShape::PointingHand => {
                                PointerShape::PointingHand
                            }
                        },
                    }
                })
        })
        .collect()
}

/// The parts of the content where the pointer's moves come, the slots'
/// views of `chrome` that show, in points of a display of `scale`.
pub(super) fn move_areas(chrome: &[SlotView], scale: f64) -> Vec<ContentRect> {
    chrome
        .iter()
        .filter_map(|view| view.frame)
        .map(|frame| content_rect(frame, scale))
        .collect()
}

/// The parts of the titlebar that are the tab bar's own of `chrome`, in
/// points of a display of `scale`.
pub(super) fn titlebar_areas(chrome: &[SlotView], scale: f64) -> Vec<ContentRect> {
    let Some((view, frame)) = chrome.iter().find_map(|view| {
        let frame = view.frame?;
        matches!(view.view, ViewId::TabBar(_)).then_some((view, frame))
    }) else {
        return Vec::new();
    };
    chrome::titlebar_areas(&view.surface.tree, &view.surface.layout)
        .into_iter()
        .map(|area| {
            content_rect(
                Frame {
                    x: area.x + frame.x,
                    y: area.y + frame.y,
                    ..area
                },
                scale,
            )
        })
        .collect()
}

/// The parts of the titlebar of `state`'s window, `bar` pixels tall in a
/// window `width` pixels wide, that its drawn panels take, in points of a
/// display of `scale`, as its content's, whose presses come to the app:
/// each panel's frame there, but a tooltip's, which the pointer goes
/// through; the whole bar while a panel shows that a press outside it
/// closes, so that a press on the bar closes it, as a press outside a menu
/// of the system's does.
pub(super) fn panel_titlebar_areas<B: Backend>(
    state: &Window<B>,
    (width, bar): (f32, f32),
    scale: f64,
) -> Vec<ContentRect> {
    if bar <= 0.0 {
        return Vec::new();
    }
    let mut areas = Vec::new();
    for id in state.panels.order() {
        let (Some(options), Some(frame)) = (state.panels.options(id), state.panels.frame(id))
        else {
            continue;
        };
        if options.role == Role::Tooltip {
            continue;
        }
        if options.dismiss.outside {
            let whole = Frame {
                x: 0.0,
                y: 0.0,
                width,
                height: bar,
            };
            return vec![content_rect(whole, scale)];
        }
        if frame.y < bar {
            let height = frame.height.min(bar - frame.y);
            areas.push(content_rect(Frame { height, ..frame }, scale));
        }
    }
    areas
}

/// `frame`, of pixels, in points of a display of `scale`.
fn content_rect(frame: Frame, scale: f64) -> ContentRect {
    let points = |pixels: f32| f64::from(pixels) / scale;
    ContentRect {
        origin: Point {
            x: points(frame.x),
            y: points(frame.y),
        },
        size: Size {
            width: points(frame.width),
            height: points(frame.height),
        },
    }
}

/// `position`, in points, in pixels of a display of `scale`.
fn pixels(position: Point, scale: f64) -> (f32, f32) {
    ((position.x * scale) as f32, (position.y * scale) as f32)
}

/// The id of the slot's view or the panel of `chrome` under `at`, in
/// pixels of the content, the topmost, as they draw, but the tooltips,
/// which the pointer goes through.
fn under(chrome: &[SlotView], (x, y): (f32, f32)) -> Option<u32> {
    chrome
        .iter()
        .rev()
        .filter(|view| !view.is_tooltip())
        .find(|view| {
            view.frame.is_some_and(|frame| {
                x >= frame.x
                    && y >= frame.y
                    && x < frame.x + frame.width
                    && y < frame.y + frame.height
            })
        })
        .map(|view| view.id)
}

/// The slot's view of `chrome` of `id`.
fn view_mut(chrome: &mut [SlotView], id: u32) -> Option<&mut SlotView> {
    chrome.iter_mut().find(|view| view.id == id)
}

/// The tree of the slot's view of `chrome` of `id`.
fn tree_of(chrome: &[SlotView], id: u32) -> Option<&Tree> {
    chrome
        .iter()
        .find(|view| view.id == id)
        .map(|view| &view.surface.tree)
}

/// `at`, in pixels of the content, from `view`'s corner.
fn local(view: &SlotView, at: (f32, f32)) -> (f32, f32) {
    view.frame
        .map_or(at, |frame| (at.0 - frame.x, at.1 - frame.y))
}

/// The events of `events`, of `view`'s nodes, each to its plugin.
fn told(view: &SlotView, events: Vec<NodeEvent>) -> impl Iterator<Item = (String, UiEvent)> + '_ {
    events
        .into_iter()
        .map(move |event| (view.plugin.clone(), ui_event(view.view.clone(), event)))
}

/// The platform's clipboard, as a field cuts, copies and pastes.
struct Board<'a>(&'a mut dyn Platform);

impl Clipboard for Board<'_> {
    fn read(&mut self) -> Option<String> {
        self.0.clipboard_text()
    }

    fn write(&mut self, text: &str) {
        self.0.set_clipboard_text(text);
    }
}
