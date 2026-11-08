//! The plugin against the SDK's fake host, whose pane holds 100 rows of
//! scrollback over a screen of 24, its view on the screen.

use lokyr_plugin_sdk::fake::FakeHost;
use lokyr_plugin_sdk::host::LineText;
use lokyr_plugin_sdk::message::Request;

use super::*;

/// The plugin's name, as its calls give it.
const NAME: &str = "lokyr/copy-mode";

/// The pane of the tests, and another of its tab.
const PANE: PaneId = PaneId(1);
const OTHER: PaneId = PaneId(2);

/// The terminal's cursor, on the screen's eleventh row.
const CURSOR: (u16, i64) = (5, 110);

/// The view on the screen: rows 100 to 123, of the scrollback from row 0.
fn screen() -> Viewport {
    Viewport {
        top: 100,
        rows: 24,
        screen: 100,
        oldest: 0,
        epoch: 1,
        alternate: false,
    }
}

/// The logical line of `row`, of `text`, a cell a character, padded to 80
/// columns.
fn line(row: i64, text: &str) -> LineText {
    let text = format!("{text:80}");
    LineText {
        cells: (0..80).map(|column| (column, row)).collect(),
        text,
    }
}

/// A host of the pane, focused, of `view`, its cursor at [`CURSOR`], and
/// the plugin, activated.
fn host_of(view: Viewport) -> (FakeHost, CopyMode) {
    let mut host = FakeHost::new(NAME);
    let pane = host.pane(PANE);
    pane.held = 0..124;
    pane.viewport = Some(view);
    pane.cursor = CURSOR;
    for row in 0..124 {
        pane.lines.insert(row, line(row, ""));
    }
    host.focus(Some(PANE));
    for (id, text) in [
        ("badge-copy", "Copy"),
        ("badge-cells", "Visual"),
        ("badge-lines", "Visual Line"),
        ("badge-block", "Visual Block"),
    ] {
        host.set_message(id, &[], Some(text));
    }
    let mut plugin = CopyMode::default();
    host.activate(&mut plugin).unwrap();
    (host, plugin)
}

fn host() -> (FakeHost, CopyMode) {
    host_of(screen())
}

fn run(host: &mut FakeHost, plugin: &mut CopyMode, name: &str, arg: Option<(&str, Value)>) {
    let args = arg
        .map(|(name, value)| Args::from([(name.to_owned(), value)]))
        .unwrap_or_default();
    host.command(plugin, name, args).unwrap();
}

fn toggle(host: &mut FakeHost, plugin: &mut CopyMode) {
    run(host, plugin, TOGGLE, None);
}

fn go(host: &mut FakeHost, plugin: &mut CopyMode, to: &str) {
    run(
        host,
        plugin,
        MOVE,
        Some(("to", Value::String(to.to_owned()))),
    );
}

fn act(host: &mut FakeHost, plugin: &mut CopyMode, action: &str) {
    run(
        host,
        plugin,
        ACT,
        Some(("action", Value::String(action.to_owned()))),
    );
}

fn select(host: &mut FakeHost, plugin: &mut CopyMode, kind: &str) {
    run(
        host,
        plugin,
        SELECT,
        Some(("kind", Value::String(kind.to_owned()))),
    );
}

fn count(host: &mut FakeHost, plugin: &mut CopyMode, digit: i64) {
    run(host, plugin, COUNT, Some(("digit", Value::Integer(digit))));
}

/// The plugin's cursor that the pane shows, if one.
fn cursor(host: &mut FakeHost) -> Option<(u16, i64)> {
    host.pane(PANE).held_cursor
}

/// The badge's text, if it shows.
fn badge_text(host: &mut FakeHost, plugin: &mut CopyMode) -> Option<String> {
    host.panels().get(&badge())?;
    let tree = host.render(plugin, &ViewId::Panel(badge()))?;
    let [text] = &tree.children[..] else {
        panic!("{tree:?}");
    };
    match &text.kind {
        lokyr_plugin_sdk::view::Kind::Text(text) => Some(text.clone()),
        other => panic!("{other:?}"),
    }
}

#[test]
fn it_registers_its_commands_of_the_mode_s_context_but_the_toggle() {
    let (host, _) = host();
    let commands: Vec<(&str, Option<&str>, usize)> = host
        .commands()
        .iter()
        .map(|command| {
            (
                command.name.as_str(),
                command.when.as_deref(),
                command.params.len(),
            )
        })
        .collect();
    assert_eq!(
        commands,
        [
            (TOGGLE, None, 0),
            (MOVE, Some(WHEN), 1),
            (SELECT, Some(WHEN), 1),
            (COUNT, Some(WHEN), 1),
            (ACT, Some(WHEN), 1),
        ]
    );
}

#[test]
fn the_toggle_shows_a_cursor_at_the_terminal_s_and_a_badge_that_holds_the_keys() {
    let (mut host, mut plugin) = host();
    toggle(&mut host, &mut plugin);
    assert_eq!(cursor(&mut host), Some(CURSOR));
    let options = host.panels().get(&badge()).cloned().expect("the badge");
    assert_eq!(
        options,
        PanelOptions {
            anchor: Anchor::PaneCorner {
                pane: PANE,
                corner: Corner::BottomRight,
            },
            role: Role::Overlay,
            close_outside: false,
            close_escape: false,
            takes_keys: true,
        }
    );
    assert_eq!(badge_text(&mut host, &mut plugin).as_deref(), Some("Copy"));
    // Again, the terminal's cursor back, and the badge gone.
    toggle(&mut host, &mut plugin);
    assert_eq!(cursor(&mut host), None);
    assert!(host.panels().is_empty());
}

#[test]
fn a_view_up_in_the_scrollback_starts_at_its_last_row_and_the_alternate_screen_at_none() {
    let (mut host, mut plugin) = host_of(Viewport {
        top: 40,
        ..screen()
    });
    toggle(&mut host, &mut plugin);
    assert_eq!(cursor(&mut host), Some((0, 63)));
    let (mut host, mut plugin) = host_of(Viewport {
        alternate: true,
        ..screen()
    });
    toggle(&mut host, &mut plugin);
    assert_eq!(cursor(&mut host), None);
    assert!(host.panels().is_empty());
}

#[test]
fn the_arrows_move_the_cursor_by_counts_within_the_pane() {
    let (mut host, mut plugin) = host();
    toggle(&mut host, &mut plugin);
    go(&mut host, &mut plugin, "left");
    go(&mut host, &mut plugin, "up");
    assert_eq!(cursor(&mut host), Some((4, 109)));
    // A count of 12, which the badge shows until the move takes it.
    count(&mut host, &mut plugin, 1);
    count(&mut host, &mut plugin, 2);
    assert_eq!(
        badge_text(&mut host, &mut plugin).as_deref(),
        Some("Copy 12")
    );
    host.take_renders();
    go(&mut host, &mut plugin, "right");
    assert_eq!(cursor(&mut host), Some((16, 109)));
    // The move takes the count, which the badge renders without.
    assert!(host.take_renders().contains(&ViewId::Panel(badge())));
    assert_eq!(badge_text(&mut host, &mut plugin).as_deref(), Some("Copy"));
    // Within the columns and the screen's last row.
    count(&mut host, &mut plugin, 9);
    count(&mut host, &mut plugin, 9);
    go(&mut host, &mut plugin, "right");
    count(&mut host, &mut plugin, 9);
    count(&mut host, &mut plugin, 9);
    go(&mut host, &mut plugin, "down");
    assert_eq!(cursor(&mut host), Some((79, 123)));
    // A 0 of no count goes to the line's start.
    count(&mut host, &mut plugin, 0);
    assert_eq!(cursor(&mut host), Some((0, 123)));
}

#[test]
fn a_move_past_the_view_scrolls_it_to_the_cursor_s_row() {
    let (mut host, mut plugin) = host();
    toggle(&mut host, &mut plugin);
    host.take_done();
    count(&mut host, &mut plugin, 1);
    count(&mut host, &mut plugin, 5);
    go(&mut host, &mut plugin, "up");
    assert_eq!(cursor(&mut host), Some((5, 95)));
    let done = host.take_done();
    assert!(
        done.contains(&Request::Scroll {
            pane: PANE,
            scroll: Scroll::Row(95),
        }),
        "{done:?}"
    );
    // To the scrollback's top, and the view with it.
    go(&mut host, &mut plugin, "top");
    assert_eq!(cursor(&mut host), Some((0, 0)));
    assert!(host.take_done().contains(&Request::Scroll {
        pane: PANE,
        scroll: Scroll::Row(0),
    }));
}

#[test]
fn half_a_page_moves_the_cursor_and_the_view_together() {
    let (mut host, mut plugin) = host();
    toggle(&mut host, &mut plugin);
    host.take_done();
    go(&mut host, &mut plugin, "half-page-up");
    assert_eq!(cursor(&mut host), Some((5, 98)));
    let done = host.take_done();
    assert!(
        done.contains(&Request::Scroll {
            pane: PANE,
            scroll: Scroll::Row(88),
        }),
        "{done:?}"
    );
}

#[test]
fn the_view_s_rows_and_the_bottom_are_reached_at_once() {
    let (mut host, mut plugin) = host();
    host.pane(PANE).lines.insert(112, line(112, "output"));
    toggle(&mut host, &mut plugin);
    go(&mut host, &mut plugin, "view-top");
    assert_eq!(cursor(&mut host), Some((5, 100)));
    go(&mut host, &mut plugin, "view-bottom");
    assert_eq!(cursor(&mut host), Some((5, 123)));
    go(&mut host, &mut plugin, "view-middle");
    assert_eq!(cursor(&mut host), Some((5, 111)));
    // The last row that shows something, below the terminal's cursor.
    go(&mut host, &mut plugin, "bottom");
    assert_eq!(cursor(&mut host), Some((0, 112)));
}

#[test]
fn a_word_s_moves_read_the_lines() {
    let (mut host, mut plugin) = host();
    host.pane(PANE)
        .lines
        .insert(110, line(110, "$ cargo test --all"));
    toggle(&mut host, &mut plugin);
    count(&mut host, &mut plugin, 0);
    go(&mut host, &mut plugin, "word-next");
    assert_eq!(cursor(&mut host), Some((2, 110)));
    go(&mut host, &mut plugin, "word-end");
    assert_eq!(cursor(&mut host), Some((6, 110)));
    go(&mut host, &mut plugin, "line-end");
    assert_eq!(cursor(&mut host), Some((17, 110)));
    go(&mut host, &mut plugin, "word-previous");
    assert_eq!(cursor(&mut host), Some((15, 110)));
    go(&mut host, &mut plugin, "big-word-previous");
    assert_eq!(cursor(&mut host), Some((13, 110)));
    go(&mut host, &mut plugin, "line-first");
    assert_eq!(cursor(&mut host), Some((0, 110)));
}

#[test]
fn a_selection_of_lines_from_the_cursor_is_copied_and_stays() {
    let (mut host, mut plugin) = host();
    host.pane(PANE).selection = Some("line 1\nline 2".to_owned());
    toggle(&mut host, &mut plugin);
    host.take_done();
    select(&mut host, &mut plugin, "lines");
    assert_eq!(
        badge_text(&mut host, &mut plugin).as_deref(),
        Some("Visual Line")
    );
    go(&mut host, &mut plugin, "up");
    let done = host.take_done();
    assert!(
        done.contains(&Request::SelectCells {
            pane: PANE,
            first: CURSOR,
            last: (5, 109),
            kind: SelectionKind::Lines,
        }),
        "{done:?}"
    );
    act(&mut host, &mut plugin, "copy");
    assert_eq!(host.clipboard(), Some("line 1\nline 2"));
    // The mode ends, its selection left as a copy leaves it.
    assert_eq!(cursor(&mut host), None);
    assert!(host.panels().is_empty());
    assert!(!host.take_done().contains(&Request::Deselect(PANE)));
}

#[test]
fn a_copy_without_a_selection_takes_the_cursor_s_line() {
    let (mut host, mut plugin) = host();
    host.pane(PANE)
        .lines
        .insert(109, line(109, "  hello world"));
    toggle(&mut host, &mut plugin);
    go(&mut host, &mut plugin, "up");
    act(&mut host, &mut plugin, "copy");
    assert_eq!(host.clipboard(), Some("  hello world"));
    assert_eq!(cursor(&mut host), None);
}

#[test]
fn a_selection_s_kind_changes_its_own_takes_it_away_and_its_ends_swap() {
    let (mut host, mut plugin) = host();
    toggle(&mut host, &mut plugin);
    select(&mut host, &mut plugin, "cells");
    go(&mut host, &mut plugin, "right");
    select(&mut host, &mut plugin, "block");
    assert_eq!(
        badge_text(&mut host, &mut plugin).as_deref(),
        Some("Visual Block")
    );
    host.take_done();
    act(&mut host, &mut plugin, "swap");
    assert_eq!(cursor(&mut host), Some(CURSOR));
    assert!(host.take_done().contains(&Request::SelectCells {
        pane: PANE,
        first: (6, 110),
        last: CURSOR,
        kind: SelectionKind::Block,
    }));
    select(&mut host, &mut plugin, "block");
    assert!(host.take_done().contains(&Request::Deselect(PANE)));
    assert_eq!(badge_text(&mut host, &mut plugin).as_deref(), Some("Copy"));
}

#[test]
fn back_takes_the_count_then_the_selection_away_then_ends() {
    let (mut host, mut plugin) = host();
    toggle(&mut host, &mut plugin);
    select(&mut host, &mut plugin, "cells");
    count(&mut host, &mut plugin, 3);
    act(&mut host, &mut plugin, "back");
    assert_eq!(
        badge_text(&mut host, &mut plugin).as_deref(),
        Some("Visual")
    );
    host.take_done();
    act(&mut host, &mut plugin, "back");
    assert!(host.take_done().contains(&Request::Deselect(PANE)));
    assert!(cursor(&mut host).is_some());
    act(&mut host, &mut plugin, "back");
    assert_eq!(cursor(&mut host), None);
    assert!(host.panels().is_empty());
}

#[test]
fn exit_ends_and_takes_the_selection_away() {
    let (mut host, mut plugin) = host();
    toggle(&mut host, &mut plugin);
    select(&mut host, &mut plugin, "lines");
    host.take_done();
    act(&mut host, &mut plugin, "exit");
    assert!(host.take_done().contains(&Request::Deselect(PANE)));
    assert_eq!(cursor(&mut host), None);
}

#[test]
fn the_alternate_screen_a_resize_another_pane_s_keyboard_and_its_pane_s_close_end_it() {
    for event in [
        Event::ViewportChanged {
            pane: PANE,
            viewport: Viewport {
                alternate: true,
                ..screen()
            },
        },
        Event::ViewportChanged {
            pane: PANE,
            viewport: Viewport {
                epoch: 2,
                ..screen()
            },
        },
        Event::Workspace(WorkspaceEvent::PaneFocused {
            pane: OTHER,
            tab: TabId(1),
        }),
        Event::Workspace(WorkspaceEvent::PaneClosed {
            pane: PANE,
            tab: TabId(1),
        }),
    ] {
        let (mut host, mut plugin) = host();
        toggle(&mut host, &mut plugin);
        host.post(event.clone());
        host.deliver(&mut plugin).unwrap();
        assert_eq!(cursor(&mut host), None, "{event:?}");
        assert!(host.panels().is_empty(), "{event:?}");
    }
    // A scroll, and a pane of another tab that takes its keyboard, leave it.
    let (mut host, mut plugin) = host();
    toggle(&mut host, &mut plugin);
    host.post(Event::ViewportChanged {
        pane: PANE,
        viewport: Viewport {
            top: 50,
            ..screen()
        },
    });
    host.post(Event::Workspace(WorkspaceEvent::PaneFocused {
        pane: OTHER,
        tab: TabId(2),
    }));
    host.deliver(&mut plugin).unwrap();
    assert_eq!(cursor(&mut host), Some(CURSOR));
}

#[test]
fn a_click_on_the_badge_ends_it() {
    let (mut host, mut plugin) = host();
    toggle(&mut host, &mut plugin);
    host.ui_event(
        &mut plugin,
        &UiEvent {
            view: ViewId::Panel(badge()),
            key: BADGE_KEY.into(),
            kind: UiEventKind::Click,
        },
    )
    .unwrap();
    assert_eq!(cursor(&mut host), None);
}

#[test]
fn its_keys_are_not_its_commands_out_of_the_mode() {
    let (mut host, mut plugin) = host();
    go(&mut host, &mut plugin, "up");
    act(&mut host, &mut plugin, "copy");
    assert_eq!(cursor(&mut host), None);
    assert_eq!(host.clipboard(), None);
}

#[test]
fn the_host_s_close_of_the_badge_gives_the_cursor_back_and_a_toggle_starts_anew() {
    let (mut host, mut plugin) = host();
    toggle(&mut host, &mut plugin);
    // As the pane's tab goes to another window.
    host.close_panel(&badge());
    host.deliver(&mut plugin).unwrap();
    assert_eq!(cursor(&mut host), None);
    toggle(&mut host, &mut plugin);
    assert_eq!(cursor(&mut host), Some(CURSOR));
    assert!(host.panels().contains_key(&badge()));
}

#[test]
fn a_count_s_steps_stop_as_a_move_stays_where_it_is() {
    let mut steps = 0;
    let at = repeated(99_999, (3, 7), |at| {
        steps += 1;
        at
    });
    assert_eq!((at, steps), ((3, 7), 1));
    let at = repeated(5, (0, 0), |(column, row)| (column + 1, row));
    assert_eq!(at, (5, 0));
}
