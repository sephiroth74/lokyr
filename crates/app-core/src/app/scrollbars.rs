//! The panes' scroll bars, which the frames draw over each pane's right
//! edge, as macOS shows its own: as the view moves through the scrollback,
//! a thin knob that fades a while after; as the pointer comes over it, a
//! wider knob over a track, which a drag moves the view by, and a click on
//! the track pages or jumps, as the system's settings say; always, as they
//! say so or the config does. The marks of a search's matches show along it
//! while the search shows them, which keeps it shown. It takes no column of
//! the grid, and a view that shows every row, or the alternate screen, has
//! none.

use std::time::{Duration, Instant};

use lokyr_kernel::config::Scrollbar as Shows;
use lokyr_kernel::workspace::{PaneId, Workspace};
use lokyr_platform_core::{Modifiers, MouseButton, Platform, Point, PointerShape, WindowId};
use lokyr_render_core::{Rect, Shape};
use lokyr_ui_core::scrollbar::{self, Look, Mark, Rows};
use lokyr_ui_core::theme::Tokens;
use lokyr_vt_core::terminal::Scroll;

use super::App;
use crate::drawing::Backend;
use crate::window::Window;

/// How long a bar shows after the view last moved, or the pointer left it.
pub(crate) const SHOW: Duration = Duration::from_millis(1000);

/// How long its fade then takes.
pub(crate) const FADE: Duration = Duration::from_millis(300);

/// When the bars show: never, as the view moves and the pointer comes over
/// them, or always.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    Never,
    Overlay,
    Always,
}

/// A pane's bar, as a window's frame laid it out: its pane, its area in
/// pixels, the rows that it moves through, the number of the oldest, how it
/// looks, and its marks.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Bar {
    pub(crate) pane: PaneId,
    pub(crate) area: Rect,
    pub(crate) rows: Rows,
    pub(crate) oldest: i64,
    pub(crate) look: Look,
    pub(crate) marks: Vec<Mark>,
}

/// What a window keeps of a pane's bar between its frames: where the view
/// was, its rows' epoch, its first row's number and how many rows it was
/// above the screen; and when the view last moved, or the pointer left the
/// bar, from when it shows a while.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Kept {
    view: Option<(u64, i64, i64)>,
    moved: Option<Instant>,
}

/// A knob's drag: its pane, and how far below the knob's top the pointer
/// took it, in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BarDrag {
    pane: PaneId,
    grab: f32,
}

/// What a frame's bars ask of the app: when the next one's fade starts, if
/// one waits, and whether one fades now, which takes the next frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Settled {
    pub(super) due: Option<Instant>,
    pub(super) fading: bool,
}

/// Lays out `state`'s panes' bars for a frame at `now`, of `workspace`'s
/// panes, as `mode` shows them, on a display of `scale`, each pane's room
/// widened by its padding at the right, `padding` pixels, its fade ending at
/// once where the system reduces motion: a view that moved since the last
/// frame, both through its rows and from the screen, shows its bar again.
pub(super) fn settle<B: Backend>(
    workspace: &Workspace,
    state: &mut Window<B>,
    mode: Mode,
    (scale, padding): (f64, u32),
    (now, reduced): (Instant, bool),
) -> Settled {
    let Window {
        views,
        bars,
        bars_kept,
        bar_hover,
        bar_drag,
        bar_due,
        ..
    } = state;
    let mut settled = Settled::default();
    let mut laid = Vec::new();
    let scale = scale as f32;
    for view in views.iter() {
        let Some(viewport) = workspace
            .pane(view.pane)
            .and_then(lokyr_kernel::workspace::Pane::viewport)
        else {
            continue;
        };
        let kept = bars_kept.entry(view.pane).or_default();
        let place = (viewport.epoch, viewport.top, viewport.screen - viewport.top);
        if let Some((epoch, top, above)) = kept.view
            && epoch == place.0
            && top != place.1
            && above != place.2
        {
            kept.moved = Some(now);
        }
        kept.view = Some(place);
        if mode == Mode::Never || viewport.alternate {
            continue;
        }
        let count = |rows: i64| u64::try_from(rows).unwrap_or(0);
        let rows = Rows {
            total: count(viewport.screen - viewport.oldest) + u64::from(viewport.rows),
            top: count(viewport.top - viewport.oldest),
            shown: u64::from(viewport.rows),
        };
        if !rows.scrolls() {
            continue;
        }
        let room = Rect {
            width: view.rect.width + padding,
            ..view.rect
        };
        let area = scrollbar::area(room, scale);
        let slots = scrollbar::mark_slots(area, scale);
        // The terminal's lock, of a search that shows its matches alone.
        let marks: Vec<Mark> = workspace
            .pane(view.pane)
            .filter(|pane| pane.snapshot().matches_shown())
            .map(|pane| pane.match_marks(slots))
            .unwrap_or_default()
            .into_iter()
            .map(|(at, current)| Mark {
                at: (at as f32 + 0.5) / slots as f32,
                current,
            })
            .collect();
        let dragged = bar_drag.is_some_and(|drag| drag.pane == view.pane);
        let hovered = *bar_hover == Some(view.pane);
        let opacity = if mode == Mode::Always || dragged || hovered || !marks.is_empty() {
            1.0
        } else {
            match kept.moved {
                Some(moved) if now < moved + SHOW => {
                    let shown = moved + SHOW;
                    settled.due = Some(settled.due.map_or(shown, |due| due.min(shown)));
                    1.0
                }
                Some(moved) if !reduced && now < moved + SHOW + FADE => {
                    settled.fading = true;
                    let since = (now - moved - SHOW).as_secs_f32();
                    1.0 - since / FADE.as_secs_f32()
                }
                _ => 0.0,
            }
        };
        laid.push(Bar {
            pane: view.pane,
            area,
            rows,
            oldest: viewport.oldest,
            look: Look {
                opacity,
                wide: mode == Mode::Always || dragged || hovered,
                pressed: dragged,
            },
            marks,
        });
    }
    bars_kept.retain(|pane, _| views.iter().any(|view| view.pane == *pane));
    if bar_hover.is_some_and(|hovered| !laid.iter().any(|bar| bar.pane == hovered)) {
        *bar_hover = None;
    }
    *bars = laid;
    *bar_due = settled.due;
    settled
}

/// The shapes of `bars`, in `tokens`' colors, on a display of `scale`.
pub(super) fn shapes(bars: &[Bar], tokens: &Tokens, scale: f64) -> Vec<Shape> {
    bars.iter()
        .flat_map(|bar| {
            scrollbar::shapes(
                bar.area,
                bar.rows,
                bar.look,
                &bar.marks,
                tokens,
                scale as f32,
            )
        })
        .collect()
}

/// Whether `position`, in points, is in `area`, in pixels, on a display of
/// `scale`.
fn holds(area: Rect, position: Point, scale: f64) -> bool {
    let (x, y) = (position.x * scale, position.y * scale);
    let (left, top) = (f64::from(area.x), f64::from(area.y));
    (left..left + f64::from(area.width)).contains(&x)
        && (top..top + f64::from(area.height)).contains(&y)
}

/// The scroll that puts the view's first row at `top`, from the oldest, of
/// `bar`'s rows: to the screen, which the view then follows, at the last.
fn scroll_to(bar: &Bar, top: u64) -> Scroll {
    if top >= bar.rows.total.saturating_sub(bar.rows.shown) {
        Scroll::Bottom
    } else {
        Scroll::Row(
            bar.oldest
                .saturating_add(i64::try_from(top).unwrap_or(i64::MAX)),
        )
    }
}

impl<B: Backend> App<B> {
    /// When the bars show, as the config says, and the system's settings
    /// where it leaves it to them.
    pub(super) fn bar_mode(&self, platform: &dyn Platform) -> Mode {
        match self.config.window.scrollbar {
            Shows::Never => Mode::Never,
            Shows::Always => Mode::Always,
            Shows::System if platform.scrollers().overlay => Mode::Overlay,
            Shows::System => Mode::Always,
        }
    }

    /// The pointer pressed or released `button` over `window` at
    /// `position`, with `mods`: a release ends a knob's drag under way; the
    /// left button's press over a bar that shows drags its knob, or, over
    /// its track, pages toward the press, or jumps there, the knob's middle
    /// at the press, which the drag then holds, as the system's settings
    /// say, Option the other way. Whether a bar took it.
    pub(super) fn bar_button(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        (button, pressed): (MouseButton, bool),
        (position, mods): (Point, Modifiers),
    ) -> bool {
        let Some(scale) = platform.window_size(window).map(|size| size.scale) else {
            return false;
        };
        let jump = platform.scrollers().jump_to_spot != (mods.left_alt || mods.right_alt);
        let Some(state) = self.windows.get_mut(&window) else {
            return false;
        };
        if !pressed {
            let Some(drag) = state.bar_drag.take() else {
                return false;
            };
            state.bars_kept.entry(drag.pane).or_default().moved = Some(Instant::now());
            state.pointer_held = None;
            platform.hold_pointer(window, None);
            platform.request_frame(window);
            return true;
        }
        if button != MouseButton::Left {
            return false;
        }
        let Some(bar) = state
            .bars
            .iter()
            .find(|bar| bar.look.opacity > 0.0 && holds(bar.area, position, scale))
            .cloned()
        else {
            return false;
        };
        let scale = scale as f32;
        let y = position.y as f32 * scale;
        let knob = scrollbar::knob(bar.area, bar.rows, true, scale);
        let on_knob = (knob.y as f32..knob.y as f32 + knob.height as f32).contains(&y);
        let grab = if on_knob {
            y - knob.y as f32
        } else if jump {
            let top = scrollbar::centered_at(bar.area, bar.rows, scale, y);
            if let Some(pane) = self.workspace.pane(bar.pane) {
                pane.scroll(scroll_to(
                    &bar,
                    scrollbar::top_at(bar.area, bar.rows, scale, top),
                ));
            }
            y - top
        } else {
            if let Some(pane) = self.workspace.pane(bar.pane) {
                pane.scroll(if y < knob.y as f32 {
                    Scroll::PageUp
                } else {
                    Scroll::PageDown
                });
            }
            platform.request_frame(window);
            return true;
        };
        if let Some(state) = self.windows.get_mut(&window) {
            state.bar_drag = Some(BarDrag {
                pane: bar.pane,
                grab,
            });
            state.pointer_held = Some(PointerShape::Arrow);
        }
        platform.hold_pointer(window, Some(PointerShape::Arrow));
        platform.request_frame(window);
        true
    }

    /// The pointer moved to `position` over `window` in a knob's drag: the
    /// view goes to the row that the knob's place gives. Whether a drag was
    /// under way.
    pub(super) fn bar_dragged(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        position: Point,
    ) -> bool {
        let Some(scale) = platform.window_size(window).map(|size| size.scale as f32) else {
            return false;
        };
        let Some(state) = self.windows.get(&window) else {
            return false;
        };
        let Some(drag) = state.bar_drag else {
            return false;
        };
        let Some(bar) = state.bars.iter().find(|bar| bar.pane == drag.pane) else {
            return true;
        };
        let top = position.y as f32 * scale - drag.grab;
        let scroll = scroll_to(bar, scrollbar::top_at(bar.area, bar.rows, scale, top));
        if let Some(pane) = self.workspace.pane(drag.pane) {
            pane.scroll(scroll);
        }
        platform.request_frame(window);
        true
    }

    /// The pointer moved to `position` over `window`, or left its areas of
    /// moves, of `None`: the bar under it shows, its knob wide over its
    /// track; the one that it left shows a while, then fades.
    pub(super) fn bar_hovered(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        position: Option<Point>,
    ) {
        let Some(scale) = platform.window_size(window).map(|size| size.scale) else {
            return;
        };
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        let under = position.and_then(|position| {
            state
                .bars
                .iter()
                .find(|bar| holds(bar.area, position, scale))
                .map(|bar| bar.pane)
        });
        if under == state.bar_hover {
            return;
        }
        if let Some(left) = state.bar_hover {
            state.bars_kept.entry(left).or_default().moved = Some(Instant::now());
        }
        state.bar_hover = under;
        platform.request_frame(window);
    }

    /// Starts the timer of the bars' fades of `due`, if it comes before the
    /// one that runs.
    pub(super) fn bar_timer_at(&mut self, platform: &mut dyn Platform, due: Option<Instant>) {
        let Some(due) = due else {
            return;
        };
        if self.bar_timer.is_some_and(|(_, at)| at <= due) {
            return;
        }
        self.bar_timer = Some((platform.start_timer(due), due));
    }

    /// The timer of the bars' fades fired: the windows whose bars fade now
    /// draw them.
    pub(super) fn bar_timer_fired(&mut self, platform: &mut dyn Platform) {
        self.bar_timer = None;
        let now = Instant::now();
        let due: Vec<WindowId> = self
            .windows
            .iter()
            .filter(|(_, state)| state.bar_due.is_some_and(|due| due <= now))
            .map(|(&window, _)| window)
            .collect();
        for window in due {
            platform.request_frame(window);
        }
        // A later one, of another window, takes a timer of its own.
        let next = self
            .windows
            .values()
            .filter_map(|state| state.bar_due)
            .filter(|&due| due > now)
            .min();
        self.bar_timer_at(platform, next);
    }
}
