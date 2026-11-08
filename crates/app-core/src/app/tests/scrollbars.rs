//! The panes' scroll bars: shown as the view moves and as the pointer comes
//! over them, faded a while after, dragged to a line, clicked on their
//! tracks, always shown or never, and marked by a search's matches.

use lokyr_platform_core::{ScrollDelta, Scrollers};
use lokyr_ui_core::scrollbar;

use super::panels::frames;
use super::*;
use crate::app::scrollbars::{Bar, FADE, SHOW};

/// A program of 200 lines past the screen, then a prompt that waits.
fn lines() -> Program {
    shell("for n in $(seq 1 200); do echo \"line $n\"; done; printf 'ready $ '; sleep 30")
}

/// The app without padding, of `config`, focused, its window running
/// [`lines`], once it shows the prompt.
fn started_bars(config: Config) -> (Fake, App, WindowId) {
    let (mut fake, mut app, window) = drawn(config, lines());
    fake.send(&mut app, window, WindowEvent::Focused(true));
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready $"));
    frames(&mut fake, &mut app, window);
    (fake, app, window)
}

/// The app of `config`, which draws its frames, its window running
/// `program`.
fn drawn(config: Config, program: Program) -> (Fake, App, WindowId) {
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let mut fake = Fake::default();
    let mut app = App::new(Some(drawing), config, None).with_program(program);
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(fakes::Surface::default());
    (fake, app, window)
}

/// The window's first pane's bar, as its last frame laid it out.
fn bar(app: &App, window: WindowId) -> Option<Bar> {
    app.windows[&window].bars.first().cloned()
}

/// The bar's opacity, 0 of none.
fn opacity(app: &App, window: WindowId) -> f32 {
    bar(app, window).map_or(0.0, |bar| bar.look.opacity)
}

/// The window's scale.
fn scale(fake: &Fake, window: WindowId) -> f64 {
    fake.window(window).unwrap().size.scale
}

/// The point of `pixels`, of the window's scale.
fn point(fake: &Fake, window: WindowId, (x, y): (f32, f32)) -> Point {
    let scale = scale(fake, window);
    Point {
        x: f64::from(x) / scale,
        y: f64::from(y) / scale,
    }
}

/// The middle of the bar's wide knob, in points.
fn knob_middle(fake: &Fake, app: &App, window: WindowId) -> Point {
    let bar = bar(app, window).unwrap();
    let knob = scrollbar::knob(bar.area, bar.rows, true, scale(fake, window) as f32);
    point(
        fake,
        window,
        (
            knob.x as f32 + knob.width as f32 / 2.0,
            knob.y as f32 + knob.height as f32 / 2.0,
        ),
    )
}

/// The view's first row, and the oldest's and the screen's first.
fn view(app: &App, window: WindowId) -> (i64, i64, i64) {
    let viewport = app.pane(window).viewport().unwrap();
    (viewport.top, viewport.oldest, viewport.screen)
}

/// A step of the wheel of `lines` up, over the pane's first cell.
fn wheel_up(lines: f64) -> WindowEvent {
    WindowEvent::Scroll {
        delta: ScrollDelta::Lines { x: 0.0, y: lines },
        phase: None,
        momentum: false,
        position: cell(0, 0),
        mods: Modifiers::NONE,
    }
}

#[test]
fn a_scroll_shows_the_bar_thin_which_fades_a_while_after() {
    let (mut fake, mut app, window) = started_bars(unpadded());
    // A view of every row but the scrollback's, still: hidden.
    let shown = bar(&app, window).expect("a bar of the scrollback");
    assert_eq!(shown.look.opacity, 0.0);
    assert_eq!(shown.rows.shown, u64::from(app.pane(window).grid().rows));
    assert!(shown.rows.total > 200);
    // Output alone, at the screen, shows none.
    let before = Instant::now();
    fake.send(&mut app, window, wheel_up(3.0));
    run_until(&mut fake, &mut app, |_, app| opacity(app, window) == 1.0);
    let shown = bar(&app, window).unwrap();
    assert!(!shown.look.wide && !shown.look.pressed);
    // Its fade starts a while after, and goes through its frames.
    let due = app.windows[&window].bar_due.expect("a fade that waits");
    assert!(
        due >= before + SHOW && due <= Instant::now() + SHOW,
        "{due:?}"
    );
    let mut faded = Vec::new();
    run_until(&mut fake, &mut app, |_, app| {
        let opacity = opacity(app, window);
        faded.push(opacity);
        opacity == 0.0
    });
    assert!(
        faded.iter().any(|&opacity| opacity > 0.0 && opacity < 1.0),
        "{faded:?}"
    );
    assert!(Instant::now() >= before + SHOW + FADE);
    assert_eq!(app.windows[&window].bar_due, None);
}

#[test]
fn a_reduced_motion_hides_the_bar_at_once() {
    let (mut fake, mut app, window) = started_bars(unpadded());
    fake.set_reduce_motion(true);
    fake.send(&mut app, window, wheel_up(3.0));
    run_until(&mut fake, &mut app, |_, app| opacity(app, window) == 1.0);
    let mut seen = Vec::new();
    run_until(&mut fake, &mut app, |_, app| {
        let opacity = opacity(app, window);
        seen.push(opacity);
        opacity == 0.0
    });
    assert!(seen.iter().all(|&opacity| opacity == 1.0 || opacity == 0.0));
}

#[test]
fn a_drag_of_the_knob_moves_the_view_to_the_row_that_its_place_gives() {
    let (mut fake, mut app, window) = started_bars(unpadded());
    fake.send(&mut app, window, wheel_up(3.0));
    run_until(&mut fake, &mut app, |_, app| opacity(app, window) == 1.0);
    let none = Modifiers::NONE;
    let grab = knob_middle(&fake, &app, window);
    fake.send(&mut app, window, click(true, grab, none, 1));
    assert!(app.windows[&window].bar_drag.is_some());
    // The knob dragged to the track's top: the view at the oldest row.
    let up = Point {
        y: grab.y - 10_000.0,
        ..grab
    };
    fake.send(&mut app, window, moved(up, none));
    run_until(&mut fake, &mut app, |_, app| {
        let (top, oldest, _) = view(app, window);
        top == oldest
    });
    let shown = bar(&app, window).unwrap();
    assert!(shown.look.wide && shown.look.pressed);
    // Half the way down: the row that the knob's place gives, as a line.
    let scale = scale(&fake, window) as f32;
    let knob = scrollbar::knob(shown.area, shown.rows, true, scale);
    let room = shown.area.height as f32 - 6.0 * scale - knob.height as f32;
    let start = point(
        &fake,
        window,
        (0.0, knob.y as f32 + knob.height as f32 / 2.0),
    );
    let middle = Point {
        y: start.y + f64::from(room / 2.0) / f64::from(scale),
        ..grab
    };
    fake.send(&mut app, window, moved(middle, none));
    let wanted = {
        let top = scrollbar::top_at(
            shown.area,
            shown.rows,
            scale,
            middle.y as f32 * scale - (start.y as f32 * scale - knob.y as f32),
        );
        shown.oldest + i64::try_from(top).unwrap()
    };
    run_until(&mut fake, &mut app, |_, app| view(app, window).0 == wanted);
    let (top, oldest, screen) = view(&app, window);
    assert!(
        top > oldest && top < screen,
        "{top} of {oldest} to {screen}"
    );
    // To the bottom: the screen, which the view follows again.
    let bottom = Point {
        y: grab.y + 10_000.0,
        ..grab
    };
    fake.send(&mut app, window, moved(bottom, none));
    run_until(&mut fake, &mut app, |_, app| {
        let (top, _, screen) = view(app, window);
        top == screen
    });
    assert_eq!(app.pane(window).view_offset(), 0);
    // The release ends the drag: a move then moves nothing.
    fake.send(&mut app, window, click(false, bottom, none, 1));
    assert!(app.windows[&window].bar_drag.is_none());
    fake.send(&mut app, window, moved(up, none));
    frames(&mut fake, &mut app, window);
    let (top, _, screen) = view(&app, window);
    assert_eq!(top, screen);
}

#[test]
fn a_click_on_the_track_pages_toward_it_or_jumps_there_as_the_system_says() {
    let (mut fake, mut app, window) = started_bars(unpadded());
    fake.send(&mut app, window, wheel_up(3.0));
    run_until(&mut fake, &mut app, |_, app| opacity(app, window) == 1.0);
    let none = Modifiers::NONE;
    let rows = i64::from(app.pane(window).grid().rows);
    let shown = bar(&app, window).unwrap();
    // Near the track's top, above the knob: a page up.
    let near_top = point(
        &fake,
        window,
        (
            shown.area.x as f32 + shown.area.width as f32 / 2.0,
            shown.area.y as f32 + 10.0,
        ),
    );
    let (before, _, _) = view(&app, window);
    fake.send(&mut app, window, click(true, near_top, none, 1));
    fake.send(&mut app, window, click(false, near_top, none, 1));
    run_until(&mut fake, &mut app, |_, app| {
        view(app, window).0 == before - rows
    });
    assert!(app.windows[&window].bar_drag.is_none());
    // As the system jumps to the spot clicked: there, the drag holding the
    // knob's middle.
    fake.set_scrollers(Scrollers {
        overlay: true,
        jump_to_spot: true,
    });
    fake.send(&mut app, window, click(true, near_top, none, 1));
    run_until(&mut fake, &mut app, |_, app| {
        let (top, oldest, _) = view(app, window);
        top == oldest
    });
    assert!(app.windows[&window].bar_drag.is_some());
    fake.send(&mut app, window, click(false, near_top, none, 1));
    // Option the other way: a page down, from the bottom of the track.
    let near_bottom = Point {
        y: f64::from(shown.area.y + shown.area.height as i32) / scale(&fake, window) - 3.0,
        ..near_top
    };
    let option = Modifiers {
        left_alt: true,
        ..none
    };
    let (before, oldest, _) = view(&app, window);
    assert_eq!(before, oldest);
    fake.send(&mut app, window, click(true, near_bottom, option, 1));
    fake.send(&mut app, window, click(false, near_bottom, option, 1));
    run_until(&mut fake, &mut app, |_, app| {
        view(app, window).0 == before + rows
    });
}

#[test]
fn the_pointer_over_the_bar_shows_it_wide_and_its_leaving_lets_it_fade() {
    let (mut fake, mut app, window) = started_bars(unpadded());
    let none = Modifiers::NONE;
    let shown = bar(&app, window).unwrap();
    assert_eq!(shown.look.opacity, 0.0);
    // Its area takes the pointer's moves, hidden too, and no arrow yet.
    let area = shown.area;
    let over = point(
        &fake,
        window,
        (
            area.x as f32 + area.width as f32 / 2.0,
            area.y as f32 + area.height as f32 / 2.0,
        ),
    );
    let moves = &fake.window(window).unwrap().move_areas;
    assert!(moves.iter().any(|rect| rect.contains(over)), "{moves:?}");
    let arrow = |fake: &Fake| {
        fake.window(window)
            .unwrap()
            .pointer_areas
            .iter()
            .any(|area| {
                area.shape == PointerShape::Arrow
                    && lokyr_platform_core::ContentRect {
                        origin: area.origin,
                        size: area.size,
                    }
                    .contains(over)
            })
    };
    assert!(!arrow(&fake));
    fake.send(&mut app, window, moved(over, none));
    run_until(&mut fake, &mut app, |_, app| opacity(app, window) == 1.0);
    let shown = bar(&app, window).unwrap();
    assert!(shown.look.wide && !shown.look.pressed);
    assert!(arrow(&fake));
    // Leaving it, it shows a while, thin, then fades.
    fake.send(&mut app, window, WindowEvent::MouseExited);
    frames(&mut fake, &mut app, window);
    let shown = bar(&app, window).unwrap();
    assert_eq!(shown.look.opacity, 1.0);
    assert!(!shown.look.wide);
    assert!(app.windows[&window].bar_due.is_some());
    // A move over the pane, off the bar, leaves it too.
    fake.send(&mut app, window, moved(over, none));
    frames(&mut fake, &mut app, window);
    assert!(bar(&app, window).unwrap().look.wide);
    fake.send(&mut app, window, moved(cell(0, 0), none));
    frames(&mut fake, &mut app, window);
    assert!(!bar(&app, window).unwrap().look.wide);
}

#[test]
fn the_system_or_the_config_shows_it_always_or_never() {
    // The system's bars always, as a mouse has it.
    let (mut fake, mut app, window) = started_bars(unpadded());
    fake.set_scrollers(Scrollers {
        overlay: false,
        jump_to_spot: false,
    });
    frames(&mut fake, &mut app, window);
    let shown = bar(&app, window).unwrap();
    assert_eq!(shown.look.opacity, 1.0);
    assert!(shown.look.wide);
    // The config's always, whatever the system says.
    let mut config = unpadded();
    config.window.scrollbar = lokyr_kernel::config::Scrollbar::Always;
    let (_, app, window) = started_bars(config);
    assert_eq!(opacity(&app, window), 1.0);
    // Never.
    let mut config = unpadded();
    config.window.scrollbar = lokyr_kernel::config::Scrollbar::Never;
    let (mut fake, mut app, window) = started_bars(config);
    fake.send(&mut app, window, wheel_up(3.0));
    run_until(&mut fake, &mut app, |_, app| {
        app.pane(window).view_offset() > 0
    });
    frames(&mut fake, &mut app, window);
    assert!(app.windows[&window].bars.is_empty());
    assert!(fake.window(window).unwrap().move_areas.is_empty());
}

#[test]
fn a_view_of_every_row_and_the_alternate_screen_have_none() {
    for (program, ready) in [
        ("echo ready; sleep 30", "ready"),
        // A program's own screen, over a scrollback.
        ("seq 1 200; printf '\\033[?1049hfull'; sleep 30", "full"),
    ] {
        let (mut fake, mut started, window) = drawn(unpadded(), shell(program));
        run_until(&mut fake, &mut started, |_, app| shows(app, window, ready));
        frames(&mut fake, &mut started, window);
        assert!(started.windows[&window].bars.is_empty(), "{program}");
    }
}

#[test]
fn a_search_s_matches_mark_the_bar_and_keep_it_shown() {
    let (mut fake, mut app, window) = started_bars(unpadded());
    // Matches on `line 10` and `line 150`, the second the current one.
    let total = {
        let pane = app.pane(window);
        let mut terminal = pane.terminal().unwrap();
        let oldest = terminal.row_number(0);
        let total = terminal.scrollback_rows() + usize::from(terminal.rows());
        let at = |row: i64| ((0, oldest + row), (3, oldest + row));
        let pins = [at(9), at(149)]
            .into_iter()
            .map(|(from, to)| {
                (
                    terminal.pin_cell(from).unwrap(),
                    terminal.pin_cell(to).unwrap(),
                )
            })
            .collect();
        terminal.add_matches(7, oldest, pins);
        terminal.show_matches(7, true, Some(at(149)));
        total
    };
    fake.send(&mut app, window, WindowEvent::Frame);
    frames(&mut fake, &mut app, window);
    let shown = bar(&app, window).unwrap();
    assert_eq!(shown.look.opacity, 1.0);
    let marks: Vec<(f32, bool)> = shown
        .marks
        .iter()
        .map(|mark| (mark.at, mark.current))
        .collect();
    let [(first, false), (second, true)] = marks[..] else {
        panic!("{marks:?}");
    };
    // At their rows' places along the rows, within a slot.
    let slot = 1.0 / scrollbar::mark_slots(shown.area, scale(&fake, window) as f32) as f32;
    let near = |at: f32, row: usize| (at - row as f32 / total as f32).abs() <= slot;
    assert!(near(first, 9), "{first}");
    assert!(near(second, 149), "{second}");
    // Gone, the bar hides.
    app.pane(window).terminal().unwrap().drop_matches(7);
    fake.send(&mut app, window, WindowEvent::Frame);
    frames(&mut fake, &mut app, window);
    let shown = bar(&app, window).unwrap();
    assert!(shown.marks.is_empty());
    assert_eq!(shown.look.opacity, 0.0);
}
