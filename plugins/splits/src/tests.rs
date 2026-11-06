//! The plugin against the SDK's fake host, which records what it does and
//! keeps the tabs' layouts as its writes change them.

use lokyr_plugin_sdk::fake::FakeHost;
use lokyr_plugin_sdk::host::{Layout, TabId};
use lokyr_plugin_sdk::message::{Effect, Request};

use super::*;
use crate::geometry::Towards;

/// The plugin's name, as its calls give it.
const NAME: &str = "lokyr/splits";

/// The tab of the tests.
const TAB: TabId = TabId(1);

/// A host of one window of one tab of pane 1, which has the keyboard, and
/// the plugin, activated.
fn host() -> (FakeHost, Splits) {
    let mut host = FakeHost::new(NAME);
    host.pane(PaneId(1)).tab = TAB;
    host.focus(Some(PaneId(1)));
    let mut plugin = Splits;
    host.activate(&mut plugin).unwrap();
    (host, plugin)
}

/// Runs `name` of `args`, and returns the writes that it made.
fn run(host: &mut FakeHost, plugin: &mut Splits, name: &str, args: Args) -> Vec<Request> {
    host.take_done();
    host.command(plugin, name, args).unwrap();
    host.take_done()
        .into_iter()
        .filter(|request| request.effect() == Effect::Write)
        .collect()
}

/// The arguments of a direction, and of `cells` cells, if given.
fn towards(direction: &str, cells: Option<i64>) -> Args {
    let mut args = Args::from([("direction".to_owned(), Value::String(direction.to_owned()))]);
    if let Some(cells) = cells {
        args.insert("cells".to_owned(), Value::Integer(cells));
    }
    args
}

/// The tab's layout, as the host keeps it.
fn layout(host: &mut FakeHost, _: &mut Splits) -> Layout {
    host.tab_layout(TAB).expect("the tab")
}

/// The pane that has the keyboard, as the host keeps it.
fn focused(host: &mut FakeHost, _: &mut Splits) -> PaneId {
    host.keyboard().expect("a pane with the keyboard")
}

/// A tab of three panes, 1 on the left, and 2 over 3 on the right, made by
/// the plugin's splits, 3 of the keyboard.
fn three() -> (FakeHost, Splits, [PaneId; 3]) {
    let (mut host, mut plugin) = host();
    run(&mut host, &mut plugin, SPLIT, towards("right", None));
    let second = focused(&mut host, &mut plugin);
    run(&mut host, &mut plugin, SPLIT, towards("down", None));
    let third = focused(&mut host, &mut plugin);
    (host, plugin, [PaneId(1), second, third])
}

#[test]
fn the_commands_register_with_their_parameters_and_their_titles() {
    let (host, _) = host();
    let registered: Vec<(&str, Vec<&str>)> = host
        .commands()
        .iter()
        .map(|command| {
            assert_eq!(command.category, Category::Panes);
            (
                command.name.as_str(),
                command
                    .params
                    .iter()
                    .map(|param| param.name.as_str())
                    .collect(),
            )
        })
        .collect();
    assert_eq!(
        registered,
        [
            (SPLIT, vec!["direction"]),
            (FOCUS, vec!["direction"]),
            (FOCUS_NEXT, vec![]),
            (FOCUS_PREV, vec![]),
            (RESIZE, vec!["direction", "cells"]),
            (ZOOM, vec![]),
            (EQUALIZE, vec![]),
            (SWAP, vec!["direction"]),
            (MOVE_TO_TAB, vec![]),
            (MOVE_TO_WINDOW, vec![]),
        ]
    );
    // Every title in both languages.
    for (_, catalog) in CATALOGS {
        for (name, ..) in COMMANDS {
            let message = format!("command-{} =", name.replace('.', "-"));
            assert!(catalog.contains(&message), "{message}");
        }
    }
}

#[test]
fn a_split_right_or_down_gives_the_new_pane_the_keyboard() {
    let (mut host, mut plugin, [first, second, third]) = three();
    assert_eq!(
        layout(&mut host, &mut plugin),
        Layout::Split {
            direction: Lie::Horizontal,
            ratio: None,
            first: Box::new(Layout::Pane(first)),
            second: Box::new(Layout::Split {
                direction: Lie::Vertical,
                ratio: None,
                first: Box::new(Layout::Pane(second)),
                second: Box::new(Layout::Pane(third)),
            }),
        }
    );
    assert_eq!(focused(&mut host, &mut plugin), third);
    // Left and up are no splits'.
    host.take_done();
    let error = host
        .command(&mut plugin, SPLIT, towards("left", None))
        .unwrap_err();
    assert_eq!(error.code, Code::Failed);
    // Of no direction, which the host's registry refuses first.
    let error = host.command(&mut plugin, SPLIT, Args::new()).unwrap_err();
    assert_eq!(error.code, Code::Failed);
}

#[test]
fn the_keyboard_goes_to_the_pane_beside_it_or_to_the_next_round_the_tab() {
    let (mut host, mut plugin, [first, second, third]) = three();
    // From 3, at the bottom right: up to 2, left to 1, none down or right.
    let mut goes = |host: &mut FakeHost, direction: &str| {
        run(host, &mut plugin, FOCUS, towards(direction, None))
    };
    assert_eq!(goes(&mut host, "down"), []);
    assert_eq!(goes(&mut host, "right"), []);
    assert_eq!(goes(&mut host, "up"), [Request::FocusPane(second)]);
    assert_eq!(goes(&mut host, "left"), [Request::FocusPane(first)]);
    // From 1, right to the one of the most room shared, the first of a tie.
    assert_eq!(goes(&mut host, "right"), [Request::FocusPane(second)]);
    // Next and previous, round.
    assert_eq!(
        run(&mut host, &mut plugin, FOCUS_NEXT, Args::new()),
        [Request::FocusPane(third)]
    );
    assert_eq!(
        run(&mut host, &mut plugin, FOCUS_NEXT, Args::new()),
        [Request::FocusPane(first)]
    );
    assert_eq!(
        run(&mut host, &mut plugin, FOCUS_PREV, Args::new()),
        [Request::FocusPane(third)]
    );
    // A pane alone goes nowhere.
    let (mut host, mut plugin) = self::host();
    assert_eq!(run(&mut host, &mut plugin, FOCUS_NEXT, Args::new()), []);
    assert_eq!(
        run(&mut host, &mut plugin, FOCUS, towards("left", None)),
        []
    );
}

#[test]
fn a_resize_moves_the_divider_by_cells_of_its_split_s_room() {
    let (mut host, mut plugin, [first, second, third]) = three();
    // 3 is 40 columns of the half on the right, a split of 80 columns: 4
    // cells right move the divider on its left, as it has none on its
    // right, a twentieth.
    for pane in [first, second, third] {
        host.pane(pane).grid = (40, 12);
    }
    assert_eq!(
        run(&mut host, &mut plugin, RESIZE, towards("right", Some(4))),
        [Request::ResizePane {
            pane: third,
            edge: Edge::Left,
            by: 0.05,
        }]
    );
    // Up: the divider over it, 12 rows of the right half's 24, one cell.
    assert_eq!(
        run(&mut host, &mut plugin, RESIZE, towards("up", None)),
        [Request::ResizePane {
            pane: third,
            edge: Edge::Top,
            by: -1.0 / 24.0,
        }]
    );
    // From 1, left: the divider on its right, which it has.
    run(&mut host, &mut plugin, FOCUS, towards("left", None));
    let moves = run(&mut host, &mut plugin, RESIZE, towards("left", Some(2)));
    assert_eq!(
        moves,
        [Request::ResizePane {
            pane: first,
            edge: Edge::Right,
            by: -0.025,
        }]
    );
    // Of no divider along it, nothing.
    assert_eq!(run(&mut host, &mut plugin, RESIZE, towards("up", None)), []);
}

#[test]
fn a_zoom_toggles_and_equalize_swap_and_moves_change_the_tab() {
    let (mut host, mut plugin, [first, second, third]) = three();
    assert_eq!(
        run(&mut host, &mut plugin, ZOOM, Args::new()),
        [Request::Zoom {
            tab: TAB,
            pane: Some(third),
        }]
    );
    // Zoomed, the keyboard goes nowhere by direction, and the zoom ends.
    assert_eq!(run(&mut host, &mut plugin, FOCUS, towards("up", None)), []);
    assert_eq!(
        run(&mut host, &mut plugin, ZOOM, Args::new()),
        [Request::Zoom {
            tab: TAB,
            pane: None,
        }]
    );
    assert_eq!(
        run(&mut host, &mut plugin, EQUALIZE, Args::new()),
        [Request::Equalize(TAB)]
    );
    assert_eq!(
        run(&mut host, &mut plugin, SWAP, towards("up", None)),
        [Request::SwapPanes {
            pane: third,
            with: second,
        }]
    );
    let rooms = crate::geometry::rects(&layout(&mut host, &mut plugin));
    assert_eq!(
        rooms.iter().map(|&(pane, _)| pane).collect::<Vec<_>>(),
        [first, third, second]
    );
    assert_eq!(
        crate::geometry::beside(&rooms, third, Towards::Down),
        Some(second)
    );
    let window = host.pane(third).window;
    assert_eq!(
        run(&mut host, &mut plugin, MOVE_TO_TAB, Args::new()),
        [Request::PaneToNewTab {
            pane: third,
            window,
            index: 1,
        }]
    );
    // A pane alone in its window's one tab moves nowhere.
    let (mut host, mut plugin) = self::host();
    assert_eq!(run(&mut host, &mut plugin, MOVE_TO_TAB, Args::new()), []);
    assert_eq!(run(&mut host, &mut plugin, MOVE_TO_WINDOW, Args::new()), []);
    assert_eq!(run(&mut host, &mut plugin, ZOOM, Args::new()), []);
}

#[test]
fn a_tab_s_rooms_follow_its_splits_ratios_and_their_panes() {
    // 1 on the left, then 2 over 3, then 4 beside them, by their panes'
    // shares; then a ratio of the user's.
    let leaf = |id| Box::new(Layout::Pane(PaneId(id)));
    let column = Layout::Split {
        direction: Lie::Vertical,
        ratio: None,
        first: leaf(2),
        second: leaf(3),
    };
    let layout = Layout::Split {
        direction: Lie::Horizontal,
        ratio: None,
        first: leaf(1),
        second: Box::new(Layout::Split {
            direction: Lie::Horizontal,
            ratio: None,
            first: Box::new(column),
            second: leaf(4),
        }),
    };
    let rooms = crate::geometry::rects(&layout);
    let widths: Vec<f64> = rooms.iter().map(|(_, room)| room.width).collect();
    assert!(
        widths
            .iter()
            .zip([1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0])
            .all(|(width, wanted)| (width - wanted).abs() < 1e-9),
        "{widths:?}"
    );
    assert_eq!(
        crate::geometry::beside(&rooms, PaneId(4), Towards::Left),
        Some(PaneId(2))
    );
    let width = crate::geometry::split_of(&layout, PaneId(3), Edge::Right).map(|room| room.width);
    assert!(
        width.is_some_and(|width| (width - 2.0 / 3.0).abs() < 1e-9),
        "{width:?}"
    );
    assert_eq!(
        crate::geometry::split_of(&layout, PaneId(4), Edge::Right),
        None
    );
}
