//! Plugins on threads of their own: their calls there, the main thread's
//! answers as it turns, which never waits, their writes as their calls
//! end, their renders, their faults; and the calls of the main thread's
//! plugins that take long.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use lokyr_plugin_sdk::event::WorkspaceEvent;
use lokyr_plugin_sdk::host::{self, PaneId as Handle, WindowId as WindowHandle};
use lokyr_plugin_sdk::message::{Data, Event};
use lokyr_plugin_sdk::plugin::{Cx, Plugin};
use lokyr_plugin_sdk::ui::ViewId;
use lokyr_plugin_sdk::view::Node;

use super::super::tests::{App, binding, command, text_once, window};
use super::super::{Chain, Plugins, Request, Runner, Served, Status};
use crate::commands::{Args, Code, Error};
use crate::keymap::Invocation;
use crate::workspace::{PaneId, TabId, WindowId};

/// The manifest of the tests' plugin of a thread of its own, of the status
/// bar.
const MANIFEST: &str = concat!(
    "[plugin]\nname = \"acme/busy\"\nkind = \"builtin\"\nlicense = \"MIT\"\n",
    "thread = \"worker\"\n[contributes]\nslots = [\"status-bar\"]\n",
);

type Activate = Box<dyn FnMut(&mut Cx<'_>) -> Result<(), Error> + Send>;
type OnCommand = Box<dyn FnMut(&mut Cx<'_>, &str) -> Result<(), Error> + Send>;
type OnEvent = Box<dyn FnMut(&mut Cx<'_>, &[Event]) -> Result<(), Error> + Send>;
type Render = Box<dyn FnMut(&mut Cx<'_>, &ViewId) -> Option<Node> + Send>;
type TakeFailure = Box<dyn FnMut() -> Option<Error> + Send>;

/// A plugin that can go to a thread, whose callbacks the test gives: by
/// default, its activation registers `busy.go`.
struct Threaded {
    activate: Activate,
    on_command: OnCommand,
    on_event: OnEvent,
    render: Render,
    take_failure: TakeFailure,
}

impl Default for Threaded {
    fn default() -> Self {
        Self {
            activate: Box::new(|cx| cx.register_command(command("busy.go"))),
            on_command: Box::new(|_, _| Ok(())),
            on_event: Box::new(|_, _| Ok(())),
            render: Box::new(|_, _| None),
            take_failure: Box::new(|| None),
        }
    }
}

impl Plugin for Threaded {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        (self.activate)(cx)
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, _args: Args) -> Result<(), Error> {
        (self.on_command)(cx, name)
    }

    fn on_event(&mut self, cx: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        (self.on_event)(cx, events)
    }

    fn render(&mut self, cx: &mut Cx<'_>, view: &ViewId) -> Option<Node> {
        (self.render)(cx, view)
    }

    fn take_failure(&mut self) -> Option<Error> {
        (self.take_failure)()
    }
}

/// The plugins, with `plugin` on its thread, activated, its activation
/// served: whose wakes of the main thread `wakes` counts.
fn started(app: &mut App, plugin: Threaded) -> (Plugins, Arc<AtomicUsize>) {
    let mut plugins = Plugins::new();
    let wakes = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&wakes);
    plugins.set_wake(move || {
        counted.fetch_add(1, Ordering::Relaxed);
    });
    plugins
        .add_builtin_send(MANIFEST, Box::new(plugin))
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(plugins.status("acme/busy"), Some(Status::Active));
    served(&mut plugins, app);
    (plugins, wakes)
}

/// Serves the plugins' threads until no call is under way, for ten seconds
/// at most: what the turns took, together.
fn served(plugins: &mut Plugins, app: &mut App) -> Served {
    let start = Instant::now();
    let mut all = Served::default();
    loop {
        let turn = plugins.serve(&mut app.context());
        all.any |= turn.any;
        all.commands.extend(turn.commands);
        all.errors.extend(turn.errors);
        if !plugins.waiting() {
            return all;
        }
        assert!(start.elapsed() < Duration::from_secs(10), "a call hangs");
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// The handle of `window`'s focused pane, and the pane's id.
fn focused(app: &App, window: WindowId) -> (Handle, PaneId) {
    let pane = app.workspace.active_pane(window).unwrap();
    (Handle(pane.get()), pane)
}

#[test]
fn a_worker_s_calls_run_on_its_thread_and_the_main_thread_answers_them_as_it_turns() {
    let mut app = App::default();
    let window = window(&mut app, "echo hello; sleep 30");
    let (handle, pane) = focused(&app, window);
    text_once(&app, pane, "hello");
    app.workspace.pane(pane).unwrap().clear_selection();
    let (told, heard) = mpsc::channel();
    let main = std::thread::current().id();
    let plugin = Threaded {
        activate: Box::new(|cx| {
            cx.register_command(command("busy.go"))?;
            cx.bind(binding("cmd+b", "busy.go", None))
        }),
        on_command: Box::new(move |cx, _| {
            // A read after a write reads the state before it.
            cx.select_all(handle)?;
            let before = cx.selection_text(handle)?;
            let windows = cx.windows()?.len();
            told.send((std::thread::current().id(), before, windows))
                .unwrap();
            cx.emit("busy.done", "busy/1", Data::Nil)
        }),
        ..Threaded::default()
    };
    let (mut plugins, wakes) = started(&mut app, plugin);
    // Its declarations, made on its thread, took effect as the main thread
    // answered them.
    assert!(app.registry.get("busy.go").is_some());
    assert!(plugins.owns("busy.go"));
    assert!(plugins.take_declarations_changed());
    assert_eq!(plugins.bindings().count(), 1);
    let woken = wakes.load(Ordering::Relaxed);
    assert!(woken >= 3, "{woken} wakes");
    plugins
        .run("busy.go", Args::new(), &mut app.context())
        .unwrap();
    assert!(plugins.waiting());
    let turns = served(&mut plugins, &mut app);
    assert!(turns.any && turns.errors.is_empty() && turns.commands.is_empty());
    let (thread, before, windows) = heard.try_recv().unwrap();
    assert_ne!(thread, main);
    assert_eq!((before, windows), (None, 1));
    // Its writes applied as its call returned, its event waits for the
    // delivery.
    let text = app.workspace.pane(pane).unwrap().selection_text();
    assert!(text.is_some_and(|text| text.contains("hello")));
    assert!(plugins.take_written());
    assert!(plugins.has_events());
}

#[test]
fn the_main_thread_never_waits_for_a_worker_whose_calls_wait_their_turn() {
    let mut app = App::default();
    let window = window(&mut app, "sleep 30");
    let (gate, held): (Sender<()>, Receiver<()>) = mpsc::channel();
    let renders = Arc::new(AtomicUsize::new(0));
    let rendered = Arc::clone(&renders);
    let batches = Arc::new(Mutex::new(Vec::new()));
    let heard = Arc::clone(&batches);
    let plugin = Threaded {
        activate: Box::new(|cx| {
            cx.register_command(command("busy.go"))?;
            cx.subscribe(&["tab-created"])
        }),
        on_command: Box::new(move |_, _| {
            held.recv().unwrap();
            Ok(())
        }),
        on_event: Box::new(move |_, events| {
            heard.lock().unwrap().push(events.len());
            Ok(())
        }),
        render: Box::new(move |_, _| {
            let count = rendered.fetch_add(1, Ordering::Relaxed) + 1;
            Some(Node::text(format!("render {count}")))
        }),
        take_failure: Box::new(|| None),
    };
    let (mut plugins, _) = started(&mut app, plugin);
    plugins
        .run("busy.go", Args::new(), &mut app.context())
        .unwrap();
    // Asked as the command waits, each waits its turn: the renders of a
    // view once, the events in one batch.
    let view = ViewId::StatusBar(WindowHandle(window.get()));
    let created = |tab| {
        Event::Workspace(WorkspaceEvent::TabCreated {
            tab: host::TabId(tab),
            window: WindowHandle(window.get()),
            index: 0,
        })
    };
    let start = Instant::now();
    for tab in 1..=3 {
        plugins
            .render("acme/busy", &view, &mut app.context())
            .unwrap();
        plugins.post(created(tab));
        assert!(plugins.deliver(&mut app.context()).is_empty());
        let turn = plugins.serve(&mut app.context());
        assert!(!turn.any && !turn.more);
    }
    assert!(
        start.elapsed() < Duration::from_millis(500),
        "the main thread waited"
    );
    assert_eq!(plugins.rendered("acme/busy", &view), None);
    gate.send(()).unwrap();
    served(&mut plugins, &mut app);
    assert_eq!(renders.load(Ordering::Relaxed), 1);
    assert!(plugins.tree("acme/busy", &view).is_some());
    assert_eq!(*batches.lock().unwrap(), [3]);
}

#[test]
fn a_worker_s_render_keeps_the_last_tree_until_the_next_and_none_of_a_view_gone() {
    let mut app = App::default();
    let window = window(&mut app, "sleep 30");
    let (gate, held): (Sender<()>, Receiver<()>) = mpsc::channel();
    let held = Mutex::new(held);
    let count = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&count);
    let plugin = Threaded {
        render: Box::new(move |cx, _| {
            held.lock().unwrap().recv().unwrap();
            let size = cx.font_size().unwrap();
            let count = counted.fetch_add(1, Ordering::Relaxed) + 1;
            Some(Node::text(format!("{count} at {size}")))
        }),
        ..Threaded::default()
    };
    let (mut plugins, _) = started(&mut app, plugin);
    let view = ViewId::StatusBar(WindowHandle(window.get()));
    let text = |plugins: &Plugins| match &plugins.tree("acme/busy", &view).unwrap().kind {
        lokyr_plugin_sdk::view::Kind::Text(text) => text.clone(),
        other => panic!("{other:?}"),
    };
    plugins
        .render("acme/busy", &view, &mut app.context())
        .unwrap();
    gate.send(()).unwrap();
    served(&mut plugins, &mut app);
    let first = plugins.rendered("acme/busy", &view).unwrap();
    assert_eq!(text(&plugins), "1 at 13");
    // The next render under way, the last tree stays.
    plugins
        .render("acme/busy", &view, &mut app.context())
        .unwrap();
    plugins.serve(&mut app.context());
    assert_eq!(plugins.rendered("acme/busy", &view), Some(first));
    gate.send(()).unwrap();
    served(&mut plugins, &mut app);
    assert_ne!(plugins.rendered("acme/busy", &view), Some(first));
    assert_eq!(text(&plugins), "2 at 13");
    // A render that ends once its window closed keeps nothing.
    plugins
        .render("acme/busy", &view, &mut app.context())
        .unwrap();
    plugins.serve(&mut app.context());
    let closed = app.workspace.close_window(window);
    assert!(closed);
    plugins.post(Event::Workspace(WorkspaceEvent::WindowClosed {
        window: WindowHandle(window.get()),
    }));
    gate.send(()).unwrap();
    served(&mut plugins, &mut app);
    assert_eq!(count.load(Ordering::Relaxed), 3);
    assert_eq!(plugins.rendered("acme/busy", &view), None);
}

#[test]
fn a_worker_that_panics_is_disabled_and_its_call_s_writes_go() {
    let mut app = App::default();
    let window = window(&mut app, "echo hello; sleep 30");
    let (handle, pane) = focused(&app, window);
    text_once(&app, pane, "hello");
    app.workspace.pane(pane).unwrap().clear_selection();
    let plugin = Threaded {
        on_command: Box::new(move |cx, _| {
            cx.select_all(handle)?;
            panic!("broken");
        }),
        ..Threaded::default()
    };
    let (mut plugins, _) = started(&mut app, plugin);
    plugins
        .run("busy.go", Args::new(), &mut app.context())
        .unwrap();
    let turns = served(&mut plugins, &mut app);
    assert_eq!(plugins.status("acme/busy"), Some(Status::Disabled));
    let [fault] = &turns.errors[..] else {
        panic!("{:?}", turns.errors);
    };
    assert_eq!(fault.code, Code::Fault);
    assert!(fault.message.contains("acme/busy") && fault.message.contains("broken"));
    assert!(!plugins.owns("busy.go"));
    assert!(app.registry.get("busy.go").is_none());
    assert_eq!(app.workspace.pane(pane).unwrap().selection_text(), None);
    let [(name, told)] = &plugins.take_disabled()[..] else {
        panic!("one plugin stopped");
    };
    assert_eq!((name.as_str(), told), ("acme/busy", &fault.message));
    // Its thread ended: nothing is under way, and nothing more runs.
    assert!(!plugins.waiting());
    assert!(
        plugins
            .run("busy.go", Args::new(), &mut app.context())
            .is_err()
    );
}

#[test]
fn a_worker_s_failures_come_as_its_calls_end() {
    let mut app = App::default();
    let plugin = Threaded {
        on_command: Box::new(|_, _| Err(Error::new(Code::Failed, "nothing to do"))),
        ..Threaded::default()
    };
    let (mut plugins, _) = started(&mut app, plugin);
    plugins
        .run("busy.go", Args::new(), &mut app.context())
        .unwrap();
    let turns = served(&mut plugins, &mut app);
    let [failed] = &turns.commands[..] else {
        panic!("{turns:?}");
    };
    assert_eq!(failed.message, "nothing to do");
    assert!(turns.errors.is_empty());
    assert_eq!(plugins.status("acme/busy"), Some(Status::Active));
    // An activation that fails disables its plugin.
    let mut app = App::default();
    let plugin = Threaded {
        activate: Box::new(|cx| {
            cx.register_command(command("busy.go"))?;
            Err(Error::new(Code::Failed, "no luck"))
        }),
        ..Threaded::default()
    };
    let mut plugins = Plugins::new();
    plugins
        .add_builtin_send(MANIFEST, Box::new(plugin))
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    let turns = served(&mut plugins, &mut app);
    assert_eq!(plugins.status("acme/busy"), Some(Status::Disabled));
    let [failed] = &turns.errors[..] else {
        panic!("{turns:?}");
    };
    assert!(failed.message.contains("did not start") && failed.message.contains("no luck"));
    assert!(app.registry.get("busy.go").is_none());
    assert!(!plugins.waiting());
}

#[test]
fn a_turn_takes_the_workers_messages_for_its_time_at_most_each_in_turn() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    let wakes = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&wakes);
    plugins.set_wake(move || {
        counted.fetch_add(1, Ordering::Relaxed);
    });
    let mut gates = Vec::new();
    let (told, ended) = mpsc::channel();
    for name in ["one", "two"] {
        let (gate, held): (Sender<()>, Receiver<()>) = mpsc::channel();
        let told = told.clone();
        gates.push(gate);
        let manifest = MANIFEST
            .replace("acme/busy", &format!("acme/{name}"))
            .replace("slots = [\"status-bar\"]", "");
        let plugin = Threaded {
            activate: Box::new(move |cx| cx.register_command(command(&format!("{name}.go")))),
            on_command: Box::new(move |_, _| {
                held.recv().unwrap();
                told.send(name).unwrap();
                Ok(())
            }),
            ..Threaded::default()
        };
        plugins
            .add_builtin_send(&manifest, Box::new(plugin))
            .unwrap();
    }
    assert!(plugins.activate(&mut app.context()).is_empty());
    served(&mut plugins, &mut app);
    let busy = |plugins: &Plugins, at: usize| matches!(&plugins.entries[at].runner, super::super::Runner::Worker(worker) if worker.is_busy());
    // A turn of no time takes one message, and wakes the main thread for
    // those left, from the first plugin's, whichever a slow turn of the
    // activations' left first.
    plugins.serve_for = Some(Duration::ZERO);
    plugins.serve_from = 0;
    for name in ["one", "one", "two"] {
        plugins
            .run(&format!("{name}.go"), Args::new(), &mut app.context())
            .unwrap();
    }
    // The first plugin's second command waits for its gate, so that its
    // end wakes nothing until the turn below is counted.
    for at in [0, 1] {
        gates[at].send(()).unwrap();
    }
    let mut ends: Vec<&str> = ended.iter().take(2).collect();
    ends.sort_unstable();
    assert_eq!(ends, ["one", "two"]);
    // The first commands ended: their ends wait for the main thread.
    std::thread::sleep(Duration::from_millis(50));
    let woken = wakes.load(Ordering::Relaxed);
    let first = plugins.serve(&mut app.context());
    assert!(first.any && first.more);
    assert_eq!(wakes.load(Ordering::Relaxed), woken + 1);
    // The first plugin's end, whose second command began, and ends.
    assert!(busy(&plugins, 0) && busy(&plugins, 1));
    gates[0].send(()).unwrap();
    assert_eq!(ended.recv_timeout(Duration::from_secs(10)), Ok("one"));
    std::thread::sleep(Duration::from_millis(50));
    // The next turn takes the next plugin's first, though the first
    // plugin's waits too.
    let second = plugins.serve(&mut app.context());
    assert!(second.any && second.more);
    assert!(busy(&plugins, 0) && !busy(&plugins, 1));
    let third = plugins.serve(&mut app.context());
    assert!(third.any);
    assert!(!plugins.waiting());
    let fourth = plugins.serve(&mut app.context());
    assert!(!fourth.any && !fourth.more);
}

#[test]
fn a_menu_does_not_ask_a_worker_whether_its_command_applies() {
    /// A plugin whose commands would never apply, were it asked.
    struct Never;
    impl Plugin for Never {
        fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
            cx.register_command(command("busy.go"))
        }

        fn applies(&mut self, _: &mut Cx<'_>, _: &str, _: &Args) -> bool {
            false
        }
    }
    let mut app = App::default();
    let mut plugins = Plugins::new();
    plugins.add_builtin_send(MANIFEST, Box::new(Never)).unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    served(&mut plugins, &mut app);
    let applies = plugins.applies("busy.go", &Args::new(), &mut app.context());
    assert_eq!(applies, Ok(true));
    assert!(!plugins.waiting());
}

#[test]
fn a_read_of_rows_under_way_waits_without_an_event_and_comes_as_one() {
    let mut app = App::default();
    let window = window(&mut app, "seq 1 100; sleep 30");
    let (handle, pane) = focused(&app, window);
    text_once(&app, pane, "100");
    let log = Arc::new(Mutex::new(Vec::new()));
    let heard = Arc::clone(&log);
    let mut plugins = Plugins::new();
    plugins
        .add(
            "reader",
            Box::new(Threaded {
                activate: Box::new(|cx| cx.register_command(command("reader.go"))),
                on_command: Box::new(move |cx, _| cx.read_lines(handle, 0, 100).map(|_| ())),
                on_event: Box::new(move |_, events| {
                    heard.lock().unwrap().extend(events.iter().cloned());
                    Ok(())
                }),
                ..Threaded::default()
            }),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("reader.go", Args::new(), &mut app.context())
        .unwrap();
    // No event until the read ends, which a turn takes.
    assert!(!plugins.has_events());
    assert!(plugins.waiting());
    served(&mut plugins, &mut app);
    assert!(plugins.has_events());
    assert!(plugins.deliver(&mut app.context()).is_empty());
    let log = log.lock().unwrap();
    let [Event::Lines { lines, .. }] = &log[..] else {
        panic!("{log:?}");
    };
    assert_eq!(lines.len(), 100);
}

#[test]
fn a_slow_call_of_a_main_thread_s_plugin_is_noted_by_its_name() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    plugins
        .add(
            "sleepy",
            Box::new(Threaded {
                activate: Box::new(|cx| {
                    cx.register_command(command("sleepy.nap"))?;
                    cx.register_command(command("sleepy.wink"))
                }),
                on_command: Box::new(|_, name| {
                    if name == "sleepy.nap" {
                        std::thread::sleep(super::super::SLOW + Duration::from_millis(2));
                    }
                    Ok(())
                }),
                ..Threaded::default()
            }),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert!(plugins.take_slow().is_empty());
    plugins
        .run("sleepy.wink", Args::new(), &mut app.context())
        .unwrap();
    assert!(plugins.take_slow().is_empty());
    plugins
        .run("sleepy.nap", Args::new(), &mut app.context())
        .unwrap();
    let [slow] = &plugins.take_slow()[..] else {
        panic!("one slow call");
    };
    assert_eq!(
        (slow.plugin.as_str(), slow.callback),
        ("sleepy", "on_command")
    );
    assert!(slow.took >= super::super::SLOW);
    assert!(plugins.take_slow().is_empty());
}

#[test]
fn a_plugin_that_cannot_go_to_a_thread_does_not_take_a_worker_s_manifest() {
    let mut plugins = Plugins::new();
    let refused = plugins
        .add_builtin(MANIFEST, Box::new(Threaded::default()))
        .unwrap_err();
    assert_eq!(refused.code, Code::InvalidConfig);
    assert_eq!(plugins.status("acme/busy"), None);
    // One that can, of the main thread's manifest, runs there.
    let mut app = App::default();
    let main = MANIFEST.replace("thread = \"worker\"\n", "");
    plugins
        .add_builtin_send(&main, Box::new(Threaded::default()))
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert!(!plugins.waiting());
    assert!(app.registry.get("busy.go").is_some());
}

/// The reads of rows of the worker's call under way, if one is.
fn reads_of_the_call(plugins: &mut Plugins) -> Option<usize> {
    plugins
        .entries
        .iter_mut()
        .find_map(|entry| match &mut entry.runner {
            Runner::Worker(worker) => worker.busy_mut().map(|busy| busy.out.reads.len()),
            _ => None,
        })
}

/// Waits for the reads of rows of the worker's call under way to end, and
/// keeps their rows for the host, as though they ended before the call.
fn end_the_call_s_reads(plugins: &mut Plugins) {
    for entry in &mut plugins.entries {
        if let Runner::Worker(worker) = &mut entry.runner
            && let Some(busy) = worker.busy_mut()
        {
            for read in &mut busy.out.reads {
                let rows = read.rows.recv_timeout(Duration::from_secs(10)).unwrap();
                let (sender, receiver) = mpsc::channel();
                sender.send(rows).unwrap();
                read.rows = receiver;
            }
        }
    }
}

#[test]
fn a_worker_hears_the_events_of_its_subscriptions_posted_as_it_activates() {
    let mut app = App::default();
    let window = window(&mut app, "sleep 30");
    let log = Arc::new(Mutex::new(Vec::new()));
    let heard = Arc::clone(&log);
    let mut plugins = Plugins::new();
    plugins
        .add_builtin_send(
            MANIFEST,
            Box::new(Threaded {
                activate: Box::new(|cx| {
                    cx.register_command(command("busy.go"))?;
                    cx.subscribe(&["tab-created"])
                }),
                on_event: Box::new(move |_, events| {
                    let names = events.iter().map(|event| event.name().to_owned());
                    heard.lock().unwrap().extend(names);
                    Ok(())
                }),
                ..Threaded::default()
            }),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    // The first window's events, delivered before the activation's
    // requests are served, as the app's start delivers them.
    let window = host::WindowId(window.get());
    plugins.post(Event::Workspace(WorkspaceEvent::TabCreated {
        tab: host::TabId(TabId::of(1).get()),
        window,
        index: 0,
    }));
    plugins.post(Event::Workspace(WorkspaceEvent::WindowOpened { window }));
    assert!(plugins.deliver(&mut app.context()).is_empty());
    served(&mut plugins, &mut app);
    assert_eq!(*log.lock().unwrap(), ["tab-created"]);
    // Those after its activation come as they did.
    plugins.post(Event::Workspace(WorkspaceEvent::TabCreated {
        tab: host::TabId(TabId::of(2).get()),
        window,
        index: 1,
    }));
    assert!(plugins.deliver(&mut app.context()).is_empty());
    served(&mut plugins, &mut app);
    assert_eq!(*log.lock().unwrap(), ["tab-created", "tab-created"]);
}

#[test]
fn a_read_that_ends_before_its_worker_s_call_comes_in_the_turn_that_ends_the_call() {
    let mut app = App::default();
    let window = window(&mut app, "seq 1 100; sleep 30");
    let (handle, pane) = focused(&app, window);
    text_once(&app, pane, "100");
    let (go, gate) = mpsc::channel::<()>();
    let plugin = Threaded {
        on_command: Box::new(move |cx, _| {
            cx.read_lines(handle, 0, 100)?;
            // The call goes on after its read ended.
            gate.recv_timeout(Duration::from_secs(10)).unwrap();
            Ok(())
        }),
        ..Threaded::default()
    };
    let (mut plugins, _) = started(&mut app, plugin);
    plugins
        .run("busy.go", Args::new(), &mut app.context())
        .unwrap();
    let start = Instant::now();
    while reads_of_the_call(&mut plugins) != Some(1) {
        plugins.serve(&mut app.context());
        assert!(start.elapsed() < Duration::from_secs(10), "no read began");
        std::thread::sleep(Duration::from_millis(1));
    }
    end_the_call_s_reads(&mut plugins);
    go.send(()).unwrap();
    while reads_of_the_call(&mut plugins).is_some() {
        plugins.serve(&mut app.context());
        assert!(start.elapsed() < Duration::from_secs(10), "a call hangs");
        std::thread::sleep(Duration::from_millis(1));
    }
    // No wake is left to bring the read: the turn that ended the call
    // took it.
    assert!(plugins.has_events());
    assert!(!plugins.waiting());
}

#[test]
fn a_worker_s_call_reads_the_focus_of_the_time_that_it_was_asked() {
    let mut app = App::default();
    let first = window(&mut app, "sleep 30");
    let second = window(&mut app, "sleep 30");
    let (told, heard) = mpsc::channel();
    let plugin = Threaded {
        on_command: Box::new(move |cx, _| {
            told.send((cx.focused_window(), cx.focused_pane())).unwrap();
            Ok(())
        }),
        ..Threaded::default()
    };
    let (mut plugins, _) = started(&mut app, plugin);
    // A context menu's command in the window that has not the focus.
    app.focused = Some(first);
    plugins
        .run("busy.go", Args::new(), &mut app.context())
        .unwrap();
    app.focused = Some(second);
    served(&mut plugins, &mut app);
    let (pane, _) = focused(&app, first);
    assert_eq!(
        heard.recv_timeout(Duration::from_secs(10)).unwrap(),
        (Some(WindowHandle(first.get())), Some(pane))
    );
}

#[test]
fn the_commands_that_a_worker_s_command_runs_are_of_its_chain_whenever_its_call_ends() {
    let mut app = App::default();
    window(&mut app, "sleep 30");
    let plugin = Threaded {
        on_command: Box::new(|cx, _| cx.run_command("busy.go", Args::new())),
        ..Threaded::default()
    };
    let (mut plugins, _) = started(&mut app, plugin);
    plugins
        .run_deep(
            "busy.go",
            Args::new(),
            Chain { id: 7, depth: 3 },
            &mut app.context(),
        )
        .unwrap();
    served(&mut plugins, &mut app);
    assert_eq!(
        plugins.take_requests(),
        [Request::Run {
            invocation: Invocation {
                command: "busy.go".to_owned(),
                args: Args::new(),
            },
            chain: Chain { id: 7, depth: 3 },
        }]
    );
    // Those of an event's call, or of a command that the user ran, start
    // from the top, of a new chain.
    plugins
        .run("busy.go", Args::new(), &mut app.context())
        .unwrap();
    served(&mut plugins, &mut app);
    assert!(matches!(
        &plugins.take_requests()[..],
        [Request::Run {
            chain: Chain { id, depth: 0 },
            ..
        }] if *id != 0 && *id != 7
    ));
}

#[test]
fn a_worker_s_value_nested_too_deep_is_refused_on_its_thread() {
    let mut app = App::default();
    window(&mut app, "sleep 30");
    let (told, heard) = mpsc::channel();
    let plugin = Threaded {
        on_command: Box::new(move |cx, _| {
            let deep = super::super::tests::nested(1_000_000);
            told.send(
                cx.emit("busy.value", "busy.value/1", deep)
                    .map_err(|error| error.code),
            )
            .unwrap();
            Ok(())
        }),
        ..Threaded::default()
    };
    let (mut plugins, _) = started(&mut app, plugin);
    plugins
        .run("busy.go", Args::new(), &mut app.context())
        .unwrap();
    served(&mut plugins, &mut app);
    assert_eq!(
        heard.recv_timeout(Duration::from_secs(10)).unwrap(),
        Err(Code::Failed)
    );
    assert_eq!(plugins.status("acme/busy"), Some(Status::Active));
}

/// A chain of containers `depth` levels deep, a text at its end, made
/// without recursion.
fn chain(depth: usize) -> Node {
    (1..depth).fold(Node::text("leaf"), |inner, _| Node::container(vec![inner]))
}

#[test]
fn a_worker_s_view_nested_too_deep_is_refused_on_its_thread_and_the_last_stays() {
    let mut app = App::default();
    let window = window(&mut app, "sleep 30");
    let renders = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&renders);
    let plugin = Threaded {
        render: Box::new(move |_, _| {
            Some(match counted.fetch_add(1, Ordering::Relaxed) {
                0 => Node::text("first"),
                1 => chain(1_000_000),
                _ => chain(Node::DEEPEST),
            })
        }),
        ..Threaded::default()
    };
    let (mut plugins, _) = started(&mut app, plugin);
    let view = ViewId::StatusBar(WindowHandle(window.get()));
    let render = |plugins: &mut Plugins, app: &mut App| {
        plugins
            .render("acme/busy", &view, &mut app.context())
            .unwrap();
        served(plugins, app)
    };
    render(&mut plugins, &mut app);
    let first = plugins.rendered("acme/busy", &view).unwrap();
    // A million levels deep, refused, of no recursion on either thread:
    // the last tree stays, and the plugin goes on.
    let served = render(&mut plugins, &mut app);
    assert!(
        served.errors.iter().any(|error| error.code == Code::Failed),
        "{:?}",
        served.errors
    );
    assert_eq!(plugins.rendered("acme/busy", &view), Some(first));
    assert_eq!(plugins.status("acme/busy"), Some(Status::Active));
    // At the limit, taken.
    render(&mut plugins, &mut app);
    assert_ne!(plugins.rendered("acme/busy", &view), Some(first));
    assert!(
        plugins
            .tree("acme/busy", &view)
            .unwrap()
            .nests_within_deepest()
    );
}

#[test]
fn a_stuck_worker_s_plugin_is_disabled_as_the_work_that_waits_for_it_passes_its_bound() {
    let mut app = App::default();
    window(&mut app, "sleep 30");
    // A command that waits until the test lets it go, as a plugin stuck
    // in a call.
    let (free, freed) = mpsc::channel::<()>();
    let freed = std::sync::Mutex::new(freed);
    let plugin = Threaded {
        on_command: Box::new(move |_, _| {
            let _ = freed.lock().unwrap().recv();
            Ok(())
        }),
        ..Threaded::default()
    };
    let (mut plugins, _) = started(&mut app, plugin);
    plugins
        .run("busy.go", Args::new(), &mut app.context())
        .unwrap();
    // The calls that wait for it, up to the bound, and the one past it.
    for _ in 0..super::MOST_WAITING {
        plugins
            .run("busy.go", Args::new(), &mut app.context())
            .unwrap();
    }
    assert_eq!(plugins.status("acme/busy"), Some(Status::Active));
    let past = plugins
        .run("busy.go", Args::new(), &mut app.context())
        .unwrap_err();
    assert_eq!(past.code, Code::Fault);
    assert!(past.message.contains("took none"), "{past}");
    assert_eq!(plugins.status("acme/busy"), Some(Status::Disabled));
    // Its commands went, and nothing waits for its thread, which ends as
    // it is let go.
    assert!(app.registry.get("busy.go").is_none());
    assert!(!plugins.waiting());
    let _ = free.send(());
}

#[test]
fn a_request_that_breaks_the_host_disables_its_plugin_and_the_app_goes_on() {
    let mut app = App::default();
    window(&mut app, "sleep 30");
    let plugin = Threaded {
        on_command: Box::new(|cx, _| {
            cx.focused_pane()
                .map(drop)
                .ok_or_else(|| Error::new(Code::NotFound, "no pane"))
        }),
        ..Threaded::default()
    };
    let (mut plugins, _) = started(&mut app, plugin);
    super::super::BREAKS.with(|breaks| breaks.set(true));
    plugins
        .run("busy.go", Args::new(), &mut app.context())
        .unwrap();
    let served = served(&mut plugins, &mut app);
    assert!(
        served
            .errors
            .iter()
            .any(|error| error.code == Code::Fault && error.message.contains("the host broke")),
        "{:?}",
        served.errors
    );
    assert_eq!(plugins.status("acme/busy"), Some(Status::Disabled));
    // The host answers the next plugin's requests as before.
    assert!(!super::super::BREAKS.with(std::cell::Cell::get));
}

#[test]
fn a_worker_s_call_that_fails_after_its_writes_leaves_the_workspace_as_it_was() {
    let mut app = App::default();
    let window = window(&mut app, "sleep 30");
    let tab = app.workspace.window(window).unwrap().tabs()[0];
    let plugin = Threaded {
        on_command: Box::new(|cx, _| {
            let window = cx.focused_window().unwrap();
            let tab = cx.tabs(window).unwrap()[0].id;
            cx.set_tab_title(tab, Some("written"))?;
            cx.new_tab(window, None)?;
            Err(Error::new(Code::Failed, "after its writes"))
        }),
        ..Threaded::default()
    };
    let (mut plugins, _) = started(&mut app, plugin);
    plugins
        .run("busy.go", Args::new(), &mut app.context())
        .unwrap();
    let served = served(&mut plugins, &mut app);
    assert!(
        served
            .commands
            .iter()
            .any(|error| error.code == Code::Failed),
        "{served:?}"
    );
    assert_eq!(app.workspace.tab(tab).unwrap().own_title(), None);
    assert_eq!(app.workspace.window(window).unwrap().tabs().len(), 1);
    assert_eq!(plugins.status("acme/busy"), Some(Status::Active));
}

#[test]
fn a_worker_s_render_that_tells_its_fault_after_it_faults_its_plugin() {
    let mut app = App::default();
    let window = window(&mut app, "sleep 30");
    let renders = Arc::new(AtomicUsize::new(0));
    let (rendered, told) = (Arc::clone(&renders), Arc::clone(&renders));
    let plugin = Threaded {
        render: Box::new(move |_, _| {
            rendered.fetch_add(1, Ordering::Relaxed);
            Some(Node::text("fine"))
        }),
        // Its second render faults, as a Luau plugin's past its deadline.
        take_failure: Box::new(move || {
            (told.load(Ordering::Relaxed) == 2)
                .then(|| Error::new(Code::Fault, "past its deadline"))
        }),
        ..Threaded::default()
    };
    let (mut plugins, _) = started(&mut app, plugin);
    let view = ViewId::StatusBar(WindowHandle(window.get()));
    plugins
        .render("acme/busy", &view, &mut app.context())
        .unwrap();
    served(&mut plugins, &mut app);
    assert_eq!(plugins.tree("acme/busy", &view), Some(&Node::text("fine")));
    plugins
        .render("acme/busy", &view, &mut app.context())
        .unwrap();
    served(&mut plugins, &mut app);
    assert_eq!(renders.load(Ordering::Relaxed), 2);
    assert_eq!(plugins.status("acme/busy"), Some(Status::Disabled));
    assert_eq!(plugins.tree("acme/busy", &view), Some(&Node::text("fine")));
    let disabled = plugins.take_disabled();
    assert!(disabled[0].1.contains("past its deadline"), "{disabled:?}");
}

#[test]
fn a_worker_s_failed_call_that_closes_a_panel_leaves_it_open_of_its_tree() {
    use lokyr_plugin_sdk::ui::{Anchor, PanelId, PanelOptions, Role};

    let mut app = App::default();
    let shown = window(&mut app, "sleep 30");
    app.focused = Some(shown);
    let plugin = Threaded {
        activate: Box::new(|cx| {
            cx.register_command(command("busy.go"))?;
            cx.register_command(command("busy.fails"))
        }),
        on_command: Box::new(|cx, name| {
            if name == "busy.go" {
                let window = cx.focused_window().unwrap();
                return cx.open_panel(
                    "p",
                    PanelOptions {
                        anchor: Anchor::Pointer(window),
                        role: Role::Dialog,
                        close_outside: false,
                        close_escape: true,
                        takes_keys: false,
                        close_typing: false,
                    },
                );
            }
            cx.close_panel("p")?;
            Err(Error::new(Code::Failed, "on purpose"))
        }),
        render: Box::new(|_, _| Some(Node::text("p"))),
        ..Threaded::default()
    };
    let (mut plugins, _) = started(&mut app, plugin);
    plugins
        .run("busy.go", Args::new(), &mut app.context())
        .unwrap();
    served(&mut plugins, &mut app);
    plugins.take_requests();
    let panel = ViewId::Panel(PanelId::from("p"));
    plugins
        .render("acme/busy", &panel, &mut app.context())
        .unwrap();
    served(&mut plugins, &mut app);
    assert!(plugins.tree("acme/busy", &panel).is_some());
    plugins
        .run("busy.fails", Args::new(), &mut app.context())
        .unwrap();
    served(&mut plugins, &mut app);
    assert!(plugins.take_requests().is_empty());
    assert!(
        !plugins
            .out
            .events
            .iter()
            .any(|(_, event)| matches!(event, Event::PanelClosed(_)))
    );
    assert!(plugins.tree("acme/busy", &panel).is_some());
}

#[test]
fn a_worker_hears_its_timers_once_its_thread_is_free_and_none_that_it_cancelled_meanwhile() {
    let mut app = App::default();
    let (gate, held): (Sender<()>, Receiver<()>) = mpsc::channel();
    let ids = Arc::new(Mutex::new(Vec::new()));
    let batches = Arc::new(Mutex::new(Vec::new()));
    let (started_ids, cancelled_ids, heard) =
        (Arc::clone(&ids), Arc::clone(&ids), Arc::clone(&batches));
    let plugin = Threaded {
        activate: Box::new(move |cx| {
            cx.register_command(command("busy.go"))?;
            for _ in 0..2 {
                let id = cx.start_timer(Duration::from_millis(50), true)?;
                started_ids.lock().unwrap().push(id);
            }
            Ok(())
        }),
        on_command: Box::new(move |cx, _| {
            held.recv().unwrap();
            let cancelled = cancelled_ids.lock().unwrap()[1];
            cx.cancel_timer(cancelled)
        }),
        on_event: Box::new(move |_, events| {
            let fired: Vec<u64> = events
                .iter()
                .filter_map(|event| match event {
                    Event::Timer { id } => Some(*id),
                    _ => None,
                })
                .collect();
            heard.lock().unwrap().push(fired);
            Ok(())
        }),
        ..Threaded::default()
    };
    let (mut plugins, _) = started(&mut app, plugin);
    let [kept, _] = ids.lock().unwrap()[..] else {
        panic!("two timers");
    };
    plugins
        .run("busy.go", Args::new(), &mut app.context())
        .unwrap();
    // Both fire as its command waits: their events wait for its thread, of
    // no delivery meanwhile.
    let now = app.wait(Duration::from_millis(50));
    plugins.timers_due(now);
    assert!(!plugins.has_events());
    assert!(plugins.deliver(&mut app.context()).is_empty());
    // Its command cancels one: the other's event comes once it is free.
    gate.send(()).unwrap();
    served(&mut plugins, &mut app);
    assert!(plugins.has_events());
    assert!(plugins.deliver(&mut app.context()).is_empty());
    served(&mut plugins, &mut app);
    assert_eq!(*batches.lock().unwrap(), [vec![kept]]);
    assert!(!plugins.has_events());
}
