//! The commands that the shell marks and the panes' views in the app: a
//! plugin hears each command's start and end, reads their marks, and
//! follows its pane's view through its scroll, output and the scrollback's
//! limit; and the command's header over the pane's top.

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
            let commands = cx.shell_commands(pane, i64::MIN, i64::MAX)?;
            self.0.borrow_mut().commands = commands;
            return Ok(());
        }
        cx.scroll(pane, Scroll::Up(10))
    }

    fn on_event(&mut self, _: &mut Cx<'_>, events: &[plugins::Event]) -> Result<(), Error> {
        let mut followed = self.0.borrow_mut();
        for event in events {
            match *event {
                plugins::Event::ViewportChanged { pane, viewport } => {
                    followed.pane = Some(pane);
                    followed.viewports.push(viewport);
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
    let last = |followed: &Rc<RefCell<Followed>>| *followed.borrow().viewports.last().unwrap();
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
    let id = ViewId::Panel(PanelId(format!("command-header-{}", pane.get())));
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
    let mut app = App::new(Some(drawing), unpadded(), None).with_program(program);
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
    let mut app = App::new(Some(drawing), unpadded(), None).with_program(program);
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
    let own = ViewId::Panel(PanelId(format!("command-header-{}", pane.get())));
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
    let id = ViewId::Panel(PanelId(format!("command-header-{}", pane.get())));
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
fn the_command_s_header_takes_the_text_s_new_size() {
    let program = shell(concat!(
        "m() { printf '\\033]133;%s\\007' \"$1\"; }\n",
        "m A; printf '$ '; m B; echo long; m C; seq 1 200; m 'D;0'; m A; printf '$ '; sleep 30\n",
    ));
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let mut fake = Fake::default();
    let mut app = App::new(Some(drawing), unpadded(), None).with_program(program);
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
