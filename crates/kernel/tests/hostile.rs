//! Hostile plugins of Luau, on the plugins' host, each a plugin of a third
//! party's manifest: a loop, an allocation, a string's growth, a deep
//! table and one that holds itself, a flood of calls, a flood of images, a
//! capability not granted, a `require` out of its package and a runaway
//! recursion. Each
//! is stopped as its limit says, what it asked goes, and the app goes on:
//! a well-behaved plugin beside it runs its command and hears its events.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use lokyr_kernel::commands::{Args, Code, Error, Registry};
use lokyr_kernel::plugins::{
    Clipboard, Context, Cx, Event, Image, ImageSpec, Images, Panes, Plugin, Plugins, Status, host,
};
use lokyr_kernel::workspace::{Pane, PaneId, Workspace};
use lokyr_pkg_model::manifest::Manifest;
use lokyr_plugin_host_luau::{Deadlines, ENTRY, Execution, LuauPlugin, Package};
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

    fn set_image(&mut self, _: &[u8]) {
        self.0 = None;
    }
}

/// The panes of the tests, which no hostile plugin opens.
struct NoPanes;

impl Panes for NoPanes {
    fn spawn(&mut self, _: PaneId, _: Option<&Pane>, _: Option<&str>) -> Pane {
        unreachable!("no plugin of the tests opens a pane")
    }
}

/// The images of the tests: a clear pixel each, counted.
struct Blank(usize);

impl Images for Blank {
    fn render(&mut self, _: &str, image: &ImageSpec) -> Result<Image, String> {
        self.0 += 1;
        lokyr_plugin_sdk::image::nominal(image, 2.0).map_err(|error| error.message)
    }

    fn drop_image(&mut self, _: &str, _: u64) {}
}

/// What the host of the tests reaches: no window.
struct App {
    registry: Registry,
    workspace: Workspace,
    panes: NoPanes,
    clipboard: Board,
    images: Blank,
    config: lokyr_kernel::config::Config,
}

impl App {
    fn new() -> Self {
        Self {
            registry: Registry::default(),
            workspace: Workspace::new(),
            panes: NoPanes,
            clipboard: Board(Some("before".to_owned())),
            images: Blank(0),
            config: lokyr_kernel::config::Config::default(),
        }
    }

    fn context(&mut self) -> Context<'_> {
        Context {
            registry: &mut self.registry,
            workspace: &mut self.workspace,
            panes: &mut self.panes,
            clipboard: &mut self.clipboard,
            images: &mut self.images,
            focused: None,
            font_size: 13.0,
            confirm_paste: lokyr_kernel::config::ConfirmPaste::Auto,
            keys: &|_, _| None,
            enabled: &|_| true,
            themes: &themes,
            fonts: &host::Fonts::default,
            plugins: &[],
            config: &self.config,
            plugin_settings: &[],
            day: 0,
            notice: false,
            saving: &|_| false,
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
    plugins_of(app, source, capabilities, Deadlines::default())
}

/// [`plugins`], the hostile plugin's calls of `deadlines`.
fn plugins_of(
    app: &mut App,
    source: &str,
    capabilities: &str,
    deadlines: Deadlines,
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
    let plugin = LuauPlugin::new(HOSTILE, package, Execution::Native).with_deadlines(deadlines);
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
    // Of a command's deadline that the 20 000 calls of a debug build fit
    // in on a slow machine.
    let deadlines = Deadlines {
        command: Duration::from_secs(2),
        ..Deadlines::default()
    };
    let (mut plugins, heard, errors) =
        plugins_of(&mut app, &source, "clipboard-write = true\n", deadlines);
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
    faulted(&plugins, &error, "past its deadline of 2000 ms");
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
fn a_flood_of_images_is_held_to_the_plugin_s_budget_of_pixels() {
    let mut app = App::new();
    // Four images of a million cells each call, as of a timer's firings.
    let source = commanded(
        r#"
        for _ = 1, 4 do
            cx.render_image({ rows = { { text = "x" } }, min_columns = 1000, min_rows = 1000 })
        end
        "#,
    );
    let (mut plugins, heard, errors) = plugins(&mut app, &source, "");
    assert!(errors.is_empty(), "{errors:?}");
    let start = Instant::now();
    for _ in 0..50 {
        plugins
            .run("hostile.go", Args::new(), &mut app.context())
            .unwrap();
    }
    // The first drawn, the rest refused before the app draws them, until
    // the budget comes back, 24 seconds later.
    assert!(start.elapsed() < Duration::from_secs(20));
    assert_eq!(app.images.0, 1);
    assert_eq!(plugins.status(HOSTILE), Some(Status::Active));
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

/// Serves the plugins' threads, in `app`, until `done` holds of them.
fn served_until(plugins: &mut Plugins, app: &mut App, done: impl Fn(&Plugins) -> bool) {
    let start = Instant::now();
    while !done(plugins) {
        plugins.serve(&mut app.context());
        assert!(start.elapsed() < Duration::from_secs(10), "a call hangs");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn an_installed_plugin_that_faults_starts_again_anew_and_turned_on_again_is_made_anew() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let mut app = App::new();
    let mut plugins = Plugins::new();
    let manifest = Manifest::parse(&format!(
        "[plugin]\nname = \"{HOSTILE}\"\nversion = \"1.0.0\"\nkind = \"luau\"\napi = \"^0.1\"\nlicense = \"MIT\"\n"
    ))
    .unwrap();
    let package = Package::new([(ENTRY.to_owned(), commanded("while true do end"))]);
    let made = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&made);
    plugins
        .add_installed(
            manifest,
            Box::new(move || {
                counted.fetch_add(1, Ordering::Relaxed);
                Box::new(LuauPlugin::new(HOSTILE, package.clone(), Execution::Native))
            }),
            (&[], &[]),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    // Active, its activation on its thread done, its command registered.
    let active = |plugins: &Plugins| {
        plugins.status(HOSTILE) == Some(Status::Active) && plugins.owns("hostile.go")
    };
    served_until(&mut plugins, &mut app, active);
    assert_eq!(made.load(Ordering::Relaxed), 1);
    let start = Instant::now();
    // Each fault, of its loop's deadline, on its thread, and what it tells.
    let fault = |plugins: &mut Plugins, app: &mut App, at: Duration| {
        plugins
            .run("hostile.go", Args::new(), &mut app.context())
            .unwrap();
        served_until(plugins, app, |plugins| {
            plugins.status(HOSTILE) == Some(Status::Disabled)
        });
        plugins.schedule_restarts(start + at);
        let told = plugins.take_disabled();
        assert_eq!(told.len(), 1, "{told:?}");
        assert!(told[0].1.contains("past its deadline"), "{told:?}");
        told[0].1.clone()
    };
    // Again in a second, a new VM of its package.
    let told = fault(&mut plugins, &mut app, Duration::ZERO);
    assert!(told.contains("starts again in 1 s"), "{told}");
    assert_eq!(plugins.next_restart(), Some(start + Duration::from_secs(1)));
    assert!(
        plugins
            .restart_due(start + Duration::from_secs(1), &mut app.context())
            .is_empty()
    );
    served_until(&mut plugins, &mut app, active);
    assert_eq!(made.load(Ordering::Relaxed), 2);
    assert!(plugins.owns("hostile.go"));
    // Twice as long, then off at the third within a minute.
    let told = fault(&mut plugins, &mut app, Duration::from_secs(2));
    assert!(told.contains("starts again in 2 s"), "{told}");
    assert!(
        plugins
            .restart_due(start + Duration::from_secs(4), &mut app.context())
            .is_empty()
    );
    served_until(&mut plugins, &mut app, active);
    assert_eq!(made.load(Ordering::Relaxed), 3);
    let told = fault(&mut plugins, &mut app, Duration::from_secs(5));
    assert!(told.contains("turned off, after 3 faults"), "{told}");
    assert_eq!(plugins.next_restart(), None);
    // Its section turns it off, then on again: a new one of it.
    let config = |text: &str| {
        lokyr_kernel::config::Config::parse(text, std::path::Path::new("config.toml")).unwrap()
    };
    assert!(
        plugins
            .configure(&config("[plugins.hostile]\nenabled = false\n").plugins)
            .is_empty()
    );
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(plugins.status(HOSTILE), Some(Status::Off));
    assert!(plugins.configure(&config("").plugins).is_empty());
    assert!(plugins.activate(&mut app.context()).is_empty());
    served_until(&mut plugins, &mut app, active);
    assert_eq!(made.load(Ordering::Relaxed), 4);
    // Its next fault counts again, of the faults within the minute.
    let told = fault(&mut plugins, &mut app, Duration::from_secs(70));
    assert!(told.contains("starts again in 1 s"), "{told}");
}
