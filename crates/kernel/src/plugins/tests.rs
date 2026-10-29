//! The plugins' host: commands and bindings through the SDK, the requests
//! that wait for a call's return, the panes and the clipboard, and the
//! plugins that fail or panic.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use lokyr_plugin_sdk::command::{Category, Kind, Param, Value};
use lokyr_plugin_sdk::host::{self, Binding as PluginBinding, HostApi};
use lokyr_pty::Program;
use lokyr_vt_core::terminal::Colors;

use super::*;
use crate::config::KeysConfig;
use crate::keymap::{Context as Keys, Keymap, Pressed, Resolved, Resolver};
use crate::l10n::Texts;
use crate::workspace::{Environment, Grid, Settings};

type Activate = Box<dyn FnMut(&mut Cx<'_>) -> Result<(), Error>>;
type OnCommand = Box<dyn FnMut(&mut Cx<'_>, &str, Args) -> Result<(), Error>>;

/// A plugin whose callbacks the test gives.
struct Scripted {
    activate: Activate,
    on_command: OnCommand,
}

impl Plugin for Scripted {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        (self.activate)(cx)
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, args: Args) -> Result<(), Error> {
        (self.on_command)(cx, name, args)
    }
}

fn scripted(
    activate: impl FnMut(&mut Cx<'_>) -> Result<(), Error> + 'static,
    on_command: impl FnMut(&mut Cx<'_>, &str, Args) -> Result<(), Error> + 'static,
) -> Box<dyn Plugin> {
    Box::new(Scripted {
        activate: Box::new(activate),
        on_command: Box::new(on_command),
    })
}

fn command(name: &str) -> Command {
    Command {
        name: name.to_owned(),
        title: name.to_owned(),
        category: Category::View,
        capabilities: Vec::new(),
        params: Vec::new(),
        when: None,
    }
}

fn binding(keys: &str, command: &str, when: Option<&str>) -> PluginBinding {
    PluginBinding {
        keys: keys.to_owned(),
        command: command.to_owned(),
        args: Args::new(),
        when: when.map(str::to_owned),
    }
}

/// A clipboard of the test's own.
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

/// What the host of the tests reaches.
#[derive(Default)]
struct App {
    registry: Registry,
    workspace: Workspace,
    clipboard: Board,
    focused: Option<WindowId>,
}

impl App {
    fn context(&mut self) -> Context<'_> {
        Context {
            registry: &mut self.registry,
            workspace: &mut self.workspace,
            clipboard: &mut self.clipboard,
            focused: self.focused,
            font_size: 13.0,
            confirm_paste: crate::config::ConfirmPaste::Auto,
        }
    }
}

/// A window whose pane runs `script` with `sh`, with the focus.
fn window(app: &mut App, script: &str) -> WindowId {
    let grid = Grid {
        columns: 80,
        rows: 24,
        cell_width: 8,
        cell_height: 16,
    };
    let pane = Pane::spawn(
        &Program::command("/bin/sh", ["-c", script]),
        &Environment::default(),
        &Settings::default(),
        grid,
        &Colors::default(),
        &Texts::new("en", &crate::l10n::KERNEL),
        || {},
    );
    let window = app.workspace.open_window(pane);
    app.focused = Some(window);
    window
}

/// The text of the whole of `pane`, once it holds `wanted`, for ten
/// seconds at most.
fn text_once(app: &App, pane: PaneId, wanted: &str) -> String {
    let start = Instant::now();
    loop {
        let pane = app.workspace.pane(pane).unwrap();
        pane.select_all();
        let text = pane.selection_text().unwrap_or_default();
        if text.contains(wanted) {
            return text;
        }
        assert!(start.elapsed() < Duration::from_secs(10), "{text:?}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_plugin_registers_a_command_and_a_binding_through_the_sdk() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    let ran = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&ran);
    plugins.add(
        "greeter",
        scripted(
            |cx| {
                cx.register_command(Command {
                    params: vec![Param {
                        name: "times".to_owned(),
                        kind: Kind::Integer,
                        default: Some(Value::Integer(1)),
                    }],
                    ..command("greeter.hello")
                })?;
                cx.bind(binding("cmd+h", "greeter.hello", Some("pane.focused")))
            },
            move |_, name, args| {
                seen.borrow_mut().push((name.to_owned(), args));
                Ok(())
            },
        ),
    );
    assert_eq!(plugins.status("greeter"), Some(Status::Inactive));
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(plugins.status("greeter"), Some(Status::Active));
    assert!(app.registry.get("greeter.hello").is_some());
    assert!(plugins.owns("greeter.hello"));
    assert_eq!(
        plugins
            .commands()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        ["greeter.hello"]
    );
    assert!(plugins.take_bindings_changed());
    assert!(!plugins.take_bindings_changed());
    // A second activation leaves an active plugin as it is.
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(plugins.bindings().count(), 1);
    // The binding runs the command, in its context, from the keymap.
    let keymap = Keymap::with_config(
        plugins.bindings().cloned().collect(),
        &KeysConfig::default(),
    );
    let pressed = Pressed::key(
        lokyr_vt_core::input::Key::Char('h'),
        crate::keymap::Mods {
            cmd: true,
            ..Default::default()
        },
        None,
    )
    .unwrap();
    let mut resolver = Resolver::new();
    let Resolved::Run(invocation) = resolver.press(&keymap, pressed, &Keys::new(["pane.focused"]))
    else {
        panic!("the binding holds in its context");
    };
    assert_eq!(invocation.command, "greeter.hello");
    assert!(matches!(
        resolver.press(&keymap, pressed, &Keys::new([])),
        Resolved::Program { .. }
    ));
    // The registry's check gives the defaults, and the plugin takes them.
    let args = app.registry.check("greeter.hello", Args::new()).unwrap();
    plugins
        .run("greeter.hello", args, &mut app.context())
        .unwrap();
    assert_eq!(
        *ran.borrow(),
        [(
            "greeter.hello".to_owned(),
            Args::from([("times".to_owned(), Value::Integer(1))])
        )]
    );
    let error = plugins
        .run("greeter.other", Args::new(), &mut app.context())
        .unwrap_err();
    assert_eq!(error.code, Code::NotFound);
}

#[test]
fn a_plugin_that_panics_is_disabled_with_its_commands_and_bindings() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    plugins.add(
        "boom",
        scripted(
            |cx| {
                cx.register_command(command("boom.go"))?;
                cx.bind(binding("cmd+b", "boom.go", None))
            },
            |cx, _, _| {
                cx.open_window()?;
                panic!("on purpose");
            },
        ),
    );
    plugins.add(
        "calm",
        scripted(
            |cx| cx.register_command(command("calm.go")),
            |_, _, _| Ok(()),
        ),
    );
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins.take_bindings_changed();
    let error = plugins
        .run("boom.go", Args::new(), &mut app.context())
        .unwrap_err();
    assert_eq!(error.code, Code::Fault);
    assert_eq!(
        error.message,
        "the plugin `boom` panicked: on purpose; it is disabled"
    );
    assert_eq!(plugins.status("boom"), Some(Status::Disabled));
    // Its commands and bindings went, and what its call asked for.
    assert!(app.registry.get("boom.go").is_none());
    assert!(!plugins.owns("boom.go"));
    assert_eq!(plugins.bindings().count(), 0);
    assert!(plugins.take_bindings_changed());
    assert!(plugins.take_requests().is_empty());
    assert_eq!(
        plugins
            .run("boom.go", Args::new(), &mut app.context())
            .unwrap_err()
            .code,
        Code::NotFound
    );
    // The other plugin goes on.
    assert_eq!(plugins.status("calm"), Some(Status::Active));
    plugins
        .run("calm.go", Args::new(), &mut app.context())
        .unwrap();
}

#[test]
fn a_plugin_whose_activation_fails_or_panics_is_disabled() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    plugins.add(
        "failing",
        scripted(
            |cx| {
                cx.register_command(command("failing.go"))?;
                Err(Error::new(Code::Failed, "no luck"))
            },
            |_, _, _| Ok(()),
        ),
    );
    plugins.add(
        "panicking",
        scripted(
            |cx| {
                cx.bind(binding("cmd+p", "panicking.go", None))?;
                panic!("{}", String::from("at start"))
            },
            |_, _, _| Ok(()),
        ),
    );
    plugins.add(
        "last",
        scripted(
            |cx| cx.register_command(command("last.go")),
            |_, _, _| Ok(()),
        ),
    );
    let errors = plugins.activate(&mut app.context());
    let messages: Vec<(Code, &str)> = errors
        .iter()
        .map(|error| (error.code, error.message.as_str()))
        .collect();
    assert_eq!(
        messages,
        [
            (
                Code::Failed,
                "the plugin `failing` did not start, and is disabled: no luck"
            ),
            (
                Code::Fault,
                "the plugin `panicking` panicked: at start; it is disabled"
            ),
        ]
    );
    assert_eq!(plugins.status("failing"), Some(Status::Disabled));
    assert_eq!(plugins.status("panicking"), Some(Status::Disabled));
    assert!(app.registry.get("failing.go").is_none());
    assert_eq!(plugins.bindings().count(), 0);
    // The plugin after them starts.
    assert_eq!(plugins.status("last"), Some(Status::Active));
    assert!(app.registry.get("last.go").is_some());
    assert_eq!(plugins.status("other"), None);
}

#[test]
fn a_binding_that_is_not_one_fails_with_108() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    let errors = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&errors);
    plugins.add(
        "keys",
        scripted(
            move |cx| {
                for wrong in [
                    binding("cmd+", "keys.go", None),
                    binding("cmd+k", "go", None),
                    binding("cmd+k", "keys.go", Some("pane.focused &&")),
                ] {
                    seen.borrow_mut().push(cx.bind(wrong).unwrap_err().code);
                }
                Ok(())
            },
            |_, _, _| Ok(()),
        ),
    );
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(*errors.borrow(), [Code::InvalidBinding; 3]);
    assert_eq!(plugins.bindings().count(), 0);
    assert!(!plugins.take_bindings_changed());
}

#[test]
fn what_changes_the_app_waits_for_the_call_to_return() {
    let mut app = App::default();
    let window = window(&mut app, "sleep 10");
    let mut plugins = Plugins::new();
    let errors = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&errors);
    plugins.add(
        "app",
        scripted(
            |cx| {
                cx.register_command(command("app.go"))?;
                cx.register_command(Command {
                    params: vec![Param {
                        name: "n".to_owned(),
                        kind: Kind::Number,
                        default: Some(Value::Number(2.0)),
                    }],
                    ..command("app.other")
                })
            },
            move |cx, _, _| {
                let mut seen = seen.borrow_mut();
                cx.open_window()?;
                cx.quit()?;
                cx.set_font_size(Some(20.0))?;
                seen.push(cx.set_font_size(Some(1.0)).unwrap_err().code);
                cx.set_font_size(None)?;
                seen.push(cx.close_window(host::WindowId(u64::MAX)).unwrap_err().code);
                let window = cx.focused_window().unwrap();
                cx.close_window(window)?;
                // Another command, checked now, which gets its defaults.
                cx.run_command("app.other", Args::new())?;
                seen.push(cx.run_command("app.none", Args::new()).unwrap_err().code);
                seen.push(
                    cx.run_command(
                        "app.other",
                        Args::from([("n".to_owned(), Value::Boolean(true))]),
                    )
                    .unwrap_err()
                    .code,
                );
                assert_eq!(cx.font_size(), 13.0);
                Ok(())
            },
        ),
    );
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert!(plugins.take_requests().is_empty());
    plugins
        .run("app.go", Args::new(), &mut app.context())
        .unwrap();
    assert_eq!(
        *errors.borrow(),
        [Code::Failed, Code::Failed, Code::NotFound, Code::WrongKind]
    );
    assert_eq!(
        plugins.take_requests(),
        [
            Request::OpenWindow,
            Request::Quit,
            Request::FontSize(Some(20.0)),
            Request::FontSize(None),
            Request::CloseWindow(window),
            Request::Run(Invocation {
                command: "app.other".to_owned(),
                args: Args::from([("n".to_owned(), Value::Number(2.0))]),
            }),
        ]
    );
    assert!(plugins.take_requests().is_empty());
}

#[test]
fn the_host_reaches_the_focused_pane_and_the_clipboard() {
    let mut app = App::default();
    let window = window(
        &mut app,
        "printf 'hello\\n'; read line; printf 'got:%s\\n' \"$line\"; sleep 10",
    );
    let pane = app.workspace.active_pane(window).unwrap();
    text_once(&app, pane, "hello");
    let mut plugins = Plugins::new();
    let log = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&log);
    plugins.add(
        "copier",
        scripted(
            |cx| {
                assert_eq!(cx.focused_window(), None);
                cx.register_command(command("copier.copy"))?;
                cx.register_command(command("copier.paste"))
            },
            move |cx, name, _| {
                let pane = cx.focused_pane().unwrap();
                match name {
                    "copier.copy" => {
                        cx.select_all(pane)?;
                        let text = cx.selection_text(pane)?.unwrap();
                        cx.set_clipboard_text(text.lines().next().unwrap())?;
                        cx.scroll(pane, host::Scroll::Up(3))?;
                        cx.scroll(pane, host::Scroll::Bottom)?;
                        cx.clear_scrollback(pane)?;
                    }
                    _ => {
                        let text = cx.clipboard_text()?.unwrap();
                        // A line, its escape left out, then a line break,
                        // which the user is to confirm.
                        cx.paste(pane, &format!("{text}\x1b[A"))?;
                        cx.paste(pane, "two\nthree")?;
                        // A carriage return alone is a line break too.
                        cx.paste(pane, "x\ry")?;
                        let missing = host::PaneId(u64::MAX);
                        seen.borrow_mut()
                            .push(cx.selection_text(missing).unwrap_err().code);
                        seen.borrow_mut()
                            .push(cx.paste(missing, "x").unwrap_err().code);
                    }
                }
                Ok(())
            },
        ),
    );
    // Activated before the window takes the focus.
    let focused = app.focused.take();
    assert!(plugins.activate(&mut app.context()).is_empty());
    app.focused = focused;
    plugins
        .run("copier.copy", Args::new(), &mut app.context())
        .unwrap();
    assert_eq!(app.clipboard.0.as_deref(), Some("hello"));
    plugins
        .run("copier.paste", Args::new(), &mut app.context())
        .unwrap();
    assert_eq!(*log.borrow(), [Code::Failed, Code::Failed]);
    // The line break waits for the user, as the app asks: confirmed, its
    // line ends.
    let requests = plugins.take_requests();
    let [
        Request::ConfirmPaste { pane: asked, text },
        Request::ConfirmPaste { text: other, .. },
    ] = &requests[..]
    else {
        panic!("two pastes to confirm: {requests:?}");
    };
    assert_eq!(
        (*asked, text.as_str(), other.as_str()),
        (pane, "two\nthree", "x\ry")
    );
    assert!(!text_once(&app, pane, "hello").contains("got:"));
    app.workspace.pane(pane).unwrap().paste(text);
    assert!(text_once(&app, pane, "got:").contains("got:hello[Atwo\n"));
}

#[test]
fn a_program_that_asks_for_bracketed_paste_takes_line_breaks_at_once() {
    let mut app = App::default();
    let window = window(
        &mut app,
        "stty raw -echo; printf '\\033[?2004hready\\r\\n'; cat -v",
    );
    let pane = app.workspace.active_pane(window).unwrap();
    text_once(&app, pane, "ready");
    let mut plugins = Plugins::new();
    plugins.add(
        "paster",
        scripted(
            |cx| cx.register_command(command("paster.paste")),
            |cx, _, _| {
                let pane = cx.focused_pane().unwrap();
                cx.paste(pane, "a\nb")
            },
        ),
    );
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("paster.paste", Args::new(), &mut app.context())
        .unwrap();
    assert!(plugins.take_requests().is_empty());
    assert!(text_once(&app, pane, "201~").contains("^[[200~a"));
}

#[test]
fn clearing_a_pane_keeps_its_prompt_at_the_top() {
    let mut app = App::default();
    let window = window(
        &mut app,
        "for n in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30; do echo line$n; done; printf '$ '; sleep 10",
    );
    let pane = app.workspace.active_pane(window).unwrap();
    // The text's lines end without their spaces.
    text_once(&app, pane, "line30\n$");
    let mut plugins = Plugins::new();
    plugins.add(
        "clearer",
        scripted(
            |cx| cx.register_command(command("clearer.clear")),
            |cx, _, _| {
                let pane = cx.focused_pane().unwrap();
                cx.clear(pane)
            },
        ),
    );
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("clearer.clear", Args::new(), &mut app.context())
        .unwrap();
    // The prompt's row at the top, the other rows empty.
    assert_eq!(text_once(&app, pane, "$").trim_end(), "$");
}

#[test]
fn the_host_reads_a_line_its_links_and_the_directory_and_opens() {
    let mut app = App::default();
    let window = window(
        &mut app,
        "cd /usr; printf '\\033]8;;https://lokyr.dev/a\\033\\\\site\\033]8;;\\033\\\\ and /etc/hosts\\n'; sleep 10",
    );
    let pane = app.workspace.active_pane(window).unwrap();
    text_once(&app, pane, "hosts");
    let mut plugins = Plugins::new();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log = Rc::clone(&seen);
    plugins.add(
        "reader",
        scripted(
            |cx| cx.register_command(command("reader.read")),
            move |cx, _, _| {
                let pane = cx.focused_pane().unwrap();
                let line = cx.line_text(pane, 0)?.unwrap();
                let mut log = log.borrow_mut();
                log.push(line.text.trim_end().to_owned());
                log.push(format!("{:?}", &line.cells[..2]));
                log.push(format!("{:?}", cx.link_at(pane, 3, 0)?));
                log.push(format!("{:?}", cx.link_at(pane, 5, 0)?));
                log.push(format!("{:?}", cx.working_directory(pane)?));
                log.push(format!("{:?}", cx.line_text(pane, 24)?));
                for target in ["https://lokyr.dev", "/etc/hosts", "~/x"] {
                    cx.open(target)?;
                }
                let refused = cx.open("javascript:alert(1)").unwrap_err();
                log.push(format!("{} {}", refused.code.number(), refused.message));
                let missing = host::PaneId(u64::MAX);
                log.push(format!(
                    "{:?}",
                    cx.working_directory(missing).unwrap_err().code
                ));
                Ok(())
            },
        ),
    );
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("reader.read", Args::new(), &mut app.context())
        .unwrap();
    assert_eq!(
        *seen.borrow(),
        [
            "site and /etc/hosts",
            "[(0, 0), (1, 0)]",
            "Some(\"https://lokyr.dev/a\")",
            "None",
            "Some(\"/usr\")",
            "None",
            "201 `javascript:` does not open: only http, https, mailto, file",
            "Failed",
        ]
    );
    // The home directory in full.
    let home = std::env::home_dir().unwrap();
    assert_eq!(
        plugins.take_requests(),
        [
            "https://lokyr.dev".to_owned(),
            "/etc/hosts".to_owned(),
            format!("{}/x", home.display()),
        ]
        .map(Request::Open)
    );
}

#[test]
fn only_urls_of_the_schemes_and_paths_from_the_root_or_home_open() {
    for target in [
        "http://a",
        "HTTPS://a/b?c#d",
        "mailto:me@lokyr.dev",
        "file:///etc/hosts",
        "/",
        "/usr/bin",
    ] {
        assert_eq!(opening(target).as_deref(), Ok(target), "{target}");
    }
    let home = std::env::home_dir().unwrap().display().to_string();
    assert_eq!(opening("~"), Ok(home.clone()));
    assert_eq!(opening("~/notes.txt"), Ok(format!("{home}/notes.txt")));
    for target in [
        "javascript:alert(1)",
        "x-apple-systempreferences:",
        "ssh://host",
        "tel:123",
        "src/main.rs",
        "~user/x",
        "",
        "1:2",
        ":a",
    ] {
        assert!(opening(target).is_err(), "{target}");
    }
    // A text before a `:` that is no scheme is no URL.
    for target in ["1:2", "src/main.rs:42"] {
        assert!(opening(target).unwrap_err().contains("neither"), "{target}");
    }
}

#[test]
fn the_host_s_api_is_a_trait_object() {
    fn takes(_: &mut dyn HostApi) {}
    let mut app = App::default();
    let mut context = app.context();
    let (mut state, mut requests, mut changed) = (
        State {
            name: "x".to_owned(),
            status: Status::Active,
            commands: Vec::new(),
            bindings: Vec::new(),
        },
        Vec::new(),
        false,
    );
    takes(&mut Host {
        state: &mut state,
        context: &mut context,
        requests: &mut requests,
        bindings_changed: &mut changed,
    });
}
