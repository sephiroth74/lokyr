//! The dividers between a window's panes: lines of the theme's
//! `pane.divider`, which the frames draw, each of a grab area wider than
//! its line, of a resize pointer, whose drag moves its split's share as the
//! pointer goes, and whose double click shares its split half and half.
//! The terminals take their new grids at the resizes' pace, so that a drag
//! sends their programs a signal every 25 ms at most, and a last one.

use lokyr_kernel::workspace::{Direction, Ratio};
use lokyr_platform_core::{
    MouseButton, Platform, Point, PointerArea, PointerShape, Size, WindowId,
};

use super::App;
use super::frame::Divider;
use crate::drawing::Backend;
use crate::window::DividerDrag;

/// How wide a divider's grab area is, across its line, in points.
const GRAB: f64 = 6.0;

/// The pointer's area of `divider`'s grab, on a display of `scale`: its
/// line widened across to [`GRAB`] points, of a column's resize between two
/// panes side by side, and of a row's between two one over the other.
pub(crate) fn grab_area(divider: &Divider, scale: f64) -> PointerArea {
    let (origin, size) = grab(divider, scale);
    PointerArea {
        origin,
        size,
        shape: shape(divider),
    }
}

/// The resize pointer of `divider`.
fn shape(divider: &Divider) -> PointerShape {
    match divider.direction {
        Direction::Horizontal => PointerShape::ResizeColumn,
        Direction::Vertical => PointerShape::ResizeRow,
    }
}

/// `divider`'s grab area, its top left corner and its size, in points of
/// the content, on a display of `scale`.
fn grab(divider: &Divider, scale: f64) -> (Point, Size) {
    let line = divider.line;
    let points = |pixels: f64| pixels / scale;
    let (x, y) = (points(f64::from(line.x)), points(f64::from(line.y)));
    let (width, height) = (
        points(f64::from(line.width)),
        points(f64::from(line.height)),
    );
    match divider.direction {
        Direction::Horizontal => {
            let wider = (GRAB - width).max(0.0) / 2.0;
            (
                Point { x: x - wider, y },
                Size {
                    width: width.max(GRAB),
                    height,
                },
            )
        }
        Direction::Vertical => {
            let wider = (GRAB - height).max(0.0) / 2.0;
            (
                Point { x, y: y - wider },
                Size {
                    width,
                    height: height.max(GRAB),
                },
            )
        }
    }
}

impl<B: Backend> App<B> {
    /// The left button's press over a divider's grab area of `window`, at
    /// `position`, which starts its drag, or of a double click, which
    /// shares its split half and half; or a release that ends a drag under
    /// way. Whether it was one.
    pub(super) fn divider_button(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        (button, pressed, clicks): (MouseButton, bool, u32),
        position: Point,
    ) -> bool {
        let Some(scale) = platform.window_size(window).map(|size| size.scale) else {
            return false;
        };
        let Some(state) = self.windows.get_mut(&window) else {
            return false;
        };
        if !pressed {
            if state.divider_drag.take().is_none() {
                return false;
            }
            state.pointer_held = None;
            platform.hold_pointer(window, None);
            return true;
        }
        if button != MouseButton::Left {
            return false;
        }
        let Some(divider) = state
            .dividers
            .iter()
            .find(|divider| {
                let (origin, size) = grab(divider, scale);
                (origin.x..origin.x + size.width).contains(&position.x)
                    && (origin.y..origin.y + size.height).contains(&position.y)
            })
            .cloned()
        else {
            return false;
        };
        let Some(tab) = self
            .workspace
            .window(state.window)
            .map(lokyr_kernel::workspace::Window::active_tab)
        else {
            return false;
        };
        // Half and half, of a double click.
        if clicks >= 2 {
            self.workspace
                .set_ratio(tab, &divider.path, Some(Ratio::HALF));
            platform.request_frame(window);
            return true;
        }
        let share = self
            .workspace
            .tab(tab)
            .and_then(|shown| shown.layout().at(&divider.path))
            .and_then(lokyr_kernel::workspace::Layout::share)
            .unwrap_or(0.5);
        let across = divider.direction == Direction::Horizontal;
        let from = if across { position.x } else { position.y };
        let room = f64::from(divider.room) / scale;
        state.divider_drag = Some(DividerDrag {
            tab,
            path: divider.path.clone(),
            across,
            from,
            share,
            room,
        });
        // The resize pointer, held wherever the drag goes.
        let held = shape(&divider);
        state.pointer_held = Some(held);
        platform.hold_pointer(window, Some(held));
        true
    }

    /// The pointer moved to `position` over `window` in a divider's drag:
    /// its split takes the share that the pointer's way along it gives,
    /// within the split's bounds. Whether a drag was under way.
    pub(super) fn divider_moved(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        position: Point,
    ) -> bool {
        let Some(drag) = self
            .windows
            .get(&window)
            .and_then(|state| state.divider_drag.clone())
        else {
            return false;
        };
        let along = if drag.across { position.x } else { position.y };
        let share = drag.share + (along - drag.from) / drag.room.max(1.0);
        if self
            .workspace
            .set_ratio(drag.tab, &drag.path, Some(Ratio::new(share)))
        {
            platform.request_frame(window);
        }
        true
    }
}
