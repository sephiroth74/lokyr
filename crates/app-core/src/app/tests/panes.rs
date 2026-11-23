//! The tests of a window's panes: the keyboard that a click gives, the keys
//! that the pane with it takes, and the ends that close a pane, then the
//! window with its last.

use lokyr_kernel::plugins::host::BroadcastTo;
use lokyr_kernel::workspace::{Direction, Environment, Side};

use super::*;

/// The text of every row of `pane`'s screen, a line each.
fn screen_of(app: &App, pane: PaneId) -> String {
    let snapshot = app.workspace.pane(pane).unwrap().snapshot();
    (0..snapshot.rows())
        .map(|row| {
            snapshot
                .line(row)
                .cells()
                .iter()
                .filter(|cell| cell.width() != lokyr_vt_core::page::Width::SpacerTail)
                .map(|cell| match cell.content() {
                    lokyr_vt_core::page::Content::Char(c)
                    | lokyr_vt_core::page::Content::Grapheme(c) => c,
                    _ => ' ',
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A program that tells it is ready, echoes a line after its `name`, and
/// ends after the next.
fn echoing(name: &str) -> Program {
    shell(&format!(
        "echo ready-{name}; read line; echo \"{name}:$line\"; read line"
    ))
}

/// The middle of `pane`'s room, in points, as `window`'s last frame laid it
/// out.
fn middle_of(fake: &Fake, app: &App, window: WindowId, pane: PaneId) -> Point {
    let scale = fake.window(window).unwrap().size.scale;
    let view = app.windows[&window]
        .views
        .iter()
        .find(|view| view.pane == pane)
        .copied()
        .expect("the pane's room");
    Point {
        x: (f64::from(view.rect.x) + f64::from(view.rect.width) / 2.0) / scale,
        y: (f64::from(view.rect.y) + f64::from(view.rect.height) / 2.0) / scale,
    }
}

/// A click in the middle of `pane`'s room.
fn click_in(fake: &mut Fake, app: &mut App, window: WindowId, pane: PaneId) {
    let at = middle_of(fake, app, window, pane);
    fake.send(app, window, click(true, at, Modifiers::NONE, 1));
    fake.send(app, window, click(false, at, Modifiers::NONE, 1));
}

/// `window` of two panes side by side, the first running `first`, the
/// second `second`, of the keyboard and the focus, once both showed
/// `ready-` and their names, `a` and `b`.
fn two_panes(first: Program, second: &Program) -> (Fake, App, WindowId, PaneId, PaneId) {
    two_panes_of(unpadded(), first, second, false)
}

/// [`two_panes`] of `config`, whose frames draw, of a surface that counts
/// them, while `drawn`.
fn two_panes_of(
    config: Config,
    first: Program,
    second: &Program,
    drawn: bool,
) -> (Fake, App, WindowId, PaneId, PaneId) {
    let mut fake = Fake::default();
    let mut app = if drawn {
        let drawing = Drawing::new(lokyr_text_core::FontConfig::default()).unwrap();
        App::new(Some(drawing), config, None).with_program(first)
    } else {
        app(config, first)
    };
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    if drawn {
        app.windows.get_mut(&window).unwrap().surface = Some(crate::fakes::Surface::default());
    }
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let a = app.focused(window).unwrap();
    let grid = app.pane(window).grid();
    let pane = app.spawn_pane(
        &fake,
        second,
        &Environment::default(),
        grid,
        &lokyr_vt_core::terminal::Colors::default(),
    );
    let b = app
        .workspace
        .split(a, Direction::Horizontal, pane, Side::Second)
        .unwrap();
    fake.send(&mut app, window, WindowEvent::Frame);
    // Both ready, and laid out side by side by a frame, each of its room's
    // grid.
    run_until(&mut fake, &mut app, |_, app| {
        screen_of(app, a).contains("ready-a")
            && screen_of(app, b).contains("ready-b")
            && app.windows[&window].views.len() == 2
            && grids_settled(app, window)
    });
    (fake, app, window, a, b)
}

#[test]
fn each_of_two_panes_takes_the_keyboard_its_keys_and_its_end() {
    let (mut fake, mut app, window, a, b) = two_panes(echoing("a"), &echoing("b"));
    // A click in the second pane gives it the keyboard, whose keys reach
    // its program alone.
    click_in(&mut fake, &mut app, window, b);
    assert_eq!(app.focused(window), Some(b));
    fake.send(&mut app, window, typed(KeyCode::KeyX, "x", false));
    fake.send(&mut app, window, typed(KeyCode::Enter, "\r", false));
    run_until(&mut fake, &mut app, |_, app| {
        screen_of(app, b).contains("b:x")
    });
    assert!(!screen_of(&app, a).contains('x'), "{}", screen_of(&app, a));
    // And back to the first.
    click_in(&mut fake, &mut app, window, a);
    assert_eq!(app.focused(window), Some(a));
    fake.send(&mut app, window, typed(KeyCode::KeyZ, "z", false));
    fake.send(&mut app, window, typed(KeyCode::Enter, "\r", false));
    run_until(&mut fake, &mut app, |_, app| {
        screen_of(app, a).contains("a:z")
    });
    assert!(!screen_of(&app, b).contains('z'));
    // The first program's end closes its pane, not the window, and the
    // keyboard goes to the other.
    fake.send(&mut app, window, typed(KeyCode::Enter, "\r", false));
    run_until(&mut fake, &mut app, |_, app| {
        app.workspace.pane(a).is_none()
    });
    assert_eq!(fake.windows().count(), 1);
    assert_eq!(app.focused(window), Some(b));
    // The last one's closes the window with its tab.
    fake.send(&mut app, window, typed(KeyCode::Enter, "\r", false));
    run_until(&mut fake, &mut app, |fake, _| fake.windows().count() == 0);
    assert!(app.workspace.pane(b).is_none());
}

#[test]
fn the_wheel_scrolls_the_pane_under_it_which_keeps_the_keyboard_where_it_was() {
    let (mut fake, mut app, window, a, b) = two_panes(
        shell("echo ready-a; exec sleep 10"),
        &shell("seq 1 200; echo ready-b; exec sleep 10"),
    );
    let offset = |app: &App, pane| app.workspace.pane(pane).unwrap().snapshot().offset();
    let at = middle_of(&fake, &app, window, b);
    fake.send(
        &mut app,
        window,
        WindowEvent::Scroll {
            delta: ScrollDelta::Lines { x: 0.0, y: 3.0 },
            phase: None,
            momentum: false,
            position: at,
            mods: Modifiers::NONE,
        },
    );
    run_until(&mut fake, &mut app, |_, app| offset(app, b) > 0);
    assert_eq!(offset(&app, a), 0);
    assert_eq!(app.focused(window), Some(a));
}

#[test]
fn a_trackpad_s_scroll_over_another_pane_ends_the_first_s_ease_where_it_went() {
    let mut config = unpadded();
    config.window.smooth_scroll = true;
    let (mut fake, mut app, window, a, b) = two_panes_of(
        config,
        shell("seq 1 200; echo ready-a; exec sleep 10"),
        &shell("seq 1 200; echo ready-b; exec sleep 10"),
        false,
    );
    let up = |app: &App, pane| {
        let snapshot = app.workspace.pane(pane).unwrap().snapshot();
        // Far fewer rows than an `f64` holds exactly.
        snapshot.offset() as f64 - f64::from(snapshot.fraction())
    };
    let swipe = |at: Point| WindowEvent::Scroll {
        delta: ScrollDelta::Points { x: 0.0, y: 16.0 },
        phase: Some(lokyr_platform_core::ScrollPhase::Moved),
        momentum: false,
        position: at,
        mods: Modifiers::NONE,
    };
    // Two rows up over the second pane, half of them in its first frame.
    let (in_a, in_b) = (
        middle_of(&fake, &app, window, a),
        middle_of(&fake, &app, window, b),
    );
    fake.send(&mut app, window, swipe(in_b));
    fake.send(&mut app, window, WindowEvent::Frame);
    assert!(up(&app, b) < 2.0);
    // Then over the first: the second's view takes the rest at once, as
    // the next frame shows, and the first's eases.
    let close = |rows: f64| (rows - 2.0).abs() < 1e-4;
    fake.send(&mut app, window, swipe(in_a));
    fake.send(&mut app, window, WindowEvent::Frame);
    assert!(close(up(&app, b)), "{}", up(&app, b));
    assert!(up(&app, a) < 2.0);
    run_until(&mut fake, &mut app, |fake, _| {
        !fake.window(window).unwrap().frame_requested
    });
    assert!(close(up(&app, a)), "{}", up(&app, a));
    assert!(close(up(&app, b)), "{}", up(&app, b));
}

#[test]
fn an_accessibility_client_s_focus_of_a_pane_gives_it_the_keyboard() {
    let (mut fake, mut app, window, a, b) = two_panes(echoing("a"), &echoing("b"));
    click_in(&mut fake, &mut app, window, b);
    assert_eq!(app.focused(window), Some(b));
    fake.ask_accessibility(&mut app, window);
    let node = |pane: PaneId| lokyr_ui_core::access::pane_id(u32::try_from(pane.get()).unwrap());
    let request = accesskit::ActionRequest {
        action: accesskit::Action::Focus,
        target_tree: accesskit::TreeId::ROOT,
        target_node: node(a),
        data: None,
    };
    fake.send(&mut app, window, WindowEvent::AccessibilityAction(request));
    assert_eq!(app.focused(window), Some(a));
    // The tree of the two, each a terminal, the keyboard's the first's.
    let trees = &fake.window(window).unwrap().accessibility;
    let first = &trees[0];
    let terminals: Vec<_> = first
        .nodes
        .iter()
        .filter(|(_, node)| node.role() == accesskit::Role::Terminal)
        .map(|(id, _)| *id)
        .collect();
    assert_eq!(terminals.len(), 2);
    assert!(terminals.contains(&node(a)) && terminals.contains(&node(b)));
    assert_eq!(first.focus, node(b));
    // Each of its place, side by side, its characters a cell wide.
    let of = |id| &first.nodes.iter().find(|(at, _)| *at == id).unwrap().1;
    let (left, right) = (of(node(a)), of(node(b)));
    assert_eq!(left.label(), Some("Terminal"));
    let (left_bounds, right_bounds) = (left.bounds().unwrap(), right.bounds().unwrap());
    assert!(
        left_bounds.x1 <= right_bounds.x0 || left_bounds.y1 <= right_bounds.y0,
        "{left_bounds:?} {right_bounds:?}"
    );
    assert!(left_bounds.x0 < left_bounds.x1 && right_bounds.x0 < right_bounds.x1);
    let widths = of(right.children()[0]).character_widths().unwrap().to_vec();
    let cell = f64::from(widths[0]);
    assert!(cell > 1.0, "{widths:?}");
    assert_eq!(
        right_bounds.x1 - right_bounds.x0,
        cell * f64::from(app.workspace.pane(b).unwrap().snapshot().cols())
    );
    // The keyboard's move to the first goes in the next tree.
    run_until(&mut fake, &mut app, |fake, _| {
        fake.window(window)
            .unwrap()
            .accessibility
            .last()
            .is_some_and(|tree| tree.focus == node(a))
    });
    // The output of the pane without the keyboard goes too.
    let runs = |tree: &accesskit::TreeUpdate| {
        tree.nodes
            .iter()
            .filter_map(|(_, node)| node.value())
            .any(|value| value.starts_with("b:later"))
    };
    app.workspace.pane(b).unwrap().send_text("later\r");
    run_until(&mut fake, &mut app, |fake, _| {
        fake.window(window).unwrap().accessibility.iter().any(runs)
    });
}

/// A plugin whose commands open a tab and activate it, go back to the
/// first, split the pane that has the keyboard and focus the new one, and
/// give the keyboard back to the first pane, as `tabs` and `splits` will,
/// each of one call.
struct Mover;

impl plugins::Plugin for Mover {
    fn activate(&mut self, cx: &mut plugins::Cx<'_>) -> Result<(), Error> {
        for name in ["mover.tab", "mover.back", "mover.split", "mover.first"] {
            cx.register_command(Command {
                name: name.to_owned(),
                title: name.to_owned(),
                category: Category::Windows,
                capabilities: Vec::new(),
                params: Vec::new(),
                when: None,
            })?;
        }
        Ok(())
    }

    fn on_command(&mut self, cx: &mut plugins::Cx<'_>, name: &str, _: Args) -> Result<(), Error> {
        use plugins::host::{Direction, Side};

        let window = cx.focused_window().unwrap();
        let tabs = cx.tabs(window)?;
        match name {
            "mover.tab" => {
                let (tab, _) = cx.new_tab(window, None)?;
                cx.activate_tab(tab)
            }
            "mover.back" => cx.activate_tab(tabs[0].id),
            "mover.split" => {
                let pane = cx.focused_pane().unwrap();
                let new = cx.split(pane, Direction::Horizontal, Side::Second)?;
                cx.focus_pane(new)
            }
            _ => {
                let first = tabs.iter().find(|tab| tab.active).unwrap().panes[0];
                cx.focus_pane(first)
            }
        }
    }
}

#[test]
fn a_plugin_s_moves_of_the_keyboard_do_what_the_app_s_own_do() {
    let mut fake = Fake::default();
    let mut app = app(Config::default(), idle()).with_plugin("mover", Box::new(Mover));
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, WindowEvent::Focused(true));
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
    let spot = crate::mouse::Spot {
        row: 0,
        col: 0,
        boundary: 0,
    };
    for (command, moved) in [
        ("mover.tab", true),
        ("mover.back", true),
        ("mover.split", true),
        ("mover.first", true),
        ("mover.first", false),
    ] {
        // The pane that had the keyboard ran vim, composed and selected.
        let state = app.windows.get_mut(&window).unwrap();
        let before = state.keyboard;
        state.place.1 = "vim".to_owned();
        state.preedit = Some(lokyr_ui_core::pane::Preedit {
            text: "に".to_owned(),
            cursor: 0,
        });
        state.selecting = Some(crate::window::Selecting {
            anchor: spot,
            last: spot,
            shape: lokyr_vt_core::terminal::SelectionShape::Cells,
            unit: lokyr_vt_core::terminal::SelectionUnit::Cell,
            held: true,
            beyond: 0,
            plain: false,
        });
        let ended = fake.window(window).unwrap().compositions_ended;
        fake.send_app(&mut app, AppEvent::Command(command.to_owned()));
        let state = &app.windows[&window];
        assert_eq!(state.keyboard != before, moved, "{command}");
        assert_eq!(
            state.keyboard,
            app.workspace.focused(state.window).map(Pane::id)
        );
        // The new pane's place, its composition and its selection ended,
        // the input method forgetting it.
        assert_eq!(state.place.1 != "vim", moved, "{command}");
        assert_eq!(state.preedit.is_none(), moved, "{command}");
        assert_eq!(state.selecting.is_none(), moved, "{command}");
        // The title takes the new pane's program.
        let title = &fake.window(window).unwrap().title;
        assert!(!moved || !title.contains("vim"), "{command}: {title}");
        assert_eq!(
            fake.window(window).unwrap().compositions_ended,
            ended + usize::from(moved),
            "{command}"
        );
    }
}

/// A program that asks for the focus's reports, sets its title and its
/// tab's color, red for `a` and blue for the others, tells it is ready
/// after its `name`, and shows what it reads.
fn reporting(name: &str) -> Program {
    let color = if name == "a" { "red" } else { "blue" };
    shell(&format!(
        "stty raw -echo; printf '\\033[?1004h\\033]2;title-{name}\\007\\033]6;1;bg;{color};brightness;255\\007'; echo ready-{name}; cat -v"
    ))
}

#[test]
fn the_keyboard_s_move_tells_the_programs_and_takes_the_pane_s_title_and_color() {
    let (mut fake, mut app, window, a, b) = two_panes(reporting("a"), &reporting("b"));
    // The programs ready, and what the window's focus told them echoed,
    // which comes some milliseconds after.
    run_until(&mut fake, &mut app, |_, app| {
        screen_of(app, a).contains("ready-a") && screen_of(app, b).contains("ready-b")
    });
    for _ in 0..20 {
        fake.deliver_wake(&mut app);
        fake.send(&mut app, window, WindowEvent::Frame);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    // The reports before the test's moves: the first pane's of the
    // window's focus, if its program asked for them before the window
    // took it.
    let before = |pane| screen_of(&app, pane).matches("^[[").count();
    let before = (before(a), before(b));
    let title = |fake: &Fake| fake.window(window).unwrap().title.clone();
    let tab = |app: &App| {
        let shown = app.windows[&window].window;
        let tab = app.workspace.window(shown).unwrap().active_tab();
        let tab = app.workspace.tab(tab).unwrap();
        (tab.title().to_owned(), tab.color())
    };
    let red = Some(lokyr_vt_core::page::Rgb::new(255, 0, 0));
    let blue = Some(lokyr_vt_core::page::Rgb::new(0, 0, 255));
    run_until(&mut fake, &mut app, |fake, app| {
        title(fake).starts_with("title-a") && tab(app).1 == red
    });
    // To the second: the first learns that it lost the focus, and then
    // the second that it took it; the tab takes the second's title and
    // color, which its program set while the first had the keyboard.
    click_in(&mut fake, &mut app, window, b);
    run_until(&mut fake, &mut app, |_, app| {
        screen_of(app, a).contains("^[[O") && screen_of(app, b).contains("^[[I")
    });
    assert_eq!(tab(&app), ("title-b".to_owned(), blue));
    assert!(title(&fake).starts_with("title-b"), "{}", title(&fake));
    // And back: the first's title and color, kept.
    click_in(&mut fake, &mut app, window, a);
    run_until(&mut fake, &mut app, |_, app| {
        screen_of(app, a).contains("^[[O^[[I") && screen_of(app, b).contains("^[[I^[[O")
    });
    assert_eq!(tab(&app), ("title-a".to_owned(), red));
    assert!(title(&fake).starts_with("title-a"), "{}", title(&fake));
    // Without the window's focus, a move tells nothing.
    fake.send(&mut app, window, WindowEvent::Focused(false));
    app.focus_pane(&mut fake, window, b);
    fake.send(&mut app, window, WindowEvent::Focused(true));
    run_until(&mut fake, &mut app, |_, app| {
        screen_of(app, a).contains("^[[O^[[I^[[O") && screen_of(app, b).contains("^[[I^[[O^[[I")
    });
    let reports = |pane| screen_of(&app, pane).matches("^[[").count();
    assert_eq!((reports(a), reports(b)), (before.0 + 3, before.1 + 3));
    assert_eq!(tab(&app), ("title-b".to_owned(), blue));
    // A pane that closes tells nothing, and the other takes its title.
    app.close_pane(&mut fake, window, b);
    run_until(&mut fake, &mut app, |_, app| {
        screen_of(app, a).contains("^[[O^[[I^[[O^[[I")
    });
    assert_eq!(tab(&app), ("title-a".to_owned(), red));
}

#[test]
fn a_change_of_the_appearance_gives_every_pane_its_colors() {
    let (mut fake, mut app, window, a, b) = two_panes(echoing("a"), &echoing("b"));
    fake.send_app(&mut app, AppEvent::AppearanceChanged(Appearance::Dark));
    let dark = app.colors(Appearance::Dark);
    assert_ne!(dark, app.colors(Appearance::Light));
    run_until(&mut fake, &mut app, |_, app| {
        [a, b]
            .iter()
            .all(|&pane| app.workspace.pane(pane).unwrap().snapshot().colors() == &dark)
    });
    let _ = window;
}

/// A program that tracks the mouse's buttons and drags, in SGR's reports,
/// tells it is ready after its `name`, and shows what it reads.
fn tracking(name: &str) -> Program {
    shell(&format!(
        "stty raw -echo; printf '\\033[?1002h\\033[?1006h'; echo ready-{name}; cat -v"
    ))
}

#[test]
fn a_program_s_drag_stays_in_its_pane_across_the_divider() {
    let (mut fake, mut app, window, a, b) = two_panes(tracking("a"), &tracking("b"));
    let (from, to) = (
        middle_of(&fake, &app, window, a),
        middle_of(&fake, &app, window, b),
    );
    let none = Modifiers::NONE;
    fake.send(&mut app, window, click(true, from, none, 1));
    fake.send(&mut app, window, moved(to, none));
    // A second button over the other pane goes to the first too, which
    // keeps the keyboard.
    fake.send(&mut app, window, press(MouseButton::Right, true, to, none));
    fake.send(&mut app, window, press(MouseButton::Right, false, to, none));
    fake.send(&mut app, window, click(false, to, none, 1));
    assert_eq!(app.focused(window), Some(a));
    // The moves at the first pane's last column, past its right edge; the
    // release there.
    let columns = app.workspace.pane(a).unwrap().grid().columns;
    let row = app.windows[&window].views[0].grid.rows / 2 + 1;
    let at_edge = format!("^[[<32;{columns};{row}M");
    run_until(&mut fake, &mut app, |_, app| {
        screen_of(app, a).contains(&format!("^[[<0;{columns};{row}m"))
    });
    let first = screen_of(&app, a);
    assert!(first.contains("^[[<0;"), "{first}");
    assert!(first.contains(&at_edge), "{first}");
    assert!(
        first.contains(&format!("^[[<2;{columns};{row}M")),
        "{first}"
    );
    let pause = Instant::now() + Duration::from_millis(200);
    run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
    assert!(
        !screen_of(&app, b).contains("^[[<"),
        "{}",
        screen_of(&app, b)
    );
    // Released, a press over the second goes to the second.
    fake.send(&mut app, window, click(true, to, none, 1));
    fake.send(&mut app, window, click(false, to, none, 1));
    run_until(&mut fake, &mut app, |_, app| {
        screen_of(app, b).contains("^[[<0;")
    });
    assert_eq!(app.focused(window), Some(b));
}

/// The middle of the first divider of `window`'s tab, in points, and the
/// share of its split's first part.
fn divider_middle(fake: &Fake, app: &App, window: WindowId) -> Point {
    let scale = fake.window(window).unwrap().size.scale;
    let line = app.windows[&window].dividers[0].line;
    Point {
        x: (f64::from(line.x) + f64::from(line.width) / 2.0) / scale,
        y: (f64::from(line.y) + f64::from(line.height) / 2.0) / scale,
    }
}

/// The share of the first part of `window`'s tab's top split.
fn top_share(app: &App, window: WindowId) -> f64 {
    let shown = app.workspace.window(app.windows[&window].window).unwrap();
    let tab = app.workspace.tab(shown.active_tab()).unwrap();
    tab.layout().share().unwrap()
}

#[test]
fn a_divider_draws_between_two_panes_and_its_grab_area_is_wider_than_its_line() {
    let (fake, app, window, a, b) = two_panes(echoing("a"), &echoing("b"));
    let state = &app.windows[&window];
    let [divider] = &state.dividers[..] else {
        panic!("one divider: {:?}", state.dividers);
    };
    assert_eq!(divider.direction, Direction::Horizontal);
    assert!(divider.path.is_empty());
    // Between the two panes' rooms, a point wide.
    let scale = fake.window(window).unwrap().size.scale;
    let room = |pane| {
        state
            .views
            .iter()
            .find(|view| view.pane == pane)
            .unwrap()
            .rect
    };
    let (left, right) = (room(a), room(b));
    assert_eq!(divider.line.x, left.x + left.width as i32);
    assert_eq!(divider.line.x + divider.line.width as i32, right.x);
    assert_eq!(f64::from(divider.line.width), scale);
    // Its grab area, six points across, of a column's resize.
    let area = crate::app::dividers::grab_area(divider, scale);
    assert_eq!(area.shape, lokyr_platform_core::PointerShape::ResizeColumn);
    assert_eq!(area.size.width, 6.0);
    assert_eq!(area.size.height, f64::from(divider.line.height) / scale);
    let middle = (f64::from(divider.line.x) + f64::from(divider.line.width) / 2.0) / scale;
    assert_eq!(area.origin.x + area.size.width / 2.0, middle);
}

#[test]
fn a_divider_s_drag_moves_its_split_and_a_double_click_shares_it_in_half() {
    use lokyr_platform_core::PointerShape;

    let none = Modifiers::NONE;
    let (mut fake, mut app, window, a, _) = two_panes(echoing("a"), &echoing("b"));
    let start = divider_middle(&fake, &app, window);
    let before = top_share(&app, window);
    let held = |fake: &Fake| fake.window(window).unwrap().held_pointer;
    // A point beside the line, within its grab area.
    let grabbed = Point {
        x: start.x + 2.0,
        ..start
    };
    fake.send(&mut app, window, click(true, grabbed, none, 1));
    assert_eq!(held(&fake), Some(PointerShape::ResizeColumn));
    let further = Point {
        x: grabbed.x + 100.0,
        ..grabbed
    };
    fake.send(&mut app, window, moved(further, none));
    fake.send(&mut app, window, click(false, further, none, 1));
    assert_eq!(held(&fake), None);
    let after = top_share(&app, window);
    assert!(after > before + 0.05, "{before} {after}");
    // The press reached neither pane's program.
    assert_eq!(app.focused(window), Some(a));
    // Laid out anew, the first pane wider.
    let scale = fake.window(window).unwrap().size.scale;
    let from = f64::from(app.windows[&window].dividers[0].line.x);
    run_until(&mut fake, &mut app, |_, app| {
        f64::from(app.windows[&window].dividers[0].line.x) > from + 50.0 * scale
    });
    // A double click shares it half and half.
    let middle = divider_middle(&fake, &app, window);
    fake.send(&mut app, window, click(true, middle, none, 2));
    fake.send(&mut app, window, click(false, middle, none, 2));
    assert!((top_share(&app, window) - 0.5).abs() < 1e-9);
}

#[test]
fn a_divider_s_drag_resizes_the_programs_at_the_resizes_pace() {
    let none = Modifiers::NONE;
    let counting = shell("trap 'echo winch' WINCH; echo ready-a; while :; do sleep 0.01; done");
    let (mut fake, mut app, window, a, _) = two_panes(counting, &echoing("b"));
    let start = divider_middle(&fake, &app, window);
    let winches = |app: &App| screen_of(app, a).matches("winch").count();
    let before = winches(&app);
    fake.send(&mut app, window, click(true, start, none, 1));
    // Forty steps of a cell's width, each with its frame, at once.
    let began = std::time::Instant::now();
    let mut at = start;
    for _ in 0..40 {
        at.x += 8.0;
        fake.send(&mut app, window, moved(at, none));
        fake.send(&mut app, window, WindowEvent::Frame);
    }
    fake.send(&mut app, window, click(false, at, none, 1));
    // A signal a pace at most while the steps came, as a slow machine
    // takes longer over them, and the settled grid's.
    let paces = began.elapsed().as_millis() / lokyr_kernel::workspace::RESIZE_PACE.as_millis();
    let most = usize::try_from(paces)
        .unwrap_or(usize::MAX)
        .saturating_add(2)
        .max(4);
    // Settled: the program's grid is the layout's, of a few signals, not
    // of one a step.
    run_until(&mut fake, &mut app, |_, app| {
        let pane = app.workspace.pane(a).unwrap();
        pane.told() == pane.grid() && winches(app) > before
    });
    std::thread::sleep(std::time::Duration::from_millis(100));
    fake.deliver_wake(&mut app);
    let told = winches(&app) - before;
    assert!((1..=most).contains(&told), "{told} signals, {most} at most");
}

#[test]
fn the_panes_without_the_keyboard_fade_toward_their_background() {
    // The lightest of the colors of the text of the pane without the
    // keyboard, `ready-`'s, of the default foreground, and its background,
    // of `config`; and whether a rectangle covers a pane.
    let text_of = |config: Config| {
        let (mut fake, mut app, window, a, b) =
            two_panes_of(config, echoing("a"), &echoing("b"), true);
        let focused = app.focused(window).unwrap();
        let other = if focused == a { b } else { a };
        fake.send(&mut app, window, WindowEvent::Frame);
        let state = &app.windows[&window];
        let room = |pane: PaneId| {
            state
                .views
                .iter()
                .find(|view| view.pane == pane)
                .map(|view| view.rect)
                .unwrap()
        };
        let items = &state.shown.as_ref().expect("a frame").items;
        let color = items
            .iter()
            .rev()
            .find_map(|item| match item {
                Item::Cells(batch) if (batch.x, batch.y) == (room(other).x, room(other).y) => batch
                    .glyphs
                    .iter()
                    .map(|glyph| glyph.color)
                    .max_by(|a, b| a.red.total_cmp(&b.red)),
                _ => None,
            })
            .expect("the pane's text");
        let background =
            lokyr_ui_core::pane::background(app.workspace.pane(other).unwrap().snapshot());
        let veiled = items.iter().any(|item| {
            matches!(item, Item::Rect { rect, .. } if *rect == room(other) || *rect == room(focused))
        });
        (color, background, veiled)
    };
    let mut plain = Config::parse(
        "[plugins.splits]\ndim-inactive = 0\n",
        std::path::Path::new("config.toml"),
    )
    .unwrap();
    plain.window = unpadded().window;
    let (shown, _, _) = text_of(plain);
    let (faded, background, veiled) = text_of(unpadded());
    // Of the splits' default fade, toward the background, of the same
    // opacity, and no rectangle over it.
    let toward = |of: f32, to: f32| of + (to - of) * 0.15;
    assert!(
        (faded.red - toward(shown.red, background.red)).abs() < 1e-5,
        "{faded:?} {shown:?}"
    );
    assert!(
        (faded.blue - toward(shown.blue, background.blue)).abs() < 1e-5,
        "{faded:?}"
    );
    assert_eq!(faded.alpha, shown.alpha);
    assert!(!veiled);
}

#[test]
fn the_keyboard_follows_the_pointer_over_the_panes_as_the_config_asks() {
    let none = Modifiers::NONE;
    // Not by default: a move over the other pane leaves the keyboard.
    let (mut fake, mut app, window, a, b) = two_panes(echoing("a"), &echoing("b"));
    let focused = app.focused(window).unwrap();
    let other = if focused == a { b } else { a };
    let over = middle_of(&fake, &app, window, other);
    fake.send(&mut app, window, moved(over, none));
    assert_eq!(app.focused(window), Some(focused));
    // Of the config's: the other pane's room wakes the app, the move there
    // gives it the keyboard, and then it alone wakes it.
    let mut config = Config::parse(
        "[plugins.splits]\nfocus-follows-mouse = true\n",
        std::path::Path::new("config.toml"),
    )
    .unwrap();
    let flat = unpadded();
    config.window = flat.window;
    let (mut fake, mut app, window, a, b) = two_panes_of(config, echoing("a"), &echoing("b"), true);
    let focused = app.focused(window).unwrap();
    let other = if focused == a { b } else { a };
    fake.send(&mut app, window, WindowEvent::Frame);
    let scale = fake.window(window).unwrap().size.scale;
    let origin = |app: &App, pane: PaneId| {
        let rect = app.windows[&window]
            .views
            .iter()
            .find(|view| view.pane == pane)
            .unwrap()
            .rect;
        Point {
            x: f64::from(rect.x) / scale,
            y: f64::from(rect.y) / scale,
        }
    };
    let moving = |fake: &Fake| -> Vec<Point> {
        fake.window(window)
            .unwrap()
            .move_areas
            .iter()
            .map(|area| area.origin)
            .collect()
    };
    assert!(moving(&fake).contains(&origin(&app, other)));
    assert!(!moving(&fake).contains(&origin(&app, focused)));
    let over = middle_of(&fake, &app, window, other);
    fake.send(&mut app, window, moved(over, none));
    assert_eq!(app.focused(window), Some(other));
    fake.send(&mut app, window, WindowEvent::Frame);
    assert!(moving(&fake).contains(&origin(&app, focused)));
    assert!(!moving(&fake).contains(&origin(&app, other)));
}

#[test]
fn a_tab_runs_the_program_of_a_pane_that_has_not_the_keyboard() {
    // The first pane's shell marks a command and starts it 0.7 s later, of
    // no output, as the keyboard moves to the second pane, a shell: the
    // looks after the mark find the first's program, and the tab runs it.
    let mut fake = Fake::default();
    let mut app = app(
        unpadded(),
        shell(concat!(
            "cd /usr; stty raw -echo; printf ready-a; ",
            "printf '\\033]133;A\\007$ \\033]133;B\\007\\033]133;C\\007'; ",
            "sleep 0.7; exec cat",
        )),
    );
    app.title_look = Some(TITLE_LOOK);
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let a = app.focused(window).unwrap();
    let grid = app.pane(window).grid();
    let pane = app.spawn_pane(
        &fake,
        &shell("printf ready-b; exec sh"),
        &Environment::default(),
        grid,
        &lokyr_vt_core::terminal::Colors::default(),
    );
    let b = app
        .workspace
        .split(a, Direction::Horizontal, pane, Side::Second)
        .unwrap();
    app.workspace.focus_pane(b);
    fake.send(&mut app, window, WindowEvent::Frame);
    assert_eq!(app.focused(window), Some(b));
    let tab = app.workspace.tab_of(a).unwrap();
    run_until(&mut fake, &mut app, |_, app| {
        app.workspace.pane(a).is_some_and(Pane::runs_program)
    });
    assert_eq!(app.focused(window), Some(b), "the keyboard stayed");
    assert!(app.workspace.tab(tab).unwrap().marks().running);
    assert!(!app.workspace.pane(b).unwrap().runs_program());
}

#[test]
fn a_pane_whose_agent_waits_is_ringed_in_a_tab_of_several() {
    let config = Config::parse(
        "[agents]\nprograms = [\"perl\"]\n",
        std::path::Path::new("config.toml"),
    )
    .unwrap();
    let agent = shell(concat!(
        "exec perl -e '$| = 1; print \"ready-b\\n\"; select(undef, undef, undef, 0.5); ",
        "print \"\\e]9;need you\\a\"; sleep 30'",
    ));
    let (mut fake, mut app, window, a, b) = two_panes_of(config, echoing("a"), &agent, true);
    let deadline = Instant::now() + Duration::from_secs(10);
    while app
        .workspace
        .pane(b)
        .and_then(Pane::agent)
        .and_then(lokyr_kernel::workspace::Agent::state)
        != Some(lokyr_kernel::plugins::host::AgentState::Waiting)
    {
        assert!(Instant::now() < deadline, "no agent waits");
        app.collect(&mut fake, &[b]);
        std::thread::sleep(Duration::from_millis(10));
    }
    // Whether a pane's room has a thin side at its top.
    let ringed = |app: &App, pane: PaneId| {
        let state = &app.windows[&window];
        let room = state
            .views
            .iter()
            .find(|view| view.pane == pane)
            .map(|view| view.rect)
            .unwrap();
        state
            .shown
            .as_ref()
            .expect("a frame")
            .items
            .iter()
            .any(|item| {
                matches!(item, Item::Rect { rect, .. }
                if (rect.x, rect.y, rect.width) == (room.x, room.y, room.width)
                    && (1..=8).contains(&rect.height))
            })
    };
    fake.send(&mut app, window, WindowEvent::Frame);
    assert!(ringed(&app, b));
    assert!(!ringed(&app, a));
    // Answered: the ring goes.
    app.workspace.focus_pane(b);
    fake.send(&mut app, window, key());
    fake.send(&mut app, window, WindowEvent::Frame);
    assert!(!ringed(&app, b));
}

#[test]
fn a_program_s_inline_image_draws_over_its_cells_backgrounds_and_under_its_text_within_its_pane() {
    // A PNG of 20 by 40 pixels, as OSC 1337 sends it, then the line that
    // the panes wait for.
    let image = shell(concat!(
        "printf '\\033]1337;File=inline=1:iVBORw0KGgoAAAANSUhEUgAAABQAAAAoCAYAAAA=\\007'; ",
        "echo; echo ready-a; read line",
    ));
    let (mut fake, mut app, window, a, _) =
        two_panes_of(Config::default(), image, &echoing("b"), true);
    fake.send(&mut app, window, WindowEvent::Frame);
    let state = &app.windows[&window];
    let room = state
        .views
        .iter()
        .find(|view| view.pane == a)
        .map(|view| view.rect)
        .unwrap();
    let items = &state.shown.as_ref().expect("a frame").items;
    let picture = items
        .iter()
        .position(|item| matches!(item, Item::Picture { .. }))
        .expect("the image's picture");
    let backgrounds = items
        .iter()
        .position(|item| matches!(item, Item::Cells(_)))
        .expect("the cells");
    let text = picture
        + items[picture..]
            .iter()
            .position(|item| matches!(item, Item::Cells(_)))
            .expect("the text");
    // Over the pane's backgrounds alone and under its text, of no
    // background, within a clip of its pane, at its grid's top left.
    assert!(backgrounds < picture, "{items:?}");
    let (Item::Cells(under), Item::Cells(over)) = (&items[backgrounds], &items[text]) else {
        unreachable!()
    };
    assert!(under.glyphs.is_empty() && !under.backgrounds.is_empty());
    assert!(over.backgrounds.is_empty() && !over.glyphs.is_empty());
    assert!(
        matches!(items[picture - 1], Item::PushClip(clip) if (clip.x, clip.y) == (room.x, room.y))
    );
    let Item::Picture { rect, .. } = items[picture] else {
        unreachable!()
    };
    assert_eq!((rect.x, rect.y), (room.x, room.y));
    assert_eq!(app.inline_images.held(), 1);
}

#[test]
fn a_click_on_an_older_notification_brings_its_own_pane_forward() {
    let (mut fake, mut app, window, a, b) = two_panes(echoing("a"), &echoing("b"));
    let shown = app.windows[&window].window;
    fake.send(&mut app, window, WindowEvent::Focused(false));
    // Two panes' notifications of one window, past the pause between them.
    app.notify(&mut fake, a, "a", "first");
    app.windows.get_mut(&window).unwrap().notified_at = None;
    app.notify(&mut fake, b, "b", "second");
    let told: Vec<u64> = fake.notifications().iter().map(|told| told.id).collect();
    assert_eq!(told.len(), 2);
    // Each click its own pane, in either order, as often as it comes.
    for (id, pane) in [(told[0], a), (told[1], b), (told[0], a)] {
        fake.send_app(&mut app, AppEvent::NotificationClicked(window, id));
        assert_eq!(app.workspace.focused(shown).map(Pane::id), Some(pane));
    }
}

/// A program that reads the keys raw and shows them as `cat -v` does, after
/// `ready-` and its `name`, asking for bracketed paste where `bracketed`.
fn raw_reader(name: &str, bracketed: bool) -> Program {
    let modes = if bracketed { "\\033[?2004h" } else { "" };
    shell(&format!(
        "stty raw -echo; printf '{modes}'; echo ready-{name}; cat -v"
    ))
}

/// The text of `pane`'s first rows, read from its terminal, as a pane of a
/// tab that does not show copies no screen.
fn read_of(app: &App, pane: PaneId) -> String {
    let pane = app.workspace.pane(pane).unwrap();
    (0..8)
        .filter_map(|row| pane.line_text(row, 8).map(|(text, _)| text))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_popup_s_keys_reach_it_alone_under_a_broadcast_which_it_takes_none_of() {
    let (mut fake, mut app, window, a, b) =
        two_panes(raw_reader("a", false), &raw_reader("b", false));
    app.popup_shell = Some(std::path::PathBuf::from("/bin/sh"));
    let shown = app.windows[&window].window;
    run_until(&mut fake, &mut app, |_, app| {
        read_of(app, a).contains("ready-a") && read_of(app, b).contains("ready-b")
    });
    assert!(app.workspace.set_broadcast(shown, BroadcastTo::Tab));
    let mut args = Args::new();
    args.insert(
        "command".to_owned(),
        Value::String("stty raw -echo; echo ready-p; cat -v".to_owned()),
    );
    app.run(&mut fake, POPUP, args);
    let popup = app.focused(window).unwrap();
    assert!(app.workspace.is_popup(popup));
    run_until(&mut fake, &mut app, |_, app| {
        read_of(app, popup).contains("ready-p")
    });
    // Typed into the popup: it alone.
    fake.send(&mut app, window, typed(KeyCode::KeyZ, "z", false));
    run_until(&mut fake, &mut app, |_, app| {
        read_of(app, popup).contains('z')
    });
    let pause = Instant::now() + Duration::from_millis(200);
    run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
    assert!(!read_of(&app, a).contains('z') && !read_of(&app, b).contains('z'));
    // Typed into a split pane: the tab's split panes, not the popup.
    app.focus_pane(&mut fake, window, a);
    fake.send(&mut app, window, typed(KeyCode::KeyK, "k", false));
    run_until(&mut fake, &mut app, |_, app| {
        read_of(app, a).contains('k') && read_of(app, b).contains('k')
    });
    let pause = Instant::now() + Duration::from_millis(200);
    run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
    assert!(
        !read_of(&app, popup).contains('k'),
        "{}",
        read_of(&app, popup)
    );
}

#[test]
fn a_broadcast_s_keys_text_and_pastes_reach_each_pane_that_takes_them_of_its_own_modes() {
    let (mut fake, mut app, window, a, b) =
        two_panes(raw_reader("a", true), &raw_reader("b", false));
    // A third pane, of another tab of the window, which shows the first.
    let shown = app.windows[&window].window;
    let first = app.workspace.window(shown).unwrap().active_tab();
    let grid = app.pane(window).grid();
    let pane = app.spawn_pane(
        &fake,
        &raw_reader("c", false),
        &Environment::default(),
        grid,
        &lokyr_vt_core::terminal::Colors::default(),
    );
    let tab = app.workspace.open_tab(shown, pane, 1).unwrap();
    let c = app.workspace.tab(tab).unwrap().active_pane();
    app.workspace.activate_tab(first);
    fake.send(&mut app, window, WindowEvent::Frame);
    run_until(&mut fake, &mut app, |_, app| {
        read_of(app, c).contains("ready-c")
    });
    assert_eq!(app.focused(window), Some(a));
    let settle = |fake: &mut Fake, app: &mut App| {
        let pause = Instant::now() + Duration::from_millis(200);
        run_until(fake, app, |_, _| Instant::now() > pause);
    };
    // To the tab's panes: a key, and the input method's text.
    assert!(app.workspace.set_broadcast(shown, BroadcastTo::Tab));
    fake.send(&mut app, window, typed(KeyCode::KeyK, "k", false));
    fake.send(
        &mut app,
        window,
        WindowEvent::Ime(Ime::Commit("q".to_owned())),
    );
    run_until(&mut fake, &mut app, |_, app| {
        read_of(app, a).contains("kq") && read_of(app, b).contains("kq")
    });
    settle(&mut fake, &mut app);
    assert!(!read_of(&app, c).contains('k'), "{}", read_of(&app, c));
    // A paste, bracketed where its program asks alone.
    fake.set_clipboard_text("p1");
    fake.send(&mut app, window, command_key(KeyCode::KeyV, "v"));
    run_until(&mut fake, &mut app, |_, app| {
        read_of(app, a).contains("^[[200~p1^[[201~") && read_of(app, b).contains("kqp1")
    });
    assert!(fake.alerts().is_empty());
    // To the window's panes, of every tab, but one left out.
    assert!(app.workspace.set_broadcast(shown, BroadcastTo::Window));
    assert!(app.workspace.set_pane_broadcast(b, true));
    fake.send(&mut app, window, typed(KeyCode::KeyW, "w", false));
    run_until(&mut fake, &mut app, |_, app| {
        read_of(app, a).contains("~w") && read_of(app, c).contains('w')
    });
    settle(&mut fake, &mut app);
    assert!(!read_of(&app, b).contains('w'), "{}", read_of(&app, b));
    // Lines into a program that takes them as Return ask once for all,
    // then reach each pane of its own modes.
    fake.set_clipboard_text("m1\nm2");
    fake.send(&mut app, window, command_key(KeyCode::KeyV, "v"));
    assert_eq!(fake.alerts().len(), 1);
    let id = fake.alerts()[0].id;
    fake.answer_alert(&mut app, id, 0);
    // Bracketed, the line break as it is, which the program shows as one.
    run_until(&mut fake, &mut app, |_, app| {
        read_of(app, a).contains("^[[200~m1")
            && read_of(app, a).contains("m2^[[201~")
            && read_of(app, c).contains("wm1^Mm2")
    });
    settle(&mut fake, &mut app);
    assert!(!read_of(&app, b).contains("m1"), "{}", read_of(&app, b));
}

/// The rectangles of `window`'s last frame of the color of the theme's
/// `token`.
fn rects_of(app: &App, window: WindowId, token: &str) -> Vec<lokyr_render_core::Rect> {
    let state = &app.windows[&window];
    let color = state
        .chrome_tokens
        .as_ref()
        .and_then(|(_, tokens)| tokens.get(token))
        .expect("the token's color");
    let surface = state.surface.as_ref().expect("a surface");
    let last = surface.last.lock().unwrap();
    last.as_ref()
        .expect("a frame")
        .items
        .iter()
        .filter_map(|item| match item {
            lokyr_render_core::Item::Rect { rect, color: of } if *of == color => Some(*rect),
            _ => None,
        })
        .collect()
}

#[test]
fn each_pane_that_takes_the_broadcast_shows_a_ring_of_its_color() {
    let (mut fake, mut app, window, a, b) =
        two_panes_of(unpadded(), echoing("a"), &echoing("b"), true);
    let shown = app.windows[&window].window;
    let frame = |fake: &mut Fake, app: &mut App| {
        fake.send(app, window, WindowEvent::Frame);
        rects_of(app, window, "broadcast.ring")
    };
    // Off: no ring.
    assert!(frame(&mut fake, &mut app).is_empty());
    // To the tab's panes: four sides round each, within its room.
    assert!(app.workspace.set_broadcast(shown, BroadcastTo::Tab));
    let rings = frame(&mut fake, &mut app);
    assert_eq!(rings.len(), 8, "{rings:?}");
    let rooms: Vec<lokyr_render_core::Rect> = app.windows[&window]
        .views
        .iter()
        .map(|view| view.rect)
        .collect();
    for room in &rooms {
        let within = |rect: &lokyr_render_core::Rect| {
            rect.x >= room.x
                && rect.y >= room.y
                && rect.x + rect.width as i32 <= room.x + room.width as i32
                && rect.y + rect.height as i32 <= room.y + room.height as i32
        };
        assert_eq!(rings.iter().filter(|rect| within(rect)).count(), 4);
    }
    // A pane left out: the other's ring alone.
    assert!(app.workspace.set_pane_broadcast(b, true));
    assert_eq!(frame(&mut fake, &mut app).len(), 4);
    let _ = a;
    // Off again: none.
    assert!(app.workspace.set_broadcast(shown, BroadcastTo::Off));
    assert!(frame(&mut fake, &mut app).is_empty());
}

/// Floats a pane of `program` over `window`'s active tab, in `frame`, once
/// it shows `ready-` and its `name`, of the keyboard: its id.
fn float_in(
    fake: &mut Fake,
    app: &mut App,
    window: WindowId,
    program: &Program,
    frame: lokyr_kernel::workspace::FloatingFrame,
    name: &str,
) -> PaneId {
    let shown = app.windows[&window].window;
    let tab = app.workspace.window(shown).unwrap().active_tab();
    let grid = app.pane(window).grid();
    let pane = app.spawn_pane(
        fake,
        program,
        &Environment::default(),
        grid,
        &lokyr_vt_core::terminal::Colors::default(),
    );
    let floating = app.workspace.float_pane(tab, pane, frame).unwrap();
    fake.send(app, window, WindowEvent::Frame);
    let ready = format!("ready-{name}");
    run_until(fake, app, |_, app| {
        screen_of(app, floating).contains(&ready)
    });
    floating
}

#[test]
fn a_floating_pane_over_the_splits_takes_the_clicks_and_the_keys_of_its_box_and_rests() {
    let (mut fake, mut app, window, a, b) =
        two_panes_of(unpadded(), echoing("a"), &echoing("b"), true);
    let frame = lokyr_kernel::workspace::FloatingFrame::new(0.55, 0.55, 0.4, 0.4);
    let floating = float_in(&mut fake, &mut app, window, &echoing("f"), frame, "f");
    // Its grid is its box's, below its header, which its program learns.
    let state = &app.windows[&window];
    let (_, room) = state.floating_rooms[0];
    let view = state
        .views
        .iter()
        .find(|view| view.pane == floating)
        .copied()
        .unwrap();
    assert_eq!(view.rect, room.grid);
    assert!(room.grid.y > room.outer.y && room.grid.width < room.outer.width);
    assert_eq!(app.workspace.pane(floating).unwrap().grid(), view.grid);
    assert_eq!(app.focused(window), Some(floating));
    // A click on the first pane, beside the box: its keyboard.
    click_in(&mut fake, &mut app, window, a);
    assert_eq!(app.focused(window), Some(a));
    // A click on the box's header, over the second pane: the floating
    // pane's.
    let scale = fake.window(window).unwrap().size.scale;
    let header = Point {
        x: (f64::from(room.header.x) + f64::from(room.header.width) / 2.0) / scale,
        y: (f64::from(room.header.y) + f64::from(room.header.height) / 2.0) / scale,
    };
    fake.send(&mut app, window, click(true, header, Modifiers::NONE, 1));
    fake.send(&mut app, window, click(false, header, Modifiers::NONE, 1));
    assert_eq!(app.focused(window), Some(floating));
    // Its keys reach its program alone.
    fake.send(&mut app, window, typed(KeyCode::KeyX, "x", false));
    fake.send(&mut app, window, typed(KeyCode::Enter, "\r", false));
    run_until(&mut fake, &mut app, |_, app| {
        screen_of(app, floating).contains("f:x")
    });
    assert!(!screen_of(&app, b).contains('x'));
    // Once still, it draws no frame of its own.
    run_until(&mut fake, &mut app, |fake, _| {
        !fake.window(window).unwrap().frame_requested
    });
    for _ in 0..3 {
        fake.send(&mut app, window, WindowEvent::Frame);
    }
    assert!(!fake.window(window).unwrap().frame_requested);
    // Its frame drew over the panes, after their cells: its box, of the
    // shadow.
    let surface = app.windows[&window].surface.as_ref().unwrap();
    let last = surface.last.lock().unwrap();
    let items = &last.as_ref().unwrap().items;
    let boxed = items.iter().position(|item| {
        matches!(item, lokyr_render_core::Item::Shapes(shapes)
            if shapes.iter().any(|shape| shape.rect == room.outer && shape.shadow.is_some()))
    });
    let first_cells = items
        .iter()
        .position(|item| matches!(item, lokyr_render_core::Item::Cells(_)));
    assert!(
        boxed.is_some() && first_cells < boxed,
        "{boxed:?} {first_cells:?}"
    );
    drop(last);
    // Accessibility finds it beside the others, a terminal each.
    assert!(fake.ask_accessibility(&mut app, window));
    let tree = fake.window(window).unwrap().accessibility.last().unwrap();
    let terminals = tree
        .nodes
        .iter()
        .filter(|(_, node)| node.role() == accesskit::Role::Terminal)
        .count();
    assert_eq!(terminals, 3);
}
