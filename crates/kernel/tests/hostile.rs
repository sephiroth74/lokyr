//! Hostile plugins of Luau, on the plugins' host, each a plugin of a third
//! party's manifest: a loop, an allocation, a string's growth, a deep
//! table and one that holds itself, a flood of calls, a capability not
//! granted, a `require` out of its package and a runaway recursion. Each
//! is stopped as its limit says, what it asked goes, and the app goes on:
//! a well-behaved plugin beside it runs its command and hears its events.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use lokyr_kernel::commands::{Args, Code, Error, Registry};
use lokyr_kernel::plugins::{Clipboard, Context, Cx, Event, Panes, Plugin, Plugins, Status, host};
use lokyr_kernel::workspace::{Pane, PaneId, Workspace};
use lokyr_pkg_model::manifest::Manifest;
use lokyr_plugin_host_luau::{ENTRY, Execution, LuauPlugin, Package};
use lokyr_plugin_sdk::command::{Category, Command};

/// The clipboard of the tests, of its text.
struct Board(Option<String>);

impl Clipboard for Board {
    fn text(&self) -> Option<String> {
        self.0.clone()
    }

    fn set_text(&mut self, text: &str) {
        self.0 = Some(text.to_owned());
    }
}

/// The panes of the tests, which no hostile plugin opens.
struct NoPanes;

impl Panes for NoPanes {
    fn spawn(&mut self, _: PaneId, _: Option<&Pane>, _: Option<&str>) -> Pane {
        unreachable!("no plugin of the tests opens a pane")
    }
}

/// What the host of the tests reaches: no window.
struct App {
    registry: Registry,
    workspace: Workspace,
    panes: NoPanes,
    clipboard: Board,
}

impl App {
    fn new() -> Self {
        Self {
            registry: Registry::default(),
            workspace: Workspace::new(),
            panes: NoPanes,
            clipboard: Board(Some("before".to_owned())),
        }
    }

    fn context(&mut self) -> Context<'_> {
        Context {
            registry: &mut self.registry,
            workspace: &mut self.workspace,
            panes: &mut self.panes,
            clipboard: &mut self.clipboard,
            focused: None,
            font_size: 13.0,
            confirm_paste: lokyr_kernel::config::ConfirmPaste::Auto,
            keys: &|_, _| None,
            enabled: &|_| true,
            themes: &themes,
            language: "en",
            now: Instant::now(),
        }
    }
}

fn themes() -> host::Themes {
    host::Themes {
        appearance: lokyr_plugin_sdk::theme::ThemeVariant::Dark,
        current: "Lokyr Dark".to_owned(),
        themes: Vec::new(),
    }
}

/// A plugin of the app's, beside the hostile one: its command `steady.go`,
/// and the events of `steady.ping` that it emits, which it hears, counted
/// in `heard`.
struct Steady {
    heard: Rc<RefCell<usize>>,
}

impl Plugin for Steady {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        cx.register_command(Command {
            name: "steady.go".to_owned(),
            title: "Go".to_owned(),
            category: Category::View,
            capabilities: Vec::new(),
            params: Vec::new(),
            when: None,
        })?;
        cx.subscribe(&["steady.ping"])
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, _: &str, _: Args) -> Result<(), Error> {
        cx.emit(
            "steady.ping",
            "steady.ping/1",
            lokyr_plugin_sdk::message::Data::Nil,
        )
    }

    fn on_event(&mut self, _: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        *self.heard.borrow_mut() += events.len();
        Ok(())
    }
}

/// The name of the hostile plugins.
const HOSTILE: &str = "acme/hostile";

/// The plugins of `app`: the hostile one, of the entry `source` and of the
/// capabilities of `capabilities`, its manifest's, beside `steady`;
/// `steady` activated, and the hostile one's activation's errors.
fn plugins(
    app: &mut App,
    source: &str,
    capabilities: &str,
) -> (Plugins, Rc<RefCell<usize>>, Vec<Error>) {
    let heard = Rc::new(RefCell::new(0));
    let mut plugins = Plugins::new();
    plugins
        .add(
            "steady",
            Box::new(Steady {
                heard: Rc::clone(&heard),
            }),
        )
        .unwrap();
    let manifest = Manifest::parse(&format!(
        "[plugin]\nname = \"{HOSTILE}\"\nversion = \"1.0.0\"\nkind = \"luau\"\napi = \"^0.1\"\nlicense = \"MIT\"\n[capabilities]\n{capabilities}"
    ))
    .unwrap();
    let package = Package::new([(ENTRY.to_owned(), source.to_owned())]);
    let plugin = LuauPlugin::new(HOSTILE, package, Execution::Native);
    plugins.add_declared(manifest, Box::new(plugin)).unwrap();
    let errors = plugins.activate(&mut app.context());
    assert_eq!(plugins.status("steady"), Some(Status::Active));
    (plugins, heard, errors)
}

/// The entry of a hostile plugin that registers `hostile.go`, whose call
/// runs `body`, of `cx`.
fn commanded(body: &str) -> String {
    format!(
        r#"
        return {{
            activate = function(cx)
                assert(cx.register_command({{ name = "hostile.go", title = "Go", category = "view" }}))
            end,
            on_command = function(cx, name, args)
                {body}
            end,
        }}
        "#
    )
}

/// Runs the hostile plugin's command, in `app`: its error, and how long it
/// took.
fn hostile_run(plugins: &mut Plugins, app: &mut App) -> (Error, Duration) {
    let start = Instant::now();
    let ran = plugins.run("hostile.go", Args::new(), &mut app.context());
    (
        ran.expect_err("the hostile plugin's call fails"),
        start.elapsed(),
    )
}

/// Checks that the app goes on: the plugin beside the hostile one runs its
/// command and hears its event.
fn goes_on(plugins: &mut Plugins, app: &mut App, heard: &Rc<RefCell<usize>>) {
    let before = *heard.borrow();
    plugins
        .run("steady.go", Args::new(), &mut app.context())
        .unwrap();
    assert!(plugins.deliver(&mut app.context()).is_empty());
    assert_eq!(*heard.borrow(), before + 1);
}

/// Checks that a call that ended as `error` faulted the hostile plugin,
/// which is disabled, of a message of `told`.
fn faulted(plugins: &Plugins, error: &Error, told: &str) {
    assert_eq!(error.code, Code::Fault, "{error:?}");
    assert!(error.message.contains(told), "{}", error.message);
    assert_eq!(plugins.status(HOSTILE), Some(Status::Disabled));
}

/// Checks that a call that ended as `error` failed, of a message of
/// `told`, and left the hostile plugin running.
fn failed(plugins: &Plugins, error: &Error, told: &str) {
    assert_eq!(error.code, Code::Failed, "{error:?}");
    assert!(error.message.contains(told), "{}", error.message);
    assert_eq!(plugins.status(HOSTILE), Some(Status::Active));
}

#[test]
fn a_loop_runs_to_its_deadline_and_faults_its_plugin() {
    let mut app = App::new();
    let (mut plugins, heard, errors) = plugins(&mut app, &commanded("while true do end"), "");
    assert!(errors.is_empty(), "{errors:?}");
    let (error, took) = hostile_run(&mut plugins, &mut app);
    faulted(&plugins, &error, "past its deadline of 250 ms");
    assert!(took >= Duration::from_millis(250), "{took:?}");
    assert!(took < Duration::from_secs(5), "{took:?}");
    // A disabled plugin's command is no more.
    assert!(!plugins.owns("hostile.go"));
    goes_on(&mut plugins, &mut app, &heard);
}

#[test]
fn an_allocation_past_the_memory_faults_its_plugin() {
    let mut app = App::new();
    let source = commanded(
        "local kept = {} for at = 1, math.huge do kept[at] = buffer.create(1024 * 1024) end",
    );
    let (mut plugins, heard, errors) = plugins(&mut app, &source, "");
    assert!(errors.is_empty(), "{errors:?}");
    let (error, _) = hostile_run(&mut plugins, &mut app);
    faulted(&plugins, &error, "ran out of its 64 MB of memory");
    goes_on(&mut plugins, &mut app, &heard);
}

#[test]
fn a_string_that_grows_past_the_memory_faults_its_plugin() {
    let mut app = App::new();
    let source = commanded(r#"local text = "x" while true do text = text .. text end"#);
    let (mut plugins, heard, errors) = plugins(&mut app, &source, "");
    assert!(errors.is_empty(), "{errors:?}");
    let (error, _) = hostile_run(&mut plugins, &mut app);
    faulted(&plugins, &error, "ran out of its 64 MB of memory");
    goes_on(&mut plugins, &mut app, &heard);
}

#[test]
fn a_deep_table_and_one_that_holds_itself_are_refused_as_they_convert() {
    let mut app = App::new();
    let source = commanded(
        r#"
        if args.which == "deep" then
            local top = {}
            local deep = top
            for _ = 1, 100000 do
                deep.inner = {}
                deep = deep.inner
            end
            cx.set_state(top)
        else
            local itself = {}
            itself.inner = itself
            cx.set_state(itself)
        end
        "#,
    );
    let (mut plugins, heard, errors) = plugins(&mut app, &source, "");
    assert!(errors.is_empty(), "{errors:?}");
    for which in ["deep", "itself"] {
        let args = Args::from([(
            "which".to_owned(),
            lokyr_plugin_sdk::command::Value::String(which.to_owned()),
        )]);
        let start = Instant::now();
        let error = plugins
            .run("hostile.go", args, &mut app.context())
            .expect_err("a value too deep is refused");
        failed(&plugins, &error, "nests 64 levels of tables at most");
        assert!(start.elapsed() < Duration::from_secs(5), "{which}");
    }
    goes_on(&mut plugins, &mut app, &heard);
}

#[test]
fn a_flood_of_calls_is_bounded_and_runs_to_its_deadline() {
    let mut app = App::new();
    // Past a call's 10 000 writes each fails, and the loop goes on: to its
    // end, which tells how many failed, or to the deadline. Either call's
    // writes go with it.
    let source = commanded(
        r#"
        local refused = 0
        local calls = if args.endless then math.huge else 20000
        local at = 0
        while at < calls do
            at += 1
            if not cx.set_clipboard_text("flood") then
                refused += 1
            end
        end
        error("refused " .. refused)
        "#,
    );
    let (mut plugins, heard, errors) = plugins(&mut app, &source, "clipboard-write = true\n");
    assert!(errors.is_empty(), "{errors:?}");
    let error = plugins
        .run("hostile.go", Args::new(), &mut app.context())
        .unwrap_err();
    failed(&plugins, &error, "refused 10000");
    assert_eq!(app.clipboard.0.as_deref(), Some("before"));
    let endless = Args::from([(
        "endless".to_owned(),
        lokyr_plugin_sdk::command::Value::Boolean(true),
    )]);
    let start = Instant::now();
    let error = plugins
        .run("hostile.go", endless, &mut app.context())
        .unwrap_err();
    faulted(&plugins, &error, "past its deadline of 250 ms");
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "{:?}",
        start.elapsed()
    );
    assert_eq!(app.clipboard.0.as_deref(), Some("before"));
    goes_on(&mut plugins, &mut app, &heard);
}

#[test]
fn a_loop_on_its_own_thread_never_holds_the_main_thread() {
    let mut app = App::new();
    let heard = Rc::new(RefCell::new(0));
    let mut plugins = Plugins::new();
    plugins
        .add(
            "steady",
            Box::new(Steady {
                heard: Rc::clone(&heard),
            }),
        )
        .unwrap();
    let manifest = format!(
        "[plugin]\nname = \"{HOSTILE}\"\nkind = \"builtin\"\nlicense = \"MIT\"\nthread = \"worker\"\n"
    );
    let package = Package::new([(ENTRY.to_owned(), commanded("while true do end"))]);
    let plugin = LuauPlugin::new(HOSTILE, package, Execution::Native);
    plugins
        .add_builtin_send(&manifest, Box::new(plugin))
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    let served = |plugins: &mut Plugins, app: &mut App| {
        let start = Instant::now();
        let mut errors = Vec::new();
        while plugins.waiting() {
            let turn = Instant::now();
            errors.extend(plugins.serve(&mut app.context()).errors);
            // A turn takes the thread's messages for a millisecond at most.
            assert!(
                turn.elapsed() < Duration::from_millis(50),
                "{:?}",
                turn.elapsed()
            );
            assert!(start.elapsed() < Duration::from_secs(10), "a call hangs");
            std::thread::sleep(Duration::from_millis(1));
        }
        errors
    };
    assert!(served(&mut plugins, &mut app).is_empty());
    assert_eq!(plugins.status(HOSTILE), Some(Status::Active));
    // Its command loops on its thread: the main thread asks and goes on,
    // the plugin beside it running, until the loop's deadline faults it.
    let start = Instant::now();
    plugins
        .run("hostile.go", Args::new(), &mut app.context())
        .unwrap();
    assert!(
        start.elapsed() < Duration::from_millis(50),
        "{:?}",
        start.elapsed()
    );
    goes_on(&mut plugins, &mut app, &heard);
    let errors = served(&mut plugins, &mut app);
    assert_eq!(errors.len(), 1, "{errors:?}");
    faulted(&plugins, &errors[0], "past its deadline of 250 ms");
    goes_on(&mut plugins, &mut app, &heard);
}

#[test]
fn a_capability_not_granted_is_refused_and_the_plugin_goes_on() {
    let mut app = App::new();
    let source = commanded(
        r#"
        local text, message, code = cx.clipboard_text()
        assert(text == nil and code == 302, message)
        assert(cx.set_clipboard_text("taken"))
        "#,
    );
    let (mut plugins, heard, errors) = plugins(&mut app, &source, "");
    assert!(errors.is_empty(), "{errors:?}");
    let (error, _) = hostile_run(&mut plugins, &mut app);
    failed(&plugins, &error, "lacks the capability `clipboard-write`");
    assert_eq!(app.clipboard.0.as_deref(), Some("before"));
    goes_on(&mut plugins, &mut app, &heard);
}

#[test]
fn a_require_out_of_its_package_stops_its_activation() {
    let mut app = App::new();
    let source = r#"local secrets = require("../../../etc/passwd") return {}"#;
    let (mut plugins, heard, errors) = plugins(&mut app, source, "");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0].message.contains("leaves the package"),
        "{}",
        errors[0].message
    );
    assert_eq!(plugins.status(HOSTILE), Some(Status::Disabled));
    goes_on(&mut plugins, &mut app, &heard);
}

#[test]
fn a_runaway_recursion_fails_its_call_and_the_plugin_goes_on() {
    let mut app = App::new();
    let source = commanded("local function deeper(n) return deeper(n + 1) + 1 end deeper(1)");
    let (mut plugins, heard, errors) = plugins(&mut app, &source, "");
    assert!(errors.is_empty(), "{errors:?}");
    let (error, _) = hostile_run(&mut plugins, &mut app);
    failed(&plugins, &error, "stack overflow");
    goes_on(&mut plugins, &mut app, &heard);
    // Its next call runs.
    let (again, _) = hostile_run(&mut plugins, &mut app);
    failed(&plugins, &again, "stack overflow");
}
