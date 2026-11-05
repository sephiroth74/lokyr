//! A window's chrome: the regions of its slots, where the views of the
//! plugins that provide them show, around and over its panes.
//!
//! The tab bar goes in the titlebar, past the window's buttons, under a
//! titlebar of the window's title, across the window's bottom, below the
//! status bar, or down a column at its left or its right, from below the
//! titlebar to the window's bottom; the window's strip goes across the
//! panes' top, over them, as it takes none of their rows; the status bar
//! goes across the bottom of the room that a column leaves, of its view's
//! height; a pane's header goes across the pane's top, of its view's
//! height, above its grid. The overlays and the popups are the rooms that
//! the floating panels anchor in. A slot whose view is empty takes no
//! room.

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
}

impl TabBarPlace {
    /// Whether the bar goes down a column, on a side.
    pub fn is_column(self) -> bool {
        matches!(self, Self::Left(_) | Self::Right(_))
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
}

/// The regions of a window's slots, in pixels from its content's top left
/// corner, the titlebar's too.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Regions {
    /// The tab bar's, if it has a view.
    pub tab_bar: Option<Frame>,
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
    let shows = heights.tab_bar > 0.0;
    let mut top = titlebar;
    let (mut left, mut right, mut bottom) = (0.0, width, height);
    // A bar across the top, a column on a side, or one across the bottom,
    // within the window.
    let tab_bar = shows.then(|| match place {
        TabBarPlace::Titlebar if titlebar > 0.0 => {
            let buttons = window.buttons.clamp(0.0, width);
            Frame {
                x: buttons,
                y: 0.0,
                width: width - buttons,
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

/// The parts of a tab bar's view in the titlebar that are its own, from
/// its corner: the boxes of the outermost nodes of a key under its root,
/// within the root's, where a press goes to the view. The rest of the bar,
/// its root's own box too, is the window's, which a drag there moves and a
/// double click zooms.
pub fn titlebar_areas(tree: &Tree, layout: &Layout) -> Vec<Frame> {
    let mut areas = Vec::new();
    let Some(root) = tree.root() else {
        return areas;
    };
    let Some(bounds) = layout.frame(tree, root) else {
        return areas;
    };
    for &child in tree.children(root) {
        keyed(tree, layout, child, bounds, &mut areas);
    }
    areas
}

/// Adds the box of `id` within `bounds` to `areas`, if it has a key;
/// otherwise its children's.
fn keyed(tree: &Tree, layout: &Layout, id: NodeId, bounds: Frame, areas: &mut Vec<Frame>) {
    if tree.key(id).is_none() {
        for &child in tree.children(id) {
            keyed(tree, layout, child, bounds, areas);
        }
        return;
    }
    let Some(frame) = layout.frame(tree, id) else {
        return;
    };
    let (left, top) = (frame.x.max(bounds.x), frame.y.max(bounds.y));
    let right = (frame.x + frame.width).min(bounds.x + bounds.width);
    let bottom = (frame.y + frame.height).min(bounds.y + bounds.height);
    if right > left && bottom > top {
        areas.push(Frame {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        });
    }
}

#[cfg(test)]
mod tests {
    use lokyr_plugin_sdk::view::{Direction, Edges, Layout as NodeLayout, Length, Node};

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
    fn a_tab_bar_in_the_titlebar_takes_it_past_the_buttons_and_the_panes_the_rest() {
        let heights = Heights {
            tab_bar: 30.0,
            window_strip: 2.0,
            status_bar: 22.0,
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

    #[test]
    fn the_titlebar_s_areas_are_the_outermost_nodes_of_a_key_under_the_root() {
        let tab = |key: &str, title: &str| {
            Node::container(vec![Node::text(title).key(format!("{key}.title").as_str())])
                .key(key)
                .layout(NodeLayout {
                    padding: Edges::all(5.0),
                    ..NodeLayout::default()
                })
        };
        let root = Node::container(vec![
            Node::container(vec![tab("tab-1", "one"), tab("tab-2", "two")]),
            Node::spacer(),
            Node::text("plain"),
        ])
        .key("bar")
        .layout(NodeLayout {
            width: Length::Points(200.0),
            height: Length::Points(40.0),
            ..NodeLayout::default()
        });
        let mut tree = Tree::new();
        let changes = tree.update(&root).unwrap();
        let mut layout = Layout::new(1.0);
        layout.update(&tree, &changes);
        layout.compute(&tree, None, None, &mut Fixed);
        // The root's own key gives it none, a text of no key none, a
        // tab's title none past its tab's.
        assert_eq!(
            titlebar_areas(&tree, &layout),
            [frame(0.0, 0.0, 40.0, 40.0), frame(40.0, 0.0, 40.0, 40.0)]
        );
        // A tab wider than the bar, its part within it.
        let wide = Node::container(vec![tab("tab-1", "one").layout(NodeLayout {
            width: Length::Points(150.0),
            ..NodeLayout::default()
        })])
        .layout(NodeLayout {
            direction: Direction::Column,
            width: Length::Points(100.0),
            ..NodeLayout::default()
        });
        let changes = tree.update(&wide).unwrap();
        layout.update(&tree, &changes);
        layout.compute(&tree, None, None, &mut Fixed);
        assert_eq!(
            titlebar_areas(&tree, &layout),
            [frame(0.0, 0.0, 100.0, 14.0)]
        );
        assert!(titlebar_areas(&Tree::new(), &layout).is_empty());
    }
}
