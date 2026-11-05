//! The conformance suite of the plugin API: scenarios that a test plugin
//! runs, each of calls, which make requests, and of deliveries of events,
//! whose trace, each request with its answer and each batch of events that
//! the plugin heard, every host and every front end of the API gives
//! alike: the kernel's host, and the SDK's fake host, which plugins' unit
//! tests take for it, both of the Rust front end. A front end of another
//! language takes the scenarios as JSON, `scenarios::json`, and gives its
//! trace to compare.

mod scenarios;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::{Duration, Instant};

use lokyr_kernel::commands::{Args, Category, Command, Error, Registry};
use lokyr_kernel::l10n::{KERNEL, Texts};
use lokyr_kernel::plugins::{Clipboard, Context, Cx, Event, Panes, Plugin, Plugins};
use lokyr_kernel::workspace::{Environment, Grid, Pane, PaneId, Settings, Workspace};
use lokyr_plugin_sdk::command::Capability;
use lokyr_plugin_sdk::fake::FakeHost;
use lokyr_plugin_sdk::host;
use lokyr_plugin_sdk::message::{Request, Response};
use lokyr_pty::Program;
use lokyr_vt_core::terminal::Colors;

use scenarios::{Scenario, World};

/// What the test plugin did and heard, in its order.
#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    /// A request, and the host's answer, of which the ids of a read and of
    /// a decoration, each host's own, are 0.
    Asked(Request, Response),
    /// A batch of events.
    Heard(Vec<Event>),
}

/// The name of the test plugin's one command, whose calls make a step's
/// requests.
const ASK: &str = "conformer.ask";

/// The test plugin: each call of its command makes the next step's
/// requests, in their order, through the front end's own requests; its
/// trace takes them with their answers, and the events that it hears.
struct Conformer {
    asks: VecDeque<Vec<Request>>,
    subscriptions: Vec<String>,
    trace: Rc<RefCell<Vec<Entry>>>,
}

impl Plugin for Conformer {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        cx.register_command(Command {
            name: ASK.to_owned(),
            title: "Ask".to_owned(),
            category: Category::View,
            capabilities: Vec::new(),
            params: Vec::new(),
            when: None,
        })?;
        let names: Vec<&str> = self.subscriptions.iter().map(String::as_str).collect();
        cx.subscribe(&names)
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, _: &str, _: Args) -> Result<(), Error> {
        for request in self.asks.pop_front().unwrap_or_default() {
            let response = normalized(cx.request(request.clone()));
            self.trace
                .borrow_mut()
                .push(Entry::Asked(request, response));
        }
        Ok(())
    }

    fn on_event(&mut self, _: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        self.trace.borrow_mut().push(Entry::Heard(events.to_vec()));
        Ok(())
    }
}

/// `response`, of which the ids of a read and of a decoration are 0.
fn normalized(response: Response) -> Response {
    match response {
        Response::Decoration(_) => Response::Decoration(0),
        Response::Reading(_) => Response::Reading(0),
        other => other,
    }
}

/// The test plugin of `scenario`, and its trace.
fn conformer(scenario: &Scenario) -> (Conformer, Rc<RefCell<Vec<Entry>>>) {
    let trace = Rc::new(RefCell::new(Vec::new()));
    let plugin = Conformer {
        asks: scenario
            .steps
            .iter()
            .filter(|step| !step.asks.is_empty())
            .map(|step| {
                step.asks
                    .iter()
                    .map(|(request, _)| request.clone())
                    .collect()
            })
            .collect(),
        subscriptions: scenario.subscriptions.clone(),
        trace: Rc::clone(&trace),
    };
    (plugin, trace)
}

/// The trace that `scenario` asks for: each step's requests with their
/// answers, and the events that each step has the plugin hear.
fn expected(scenario: &Scenario) -> Vec<Entry> {
    let mut trace = Vec::new();
    for step in &scenario.steps {
        trace.extend(
            step.asks
                .iter()
                .map(|(request, response)| Entry::Asked(request.clone(), response.clone())),
        );
        if !step.heard.is_empty() {
            trace.push(Entry::Heard(step.heard.clone()));
        }
    }
    trace
}

/// The clipboard of the kernel's host.
#[derive(Default)]
struct Board(Option<String>);

impl Clipboard for Board {
    fn text(&self) -> Option<String> {
        self.0.clone()
    }

    fn set_text(&mut self, text: &str) {
        self.0 = Some(text.to_owned());
    }
}

/// The grid of the kernel's host's panes.
const GRID: Grid = Grid {
    columns: 80,
    rows: 24,
    cell_width: 8,
    cell_height: 16,
};

/// What starts the panes of the kernel's host: a shell that waits.
struct Spawner;

impl Panes for Spawner {
    fn spawn(&mut self, id: PaneId, _: Option<&Pane>) -> Pane {
        Pane::spawn_as(
            id,
            &Program::command("/bin/sh", ["-c", "sleep 30"]),
            &Environment::default(),
            &Settings::default(),
            GRID,
            &Colors::default(),
            &Texts::new("en", &KERNEL),
            || {},
        )
    }
}

/// The kernel's host's state: its registry, its workspace and its
/// clipboard, of the world of the scenarios, a window of a pane in `/`
/// whose program wrote [`scenarios::LINE`].
struct Kernel {
    registry: Registry,
    workspace: Workspace,
    spawner: Spawner,
    clipboard: Board,
    world: World,
}

impl Kernel {
    fn new() -> Self {
        let pane = Pane::spawn(
            &Program::command(
                "/bin/sh",
                [
                    "-c",
                    &format!("cd /; printf '%s\\n' '{}'; sleep 30", scenarios::LINE),
                ],
            ),
            &Environment::default(),
            &Settings::default(),
            GRID,
            &Colors::default(),
            &Texts::new("en", &KERNEL),
            || {},
        );
        let id = pane.id();
        let mut workspace = Workspace::new();
        let window = workspace.open_window(pane);
        let tab = workspace.tab_of(id).expect("the window's tab");
        // The program's line, and its directory, as the pane reads them.
        let start = Instant::now();
        let (row, line) = loop {
            let pane = workspace.pane_mut(id).expect("the pane");
            pane.update();
            let found = (0..GRID.rows.into()).find_map(|row| {
                pane.line_text(row, 0)
                    .filter(|(text, _)| text.trim_end() == scenarios::LINE)
                    .map(|line| (row, line))
            });
            let directory = pane.working_directory();
            if let Some(found) = found
                && directory.as_deref() == Some(std::path::Path::new("/"))
            {
                break found;
            }
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "the program wrote nothing, in {directory:?}"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        Self {
            registry: Registry::default(),
            workspace,
            spawner: Spawner,
            clipboard: Board(Some(scenarios::CLIPBOARD.to_owned())),
            world: World {
                window: host::WindowId(window.get()),
                tab: host::TabId(tab.get()),
                pane: host::PaneId(id.get()),
                row,
                line: host::LineText {
                    text: line.0,
                    cells: line.1,
                },
            },
        }
    }

    fn context(&mut self) -> Context<'_> {
        Context {
            registry: &mut self.registry,
            workspace: &mut self.workspace,
            panes: &mut self.spawner,
            clipboard: &mut self.clipboard,
            focused: Some(lokyr_kernel::workspace::WindowId::of(self.world.window.0)),
            font_size: 13.0,
            confirm_paste: lokyr_kernel::config::ConfirmPaste::Auto,
        }
    }
}

/// The plugin's name, of its manifest, which the scenarios' messages name.
fn name(scenario: &Scenario) -> &'static str {
    if scenario.capabilities.is_some() {
        "acme/conformer"
    } else {
        "conformer"
    }
}

/// The trace of `scenario` on the kernel's host, whose world `kernel` is.
fn on_kernel(scenario: &Scenario, kernel: &mut Kernel) -> Vec<Entry> {
    // Each scenario's plugin registers its command anew.
    kernel.registry = Registry::default();
    let (plugin, trace) = conformer(scenario);
    let mut plugins = Plugins::new();
    match &scenario.capabilities {
        None => plugins.add(name(scenario), Box::new(plugin)).unwrap(),
        Some(capabilities) => {
            let manifest =
                lokyr_pkg_model::manifest::Manifest::parse(&manifest(capabilities)).unwrap();
            plugins.add_declared(manifest, Box::new(plugin)).unwrap();
        }
    }
    assert!(plugins.activate(&mut kernel.context()).is_empty());
    let deliver = |plugins: &mut Plugins, kernel: &mut Kernel| {
        let start = Instant::now();
        while plugins.waiting() {
            plugins.serve(&mut kernel.context());
            assert!(start.elapsed() < Duration::from_secs(10));
        }
        assert!(plugins.deliver(&mut kernel.context()).is_empty());
    };
    for step in &scenario.steps {
        if !step.asks.is_empty() {
            // Its own failures are the requests' answers in the trace.
            let _ = plugins.run(ASK, Args::new(), &mut kernel.context());
        }
        for event in &step.posts {
            plugins.post(event.clone());
        }
        deliver(&mut plugins, kernel);
    }
    trace.borrow().clone()
}

/// The trace of `scenario` on the SDK's fake host, of the same world as
/// the kernel's host's.
fn on_fake(scenario: &Scenario, world: &World) -> Vec<Entry> {
    let (mut plugin, trace) = conformer(scenario);
    let mut host = FakeHost::new(name(scenario));
    if let Some(capabilities) = &scenario.capabilities {
        host = host.with_capabilities(capabilities.clone());
    }
    host.set_clipboard(Some(scenarios::CLIPBOARD));
    host.focus(Some(world.pane));
    let pane = host.pane(world.pane);
    pane.window = world.window;
    pane.tab = world.tab;
    pane.directory = Some("/".to_owned());
    pane.lines.insert(world.row, world.line.clone());
    host.activate(&mut plugin).unwrap();
    for step in &scenario.steps {
        if !step.asks.is_empty() {
            let _ = host.command(&mut plugin, ASK, Args::new());
        }
        for event in &step.posts {
            host.post(event.clone());
        }
        host.deliver(&mut plugin).unwrap();
    }
    trace.borrow().clone()
}

/// The manifest of a plugin of `capabilities` alone.
fn manifest(capabilities: &[Capability]) -> String {
    let mut text = "[plugin]\nname = \"acme/conformer\"\nkind = \"builtin\"\nlicense = \"MIT\"\n[capabilities]\n".to_owned();
    for capability in capabilities {
        text.push_str(&format!("{capability} = true\n"));
    }
    text
}

#[test]
fn every_scenario_s_trace_is_the_same_on_the_kernel_s_host_and_on_the_fake() {
    let mut kernel = Kernel::new();
    let world = kernel.world.clone();
    let scenarios = scenarios::all(&world);
    assert!(scenarios.len() >= 4);
    for scenario in &scenarios {
        let wanted = expected(scenario);
        assert_eq!(
            on_kernel(scenario, &mut kernel),
            wanted,
            "the kernel's host, `{}`",
            scenario.name
        );
        assert_eq!(
            on_fake(scenario, &world),
            wanted,
            "the fake host, `{}`",
            scenario.name
        );
    }
}

#[test]
fn the_scenarios_cross_as_json_and_come_back_the_same() {
    let world = Kernel::new().world;
    for scenario in scenarios::all(&world) {
        let text = scenarios::json(&scenario);
        assert_eq!(scenarios::from_json(&text), scenario, "{text}");
    }
}
