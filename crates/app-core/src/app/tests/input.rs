//! The tests of the windows' input: keys, the input method's
//! composition, the mouse's gestures and the selection, and drops.

use super::*;

/// Return, for the program's next numbers, and their drawing.
fn more_output(fake: &mut Fake, app: &mut App, window: WindowId, last: &str) {
    fake.send(app, window, typed(KeyCode::Enter, "\r", false));
    run_until(fake, app, |_, app| shows(app, window, last));
}

fn wheel(delta: ScrollDelta, mods: Modifiers) -> WindowEvent {
    WindowEvent::Scroll {
        delta,
        phase: None,
        momentum: false,
        position: cell(0, 0),
        mods,
    }
}

/// `event`, a key's press, as the system repeats it.
fn repeated(event: WindowEvent) -> WindowEvent {
    let WindowEvent::Key(key) = event else {
        unreachable!("a key");
    };
    WindowEvent::Key(KeyEvent {
        state: KeyState::Repeated,
        ..key
    })
}

#[test]
fn the_pointer_is_the_text_s_but_over_a_program_that_takes_the_mouse() {
    // A program that takes the mouse's clicks at a key, and gives them
    // back at the next.
    let (mut fake, mut app, window) = started_with(shell(concat!(
        "stty raw -echo; echo ready; head -c1 >/dev/null; ",
        "printf '\\033[?1000h'; head -c1 >/dev/null; printf '\\033[?1000l'; cat"
    )));
    let pointer = |fake: &Fake| fake.window(window).unwrap().pointer;
    run_until(&mut fake, &mut app, |fake, _| {
        pointer(fake) == PointerShape::Text
    });
    let a = key_event(KeyCode::KeyA, "a", Modifiers::NONE, true);
    fake.send(&mut app, window, a.clone());
    run_until(&mut fake, &mut app, |fake, _| {
        pointer(fake) == PointerShape::Arrow
    });
    fake.send(&mut app, window, a);
    run_until(&mut fake, &mut app, |fake, _| {
        pointer(fake) == PointerShape::Text
    });
}

#[test]
fn a_drag_selects_the_cells_between_its_ends_and_a_click_selects_none() {
    let (mut fake, mut app, window) = selecting("", "");
    let none = Modifiers::NONE;
    select_drag(&mut fake, &mut app, window, (cell(0, 0), cell(5, 0)), none);
    assert_eq!(selected_text(&app, window).as_deref(), Some("hello"));
    // Backwards, across rows.
    select_drag(&mut fake, &mut app, window, (cell(6, 1), cell(6, 0)), none);
    assert_eq!(
        selected_text(&app, window).as_deref(),
        Some("world\nsecond")
    );
    // The view shows it at its next frame.
    run_until(&mut fake, &mut app, |_, app| {
        app.pane(window).snapshot().selection().is_some()
    });
    assert!(fake.window(window).is_some());
    // A click takes it away.
    fake.send(&mut app, window, click(true, cell(2, 1), none, 1));
    fake.send(&mut app, window, click(false, cell(2, 1), none, 1));
    assert_eq!(selected_text(&app, window), None);
}

#[test]
fn clicks_select_a_word_and_a_line_and_option_a_rectangle() {
    let (mut fake, mut app, window) = selecting("", "");
    let none = Modifiers::NONE;
    fake.send(&mut app, window, click(true, cell(8, 0), none, 2));
    assert_eq!(selected_text(&app, window).as_deref(), Some("world"));
    // Dragged, a word's selection goes on by words.
    fake.send(&mut app, window, moved(cell(1, 1), none));
    assert_eq!(
        selected_text(&app, window).as_deref(),
        Some("world\nsecond")
    );
    fake.send(&mut app, window, click(false, cell(1, 1), none, 2));
    fake.send(&mut app, window, click(true, cell(3, 1), none, 3));
    fake.send(&mut app, window, click(false, cell(3, 1), none, 3));
    assert_eq!(selected_text(&app, window).as_deref(), Some("second line"));
    let option = Modifiers {
        left_alt: true,
        ..Modifiers::NONE
    };
    select_drag(
        &mut fake,
        &mut app,
        window,
        (cell(1, 0), cell(4, 1)),
        option,
    );
    assert_eq!(selected_text(&app, window).as_deref(), Some("ell\neco"));
}

#[test]
fn shift_s_click_extends_the_selection() {
    let (mut fake, mut app, window) = selecting("", "");
    select_drag(
        &mut fake,
        &mut app,
        window,
        (cell(0, 0), cell(5, 0)),
        Modifiers::NONE,
    );
    fake.send(&mut app, window, click(true, cell(3, 1), SHIFT, 1));
    fake.send(&mut app, window, click(false, cell(3, 1), SHIFT, 1));
    assert_eq!(
        selected_text(&app, window).as_deref(),
        Some("hello world\nsec")
    );
}

#[test]
fn a_selection_copies_its_text_as_it_ends_when_the_config_says() {
    let (mut fake, mut app, window) = selecting("", "");
    select_drag(
        &mut fake,
        &mut app,
        window,
        (cell(0, 0), cell(5, 0)),
        Modifiers::NONE,
    );
    assert_eq!(fake.clipboard_text(), None);
    let (mut fake, mut app, window) = selecting("copy-on-select = true\n", "");
    fake.send(
        &mut app,
        window,
        click(true, cell(0, 0), Modifiers::NONE, 1),
    );
    fake.send(&mut app, window, moved(cell(5, 0), Modifiers::NONE));
    // Not before the button goes up.
    assert_eq!(fake.clipboard_text(), None);
    fake.send(
        &mut app,
        window,
        click(false, cell(5, 0), Modifiers::NONE, 1),
    );
    assert_eq!(fake.clipboard_text().as_deref(), Some("hello"));
    // A click, which selects nothing, copies nothing.
    fake.send(
        &mut app,
        window,
        click(true, cell(2, 1), Modifiers::NONE, 1),
    );
    fake.send(
        &mut app,
        window,
        click(false, cell(2, 1), Modifiers::NONE, 1),
    );
    assert_eq!(fake.clipboard_text().as_deref(), Some("hello"));
    // Nor do empty cells, whose text is empty.
    select_drag(
        &mut fake,
        &mut app,
        window,
        (cell(0, 6), cell(5, 6)),
        Modifiers::NONE,
    );
    assert_eq!(selected_text(&app, window).as_deref(), Some(""));
    assert_eq!(fake.clipboard_text().as_deref(), Some("hello"));
}

#[test]
fn a_program_that_takes_the_mouse_has_the_clicks_but_shift_s() {
    let (mut fake, mut app, window) = selecting("", "\\033[?1000h\\033[?1006h");
    select_drag(
        &mut fake,
        &mut app,
        window,
        (cell(0, 0), cell(5, 0)),
        Modifiers::NONE,
    );
    assert_eq!(selected_text(&app, window), None);
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "[<0;1;1M"));
    select_drag(&mut fake, &mut app, window, (cell(0, 0), cell(5, 0)), SHIFT);
    assert_eq!(selected_text(&app, window).as_deref(), Some("hello"));
}

#[test]
fn a_program_that_takes_the_clicks_has_the_right_button_but_with_shift() {
    let (mut fake, mut app, window) = selecting("", "\\033[?1000h\\033[?1006h");
    right_click(&mut fake, &mut app, window, cell(2, 0), Modifiers::NONE);
    let control = Modifiers {
        control: true,
        ..Modifiers::NONE
    };
    fake.send(&mut app, window, click(true, cell(4, 0), control, 1));
    fake.send(&mut app, window, click(false, cell(4, 0), control, 1));
    run_until(&mut fake, &mut app, |_, app| {
        shows(app, window, "^[[<2;3;1M^[[<2;3;1m^[[<16;5;1M^[[<16;5;1m")
    });
    assert!(fake.context_menus().is_empty());
    // With Shift, the menu opens, and the program learns of nothing.
    right_click(&mut fake, &mut app, window, cell(8, 0), SHIFT);
    assert_eq!(selected_text(&app, window).as_deref(), Some("world"));
    assert_eq!(context_menu(&fake, window).0, cell(8, 0));
    fake.send(&mut app, window, typed(KeyCode::KeyX, "x", false));
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "1mx"));
}

#[test]
fn output_during_a_drag_leaves_its_anchor_on_its_text() {
    let (mut fake, mut app, window, ninety) = numbers();
    let none = Modifiers::NONE;
    fake.send(&mut app, window, click(true, cell(0, ninety), none, 1));
    fake.send(&mut app, window, moved(cell(2, ninety), none));
    assert_eq!(selected_text(&app, window).as_deref(), Some("90"));
    // Three lines scroll `90` up; the pointer, on the same row of the
    // screen, now over `93`.
    more_output(&mut fake, &mut app, window, "103");
    fake.send(&mut app, window, moved(cell(2, ninety), none));
    assert_eq!(
        selected_text(&app, window).as_deref(),
        Some("90\n91\n92\n93")
    );
    fake.send(&mut app, window, click(false, cell(2, ninety), none, 1));
    // Shift's click after more output extends from `90` too.
    more_output(&mut fake, &mut app, window, "106");
    fake.send(&mut app, window, click(true, cell(2, ninety), SHIFT, 1));
    fake.send(&mut app, window, click(false, cell(2, ninety), SHIFT, 1));
    let text = selected_text(&app, window).unwrap();
    assert!(
        text.starts_with("90\n91") && text.ends_with("\n96"),
        "{text:?}"
    );
}

#[test]
fn a_drag_past_the_bottom_scrolls_the_view_down_and_a_release_stops_it() {
    let mut fake = Fake::default();
    let mut app = app(
        unpadded(),
        shell("seq 1 200; echo ready; stty raw -echo; cat -v"),
    );
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
    app.pane(window).scroll(Scroll::Up(10));
    let none = Modifiers::NONE;
    fake.send(&mut app, window, click(true, cell(3, 2), none, 1));
    // Two rows below the window's bottom.
    let rows = f64::from(app.pane(window).grid().rows);
    let below = Point {
        x: 1.0,
        y: 8.0 * (rows + 2.0),
    };
    fake.send(&mut app, window, moved(below, none));
    assert!(app.scroll_timer.is_some());
    fake.fire_timers(&mut app, Instant::now() + SELECTION_SCROLL);
    assert!(app.pane(window).view_offset() < 10);
    // The release, past the edge, stops it.
    fake.send(&mut app, window, click(false, below, none, 1));
    fake.fire_timers(&mut app, Instant::now() + SELECTION_SCROLL);
    assert!(app.scroll_timer.is_none());
    let offset = app.pane(window).view_offset();
    fake.fire_timers(&mut app, Instant::now() + SELECTION_SCROLL * 2);
    assert_eq!(app.pane(window).view_offset(), offset);
}

#[test]
fn a_double_click_copies_its_word_on_select() {
    let (mut fake, mut app, window) = selecting("copy-on-select = true\n", "");
    let none = Modifiers::NONE;
    fake.send(&mut app, window, click(true, cell(7, 0), none, 2));
    fake.send(&mut app, window, click(false, cell(7, 0), none, 2));
    assert_eq!(fake.clipboard_text().as_deref(), Some("world"));
}

#[test]
fn a_drag_past_the_top_scrolls_the_view_under_the_selection() {
    let mut fake = Fake::default();
    let mut app = app(
        unpadded(),
        shell("seq 1 200; echo ready; stty raw -echo; cat -v"),
    );
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
    let none = Modifiers::NONE;
    fake.send(&mut app, window, click(true, cell(3, 2), none, 1));
    // Two rows above the window's top.
    let above = Point { x: 1.0, y: -12.0 };
    fake.send(&mut app, window, moved(above, none));
    assert!(app.scroll_timer.is_some());
    for _ in 0..3 {
        fake.fire_timers(&mut app, Instant::now() + SELECTION_SCROLL);
    }
    // Three steps of two rows; the selection goes up with the view.
    assert_eq!(app.pane(window).view_offset(), 6);
    // From where the view brought the pointer, six rows above the
    // screen's top row, `141`, to the press's, after `143`.
    let text = selected_text(&app, window).unwrap();
    assert_eq!(text.lines().count(), 9, "{text:?}");
    assert!(text.starts_with("135\n136"), "{text:?}");
    assert!(text.ends_with("142\n143"), "{text:?}");
    // Back within the grid, the scrolling stops.
    fake.send(&mut app, window, moved(cell(0, 0), none));
    fake.fire_timers(&mut app, Instant::now() + SELECTION_SCROLL);
    assert_eq!(app.pane(window).view_offset(), 6);
    assert!(app.scroll_timer.is_none());
    fake.send(&mut app, window, click(false, cell(0, 0), none, 1));
}

#[test]
fn a_click_reaches_a_program_that_tracks_the_mouse_but_with_shift() {
    let (mut fake, mut app, window) = started_with(reader("\\033[?1000h\\033[?1006h"));
    // Shift keeps the click for the terminal.
    fake.send(
        &mut app,
        window,
        press(MouseButton::Left, true, cell(2, 1), SHIFT),
    );
    fake.send(
        &mut app,
        window,
        press(MouseButton::Left, false, cell(2, 1), SHIFT),
    );
    fake.send(
        &mut app,
        window,
        press(MouseButton::Left, true, cell(4, 2), Modifiers::NONE),
    );
    fake.send(
        &mut app,
        window,
        press(MouseButton::Left, false, cell(4, 2), Modifiers::NONE),
    );
    run_until(&mut fake, &mut app, |_, app| {
        shows(app, window, "^[[<0;5;3M^[[<0;5;3m")
    });
    assert!(!shows(&app, window, ";3;2"), "{:?}", screen(&app, window));
}

#[test]
fn a_release_with_shift_reaches_a_program_that_learned_of_the_press() {
    let (mut fake, mut app, window) = started_with(reader("\\033[?1000h\\033[?1006h"));
    fake.send(
        &mut app,
        window,
        press(MouseButton::Left, true, cell(4, 2), Modifiers::NONE),
    );
    fake.send(
        &mut app,
        window,
        press(MouseButton::Left, false, cell(4, 2), SHIFT),
    );
    run_until(&mut fake, &mut app, |_, app| {
        shows(app, window, "^[[<0;5;3M^[[<4;5;3m")
    });
}

#[test]
fn a_release_goes_nowhere_when_the_program_did_not_learn_of_the_press() {
    // The mouse's tracking from the first key on.
    let program = shell(
        "stty raw -echo; echo ready; head -c 1 >/dev/null; printf '\\033[?1000h\\033[?1006h'; echo on; cat -v",
    );
    let (mut fake, mut app, window) = started_with(program);
    let at = cell(4, 2);
    fake.send(
        &mut app,
        window,
        press(MouseButton::Left, true, at, Modifiers::NONE),
    );
    fake.send(&mut app, window, key());
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "on"));
    for pressed in [false, true, false] {
        let event = press(MouseButton::Left, pressed, at, Modifiers::NONE);
        fake.send(&mut app, window, event);
    }
    run_until(&mut fake, &mut app, |_, app| {
        screen(app, window)
            .iter()
            .any(|line| line.trim() == "^[[<0;5;3M^[[<0;5;3m")
    });
}

#[test]
fn a_click_in_a_scrolled_view_reaches_the_program_at_the_screen_s_row() {
    let program =
        shell("seq 1 100; printf '\\033[?1003h\\033[?1006h'; stty raw -echo; echo ready; cat -v");
    let (mut fake, mut app, window) = started_with(program);
    let offset = |app: &App| app.pane(window).snapshot().offset();
    // Shift's wheel moves the view, which the program takes otherwise.
    let up = ScrollDelta::Lines { x: 0.0, y: 3.0 };
    fake.send(&mut app, window, wheel(up, SHIFT));
    run_until(&mut fake, &mut app, |_, app| offset(app) == 3);
    // A move and a click above the screen go nowhere; those on its
    // fifth row, the view's eighth, at its row.
    for at in [cell(3, 2), cell(3, 7)] {
        let moved = WindowEvent::MouseMoved {
            position: at,
            mods: Modifiers::NONE,
        };
        fake.send(&mut app, window, moved);
    }
    for row in [2, 7] {
        for pressed in [true, false] {
            let event = press(MouseButton::Left, pressed, cell(1, row), Modifiers::NONE);
            fake.send(&mut app, window, event);
        }
    }
    // The view stays up as the program writes: down to read it.
    let down = ScrollDelta::Lines { x: 0.0, y: -3.0 };
    fake.send(&mut app, window, wheel(down, SHIFT));
    run_until(&mut fake, &mut app, |_, app| {
        screen(app, window)
            .iter()
            .any(|line| line.trim() == "^[[<35;4;5M^[[<0;2;5M^[[<0;2;5m")
    });
}

#[test]
fn a_composition_brings_the_view_back_to_the_screen() {
    let mut fake = Fake::default();
    let mut app = app(unpadded(), shell("seq 1 100; echo ready; sleep 10"));
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
    let offset = |app: &App| app.pane(window).snapshot().offset();
    let up = ScrollDelta::Lines { x: 0.0, y: 3.0 };
    fake.send(&mut app, window, wheel(up, Modifiers::NONE));
    run_until(&mut fake, &mut app, |_, app| offset(app) == 3);
    let composing = Ime::Preedit {
        text: "に".to_owned(),
        selection: 3..3,
    };
    fake.send(&mut app, window, WindowEvent::Ime(composing));
    run_until(&mut fake, &mut app, |_, app| offset(app) == 0);
}

#[test]
fn the_candidates_go_at_a_composition_moved_left_of_the_cursor() {
    // The cursor in the last column.
    let mut fake = Fake::default();
    let mut app = app(unpadded(), shell("printf 'ready\\033[999G'; sleep 10"));
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
    let columns = app.pane(window).snapshot().cols();
    let area = |fake: &mut Fake, app: &mut App| {
        fake.send(app, window, WindowEvent::Frame);
        fake.window(window).unwrap().text_area.unwrap().0.x
    };
    run_until(&mut fake, &mut app, |_, app| {
        app.pane(window).snapshot().cursor().position.x == columns - 1
    });
    // A column is 4 points: the cursor's, then the composition's, of
    // four cells, as far left as it takes.
    assert_eq!(area(&mut fake, &mut app), f64::from(columns - 1) * 4.0);
    let composing = Ime::Preedit {
        text: "日本".to_owned(),
        selection: 6..6,
    };
    fake.send(&mut app, window, WindowEvent::Ime(composing));
    assert_eq!(area(&mut fake, &mut app), f64::from(columns - 4) * 4.0);
}

#[test]
fn the_wheel_moves_the_view_through_the_scrollback() {
    let mut fake = Fake::default();
    let mut app = app(unpadded(), shell("seq 1 100; echo ready; sleep 10"));
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
    let offset = |app: &App| app.pane(window).snapshot().offset();
    fake.send(
        &mut app,
        window,
        wheel(ScrollDelta::Lines { x: 0.0, y: 3.0 }, Modifiers::NONE),
    );
    run_until(&mut fake, &mut app, |_, app| offset(app) == 3);
    // A trackpad's points, by rows of 8: a row and a half down, and the
    // half that makes a row with the next.
    let points = |y| wheel(ScrollDelta::Points { x: 0.0, y }, Modifiers::NONE);
    fake.send(&mut app, window, points(-12.0));
    run_until(&mut fake, &mut app, |_, app| offset(app) == 2);
    fake.send(&mut app, window, points(-4.0));
    run_until(&mut fake, &mut app, |_, app| offset(app) == 1);
    // Shift turns a mouse's wheel sideways, which still scrolls the view.
    fake.send(
        &mut app,
        window,
        wheel(ScrollDelta::Lines { x: 2.0, y: 0.0 }, SHIFT),
    );
    run_until(&mut fake, &mut app, |_, app| offset(app) == 3);
}

#[test]
fn the_wheel_goes_to_a_program_that_takes_it_but_with_shift() {
    let (mut fake, mut app, window) = started_with(reader("\\033[?1000h\\033[?1006h"));
    let up = ScrollDelta::Lines { x: 0.0, y: 1.0 };
    fake.send(&mut app, window, wheel(up, Modifiers::NONE));
    run_until(&mut fake, &mut app, |_, app| {
        shows(app, window, "^[[<64;1;1M")
    });
    // With Shift, the view moves, without the program.
    fake.send(&mut app, window, wheel(up, SHIFT));
    let offset = |app: &App| app.pane(window).snapshot().offset();
    // The screen has no scrollback yet: the view stays, and the program
    // learns nothing more, a while after.
    let pause = Instant::now() + Duration::from_millis(300);
    run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
    assert_eq!(offset(&app), 0);
    assert_eq!(
        screen(&app, window).concat().matches("^[[<").count(),
        1,
        "{:?}",
        screen(&app, window)
    );
}

#[test]
fn the_wheel_sends_the_cursor_keys_on_the_alternate_screen() {
    let (mut fake, mut app, window) = started_with(reader("\\033[?1049h"));
    let down = ScrollDelta::Lines { x: 0.0, y: -2.0 };
    fake.send(&mut app, window, wheel(down, Modifiers::NONE));
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "^[[B^[[B"));
}

#[test]
fn moves_come_for_a_program_that_asked_for_them_once_a_cell() {
    let (mut fake, mut app, window) = started_with(reader("\\033[?1003h\\033[?1006h"));
    run_until(&mut fake, &mut app, |fake, _| {
        fake.window(window).unwrap().mouse_moves
    });
    for at in [cell(1, 0), Point { x: 6.0, y: 2.0 }, cell(3, 0)] {
        let event = WindowEvent::MouseMoved {
            position: at,
            mods: Modifiers::NONE,
        };
        fake.send(&mut app, window, event);
    }
    run_until(&mut fake, &mut app, |_, app| {
        shows(app, window, "^[[<35;4;1M")
    });
    // The second move, in the first's cell, reported nothing.
    let shown = screen(&app, window).concat();
    assert_eq!(shown.matches("^[[<35;").count(), 2, "{shown:?}");
    assert!(shown.contains("^[[<35;2;1M^[[<35;4;1M"), "{shown:?}");
}

#[test]
fn a_bound_key_runs_its_command_and_its_release_goes_nowhere() {
    let keys = "\"cmd+shift+n\" = \"window.new\"\n";
    let (mut fake, mut app, window) = bound(keys, reader("\\033[>3u"));
    let shifted = Modifiers { shift: true, ..CMD };
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::KeyN, "n", shifted, true),
    );
    assert_eq!(fake.windows().count(), 2);
    // The release reaches nothing, though the program asked for
    // releases, as the kitty protocol's flags have it.
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::KeyN, "n", shifted, false),
    );
    // The default too.
    fake.send(&mut app, window, key_event(KeyCode::KeyN, "n", CMD, true));
    assert_eq!(fake.windows().count(), 3);
    let pause = Instant::now() + Duration::from_millis(300);
    run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
    let shown = screen(&app, window).concat();
    assert!(!shown.contains("^["), "{shown:?}");
}

#[test]
fn a_sequence_holds_its_keys_back_until_it_runs_or_gives_them_to_the_program() {
    let keys = "\"ctrl+a>n\" = \"window.new\"\n";
    let (mut fake, mut app, window) = bound(keys, reader(""));
    fake.send(&mut app, window, key_event(KeyCode::KeyA, "a", CTRL, true));
    fake.send(&mut app, window, key_event(KeyCode::KeyA, "a", CTRL, false));
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::KeyN, "n", Modifiers::NONE, true),
    );
    assert_eq!(fake.windows().count(), 2);
    // A key that goes on with no sequence: both reach the program.
    fake.send(&mut app, window, key_event(KeyCode::KeyA, "a", CTRL, true));
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::KeyE, "e", Modifiers::NONE, true),
    );
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "^Ae"));
    let shown = screen(&app, window).concat();
    assert_eq!(shown.matches("^A").count(), 1, "{shown:?}");
    assert!(!shown.contains('n'), "{shown:?}");
}

#[test]
fn a_cmd_click_opens_the_link_under_it_rather_than_selecting() {
    let (mut fake, mut app, window, _) = pasting(
        "click",
        "[links]\nurls = \"open\"\n",
        shell(concat!(
            "stty raw -echo; cd /usr; printf 'https://lokyr.dev/x bin/\\r\\n",
            "\\033]8;;https://lokyr.dev/osc\\033\\\\here\\033]8;;\\033\\\\\\r\\n'; ",
            "echo ready; cat -v"
        )),
    );
    let cmd_click = |fake: &mut Fake, app: &mut App, col, row| {
        fake.send(app, window, click(true, cell(col, row), CMD, 1));
        fake.send(app, window, click(false, cell(col, row), CMD, 1));
    };
    // A URL, a path from the program's directory, an OSC 8 link, and
    // nothing on a space.
    for (col, row) in [(2, 0), (21, 0), (1, 1), (10, 2)] {
        cmd_click(&mut fake, &mut app, col, row);
    }
    assert_eq!(
        fake.opened(),
        ["https://lokyr.dev/x", "/usr/bin", "https://lokyr.dev/osc"]
    );
    assert_eq!(selected_text(&app, window), None);
    assert!(app.windows[&window].selecting.is_none());
    // A window without the focus takes the click as a click.
    fake.send(&mut app, window, WindowEvent::Focused(false));
    cmd_click(&mut fake, &mut app, 2, 0);
    assert_eq!(fake.opened().len(), 3);
    assert!(app.windows[&window].selecting.is_some());
}

#[test]
fn a_click_runs_the_command_that_the_config_binds_it_to() {
    let mut config = Config::parse(
        "[keys]\n\"alt+click\" = \"window.new\"\n",
        std::path::Path::new("config.toml"),
    )
    .unwrap();
    config.window = unpadded().window;
    let mut fake = Fake::default();
    let mut app = app(config, shell("echo ready; sleep 10"));
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let alt = Modifiers {
        left_alt: true,
        ..Modifiers::NONE
    };
    // Without the arguments that it does not take; a double click is
    // not one.
    fake.send(&mut app, window, click(true, cell(1, 0), alt, 1));
    assert_eq!(fake.windows().count(), 2);
    fake.send(&mut app, window, click(false, cell(1, 0), alt, 1));
    fake.send(&mut app, window, click(true, cell(1, 0), alt, 2));
    fake.send(&mut app, window, click(false, cell(1, 0), alt, 2));
    assert_eq!(fake.windows().count(), 2);
}

#[test]
fn a_drop_pastes_its_files_paths_or_its_text() {
    let (mut fake, mut app, window) = focused_with(reader(""));
    let paths = ["/tmp/a file.txt", "/usr/bin"].map(PathBuf::from).to_vec();
    fake.send(
        &mut app,
        window,
        WindowEvent::Dropped(Dropped::Files(paths)),
    );
    run_until(&mut fake, &mut app, |_, app| {
        shows(app, window, "'/tmp/a file.txt' /usr/bin")
    });
    // Text pastes as a paste does: several lines ask first.
    fake.send(
        &mut app,
        window,
        WindowEvent::Dropped(Dropped::Text("x".to_owned())),
    );
    run_until(&mut fake, &mut app, |_, app| {
        shows(app, window, "/usr/bin x")
    });
    assert!(fake.alerts().is_empty());
    let lines = Dropped::Text("a\nb\n".to_owned());
    fake.send(&mut app, window, WindowEvent::Dropped(lines));
    assert_eq!(fake.alerts().len(), 1);
}

#[test]
fn a_key_s_release_reaches_the_program_after_its_binding_repeated() {
    let keys = "\"ctrl+a\" = \"window.new\"\n";
    let (mut fake, mut app, window) = bound(keys, reader("\\033[>3u"));
    let bound = key_event(KeyCode::KeyA, "a", CTRL, true);
    fake.send(&mut app, window, bound.clone());
    fake.send(&mut app, window, repeated(bound));
    fake.send(&mut app, window, key_event(KeyCode::KeyA, "a", CTRL, false));
    assert_eq!(fake.windows().count(), 3);
    // The key without Control is the program's, its release too.
    let plain = Modifiers::NONE;
    fake.send(&mut app, window, key_event(KeyCode::KeyA, "a", plain, true));
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::KeyA, "a", plain, false),
    );
    run_until(&mut fake, &mut app, |_, app| {
        shows(app, window, "a^[[97;1:3u")
    });
}

#[test]
fn a_sequence_s_keys_go_back_to_the_program_with_their_releases() {
    let keys = "\"ctrl+a>n\" = \"window.new\"\n";
    let (mut fake, mut app, window) = bound(keys, reader("\\033[>3u"));
    // Released while the sequence waits, and held while it repeats.
    let first = key_event(KeyCode::KeyA, "a", CTRL, true);
    fake.send(&mut app, window, first.clone());
    fake.send(&mut app, window, repeated(first));
    fake.send(&mut app, window, key_event(KeyCode::KeyA, "a", CTRL, false));
    let plain = Modifiers::NONE;
    fake.send(&mut app, window, key_event(KeyCode::KeyE, "e", plain, true));
    run_until(&mut fake, &mut app, |_, app| {
        shows(app, window, "^[[97;5u^[[97;5:3ue")
    });
    // Pressed, then given back, then released: the release follows.
    fake.send(&mut app, window, key_event(KeyCode::KeyA, "a", CTRL, true));
    fake.send(&mut app, window, key_event(KeyCode::KeyE, "e", plain, true));
    fake.send(&mut app, window, key_event(KeyCode::KeyA, "a", CTRL, false));
    run_until(&mut fake, &mut app, |_, app| {
        shows(app, window, "e^[[97;5ue^[[97;5:3u")
    });
    assert_eq!(fake.windows().count(), 1);
}

#[test]
fn a_commit_and_a_keypad_key_end_a_sequence() {
    let keys = "\"ctrl+a>n\" = \"window.new\"\n";
    let (mut fake, mut app, window) = bound(keys, reader(""));
    fake.send(&mut app, window, key_event(KeyCode::KeyA, "a", CTRL, true));
    fake.send(
        &mut app,
        window,
        WindowEvent::Ime(Ime::Commit("x".to_owned())),
    );
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "^Ax"));
    fake.send(&mut app, window, key_event(KeyCode::KeyA, "a", CTRL, true));
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::Numpad1, "1", Modifiers::NONE, true),
    );
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "^Ax^A1"));
    // The sequence ended: `n` types.
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::KeyN, "n", Modifiers::NONE, true),
    );
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "^Ax^A1n"));
    assert_eq!(fake.windows().count(), 1);
}

#[test]
fn losing_the_focus_forgets_the_keys_that_the_keymap_took() {
    let keys = "\"ctrl+a\" = \"window.new\"\n";
    let (mut fake, mut app, window) = bound(keys, reader("\\033[>3u"));
    fake.send(&mut app, window, key_event(KeyCode::KeyA, "a", CTRL, true));
    // The release comes to another window.
    fake.send(&mut app, window, WindowEvent::Focused(false));
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let plain = Modifiers::NONE;
    fake.send(&mut app, window, key_event(KeyCode::KeyA, "a", plain, true));
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::KeyA, "a", plain, false),
    );
    run_until(&mut fake, &mut app, |_, app| {
        shows(app, window, "a^[[97;1:3u")
    });
}

#[test]
fn a_binding_holds_with_a_selection_or_without() {
    let keys = concat!(
        "\"cmd+e\" = [\n",
        "  { command = \"pane.send-text\", args = { text = \"S\" }, when = \"selection\" },\n",
        "  { command = \"pane.send-text\", args = { text = \"N\" }, when = \"!selection\" },\n",
        "]\n",
    );
    let (mut fake, mut app, window) = bound(keys, reader(""));
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let cmd_e = || key_event(KeyCode::KeyE, "e", CMD, true);
    fake.send(&mut app, window, cmd_e());
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "N"));
    // Command-A selects everything.
    fake.send(&mut app, window, command_key(KeyCode::KeyA, "a"));
    fake.send(&mut app, window, cmd_e());
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "NS"));
}

#[test]
fn a_step_of_the_wheel_runs_its_binding() {
    let keys = "\"cmd+wheel-up\" = \"window.new\"\n";
    let (mut fake, mut app, window) = bound(keys, reader(""));
    let up = ScrollDelta::Lines { x: 0.0, y: 2.0 };
    fake.send(&mut app, window, wheel(up, CMD));
    assert_eq!(fake.windows().count(), 3, "a window a step");
    // Without Command, the wheel is the view's.
    fake.send(&mut app, window, wheel(up, Modifiers::NONE));
    assert_eq!(fake.windows().count(), 3);
}

#[test]
fn keys_reach_the_program_but_command_s_do_not() {
    use lokyr_platform_core::KeyCode;

    let (mut fake, mut app) = started();
    let window = fake.windows().next().unwrap();
    // `cat`, which the terminal echoes, and which writes back each line.
    fake.send(&mut app, window, typed(KeyCode::KeyH, "h", true));
    fake.send(&mut app, window, typed(KeyCode::KeyO, "o", false));
    fake.send(&mut app, window, typed(KeyCode::KeyK, "k", false));
    fake.send(&mut app, window, typed(KeyCode::Enter, "\r", false));
    run_until(&mut fake, &mut app, |_, app| row(app, window, 1) == "ok");
    assert_eq!(row(&app, window, 0), "ok");
}

#[test]
fn a_kept_window_closes_with_a_key_that_types_or_acts_alone() {
    use lokyr_platform_core::KeyCode;

    let (mut fake, mut app, window) = ended(CloseOnExit::Clean, "exit 3");
    // A lock, Fn, a modifier and a Command key leave it open.
    for key in [
        typed(KeyCode::CapsLock, "", false),
        typed(KeyCode::Fn, "", false),
        typed(KeyCode::ShiftLeft, "", false),
        typed(KeyCode::KeyH, "h", true),
    ] {
        fake.send(&mut app, window, key);
        assert_eq!(fake.windows().count(), 1);
    }
    // What an input method composes does not show; its commit closes.
    let composing = Ime::Preedit {
        text: "に".to_owned(),
        selection: 3..3,
    };
    fake.send(&mut app, window, WindowEvent::Ime(composing));
    assert_eq!(app.windows[&window].preedit, None);
    fake.send(
        &mut app,
        window,
        WindowEvent::Ime(Ime::Commit("日".to_owned())),
    );
    assert_eq!(fake.windows().count(), 0);
}
