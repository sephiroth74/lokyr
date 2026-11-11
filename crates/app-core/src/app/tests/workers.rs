//! The tests of the plugins on threads of their own in the app: their
//! views show once their renders come, as the main thread turns without
//! waiting for them, and their commands' failures tell the user.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};

use lokyr_kernel::commands::{Category, Command};
use lokyr_kernel::plugins::ui::{Slot, ViewId};
use lokyr_kernel::plugins::view::{Layout as NodeLayout, Length, Node};
use lokyr_kernel::plugins::{Cx, Plugin};
use lokyr_text_core::FontConfig;

use super::*;
use crate::fakes;

const MANIFEST: &str = concat!(
    "[plugin]\nname = \"lokyr/threaded\"\nkind = \"builtin\"\nlicense = \"MIT\"\n",
    "thread = \"worker\"\n[contributes]\nslots = [\"status-bar\"]\n",
);

/// A plugin of a thread of its own, of a status bar of a text, whose
/// first render waits for `gate`, ten seconds at most, and counts its
/// renders in `renders`; its command `threaded.fail` fails.
struct Threaded {
    gate: Option<Receiver<()>>,
    renders: Arc<AtomicUsize>,
}

impl Plugin for Threaded {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        cx.register_command(Command {
            name: "threaded.fail".to_owned(),
            title: "Fail".to_owned(),
            category: Category::View,
            capabilities: Vec::new(),
            params: Vec::new(),
            when: None,
        })
    }

    fn on_command(&mut self, _: &mut Cx<'_>, _: &str, _: Args) -> Result<(), Error> {
        Err(Error::new(
            Code::Failed,
            "the threaded plugin had nothing to do",
        ))
    }

    fn render(&mut self, _: &mut Cx<'_>, view: &ViewId) -> Option<Node> {
        let ViewId::StatusBar(_) = view else {
            return None;
        };
        if let Some(gate) = self.gate.take() {
            let _ = gate.recv_timeout(Duration::from_secs(10));
        }
        self.renders.fetch_add(1, Ordering::Relaxed);
        let mut text = Node::text("from its thread");
        text.layout = NodeLayout {
            width: Length::Points(120.0),
            height: Length::Points(20.0),
            ..NodeLayout::default()
        };
        Some(Node::container(vec![text]))
    }
}

/// The frame of `window`'s status bar, if it shows.
fn status_bar(app: &App, window: WindowId) -> Option<lokyr_ui_core::layout::Frame> {
    app.windows[&window]
        .chrome
        .iter()
        .find(|view| view.view.slot() == Some(Slot::StatusBar))
        .and_then(|view| view.frame)
}

#[test]
fn a_worker_s_status_bar_shows_once_its_render_comes_and_its_failure_tells_the_user() {
    let (gate, held) = mpsc::channel();
    let renders = Arc::new(AtomicUsize::new(0));
    let plugin = Threaded {
        gate: Some(held),
        renders: Arc::clone(&renders),
    };
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let mut fake = Fake::default();
    fake.set_bar(28.0);
    let mut app = App::new(Some(drawing), unpadded(), None)
        .with_program(idle())
        .with_builtin_send(MANIFEST, Box::new(plugin));
    let start = Instant::now();
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(fakes::Surface::default());
    // The window draws as the render waits on its thread, without it.
    for _ in 0..3 {
        fake.deliver_wake(&mut app);
        fake.send(&mut app, window, WindowEvent::Frame);
    }
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "the main thread waited for the render"
    );
    assert_eq!(status_bar(&app, window), None);
    // The render's end wakes the main thread, which nothing else does once
    // the shell is ready.
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
    std::thread::sleep(Duration::from_millis(20));
    while fake.deliver_wake(&mut app) {}
    gate.send(()).unwrap();
    let rendering = Instant::now();
    while renders.load(Ordering::Relaxed) == 0 {
        assert!(rendering.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(1));
    }
    std::thread::sleep(Duration::from_millis(20));
    assert!(fake.deliver_wake(&mut app), "the render's end woke nothing");
    run_until(&mut fake, &mut app, |_, app| {
        status_bar(app, window).is_some()
    });
    // The frames that asked for the view as it rendered asked for it once.
    assert_eq!(renders.load(Ordering::Relaxed), 1);
    // A command that fails on its thread tells it, as one of the main
    // thread's does.
    assert_eq!(app.diagnostics.shown(), None);
    fake.send_app(&mut app, AppEvent::Command("threaded.fail".to_owned()));
    run_until(&mut fake, &mut app, |_, app| {
        app.diagnostics.shown().is_some()
    });
    let shown = app.diagnostics.shown().unwrap();
    assert!(shown.text.contains("nothing to do"), "{shown:?}");
}

/// A plugin of a thread of its own whose command, `again.go`, runs itself,
/// and `again.twice` itself twice, as `runs` counts.
struct Again {
    runs: Arc<AtomicUsize>,
}

impl Plugin for Again {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        for name in ["again.go", "again.twice"] {
            cx.register_command(Command {
                name: name.to_owned(),
                title: "Again".to_owned(),
                category: Category::View,
                capabilities: Vec::new(),
                params: Vec::new(),
                when: None,
            })?;
        }
        Ok(())
    }

    fn catalogs(&self) -> lokyr_kernel::plugins::Catalogs {
        lokyr_kernel::plugins::built_in(&[
            ("en", "command-again-go = Again\n"),
            ("it", "command-again-go = Ancora\n"),
        ])
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, _: Args) -> Result<(), Error> {
        self.runs.fetch_add(1, Ordering::Relaxed);
        if name == "again.twice" {
            cx.run_command(name, Args::new())?;
        }
        cx.run_command(name, Args::new())
    }
}

/// The app of a plugin of a thread of its own, `again`, started, in
/// `language`.
fn with_again(language: &str) -> (Fake, App, Arc<AtomicUsize>) {
    let runs = Arc::new(AtomicUsize::new(0));
    let manifest = MANIFEST.replace("lokyr/threaded", "lokyr/again");
    let mut fake = Fake::default();
    let mut config = unpadded();
    config.app.language = language.to_owned();
    let mut app = App::new(None, config, None)
        .with_program(idle())
        .with_builtin_send(
            &manifest,
            Box::new(Again {
                runs: Arc::clone(&runs),
            }),
        );
    fake.start(&mut app);
    (fake, app, runs)
}

#[test]
fn a_worker_s_commands_show_in_the_menu_once_its_activation_is_served() {
    // Titled from its catalogs, as a main thread's command is.
    let (mut fake, mut app, _) = with_again("it");
    run_until(&mut fake, &mut app, |fake, _| {
        menu_items(fake, 3).iter().any(|item| item == "Ancora")
    });
    assert_eq!(app.commands.get("again.go").unwrap().title, "Ancora");
}

#[test]
fn a_worker_s_command_that_runs_itself_stops_as_deep_as_a_main_thread_s() {
    let (mut fake, mut app, runs) = with_again("en");
    run_until(&mut fake, &mut app, |_, app| {
        app.commands.get("again.go").is_some()
    });
    fake.send_app(&mut app, AppEvent::Command("again.go".to_owned()));
    run_until(&mut fake, &mut app, |_, _| {
        runs.load(Ordering::Relaxed) > COMMANDS_DEEP
    });
    // The turns that more runs would take, which none takes.
    let pause = Instant::now() + Duration::from_millis(200);
    run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
    assert_eq!(runs.load(Ordering::Relaxed), COMMANDS_DEEP + 1);
}

#[test]
fn a_worker_s_command_that_runs_itself_twice_stops_as_its_chain_ran_its_most() {
    let (mut fake, mut app, runs) = with_again("en");
    run_until(&mut fake, &mut app, |_, app| {
        app.commands.get("again.twice").is_some()
    });
    fake.send_app(&mut app, AppEvent::Command("again.twice".to_owned()));
    run_until(&mut fake, &mut app, |_, _| {
        runs.load(Ordering::Relaxed) > COMMANDS_WIDE
    });
    // The turns that more runs would take, which none takes.
    let pause = Instant::now() + Duration::from_millis(300);
    run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
    assert_eq!(runs.load(Ordering::Relaxed), COMMANDS_WIDE + 1);
}
