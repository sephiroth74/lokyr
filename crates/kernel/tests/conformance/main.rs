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

use lokyr_kernel::commands::{Args, Error, Registry};
use lokyr_kernel::l10n::{KERNEL, Texts};
use lokyr_kernel::plugins::{
    Clipboard, Closing, Context, Cx, Event, Panes, Plugin, Plugins, Request as AppRequest,
};
use lokyr_kernel::workspace::{Environment, Grid, Pane, PaneId, Settings, Workspace};
use lokyr_plugin_sdk::command::Capability;
use lokyr_plugin_sdk::event::WorkspaceEvent;
use lokyr_plugin_sdk::fake::FakeHost;
use lokyr_plugin_sdk::host::{self, Layout, TabInfo, WindowInfo};
use lokyr_plugin_sdk::message::{Request, Response};
use lokyr_plugin_sdk::ui::ViewId;
use lokyr_plugin_sdk::view::{ListItem, Node};
use lokyr_pty::Program;
use lokyr_vt_core::terminal::Colors;

use scenarios::{SYMBOL, Scenario, World};

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
/// requests, in their order, through the front end's own requests, of the
/// host's ids of their symbols; its trace takes them with their answers,
/// and the events that it hears, of the symbols of the ids that the
/// scenario opened. Its panel's tree is of a field and a list.
struct Conformer {
    asks: VecDeque<Vec<Request>>,
    subscriptions: Vec<String>,
    symbols: Symbols,
    trace: Rc<RefCell<Vec<Entry>>>,
}

impl Plugin for Conformer {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        cx.register_command(scenarios::ask_command())?;
        let names: Vec<&str> = self.subscriptions.iter().map(String::as_str).collect();
        cx.subscribe(&names)
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, _: &str, _: Args) -> Result<(), Error> {
        for request in self.asks.pop_front().unwrap_or_default() {
            let asked = self.symbols.resolved(&request);
            let response = self.symbols.response(cx.request(asked));
            self.trace
                .borrow_mut()
                .push(Entry::Asked(request, response));
        }
        Ok(())
    }

    fn on_event(&mut self, _: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        let heard = events
            .iter()
            .map(|event| self.symbols.event(event.clone()))
            .collect();
        self.trace.borrow_mut().push(Entry::Heard(heard));
        Ok(())
    }

    fn render(&mut self, _: &mut Cx<'_>, view: &ViewId) -> Option<Node> {
        let ViewId::Panel(_) = view else {
            return None;
        };
        Some(Node::container(vec![
            Node::field("Find").key("query"),
            Node::list(
                Node::text("row"),
                2,
                (0, vec![ListItem::default(), ListItem::default()]),
            )
            .key("results"),
        ]))
    }
}

/// The ids of the windows, the tabs and the panes that a scenario opened,
/// each host's own, in the order in which the trace met them, the `k`th of
/// each kind of the symbol `k`; the world's ids stand for themselves.
struct Symbols {
    world: World,
    windows: Vec<u64>,
    tabs: Vec<u64>,
    panes: Vec<u64>,
}

impl Symbols {
    fn new(world: &World) -> Self {
        Self {
            world: world.clone(),
            windows: Vec::new(),
            tabs: Vec::new(),
            panes: Vec::new(),
        }
    }

    /// `request` of the host's ids of its symbols, which the trace met
    /// before.
    fn resolved(&self, request: &Request) -> Request {
        fn resolve(value: &mut serde_json::Value, symbols: &Symbols) {
            match value {
                serde_json::Value::Number(number) => {
                    if let Some(symbol) = number.as_u64().filter(|&id| id >= SYMBOL) {
                        let (kind, k) = (symbol / SYMBOL, (symbol % SYMBOL) as usize);
                        let ids = match kind {
                            1 => &symbols.windows,
                            2 => &symbols.tabs,
                            _ => &symbols.panes,
                        };
                        let id = ids.get(k).unwrap_or_else(|| {
                            panic!(
                                "the symbol {k} of the kind {kind}, which the trace did not meet"
                            )
                        });
                        *value = serde_json::Value::from(*id);
                    }
                }
                serde_json::Value::Array(items) => {
                    items.iter_mut().for_each(|item| resolve(item, symbols));
                }
                serde_json::Value::Object(map) => {
                    map.values_mut().for_each(|item| resolve(item, symbols));
                }
                _ => {}
            }
        }
        let mut value = serde_json::to_value(request).expect("a request writes as JSON");
        resolve(&mut value, self);
        serde_json::from_value(value).expect("a request reads from JSON")
    }

    /// The symbol of the id `id`, of the kind `kind`, of those of `ids`.
    fn symbol(ids: &mut Vec<u64>, id: u64, kind: u64) -> u64 {
        let k = match ids.iter().position(|&met| met == id) {
            Some(k) => k,
            None => {
                ids.push(id);
                ids.len() - 1
            }
        };
        kind * SYMBOL + k as u64
    }

    fn window(&mut self, id: host::WindowId) -> host::WindowId {
        if id == self.world.window {
            return id;
        }
        host::WindowId(Self::symbol(&mut self.windows, id.0, 1))
    }

    fn tab(&mut self, id: host::TabId) -> host::TabId {
        if id == self.world.tab {
            return id;
        }
        host::TabId(Self::symbol(&mut self.tabs, id.0, 2))
    }

    fn pane(&mut self, id: host::PaneId) -> host::PaneId {
        if id == self.world.pane {
            return id;
        }
        host::PaneId(Self::symbol(&mut self.panes, id.0, 3))
    }

    fn layout(&mut self, layout: Layout) -> Layout {
        match layout {
            Layout::Pane(pane) => Layout::Pane(self.pane(pane)),
            Layout::Split {
                direction,
                ratio,
                first,
                second,
            } => Layout::Split {
                direction,
                ratio,
                first: Box::new(self.layout(*first)),
                second: Box::new(self.layout(*second)),
            },
        }
    }

    /// `response` of the symbols of its ids, of which the ids of a read
    /// and of a decoration are 0.
    fn response(&mut self, response: Response) -> Response {
        match response {
            Response::Decoration(_) => Response::Decoration(0),
            Response::Reading(_) => Response::Reading(0),
            Response::Searching(_) => Response::Searching(0),
            Response::Asking(_) => Response::Asking(0),
            Response::Window(window) => Response::Window(window.map(|id| self.window(id))),
            Response::Pane(pane) => Response::Pane(pane.map(|id| self.pane(id))),
            Response::Tab { tab, pane } => Response::Tab {
                tab: self.tab(tab),
                pane: self.pane(pane),
            },
            Response::Windows(windows) => Response::Windows(
                windows
                    .into_iter()
                    .map(|window| WindowInfo {
                        id: self.window(window.id),
                        tabs: window.tabs.into_iter().map(|id| self.tab(id)).collect(),
                        active: self.tab(window.active),
                        ..window
                    })
                    .collect(),
            ),
            Response::Tabs(tabs) => Response::Tabs(
                tabs.into_iter()
                    .map(|info| TabInfo {
                        id: self.tab(info.id),
                        window: self.window(info.window),
                        title: info.title,
                        color: info.color,
                        active: info.active,
                        focused: self.pane(info.focused),
                        panes: info.panes.into_iter().map(|id| self.pane(id)).collect(),
                        zoomed: info.zoomed.map(|id| self.pane(id)),
                        ..info
                    })
                    .collect(),
            ),
            Response::Layout(layout) => Response::Layout(self.layout(layout)),
            other => other,
        }
    }

    /// `event` of the symbols of its ids, of which a read's id is 0.
    fn event(&mut self, event: Event) -> Event {
        use WorkspaceEvent as W;
        match event {
            Event::Lines {
                pane,
                lines,
                complete,
                ..
            } => Event::Lines {
                id: 0,
                pane: self.pane(pane),
                lines,
                complete,
            },
            Event::Searched {
                pane,
                matches,
                count,
                done,
                ..
            } => Event::Searched {
                id: 0,
                pane: self.pane(pane),
                matches,
                count,
                done,
            },
            Event::Answered { text, .. } => Event::Answered { id: 0, text },
            Event::Workspace(change) => Event::Workspace(match change {
                W::WindowOpened { window } => W::WindowOpened {
                    window: self.window(window),
                },
                W::WindowClosed { window } => W::WindowClosed {
                    window: self.window(window),
                },
                W::TabCreated { tab, window, index } => W::TabCreated {
                    tab: self.tab(tab),
                    window: self.window(window),
                    index,
                },
                W::TabClosed { tab, window } => W::TabClosed {
                    tab: self.tab(tab),
                    window: self.window(window),
                },
                W::TabActivated { tab, window } => W::TabActivated {
                    tab: self.tab(tab),
                    window: self.window(window),
                },
                W::TabMoved {
                    tab,
                    from,
                    window,
                    index,
                } => W::TabMoved {
                    tab: self.tab(tab),
                    from: self.window(from),
                    window: self.window(window),
                    index,
                },
                W::PaneCreated { pane, tab } => W::PaneCreated {
                    pane: self.pane(pane),
                    tab: self.tab(tab),
                },
                W::PaneClosed { pane, tab } => W::PaneClosed {
                    pane: self.pane(pane),
                    tab: self.tab(tab),
                },
                W::PaneFocused { pane, tab } => W::PaneFocused {
                    pane: self.pane(pane),
                    tab: self.tab(tab),
                },
                W::PaneMoved { pane, from, tab } => W::PaneMoved {
                    pane: self.pane(pane),
                    from: self.tab(from),
                    tab: self.tab(tab),
                },
                W::LayoutChanged { tab } => W::LayoutChanged { tab: self.tab(tab) },
                W::TabChanged { tab } => W::TabChanged { tab: self.tab(tab) },
                W::PaneChanged { pane, tab } => W::PaneChanged {
                    pane: self.pane(pane),
                    tab: self.tab(tab),
                },
            }),
            other => other,
        }
    }
}

/// The test plugin of `scenario`, in `world`, and its trace.
fn conformer(scenario: &Scenario, world: &World) -> (Conformer, Rc<RefCell<Vec<Entry>>>) {
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
        symbols: Symbols::new(world),
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
    fn spawn(&mut self, id: PaneId, _: Option<&Pane>, _: Option<&str>) -> Pane {
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
            keys: &keys,
            enabled: &|_| true,
            themes: &themes,
            language: "en",
        }
    }
}

/// The keys of the world's bindings, as the platform writes them: `⌘1` of
/// the first tab's.
fn keys(command: &str, args: &Args) -> Option<String> {
    (command == "tabs.goto" && *args == goto_first()).then(|| "⌘1".to_owned())
}

/// The themes of the world, as the fake host's are at first: the built-in
/// ones, dark in force.
fn themes() -> lokyr_plugin_sdk::host::Themes {
    use lokyr_plugin_sdk::host::{ThemeInfo, ThemeOrigin, Themes};
    use lokyr_plugin_sdk::theme::ThemeVariant;
    Themes {
        appearance: ThemeVariant::Dark,
        current: "Lokyr Dark".to_owned(),
        themes: vec![
            ThemeInfo {
                name: "Lokyr Light".to_owned(),
                variant: ThemeVariant::Light,
                origin: ThemeOrigin::BuiltIn,
            },
            ThemeInfo {
                name: "Lokyr Dark".to_owned(),
                variant: ThemeVariant::Dark,
                origin: ThemeOrigin::BuiltIn,
            },
        ],
    }
}

/// The arguments of the first tab's selection.
fn goto_first() -> Args {
    Args::from([(
        "index".to_owned(),
        lokyr_kernel::commands::Value::Integer(1),
    )])
}

/// The plugin's name, of its manifest, which the scenarios' messages name.
fn name(scenario: &Scenario) -> &'static str {
    if scenario.capabilities.is_some() {
        "acme/conformer"
    } else {
        "conformer"
    }
}

/// The trace of `scenario` on the kernel's host, whose world `kernel` is,
/// of a state directory of its own, `state`: the host's part of the app's,
/// the workspace's changes posted to the plugins and the renders that
/// their calls ask for made, after each step's call.
fn on_kernel(scenario: &Scenario, kernel: &mut Kernel, state: &std::path::Path) -> Vec<Entry> {
    // Each scenario's plugin registers its command anew.
    kernel.registry = Registry::default();
    let _ = kernel.workspace.take_events();
    let world = kernel.world.clone();
    let (plugin, trace) = conformer(scenario, &world);
    let mut plugins = Plugins::new();
    match &scenario.capabilities {
        None => plugins.add(name(scenario), Box::new(plugin)).unwrap(),
        Some(capabilities) => {
            let manifest =
                lokyr_pkg_model::manifest::Manifest::parse(&manifest(capabilities)).unwrap();
            plugins.add_declared(manifest, Box::new(plugin)).unwrap();
        }
    }
    plugins.set_state_directory(Some(state.to_path_buf()));
    assert!(plugins.activate(&mut kernel.context()).is_empty());
    let deliver = |plugins: &mut Plugins, kernel: &mut Kernel| {
        let start = Instant::now();
        while plugins.waiting() {
            plugins.serve(&mut kernel.context());
            assert!(start.elapsed() < Duration::from_secs(10));
        }
        for (name, view) in plugins.take_renders() {
            plugins.render(&name, &view, &mut kernel.context()).unwrap();
        }
        // A close that asks closes, as the user confirms it.
        for request in plugins.take_requests() {
            match request {
                AppRequest::CloseAsked(Closing::Tab(tab)) => {
                    kernel.workspace.close_tab(tab);
                }
                AppRequest::CloseAsked(Closing::Pane(pane)) => {
                    kernel.workspace.close_pane(pane);
                }
                _ => {}
            }
        }
        for event in kernel.workspace.take_events() {
            if let lokyr_kernel::workspace::Event::Changed(change) = event {
                plugins.post(Event::Workspace(change));
            }
        }
        assert!(plugins.deliver(&mut kernel.context()).is_empty());
    };
    for step in &scenario.steps {
        if !step.asks.is_empty() {
            // Its own failures are the requests' answers in the trace.
            let _ = plugins.run(ASK, Args::new(), &mut kernel.context());
        }
        for event in &step.posts {
            // An answer goes to its plugin alone.
            match event {
                Event::Answered { id, text } => plugins.answer(name(scenario), *id, text.clone()),
                Event::TabBarResized { width } => plugins.tab_bar_resized(name(scenario), *width),
                event => plugins.post(event.clone()),
            }
        }
        deliver(&mut plugins, kernel);
    }
    trace.borrow().clone()
}

/// The trace of `scenario` on the SDK's fake host, of the same world as
/// the kernel's host's.
fn on_fake(scenario: &Scenario, world: &World) -> Vec<Entry> {
    let (mut plugin, trace) = conformer(scenario, world);
    // The plugin fills no slot, as a manifest of none.
    let mut host = FakeHost::new(name(scenario)).with_slots(Vec::new());
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
    pane.rows
        .insert(world.row, (scenarios::LINE.to_owned(), false));
    host.set_keys("tabs.goto", goto_first(), Some("⌘1"));
    host.activate(&mut plugin).unwrap();
    for step in &scenario.steps {
        if !step.asks.is_empty() {
            let _ = host.command(&mut plugin, ASK, Args::new());
        }
        for view in host.take_renders() {
            host.render(&mut plugin, &view);
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
    assert!(scenarios.len() >= 9);
    let states = std::env::temp_dir().join(format!("lokyr-conformance-{}", std::process::id()));
    for (at, scenario) in scenarios.iter().enumerate() {
        let wanted = expected(scenario);
        let state = states.join(at.to_string());
        assert_eq!(
            on_kernel(scenario, &mut kernel, &state),
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
    let _ = std::fs::remove_dir_all(&states);
}

#[test]
fn the_scenarios_cross_as_json_and_come_back_the_same() {
    let world = Kernel::new().world;
    for scenario in scenarios::all(&world) {
        let text = scenarios::json(&scenario);
        assert_eq!(scenarios::from_json(&text), scenario, "{text}");
    }
}

/// The place of `request`'s kind among the plugin API's requests, of a
/// match that a new kind does not compile without.
fn kind(request: &Request) -> usize {
    match request {
        Request::RegisterCommand(_) => 0,
        Request::Bind(_) => 1,
        Request::Subscribe(_) => 2,
        Request::Emit { .. } => 3,
        Request::RunCommand { .. } => 4,
        Request::ClipboardText => 5,
        Request::SetClipboardText(_) => 6,
        Request::FocusedWindow => 7,
        Request::FocusedPane => 8,
        Request::SelectionText(_) => 9,
        Request::LineText { .. } => 10,
        Request::LinkAt { .. } => 11,
        Request::WorkingDirectory(_) => 12,
        Request::ReadLines { .. } => 13,
        Request::Styles { .. } => 14,
        Request::Decorate { .. } => 15,
        Request::Undecorate { .. } => 16,
        Request::Open(_) => 17,
        Request::Opens(_) => 18,
        Request::SelectAll(_) => 19,
        Request::ClearScrollback(_) => 20,
        Request::Clear(_) => 21,
        Request::Paste { .. } => 22,
        Request::SendText { .. } => 23,
        Request::Scroll { .. } => 24,
        Request::FontSize => 25,
        Request::SetFontSize(_) => 26,
        Request::OpenWindow { .. } => 27,
        Request::CloseWindow(_) => 28,
        Request::Quit => 29,
        Request::Windows => 30,
        Request::Tabs(_) => 31,
        Request::Layout(_) => 32,
        Request::NewTab { .. } => 33,
        Request::Split { .. } => 34,
        Request::CloseTab { .. } => 35,
        Request::ClosePane { .. } => 36,
        Request::ActivateTab(_) => 37,
        Request::FocusPane(_) => 38,
        Request::MoveTab { .. } => 39,
        Request::TabToNewWindow { .. } => 40,
        Request::PaneToNewTab { .. } => 41,
        Request::PaneToNewWindow(_) => 42,
        Request::ResizePane { .. } => 43,
        Request::SetRatio { .. } => 44,
        Request::Equalize(_) => 45,
        Request::Zoom { .. } => 46,
        Request::RequestRender(_) => 47,
        Request::SetText { .. } => 48,
        Request::Select { .. } => 49,
        Request::OpenPanel { .. } => 50,
        Request::ClosePanel(_) => 51,
        Request::Config => 52,
        Request::State => 53,
        Request::SetState(_) => 54,
        Request::SetTabTitle { .. } => 55,
        Request::SetTabColor { .. } => 56,
        Request::AskText { .. } => 57,
        Request::SetTabBarPlace(_) => 58,
        Request::TabBarPlace => 59,
        Request::KeysOf { .. } => 60,
        Request::Message { .. } => 61,
        Request::Reveal { .. } => 62,
        Request::PaneGrid(_) => 63,
        Request::SwapPanes { .. } => 64,
        Request::SetWindowTabBarPlace { .. } => 65,
        Request::TabIntoPane { .. } => 66,
        Request::SetInactiveDim(_) => 67,
        Request::SetFocusFollowsMouse(_) => 68,
        Request::PaneTitle(_) => 69,
        Request::Search { .. } => 70,
        Request::EndSearch(_) => 71,
        Request::SearchMatches { .. } => 72,
        Request::SelectCells { .. } => 73,
        Request::FocusNode { .. } => 74,
        Request::ShowMatches { .. } => 75,
        Request::CurrentMatch(_) => 76,
        Request::Commands => 77,
        Request::Themes => 78,
        Request::PreviewTheme(_) => 79,
        Request::SetTheme { .. } => 80,
        Request::AddMenuItem(_) => 81,
    }
}

/// How many kinds of requests the plugin API has.
const KINDS: usize = 82;

#[test]
fn the_scenarios_hold_every_request_of_the_plugin_api() {
    let world = World {
        window: host::WindowId(1),
        tab: host::TabId(1),
        pane: host::PaneId(1),
        row: 0,
        line: host::LineText {
            text: String::new(),
            cells: Vec::new(),
        },
    };
    let mut held = [false; KINDS];
    for scenario in scenarios::all(&world) {
        for (request, _) in scenario.steps.iter().flat_map(|step| &step.asks) {
            held[kind(request)] = true;
        }
    }
    let missing: Vec<usize> = (0..KINDS).filter(|&at| !held[at]).collect();
    assert!(missing.is_empty(), "the kinds of no scenario: {missing:?}");
}
