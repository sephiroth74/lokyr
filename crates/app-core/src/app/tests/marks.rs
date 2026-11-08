//! The commands that the shell marks and the panes' views in the app: a
//! plugin hears each command's start and end, reads their marks, and
//! follows its pane's view through its scroll, output and the scrollback's
//! limit.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use lokyr_kernel::commands::{Category, Command};
use lokyr_kernel::plugins::host::{PaneId, Scroll, ShellCommand, Viewport};
use lokyr_kernel::plugins::{Cx, Plugin};

use super::*;

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
