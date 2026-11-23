//! A tab's floating panes from the menus and the mouse: a new one, and all
//! of the tab's shown or hidden, from the Shell menu; a pane of the splits
//! that floats, and a floating one that goes into the splits, from its
//! context menu; a floating pane moved by its header and resized by its
//! edges and corners, of the frame's resize pointers; and a popup, a
//! floating pane of a command that a binding names, which closes as the
//! command ends.

use lokyr_kernel::commands::{Args, Code, Error, Value};
use lokyr_kernel::workspace::{FloatingFrame, Pane, PaneId};
use lokyr_platform_core::{
    FrameEdge, MouseButton, Platform, Point, PointerArea, PointerShape, Size, WindowId,
};
use lokyr_pty::Program;
use lokyr_ui_core::floating::Room;

use super::{App, spawn};
use crate::drawing::Backend;
use crate::window::{FloatingDrag, FloatingGrab};

/// How far inside a floating pane's box its edges take a resize, in
/// points.
const EDGE: f64 = 5.0;

/// The share of the tab's room that a new floating pane takes each way,
/// and how far each one more moves from the one before, down and right.
const NEW_SHARE: f64 = 0.6;
const CASCADE: f64 = 0.04;

/// The shares of the tab's room that a popup takes across and down, of a
/// command that asks for none.
pub(super) const POPUP_WIDTH: f64 = 0.8;
pub(super) const POPUP_HEIGHT: f64 = 0.7;

/// The edge or the corner of `room`'s box at `position`, in points, on a
/// display of `scale`, within [`EDGE`] of it; none inside.
fn edge_at(room: &Room, position: Point, scale: f64) -> Option<FrameEdge> {
    let points = |pixels: i32| f64::from(pixels) / scale;
    let (left, top) = (points(room.outer.x), points(room.outer.y));
    let right = left + f64::from(room.outer.width) / scale;
    let bottom = top + f64::from(room.outer.height) / scale;
    if !(left..right).contains(&position.x) || !(top..bottom).contains(&position.y) {
        return None;
    }
    let (near_left, near_right) = (position.x < left + EDGE, position.x >= right - EDGE);
    let (near_top, near_bottom) = (position.y < top + EDGE, position.y >= bottom - EDGE);
    Some(match (near_left, near_right, near_top, near_bottom) {
        (true, _, true, _) => FrameEdge::TopLeft,
        (_, true, true, _) => FrameEdge::TopRight,
        (true, _, _, true) => FrameEdge::BottomLeft,
        (_, true, _, true) => FrameEdge::BottomRight,
        (true, ..) => FrameEdge::Left,
        (_, true, ..) => FrameEdge::Right,
        (_, _, true, _) => FrameEdge::Top,
        (.., true) => FrameEdge::Bottom,
        _ => return None,
    })
}

/// Whether `position`, in points, is on `room`'s header, on a display of
/// `scale`.
fn on_header(room: &Room, position: Point, scale: f64) -> bool {
    let points = |pixels: i32| f64::from(pixels) / scale;
    let (left, top) = (points(room.header.x), points(room.header.y));
    (left..left + f64::from(room.header.width) / scale).contains(&position.x)
        && (top..top + f64::from(room.header.height) / scale).contains(&position.y)
}

/// The pointer's areas of the floating panes `rooms`, the one in front
/// first, on a display of `scale`: for each, its edges and corners, of the
/// frame's resize pointers, its header, of the arrow, and its box, of
/// `inside`'s pointer of its pane, over what is under it.
pub(super) fn pointer_areas(
    rooms: &[(PaneId, Room)],
    scale: f64,
    inside: impl Fn(PaneId) -> PointerShape,
) -> Vec<PointerArea> {
    let points = |pixels: i32| f64::from(pixels) / scale;
    let area = |x: f64, y: f64, width: f64, height: f64, shape| PointerArea {
        origin: Point { x, y },
        size: Size { width, height },
        shape,
    };
    let mut areas = Vec::new();
    for &(pane, room) in rooms.iter().rev() {
        let (left, top) = (points(room.outer.x), points(room.outer.y));
        let (width, height) = (
            f64::from(room.outer.width) / scale,
            f64::from(room.outer.height) / scale,
        );
        let (right, bottom) = (left + width - EDGE, top + height - EDGE);
        let resize = PointerShape::ResizeFrame;
        areas.extend([
            area(left, top, EDGE, EDGE, resize(FrameEdge::TopLeft)),
            area(right, top, EDGE, EDGE, resize(FrameEdge::TopRight)),
            area(left, bottom, EDGE, EDGE, resize(FrameEdge::BottomLeft)),
            area(right, bottom, EDGE, EDGE, resize(FrameEdge::BottomRight)),
            area(left, top, width, EDGE, resize(FrameEdge::Top)),
            area(left, bottom, width, EDGE, resize(FrameEdge::Bottom)),
            area(left, top, EDGE, height, resize(FrameEdge::Left)),
            area(right, top, EDGE, height, resize(FrameEdge::Right)),
            area(
                points(room.header.x),
                points(room.header.y),
                f64::from(room.header.width) / scale,
                f64::from(room.header.height) / scale,
                PointerShape::Arrow,
            ),
            area(left, top, width, height, inside(pane)),
        ]);
    }
    areas
}

/// The frame that `grab` moves `frame` to, as the pointer moved by `by`, a
/// share of the tab's room each way.
fn dragged(frame: FloatingFrame, grab: FloatingGrab, (across, down): (f64, f64)) -> FloatingFrame {
    let (mut x, mut y, mut width, mut height) =
        (frame.x(), frame.y(), frame.width(), frame.height());
    match grab {
        FloatingGrab::Move => {
            x += across;
            y += down;
        }
        FloatingGrab::Resize(edge) => {
            let (left, right, top, bottom) = match edge {
                FrameEdge::Top => (false, false, true, false),
                FrameEdge::Left => (true, false, false, false),
                FrameEdge::Bottom => (false, false, false, true),
                FrameEdge::Right => (false, true, false, false),
                FrameEdge::TopLeft => (true, false, true, false),
                FrameEdge::TopRight => (false, true, true, false),
                FrameEdge::BottomLeft => (true, false, false, true),
                FrameEdge::BottomRight => (false, true, false, true),
            };
            // An edge that moves moves alone, the opposite one staying.
            if left {
                let moved = across.min(width - lokyr_kernel::workspace::LEAST_FLOATING_SHARE);
                x += moved;
                width -= moved;
            }
            if right {
                width += across;
            }
            if top {
                let moved = down.min(height - lokyr_kernel::workspace::LEAST_FLOATING_SHARE);
                y += moved;
                height -= moved;
            }
            if bottom {
                height += down;
            }
            width = width.min(1.0 - x.max(0.0));
            height = height.min(1.0 - y.max(0.0));
        }
    }
    FloatingFrame::new(x, y, width, height)
}

impl<B: Backend> App<B> {
    /// Floats a new pane over the active tab of the window that acts, or
    /// has the focus, its program and folder as a new pane's beside the
    /// focused one, in the room's middle, each one more down and right of
    /// the one before.
    pub(super) fn new_floating_pane(&mut self, platform: &mut dyn Platform) {
        self.float_new(platform, None, None, false, |count| {
            let offset = CASCADE * (count % 5) as f64;
            let start = (1.0 - NEW_SHARE) / 2.0 + offset - CASCADE * 2.0;
            FloatingFrame::new(start, start, NEW_SHARE, NEW_SHARE)
        });
    }

    /// Opens a popup over the active tab of the window that acts, or has
    /// the focus, as `args` say: its `command`, which the login shell runs,
    /// in its `folder`, the focused pane's of none, in the room's middle of
    /// its `width` and `height`, shares of the room.
    pub(super) fn open_popup(
        &mut self,
        platform: &mut dyn Platform,
        args: &Args,
    ) -> Result<(), Error> {
        let command = match args.get("command") {
            Some(Value::String(command)) if !command.trim().is_empty() => command.clone(),
            _ => {
                return Err(Error::new(
                    Code::MissingArgument,
                    "a popup takes the `command` that it runs",
                ));
            }
        };
        let folder = match args.get("folder") {
            Some(Value::String(folder)) if !folder.is_empty() => Some(folder.clone()),
            _ => None,
        };
        let share = |name: &str, default: f64| match args.get(name) {
            Some(Value::Number(share)) => *share,
            #[expect(
                clippy::cast_precision_loss,
                reason = "a share is 1 at most; a larger one is the whole room"
            )]
            Some(Value::Integer(share)) => *share as f64,
            _ => default,
        };
        let (width, height) = (share("width", POPUP_WIDTH), share("height", POPUP_HEIGHT));
        let program = match &self.popup_shell {
            Some(shell) => Program::command(shell, ["-c", command.as_str()]),
            None => Program::LoginCommand {
                command: command.into(),
            },
        };
        self.float_new(platform, Some(&program), folder.as_deref(), true, |_| {
            FloatingFrame::centered(width, height)
        });
        Ok(())
    }

    /// Floats a new pane, a popup or not, over the active tab of the window
    /// that acts, or has the focus, of `program`, the app's for none, in
    /// `folder`, the focused pane's for none, in the frame that `frame`
    /// gives of how many floating panes the tab has; whether it opened.
    fn float_new(
        &mut self,
        platform: &mut dyn Platform,
        program: Option<&Program>,
        folder: Option<&str>,
        popup: bool,
        frame: impl FnOnce(usize) -> FloatingFrame,
    ) -> bool {
        let Some(window) = self.acting.or_else(|| platform.focused_window()) else {
            return false;
        };
        let Some(shown) = self.windows.get(&window).map(|state| state.window) else {
            return false;
        };
        let Some(tab) = self
            .workspace
            .window(shown)
            .map(lokyr_kernel::workspace::Window::active_tab)
        else {
            return false;
        };
        let Some(count) = self.workspace.tab(tab).map(|held| held.floating().len()) else {
            return false;
        };
        let near = self.workspace.focused(shown).map(Pane::id);
        let dark = platform.appearance() == lokyr_platform_core::Appearance::Dark;
        let mut panes = spawn::Spawner {
            program: program.unwrap_or(&self.program),
            environment: &self.environment,
            budget: &self.budget,
            colors: &self.theme.variant(dark).colors,
            texts: &self.texts,
            config: &self.config,
            waker: platform.waker(),
            wakes: &mut self.pane_wakes,
        };
        let pane = lokyr_kernel::plugins::Panes::spawn(
            &mut panes,
            PaneId::new(),
            near.and_then(|near| self.workspace.pane(near)),
            folder,
        );
        let frame = frame(count);
        let opened = if popup {
            self.workspace.open_popup(tab, pane, frame)
        } else {
            self.workspace.float_pane(tab, pane, frame)
        };
        if opened.is_some() {
            self.keyboard_moved_in(platform, window);
            platform.request_frame(window);
        }
        opened.is_some()
    }

    /// Shows the floating panes of the active tab of the window that acts,
    /// or has the focus, or hides them, its popups showing of their own; of
    /// a tab of none but popups, a new one.
    pub(super) fn toggle_floating_panes(&mut self, platform: &mut dyn Platform) {
        let Some(window) = self.acting.or_else(|| platform.focused_window()) else {
            return;
        };
        let Some(shown) = self.windows.get(&window).map(|state| state.window) else {
            return;
        };
        let Some(tab) = self
            .workspace
            .window(shown)
            .map(lokyr_kernel::workspace::Window::active_tab)
        else {
            return;
        };
        let Some((none, floating_shown)) = self.workspace.tab(tab).map(|held| {
            (
                held.floating().iter().all(|floating| floating.popup),
                held.floating_shown(),
            )
        }) else {
            return;
        };
        if none {
            self.new_floating_pane(platform);
            return;
        }
        self.workspace.set_floating_shown(tab, !floating_shown);
        self.keyboard_moved_in(platform, window);
        platform.request_frame(window);
    }

    /// Floats the pane with the keyboard of the window that acts, or has
    /// the focus, of its tab's splits, in the room's middle, or moves the
    /// floating one into the splits, as `float` says.
    pub(super) fn float_or_dock(&mut self, platform: &mut dyn Platform, float: bool) {
        let Some(window) = self.acting.or_else(|| platform.focused_window()) else {
            return;
        };
        let Some(pane) = self
            .windows
            .get(&window)
            .and_then(|state| self.workspace.focused(state.window))
            .map(Pane::id)
        else {
            return;
        };
        let done = if float {
            let frame = FloatingFrame::centered(NEW_SHARE, NEW_SHARE);
            self.workspace.float_split_pane(pane, frame)
        } else {
            self.workspace.dock_floating(pane)
        };
        if done {
            self.keyboard_moved_in(platform, window);
            platform.request_frame(window);
        }
    }

    /// The left button's press over a floating pane's header, which starts
    /// its move, or over its edges and corners, which starts its resize, of
    /// `window` at `position`; or a release that ends one under way.
    /// Whether it was one.
    pub(super) fn floating_button(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        (button, pressed): (MouseButton, bool),
        position: Point,
    ) -> bool {
        let Some(scale) = platform.window_size(window).map(|size| size.scale) else {
            return false;
        };
        let Some(state) = self.windows.get_mut(&window) else {
            return false;
        };
        if !pressed {
            if state.floating_drag.take().is_none() {
                return false;
            }
            state.pointer_held = None;
            platform.hold_pointer(window, None);
            return true;
        }
        if button != MouseButton::Left {
            return false;
        }
        let Some(&(pane, room)) = state.floating_rooms.iter().rev().find(|(_, room)| {
            let points = |pixels: i32| f64::from(pixels) / scale;
            let (left, top) = (points(room.outer.x), points(room.outer.y));
            (left..left + f64::from(room.outer.width) / scale).contains(&position.x)
                && (top..top + f64::from(room.outer.height) / scale).contains(&position.y)
        }) else {
            return false;
        };
        let grab = match edge_at(&room, position, scale) {
            Some(edge) => FloatingGrab::Resize(edge),
            None if on_header(&room, position, scale) => FloatingGrab::Move,
            None => return false,
        };
        let Some(content) = state.panes_room else {
            return false;
        };
        let Some(frame) = self
            .workspace
            .tab_of(pane)
            .and_then(|tab| self.workspace.tab(tab))
            .and_then(|tab| tab.floating().iter().find(|of| of.pane == pane))
            .map(|floating| floating.frame)
        else {
            return false;
        };
        state.floating_drag = Some(FloatingDrag {
            pane,
            grab,
            from: position,
            frame,
            room: (
                f64::from(content.width) / scale,
                f64::from(content.height) / scale,
            ),
        });
        let held = match grab {
            FloatingGrab::Move => PointerShape::Arrow,
            FloatingGrab::Resize(edge) => PointerShape::ResizeFrame(edge),
        };
        state.pointer_held = Some(held);
        platform.hold_pointer(window, Some(held));
        // It comes in front, with the keyboard.
        self.focus_pane(platform, window, pane);
        platform.request_frame(window);
        true
    }

    /// The pointer moved to `position` over `window` in a floating pane's
    /// move or resize: its frame follows, within the tab's room. Whether
    /// one was under way.
    pub(super) fn floating_moved(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        position: Point,
    ) -> bool {
        let Some(drag) = self
            .windows
            .get(&window)
            .and_then(|state| state.floating_drag)
        else {
            return false;
        };
        let by = (
            (position.x - drag.from.x) / drag.room.0.max(1.0),
            (position.y - drag.from.y) / drag.room.1.max(1.0),
        );
        if self
            .workspace
            .move_floating(drag.pane, dragged(drag.frame, drag.grab, by))
        {
            platform.request_frame(window);
        }
        true
    }

    /// The keyboard moved in `window` as its floating panes changed, which
    /// its programs and its title learn.
    fn keyboard_moved_in(&mut self, platform: &mut dyn Platform, window: WindowId) {
        if let Some(pane) = self
            .windows
            .get(&window)
            .and_then(|state| self.workspace.focused(state.window))
            .map(Pane::id)
        {
            self.focus_pane(platform, window, pane);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drag_moves_the_frame_or_one_edge_within_the_room() {
        let frame = FloatingFrame::new(0.2, 0.2, 0.4, 0.4);
        let close = |a: f64, b: f64| (a - b).abs() < 1e-3;
        let moved = dragged(frame, FloatingGrab::Move, (0.1, -0.1));
        assert!(close(moved.x(), 0.3) && close(moved.y(), 0.1) && close(moved.width(), 0.4));
        // The left edge, the right one staying.
        let left = dragged(frame, FloatingGrab::Resize(FrameEdge::Left), (-0.1, 0.0));
        assert!(close(left.x(), 0.1) && close(left.width(), 0.5));
        // The bottom right corner, within the room.
        let corner = dragged(
            frame,
            FloatingGrab::Resize(FrameEdge::BottomRight),
            (0.9, 0.9),
        );
        assert!(close(corner.width(), 0.8) && close(corner.height(), 0.8));
        // A left edge past the right one stops at the least share.
        let past = dragged(frame, FloatingGrab::Resize(FrameEdge::Left), (0.9, 0.0));
        assert!(close(past.x() + past.width(), 0.6), "{past:?}");
    }
}
