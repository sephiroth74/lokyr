//! The commands that the shell marks and the panes' views in the app: a
//! plugin hears each command's start and end, reads their marks, and
//! follows its pane's view through its scroll, output and the scrollback's
//! limit; and the command's header over the pane's top, of `lokyr/blocks`
//! and of `lokyr/command-header` while that one is off.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use lokyr_kernel::commands::{Category, Command};
use lokyr_kernel::plugins::host::{PaneId, Scroll, ShellCommand, Viewport};
use lokyr_kernel::plugins::ui::{PanelId, ViewId};
use lokyr_kernel::plugins::{Cx, Plugin};
use lokyr_text_core::FontConfig;

use super::*;
use crate::fakes;

/// What the plugin heard and read.
#[derive(Default)]
struct Followed {
    /// The commands' starts, of their ids and of no status, and their
    /// ends, of their ids and their statuses, in their order.
    commands_told: Vec<(u64, Option<Option<i32>>)>,
    /// The views that it heard of.
    viewports: Vec<Viewport>,
    /// The pane of the views.
    pane: Option<PaneId>,
    /// The commands that its last read gave.
    commands: Vec<ShellCommand>,
}

/// A plugin that follows its pane's commands and view: its command
/// `follower.read` reads every command that the pane keeps, and
/// `follower.up` scrolls the view up by 10 rows.
struct Follower(Rc<RefCell<Followed>>);

impl Plugin for Follower {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        for name in ["follower.read", "follower.up"] {
            cx.register_command(Command {
                name: name.to_owned(),
                title: name.to_owned(),
                category: Category::View,
                capabilities: Vec::new(),
                params: Vec::new(),
                when: None,
            })?;
        }
        cx.subscribe(&[
            plugins::VIEWPORT_CHANGED,
            plugins::COMMAND_STARTED,
            plugins::COMMAND_ENDED,
        ])
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, _: Args) -> Result<(), Error> {
        let pane = self.0.borrow().pane.expect("the pane of a view");
        if name == "follower.read" {
            // Their marks, of no times nor folder, which the clock and the
            // shell give.
            let commands = cx.shell_commands(pane, i64::MIN, i64::MAX)?;
            self.0.borrow_mut().commands = commands
                .into_iter()
                .map(|command| ShellCommand {
                    started: None,
                    ended: None,
                    folder: None,
                    host: None,
                    ..command
                })
                .collect();
            return Ok(());
        }
        cx.scroll(pane, Scroll::Up(10))
    }

    fn on_event(&mut self, _: &mut Cx<'_>, events: &[plugins::Event]) -> Result<(), Error> {
        let mut followed = self.0.borrow_mut();
        for event in events {
            match *event {
                plugins::Event::ViewportChanged { pane, ref viewport } => {
                    followed.pane = Some(pane);
                    followed.viewports.push(viewport.clone());
                }
                plugins::Event::CommandStarted { command, .. } => {
                    followed.commands_told.push((command, None));
                }
                plugins::Event::CommandEnded {
                    command, status, ..
                } => followed.commands_told.push((command, Some(status))),
                _ => {}
            }
        }
        Ok(())
    }
}

/// Runs `follower.read` until the commands that it reads hold `done`.
fn read_until(
    fake: &mut Fake,
    app: &mut App,
    followed: &Rc<RefCell<Followed>>,
    done: impl Fn(&[ShellCommand]) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        fake.send_app(app, AppEvent::Command("follower.read".to_owned()));
        if done(&followed.borrow().commands) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "read {:?}",
            followed.borrow().commands
        );
        fake.deliver_wake(app);
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// A command's marks, each of its column and its row's number.
fn marks(
    id: u64,
    prompt: Option<i64>,
    output: Option<i64>,
    end: Option<i64>,
    status: Option<i32>,
) -> ShellCommand {
    ShellCommand {
        id,
        prompt: prompt.map(|row| (0, row)),
        input: prompt.map(|row| (2, row)),
        output: output.map(|row| (0, row)),
        end: end.map(|row| (0, row)),
        status,
        ..ShellCommand::default()
    }
}

#[test]
fn a_plugin_follows_three_commands_and_its_pane_s_view_through_output_and_the_scrollback_s_limit() {
    let directory = std::env::temp_dir().join(format!("lokyr-app-marks-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let (next, more) = (directory.join("next"), directory.join("more"));
    // Two commands, `one` and `two`, of a line of output each and the
    // statuses 0 and 1; once `next` is there, `seq`, of 30 000 lines and the
    // status 2, which take the first two past the scrollback's limit; once
    // `more` is there, 50 lines more.
    let script = format!(
        concat!(
            "m() {{ printf '\\033]133;%s\\007' \"$1\"; }}\n",
            "c() {{ m A; printf '$ '; m B; printf '%s\\n' \"$1\"; m C; }}\n",
            "c one; echo one; m 'D;0'\n",
            "c two; echo two; m 'D;1'\n",
            "m A; printf '$ '\n",
            "while [ ! -e '{next}' ]; do sleep 0.02; done\n",
            "m B; echo seq; m C; seq 1 30000; m 'D;2'\n",
            "m A; printf '$ '\n",
            "while [ ! -e '{more}' ]; do sleep 0.02; done\n",
            "seq 1 50; sleep 30\n",
        ),
        next = next.display(),
        more = more.display(),
    );
    let followed = Rc::new(RefCell::new(Followed::default()));
    let mut config = Config::parse("[scrollback]\nlimit = 1\n", Path::new("config.toml")).unwrap();
    config.window = unpadded().window;
    let mut fake = Fake::default();
    let mut app = app(config, shell(&script))
        .with_plugin("lokyr/follower", Box::new(Follower(Rc::clone(&followed))));
    fake.start(&mut app);
    let told = |followed: &Rc<RefCell<Followed>>| followed.borrow().commands_told.clone();
    // The first two commands start and end, and the third's prompt shows.
    run_until(&mut fake, &mut app, |_, _| told(&followed).len() == 4);
    assert_eq!(
        told(&followed),
        [(1, None), (1, Some(Some(0))), (2, None), (2, Some(Some(1)))]
    );
    read_until(&mut fake, &mut app, &followed, |commands| {
        commands.len() == 3
    });
    assert_eq!(
        followed.borrow().commands,
        [
            marks(1, Some(0), Some(1), Some(2), Some(0)),
            marks(2, Some(2), Some(3), Some(4), Some(1)),
            ShellCommand {
                id: 3,
                prompt: Some((0, 4)),
                ..ShellCommand::default()
            },
        ]
    );
    // The third runs and ends: the first two went with the rows that the
    // scrollback's limit took, and the third's prompt and output, whose end
    // stays, below its 30 000 lines.
    std::fs::write(&next, "").unwrap();
    run_until(&mut fake, &mut app, |_, _| told(&followed).len() == 6);
    assert_eq!(told(&followed)[4..], [(3, None), (3, Some(Some(2)))]);
    let end = 4 + 1 + 30_000;
    read_until(&mut fake, &mut app, &followed, |commands| {
        commands.last().is_some_and(|last| last.id == 4)
    });
    assert_eq!(
        followed.borrow().commands,
        [
            marks(3, None, None, Some(end), Some(2)),
            ShellCommand {
                id: 4,
                prompt: Some((0, end)),
                input: None,
                ..ShellCommand::default()
            },
        ]
    );
    // The view follows the screen down to the last prompt, the oldest row
    // raised past the first, as the limit took the rows above.
    let last =
        |followed: &Rc<RefCell<Followed>>| followed.borrow().viewports.last().unwrap().clone();
    let shows_end = |viewport: Viewport| {
        viewport.screen <= end && end < viewport.screen + i64::from(viewport.rows)
    };
    run_until(&mut fake, &mut app, |_, _| shows_end(last(&followed)));
    let at_end = last(&followed);
    assert_eq!(at_end.top, at_end.screen, "{at_end:?}");
    assert!(at_end.oldest > 0, "{at_end:?}");
    assert!(!at_end.alternate);
    // Scrolled up, it is 10 rows above the screen.
    let heard = followed.borrow().viewports.len();
    fake.send_app(&mut app, AppEvent::Command("follower.up".to_owned()));
    run_until(&mut fake, &mut app, |_, _| {
        last(&followed).top == at_end.screen - 10
    });
    let up = last(&followed);
    assert_eq!(
        (up.screen, up.rows, up.epoch),
        (at_end.screen, at_end.rows, at_end.epoch)
    );
    assert_eq!(followed.borrow().viewports.len(), heard + 1, "once a frame");
    // Output scrolls the screen under it, and the view keeps its rows.
    std::fs::write(&more, "").unwrap();
    run_until(&mut fake, &mut app, |_, _| {
        last(&followed).screen == at_end.screen + 50
    });
    let after = last(&followed);
    assert_eq!(after.top, up.top, "{after:?}");
    assert!(after.oldest >= up.oldest, "{after:?}");
    let _ = std::fs::remove_dir_all(&directory);
}

/// The frame, in pixels, of the command's header of `window`'s pane, and
/// its pill's, if it shows.
fn header(app: &App, window: WindowId) -> Option<(Frame, Frame)> {
    let state = &app.windows[&window];
    let pane = state.views.first()?.pane;
    let id = ViewId::Panel(PanelId(format!("block-header-{}", pane.get())));
    let view = state.chrome.iter().find(|view| view.view == id)?;
    let frame = view.frame?;
    let node = view.surface.tree.by_key(&"command".into())?;
    let pill = view.surface.layout.frame(&view.surface.tree, node)?;
    Some((
        frame,
        Frame {
            x: pill.x + frame.x,
            y: pill.y + frame.y,
            ..pill
        },
    ))
}

type Frame = lokyr_ui_core::layout::Frame;

/// The middle of `frame`, of pixels, in points of a display of `scale`.
fn middle(frame: Frame, scale: f64) -> Point {
    Point {
        x: f64::from(frame.x + frame.width / 2.0) / scale,
        y: f64::from(frame.y + frame.height / 2.0) / scale,
    }
}

#[test]
fn the_command_s_header_spans_the_pane_s_top_takes_no_keys_lets_a_scroll_through_and_scrolls_at_a_click()
 {
    // Three commands of 40 lines each, then a prompt: the view, at the
    // screen, shows the third's output, whose line is above it.
    let program = shell(concat!(
        "m() { printf '\\033]133;%s\\007' \"$1\"; }\n",
        "for c in one two three; do m A; printf '$ '; m B; echo \"$c\"; m C; ",
        "seq 1 40; m 'D;0'; done; m A; printf '$ '; sleep 30\n",
    ));
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let mut fake = Fake::default();
    let mut app = App::new(Some(drawing), pill_config(), None).with_program(program);
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(fakes::Surface::default());
    fake.send(&mut app, window, WindowEvent::Focused(true));
    run_until(&mut fake, &mut app, |_, app| header(app, window).is_some());
    let (frame, pill) = header(&app, window).unwrap();
    // Across the pane's top, of its width.
    let rect = app.windows[&window].views[0].rect;
    assert_eq!(
        (frame.x, frame.y, frame.width),
        (rect.x as f32, rect.y as f32, rect.width as f32)
    );
    assert!(pill.width < frame.width && pill.x > frame.x, "{pill:?}");
    // The keyboard stays at the pane.
    assert_eq!(
        app.windows[&window].focus.target(),
        lokyr_ui_core::focus::Target::Pane
    );
    // A scroll over it scrolls the pane.
    let scale = fake.window(window).unwrap().size.scale;
    let pane = app.windows[&window].views[0].pane;
    let offset = |app: &App| app.workspace.pane(pane).unwrap().view_offset();
    fake.send(
        &mut app,
        window,
        WindowEvent::Scroll {
            delta: ScrollDelta::Lines { x: 0.0, y: 1.0 },
            phase: None,
            momentum: false,
            position: middle(frame, scale),
            mods: Modifiers::NONE,
        },
    );
    run_until(&mut fake, &mut app, |_, app| offset(app) > 0);
    // A click on its pill scrolls the view to its command's prompt, at its
    // top, which takes it away.
    run_until(&mut fake, &mut app, |_, app| header(app, window).is_some());
    let (_, pill) = header(&app, window).unwrap();
    let at = middle(pill, scale);
    fake.send(&mut app, window, click(true, at, Modifiers::NONE, 1));
    fake.send(&mut app, window, click(false, at, Modifiers::NONE, 1));
    run_until(&mut fake, &mut app, |_, app| header(app, window).is_none());
    let line = row(&app, window, 0);
    assert!(line.starts_with("$ "), "{line:?}");
}

#[test]
fn the_command_s_header_opens_anew_in_the_window_that_its_tab_moves_to() {
    let program = shell(concat!(
        "m() { printf '\\033]133;%s\\007' \"$1\"; }\n",
        "for c in one two three; do m A; printf '$ '; m B; echo \"$c\"; m C; ",
        "seq 1 40; m 'D;0'; done; m A; printf '$ '; sleep 30\n",
    ));
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let mut fake = Fake::default();
    let mut app = App::new(Some(drawing), pill_config(), None).with_program(program);
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(fakes::Surface::default());
    fake.send(&mut app, window, WindowEvent::Focused(true));
    run_until(&mut fake, &mut app, |_, app| header(app, window).is_some());
    // Another tab, then the header's tab, moved to a window of its own.
    app.run(&mut fake, "tabs.new", Args::new());
    let first = Args::from([(
        "index".to_owned(),
        lokyr_kernel::commands::Value::Integer(1),
    )]);
    app.run(&mut fake, "tabs.goto", first);
    run_until(&mut fake, &mut app, |_, app| header(app, window).is_some());
    app.run(&mut fake, "tabs.detach", Args::new());
    run_until(&mut fake, &mut app, |fake, _| fake.windows().count() == 2);
    let moved = fake.windows().find(|&other| other != window).unwrap();
    app.windows.get_mut(&moved).unwrap().surface = Some(fakes::Surface::default());
    run_until(&mut fake, &mut app, |_, app| header(app, moved).is_some());
    // Its header stays in none of the window that it left, which shows the
    // other tab's own.
    let pane = app.windows[&moved].views[0].pane;
    let own = ViewId::Panel(PanelId(format!("block-header-{}", pane.get())));
    assert!(
        app.windows[&window]
            .chrome
            .iter()
            .all(|view| view.view != own)
    );
}

/// The text of the command's header's pill of `window`'s pane, and its
/// text's frame, in pixels, if it shows.
fn header_text(app: &App, window: WindowId) -> Option<(String, Frame)> {
    let state = &app.windows[&window];
    let pane = state.views.first()?.pane;
    let id = ViewId::Panel(PanelId(format!("block-header-{}", pane.get())));
    let view = state.chrome.iter().find(|view| view.view == id)?;
    let tree = &view.surface.tree;
    let pill = tree.by_key(&"command".into())?;
    let text = *tree.children(pill).first()?;
    let lokyr_kernel::plugins::view::Kind::Text(line) = tree.kind(text)? else {
        return None;
    };
    Some((line.clone(), view.surface.layout.frame(tree, text)?))
}

#[test]
fn a_long_command_s_header_keeps_within_its_pane_as_the_window_narrows() {
    let program = shell(concat!(
        "m() { printf '\\033]133;%s\\007' \"$1\"; }\n",
        "m A; printf '$ '; m B; echo xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx; m C; ",
        "seq 1 200; m 'D;0'; m A; printf '$ '; sleep 30\n",
    ));
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let mut fake = Fake::default();
    let mut app = App::new(Some(drawing), pill_config(), None).with_program(program);
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(fakes::Surface::default());
    fake.send(&mut app, window, WindowEvent::Focused(true));
    run_until(&mut fake, &mut app, |_, app| {
        header_text(app, window).is_some_and(|(line, _)| line.starts_with("xxxx"))
    });
    let (frame, pill) = header(&app, window).unwrap();
    let within = |frame: Frame, pill: Frame| pill.x + pill.width <= frame.x + frame.width;
    assert!(within(frame, pill), "{pill:?} in {frame:?}");
    let (_, wide) = header_text(&app, window).unwrap();
    // A narrow window: the pill within its pane still, its text narrower,
    // which the host cuts of its ellipsis.
    fake.send(&mut app, window, WindowEvent::Resized(points(260.0, 400.0)));
    run_until(&mut fake, &mut app, |_, app| {
        header(app, window).is_some_and(|(frame, _)| frame.width < 600.0)
    });
    let (frame, pill) = header(&app, window).unwrap();
    assert!(within(frame, pill), "{pill:?} in {frame:?}");
    let (line, narrow) = header_text(&app, window).unwrap();
    assert!(line.starts_with("xxxx"), "{line}");
    assert!(narrow.width < wide.width, "{narrow:?} of {wide:?}");
}

#[test]
fn the_command_s_header_takes_the_text_s_new_size() {
    let program = shell(concat!(
        "m() { printf '\\033]133;%s\\007' \"$1\"; }\n",
        "m A; printf '$ '; m B; echo long; m C; seq 1 200; m 'D;0'; m A; printf '$ '; sleep 30\n",
    ));
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let mut fake = Fake::default();
    let mut app = App::new(Some(drawing), pill_config(), None).with_program(program);
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(fakes::Surface::default());
    fake.send(&mut app, window, WindowEvent::Focused(true));
    run_until(&mut fake, &mut app, |_, app| {
        header_text(app, window).is_some()
    });
    let (line, before) = header_text(&app, window).unwrap();
    assert_eq!(line, "long");
    // A point larger, ⌘+'s: the text measured anew, of the pill's line.
    let points = app.config.font.size.points();
    app.set_font_size(&mut fake, Some(points + 4.0));
    run_until(&mut fake, &mut app, |_, app| {
        header_text(app, window).is_some_and(|(_, after)| after.height > before.height)
    });
    let (line, after) = header_text(&app, window).unwrap();
    assert_eq!(line, "long");
    assert!(after.width > before.width, "{before:?} {after:?}");
}

/// The frame, in pixels, of the key `key` of the header of `window`'s
/// pane whose panel's id is `prefix` and the pane's, if it shows.
fn header_part(app: &App, window: WindowId, prefix: &str, key: &str) -> Option<Frame> {
    let state = &app.windows[&window];
    let pane = state.views.first()?.pane;
    let id = ViewId::Panel(PanelId(format!("{prefix}{}", pane.get())));
    let view = state.chrome.iter().find(|view| view.view == id)?;
    let frame = view.frame?;
    let node = view.surface.tree.by_key(&key.into())?;
    let part = view.surface.layout.frame(&view.surface.tree, node)?;
    Some(Frame {
        x: part.x + frame.x,
        y: part.y + frame.y,
        ..part
    })
}

#[test]
fn the_block_s_button_scrolls_to_its_end_and_the_plain_header_comes_back_as_blocks_goes() {
    // Three commands of 100 lines each, then a prompt, on the screen's last
    // row: the view, at the screen, shows the third's end.
    let program = shell(concat!(
        "m() { printf '\\033]133;%s\\007' \"$1\"; }\n",
        "for c in one two three; do m A; printf '$ '; m B; echo \"$c\"; m C; ",
        "seq 1 100; m 'D;0'; done; m A; printf '$ '; sleep 30\n",
    ));
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let mut fake = Fake::default();
    let mut app = App::new(Some(drawing), pill_config(), None).with_program(program);
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(fakes::Surface::default());
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let end = |app: &App| header_part(app, window, "block-header-", "end");
    // Once the last prompt came, the header of the third, whose end the
    // view shows: an earlier one's may show while the output still comes.
    let pane = app.windows[&window].views[0].pane;
    run_until(&mut fake, &mut app, |_, app| {
        app.workspace
            .pane(pane)
            .is_some_and(|pane| pane.commands_meeting(i64::MIN, i64::MAX).len() == 4)
            && header_text(app, window).is_some_and(|(line, _)| line == "three")
            && end(app).is_none()
    });
    // Up into the third's output: its end below the view, the button
    // beside the pill.
    let scale = fake.window(window).unwrap().size.scale;
    let rect = app.windows[&window].views[0].rect;
    let inside = Point {
        x: (f64::from(rect.x) + f64::from(rect.width) / 2.0) / scale,
        y: (f64::from(rect.y) + f64::from(rect.height) / 2.0) / scale,
    };
    fake.send(
        &mut app,
        window,
        WindowEvent::Scroll {
            delta: ScrollDelta::Lines { x: 0.0, y: 10.0 },
            phase: None,
            momentum: false,
            position: inside,
            mods: Modifiers::NONE,
        },
    );
    run_until(&mut fake, &mut app, |_, app| end(app).is_some());
    // Its click puts the block's last row at the view's bottom, under the
    // header, which stays, of no button.
    let at = middle(end(&app).unwrap(), scale);
    fake.send(&mut app, window, click(true, at, Modifiers::NONE, 1));
    fake.send(&mut app, window, click(false, at, Modifiers::NONE, 1));
    run_until(&mut fake, &mut app, |_, app| {
        header(app, window).is_some() && end(app).is_none()
    });
    let rows = app.pane(window).snapshot().rows();
    assert_eq!(row(&app, window, rows - 1), "100");
    // Turned off, its header goes, and the plain one comes back of the view
    // that shows, without a scroll.
    // The window's own keys kept, so that nothing but the plugins changes.
    let off = concat!(
        "[window]\npadding-x = 0\npadding-y = 0\ngutter = false\ncolumns = 200\nrows = 62\n",
        "[plugins.blocks]\nenabled = false\n",
    );
    app.apply(
        &mut fake,
        Config::parse(off, Path::new("config.toml")).unwrap(),
    );
    run_until(&mut fake, &mut app, |_, app| {
        header(app, window).is_none()
            && header_part(app, window, "command-header-", "command").is_some()
    });
    assert!(header_part(&app, window, "command-header-", "end").is_none());
}

#[test]
fn a_block_s_parts_are_copied_and_the_block_selected_of_zsh_s_bash_s_and_fish_s_marks() {
    // `ls` of two rows, then the prompt where you type, of each shell's
    // marks: zsh's line told at its output's mark, bash's read of its
    // row, fish 4's own prompt's mark before the integration's.
    let shells = [
        (
            "zsh",
            "\\033]133;A;cl=line\\007%% \\033]133;B\\007",
            "\\033]133;C;cmdline_url=ls\\007",
            "% ",
        ),
        (
            "bash",
            "\\033]133;A;cl=line\\007$ \\033]133;B\\007",
            "\\033]133;C\\007",
            "$ ",
        ),
        (
            "fish",
            "\\033]133;A;click_events=1\\007\\033]133;A;cl=line\\007> \\033]133;B\\007",
            "\\033]133;C;cmdline_url=ls\\007",
            "> ",
        ),
    ];
    for (name, prompt, output, shown) in shells {
        let script = format!(
            "printf '{prompt}ls\\r\\n{output}a.txt\\r\\nb.txt\\r\\n\\033]133;D;0\\007{prompt}'; sleep 30"
        );
        let mut fake = Fake::default();
        let mut app = App::new(None, all_plugins(""), None).with_program(shell(&script));
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        fake.send(&mut app, window, WindowEvent::Focused(true));
        let pane = app.windows[&window].views[0].pane;
        run_until(&mut fake, &mut app, |_, app| {
            app.workspace
                .pane(pane)
                .is_some_and(|pane| pane.commands_meeting(i64::MIN, i64::MAX).len() == 2)
        });
        let cell = |column: i64, row: i64| {
            Args::from([
                (
                    "column".to_owned(),
                    lokyr_kernel::commands::Value::Integer(column),
                ),
                (
                    "row".to_owned(),
                    lokyr_kernel::commands::Value::Integer(row),
                ),
            ])
        };
        // A cell of its output: the line, the line and the output, and
        // the block from its prompt.
        app.run(&mut fake, "blocks.copy-command", cell(1, 1));
        run_until(&mut fake, &mut app, |fake, _| {
            fake.clipboard_text().as_deref() == Some("ls")
        });
        app.run(&mut fake, "blocks.copy-block", cell(1, 2));
        run_until(&mut fake, &mut app, |fake, _| {
            fake.clipboard_text().as_deref() == Some("ls\na.txt\nb.txt")
        });
        app.run(&mut fake, "blocks.select-block", cell(0, 0));
        run_until(&mut fake, &mut app, |_, app| {
            selected_text(app, window).is_some()
        });
        assert_eq!(
            selected_text(&app, window).map(|text| text.trim_end().to_owned()),
            Some(format!("{shown}ls\na.txt\nb.txt")),
            "{name}"
        );
    }
}

/// The app's config without padding, of the blocks' header of the pill,
/// as the header's tests read it, Blocks turned on.
fn pill_config() -> Config {
    all_plugins("header = \"pill\"\n")
}

/// The frame of the part of `key` of the details' panel that shows in
/// `window`, if one.
fn details_part(app: &App, window: WindowId, key: &str) -> Option<Frame> {
    let state = &app.windows[&window];
    let view = state.chrome.iter().find(
        |view| matches!(&view.view, ViewId::Panel(id) if id.0.starts_with("block-details-")),
    )?;
    let frame = view.frame?;
    let node = view.surface.tree.by_key(&key.into())?;
    let part = view.surface.layout.frame(&view.surface.tree, node)?;
    Some(Frame {
        x: part.x + frame.x,
        y: part.y + frame.y,
        ..part
    })
}

#[test]
fn a_block_s_details_tell_its_times_and_its_folder_each_copied_of_a_click() {
    // `ls` in /tmp, which OSC 7 told before its prompt, then the prompt
    // where you type.
    let program = shell(concat!(
        "printf '\\033]7;file://localhost/tmp\\007\\033]133;A\\007$ \\033]133;B\\007ls\\r\\n",
        "\\033]133;C\\007a.txt\\r\\n\\033]133;D;0\\007\\033]133;A\\007$ \\033]133;B\\007'; ",
        "sleep 30\n",
    ));
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let mut fake = Fake::default();
    let mut app = App::new(Some(drawing), all_plugins(""), None).with_program(program);
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(fakes::Surface::default());
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let pane = app.windows[&window].views[0].pane;
    run_until(&mut fake, &mut app, |_, app| {
        app.workspace.pane(pane).is_some_and(|pane| {
            pane.commands_meeting(i64::MIN, i64::MAX)
                .first()
                .is_some_and(|command| command.ended.is_some())
        })
    });
    let command = app.workspace.pane(pane).unwrap().commands_meeting(0, 1)[0].clone();
    let (started, ended) = (command.started.unwrap(), command.ended.unwrap());
    assert!(ended >= started, "{command:?}");
    assert_eq!(
        (command.folder.as_deref(), command.host.as_deref()),
        (Some("/tmp"), None)
    );
    // The details of a cell of its output, each part copied of a click on
    // its row or its button.
    let cell = Args::from([
        (
            "column".to_owned(),
            lokyr_kernel::commands::Value::Integer(1),
        ),
        ("row".to_owned(), lokyr_kernel::commands::Value::Integer(1)),
    ]);
    app.run(&mut fake, "blocks.details", cell);
    run_until(&mut fake, &mut app, |_, app| {
        details_part(app, window, "duration").is_some()
    });
    assert!(details_part(&app, window, "started").is_some());
    let scale = fake.window(window).unwrap().size.scale;
    for (key, copied) in [("folder", "/tmp"), ("copy-status", "0"), ("line", "ls")] {
        let at = middle(details_part(&app, window, key).unwrap(), scale);
        fake.send(&mut app, window, click(true, at, Modifiers::NONE, 1));
        fake.send(&mut app, window, click(false, at, Modifiers::NONE, 1));
        run_until(&mut fake, &mut app, |fake, _| {
            fake.clipboard_text().as_deref() == Some(copied)
        });
    }
}

/// The text of the plugin's notice that `app` shows, if one shows.
fn plugin_notice(app: &App) -> Option<String> {
    match app.diagnostics.shown()? {
        lokyr_kernel::diagnostics::Notice::Plugin { notice, .. } => Some(notice.text.clone()),
        lokyr_kernel::diagnostics::Notice::Diagnostic(_) => None,
    }
}

/// Whether a row of `window`'s screen holds `text`.
fn screen_holds(app: &App, window: WindowId, text: &str) -> bool {
    (0..24).any(|at| row(app, window, at).contains(text))
}

#[test]
fn a_block_runs_again_at_the_prompt_of_zsh_s_bash_s_and_fish_s_marks_and_is_refused_otherwise() {
    // `ls`, then an empty command line of bracketed paste, which a line
    // read takes, its bytes shown; of zsh, first, a command line of text
    // typed, and, last, a command that runs.
    let shells = [
        (
            "zsh",
            "\\033]133;A;cl=line\\007%% \\033]133;B\\007",
            "\\033]133;C;cmdline_url=ls\\007",
        ),
        (
            "bash",
            "\\033]133;A;cl=line\\007$ \\033]133;B\\007",
            "\\033]133;C\\007",
        ),
        (
            "fish",
            "\\033]133;A;click_events=1\\007\\033]133;A;cl=line\\007> \\033]133;B\\007",
            "\\033]133;C;cmdline_url=ls\\007",
        ),
    ];
    for (name, prompt, output) in shells {
        let refusals = name == "zsh";
        let typed = if refusals {
            format!(
                "printf '{prompt}git st'; IFS= read -r first; \
                 printf '\\r\\n\\033]133;C\\007first:%s\\r\\n\\033]133;D;0\\007' \"$first\"; "
            )
        } else {
            String::new()
        };
        let script = format!(
            "printf '{prompt}ls\\r\\n{output}a.txt\\r\\n\\033]133;D;0\\007'; {typed}\
             printf '{prompt}\\033[?2004h'; IFS= read -r line; \
             printf '\\033[?2004l\\r\\n\\033]133;C\\007ran:%s\\r\\n\\033]133;D;0\\007' \
             \"$(printf %s \"$line\" | cat -v)\"; \
             printf '{prompt}sleep\\r\\n\\033]133;C\\007'; sleep 30"
        );
        let mut fake = Fake::default();
        let mut app = App::new(None, all_plugins(""), None).with_program(shell(&script));
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        fake.send(&mut app, window, WindowEvent::Focused(true));
        let pane = app.windows[&window].views[0].pane;
        let edited = |app: &App| app.workspace.pane(pane).unwrap().edited_command_line();
        let cell = Args::from([
            (
                "column".to_owned(),
                lokyr_kernel::commands::Value::Integer(1),
            ),
            ("row".to_owned(), lokyr_kernel::commands::Value::Integer(1)),
        ]);
        if refusals {
            // Of text typed: a notice, and nothing sent, as the line that
            // the shell reads next is the one sent after.
            run_until(&mut fake, &mut app, |_, app| {
                edited(app).as_deref() == Some("git st")
            });
            app.run(&mut fake, "blocks.rerun", cell.clone());
            run_until(&mut fake, &mut app, |_, app| plugin_notice(app).is_some());
            assert_eq!(
                plugin_notice(&app).as_deref(),
                Some("The command line holds text. Clear it, then run the command again.")
            );
            app.diagnostics.dismiss();
            app.workspace.pane(pane).unwrap().send_text("go\r");
            run_until(&mut fake, &mut app, |_, app| {
                screen_holds(app, window, "first:go")
            });
        }
        // At an empty command line: the line, bracketed, then Return.
        run_until(&mut fake, &mut app, |_, app| {
            edited(app).as_deref() == Some("")
                && app
                    .workspace
                    .pane(pane)
                    .unwrap()
                    .input_modes()
                    .is_some_and(|modes| modes.bracketed_paste)
        });
        app.run(&mut fake, "blocks.rerun", cell.clone());
        run_until(&mut fake, &mut app, |_, app| {
            screen_holds(app, window, "ran:^[[200~ls^[[201~")
        });
        assert!(plugin_notice(&app).is_none(), "{name}");
        if refusals {
            // While a command runs: a notice, past the second between two.
            run_until(&mut fake, &mut app, |_, app| {
                edited(app).is_none() && screen_holds(app, window, "sleep")
            });
            std::thread::sleep(Duration::from_millis(1100));
            app.run(&mut fake, "blocks.rerun", cell);
            run_until(&mut fake, &mut app, |_, app| plugin_notice(app).is_some());
            assert_eq!(
                plugin_notice(&app).as_deref(),
                Some(
                    "The shell isn't at its prompt: a command is still running. \
                     Run it again once the command ends."
                )
            );
        }
    }
}

/// Draws `window`'s frames until `done` holds of its last, for a while at
/// most.
fn framed_until(fake: &mut Fake, app: &mut App, window: WindowId, done: impl Fn(&App) -> bool) {
    for _ in 0..500 {
        fake.send(app, window, WindowEvent::Frame);
        if done(app) {
            return;
        }
        run_until(fake, app, |_, _| true);
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the frame never came");
}

/// The outlines that `window`'s frame draws of a theme's token `color`, as
/// a selected block's, by their rectangles.
fn tints(
    app: &App,
    window: WindowId,
    color: lokyr_render_core::Color,
) -> Vec<lokyr_render_core::Rect> {
    app.windows[&window]
        .shown
        .as_ref()
        .expect("a frame")
        .items
        .iter()
        .flat_map(|item| match item {
            Item::Shapes(shapes) => shapes.clone(),
            _ => Vec::new(),
        })
        .filter(|shape| shape.fill == color)
        .map(|shape| shape.rect)
        .collect()
}

#[test]
fn the_keyboard_selects_a_block_moves_the_selection_copies_it_and_a_key_or_a_click_ends_it() {
    // The system's line of the last login, as a login shell's first, then
    // three commands of a line of output each, then the prompt.
    let program = shell(concat!(
        "echo 'Last login: Thu Nov 19 on ttys001'\n",
        "m() { printf '\\033]133;%s\\007' \"$1\"; }\n",
        "for c in one two three; do m A; printf '$ '; m B; echo \"$c\"; m C; ",
        "echo \"out $c\"; m 'D;0'; done; m A; printf '$ '; m B; sleep 30\n",
    ));
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let mut fake = Fake::default();
    let mut app = App::new(Some(drawing), all_plugins(""), None).with_program(program);
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(fakes::Surface::default());
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let pane = app.windows[&window].views[0].pane;
    run_until(&mut fake, &mut app, |_, app| {
        app.workspace
            .pane(pane)
            .is_some_and(|pane| pane.commands_meeting(i64::MIN, i64::MAX).len() == 4)
    });
    let ids: Vec<u64> = app
        .workspace
        .pane(pane)
        .unwrap()
        .commands_meeting(i64::MIN, i64::MAX)
        .iter()
        .map(|command| command.id)
        .collect();
    let host_pane = lokyr_kernel::plugins::host::PaneId(pane.get());
    let selected = |app: &App| app.plugins.selected_block(host_pane);
    let press = |fake: &mut Fake, app: &mut App, code: KeyCode, text: &str, mods: Modifiers| {
        fake.send(app, window, key_event(code, text, mods, true));
        fake.send(app, window, key_event(code, text, mods, false));
    };
    let cmd = Modifiers {
        super_key: true,
        ..Modifiers::NONE
    };
    // ⌘↑ selects the last block, which the frame tints of the theme's
    // `blocks.selected.bg`.
    press(&mut fake, &mut app, KeyCode::ArrowUp, "", cmd);
    run_until(&mut fake, &mut app, |_, app| selected(app) == Some(ids[2]));
    let dark = fake.appearance() == lokyr_platform_core::Appearance::Dark;
    let tint = app
        .theme
        .variant(dark)
        .tokens
        .get("blocks.selected.bg")
        .unwrap();
    framed_until(&mut fake, &mut app, window, |app| {
        tints(app, window, tint).len() == 1
    });
    // ↑ selects the one before; ⌘C, of no text selected, copies it.
    press(&mut fake, &mut app, KeyCode::ArrowUp, "", Modifiers::NONE);
    run_until(&mut fake, &mut app, |_, app| selected(app) == Some(ids[1]));
    press(&mut fake, &mut app, KeyCode::KeyC, "c", cmd);
    run_until(&mut fake, &mut app, |fake, _| {
        fake.clipboard_text().as_deref() == Some("two\nout two")
    });
    // Escape ends it, and the frame draws no tint.
    press(
        &mut fake,
        &mut app,
        KeyCode::Escape,
        "\u{1b}",
        Modifiers::NONE,
    );
    run_until(&mut fake, &mut app, |_, app| selected(app).is_none());
    framed_until(&mut fake, &mut app, window, |app| {
        tints(app, window, tint).is_empty()
    });
    // So do a key typed, which goes to the program, and a click.
    press(&mut fake, &mut app, KeyCode::ArrowUp, "", cmd);
    run_until(&mut fake, &mut app, |_, app| selected(app).is_some());
    press(&mut fake, &mut app, KeyCode::KeyX, "x", Modifiers::NONE);
    run_until(&mut fake, &mut app, |_, app| selected(app).is_none());
    press(&mut fake, &mut app, KeyCode::ArrowUp, "", cmd);
    run_until(&mut fake, &mut app, |_, app| selected(app).is_some());
    let at = Point { x: 40.0, y: 10.0 };
    fake.send(&mut app, window, click(true, at, Modifiers::NONE, 1));
    fake.send(&mut app, window, click(false, at, Modifiers::NONE, 1));
    run_until(&mut fake, &mut app, |_, app| selected(app).is_none());
}

#[test]
fn a_click_on_a_block_selects_it_and_the_prompt_or_a_drag_ends_it() {
    // The system's line of the last login, then three commands of a line
    // of output each, on the rows 1 to 6, then the prompt.
    let program = shell(concat!(
        "echo 'Last login: Thu Nov 19 on ttys001'\n",
        "m() { printf '\\033]133;%s\\007' \"$1\"; }\n",
        "for c in one two three; do m A; printf '$ '; m B; echo \"$c\"; m C; ",
        "echo \"out $c\"; m 'D;0'; done; m A; printf '$ '; m B; sleep 30\n",
    ));
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let mut fake = Fake::default();
    let mut app = App::new(Some(drawing), all_plugins(""), None).with_program(program);
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(fakes::Surface::default());
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let pane = app.windows[&window].views[0].pane;
    run_until(&mut fake, &mut app, |_, app| {
        app.workspace
            .pane(pane)
            .is_some_and(|pane| pane.commands_meeting(i64::MIN, i64::MAX).len() == 4)
    });
    framed_until(&mut fake, &mut app, window, |app| {
        app.workspace.pane(pane).unwrap().snapshot().blocks().len() == 4
    });
    let ids: Vec<u64> = app
        .workspace
        .pane(pane)
        .unwrap()
        .commands_meeting(i64::MIN, i64::MAX)
        .iter()
        .map(|command| command.id)
        .collect();
    let host_pane = lokyr_kernel::plugins::host::PaneId(pane.get());
    let selected = |app: &App| app.plugins.selected_block(host_pane);
    let dark = fake.appearance() == lokyr_platform_core::Appearance::Dark;
    let tint = app
        .theme
        .variant(dark)
        .tokens
        .get("blocks.selected.bg")
        .unwrap();
    // The middle of the pane's row `row`, in points.
    let scale = fake.window(window).unwrap().size.scale;
    let rect = app.windows[&window].views[0].rect;
    let height = i32::from(app.workspace.pane(pane).unwrap().grid().cell_height);
    let over = |row: i32| Point {
        x: f64::from(rect.x + 40) / scale,
        y: f64::from(rect.y + row * height + height / 2) / scale,
    };
    let tap = |fake: &mut Fake, app: &mut App, at: Point| {
        fake.send(app, window, click(true, at, Modifiers::NONE, 1));
        fake.send(app, window, click(false, at, Modifiers::NONE, 1));
    };
    // A click on `two`'s output selects its block, tinted across its two
    // rows, from its prompt's.
    tap(&mut fake, &mut app, over(4));
    run_until(&mut fake, &mut app, |_, app| selected(app) == Some(ids[1]));
    framed_until(&mut fake, &mut app, window, |app| {
        tints(app, window, tint).len() == 1
    });
    let tinted = tints(&app, window, tint)[0];
    assert_eq!(
        (tinted.y, tinted.height),
        (rect.y + 3 * height, 2 * height as u32)
    );
    // On `one`'s prompt, its block; at the prompt where you type, none.
    tap(&mut fake, &mut app, over(1));
    run_until(&mut fake, &mut app, |_, app| selected(app) == Some(ids[0]));
    tap(&mut fake, &mut app, over(7));
    run_until(&mut fake, &mut app, |_, app| selected(app).is_none());
    framed_until(&mut fake, &mut app, window, |app| {
        tints(app, window, tint).is_empty()
    });
    // A drag that selects text ends a block's selection.
    tap(&mut fake, &mut app, over(6));
    run_until(&mut fake, &mut app, |_, app| selected(app) == Some(ids[2]));
    select_drag(
        &mut fake,
        &mut app,
        window,
        (over(2), over(4)),
        Modifiers::NONE,
    );
    run_until(&mut fake, &mut app, |_, app| selected(app).is_none());
    assert!(selected_text(&app, window).is_some());
}

#[test]
fn a_click_and_a_drag_below_a_fold_take_the_rows_that_the_view_shows_there() {
    // `ls` of five rows, `echo hi`, then the prompt where you type: rows 0
    // to 8 of the screen.
    let marks = |line: &str, output: &str| {
        format!(
            "\\033]133;A\\007$ \\033]133;B\\007{line}\\r\\n\\033]133;C\\007{output}\\033]133;D;0\\007"
        )
    };
    let script = format!(
        "printf '{}{}\\033]133;A\\007$ \\033]133;B\\007'; sleep 30",
        marks("ls", "a1\\r\\na2\\r\\na3\\r\\na4\\r\\na5\\r\\n"),
        marks("echo hi", "hi\\r\\n"),
    );
    let mut fake = Fake::default();
    let mut app = App::new(None, unpadded(), None).with_program(shell(&script));
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let pane = app.windows[&window].views[0].pane;
    run_until(&mut fake, &mut app, |_, app| {
        app.workspace
            .pane(pane)
            .is_some_and(|pane| pane.commands_meeting(i64::MIN, i64::MAX).len() == 3)
    });
    // `ls`' output folded: the view shows `$ ls`, the placeholder, `$ echo
    // hi`, `hi` and the prompt.
    let ls = app
        .workspace
        .pane(pane)
        .unwrap()
        .commands_meeting(i64::MIN, i64::MAX)[0]
        .id;
    assert!(
        app.workspace
            .pane(pane)
            .unwrap()
            .terminal()
            .unwrap()
            .fold(ls, false)
    );
    framed_until(&mut fake, &mut app, window, |app| {
        !app.workspace
            .pane(pane)
            .unwrap()
            .snapshot()
            .view()
            .is_empty()
    });
    assert!(row(&app, window, 2).starts_with("$ echo hi"));
    // A right click on `hi`, the view's row 3, marks the row 7's cell.
    right_click(&mut fake, &mut app, window, cell(1, 3), Modifiers::NONE);
    assert_eq!(
        app.workspace.pane(pane).unwrap().marked_cell(),
        Some((1, 7))
    );
    // A drag from `$ ls` over the placeholder to `hi` selects the fold's
    // rows too.
    select_drag(
        &mut fake,
        &mut app,
        window,
        (cell(0, 0), cell(2, 3)),
        Modifiers::NONE,
    );
    assert_eq!(
        selected_text(&app, window).as_deref(),
        Some("$ ls\na1\na2\na3\na4\na5\n$ echo hi\nhi")
    );
    // Its end on the placeholder, from above it: the fold's rows whole.
    select_drag(
        &mut fake,
        &mut app,
        window,
        (cell(0, 0), cell(3, 1)),
        Modifiers::NONE,
    );
    assert_eq!(
        selected_text(&app, window).as_deref(),
        Some("$ ls\na1\na2\na3\na4\na5")
    );
}

#[test]
fn a_blank_row_comes_before_a_prompt_after_a_command_as_blocks_asks() {
    // `ls`, then, once a line comes, of no echo from the end on, the
    // prompt where you type.
    let program = shell(concat!(
        "m() { printf '\\033]133;%s\\007' \"$1\"; }\n",
        "m A; printf '$ '; m B; echo ls; m C; echo a; stty -echo; m 'D;0'; ",
        "IFS= read -r go; m A; printf '$ '; m B; sleep 30\n",
    ));
    let config = all_plugins("spacing = true\n");
    let mut fake = Fake::default();
    let mut app = App::new(None, config, None).with_program(program);
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let pane = app.windows[&window].views[0].pane;
    let blocks = |app: &App| -> Vec<(i32, i32, bool, bool)> {
        app.workspace
            .pane(pane)
            .unwrap()
            .snapshot()
            .blocks()
            .iter()
            .map(|block| (block.first, block.last, block.room_above, block.room_below))
            .collect()
    };
    // `ls` ended, its frames having told the terminal of the blank row.
    framed_until(&mut fake, &mut app, window, |app| {
        app.workspace
            .pane(pane)
            .unwrap()
            .snapshot()
            .blocks()
            .first()
            .is_some_and(|block| {
                block
                    .kind
                    .is_some_and(|kind| kind != lokyr_vt_core::terminal::GutterKind::Running)
            })
    });
    app.workspace.pane(pane).unwrap().send_text("\r");
    // The prompt a row below the end, the blank row of no block.
    framed_until(&mut fake, &mut app, window, |app| blocks(app).len() == 2);
    assert_eq!(blocks(&app), [(0, 1, false, true), (3, 3, true, false)]);
}

#[test]
fn a_right_click_on_a_block_s_pinned_rows_opens_its_menu_which_acts_on_it() {
    // A prompt of two rows, then 80 rows of output, more than the pane's
    // 62, then the prompt where you type.
    let program = shell(concat!(
        "m() { printf '\\033]133;%s\\007' \"$1\"; }\n",
        "m A; printf 'user ~/dir\\r\\n$ '; m B; echo 'seq 80'; m C; seq 80; m 'D;0'; ",
        "m A; printf '$ '; m B; sleep 30\n",
    ));
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let mut fake = Fake::default();
    let mut app = App::new(Some(drawing), all_plugins(""), None).with_program(program);
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(fakes::Surface::default());
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let pane = app.windows[&window].views[0].pane;
    framed_until(&mut fake, &mut app, window, |app| {
        app.workspace
            .pane(pane)
            .unwrap()
            .snapshot()
            .sticky()
            .is_some_and(|sticky| sticky.rows() == 2)
    });
    let menus = fake.context_menus().len();
    // A right click on them: the block's menu, of its actions alone; the
    // word under them stays unselected.
    right_click(&mut fake, &mut app, window, cell(1, 0), Modifiers::NONE);
    run_until(&mut fake, &mut app, |fake, _| {
        fake.context_menus().len() > menus
    });
    let (_, items, dimmed) = context_menu(&fake, window);
    assert_eq!(
        items,
        [
            "Copy Command",
            "Copy Command Output",
            "Copy Command and Output",
            "Select Command Output",
            "Select Block",
            "-",
            "Block Details",
            "Run Command Again"
        ]
    );
    assert!(dimmed.is_empty(), "{dimmed:?}");
    assert_eq!(selected_text(&app, window), None);
    let (_, _, menu) = fake.context_menus().last().unwrap();
    let item = commands_of(&menu.items)
        .find_map(|(title, command)| (title == "Copy Command").then(|| command.clone()))
        .unwrap();
    fake.send(&mut app, window, WindowEvent::ContextCommand(item));
    run_until(&mut fake, &mut app, |fake, _| {
        fake.clipboard_text().as_deref() == Some("seq 80")
    });
}

#[test]
fn a_block_s_prompt_rows_pin_at_the_pane_s_top_and_a_click_on_them_scrolls_to_its_prompt() {
    // A prompt of two rows, then 80 rows of output, more than the pane's
    // 62, then the prompt where you type.
    let program = shell(concat!(
        "m() { printf '\\033]133;%s\\007' \"$1\"; }\n",
        "m A; printf 'user ~/dir\\r\\n$ '; m B; echo 'seq 80'; m C; seq 80; m 'D;0'; ",
        "m A; printf '$ '; m B; sleep 30\n",
    ));
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let mut fake = Fake::default();
    let mut app = App::new(Some(drawing), all_plugins(""), None).with_program(program);
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(fakes::Surface::default());
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let pane = app.windows[&window].views[0].pane;
    // The view at the screen, of the output's last rows: the prompt's two
    // rows pinned over its first.
    framed_until(&mut fake, &mut app, window, |app| {
        app.workspace
            .pane(pane)
            .unwrap()
            .snapshot()
            .sticky()
            .is_some_and(|sticky| sticky.rows() == 2)
    });
    let pinned: Vec<String> = {
        let sticky = app
            .workspace
            .pane(pane)
            .unwrap()
            .snapshot()
            .sticky()
            .unwrap();
        (0..2)
            .map(|y| {
                sticky
                    .line(y)
                    .cells()
                    .iter()
                    .map(|cell| match cell.content() {
                        lokyr_vt_core::page::Content::Char(c) if c != '\0' => c,
                        _ => ' ',
                    })
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect()
    };
    assert_eq!(pinned, ["user ~/dir", "$ seq 80"]);
    // The frame takes the window's place under them, across the pane.
    let rect = app.windows[&window].views[0].rect;
    let height = i32::from(app.workspace.pane(pane).unwrap().grid().cell_height);
    let knocked: Vec<lokyr_render_core::Rect> = app.windows[&window]
        .shown
        .as_ref()
        .expect("a frame")
        .items
        .iter()
        .flat_map(|item| match item {
            Item::Shapes(shapes) => shapes.clone(),
            _ => Vec::new(),
        })
        .filter(|shape| shape.knockout)
        .map(|shape| shape.rect)
        .collect();
    assert!(
        knocked
            .iter()
            .any(|knocked| knocked.y == rect.y && knocked.height == 2 * height as u32),
        "{knocked:?}"
    );
    // A click on them scrolls the view to the prompt, which then shows
    // itself, of nothing pinned.
    let scale = fake.window(window).unwrap().size.scale;
    let on = Point {
        x: f64::from(rect.x + 40) / scale,
        y: f64::from(rect.y + height / 2) / scale,
    };
    fake.send(&mut app, window, click(true, on, Modifiers::NONE, 1));
    fake.send(&mut app, window, click(false, on, Modifiers::NONE, 1));
    framed_until(&mut fake, &mut app, window, |app| {
        app.workspace
            .pane(pane)
            .unwrap()
            .snapshot()
            .sticky()
            .is_none()
    });
    assert!(row(&app, window, 0).starts_with("user ~/dir"));
    assert!(selected_text(&app, window).is_none());
}
