//! The plugins' host: commands and bindings through the SDK, the requests
//! that wait for a call's return, the panes and the clipboard, and the
//! plugins that fail or panic.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use lokyr_plugin_sdk::command::{Category, Kind, Param, Value};
use lokyr_plugin_sdk::host::{self, Binding as PluginBinding};
use lokyr_pty::Program;
use lokyr_vt_core::terminal::Colors;

use super::*;
use crate::config::KeysConfig;
use crate::keymap::{Context as Keys, Keymap, Pressed, Resolved, Resolver};
use crate::l10n::Texts;
use crate::workspace::{Environment, Grid, Settings};

type Activate = Box<dyn FnMut(&mut Cx<'_>) -> Result<(), Error>>;
type OnCommand = Box<dyn FnMut(&mut Cx<'_>, &str, Args) -> Result<(), Error>>;
type OnEvent = Box<dyn FnMut(&mut Cx<'_>, &[Event]) -> Result<(), Error>>;

/// A plugin whose callbacks the test gives.
struct Scripted {
    activate: Activate,
    on_command: OnCommand,
    on_event: Option<OnEvent>,
}

impl Plugin for Scripted {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        (self.activate)(cx)
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, args: Args) -> Result<(), Error> {
        (self.on_command)(cx, name, args)
    }

    fn on_event(&mut self, cx: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        match &mut self.on_event {
            Some(on_event) => on_event(cx, events),
            None => Ok(()),
        }
    }
}

fn scripted(
    activate: impl FnMut(&mut Cx<'_>) -> Result<(), Error> + 'static,
    on_command: impl FnMut(&mut Cx<'_>, &str, Args) -> Result<(), Error> + 'static,
) -> Box<dyn Plugin> {
    Box::new(Scripted {
        activate: Box::new(activate),
        on_command: Box::new(on_command),
        on_event: None,
    })
}

/// A plugin that subscribes to `names` as it activates, and records the
/// names of the events of each batch that it takes, in `log`, as `a b`.
fn listening(names: &'static [&'static str], log: &Rc<RefCell<Vec<String>>>) -> Box<dyn Plugin> {
    let log = Rc::clone(log);
    Box::new(Scripted {
        activate: Box::new(move |cx| cx.subscribe(names)),
        on_command: Box::new(|_, _, _| Ok(())),
        on_event: Some(Box::new(move |_, events| {
            let names: Vec<&str> = events.iter().map(Event::name).collect();
            log.borrow_mut().push(names.join(" "));
            Ok(())
        })),
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

/// A clipboard of the test's own: its text, and every text put on it, in
/// order.
#[derive(Default)]
struct Board(Option<String>, Vec<String>);

impl Clipboard for Board {
    fn text(&self) -> Option<String> {
        self.0.clone()
    }

    fn set_text(&mut self, text: &str) {
        self.0 = Some(text.to_owned());
        self.1.push(text.to_owned());
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
    plugins
        .add(
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
        )
        .unwrap();
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
    plugins
        .add(
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
        )
        .unwrap();
    plugins
        .add(
            "calm",
            scripted(
                |cx| cx.register_command(command("calm.go")),
                |_, _, _| Ok(()),
            ),
        )
        .unwrap();
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

/// A plugin whose command `look.at` applies to its `answer` `yes`, panics
/// on `boom`, and asks for a window as it looks.
struct Looking;

impl Plugin for Looking {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        cx.register_command(command("look.at"))
    }

    fn applies(&mut self, cx: &mut Cx<'_>, name: &str, args: &Args) -> bool {
        cx.open_window().unwrap();
        match args.get("answer") {
            Some(Value::String(answer)) if answer == "boom" => panic!("on purpose"),
            Some(Value::String(answer)) => name == "look.at" && answer == "yes",
            _ => false,
        }
    }
}

#[test]
fn a_plugin_tells_whether_its_command_applies_and_asks_nothing_meanwhile() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    plugins.add("look", Box::new(Looking)).unwrap();
    plugins
        .add(
            "calm",
            scripted(
                |cx| cx.register_command(command("calm.go")),
                |_, _, _| Ok(()),
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    let answer = |text: &str| Args::from([("answer".to_owned(), Value::String(text.to_owned()))]);
    assert!(
        plugins
            .applies("look.at", &answer("yes"), &mut app.context())
            .unwrap()
    );
    assert!(
        !plugins
            .applies("look.at", &answer("no"), &mut app.context())
            .unwrap()
    );
    // What it asked for as it looked went.
    assert!(plugins.take_requests().is_empty());
    // A plugin that does not say applies.
    assert!(
        plugins
            .applies("calm.go", &Args::new(), &mut app.context())
            .unwrap()
    );
    assert_eq!(
        plugins
            .applies("none.go", &Args::new(), &mut app.context())
            .unwrap_err()
            .code,
        Code::NotFound
    );
    let error = plugins
        .applies("look.at", &answer("boom"), &mut app.context())
        .unwrap_err();
    assert_eq!(error.code, Code::Fault);
    assert_eq!(plugins.status("look"), Some(Status::Disabled));
    assert!(app.registry.get("look.at").is_none());
    assert!(plugins.take_requests().is_empty());
}

/// A plugin of three commands, of which its catalogs title the first in
/// English and Italian, the second in English alone, and the third not;
/// or whose catalogs panic.
struct Titled {
    panics: bool,
}

/// The catalogs of [`Titled`].
static TITLED: [(&str, &str); 2] = [
    (
        "en",
        "command-titled-first = First\ncommand-titled-second = Second\n",
    ),
    ("it", "command-titled-first = Primo\n"),
];

impl Plugin for Titled {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        for name in ["titled.first", "titled.second", "titled.third"] {
            cx.register_command(command(name))?;
        }
        Ok(())
    }

    fn catalogs(&self) -> &'static [(&'static str, &'static str)] {
        assert!(!self.panics, "on purpose");
        &TITLED
    }
}

#[test]
fn a_plugin_s_catalogs_title_its_commands_in_the_language_shown() {
    let titles = |plugin: Titled, language: &str| -> Vec<String> {
        let mut app = App::default();
        let mut plugins = Plugins::new();
        plugins.add("titled", Box::new(plugin)).unwrap();
        assert!(plugins.activate(&mut app.context()).is_empty());
        plugins
            .commands_in(language)
            .into_iter()
            .map(|command| command.title)
            .collect()
    };
    let calm = || Titled { panics: false };
    assert_eq!(titles(calm(), "it"), ["Primo", "Second", "titled.third"]);
    assert_eq!(titles(calm(), "en"), ["First", "Second", "titled.third"]);
    // A language that the catalogs lack takes English.
    assert_eq!(titles(calm(), "fr"), ["First", "Second", "titled.third"]);
    // Catalogs that panic leave the titles registered.
    assert_eq!(
        titles(Titled { panics: true }, "it"),
        ["titled.first", "titled.second", "titled.third"]
    );
    assert_eq!(title_message("pane.select-all"), "command-pane-select-all");
}

#[test]
fn a_plugin_whose_activation_fails_or_panics_is_disabled() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    plugins
        .add(
            "failing",
            scripted(
                |cx| {
                    cx.register_command(command("failing.go"))?;
                    Err(Error::new(Code::Failed, "no luck"))
                },
                |_, _, _| Ok(()),
            ),
        )
        .unwrap();
    plugins
        .add(
            "panicking",
            scripted(
                |cx| {
                    cx.bind(binding("cmd+p", "panicking.go", None))?;
                    panic!("{}", String::from("at start"))
                },
                |_, _, _| Ok(()),
            ),
        )
        .unwrap();
    plugins
        .add(
            "last",
            scripted(
                |cx| cx.register_command(command("last.go")),
                |_, _, _| Ok(()),
            ),
        )
        .unwrap();
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
    plugins
        .add(
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
        )
        .unwrap();
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
    plugins
        .add(
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
                    assert_eq!(cx.font_size(), Ok(13.0));
                    Ok(())
                },
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert!(plugins.take_requests().is_empty());
    plugins
        .run("app.go", Args::new(), &mut app.context())
        .unwrap();
    assert_eq!(
        *errors.borrow(),
        [
            Code::Failed,
            Code::NotFound,
            Code::NotFound,
            Code::WrongKind
        ]
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
    plugins
        .add(
            "copier",
            scripted(
                |cx| {
                    assert_eq!(cx.focused_window(), None);
                    cx.register_command(command("copier.select"))?;
                    cx.register_command(command("copier.copy"))?;
                    cx.register_command(command("copier.paste"))
                },
                move |cx, name, _| {
                    let pane = cx.focused_pane().unwrap();
                    match name {
                        // Applied as the call returns, for the next to read.
                        "copier.select" => cx.select_all(pane)?,
                        "copier.copy" => {
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
        )
        .unwrap();
    // Activated before the window takes the focus.
    let focused = app.focused.take();
    assert!(plugins.activate(&mut app.context()).is_empty());
    app.focused = focused;
    for copy in ["copier.select", "copier.copy"] {
        plugins.run(copy, Args::new(), &mut app.context()).unwrap();
    }
    assert_eq!(app.clipboard.0.as_deref(), Some("hello"));
    plugins
        .run("copier.paste", Args::new(), &mut app.context())
        .unwrap();
    assert_eq!(*log.borrow(), [Code::NotFound, Code::NotFound]);
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
    plugins
        .add(
            "paster",
            scripted(
                |cx| cx.register_command(command("paster.paste")),
                |cx, _, _| {
                    let pane = cx.focused_pane().unwrap();
                    cx.paste(pane, "a\nb")
                },
            ),
        )
        .unwrap();
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
    plugins
        .add(
            "clearer",
            scripted(
                |cx| cx.register_command(command("clearer.clear")),
                |cx, _, _| {
                    let pane = cx.focused_pane().unwrap();
                    cx.clear(pane)
                },
            ),
        )
        .unwrap();
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
    plugins
        .add(
            "reader",
            scripted(
                |cx| cx.register_command(command("reader.read")),
                move |cx, _, _| {
                    let pane = cx.focused_pane().unwrap();
                    let line = cx.line_text(pane, 0, 4)?.unwrap();
                    let mut log = log.borrow_mut();
                    log.push(line.text.trim_end().to_owned());
                    log.push(format!("{:?}", &line.cells[..2]));
                    log.push(format!("{:?}", cx.link_at(pane, 3, 0)?));
                    log.push(format!("{:?}", cx.link_at(pane, 5, 0)?));
                    log.push(format!("{:?}", cx.working_directory(pane)?));
                    log.push(format!("{:?}", cx.line_text(pane, 24, usize::MAX)?));
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
        )
        .unwrap();
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
            "NotFound",
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
    // A path opens something once it is there.
    for target in ["https://lokyr.dev", "/", "/usr/bin", "~"] {
        assert!(opens(target), "{target}");
    }
    for target in ["/lokyr-nowhere", "~/lokyr-nowhere", "src", "tel:123"] {
        assert!(!opens(target), "{target}");
    }
}

#[test]
fn a_plugin_s_writes_apply_in_their_order_as_its_call_returns() {
    let mut app = App::default();
    let window = window(&mut app, "stty -echo; echo ready; cat");
    let pane = app.workspace.active_pane(window).unwrap();
    text_once(&app, pane, "ready");
    app.workspace.pane(pane).unwrap().clear_selection();
    app.clipboard.0 = Some("before".to_owned());
    let mut plugins = Plugins::new();
    let log = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&log);
    plugins
        .add(
            "writer",
            scripted(
                |cx| cx.register_command(command("writer.write")),
                move |cx, _, _| {
                    let pane = cx.focused_pane().unwrap();
                    cx.set_clipboard_text("one")?;
                    // A read in the call reads the state before its writes.
                    seen.borrow_mut().push(cx.clipboard_text()?);
                    cx.send_text(pane, "a")?;
                    cx.set_clipboard_text("two")?;
                    cx.send_text(pane, "b\n")?;
                    Ok(())
                },
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("writer.write", Args::new(), &mut app.context())
        .unwrap();
    assert_eq!(*log.borrow(), [Some("before".to_owned())]);
    assert_eq!(app.clipboard.1, ["one", "two"]);
    // The program read `a`, then `b`.
    assert!(text_once(&app, pane, "\nab").contains("ready\nab"));
}

#[test]
fn a_selection_that_a_plugin_makes_reads_after_its_call() {
    // A program that writes nothing, which would clear the selection.
    let mut app = App::default();
    let window = window(&mut app, "sleep 10");
    let pane = app.workspace.active_pane(window).unwrap();
    let mut plugins = Plugins::new();
    let log = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&log);
    plugins
        .add(
            "selector",
            scripted(
                |cx| cx.register_command(command("selector.select")),
                move |cx, _, _| {
                    let pane = cx.focused_pane().unwrap();
                    cx.select_all(pane)?;
                    seen.borrow_mut().push(cx.selection_text(pane)?.is_some());
                    Ok(())
                },
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("selector.select", Args::new(), &mut app.context())
        .unwrap();
    assert_eq!(*log.borrow(), [false]);
    assert!(app.workspace.pane(pane).unwrap().selection().is_some());
}

#[test]
fn a_read_or_a_write_of_a_pane_that_closed_is_not_found() {
    let mut app = App::default();
    let window = window(&mut app, "sleep 10");
    let mut plugins = Plugins::new();
    let log = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&log);
    let kept = Rc::new(RefCell::new(None));
    plugins
        .add(
            "keeper",
            scripted(
                |cx| cx.register_command(command("keeper.look")),
                move |cx, _, _| {
                    let pane = *kept
                        .borrow_mut()
                        .get_or_insert_with(|| cx.focused_pane().unwrap());
                    let mut seen = seen.borrow_mut();
                    seen.push(cx.selection_text(pane).map_err(|error| error.code));
                    seen.push(
                        cx.send_text(pane, "x")
                            .map(|()| None)
                            .map_err(|error| error.code),
                    );
                    Ok(())
                },
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    let mut look = |app: &mut App| {
        plugins
            .run("keeper.look", Args::new(), &mut app.context())
            .unwrap();
    };
    look(&mut app);
    assert!(app.workspace.close_window(window));
    look(&mut app);
    assert_eq!(
        *log.borrow(),
        [Ok(None), Ok(None), Err(Code::NotFound), Err(Code::NotFound)]
    );
}

#[test]
fn a_request_needs_its_capability() {
    let mut app = App::default();
    window(&mut app, "sleep 10");
    let mut plugins = Plugins::new();
    let log = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&log);
    let manifest = Manifest::parse(concat!(
        "[plugin]\nname = \"acme/reader\"\nversion = \"1.0.0\"\nkind = \"luau\"\n",
        "api = \"^0.1\"\nlicense = \"MIT\"\n[capabilities]\nterminal-read = true\n",
    ))
    .unwrap();
    plugins
        .add_declared(
            manifest,
            scripted(
                |cx| cx.register_command(command("reader.read")),
                move |cx, _, _| {
                    let pane = cx.focused_pane().unwrap();
                    let mut seen = seen.borrow_mut();
                    let code = |done: Result<(), Error>| done.err().map(|error| error.code);
                    seen.push(code(cx.selection_text(pane).map(drop)));
                    seen.push(code(cx.select_all(pane)));
                    seen.push(code(cx.clipboard_text().map(drop)));
                    seen.push(code(cx.send_text(pane, "x")));
                    seen.push(code(cx.open("https://lokyr.dev")));
                    Ok(())
                },
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("reader.read", Args::new(), &mut app.context())
        .unwrap();
    assert_eq!(
        *log.borrow(),
        [
            None,
            None,
            Some(Code::MissingCapability),
            Some(Code::MissingCapability),
            Some(Code::MissingCapability)
        ]
    );
    // Nothing that it lacked went to the app.
    assert!(plugins.take_requests().is_empty());
}

#[test]
fn a_call_is_its_own_plugin_s() {
    let mut app = App::default();
    let mut context = app.context();
    let (mut state, mut requests, mut writes, mut changed) = (
        State {
            name: "x".to_owned(),
            manifest: None,
            status: Status::Active,
            granted: None,
            uses: None,
            namespace: "x".to_owned(),
            subscribed: Vec::new(),
            commands: Vec::new(),
            bindings: Vec::new(),
            catalogs: &[],
        },
        Vec::new(),
        Vec::new(),
        false,
    );
    let mut host = Host {
        state: &mut state,
        context: &mut context,
        requests: &mut requests,
        writes: &mut writes,
        bindings_changed: &mut changed,
    };
    let call = |caller: &str| Call {
        caller: caller.to_owned(),
        request: Asked::Quit,
    };
    let forged = host::Host::handle(&mut host, call("y"));
    assert!(
        matches!(&forged, Response::Failed(error) if error.code == Code::Fault),
        "{forged:?}"
    );
    assert_eq!(host::Host::handle(&mut host, call("x")), Response::Done);
    assert_eq!(requests, [Request::Quit]);
}

#[test]
fn a_plugin_that_declares_runs_its_own_commands_and_those_of_its_uses() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    // The app's commands, which another plugin registers.
    plugins
        .add(
            "app",
            scripted(
                |cx| {
                    cx.register_command(command("app.used"))?;
                    cx.register_command(Command {
                        capabilities: vec![Capability::PtyWrite],
                        ..command("app.writes")
                    })?;
                    cx.register_command(command("app.other"))
                },
                |_, _, _| Ok(()),
            ),
        )
        .unwrap();
    let manifest = Manifest::parse(concat!(
        "[plugin]\nname = \"acme/runner\"\nversion = \"1.0.0\"\nkind = \"luau\"\n",
        "api = \"^0.1\"\nlicense = \"MIT\"\n",
        "[uses]\ncommands = [\"app.used\", \"app.writes\"]\n",
    ))
    .unwrap();
    let log = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&log);
    plugins
        .add_declared(
            manifest,
            scripted(
                |cx| cx.register_command(command("runner.go")),
                move |cx, _, _| {
                    let mut seen = seen.borrow_mut();
                    for name in ["runner.go", "app.used", "app.writes", "app.other"] {
                        let ran = cx.run_command(name, Args::new());
                        seen.push(ran.err().map(|error| error.code));
                    }
                    Ok(())
                },
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(
        plugins.manifest("acme/runner").map(Manifest::namespace),
        Some("runner")
    );
    plugins
        .run("runner.go", Args::new(), &mut app.context())
        .unwrap();
    // Its own command and the one that it declares run; the one that needs
    // a capability that it lacks, and the one that it does not declare,
    // do not.
    assert_eq!(
        *log.borrow(),
        [
            None,
            None,
            Some(Code::MissingCapability),
            Some(Code::Undeclared)
        ]
    );
}

#[test]
fn a_built_in_plugin_comes_by_its_manifest_s_name_with_every_capability() {
    let mut app = App::default();
    window(&mut app, "sleep 10");
    let mut plugins = Plugins::new();
    let manifest = concat!(
        "[plugin]\nname = \"lokyr/reader\"\nkind = \"builtin\"\nlicense = \"MIT\"\n",
        "[capabilities]\nterminal-read = true\n",
    );
    let log = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&log);
    plugins
        .add_builtin(
            manifest,
            scripted(
                |cx| cx.register_command(command("reader.read")),
                move |cx, _, _| {
                    seen.borrow_mut().push(cx.clipboard_text().is_ok());
                    Ok(())
                },
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(plugins.status("lokyr/reader"), Some(Status::Active));
    plugins
        .run("reader.read", Args::new(), &mut app.context())
        .unwrap();
    // Beyond what its manifest tells it uses.
    assert_eq!(*log.borrow(), [true]);
    // One whose manifest does not read is not added.
    let wrong = plugins.add_builtin(
        "[plugin]\nname = \"x\"\n",
        scripted(|_| Ok(()), |_, _, _| Ok(())),
    );
    assert!(wrong.is_err());
    assert_eq!(plugins.status("x"), None);
}

#[test]
fn events_come_in_batches_of_those_subscribed_to_in_their_order() {
    use lokyr_plugin_sdk::message::Data;

    let mut app = App::default();
    let mut plugins = Plugins::new();
    let (tabs, panes) = (Rc::default(), Rc::default());
    plugins
        .add("tabs", listening(&["tab-created", "acme.ping"], &tabs))
        .unwrap();
    plugins
        .add("panes", listening(&["pane-focused"], &panes))
        .unwrap();
    plugins
        .add(
            "acme",
            scripted(
                |cx| cx.register_command(command("acme.ping")),
                |cx, _, _| cx.emit("acme.ping", "acme.ping/1", Data::Integer(1)),
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    let created = |tab| {
        Event::Workspace(WorkspaceEvent::TabCreated {
            tab: host::TabId(tab),
            window: host::WindowId(1),
            index: 0,
        })
    };
    plugins.post(created(1));
    plugins.post(Event::Workspace(WorkspaceEvent::PaneFocused {
        pane: host::PaneId(3),
        tab: host::TabId(1),
    }));
    plugins.post(created(2));
    plugins
        .run("acme.ping", Args::new(), &mut app.context())
        .unwrap();
    // Nothing until the delivery.
    assert!(tabs.borrow().is_empty() && panes.borrow().is_empty());
    assert!(plugins.has_events());
    assert!(plugins.deliver(&mut app.context()).is_empty());
    assert_eq!(*tabs.borrow(), ["tab-created tab-created acme.ping"]);
    assert_eq!(*panes.borrow(), ["pane-focused"]);
    // Delivered once.
    assert!(!plugins.has_events());
    assert!(plugins.deliver(&mut app.context()).is_empty());
    assert_eq!(tabs.borrow().len(), 1);
}

#[test]
fn an_event_comes_after_the_call_that_emits_it_and_a_delivery_s_in_the_next() {
    use lokyr_plugin_sdk::message::Data;

    let mut app = App::default();
    let mut plugins = Plugins::new();
    let log = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&log);
    let inside = Rc::new(std::cell::Cell::new(false));
    let within = Rc::clone(&inside);
    plugins
        .add(
            "echo",
            Box::new(Scripted {
                activate: Box::new(|cx| {
                    cx.register_command(command("echo.go"))?;
                    cx.subscribe(&["echo.ping", "echo.pong"])
                }),
                on_command: Box::new(move |cx, _, _| {
                    within.set(true);
                    cx.emit("echo.ping", "echo/1", Data::Nil)?;
                    within.set(false);
                    Ok(())
                }),
                on_event: Some(Box::new(move |cx, events| {
                    assert!(!inside.get(), "an event within the call that emitted it");
                    for event in events {
                        seen.borrow_mut().push(event.name().to_owned());
                        if event.name() == "echo.ping" {
                            cx.emit("echo.pong", "echo/1", Data::Text("back".to_owned()))?;
                        }
                    }
                    Ok(())
                })),
            }),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("echo.go", Args::new(), &mut app.context())
        .unwrap();
    assert!(log.borrow().is_empty());
    // The ping, whose answer waits for the next batch.
    assert!(plugins.deliver(&mut app.context()).is_empty());
    assert_eq!(*log.borrow(), ["echo.ping"]);
    assert!(plugins.has_events());
    assert!(plugins.deliver(&mut app.context()).is_empty());
    assert_eq!(*log.borrow(), ["echo.ping", "echo.pong"]);
}

#[test]
fn a_subscription_and_an_event_s_name_are_checked() {
    use lokyr_plugin_sdk::message::Data;

    let mut app = App::default();
    let mut plugins = Plugins::new();
    let log = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&log);
    plugins
        .add(
            "checker",
            scripted(
                move |cx| {
                    let mut seen = seen.borrow_mut();
                    let code = |done: Result<(), Error>| done.err().map(|error| error.code);
                    seen.push(code(cx.subscribe(&["tab-created", "other.event"])));
                    seen.push(code(cx.subscribe(&["tab-made"])));
                    seen.push(code(cx.emit("checker.done", "checker/1", Data::Nil)));
                    seen.push(code(cx.emit("other.done", "checker/1", Data::Nil)));
                    seen.push(code(cx.emit("checker", "checker/1", Data::Nil)));
                    seen.push(code(cx.emit("checker.done", "", Data::Nil)));
                    Ok(())
                },
                |_, _, _| Ok(()),
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(
        *log.borrow(),
        [
            None,
            Some(Code::InvalidName),
            None,
            Some(Code::InvalidName),
            Some(Code::InvalidName),
            Some(Code::InvalidName)
        ]
    );
}

#[test]
fn a_plugin_that_panics_at_its_events_is_disabled_and_the_others_take_theirs() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    plugins
        .add(
            "broken",
            Box::new(Scripted {
                activate: Box::new(|cx| cx.subscribe(&["window-opened"])),
                on_command: Box::new(|_, _, _| Ok(())),
                on_event: Some(Box::new(|_, _| panic!("on purpose"))),
            }),
        )
        .unwrap();
    let log = Rc::default();
    plugins
        .add("after", listening(&["window-opened"], &log))
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins.post(Event::Workspace(WorkspaceEvent::WindowOpened {
        window: host::WindowId(1),
    }));
    let errors = plugins.deliver(&mut app.context());
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, Code::Fault);
    assert_eq!(plugins.status("broken"), Some(Status::Disabled));
    assert_eq!(*log.borrow(), ["window-opened"]);
}

#[test]
fn a_plugin_s_namespace_is_its_own_and_its_prefix_gives_it_nothing() {
    use lokyr_plugin_sdk::message::Data;

    let mut app = App::default();
    let mut plugins = Plugins::new();
    // A first-party plugin, whose commands are of another namespace.
    plugins
        .add(
            "core",
            scripted(
                |cx| {
                    cx.register_command(Command {
                        capabilities: vec![Capability::PtyWrite],
                        ..command("pane.paste")
                    })
                },
                |_, _, _| Ok(()),
            ),
        )
        .unwrap();
    let declared = |name: &str| {
        Manifest::parse(&format!(
            "[plugin]\nname = \"{name}\"\nversion = \"1.0.0\"\nkind = \"luau\"\napi = \"^0.1\"\nlicense = \"MIT\"\n"
        ))
        .unwrap()
    };
    // A second plugin of a namespace is refused.
    let taken = plugins.add_declared(
        declared("mallory/core"),
        scripted(|_| Ok(()), |_, _, _| Ok(())),
    );
    assert_eq!(taken.unwrap_err().code, Code::AlreadyRegistered);
    assert_eq!(plugins.status("mallory/core"), None);
    // One whose namespace another's commands have.
    let log = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&log);
    plugins
        .add_declared(
            declared("acme/pane"),
            scripted(
                move |cx| {
                    let mut seen = seen.borrow_mut();
                    let code = |done: Result<(), Error>| done.err().map(|error| error.code);
                    seen.push(code(cx.register_command(command("pane.evil"))));
                    seen.push(code(cx.register_command(command("other.x"))));
                    seen.push(code(cx.run_command("pane.paste", Args::new())));
                    seen.push(code(cx.emit("other.event", "x/1", Data::Nil)));
                    Ok(())
                },
                |_, _, _| Ok(()),
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(
        *log.borrow(),
        [
            Some(Code::AlreadyRegistered),
            Some(Code::InvalidName),
            Some(Code::Undeclared),
            Some(Code::InvalidName)
        ]
    );
    assert!(app.registry.get("pane.evil").is_none());
    assert!(plugins.take_requests().is_empty());
}
