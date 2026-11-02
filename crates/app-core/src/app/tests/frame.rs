//! The tests of the windows' frames, their cursors' and blinking text's layers, and the grid's geometry.

use lokyr_kernel::workspace::{Direction, Environment, Layout, Pane, Settings, Side};
use lokyr_render_core::CellBatch;

use super::*;
use crate::app::frame::pane_rects;

/// A program that shows `hello world` and `second line`, then `ready`.
fn selecting_program() -> Program {
    shell("stty raw -echo; printf 'hello world\\r\\nsecond line\\r\\n'; echo ready; cat -v")
}

#[test]
fn a_window_without_a_layer_draws_nothing() {
    let (mut fake, mut app) = started();
    let window = fake.windows().next().unwrap();
    assert!(app.windows[&window].surface.is_none());
    let size = WindowSize::new(
        Size {
            width: 900.0,
            height: 600.0,
        },
        1.0,
    );
    assert!(fake.send(&mut app, window, WindowEvent::Resized(size)));
    assert!(fake.send(&mut app, window, WindowEvent::ScaleChanged(size)));
    fake.send_app(&mut app, AppEvent::AppearanceChanged(Appearance::Dark));
    assert_eq!(fake.windows().count(), 1);
}

#[test]
fn a_wake_asks_for_the_frames_of_the_panes_that_woke_it() {
    let (mut fake, mut app) = started();
    let window = fake.windows().next().unwrap();
    if fake.window(window).unwrap().frame_requested {
        fake.send(&mut app, window, WindowEvent::Frame);
    }
    // A wake of nothing that changed asks for nothing.
    fake.send_app(&mut app, AppEvent::Wake);
    assert!(!fake.window(window).unwrap().frame_requested);
    let pane = app.focused(window).unwrap();
    app.pane_wakes[&pane].wake();
    fake.send_app(&mut app, AppEvent::Wake);
    assert!(fake.window(window).unwrap().frame_requested);
    assert!(fake.send(&mut app, window, WindowEvent::Frame));
    assert!(!fake.window(window).unwrap().frame_requested);
}

#[test]
fn the_focused_window_s_cursor_blinks_on_its_timer() {
    let (mut fake, mut app, window) = focused();
    let timers: Vec<_> = fake.timers().collect();
    assert_eq!(timers.len(), 1);
    let (timer, at) = timers[0];
    assert_eq!(app.blink_timer, Some((timer, at)));
    // The timer asks the window for the frame of the change.
    assert_eq!(fake.fire_timers(&mut app, at), 1);
    assert_eq!(app.blink_timer, None);
    assert!(fake.window(window).unwrap().frame_requested);
    // Off, then on, as the clock goes.
    let now = at;
    assert_eq!(
        app.cursor(&mut fake, window, now).map(|(shape, _)| shape),
        None
    );
    assert_eq!(
        app.cursor(&mut fake, window, now + cursor::BLINK_INTERVAL)
            .map(|(shape, _)| shape),
        Some(CursorShape::Block)
    );
    assert_eq!(fake.timers().count(), 1);
}

#[test]
fn blinking_stops_on_after_its_timeout() {
    let (mut fake, mut app, window) = focused();
    let later = Instant::now() + cursor::BLINK_TIMEOUT;
    assert_eq!(
        app.cursor(&mut fake, window, later).map(|(shape, _)| shape),
        Some(CursorShape::Block)
    );
    assert!(!app.windows[&window].blinking);
    // Its last timer asks for no frame.
    let fired = fake.fire_timers(&mut app, later + cursor::BLINK_TIMEOUT);
    assert_eq!(fired, 1);
    assert!(!fake.window(window).unwrap().frame_requested);
    assert_eq!(fake.timers().count(), 0);
}

#[test]
fn a_window_without_the_focus_shows_a_hollow_block_that_does_not_blink() {
    let (mut fake, mut app, window) = focused();
    assert!(fake.send(&mut app, window, WindowEvent::Focused(false)));
    let now = Instant::now();
    for millis in [0, 600, 1200] {
        let at = now + std::time::Duration::from_millis(millis);
        assert_eq!(
            app.cursor(&mut fake, window, at).map(|(shape, _)| shape),
            Some(CursorShape::Hollow)
        );
    }
    assert!(!app.windows[&window].blinking);
}

#[test]
fn a_cursor_on_its_layer_blinks_without_a_timer_but_its_end() {
    let mut fake = Fake::default();
    let mut config = Config::default();
    config.cursor.smooth_blink = cursor::SmoothBlink::Breathing;
    let mut app = app(config, quiet());
    app.layers = true;
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, WindowEvent::Focused(true));
    fake.send(&mut app, window, WindowEvent::Frame);
    let now = Instant::now();
    let (shape, blink) = app.cursor(&mut fake, window, now).unwrap();
    assert_eq!(shape, CursorShape::Block);
    // Breathing's, from the clock's start, which the layer runs.
    let blink = blink.expect("a blink");
    let millis = Duration::from_millis;
    assert_eq!(
        (blink.visible, blink.fade_out, blink.hidden, blink.fade_in),
        (millis(500), millis(900), millis(500), millis(900))
    );
    assert_eq!(
        (blink.fade_out_curve, blink.fade_in_curve),
        (Easing::EaseInOut, Easing::EaseInOut)
    );
    let since = app.windows[&window].blink.since();
    assert_eq!(blink.start, since);
    // One timer alone, of the blinking's end.
    let timers: Vec<Instant> = fake.timers().map(|(_, at)| at).collect();
    assert_eq!(timers, [since + cursor::BLINK_TIMEOUT]);
    // At its end, a frame, and the cursor steady, without a timer.
    assert_eq!(fake.fire_timers(&mut app, timers[0]), 1);
    assert!(fake.window(window).unwrap().frame_requested);
    let (_, blink) = app.cursor(&mut fake, window, timers[0]).unwrap();
    assert_eq!(blink, None);
    assert_eq!(fake.timers().count(), 0);
    // Off switches, on and off for the interval each.
    app.config.cursor.smooth_blink = cursor::SmoothBlink::Off;
    app.windows
        .get_mut(&window)
        .unwrap()
        .blink
        .restart(Instant::now());
    let (_, blink) = app.cursor(&mut fake, window, Instant::now()).unwrap();
    let blink = blink.expect("a blink");
    assert_eq!((blink.visible, blink.hidden), (millis(500), millis(500)));
    assert_eq!(
        (blink.fade_out, blink.fade_in),
        (Duration::ZERO, Duration::ZERO)
    );
    // Without the focus, a hollow block, steady.
    fake.send(&mut app, window, WindowEvent::Focused(false));
    let (shape, blink) = app.cursor(&mut fake, window, Instant::now()).unwrap();
    assert_eq!((shape, blink), (CursorShape::Hollow, None));
}

#[test]
fn a_new_font_has_the_layers_draw_their_cells_again() {
    let mut drawing = Drawing::new(FontConfig::default()).unwrap();
    assert_eq!(drawing.fonts, 1);
    drawing.set_font(FontConfig::default()).unwrap();
    assert_eq!(drawing.fonts, 2);
}

#[test]
fn blinking_text_on_its_layer_blinks_without_a_timer_but_its_end() {
    // A cursor that does not blink, and a smooth blink, which the text
    // does not take.
    let mut config = Config::default();
    config.cursor.blink = false;
    config.cursor.smooth_blink = cursor::SmoothBlink::Breathing;
    let mut fake = Fake::default();
    let mut app = app(config, shell("printf '\\033[5mblink\\033[0m'; sleep 10"));
    app.layers = true;
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, WindowEvent::Focused(true));
    run_until(&mut fake, &mut app, |_, app| row(app, window, 0) == "blink");
    fake.send(&mut app, window, WindowEvent::Frame);
    // The grid hides it, and its layer switches it at the interval, from
    // the clock's start.
    let (shown, blink) = app.text_blink(&mut fake, window, Instant::now());
    assert!(!shown);
    let blink = blink.expect("a blink");
    let since = app.windows[&window].blink.since();
    assert_eq!(blink.start, since);
    let interval = cursor::BLINK_INTERVAL;
    assert_eq!((blink.visible, blink.hidden), (interval, interval));
    assert_eq!(
        (blink.fade_out, blink.fade_in),
        (Duration::ZERO, Duration::ZERO)
    );
    // One timer alone, of the blinking's end; at its end, a frame, and
    // the text in the grid, steady.
    let (timer, at) = app.blink_timer.expect("the blinking's timer");
    assert_eq!(at, since + cursor::BLINK_TIMEOUT);
    fake.send_app(&mut app, AppEvent::Timer(timer));
    assert!(fake.window(window).unwrap().frame_requested);
    assert_eq!(app.text_blink(&mut fake, window, at), (true, None));
    assert_eq!(app.blink_timer, None);
    // Without the focus, steady.
    app.windows
        .get_mut(&window)
        .unwrap()
        .blink
        .restart(Instant::now());
    fake.send(&mut app, window, WindowEvent::Focused(false));
    let unfocused = app.text_blink(&mut fake, window, Instant::now());
    assert_eq!(unfocused, (true, None));
}

#[test]
fn the_configuration_gives_the_cursor_s_shape_and_blinking() {
    let mut fake = Fake::default();
    let mut config = Config::default();
    config.cursor = CursorConfig {
        shape: cursor::Shape::Bar,
        blink: false,
        ..CursorConfig::default()
    };
    let mut app = app(config, quiet());
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let half = Instant::now() + cursor::BLINK_INTERVAL + cursor::BLINK_INTERVAL / 2;
    assert_eq!(
        app.cursor(&mut fake, window, half).map(|(shape, _)| shape),
        Some(CursorShape::Bar)
    );
    assert_eq!(fake.timers().count(), 0);
}

#[test]
fn a_window_that_does_not_show_draws_nothing_until_it_shows() {
    let (mut fake, mut app, window) = started_with(shell(
        "stty raw -echo; echo ready; head -c 1 >/dev/null; echo more; cat",
    ));
    fake.send(&mut app, window, WindowEvent::Frame);
    // Hidden and shown again without a frame asked meanwhile, it draws
    // anew, as it let its drawables go.
    fake.send(&mut app, window, WindowEvent::Occluded(true));
    fake.send(&mut app, window, WindowEvent::Occluded(false));
    assert!(fake.send(&mut app, window, WindowEvent::Frame));
    assert!(!fake.window(window).unwrap().frame_requested);
    fake.send(&mut app, window, WindowEvent::Occluded(true));
    fake.send(&mut app, window, WindowEvent::Occluded(false));
    assert!(fake.window(window).unwrap().frame_requested);
    fake.send(&mut app, window, WindowEvent::Frame);
    fake.send(&mut app, window, WindowEvent::Occluded(true));
    assert!(app.windows[&window].occluded);
    // Output asks for a frame, which waits.
    fake.send(&mut app, window, key());
    let pause = Instant::now() + Duration::from_millis(300);
    run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
    let shown = fake.window(window).unwrap();
    assert!(!shown.frame_requested && shown.frame_held);
    // Shown again, it draws.
    fake.send(&mut app, window, WindowEvent::Occluded(false));
    assert!(!app.windows[&window].occluded);
    assert!(fake.window(window).unwrap().frame_requested);
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "more"));
}

#[test]
fn the_grid_starts_below_the_titlebar() {
    let mut fake = Fake::default();
    fake.set_bar(14.0);
    let mut app = app(unpadded(), selecting_program());
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
    // The size asked for is below the bar, whose height the grid leaves.
    let size = fake.window(window).unwrap().size;
    assert_eq!(size.points.height, 510.0);
    let grid = app.pane(window).grid();
    assert_eq!((grid.columns, grid.rows), (200, 62));
    // The cells start below it.
    let below = |col: u16, row: u16| Point {
        y: cell(col, row).y + 14.0,
        ..cell(col, row)
    };
    fake.send(
        &mut app,
        window,
        click(true, below(8, 0), Modifiers::NONE, 2),
    );
    fake.send(
        &mut app,
        window,
        click(false, below(8, 0), Modifiers::NONE, 2),
    );
    assert_eq!(selected_text(&app, window).as_deref(), Some("world"));
    fake.send(
        &mut app,
        window,
        click(true, below(3, 1), Modifiers::NONE, 2),
    );
    fake.send(
        &mut app,
        window,
        click(false, below(3, 1), Modifiers::NONE, 2),
    );
    assert_eq!(selected_text(&app, window).as_deref(), Some("second"));
    // Over the bar, the first row.
    fake.send(
        &mut app,
        window,
        click(true, cell(8, 0), Modifiers::NONE, 2),
    );
    fake.send(
        &mut app,
        window,
        click(false, cell(8, 0), Modifiers::NONE, 2),
    );
    assert_eq!(selected_text(&app, window).as_deref(), Some("world"));
    // An opaque bar covers the bar's pixels, of the background.
    let background = Color::from_rgb8(1, 2, 3);
    let Some(Item::Rect { rect, color }) = bar_item(size, background) else {
        panic!("a bar");
    };
    assert_eq!((rect.x, rect.y, rect.width, rect.height), (0, 0, 1600, 28));
    assert_eq!(color, background.with_alpha(1.0));
    assert!(bar_item(points(400.0, 300.0), background).is_none());
}

#[test]
fn a_window_takes_the_appearance_of_its_background_where_it_is_not_the_app_s() {
    let gray = |level: u8| Color::from_rgb8(level, level, level);
    assert_eq!(appearance_over(gray(250), Appearance::Light), None);
    assert_eq!(
        appearance_over(gray(250), Appearance::Dark),
        Some(Appearance::Light)
    );
    assert_eq!(
        appearance_over(gray(30), Appearance::Light),
        Some(Appearance::Dark)
    );
    assert_eq!(appearance_over(gray(30), Appearance::Dark), None);
    // Past the luminance where black and white contrast alike, about
    // 118 of 255.
    assert_eq!(
        appearance_over(gray(124), Appearance::Dark),
        Some(Appearance::Light)
    );
    assert_eq!(
        appearance_over(gray(112), Appearance::Light),
        Some(Appearance::Dark)
    );
}

#[test]
fn the_grid_fits_the_window_less_its_padding() {
    let (mut fake, mut app, path) = with_file("padding", "[window]\npadding-x = 10\n");
    // 1600 by 1000 pixels, less 20 at the left and the right and 8 at
    // the top and the bottom.
    let grid = app.grid(points(800.0, 500.0));
    assert_eq!((grid.columns, grid.rows), (195, 61));
    // A window smaller than its padding keeps a cell.
    let grid = app.grid(points(15.0, 5.0));
    assert_eq!((grid.columns, grid.rows), (1, 1));
    // The input method's candidates go at the cursor's cell, within the
    // padding.
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, WindowEvent::Frame);
    let (origin, _) = fake.window(window).unwrap().text_area.unwrap();
    assert_eq!((origin.x, origin.y), (10.0, 4.0));
    // A reloaded padding gives the grid that it leaves: of a window of
    // 100 columns and 30 rows with their padding, 420 points by 248.
    std::fs::write(&path, "[window]\npadding-x = 0\npadding-y = 0\n").unwrap();
    change(&mut fake, &mut app);
    assert!(fake.window(window).unwrap().frame_requested);
    fake.send(&mut app, window, WindowEvent::Frame);
    let snapshot = app.pane(window).snapshot();
    assert_eq!((snapshot.cols(), snapshot.rows()), (105, 31));
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn a_translucent_background_makes_the_window_translucent() {
    let (mut fake, mut reloaded, path) = with_file("translucent", "");
    let window = fake.windows().next().unwrap();
    let backdrop = |fake: &mut Fake, app: &mut App| {
        fake.send(app, window, WindowEvent::Frame);
        fake.window(window)
            .unwrap()
            .translucent
            .map(|backdrop| (backdrop.color, backdrop.blur))
    };
    assert_eq!(backdrop(&mut fake, &mut reloaded), None);
    std::fs::write(
        &path,
        "[window]\nbackground-opacity = 0.9\nbackground-blur = 20\n",
    )
    .unwrap();
    change(&mut fake, &mut reloaded);
    // The terminal's default background, of the light variant, with the
    // opacity.
    let light = reloaded.colors(Appearance::Light).background;
    let rgb = |rgb: lokyr_vt_core::page::Rgb| {
        [rgb.r, rgb.g, rgb.b].map(|component| f64::from(f32::from(component) / 255.0))
    };
    let [red, green, blue] = rgb(light);
    let opacity = f64::from(0.9_f32);
    assert_eq!(
        backdrop(&mut fake, &mut reloaded),
        Some(([red, green, blue, opacity], 20.0))
    );
    // The dark variant's background takes its place.
    fake.send_app(&mut reloaded, AppEvent::AppearanceChanged(Appearance::Dark));
    let [red, green, blue] = rgb(reloaded.colors(Appearance::Dark).background);
    assert_eq!(
        backdrop(&mut fake, &mut reloaded),
        Some(([red, green, blue, opacity], 20.0))
    );
    // A blur alone, of an opaque background, leaves the window opaque.
    std::fs::write(&path, "[window]\nbackground-blur = 20\n").unwrap();
    change(&mut fake, &mut reloaded);
    assert_eq!(backdrop(&mut fake, &mut reloaded), None);
    std::fs::write(&path, "[window]\nbackground-opacity = 0.5\n").unwrap();
    change(&mut fake, &mut reloaded);
    let blur = backdrop(&mut fake, &mut reloaded).map(|(_, blur)| blur);
    assert_eq!(blur, Some(0.0));
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
    // A window opens as the config says.
    let mut config = Config::default();
    config.window.background_opacity = 0.8.try_into().unwrap();
    config.window.background_blur = 12.0.try_into().unwrap();
    let mut fake = Fake::default();
    let mut app = app(config, quiet());
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    let opened = fake.window(window).unwrap().translucent;
    assert_eq!(opened.map(|backdrop| backdrop.blur), Some(12.0));
}

#[test]
fn a_resize_shows_the_grid_s_size_until_a_moment_after_it_ends() {
    let (mut fake, mut app) = started();
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, WindowEvent::Frame);
    let label = |app: &App| app.windows[&window].size_label;
    // The first frame shows none.
    assert_eq!(label(&app), None);
    // A live resize that changes the grid shows it, while it lasts.
    fake.send(&mut app, window, WindowEvent::LiveResize(true));
    assert!(app.windows[&window].live_resize);
    fake.send(&mut app, window, WindowEvent::Resized(points(400.0, 300.0)));
    let until = label(&app).expect("the label");
    let (timer, at) = app.label_timer.expect("its timer");
    assert_eq!(at, until);
    // Its timer, within the live resize, leaves it.
    fake.send_app(&mut app, AppEvent::Timer(timer));
    assert_eq!(label(&app), Some(until));
    // Its end keeps it a moment more, then takes it away, with a frame.
    let ended = Instant::now();
    fake.send(&mut app, window, WindowEvent::LiveResize(false));
    let until = label(&app).expect("the label");
    assert!(until >= ended + size_label::HOLD);
    let (_, at) = app.label_timer.expect("its timer");
    fake.send(&mut app, window, WindowEvent::Frame);
    assert!(!fake.window(window).unwrap().frame_requested);
    // The cursor's blinking has a timer of its own before it.
    assert!(fake.fire_timers(&mut app, at) >= 1);
    assert_eq!(label(&app), None);
    assert_eq!(app.label_timer, None);
    assert!(fake.window(window).unwrap().frame_requested);
    // A resize of the same grid shows none.
    fake.send(&mut app, window, WindowEvent::Resized(points(401.0, 301.0)));
    assert_eq!(label(&app), None);
}

#[test]
fn a_live_resize_draws_once_a_refresh() {
    let (mut fake, mut app) = started();
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, WindowEvent::LiveResize(true));
    // A frame drawn within this refresh: the step waits for the next.
    app.windows.get_mut(&window).unwrap().drawn_at = Some(Instant::now());
    let before = app.pane(window).grid();
    fake.send(&mut app, window, WindowEvent::Frame);
    assert!(!fake.window(window).unwrap().frame_requested);
    fake.send(&mut app, window, WindowEvent::Resized(points(400.0, 300.0)));
    assert_eq!(app.pane(window).grid(), before);
    assert!(fake.window(window).unwrap().frame_requested);
    // A refresh later, it draws at once.
    let earlier = Instant::now() - Duration::from_millis(50);
    app.windows.get_mut(&window).unwrap().drawn_at = Some(earlier);
    fake.send(&mut app, window, WindowEvent::Resized(points(400.0, 300.0)));
    assert_ne!(app.pane(window).grid(), before);
}

#[test]
fn a_synchronized_update_shows_whole() {
    let mut fake = Fake::default();
    let script = "printf 'before\\r\\n'; sleep 0.3; printf '\\033[?2026hhalf'; sleep 0.5; \
                  printf ' done\\033[?2026l'; sleep 10";
    let mut app = app(Config::default(), shell(script));
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    let mut seen = Vec::new();
    run_until(&mut fake, &mut app, |_, app| {
        let shown = row(app, window, 1);
        if seen.last() != Some(&shown) {
            seen.push(shown.clone());
        }
        shown == "half done"
    });
    assert!(!seen.iter().any(|shown| shown == "half"), "{seen:?}");
    assert_eq!(row(&app, window, 0), "before");
}

#[test]
fn output_starts_the_blinking_again() {
    let mut fake = Fake::default();
    let mut app = app(Config::default(), shell("sleep 0.3; printf x; sleep 10"));
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    let focused = Instant::now();
    fake.send(&mut app, window, WindowEvent::Focused(true));
    run_until(&mut fake, &mut app, |_, app| row(app, window, 0) == "x");
    // The output came at least 0.3 s after the focus: 15.1 s after the
    // focus, the cursor still blinks, as its timeout counts from it.
    let later = focused + cursor::BLINK_TIMEOUT + Duration::from_millis(100);
    app.cursor(&mut fake, window, later).map(|(shape, _)| shape);
    assert!(app.windows[&window].blinking);
}

#[test]
fn blinking_text_keeps_the_blinking_s_timer_in_the_focused_window() {
    // A cursor that does not blink, over text that does.
    let config = |text_blink: bool| {
        let mut config = Config::default();
        config.cursor.blink = false;
        config.text.blink = text_blink;
        config
    };
    let script = "printf '\\033[5mblink\\033[0m'; sleep 10";
    for (text_blink, focused, timer) in [
        (true, true, true),
        (false, true, false),
        (true, false, false),
    ] {
        let mut fake = Fake::default();
        let mut app = app(config(text_blink), shell(script));
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        fake.send(&mut app, window, WindowEvent::Focused(focused));
        run_until(&mut fake, &mut app, |_, app| row(app, window, 0) == "blink");
        fake.send(&mut app, window, WindowEvent::Frame);
        assert_eq!(app.blink_timer.is_some(), timer, "{text_blink} {focused}");
        assert_eq!(app.windows[&window].blinking, timer);
    }
}

#[test]
fn a_pane_s_output_asks_for_its_own_window_s_frame_alone() {
    let (mut fake, mut app, first) = started_running(idle());
    fake.send_app(&mut app, AppEvent::Command(NEW_WINDOW.to_owned()));
    let second = fake.windows().find(|&window| window != first).unwrap();
    run_until(&mut fake, &mut app, |_, app| {
        screen(app, second)
            .iter()
            .any(|line| line.contains("ready"))
    });
    for window in [first, second] {
        if fake.window(window).unwrap().frame_requested {
            fake.send(&mut app, window, WindowEvent::Frame);
        }
    }
    // The second's shell echoes a key: its window's frame, not the
    // first's.
    fake.send(&mut app, second, typed(KeyCode::KeyX, "x", false));
    let start = Instant::now();
    while !screen(&app, second).iter().any(|line| line.ends_with('x')) {
        assert!(start.elapsed() < Duration::from_secs(10), "no echo");
        fake.deliver_wake(&mut app);
        assert!(!fake.window(first).unwrap().frame_requested);
        if fake.window(second).unwrap().frame_requested {
            fake.send(&mut app, second, WindowEvent::Frame);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn a_frame_with_nothing_new_presents_nothing() {
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let mut fake = Fake::default();
    let mut app = App::new(Some(drawing), unpadded(), None).with_program(idle());
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(fakes::Surface::default());
    run_until(&mut fake, &mut app, |_, app| {
        screen(app, window)
            .iter()
            .any(|line| line.contains("ready"))
    });
    let drawn = |app: &App| app.windows[&window].drawn_at;
    fake.send(&mut app, window, WindowEvent::Frame);
    let first = drawn(&app);
    assert!(first.is_some());
    // The same again: nothing presents, and the surface keeps it.
    fake.send(&mut app, window, WindowEvent::Frame);
    assert_eq!(drawn(&app), first);
    // The focus's cursor is new.
    fake.send(&mut app, window, WindowEvent::Focused(true));
    fake.send(&mut app, window, WindowEvent::Frame);
    assert_ne!(drawn(&app), first);
}

#[test]
fn a_frame_and_a_key_lock_the_pane_once_each() {
    let (mut fake, mut app, window) = started_running(idle());
    let locks = |app: &App| app.pane(window).locks();
    // A frame copies the screen, and reads the rest from the copy.
    let before = locks(&app);
    fake.send(&mut app, window, WindowEvent::Frame);
    assert_eq!(locks(&app) - before, 1);
    // A key resolves in the frame's contexts, and locks to write.
    let before = locks(&app);
    fake.send(&mut app, window, typed(KeyCode::KeyX, "x", false));
    assert_eq!(locks(&app) - before, 1);
    // Command, down alone, as the system tells it before its keys, which
    // a plugin that underlines the link under the pointer looks at.
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::SuperLeft, "", CMD, true),
    );
    let before = locks(&app);
    fake.send(&mut app, window, typed(KeyCode::KeyX, "x", true));
    assert_eq!(
        locks(&app) - before,
        0,
        "Command's key, which the program does not take"
    );
}

#[test]
fn a_tab_s_panes_share_its_room_in_whole_cells() {
    let id = |n| lokyr_kernel::workspace::PaneId::of(n);
    let area = Rect {
        x: 4,
        y: 10,
        width: 800,
        height: 400,
    };
    let cell = (8, 16);
    // One pane takes the whole room.
    let one = Layout::Leaf(id(1));
    assert_eq!(pane_rects(&one, None, area, cell, 2), [(id(1), area)]);
    // Three split alike: a third each, past the dividers, the first parts
    // of whole cells.
    let mut three = Layout::Leaf(id(1));
    three.split(id(1), Direction::Horizontal, id(2), Side::Second);
    three.split(id(2), Direction::Horizontal, id(3), Side::Second);
    let rooms = pane_rects(&three, None, area, cell, 2);
    let widths: Vec<u32> = rooms.iter().map(|(_, rect)| rect.width).collect();
    assert_eq!(widths, [264, 264, 268]);
    let lefts: Vec<i32> = rooms.iter().map(|(_, rect)| rect.x).collect();
    assert_eq!(lefts, [4, 4 + 264 + 2, 4 + 264 + 2 + 264 + 2]);
    assert!(
        rooms
            .iter()
            .all(|(_, rect)| rect.y == 10 && rect.height == 400)
    );
    // Over each other, the same way down.
    let mut stacked = Layout::Leaf(id(1));
    stacked.split(id(1), Direction::Vertical, id(2), Side::Second);
    let rooms = pane_rects(&stacked, None, area, cell, 2);
    assert_eq!(
        rooms
            .iter()
            .map(|(_, rect)| (rect.y, rect.height))
            .collect::<Vec<_>>(),
        [(10, 192), (10 + 192 + 2, 206)]
    );
    // A zoomed pane takes the whole room.
    assert_eq!(
        pane_rects(&three, Some(id(2)), area, cell, 2),
        [(id(2), area)]
    );
    // A user's ratio, within the room.
    let mut ratio = stacked.clone();
    ratio.set_ratio(&[], Some(lokyr_kernel::workspace::Ratio::new(0.25)));
    let rooms = pane_rects(&ratio, None, area, cell, 2);
    assert_eq!(rooms[0].1.height, 96);
}

#[test]
fn a_window_of_two_panes_draws_both_each_in_its_room() {
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let mut fake = Fake::default();
    let mut app = App::new(Some(drawing), unpadded(), None).with_program(idle());
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(fakes::Surface::default());
    fake.send(&mut app, window, WindowEvent::Focused(true));
    // The tests' layout: a second pane beside the first.
    let first = app.focused(window).unwrap();
    let grid = app.pane(window).grid();
    let pane = Pane::spawn(
        &idle(),
        &Environment::default(),
        &Settings::default(),
        grid,
        &lokyr_vt_core::terminal::Colors::default(),
        &app.texts,
        || {},
    );
    let second = app
        .workspace
        .split(first, Direction::Horizontal, pane, Side::Second)
        .unwrap();
    let drawn = |fake: &mut Fake, app: &mut App| {
        fake.send(app, window, WindowEvent::Frame);
        let views = app.windows[&window].views.clone();
        let shown = app.windows[&window].shown.as_ref().expect("a frame");
        let batches: Vec<CellBatch> = shown
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Cells(batch) => Some(batch.clone()),
                _ => None,
            })
            .collect();
        (views, batches)
    };
    let (views, batches) = drawn(&mut fake, &mut app);
    assert_eq!(
        views.iter().map(|view| view.pane).collect::<Vec<_>>(),
        [first, second]
    );
    // Each pane's program has its grid, and its cells are drawn in its
    // room, the keyboard's pane with its cursor, the other's hollow.
    for (view, batch) in views.iter().zip(&batches) {
        assert_eq!(app.workspace.pane(view.pane).unwrap().grid(), view.grid);
        assert_eq!((batch.x, batch.y), (view.rect.x, view.rect.y));
        assert_eq!(
            (batch.columns, batch.rows),
            (view.grid.columns, view.grid.rows)
        );
    }
    assert!(views[1].rect.x > views[0].rect.x + i32::try_from(views[0].rect.width).unwrap());
    let shape = |batch: &CellBatch| batch.cursor.map(|cursor| cursor.shape);
    assert!(shape(&batches[0]).is_some_and(|shape| shape != CursorShape::Hollow));
    assert_eq!(shape(&batches[1]), Some(CursorShape::Hollow));
    // The window resized, each pane's grid follows.
    let before = views;
    fake.send(&mut app, window, WindowEvent::Resized(points(300.0, 200.0)));
    let (views, _) = drawn(&mut fake, &mut app);
    for (view, old) in views.iter().zip(&before) {
        assert!(view.grid.columns < old.grid.columns, "{view:?} {old:?}");
        assert_eq!(app.workspace.pane(view.pane).unwrap().grid(), view.grid);
    }
}
