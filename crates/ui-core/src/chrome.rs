//! A window's chrome: the regions of its slots, where the views of the
//! plugins that provide them show, around and over its panes.
//!
//! The tab bar goes in the titlebar, past the window's buttons, under a
//! titlebar of the window's title, across the window's bottom, below the
//! status bar, or down a column at its left or its right, from below the
//! titlebar to the window's bottom, or at its left from its top, through
//! the titlebar, as macOS's sidebars; the window's strip goes across the
//! panes' top, over them, as it takes none of their rows; the status bar
//! goes across the bottom of the room that a column leaves, of its view's
//! height; a pane's header goes across the pane's top, of its view's
//! height, above its grid. The overlays and the popups are the rooms that
//! the floating panels anchor in. A slot whose view is empty takes no
//! room.

use lokyr_plugin_sdk::view::Kind;

use crate::layout::{Frame, Layout};
use crate::tree::{NodeId, Tree};

/// Where the tab bar goes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum TabBarPlace {
    /// In the titlebar, past the window's buttons, the titlebar's height;
    /// at the window's top, of its view's height, where the window has no
    /// titlebar, as in full screen.
    #[default]
    Titlebar,
    /// Under a titlebar of the window's title, of its view's height.
    BelowTitlebar,
    /// Across the window's bottom, below the status bar, of its view's
    /// height.
    Bottom,
    /// Down a column at the window's left, of this width in pixels, from
    /// below the titlebar to the window's bottom.
    Left(f32),
    /// Down a column at the window's right, likewise.
    Right(f32),
    /// Down a column at the window's left, of this width in pixels, from
    /// the window's top, through the titlebar, under the window's buttons,
    /// the panes beside it below the titlebar.
    Sidebar(f32),
    /// Across the whole titlebar, of its height, in place of the window's
    /// title; none where the window has no titlebar, as in full screen.
    Title,
}

impl TabBarPlace {
    /// Whether the bar goes down a column, on a side.
    pub fn is_column(self) -> bool {
        matches!(self, Self::Left(_) | Self::Right(_) | Self::Sidebar(_))
    }
}

/// A window's content, as its platform gives it, in pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Window {
    /// Its width and its height, the titlebar's too.
    pub size: (f32, f32),
    /// The titlebar's height over its top, which the content runs under; 0
    /// without one, as in full screen.
    pub titlebar: f32,
    /// The room at the titlebar's left that the window's buttons take; 0
    /// without them.
    pub buttons: f32,
}

/// The heights of the views of the window's slots, in pixels, as their
/// trees take them laid out across the window; 0 for a slot of no view.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Heights {
    /// The tab bar's, which the titlebar's height replaces in the
    /// titlebar, and a column's height its own.
    pub tab_bar: f32,
    /// The window's strip's.
    pub window_strip: f32,
    /// The status bar's.
    pub status_bar: f32,
    /// The width of the titlebar's small buttons, their views side by side.
    pub title_buttons: f32,
}

/// The regions of a window's slots, in pixels from its content's top left
/// corner, the titlebar's too.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Regions {
    /// The tab bar's, if it has a view.
    pub tab_bar: Option<Frame>,
    /// The titlebar's small buttons', right after the window's buttons, if
    /// they have views: in full screen, at the top's left, beside a tab
    /// bar across the top, and nowhere without one.
    pub title_buttons: Option<Frame>,
    /// The window's strip's, over the panes' top, if it has a view.
    pub window_strip: Option<Frame>,
    /// The status bar's, if it has a view.
    pub status_bar: Option<Frame>,
    /// The room that the panes share: below the titlebar and a tab bar
    /// across the top, beside a column's, above the status bar and a tab
    /// bar across the bottom.
    pub panes: Frame,
    /// The window's overlays', as a palette's: the panes' room and the
    /// status bar's, which the tab bar leaves.
    pub window_overlay: Frame,
    /// The popups', the whole window.
    pub popup: Frame,
}

/// The regions of a pane, in pixels from the window's content's corner.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PaneRegions {
    /// Its header's, at its top, if it has a view.
    pub header: Option<Frame>,
    /// Its grid's, below its header.
    pub grid: Frame,
    /// Its overlays', as a find bar's, over its grid.
    pub overlay: Frame,
}

/// The regions of `window`'s slots, the tab bar at `place`, of the views of
/// `heights`, the tab bar's of a column any height but 0.
pub fn regions(window: &Window, place: TabBarPlace, heights: Heights) -> Regions {
    let (width, height) = (window.size.0.max(0.0), window.size.1.max(0.0));
    let titlebar = window.titlebar.clamp(0.0, height);
    // A title has no room but the titlebar's.
    let shows = heights.tab_bar > 0.0 && (place != TabBarPlace::Title || titlebar > 0.0);
    let mut top = titlebar;
    let (mut left, mut right, mut bottom) = (0.0, width, height);
    // A bar across the top, a column on a side, or one across the bottom,
    // within the window.
    let tab_bar = shows.then(|| match place {
        TabBarPlace::Title => Frame {
            x: 0.0,
            y: 0.0,
            width,
            height: titlebar,
        },
        TabBarPlace::Titlebar if titlebar > 0.0 => {
            let buttons = window.buttons.clamp(0.0, width);
            let after = (buttons + heights.title_buttons.max(0.0)).min(width);
            Frame {
                x: after,
                y: 0.0,
                width: width - after,
                height: titlebar,
            }
        }
        TabBarPlace::Titlebar | TabBarPlace::BelowTitlebar => {
            let bar = Frame {
                x: 0.0,
                y: top,
                width,
                height: heights.tab_bar.min(height - top),
            };
            top += bar.height;
            bar
        }
        TabBarPlace::Bottom => {
            let bar_height = heights.tab_bar.min(height - top);
            bottom = height - bar_height;
            Frame {
                x: 0.0,
                y: bottom,
                width,
                height: bar_height,
            }
        }
        TabBarPlace::Sidebar(column) => {
            let column = column.clamp(0.0, width);
            left = column;
            Frame {
                x: 0.0,
                y: 0.0,
                width: column,
                height,
            }
        }
        TabBarPlace::Left(column) | TabBarPlace::Right(column) => {
            let column = column.clamp(0.0, width);
            let x = if matches!(place, TabBarPlace::Left(_)) {
                left = column;
                0.0
            } else {
                right = width - column;
                right
            };
            Frame {
                x,
                y: top,
                width: column,
                height: height - top,
            }
        }
    });
    // The titlebar's small buttons, right after the window's; in full
    // screen, at the left of a tab bar across the top, which they push.
    let small = heights.title_buttons.max(0.0);
    let mut tab_bar = tab_bar;
    let title_buttons = if small <= 0.0 {
        None
    } else if titlebar > 0.0 {
        let buttons = window.buttons.clamp(0.0, width);
        Some(Frame {
            x: buttons,
            y: 0.0,
            width: small.min(width - buttons),
            height: titlebar,
        })
    } else {
        match (&mut tab_bar, place) {
            (Some(bar), TabBarPlace::Titlebar | TabBarPlace::BelowTitlebar) => {
                let taken = small.min(bar.width);
                let frame = Frame {
                    width: taken,
                    ..*bar
                };
                bar.x += taken;
                bar.width -= taken;
                Some(frame)
            }
            _ => None,
        }
    };
    // The room that the tab bar leaves, which the status bar takes the
    // bottom of.
    let room_bottom = bottom;
    let status_bar = (heights.status_bar > 0.0).then(|| {
        let bar_height = heights.status_bar.min(room_bottom - top);
        bottom = room_bottom - bar_height;
        Frame {
            x: left,
            y: bottom,
            width: right - left,
            height: bar_height,
        }
    });
    let panes = Frame {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    };
    let window_strip = (heights.window_strip > 0.0).then_some(Frame {
        height: heights.window_strip.min(panes.height),
        ..panes
    });
    Regions {
        tab_bar,
        title_buttons,
        window_strip,
        status_bar,
        panes,
        window_overlay: Frame {
            x: left,
            y: top,
            width: right - left,
            height: room_bottom - top,
        },
        popup: Frame {
            x: 0.0,
            y: 0.0,
            width,
            height,
        },
    }
}

/// The regions of a pane of `pane`, its header's view `header` high, 0 for
/// none.
pub fn pane_regions(pane: Frame, header: f32) -> PaneRegions {
    let header = header.clamp(0.0, pane.height.max(0.0));
    let grid = Frame {
        y: pane.y + header,
        height: pane.height.max(0.0) - header,
        ..pane
    };
    PaneRegions {
        header: (header > 0.0).then_some(Frame {
            height: header,
            ..pane
        }),
        grid,
        overlay: grid,
    }
}

/// The parts of a view in the titlebar that are its own, as a tab bar's or
/// the small buttons', from its corner: the boxes of the outermost nodes
/// under its root that take a press, each within the clips around it, the
/// root's too, where a press goes to the view; or the root's whole box, if
/// it takes one itself. A node takes a press of its key, and of its kind,
/// a button, a toggle, a field, a list or a view of tabs, or of a pointer,
/// a hover or a press of its own style, as a tab: a node of a key that
/// takes none, as a group of tabs that a transition needs, gives its
/// children's parts. The rest of the view, its root's own box too, is the
/// window's, which a drag there moves and a double click zooms.
pub fn titlebar_areas(tree: &Tree, layout: &Layout) -> Vec<Frame> {
    let mut areas = Vec::new();
    let Some(root) = tree.root() else {
        return areas;
    };
    let Some(bounds) = layout.frame(tree, root) else {
        return areas;
    };
    // A root that takes a press itself, as a view of one button, is whole.
    if takes_press(tree, root) {
        areas.push(bounds);
        return areas;
    }
    for &child in tree.children(root) {
        pressed(tree, layout, child, bounds, &mut areas);
    }
    areas
}

/// Adds the box of `id` within `bounds` to `areas`, if it takes a press;
/// otherwise its children's, within its own box too if it clips them, as
/// a scroll area does.
fn pressed(tree: &Tree, layout: &Layout, id: NodeId, bounds: Frame, areas: &mut Vec<Frame>) {
    let Some(frame) = layout.frame(tree, id) else {
        return;
    };
    if takes_press(tree, id) {
        areas.extend(within(frame, bounds));
        return;
    }
    let clips = matches!(tree.kind(id), Some(Kind::Scroll))
        || tree.style(id).is_some_and(|style| style.clip);
    let bounds = if clips {
        match within(frame, bounds) {
            Some(bounds) => bounds,
            None => return,
        }
    } else {
        bounds
    };
    for &child in tree.children(id) {
        pressed(tree, layout, child, bounds, areas);
    }
}

/// Whether the node `id` takes a press: of a key, and of a kind that takes
/// one, or of a pointer, a hover or a press of its own style.
pub(crate) fn takes_press(tree: &Tree, id: NodeId) -> bool {
    if tree.key(id).is_none() {
        return false;
    }
    let kind = matches!(
        tree.kind(id),
        Some(Kind::Button(_) | Kind::Toggle(_) | Kind::Field(_) | Kind::List(_) | Kind::Tabs(_))
    );
    kind || tree.style(id).is_some_and(|style| {
        style.pointer.is_some() || style.hover.is_some() || style.pressed.is_some()
    })
}

/// The part of `frame` within `bounds`, if they overlap.
fn within(frame: Frame, bounds: Frame) -> Option<Frame> {
    let (left, top) = (frame.x.max(bounds.x), frame.y.max(bounds.y));
    let right = (frame.x + frame.width).min(bounds.x + bounds.width);
    let bottom = (frame.y + frame.height).min(bounds.y + bounds.height);
    (right > left && bottom > top).then_some(Frame {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

#[cfg(test)]
mod tests {
    use lokyr_plugin_sdk::view::{
        Direction, Edges, Hover, Layout as NodeLayout, Length, Node, PointerShape, Style,
    };

    use super::*;
    use crate::layout::Measure;

    struct Fixed;

    impl Measure for Fixed {
        fn text(&mut self, text: &str) -> (f32, f32) {
            (10.0 * text.chars().count() as f32, 14.0)
        }

        fn icon(&mut self, _: &str) -> (f32, f32) {
            (16.0, 16.0)
        }
    }

    fn frame(x: f32, y: f32, width: f32, height: f32) -> Frame {
        Frame {
            x,
            y,
            width,
            height,
        }
    }

    /// A window of 800 by 600 pixels, of a titlebar 40 high and buttons 80
    /// wide.
    const WINDOW: Window = Window {
        size: (800.0, 600.0),
        titlebar: 40.0,
        buttons: 80.0,
    };

    #[test]
    fn the_title_buttons_go_after_the_window_s_and_push_a_tab_bar_in_the_titlebar() {
        let heights = Heights {
            tab_bar: 40.0,
            title_buttons: 30.0,
            ..Heights::default()
        };
        let laid = regions(&WINDOW, TabBarPlace::Titlebar, heights);
        assert_eq!(laid.title_buttons, Some(frame(80.0, 0.0, 30.0, 40.0)));
        assert_eq!(laid.tab_bar, Some(frame(110.0, 0.0, 690.0, 40.0)));
        // Below a titled bar, the bar across the window, the buttons in the
        // titlebar still.
        let below = regions(&WINDOW, TabBarPlace::BelowTitlebar, heights);
        assert_eq!(below.title_buttons, Some(frame(80.0, 0.0, 30.0, 40.0)));
        assert_eq!(below.tab_bar, Some(frame(0.0, 40.0, 800.0, 40.0)));
        // Of no view, none, the tab bar after the window's buttons.
        let none = regions(
            &WINDOW,
            TabBarPlace::Titlebar,
            Heights {
                title_buttons: 0.0,
                ..heights
            },
        );
        assert_eq!(none.title_buttons, None);
        assert_eq!(none.tab_bar, Some(frame(80.0, 0.0, 720.0, 40.0)));
    }

    #[test]
    fn in_full_screen_the_title_buttons_go_beside_a_tab_bar_across_the_top_or_nowhere() {
        let full = Window {
            titlebar: 0.0,
            buttons: 0.0,
            ..WINDOW
        };
        let heights = Heights {
            tab_bar: 30.0,
            title_buttons: 40.0,
            ..Heights::default()
        };
        let top = regions(&full, TabBarPlace::Titlebar, heights);
        assert_eq!(top.title_buttons, Some(frame(0.0, 0.0, 40.0, 30.0)));
        assert_eq!(top.tab_bar, Some(frame(40.0, 0.0, 760.0, 30.0)));
        let bottom = regions(&full, TabBarPlace::Bottom, heights);
        assert_eq!(bottom.title_buttons, None);
    }

    #[test]
    fn a_tab_bar_in_the_titlebar_takes_it_past_the_buttons_and_the_panes_the_rest() {
        let heights = Heights {
            tab_bar: 30.0,
            window_strip: 2.0,
            status_bar: 22.0,
            title_buttons: 0.0,
        };
        let regions = regions(&WINDOW, TabBarPlace::Titlebar, heights);
        // The titlebar's height, not the view's.
        assert_eq!(regions.tab_bar, Some(frame(80.0, 0.0, 720.0, 40.0)));
        assert_eq!(regions.panes, frame(0.0, 40.0, 800.0, 538.0));
        // The strip over the panes' top, which keep their room.
        assert_eq!(regions.window_strip, Some(frame(0.0, 40.0, 800.0, 2.0)));
        assert_eq!(regions.status_bar, Some(frame(0.0, 578.0, 800.0, 22.0)));
        assert_eq!(regions.window_overlay, frame(0.0, 40.0, 800.0, 560.0));
        assert_eq!(regions.popup, frame(0.0, 0.0, 800.0, 600.0));
    }

    #[test]
    fn a_tab_bar_below_the_titlebar_takes_its_view_s_height_across_the_window() {
        let heights = Heights {
            tab_bar: 30.0,
            ..Heights::default()
        };
        let window = Window {
            titlebar: 32.0,
            ..WINDOW
        };
        let regions = regions(&window, TabBarPlace::BelowTitlebar, heights);
        assert_eq!(regions.tab_bar, Some(frame(0.0, 32.0, 800.0, 30.0)));
        assert_eq!(regions.panes, frame(0.0, 62.0, 800.0, 538.0));
        assert_eq!(regions.window_overlay, frame(0.0, 62.0, 800.0, 538.0));
        assert_eq!((regions.window_strip, regions.status_bar), (None, None));
    }

    #[test]
    fn in_full_screen_the_tab_bar_takes_the_window_s_top_of_its_view_s_height() {
        let heights = Heights {
            tab_bar: 40.0,
            ..Heights::default()
        };
        let window = Window {
            titlebar: 0.0,
            buttons: 0.0,
            ..WINDOW
        };
        let regions = regions(&window, TabBarPlace::Titlebar, heights);
        assert_eq!(regions.tab_bar, Some(frame(0.0, 0.0, 800.0, 40.0)));
        assert_eq!(regions.panes, frame(0.0, 40.0, 800.0, 560.0));
    }

    #[test]
    fn slots_of_no_view_take_no_room() {
        let regions = regions(&WINDOW, TabBarPlace::Titlebar, Heights::default());
        assert_eq!(
            (regions.tab_bar, regions.window_strip, regions.status_bar),
            (None, None, None)
        );
        // Below the titlebar, as without a chrome.
        assert_eq!(regions.panes, frame(0.0, 40.0, 800.0, 560.0));
        let below = super::regions(&WINDOW, TabBarPlace::BelowTitlebar, Heights::default());
        assert_eq!(below.panes, regions.panes);
    }

    #[test]
    fn a_window_too_small_for_its_bars_keeps_them_within_it() {
        let window = Window {
            size: (60.0, 50.0),
            titlebar: 40.0,
            buttons: 80.0,
        };
        let heights = Heights {
            tab_bar: 30.0,
            window_strip: 20.0,
            status_bar: 22.0,
            title_buttons: 0.0,
        };
        let regions = regions(&window, TabBarPlace::Titlebar, heights);
        // Past buttons wider than the window, nothing.
        assert_eq!(regions.tab_bar, Some(frame(60.0, 0.0, 0.0, 40.0)));
        assert_eq!(regions.status_bar, Some(frame(0.0, 40.0, 60.0, 10.0)));
        assert_eq!(regions.panes, frame(0.0, 40.0, 60.0, 0.0));
        assert_eq!(regions.window_strip, Some(frame(0.0, 40.0, 60.0, 0.0)));
        let below = super::regions(&window, TabBarPlace::BelowTitlebar, heights);
        assert_eq!(below.tab_bar, Some(frame(0.0, 40.0, 60.0, 10.0)));
        assert_eq!(below.status_bar, Some(frame(0.0, 50.0, 60.0, 0.0)));
        assert_eq!(below.panes, frame(0.0, 50.0, 60.0, 0.0));
    }

    #[test]
    fn a_tab_bar_at_the_bottom_goes_below_the_status_bar() {
        let heights = Heights {
            tab_bar: 30.0,
            window_strip: 2.0,
            status_bar: 22.0,
            title_buttons: 0.0,
        };
        let regions = regions(&WINDOW, TabBarPlace::Bottom, heights);
        assert_eq!(regions.tab_bar, Some(frame(0.0, 570.0, 800.0, 30.0)));
        assert_eq!(regions.status_bar, Some(frame(0.0, 548.0, 800.0, 22.0)));
        assert_eq!(regions.panes, frame(0.0, 40.0, 800.0, 508.0));
        assert_eq!(regions.window_strip, Some(frame(0.0, 40.0, 800.0, 2.0)));
        assert_eq!(regions.window_overlay, frame(0.0, 40.0, 800.0, 530.0));
    }

    #[test]
    fn a_column_goes_down_a_side_below_the_titlebar_and_the_rest_beside_it() {
        let heights = Heights {
            tab_bar: 1.0,
            window_strip: 2.0,
            status_bar: 22.0,
            title_buttons: 0.0,
        };
        let left = regions(&WINDOW, TabBarPlace::Left(200.0), heights);
        assert_eq!(left.tab_bar, Some(frame(0.0, 40.0, 200.0, 560.0)));
        assert_eq!(left.panes, frame(200.0, 40.0, 600.0, 538.0));
        assert_eq!(left.window_strip, Some(frame(200.0, 40.0, 600.0, 2.0)));
        assert_eq!(left.status_bar, Some(frame(200.0, 578.0, 600.0, 22.0)));
        assert_eq!(left.window_overlay, frame(200.0, 40.0, 600.0, 560.0));
        let right = regions(&WINDOW, TabBarPlace::Right(200.0), heights);
        assert_eq!(right.tab_bar, Some(frame(600.0, 40.0, 200.0, 560.0)));
        assert_eq!(right.panes, frame(0.0, 40.0, 600.0, 538.0));
        assert_eq!(right.status_bar, Some(frame(0.0, 578.0, 600.0, 22.0)));
        // In full screen, from the window's top.
        let full = Window {
            titlebar: 0.0,
            buttons: 0.0,
            ..WINDOW
        };
        let column = regions(&full, TabBarPlace::Left(200.0), heights);
        assert_eq!(column.tab_bar, Some(frame(0.0, 0.0, 200.0, 600.0)));
        // A column of no view takes no room.
        let bare = regions(&WINDOW, TabBarPlace::Right(200.0), Heights::default());
        assert_eq!(
            (bare.tab_bar, bare.panes),
            (None, frame(0.0, 40.0, 800.0, 560.0))
        );
        assert!(TabBarPlace::Left(1.0).is_column() && !TabBarPlace::Bottom.is_column());
    }

    #[test]
    fn a_sidebar_runs_through_the_titlebar_and_the_panes_beside_it_below_it() {
        let heights = Heights {
            tab_bar: 1.0,
            window_strip: 2.0,
            status_bar: 22.0,
            title_buttons: 0.0,
        };
        let side = regions(&WINDOW, TabBarPlace::Sidebar(200.0), heights);
        assert_eq!(side.tab_bar, Some(frame(0.0, 0.0, 200.0, 600.0)));
        assert_eq!(side.panes, frame(200.0, 40.0, 600.0, 538.0));
        assert_eq!(side.window_strip, Some(frame(200.0, 40.0, 600.0, 2.0)));
        assert_eq!(side.status_bar, Some(frame(200.0, 578.0, 600.0, 22.0)));
        assert_eq!(side.window_overlay, frame(200.0, 40.0, 600.0, 560.0));
        assert!(TabBarPlace::Sidebar(1.0).is_column());
        // Of no view, the titlebar's room is the panes'.
        let bare = regions(&WINDOW, TabBarPlace::Sidebar(200.0), Heights::default());
        assert_eq!(
            (bare.tab_bar, bare.panes),
            (None, frame(0.0, 40.0, 800.0, 560.0))
        );
    }

    #[test]
    fn a_window_too_small_for_its_column_or_its_bottom_bar_keeps_them_within_it() {
        let window = Window {
            size: (150.0, 60.0),
            titlebar: 40.0,
            buttons: 80.0,
        };
        let heights = Heights {
            tab_bar: 30.0,
            window_strip: 2.0,
            status_bar: 22.0,
            title_buttons: 0.0,
        };
        let left = regions(&window, TabBarPlace::Left(200.0), heights);
        assert_eq!(left.tab_bar, Some(frame(0.0, 40.0, 150.0, 20.0)));
        assert_eq!(left.panes, frame(150.0, 40.0, 0.0, 0.0));
        assert_eq!(left.status_bar, Some(frame(150.0, 40.0, 0.0, 20.0)));
        let bottom = regions(&window, TabBarPlace::Bottom, heights);
        assert_eq!(bottom.tab_bar, Some(frame(0.0, 40.0, 150.0, 20.0)));
        assert_eq!(bottom.status_bar, Some(frame(0.0, 40.0, 150.0, 0.0)));
        assert_eq!(bottom.panes, frame(0.0, 40.0, 150.0, 0.0));
    }

    #[test]
    fn a_pane_s_header_takes_its_top_over_its_grid() {
        let pane = frame(10.0, 40.0, 300.0, 200.0);
        let regions = pane_regions(pane, 24.0);
        assert_eq!(regions.header, Some(frame(10.0, 40.0, 300.0, 24.0)));
        assert_eq!(regions.grid, frame(10.0, 64.0, 300.0, 176.0));
        assert_eq!(regions.overlay, regions.grid);
        let bare = pane_regions(pane, 0.0);
        assert_eq!((bare.header, bare.grid), (None, pane));
        assert_eq!(pane_regions(pane, 500.0).grid.height, 0.0);
    }

    /// The areas of the titlebar of `root`'s tree, laid out.
    fn areas_of(root: &Node) -> Vec<Frame> {
        let mut tree = Tree::new();
        let changes = tree.update(root).unwrap();
        let mut layout = Layout::new(1.0);
        layout.update(&tree, &changes);
        layout.compute(&tree, None, None, &mut Fixed);
        titlebar_areas(&tree, &layout)
    }

    /// A tab of `key`, 40 points wide of its title and padding, of a hover
    /// and a pointer, as the bar's.
    fn tab(key: &str, title: &str) -> Node {
        Node::container(vec![Node::text(title).key(format!("{key}.title").as_str())])
            .key(key)
            .layout(NodeLayout {
                padding: Edges::all(5.0),
                ..NodeLayout::default()
            })
            .style(Style {
                pointer: Some(PointerShape::Arrow),
                hover: Some(Hover::default()),
                ..Style::default()
            })
    }

    /// A bar of `children`, 200 points by 40.
    fn bar(children: Vec<Node>) -> Node {
        Node::container(children).key("bar").layout(NodeLayout {
            width: Length::Points(200.0),
            height: Length::Points(40.0),
            ..NodeLayout::default()
        })
    }

    #[test]
    fn the_titlebar_s_areas_are_the_outermost_nodes_that_take_a_press() {
        // The root's own key gives it none, a text of no key none, a
        // tab's title none past its tab's.
        let root = bar(vec![
            Node::container(vec![tab("tab-1", "one"), tab("tab-2", "two")]),
            Node::spacer(),
            Node::text("plain"),
        ]);
        let tabs = [frame(0.0, 0.0, 40.0, 40.0), frame(40.0, 0.0, 40.0, 40.0)];
        assert_eq!(areas_of(&root), tabs);
        // A group of the tabs of a key, as a transition needs, which takes
        // no press, leaves the window its room: its tabs alone are the
        // bar's; a node of a key that takes none, none.
        let root = bar(vec![
            Node::container(vec![tab("tab-1", "one"), tab("tab-2", "two")]).key("tabs"),
            Node::container(vec![Node::text("⌥⌘1")]).key("keys"),
        ]);
        assert_eq!(areas_of(&root), tabs);
        // A button takes a press of its kind.
        let root = bar(vec![Node::icon_button("add").key("new")]);
        assert_eq!(areas_of(&root).len(), 1);
        // Within the clips around them: a tab past a box that clips, or
        // past a scroll area, has the part of it within the box.
        for clip in [
            Node::container(vec![tab("tab-1", "one"), tab("tab-2", "two")]).style(Style {
                clip: true,
                ..Style::default()
            }),
            Node::scroll(vec![tab("tab-1", "one"), tab("tab-2", "two")]).key("strip"),
        ] {
            let clip = clip.layout(NodeLayout {
                width: Length::Points(60.0),
                height: Length::Points(40.0),
                ..NodeLayout::default()
            });
            assert_eq!(
                areas_of(&bar(vec![clip])),
                [frame(0.0, 0.0, 40.0, 40.0), frame(40.0, 0.0, 20.0, 40.0)]
            );
        }
        // A tab wider than the bar, its part within it.
        let wide = Node::container(vec![tab("tab-1", "one").layout(NodeLayout {
            width: Length::Points(150.0),
            padding: Edges::all(5.0),
            ..NodeLayout::default()
        })])
        .layout(NodeLayout {
            direction: Direction::Column,
            width: Length::Points(100.0),
            ..NodeLayout::default()
        });
        assert_eq!(areas_of(&wide), [frame(0.0, 0.0, 100.0, 24.0)]);
        let layout = Layout::new(1.0);
        assert!(titlebar_areas(&Tree::new(), &layout).is_empty());
    }
}
