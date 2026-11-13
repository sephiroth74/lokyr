//! The plugins' host: commands and bindings through the SDK, the requests
//! that wait for a call's return, the panes and the clipboard, and the
//! plugins that fail or panic.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use lokyr_plugin_sdk::command::{Category, Kind, Param, Value};
use lokyr_plugin_sdk::host::{self, Binding as PluginBinding};
use lokyr_pty::Program;
use lokyr_vt_core::terminal::{Colors, GutterKind};

use super::*;
use crate::config::KeysConfig;
use crate::keymap::{Context as Keys, Keymap, Pressed, Resolved, Resolver};
use crate::l10n::Texts;
use crate::workspace::{Environment, Grid, Settings};

mod awake;
mod bounds;
mod cells;
mod config;
mod consent;
mod images;
mod lazy;
mod pages;
mod timers;

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

pub(super) fn command(name: &str) -> Command {
    Command {
        name: name.to_owned(),
        title: name.to_owned(),
        category: Category::View,
        capabilities: Vec::new(),
        params: Vec::new(),
        when: None,
    }
}

pub(super) fn binding(keys: &str, command: &str, when: Option<&str>) -> PluginBinding {
    PluginBinding {
        keys: keys.to_owned(),
        command: command.to_owned(),
        args: Args::new(),
        when: when.map(str::to_owned),
    }
}

/// A clipboard of the test's own: its text, every text put on it, in
/// order, and the image put on it last, which takes its text's place.
#[derive(Default)]
pub(super) struct Board(Option<String>, Vec<String>, Option<Vec<u8>>);

impl Clipboard for Board {
    fn text(&self) -> Option<String> {
        self.0.clone()
    }

    fn set_text(&mut self, text: &str) {
        self.0 = Some(text.to_owned());
        self.1.push(text.to_owned());
    }

    fn set_image(&mut self, png: &[u8]) {
        self.0 = None;
        self.2 = Some(png.to_vec());
    }
}

/// The images of the tests' plugins: each asked, in order, answered by a
/// clear pixel, or by the error that the test gives.
#[derive(Default)]
pub(super) struct Painter {
    pub(super) asked: Vec<ImageSpec>,
    pub(super) failure: Option<String>,
    /// The images kept, each its plugin and its id.
    pub(super) kept: Vec<(String, u64)>,
}

impl Images for Painter {
    fn render(&mut self, plugin: &str, image: &ImageSpec) -> Result<Image, String> {
        self.asked.push(image.clone());
        if let Some(failure) = &self.failure {
            return Err(failure.clone());
        }
        let mut rendered = Image::blank(image.scale.unwrap_or(2.0));
        if image.keep {
            let id = 900 + self.kept.len() as u64;
            self.kept.push((plugin.to_owned(), id));
            rendered.id = Some(id);
        }
        Ok(rendered)
    }

    fn drop_image(&mut self, plugin: &str, id: u64) {
        self.kept.retain(|kept| *kept != (plugin.to_owned(), id));
    }
}

/// The panes of the tests' plugins: a shell that waits, and each pane
/// asked, by its id, with the pane that it was beside.
#[derive(Default)]
pub(super) struct Spawner(Vec<(PaneId, Option<PaneId>)>);

impl Panes for Spawner {
    fn spawn(&mut self, id: PaneId, near: Option<&Pane>, _: Option<&str>) -> Pane {
        self.0.push((id, near.map(Pane::id)));
        Pane::spawn_as(
            id,
            &Program::command("/bin/sh", ["-c", "sleep 30"]),
            &Environment::default(),
            &Settings::default(),
            near.map_or(GRID, Pane::grid),
            &Colors::default(),
            &Texts::new("en", &crate::l10n::KERNEL),
            || {},
        )
    }
}

/// The grid of the tests' windows.
const GRID: Grid = Grid {
    columns: 80,
    rows: 24,
    cell_width: 8,
    cell_height: 16,
};

/// What the host of the tests reaches.
#[derive(Default)]
pub(super) struct App {
    pub(super) registry: Registry,
    pub(super) workspace: Workspace,
    panes: Spawner,
    clipboard: Board,
    pub(super) images: Painter,
    /// Whether a window shows a save panel, as the test says; none does
    /// without.
    pub(super) saving: Option<Box<dyn Fn(WindowId) -> bool>>,
    pub(super) focused: Option<WindowId>,
    /// The language shown, English's of none.
    pub(super) language: Option<&'static str>,
    /// The host's time, which the test moves on, from the time of its
    /// first look.
    pub(super) clock: Option<Instant>,
    /// The plugins, as a read of them tells, which the test takes of the
    /// host's [`Plugins::infos`]; none without.
    pub(super) plugin_infos: Vec<host::PluginInfo>,
}

/// The themes of an app of the tests: the built-in ones, dark in force.
fn no_themes() -> host::Themes {
    host::Themes {
        appearance: crate::themes::ThemeVariant::Dark,
        current: crate::themes::DARK.to_owned(),
        themes: Vec::new(),
    }
}

impl App {
    /// The host's time now.
    pub(super) fn now(&mut self) -> Instant {
        *self.clock.get_or_insert_with(Instant::now)
    }

    /// Moves the host's time on by `by`: the time now.
    pub(super) fn wait(&mut self, by: Duration) -> Instant {
        let now = self.now() + by;
        self.clock = Some(now);
        now
    }

    pub(super) fn context(&mut self) -> Context<'_> {
        let now = self.now();
        Context {
            registry: &mut self.registry,
            workspace: &mut self.workspace,
            panes: &mut self.panes,
            clipboard: &mut self.clipboard,
            images: &mut self.images,
            focused: self.focused,
            font_size: 13.0,
            confirm_paste: crate::config::ConfirmPaste::Auto,
            keys: &|_, _| None,
            enabled: &|_| true,
            themes: &no_themes,
            fonts: &lokyr_plugin_sdk::fake::fonts,
            plugins: &self.plugin_infos,
            day: lokyr_plugin_sdk::fake::DAY,
            notice: false,
            saving: match &self.saving {
                Some(saving) => &**saving,
                None => &|_| false,
            },
            language: self.language.unwrap_or("en"),
            now,
        }
    }
}

/// A window whose pane runs `script` with `sh`, with the focus.
pub(super) fn window(app: &mut App, script: &str) -> WindowId {
    let pane = Pane::spawn(
        &Program::command("/bin/sh", ["-c", script]),
        &Environment::default(),
        &Settings::default(),
        GRID,
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
pub(super) fn text_once(app: &App, pane: PaneId, wanted: &str) -> String {
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
    assert!(plugins.take_declarations_changed());
    assert!(!plugins.take_declarations_changed());
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
    plugins.take_declarations_changed();
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
    assert!(plugins.take_declarations_changed());
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
/// English and Italian, the second in English alone, and the third not,
/// and of three menus' items, of a message, of a text and of none, the
/// first in a submenu of a message and the second in one of a text; or
/// whose catalogs panic.
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
        for (command, title) in [
            ("titled.first", Some("command-titled-first")),
            ("titled.second", Some("Just This")),
            ("titled.third", None),
        ] {
            cx.add_menu_item(host::MenuItem {
                place: host::MenuPlace::PaneContext,
                command: command.to_owned(),
                args: Args::new(),
                title: title.map(str::to_owned),
                submenu: title.map(|title| title.replace("Just This", "Just Here")),
                ticked: false,
            })?;
        }
        Ok(())
    }

    fn catalogs(&self) -> lokyr_plugin_sdk::plugin::Catalogs {
        assert!(!self.panics, "on purpose");
        lokyr_plugin_sdk::plugin::built_in(&TITLED)
    }
}

#[test]
fn a_plugin_s_catalogs_title_its_commands_in_the_language_shown() {
    let titles = |plugin: Titled, language: &str| -> Vec<String> {
        let mut app = App::default();
        let mut plugins = Plugins::new();
        plugins.add("titled", Box::new(plugin)).unwrap();
        assert!(plugins.activate(&mut app.context()).is_empty());
        let commands = plugins.commands_in(language);
        // Each of its plugin.
        assert!(commands.iter().all(|(_, plugin, _)| plugin == "titled"));
        commands
            .into_iter()
            .map(|(command, _, _)| command.title)
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
fn a_plugin_s_menu_items_take_their_titles_from_its_catalogs_or_their_own() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    plugins
        .add("titled", Box::new(Titled { panics: false }))
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    let titles = |plugins: &Plugins, language: &str| -> Vec<Option<String>> {
        plugins
            .menu_items_in(language)
            .into_iter()
            .map(|item| item.title)
            .collect()
    };
    assert_eq!(
        titles(&plugins, "it"),
        [Some("Primo".to_owned()), Some("Just This".to_owned()), None]
    );
    assert_eq!(titles(&plugins, "en")[0].as_deref(), Some("First"));
    let item = &plugins.menu_items_in("en")[2];
    assert_eq!(item.place, host::MenuPlace::PaneContext);
    assert_eq!(item.invocation.command, "titled.third");
    // Their submenus' titles alike.
    let submenus: Vec<Option<String>> = plugins
        .menu_items_in("it")
        .into_iter()
        .map(|item| item.submenu)
        .collect();
    assert_eq!(
        submenus,
        [Some("Primo".to_owned()), Some("Just Here".to_owned()), None]
    );
}

#[test]
fn a_declared_plugin_s_keys_run_what_it_may_run_as_its_menu_items_do() {
    let mut app = App::default();
    // The app's command that types its text into a pane, of `pty-write`.
    let mut send = command("pane.send-text");
    send.capabilities = vec![Capability::PtyWrite];
    send.params = vec![Param {
        name: "text".to_owned(),
        kind: Kind::String,
        default: None,
    }];
    app.registry.register(send).unwrap();
    app.registry.register(command("tabs.new")).unwrap();
    let codes = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&codes);
    let manifest = Manifest::parse(concat!(
        "[plugin]\nname = \"acme/keys\"\nversion = \"1.0.0\"\nkind = \"luau\"\n",
        "api = \"^0.1\"\nlicense = \"MIT\"\n[capabilities]\nglobal-hotkey = true\n",
        "[uses]\ncommands = [\"tabs.new\"]\n",
    ))
    .unwrap();
    let mut plugins = Plugins::new();
    plugins
        .add_declared(
            manifest,
            scripted(
                move |cx| {
                    cx.register_command(command("keys.own"))?;
                    let code = |done: Result<(), Error>| done.err().map(|error| error.code);
                    let text =
                        Args::from([("text".to_owned(), Value::String("rm -rf ~\n".to_owned()))]);
                    let mut seen = seen.borrow_mut();
                    // A global key that types into the shell at the next ⌘C
                    // in any app: of a command that it does not declare, nor
                    // the capability that it needs.
                    seen.push(code(cx.bind(host::Binding {
                        keys: "global:cmd+c".to_owned(),
                        command: "pane.send-text".to_owned(),
                        args: text.clone(),
                        when: None,
                    })));
                    // A key of the same, in the app.
                    seen.push(code(cx.bind(host::Binding {
                        keys: "ctrl+t".to_owned(),
                        command: "pane.send-text".to_owned(),
                        args: text,
                        when: None,
                    })));
                    // Its own command, and one that its manifest declares.
                    seen.push(code(cx.bind(binding("ctrl+o", "keys.own", None))));
                    seen.push(code(cx.bind(binding("ctrl+n", "tabs.new", None))));
                    // A declared one of arguments that it does not take.
                    seen.push(code(cx.bind(host::Binding {
                        keys: "ctrl+m".to_owned(),
                        command: "tabs.new".to_owned(),
                        args: Args::from([("index".to_owned(), Value::Integer(1))]),
                        when: None,
                    })));
                    Ok(())
                },
                |_, _, _| Ok(()),
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(
        *codes.borrow(),
        [
            Some(Code::Undeclared),
            Some(Code::Undeclared),
            None,
            None,
            Some(Code::UnknownArgument)
        ]
    );
}

/// A plugin whose command reads three messages of its catalogs, of
/// English and Italian, into what it said.
struct Speaker(std::rc::Rc<std::cell::RefCell<Vec<String>>>);

/// The catalogs of [`Speaker`].
static SPOKEN: [(&str, &str); 2] = [
    ("en", "tab-new = New Tab\ntab-untitled = Tab { $index }\n"),
    ("it", "tab-new = Nuovo tab\n"),
];

impl Plugin for Speaker {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        cx.register_command(command("speaker.say"))
    }

    fn catalogs(&self) -> lokyr_plugin_sdk::plugin::Catalogs {
        lokyr_plugin_sdk::plugin::built_in(&SPOKEN)
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, _: &str, _: Args) -> Result<(), Error> {
        let index = [("index", lokyr_plugin_sdk::message::Data::Integer(2))];
        let said = [
            cx.message("tab-new", &[])?,
            cx.message("tab-untitled", &index)?,
            cx.message("tab-missing", &[])?,
        ];
        self.0.borrow_mut().extend(said);
        Ok(())
    }
}

#[test]
fn a_plugin_s_messages_are_of_its_catalogs_in_the_language_shown() {
    let said = |language: &'static str| {
        let heard = std::rc::Rc::default();
        let mut app = App {
            language: Some(language),
            ..App::default()
        };
        let mut plugins = Plugins::new();
        plugins
            .add("speaker", Box::new(Speaker(std::rc::Rc::clone(&heard))))
            .unwrap();
        assert!(plugins.activate(&mut app.context()).is_empty());
        plugins
            .run("speaker.say", Args::new(), &mut app.context())
            .unwrap();
        heard.take()
    };
    // Of the language's, English's where it lacks one, and the message's
    // id where the catalogs lack it.
    assert_eq!(said("it"), ["Nuovo tab", "Tab 2", "tab-missing"]);
    assert_eq!(said("en"), ["New Tab", "Tab 2", "tab-missing"]);
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
    assert!(!plugins.take_declarations_changed());
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
    let mut asked = plugins.take_requests();
    // The run of the user's run's chain, a new one.
    let Some(Request::Run { chain, .. }) = asked.last_mut() else {
        panic!("the last request is a run");
    };
    assert!(chain.id != 0 && chain.depth == 0);
    *chain = Chain::default();
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
        asked,
        [
            Request::OpenWindow(None),
            Request::Quit,
            Request::FontSize(Some(20.0)),
            Request::FontSize(None),
            Request::CloseWindow(window),
            Request::Run {
                invocation: Invocation {
                    command: "app.other".to_owned(),
                    args: Args::from([("n".to_owned(), Value::Number(2.0))]),
                },
                chain: Chain::default(),
            },
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
fn a_call_that_writes_tells_it_and_one_that_reads_does_not() {
    let mut app = App::default();
    let window = window(&mut app, "sleep 10");
    let pane = app.workspace.active_pane(window).unwrap();
    let mut plugins = Plugins::new();
    plugins
        .add(
            "both",
            scripted(
                |cx| {
                    cx.register_command(command("both.read"))?;
                    cx.register_command(command("both.select"))
                },
                move |cx, name, _| {
                    let pane = cx.focused_pane().unwrap();
                    if name == "both.select" {
                        cx.select_all(pane)?;
                    } else {
                        cx.selection_text(pane)?;
                    }
                    Ok(())
                },
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    let _ = plugins.take_written();
    plugins
        .run("both.read", Args::new(), &mut app.context())
        .unwrap();
    assert!(!plugins.take_written());
    plugins
        .run("both.select", Args::new(), &mut app.context())
        .unwrap();
    assert!(app.workspace.pane(pane).unwrap().selection().is_some());
    assert!(plugins.take_written());
    // Taken, it is told once.
    assert!(!plugins.take_written());
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
fn a_tab_s_folder_and_program_are_read_of_process_read_alone() {
    let mut app = App::default();
    window(&mut app, "echo out; sleep 10");
    let shown = app.focused.unwrap();
    let tab = app.workspace.window(shown).unwrap().active_tab();
    assert!(
        app.workspace
            .set_tab_place(tab, Some("src — vim".to_owned()))
    );
    // A program other than a shell, which prints of its own.
    assert!(app.workspace.set_tab_running(tab, true, false));
    let start = std::time::Instant::now();
    while !app.workspace.tab(tab).unwrap().marks().printing {
        assert!(start.elapsed() < std::time::Duration::from_secs(10));
        let panes = app.workspace.pane_ids();
        app.workspace.collect(&panes, std::time::Instant::now());
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let mut plugins = Plugins::new();
    let titles = Rc::new(RefCell::new(Vec::new()));
    for (name, capabilities) in [("acme/blind", ""), ("acme/seeing", "process-read = true\n")] {
        let manifest = Manifest::parse(&format!(
            "[plugin]\nname = \"{name}\"\nversion = \"1.0.0\"\nkind = \"luau\"\napi = \"^0.1\"\nlicense = \"MIT\"\n[capabilities]\n{capabilities}"
        ))
        .unwrap();
        let namespace = name.rsplit('/').next().unwrap().to_owned();
        let seen = Rc::clone(&titles);
        let read = format!("{namespace}.read");
        plugins
            .add_declared(
                manifest,
                scripted(
                    move |cx| cx.register_command(command(&read)),
                    move |cx, _, _| {
                        let window = cx.focused_window().unwrap();
                        let tabs = cx.tabs(window)?;
                        let tab = &tabs[0];
                        seen.borrow_mut()
                            .push((tab.title.clone(), tab.running, tab.printing));
                        Ok(())
                    },
                ),
            )
            .unwrap();
    }
    assert!(plugins.activate(&mut app.context()).is_empty());
    for command in ["blind.read", "seeing.read"] {
        plugins
            .run(command, Args::new(), &mut app.context())
            .unwrap();
    }
    assert_eq!(
        *titles.borrow(),
        [
            (String::new(), false, false),
            ("src — vim".to_owned(), true, true)
        ]
    );
}

#[test]
fn a_pane_s_change_that_a_plugin_cannot_read_tells_it_nothing() {
    let mut app = App::default();
    window(&mut app, "read line; printf '\\033]0;mine\\007'; sleep 10");
    let shown = app.focused.unwrap();
    let tab = app.workspace.window(shown).unwrap().active_tab();
    let pane = app.workspace.tab(tab).unwrap().active_pane();
    let mut plugins = Plugins::new();
    let logs = [Rc::default(), Rc::default()];
    for ((name, capabilities), log) in
        [("acme/blind", ""), ("acme/seeing", "process-read = true\n")]
            .into_iter()
            .zip(&logs)
    {
        let manifest = Manifest::parse(&format!(
            "[plugin]\nname = \"{name}\"\nversion = \"1.0.0\"\nkind = \"luau\"\napi = \"^0.1\"\nlicense = \"MIT\"\n[capabilities]\n{capabilities}"
        ))
        .unwrap();
        plugins
            .add_declared(manifest, listening(&["pane-changed"], log))
            .unwrap();
    }
    assert!(plugins.activate(&mut app.context()).is_empty());
    let changed = |app: &mut App, plugins: &mut Plugins| {
        plugins.post(Event::Workspace(WorkspaceEvent::PaneChanged {
            pane: pane.into(),
            tab: tab.into(),
        }));
        assert!(plugins.deliver(&mut app.context()).is_empty());
    };
    let counts =
        |logs: &[Rc<RefCell<Vec<String>>>; 2]| logs.each_ref().map(|log| log.borrow().len());
    // Its folder or its program, of no title of its program's, which the
    // blind plugin cannot read.
    changed(&mut app, &mut plugins);
    assert_eq!(counts(&logs), [0, 1]);
    // Its program's title, which both read; the same again tells the
    // blind one nothing.
    app.workspace.pane(pane).unwrap().text("\n");
    let start = Instant::now();
    while app
        .workspace
        .pane(pane)
        .unwrap()
        .header_title(false)
        .is_none()
    {
        assert!(start.elapsed() < Duration::from_secs(10), "no title");
        let panes = app.workspace.pane_ids();
        app.workspace.collect(&panes, Instant::now());
        std::thread::sleep(Duration::from_millis(10));
    }
    changed(&mut app, &mut plugins);
    assert_eq!(counts(&logs), [1, 2]);
    changed(&mut app, &mut plugins);
    assert_eq!(counts(&logs), [1, 3]);
}

#[test]
fn a_tab_s_change_that_a_plugin_cannot_read_tells_it_nothing() {
    let mut app = App::default();
    window(&mut app, "sleep 10");
    let shown = app.focused.unwrap();
    let tab = app.workspace.window(shown).unwrap().active_tab();
    let mut plugins = Plugins::new();
    let logs = [Rc::default(), Rc::default()];
    for ((name, capabilities), log) in
        [("acme/blind", ""), ("acme/seeing", "process-read = true\n")]
            .into_iter()
            .zip(&logs)
    {
        let manifest = Manifest::parse(&format!(
            "[plugin]\nname = \"{name}\"\nversion = \"1.0.0\"\nkind = \"luau\"\napi = \"^0.1\"\nlicense = \"MIT\"\n[capabilities]\n{capabilities}"
        ))
        .unwrap();
        plugins
            .add_declared(manifest, listening(&["tab-changed"], log))
            .unwrap();
    }
    assert!(plugins.activate(&mut app.context()).is_empty());
    let deliver = |app: &mut App, plugins: &mut Plugins| {
        for event in app.workspace.take_events() {
            if let crate::workspace::Event::Changed(change) = event {
                plugins.post(Event::Workspace(change));
            }
        }
        assert!(plugins.deliver(&mut app.context()).is_empty());
    };
    // Its folder and program, which the blind plugin cannot read.
    assert!(
        app.workspace
            .set_tab_place(tab, Some("src — vim".to_owned()))
    );
    deliver(&mut app, &mut plugins);
    assert!(logs[0].borrow().is_empty(), "{:?}", logs[0].borrow());
    assert_eq!(*logs[1].borrow(), ["tab-changed"]);
    // Its own title, which both read.
    assert!(app.workspace.set_tab_title(tab, Some("mine".to_owned())));
    deliver(&mut app, &mut plugins);
    assert_eq!(*logs[0].borrow(), ["tab-changed"]);
    assert_eq!(*logs[1].borrow(), ["tab-changed", "tab-changed"]);
    // A tab behind it, whose bell rings: a change of its marks alone,
    // which both read.
    let ringing = Pane::spawn(
        &Program::command("/bin/sh", ["-c", "sleep 0.2; printf '\\a'; sleep 10"]),
        &Environment::default(),
        &Settings::default(),
        GRID,
        &Colors::default(),
        &Texts::new("en", &crate::l10n::KERNEL),
        || {},
    );
    let behind = app.workspace.open_tab(shown, ringing, 1).unwrap();
    deliver(&mut app, &mut plugins);
    let before = logs.each_ref().map(|log| log.borrow().len());
    let start = Instant::now();
    while !app.workspace.tab(behind).unwrap().marks().bell {
        assert!(start.elapsed() < Duration::from_secs(10), "no bell");
        let panes = app.workspace.pane_ids();
        app.workspace.collect(&panes, Instant::now());
        std::thread::sleep(Duration::from_millis(10));
    }
    deliver(&mut app, &mut plugins);
    for (log, before) in logs.iter().zip(before) {
        assert!(log.borrow().len() > before, "{:?}", log.borrow());
    }
    // A change of the agent's state, which tells the program that runs:
    // the seeing plugin's alone.
    let told = logs.each_ref().map(|log| log.borrow().len());
    assert!(
        app.workspace
            .set_tab_agent(tab, Some(lokyr_plugin_sdk::host::AgentState::Working))
    );
    deliver(&mut app, &mut plugins);
    assert_eq!(logs[0].borrow().len(), told[0], "{:?}", logs[0].borrow());
    assert_eq!(
        logs[1].borrow().len(),
        told[1] + 1,
        "{:?}",
        logs[1].borrow()
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
            Some(Code::MissingCapability),
            Some(Code::MissingCapability),
            Some(Code::MissingCapability),
            Some(Code::MissingCapability)
        ]
    );
    // Nothing that it lacked went to the app.
    assert!(plugins.take_requests().is_empty());
}

#[test]
fn a_read_of_the_agents_tells_each_pane_s_agent_to_a_plugin_that_reads_the_processes() {
    use lokyr_plugin_sdk::host::{AgentInfo, AgentState};
    let mut app = App::default();
    let shown = window(&mut app, "sleep 10");
    let tab = app.workspace.window(shown).unwrap().active_tab();
    let pane = app.workspace.tab(tab).unwrap().active_pane();
    assert!(app.workspace.set_pane_agent(pane, Some("claude")));
    let mut plugins = Plugins::new();
    type Read = Result<Vec<AgentInfo>, Code>;
    let read: Rc<RefCell<Vec<Read>>> = Rc::default();
    for (name, capabilities) in [("acme/seer", "process-read = true\n"), ("acme/blind", "")] {
        let manifest = Manifest::parse(&format!(
            "[plugin]\nname = \"{name}\"\nversion = \"1.0.0\"\nkind = \"luau\"\n\
             api = \"^0.1\"\nlicense = \"MIT\"\n[capabilities]\n{capabilities}"
        ))
        .unwrap();
        let command_name = format!("{}.read", &name[5..]);
        let seen = Rc::clone(&read);
        let registered = command_name.clone();
        plugins
            .add_declared(
                manifest,
                scripted(
                    move |cx| cx.register_command(command(&registered)),
                    move |cx, _, _| {
                        seen.borrow_mut()
                            .push(cx.agents().map_err(|error| error.code));
                        Ok(())
                    },
                ),
            )
            .unwrap();
        assert!(plugins.activate(&mut app.context()).is_empty());
        plugins
            .run(&command_name, Args::new(), &mut app.context())
            .unwrap();
    }
    let read = read.borrow();
    let agents = read[0].as_ref().expect("the agents");
    assert_eq!(agents.len(), 1);
    assert_eq!(
        (
            agents[0].pane,
            agents[0].tab,
            agents[0].window,
            agents[0].name.as_str(),
            agents[0].state,
            agents[0].message.as_deref(),
        ),
        (
            pane.into(),
            tab.into(),
            shown.into(),
            "claude",
            None::<AgentState>,
            None
        )
    );
    assert!(agents[0].since < 10_000);
    assert_eq!(read[1], Err(Code::MissingCapability));
}

#[test]
fn the_settings_tab_reads_as_a_tab_of_no_pane_moved_shown_and_closed_as_a_tab() {
    use lokyr_plugin_sdk::host::TabKind;
    let mut app = App::default();
    let shown = window(&mut app, "sleep 10");
    let settings = app.workspace.open_settings(shown, "Settings").unwrap();
    let mut plugins = Plugins::new();
    type Seen = (Vec<(TabKind, String, usize, bool)>, Vec<Result<(), Code>>);
    let seen: Rc<RefCell<Seen>> = Rc::default();
    let told = Rc::clone(&seen);
    plugins
        .add(
            "tidy",
            scripted(
                |cx| cx.register_command(command("tidy.go")),
                move |cx, _, _| {
                    let window = cx.focused_window().unwrap();
                    let tabs = cx.tabs(window)?;
                    let mut told = told.borrow_mut();
                    told.0 = tabs
                        .iter()
                        .map(|tab| (tab.kind, tab.title.clone(), tab.panes.len(), tab.active))
                        .collect();
                    let settings = tabs[1].id;
                    let code = |done: Result<(), Error>| done.map_err(|error| error.code);
                    told.1.push(code(cx.set_tab_title(settings, Some("mine"))));
                    told.1.push(code(cx.activate_tab(tabs[0].id)));
                    told.1.push(code(cx.move_tab(settings, window, 0)));
                    told.1.push(code(cx.close_tab(settings, false)));
                    Ok(())
                },
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("tidy.go", Args::new(), &mut app.context())
        .unwrap();
    let seen = seen.borrow();
    assert_eq!(seen.0.len(), 2);
    assert_eq!(seen.0[0].0, TabKind::Terminal);
    assert_eq!(
        seen.0[1],
        (TabKind::Settings, "Settings".to_owned(), 0, true)
    );
    // Its title is the host's; it is shown, moved and closed as a tab.
    assert_eq!(seen.1, [Err(Code::Failed), Ok(()), Ok(()), Ok(())]);
    assert_eq!(app.workspace.settings_tab(), None);
    assert!(!app.workspace.is_settings(settings));
}

/// A plugin that does nothing, of a thread of its own if the host gives it
/// one.
struct Still;

impl Plugin for Still {}

#[test]
fn a_read_of_the_plugins_tells_their_states_and_decisions_and_a_plugin_is_turned_off_by_name() {
    use lokyr_plugin_sdk::host::{Decision, PluginInfo, PluginState};
    let mut app = App::default();
    let mut plugins = Plugins::new();
    type Read = Result<Vec<PluginInfo>, Code>;
    let read: Rc<RefCell<Vec<Read>>> = Rc::default();
    let turned: Rc<RefCell<Vec<Result<(), Code>>>> = Rc::default();
    let (seen, told) = (Rc::clone(&read), Rc::clone(&turned));
    // The manager, of every capability, as a first-party plugin of no
    // manifest.
    plugins
        .add(
            "manager",
            scripted(
                |cx| cx.register_command(command("manager.read")),
                move |cx, _, _| {
                    seen.borrow_mut()
                        .push(cx.plugins().map_err(|error| error.code));
                    let mut told = told.borrow_mut();
                    told.push(
                        cx.set_plugin_enabled("acme/notes", false)
                            .map_err(|error| error.code),
                    );
                    told.push(
                        cx.set_plugin_enabled("acme/none", true)
                            .map_err(|error| error.code),
                    );
                    Ok(())
                },
            ),
        )
        .unwrap();
    // An installed plugin of a capability granted, one refused and one
    // undecided, which waits for its command.
    let manifest = Manifest::parse(concat!(
        "[plugin]\nname = \"acme/notes\"\nversion = \"1.2.0\"\nkind = \"luau\"\n",
        "api = \"^0.1\"\nlicense = \"MIT\"\n",
        "[activation]\non = [\"command:notes.open\"]\n",
        "[capabilities]\nclipboard-read = true\nclipboard-write = true\nfs-read = [\"~/Notes\"]\n",
    ))
    .unwrap();
    plugins
        .add_installed(
            manifest,
            Box::new(|| Box::new(Still)),
            (&[Capability::ClipboardRead], &[Capability::ClipboardWrite]),
        )
        .unwrap();
    // A plugin that faults at its command.
    plugins
        .add(
            "crasher",
            scripted(
                |cx| cx.register_command(command("crasher.go")),
                |_, _, _| panic!("a fault of the test's"),
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    // The same list until one of them changes.
    let infos = plugins.infos();
    assert!(Arc::ptr_eq(&infos, &plugins.infos()));
    app.plugin_infos = infos.to_vec();
    plugins
        .run("manager.read", Args::new(), &mut app.context())
        .unwrap();
    let read = read.borrow();
    let listed = read[0].as_ref().expect("the plugins");
    let names: Vec<(&str, &str, PluginState, bool)> = listed
        .iter()
        .map(|info| {
            (
                info.name.as_str(),
                info.namespace.as_str(),
                info.state,
                info.built_in,
            )
        })
        .collect();
    assert_eq!(
        names,
        [
            ("manager", "manager", PluginState::Active, true),
            ("acme/notes", "notes", PluginState::Waiting, false),
            ("crasher", "crasher", PluginState::Active, true),
        ]
    );
    let notes = &listed[1];
    assert_eq!(notes.version.as_deref(), Some("1.2.0"));
    let decisions: Vec<(&str, Decision)> = notes
        .capabilities
        .iter()
        .map(|decided| (decided.capability.as_str(), decided.decision))
        .collect();
    assert_eq!(
        decisions,
        [
            ("clipboard-read", Decision::Granted),
            ("clipboard-write", Decision::Refused),
            ("fs-read:~/Notes", Decision::Undecided),
        ]
    );
    // The plugin turned off by its name goes to the app by its namespace;
    // one of no name is refused.
    assert_eq!(*turned.borrow(), [Ok(()), Err(Code::NotFound)]);
    let requests = plugins.take_requests();
    assert!(
        requests.iter().any(|request| matches!(
            request,
            Request::SetPluginEnabled { namespace, enabled: false } if namespace == "notes"
        )),
        "{requests:?}"
    );
    // A fault changes the list, of its message.
    let _ = plugins.run("crasher.go", Args::new(), &mut app.context());
    let after = plugins.infos();
    assert!(!Arc::ptr_eq(&infos, &after));
    assert_eq!(after[2].state, PluginState::Faulted);
    assert!(
        after[2]
            .fault
            .as_deref()
            .is_some_and(|fault| fault.contains("a fault of the test's")),
        "{:?}",
        after[2].fault
    );
}

/// Whether the host refused a plugin's calls of each group of requests
/// that take `workspace-write`, `pane-view`, `decorate` and
/// `pointer-read`, for a capability that it lacks, of the manifest's
/// `capabilities`, the part after `[capabilities]`: of a new tab, a tab's
/// title, a decoration let go and a subscription to the pointer; and how
/// many tabs the window has after them.
fn refused_of(capabilities: &str) -> (Vec<bool>, usize) {
    let mut app = App::default();
    let window = window(&mut app, "sleep 10");
    let mut plugins = Plugins::new();
    let log = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&log);
    let manifest = Manifest::parse(&format!(
        "[plugin]\nname = \"acme/tidy\"\nversion = \"1.0.0\"\nkind = \"luau\"\napi = \"^0.1\"\nlicense = \"MIT\"\n[capabilities]\n{capabilities}"
    ))
    .unwrap();
    plugins
        .add_declared(
            manifest,
            scripted(
                |cx| cx.register_command(command("tidy.go")),
                move |cx, _, _| {
                    let window = cx.focused_window().unwrap();
                    let pane = cx.focused_pane().unwrap();
                    let tab = cx.tabs(window).unwrap()[0].id;
                    let code = |done: Result<(), Error>| {
                        done.is_err_and(|error| error.code == Code::MissingCapability)
                    };
                    let mut seen = seen.borrow_mut();
                    seen.push(code(cx.new_tab(window, None).map(drop)));
                    seen.push(code(cx.set_tab_title(tab, Some("tidy"))));
                    seen.push(code(cx.undecorate(pane, 1)));
                    seen.push(code(cx.subscribe(&[POINTER])));
                    Ok(())
                },
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("tidy.go", Args::new(), &mut app.context())
        .unwrap();
    let refused = log.borrow().clone();
    let tabs = app.workspace.window(window).unwrap().tabs().len();
    (refused, tabs)
}

#[test]
fn the_workspace_s_writes_the_views_the_decorations_and_the_pointer_take_their_capabilities() {
    // Of none, each refused, the window of its one tab still.
    let (refused, tabs) = refused_of("");
    assert_eq!((refused, tabs), (vec![true; 4], 1));
    // Of each, none refused.
    let (refused, tabs) = refused_of(
        "workspace-write = true\npane-view = true\ndecorate = true\npointer-read = true\n",
    );
    assert_eq!((refused, tabs), (vec![false; 4], 2));
    // Of one, the others still refused.
    let (refused, _) = refused_of("pane-view = true\n");
    assert_eq!(refused, [true, false, true, true]);
}

#[test]
fn the_packages_capability_is_lokyr_s_own_plugins_alone() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    let asks = Rc::new(RefCell::new(Vec::new()));
    for (name, kind) in [("acme/asker", "luau"), ("lokyr/manager", "builtin")] {
        let manifest = Manifest::parse(&format!(
            "[plugin]\nname = \"{name}\"\nversion = \"1.0.0\"\nkind = \"{kind}\"\napi = \"^0.1\"\nlicense = \"MIT\"\n[capabilities]\npackages = true\n"
        ))
        .unwrap();
        let go = format!("{}.go", name.split_once('/').unwrap().1);
        let seen = Rc::clone(&asks);
        plugins
            .add_declared(
                manifest,
                scripted(
                    move |cx| cx.register_command(command(&go)),
                    move |cx, _, _| {
                        let asked = cx.packages(host::Packages::List);
                        seen.borrow_mut().push(asked.map_err(|error| error.code));
                        Ok(())
                    },
                ),
            )
            .unwrap();
    }
    // Refused to another's manifest, as a notice tells.
    assert_eq!(
        plugins.take_withheld(),
        [("acme/asker".to_owned(), Capability::Packages)]
    );
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("asker.go", Args::new(), &mut app.context())
        .unwrap();
    assert_eq!(asks.borrow()[0], Err(Code::MissingCapability));
    // Lokyr's own holds it: the job's id, and its request the app's.
    let _ = plugins.take_requests();
    plugins
        .run("manager.go", Args::new(), &mut app.context())
        .unwrap();
    let Ok(id) = asks.borrow()[1] else {
        panic!("{:?}", asks.borrow());
    };
    let started: Vec<Request> = plugins
        .take_requests()
        .into_iter()
        .filter(|request| matches!(request, Request::Packages { .. }))
        .collect();
    assert!(
        matches!(
            started.as_slice(),
            [Request::Packages { plugin, id: asked, call: host::Packages::List }]
                if plugin == "lokyr/manager" && *asked == id
        ),
        "{started:?}"
    );
}

#[test]
fn the_network_s_and_the_processes_capabilities_are_withheld_and_told() {
    let mut app = App::default();
    window(&mut app, "sleep 10");
    let mut plugins = Plugins::new();
    let manifest = Manifest::parse(concat!(
        "[plugin]\nname = \"acme/fetch\"\nversion = \"1.0.0\"\nkind = \"luau\"\n",
        "api = \"^0.1\"\nlicense = \"MIT\"\n[capabilities]\nterminal-read = true\n",
        "net = [\"api.github.com\"]\nprocess-spawn = true\n",
    ))
    .unwrap();
    let read = Rc::new(RefCell::new(None));
    let seen = Rc::clone(&read);
    plugins
        .add_declared(
            manifest,
            scripted(
                |cx| cx.register_command(command("fetch.go")),
                move |cx, _, _| {
                    let pane = cx.focused_pane().unwrap();
                    *seen.borrow_mut() = Some(cx.selection_text(pane).is_ok());
                    Ok(())
                },
            ),
        )
        .unwrap();
    assert_eq!(
        plugins.take_withheld(),
        [
            (
                "acme/fetch".to_owned(),
                Capability::Net("api.github.com".to_owned())
            ),
            ("acme/fetch".to_owned(), Capability::ProcessSpawn)
        ]
    );
    assert!(plugins.take_withheld().is_empty());
    // It runs of the others.
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("fetch.go", Args::new(), &mut app.context())
        .unwrap();
    assert_eq!(*read.borrow(), Some(true));
}

#[test]
fn a_call_is_its_own_plugin_s() {
    let mut app = App::default();
    let mut context = app.context();
    let (mut state, mut out, mut writes, mut changed) = (
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
            global_bindings: Vec::new(),
            menu_items: Vec::new(),
            catalogs: Vec::new(),
            views: BTreeMap::new(),
            panels: BTreeMap::new(),
            decorations: BTreeMap::new(),
            gutter_marks: BTreeMap::new(),
            held_cursors: std::collections::BTreeSet::new(),
            awake: host::Awake::Off,
            images: (image::Budget::default(), None),
            quick_windows: Vec::new(),
            slow: Vec::new(),
            fault: None,
            schema: None,
            config: Data::Map(BTreeMap::new()),
            stored: Stored::default(),
            wanted: true,
            ran: true,
            bar_place: host::BarPlace::default(),
            window_places: BTreeMap::new(),
            dim: 0.0,
            follows: false,
            told_tabs: BTreeMap::new(),
            told_panes: BTreeMap::new(),
            searches: Vec::new(),
            reading: Arc::default(),
            restarts: Restarts::default(),
            lingering: Vec::new(),
            replaced: Vec::new(),
            timers: BTreeMap::new(),
            triggers: vec![Trigger::Startup],
            undecided: Vec::new(),
            asking: false,
            deferred: Deferred::default(),
            woken: Vec::new(),
        },
        Outbox::default(),
        Vec::new(),
        false,
    );
    let mut host = Host {
        state: &mut state,
        context: &mut context,
        out: &mut out,
        writes: &mut writes,
        declarations_changed: &mut changed,
        looking: false,
        tally: &mut Tally::default(),
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
    assert_eq!(out.requests, [Request::Quit]);
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
fn a_plugin_of_user_commands_reads_the_whole_registry_and_runs_any_command() {
    let mut app = App::default();
    app.registry
        .register(Command {
            when: Some("selection".to_owned()),
            ..command("pane.copy")
        })
        .unwrap();
    let mut plugins = Plugins::new();
    plugins
        .add(
            "acme/other",
            scripted(
                |cx| {
                    cx.register_command(Command {
                        capabilities: vec![Capability::PtyWrite],
                        ..command("other.writes")
                    })
                },
                |_, _, _| Ok(()),
            ),
        )
        .unwrap();
    let read = Rc::new(RefCell::new(Vec::new()));
    for (name, capabilities) in [
        ("acme/blind", ""),
        ("acme/palette", "user-commands = true\n"),
    ] {
        let manifest = Manifest::parse(&format!(
            "[plugin]\nname = \"{name}\"\nversion = \"1.0.0\"\nkind = \"luau\"\napi = \"^0.1\"\nlicense = \"MIT\"\n[capabilities]\n{capabilities}"
        ))
        .unwrap();
        let namespace = name.rsplit('/').next().unwrap().to_owned();
        let seen = Rc::clone(&read);
        let command_name = format!("{namespace}.go");
        plugins
            .add_declared(
                manifest,
                scripted(
                    move |cx| cx.register_command(command(&command_name)),
                    move |cx, _, _| {
                        let commands = cx.commands()?;
                        let ran = ["pane.copy", "other.writes"]
                            .map(|name| cx.run_command(name, Args::new()).err().map(|e| e.code));
                        seen.borrow_mut().push((commands, ran));
                        Ok(())
                    },
                ),
            )
            .unwrap();
    }
    assert!(plugins.activate(&mut app.context()).is_empty());
    for command in ["blind.go", "palette.go"] {
        plugins
            .run(command, Args::new(), &mut app.context())
            .unwrap();
    }
    let read = read.borrow();
    let names: Vec<&str> = read[0].0.iter().map(|info| info.name.as_str()).collect();
    assert_eq!(
        names,
        ["pane.copy", "other.writes", "blind.go", "palette.go"]
    );
    // Without the capability: the names, titles, categories and parameters
    // alone; and no command that its manifest does not declare runs.
    assert!(
        read[0]
            .0
            .iter()
            .all(|info| info.plugin.is_none() && info.when.is_none() && info.enabled.is_none())
    );
    assert_eq!(read[0].1, [Some(Code::Undeclared), Some(Code::Undeclared)]);
    // With it: each one's plugin, of none the app's own, its context and
    // its state; and any command runs, as the user's.
    let plugins_of: Vec<Option<&str>> = read[1]
        .0
        .iter()
        .map(|info| info.plugin.as_deref())
        .collect();
    assert_eq!(
        plugins_of,
        [
            None,
            Some("acme/other"),
            Some("acme/blind"),
            Some("acme/palette")
        ]
    );
    assert_eq!(read[1].0[0].when.as_deref(), Some("selection"));
    assert!(read[1].0.iter().all(|info| info.enabled == Some(true)));
    assert_eq!(read[1].1, [None, None]);
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
                        ..command("edit.paste")
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
    // One of a namespace of the app's own contexts, whose panels would
    // hold as `pane.focused`.
    for name in ["acme/pane", "acme/selection", "acme/alternate-screen"] {
        let reserved = plugins.add_declared(declared(name), scripted(|_| Ok(()), |_, _, _| Ok(())));
        assert_eq!(
            reserved.unwrap_err().code,
            Code::AlreadyRegistered,
            "{name}"
        );
        assert_eq!(plugins.status(name), None);
    }
    // One whose namespace another's commands have.
    let log = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&log);
    plugins
        .add_declared(
            declared("acme/edit"),
            scripted(
                move |cx| {
                    let mut seen = seen.borrow_mut();
                    let code = |done: Result<(), Error>| done.err().map(|error| error.code);
                    seen.push(code(cx.register_command(command("edit.evil"))));
                    seen.push(code(cx.register_command(command("other.x"))));
                    seen.push(code(cx.run_command("edit.paste", Args::new())));
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
    assert!(app.registry.get("edit.evil").is_none());
    assert!(plugins.take_requests().is_empty());
}

/// Waits until `pane`'s row numbered `row` reads `text`, ten seconds at
/// most.
fn row_once(app: &App, pane: PaneId, row: i64, text: &str) {
    let start = Instant::now();
    while app
        .workspace
        .pane(pane)
        .unwrap()
        .line_text(row, 0)
        .is_none_or(|(line, _)| line.trim_end() != text)
    {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "no `{text}` at {row}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_read_of_the_epoch_before_a_resize_is_incomplete_without_other_rows() {
    let mut app = App::default();
    let window = window(&mut app, "seq 1 300; echo ready; sleep 30");
    let pane = app.workspace.active_pane(window).unwrap();
    row_once(&app, pane, 300, "ready");
    // The request's epoch, then a resize before the read's first part.
    let found = app.workspace.pane(pane).unwrap();
    let epoch = found.origin().unwrap().epoch;
    let reader = found.reader().unwrap();
    let grid = Grid {
        columns: 60,
        rows: 20,
        cell_width: 8,
        cell_height: 16,
    };
    app.workspace
        .pane_mut(pane)
        .unwrap()
        .resize(grid, Instant::now());
    let read = |reader, epoch| {
        super::read_rows(reader, (epoch, 0, 3), Arc::default(), Arc::default())
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
    };
    let (lines, complete) = read(reader, epoch);
    assert!(!complete);
    assert!(
        lines.is_empty(),
        "{:?}",
        lines.iter().map(|line| &line.text).collect::<Vec<_>>()
    );
    // Of the epoch now, the rows come whole.
    let found = app.workspace.pane(pane).unwrap();
    let (lines, complete) = read(found.reader().unwrap(), found.origin().unwrap().epoch);
    assert!(complete);
    assert_eq!(lines.len(), 3);
}

#[test]
fn a_read_counts_until_its_thread_ends_and_stops_as_it_is_cancelled() {
    let mut app = App::default();
    let window = window(&mut app, "seq 1 300; echo ready; sleep 30");
    let pane = app.workspace.active_pane(window).unwrap();
    row_once(&app, pane, 300, "ready");
    let found = app.workspace.pane(pane).unwrap();
    let epoch = found.origin().unwrap().epoch;
    let reading = Arc::new(AtomicUsize::new(0));
    // A read cancelled before its first part reads nothing, and tells it
    // incomplete; each, once its thread ends, counts no more.
    let cancelled = Arc::new(AtomicBool::new(true));
    let (lines, complete) = super::read_rows(
        found.reader().unwrap(),
        (epoch, 0, 300),
        cancelled,
        Arc::clone(&reading),
    )
    .recv_timeout(Duration::from_secs(10))
    .unwrap();
    assert_eq!((lines.len(), complete), (0, false));
    let (lines, complete) = super::read_rows(
        found.reader().unwrap(),
        (epoch, 0, 300),
        Arc::default(),
        Arc::clone(&reading),
    )
    .recv_timeout(Duration::from_secs(10))
    .unwrap();
    assert_eq!((lines.len(), complete), (300, true));
    let start = Instant::now();
    while reading.load(Ordering::Acquire) > 0 {
        assert!(start.elapsed() < Duration::from_secs(10), "still counted");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_plugin_s_reads_of_rows_are_a_few_at_a_time() {
    let mut app = App::default();
    window(&mut app, "seq 1 30; echo ready; sleep 30");
    let mut plugins = Plugins::new();
    let codes = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&codes);
    plugins
        .add(
            "reader",
            scripted(
                |cx| cx.register_command(command("reader.read")),
                move |cx, _, _| {
                    let pane = cx.focused_pane().unwrap();
                    let code = cx.read_lines(pane, 0, 10).err().map(|error| error.code);
                    seen.borrow_mut().push(code);
                    Ok(())
                },
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    // Of as many under way as the most, another is refused; of one fewer,
    // it reads.
    let reading = Arc::clone(&plugins.entries[0].state.reading);
    reading.store(MOST_READS, Ordering::Release);
    plugins
        .run("reader.read", Args::new(), &mut app.context())
        .unwrap();
    reading.store(MOST_READS - 1, Ordering::Release);
    plugins
        .run("reader.read", Args::new(), &mut app.context())
        .unwrap();
    assert_eq!(*codes.borrow(), [Some(Code::Unavailable), None]);
    // A plugin that stops lets its reads go, which stop at their next
    // parts.
    assert_eq!(plugins.out.reads.len(), 1);
    plugins.entries[0].state.status = Status::Disabled;
    plugins.take_reads();
    assert!(plugins.out.reads.is_empty());
}

#[test]
fn a_plugin_reads_ten_thousand_lines_on_a_thread_of_the_host_s() {
    let mut app = App::default();
    let window = window(&mut app, "seq 1 12000; echo ready; sleep 30");
    let pane = app.workspace.active_pane(window).unwrap();
    // `1` on the row numbered 0, `12000` on 11 999.
    row_once(&app, pane, 12_000, "ready");
    let mut plugins = Plugins::new();
    let log = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&log);
    plugins
        .add(
            "reader",
            Box::new(Scripted {
                activate: Box::new(|cx| cx.register_command(command("reader.read"))),
                on_command: Box::new(|cx, _, _| {
                    let pane = cx.focused_pane().unwrap();
                    cx.read_lines(pane, 1000, 10_000).map(drop)
                }),
                on_event: Some(Box::new(move |_, events| {
                    seen.borrow_mut().extend(events.iter().cloned());
                    Ok(())
                })),
            }),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    let start = Instant::now();
    plugins
        .run("reader.read", Args::new(), &mut app.context())
        .unwrap();
    // The call does not wait for the rows.
    assert!(
        start.elapsed() < Duration::from_millis(50),
        "{:?}",
        start.elapsed()
    );
    // The frames' copies go on as the thread reads, a part at a time: the
    // longest waits for a part, a small share of the whole read, on a
    // machine slow or fast.
    let mut longest = Duration::ZERO;
    while log.borrow().is_empty() {
        assert!(start.elapsed() < Duration::from_secs(10), "no rows");
        let copy = Instant::now();
        app.workspace.pane_mut(pane).unwrap().update();
        longest = longest.max(copy.elapsed());
        assert!(plugins.deliver(&mut app.context()).is_empty());
    }
    let read = start.elapsed();
    assert!(
        longest < Duration::from_millis(20).max(read / 4),
        "a frame's copy took {longest:?} of a read of {read:?}"
    );
    let log = log.borrow();
    let [
        Event::Lines {
            pane: of,
            lines,
            complete,
            ..
        },
    ] = &log[..]
    else {
        panic!("one read: {log:?}");
    };
    assert_eq!(
        (*of, *complete, lines.len()),
        (host::PaneId(pane.get()), true, 10_000)
    );
    assert_eq!((lines[0].row, lines[0].text.as_str()), (1000, "1001"));
    assert_eq!(lines[9999].text, "11000");
    assert!(lines.iter().all(|line| !line.wrapped));
    assert!(!plugins.has_events());
}

#[test]
fn a_plugin_s_decorations_keep_their_text_through_new_output() {
    let mut app = App::default();
    let window = window(
        &mut app,
        "printf 'one match\\ntwo\\nthe match\\n'; echo ready; read x; seq 1 40; echo done; sleep 30",
    );
    let pane = app.workspace.active_pane(window).unwrap();
    row_once(&app, pane, 3, "ready");
    let mut plugins = Plugins::new();
    let ids = Rc::new(RefCell::new(Vec::new()));
    let found = Rc::clone(&ids);
    plugins
        .add(
            "marker",
            scripted(
                |cx| cx.register_command(command("marker.mark")),
                move |cx, _, _| {
                    let pane = cx.focused_pane().unwrap();
                    let look = host::DecorationLook {
                        underline: Some(host::Underline::Curly),
                        underline_color: Some((255, 0, 0)),
                        background: None,
                        pointing: false,
                    };
                    for row in 0..3 {
                        let line = cx.line_text(pane, row, 0)?.unwrap();
                        if let Some(at) = line.text.find("match") {
                            let at = u16::try_from(at).unwrap();
                            let id = cx.decorate(pane, (at, row), (at + 4, row), look)?;
                            found.borrow_mut().push(id);
                        }
                    }
                    Ok(())
                },
            ),
        )
        .unwrap();
    plugins
        .add("other", scripted(|_| Ok(()), |_, _, _| Ok(())))
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("marker.mark", Args::new(), &mut app.context())
        .unwrap();
    let spans = |app: &mut App| {
        let found = app.workspace.pane_mut(pane).unwrap();
        found.update();
        found
            .snapshot()
            .decorations()
            .iter()
            .map(|span| (span.row, span.first, span.last))
            .collect::<Vec<_>>()
    };
    assert_eq!(spans(&mut app), [(0, 4, 8), (2, 4, 8)]);
    // New output scrolls them out of the view; up there, they kept their
    // text.
    app.workspace.pane(pane).unwrap().send_text("\r");
    row_once(&app, pane, 45, "done");
    assert!(spans(&mut app).is_empty());
    app.workspace.pane(pane).unwrap().scroll(Scroll::Top);
    assert_eq!(spans(&mut app), [(0, 4, 8), (2, 4, 8)]);
    // Another plugin cannot take them away; their own can.
    let first = ids.borrow()[0];
    let mut asked = Plugins::new();
    asked
        .add(
            "thief",
            scripted(
                |cx| cx.register_command(command("thief.take")),
                move |cx, _, _| {
                    let pane = cx.focused_pane().unwrap();
                    cx.undecorate(pane, first)
                },
            ),
        )
        .unwrap();
    assert!(asked.activate(&mut app.context()).is_empty());
    let refused = asked
        .run("thief.take", Args::new(), &mut app.context())
        .unwrap_err();
    assert_eq!(refused.code, Code::NotFound);
    assert_eq!(spans(&mut app).len(), 2);
}

#[test]
fn a_plugin_s_cursor_goes_as_the_plugin_stops() {
    let mut app = App::default();
    let window = window(&mut app, "echo ready; sleep 30");
    let pane = app.workspace.active_pane(window).unwrap();
    row_once(&app, pane, 0, "ready");
    let mut plugins = Plugins::new();
    plugins
        .add(
            "pointer",
            scripted(
                |cx| {
                    cx.register_command(command("pointer.show"))?;
                    cx.register_command(command("pointer.panic"))
                },
                move |cx, name, _| {
                    let pane = cx.focused_pane().unwrap();
                    if name == "pointer.panic" {
                        panic!("the plugin fails");
                    }
                    let at = cx.cursor_cell(pane)?;
                    assert_eq!(at, Some((0, 1)));
                    cx.show_cursor(pane, Some((2, 0)))
                },
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("pointer.show", Args::new(), &mut app.context())
        .unwrap();
    let held = |app: &App| {
        app.workspace
            .pane(pane)
            .unwrap()
            .terminal()
            .unwrap()
            .held_cursor()
    };
    assert_eq!(held(&app), Some((2, 0)));
    // A panic disables the plugin, whose cursor goes.
    let _ = plugins.run("pointer.panic", Args::new(), &mut app.context());
    assert_eq!(plugins.status("pointer"), Some(Status::Disabled));
    assert_eq!(held(&app), None);
}

#[test]
fn a_plugin_s_gutter_marks_keep_their_rows_through_new_output() {
    let mut app = App::default();
    let window = window(
        &mut app,
        "printf 'one\\ntwo\\nthree\\n'; echo ready; read x; seq 1 40; echo done; sleep 30",
    );
    let pane = app.workspace.active_pane(window).unwrap();
    row_once(&app, pane, 3, "ready");
    let mut plugins = Plugins::new();
    let ids = Rc::new(RefCell::new(Vec::new()));
    let found = Rc::clone(&ids);
    plugins
        .add(
            "marker",
            scripted(
                |cx| {
                    cx.register_command(command("marker.mark"))?;
                    cx.register_command(command("marker.erase"))
                },
                move |cx, name, _| {
                    let pane = cx.focused_pane().unwrap();
                    if name == "marker.erase" {
                        let first = found.borrow()[0];
                        return cx.unmark_gutter(pane, first);
                    }
                    found
                        .borrow_mut()
                        .push(cx.mark_gutter(pane, 0, (255, 0, 0))?);
                    found
                        .borrow_mut()
                        .push(cx.mark_gutter(pane, 2, (0, 0, 255))?);
                    let missing = cx.mark_gutter(pane, 900, (0, 0, 0)).unwrap_err();
                    assert_eq!(missing.code, Code::Failed);
                    Ok(())
                },
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("marker.mark", Args::new(), &mut app.context())
        .unwrap();
    let marks = |app: &mut App| {
        let found = app.workspace.pane_mut(pane).unwrap();
        found.update();
        found
            .snapshot()
            .gutter()
            .iter()
            .filter_map(|mark| match mark.kind {
                GutterKind::Laid(color) => Some((mark.row, color.r, color.b)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(marks(&mut app), [(0, 255, 0), (2, 0, 255)]);
    // New output scrolls them out of the view; up there, they kept their
    // rows.
    app.workspace.pane(pane).unwrap().send_text("\r");
    row_once(&app, pane, 45, "done");
    assert!(marks(&mut app).is_empty());
    app.workspace.pane(pane).unwrap().scroll(Scroll::Top);
    assert_eq!(marks(&mut app), [(0, 255, 0), (2, 0, 255)]);
    // Another plugin cannot take them away; their own can.
    let first = ids.borrow()[0];
    let mut asked = Plugins::new();
    asked
        .add(
            "thief",
            scripted(
                |cx| cx.register_command(command("thief.take")),
                move |cx, _, _| {
                    let pane = cx.focused_pane().unwrap();
                    cx.unmark_gutter(pane, first)
                },
            ),
        )
        .unwrap();
    assert!(asked.activate(&mut app.context()).is_empty());
    let refused = asked
        .run("thief.take", Args::new(), &mut app.context())
        .unwrap_err();
    assert_eq!(refused.code, Code::NotFound);
    assert_eq!(marks(&mut app).len(), 2);
    plugins
        .run("marker.erase", Args::new(), &mut app.context())
        .unwrap();
    assert_eq!(marks(&mut app), [(2, 0, 255)]);
}

#[test]
fn a_plugin_reads_a_row_s_styles_and_decorates_only_what_the_pane_holds() {
    let mut app = App::default();
    let window = window(
        &mut app,
        "printf 'ab\\033[1;31mcd\\033[0me\\n'; echo ready; sleep 30",
    );
    let pane = app.workspace.active_pane(window).unwrap();
    row_once(&app, pane, 1, "ready");
    let mut plugins = Plugins::new();
    let log = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&log);
    plugins
        .add(
            "styler",
            scripted(
                |cx| cx.register_command(command("styler.look")),
                move |cx, _, _| {
                    let pane = cx.focused_pane().unwrap();
                    let runs = cx.styles(pane, 0)?.unwrap();
                    let mut seen = seen.borrow_mut();
                    for run in &runs[..3] {
                        seen.push(format!(
                            "{}-{} {:?} {}",
                            run.first, run.last, run.style.foreground, run.style.bold
                        ));
                    }
                    seen.push(format!("{:?}", cx.styles(pane, 900)?));
                    let look = host::DecorationLook::default();
                    let missing = cx.decorate(pane, (0, 900), (1, 900), look).unwrap_err();
                    let backwards = cx.decorate(pane, (3, 0), (1, 0), look).unwrap_err();
                    seen.push(format!("{:?} {:?}", missing.code, backwards.code));
                    Ok(())
                },
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("styler.look", Args::new(), &mut app.context())
        .unwrap();
    assert_eq!(
        *log.borrow(),
        [
            "0-1 None false",
            "2-3 Some(Palette(1)) true",
            "4-79 None false",
            "None",
            "Failed Failed"
        ]
    );
}

/// The workspace's changes that wait, as their names and ids, in order.
fn changes(app: &mut App) -> Vec<WorkspaceEvent> {
    app.workspace
        .take_events()
        .into_iter()
        .filter_map(|event| match event {
            crate::workspace::Event::Changed(change) => Some(change),
            _ => None,
        })
        .collect()
}

/// A value of lists nested `depth` levels deep, made without recursion.
pub(super) fn nested(depth: usize) -> lokyr_plugin_sdk::message::Data {
    use lokyr_plugin_sdk::message::Data;
    let mut value = Data::Nil;
    for _ in 0..depth {
        value = Data::List(vec![value]);
    }
    value
}

#[test]
fn a_plugin_s_value_nested_too_deep_is_refused_and_the_app_goes_on() {
    use lokyr_plugin_sdk::message::Data;

    let mut app = App::default();
    let state = std::env::temp_dir().join(format!("lokyr-deep-state-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    let mut plugins = Plugins::new();
    let codes = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&codes);
    plugins
        .add(
            "deep",
            scripted(
                |cx| cx.register_command(command("deep.go")),
                move |cx, _, _| {
                    let push = |result: Result<(), Error>| {
                        seen.borrow_mut().push(result.err().map(|error| error.code));
                    };
                    push(cx.set_state(nested(Data::DEEPEST)));
                    push(cx.set_state(nested(Data::DEEPEST + 1)));
                    // Far too deep for a recursive walk or drop.
                    push(cx.set_state(nested(1_000_000)));
                    push(cx.emit("deep.value", "deep.value/1", nested(1_000_000)));
                    Ok(())
                },
            ),
        )
        .unwrap();
    plugins.set_state_directory(Some(state.clone()));
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("deep.go", Args::new(), &mut app.context())
        .unwrap();
    assert_eq!(
        *codes.borrow(),
        [
            None,
            Some(Code::Failed),
            Some(Code::Failed),
            Some(Code::Failed)
        ]
    );
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
fn a_call_writes_to_the_panes_that_its_earlier_writes_open() {
    use lokyr_plugin_sdk::host::{Scroll as Scrolled, Side as Part};

    let mut app = App::default();
    window(&mut app, "sleep 30");
    let mut plugins = Plugins::new();
    let got = Rc::new(RefCell::new(None));
    let seen = Rc::clone(&got);
    plugins
        .add(
            "ws",
            scripted(
                |cx| cx.register_command(command("ws.open")),
                move |cx, _, _| {
                    let window = cx.focused_window().unwrap();
                    let (_, pane) = cx.new_tab(window, None)?;
                    let split = cx.split(pane, host::Direction::Vertical, Part::Second)?;
                    cx.send_text(pane, "sent to the new tab")?;
                    cx.scroll(pane, Scrolled::Top)?;
                    cx.select_all(split)?;
                    cx.clear(split)?;
                    cx.clear_scrollback(split)?;
                    cx.paste(split, "pasted in the split")?;
                    *seen.borrow_mut() = Some((pane, split));
                    Ok(())
                },
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("ws.open", Args::new(), &mut app.context())
        .unwrap();
    let (pane, split) = got.borrow_mut().take().unwrap();
    // The terminal's echo shows what the writes sent.
    text_once(&app, PaneId::of(pane.0), "sent to the new tab");
    text_once(&app, PaneId::of(split.0), "pasted in the split");
}

#[test]
fn a_plugin_opens_a_tab_and_a_split_of_the_ids_that_its_call_gets() {
    use lokyr_plugin_sdk::host::{Direction as Lie, Layout as Laid, Side as Part};

    let mut app = App::default();
    let window = window(&mut app, "sleep 30");
    let first = app.workspace.active_pane(window).unwrap();
    let first_tab = app.workspace.tab_of(first).unwrap();
    changes(&mut app);
    let mut plugins = Plugins::new();
    let got = Rc::new(RefCell::new(None));
    let seen = Rc::clone(&got);
    plugins
        .add(
            "ws",
            scripted(
                |cx| cx.register_command(command("ws.open")),
                move |cx, _, _| {
                    let window = cx.focused_window().unwrap();
                    let pane = cx.focused_pane().unwrap();
                    let (tab, tab_pane) = cx.new_tab(window, None)?;
                    let split = cx.split(pane, Lie::Horizontal, Part::Second)?;
                    // A read in the call reads the state before its writes,
                    // and a later write of the call takes the ids that the
                    // earlier ones gave, which a pane of none fails.
                    let tabs = cx.tabs(window)?.len();
                    let early = cx.split(tab_pane, Lie::Vertical, Part::Second);
                    let none = cx.focus_pane(lokyr_plugin_sdk::host::PaneId(9999));
                    cx.equalize(tab)?;
                    *seen.borrow_mut() = Some((tab, tab_pane, split, tabs, (early, none)));
                    Ok(())
                },
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("ws.open", Args::new(), &mut app.context())
        .unwrap();
    let (tab, tab_pane, split, tabs, (early, none)) = got.borrow_mut().take().unwrap();
    assert_eq!(tabs, 1);
    let early = early.unwrap();
    assert_eq!(none.unwrap_err().code, Code::NotFound);
    // The ids that the call got are the tab's and the panes', which its
    // later writes split and equalized.
    let opened = app.workspace.tab(TabId::of(tab.0)).unwrap();
    assert_eq!(opened.window(), window);
    assert_eq!(
        opened.layout().panes(),
        [PaneId::of(tab_pane.0), PaneId::of(early.0)]
    );
    assert_eq!(
        app.workspace.window(window).unwrap().active_tab(),
        first_tab
    );
    assert_eq!(
        app.workspace.tab(first_tab).unwrap().layout().panes(),
        [first, PaneId::of(split.0)]
    );
    // Each pane beside the window's focused one, or the one that it
    // splits, its programs started as the call returned.
    assert_eq!(
        app.panes.0,
        [
            (PaneId::of(tab_pane.0), Some(first)),
            (PaneId::of(split.0), Some(first)),
            (PaneId::of(early.0), Some(PaneId::of(tab_pane.0)))
        ]
    );
    assert_eq!(
        changes(&mut app),
        [
            WorkspaceEvent::TabCreated {
                tab,
                window: window.into(),
                index: 1
            },
            WorkspaceEvent::PaneCreated {
                pane: tab_pane,
                tab
            },
            WorkspaceEvent::PaneCreated {
                pane: split,
                tab: first_tab.into()
            },
            WorkspaceEvent::LayoutChanged {
                tab: first_tab.into()
            },
            WorkspaceEvent::PaneCreated { pane: early, tab },
            WorkspaceEvent::LayoutChanged { tab },
        ]
    );
    // The reads of the windows, their tabs and a tab's layout.
    let reads = Rc::new(RefCell::new(None));
    let read = Rc::clone(&reads);
    plugins
        .add(
            "reader",
            scripted(
                |cx| cx.register_command(command("reader.read")),
                move |cx, _, _| {
                    let windows = cx.windows()?;
                    let tabs = cx.tabs(windows[0].id)?;
                    let layout = cx.layout(tabs[0].id)?;
                    *read.borrow_mut() = Some((windows, tabs, layout));
                    Ok(())
                },
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    // The window's number and keys, as the app gives them.
    assert!(
        app.workspace
            .number_window(window, Some(1), Some("⌥⌘1".to_owned()))
    );
    plugins
        .run("reader.read", Args::new(), &mut app.context())
        .unwrap();
    let (windows, tabs, layout) = reads.borrow_mut().take().unwrap();
    assert_eq!(
        windows,
        [host::WindowInfo {
            id: window.into(),
            tabs: vec![first_tab.into(), tab],
            active: first_tab.into(),
            number: Some(1),
            keys: Some("⌥⌘1".to_owned()),
            bell: false,
            title: String::new(),
            quick: false,
            hidden: false,
        }]
    );
    assert_eq!(
        tabs,
        [
            host::TabInfo {
                id: first_tab.into(),
                window: window.into(),
                kind: lokyr_plugin_sdk::host::TabKind::Terminal,
                title: String::new(),
                color: None,
                active: true,
                panes: vec![first.into(), split],
                focused: first.into(),
                zoomed: None,
                activity: false,
                bell: false,
                progress: None,
                printing: false,
                running: false,
                alternate: false,
                secure: false,
                agent: None,
                agent_message: None,
            },
            host::TabInfo {
                id: tab,
                window: window.into(),
                kind: lokyr_plugin_sdk::host::TabKind::Terminal,
                title: String::new(),
                color: None,
                active: false,
                panes: vec![tab_pane, early],
                focused: tab_pane,
                zoomed: None,
                activity: false,
                bell: false,
                progress: None,
                printing: false,
                running: false,
                alternate: false,
                secure: false,
                agent: None,
                agent_message: None,
            },
        ]
    );
    assert_eq!(
        layout,
        Laid::Split {
            direction: Lie::Horizontal,
            ratio: None,
            first: Box::new(Laid::Pane(first.into())),
            second: Box::new(Laid::Pane(split)),
        }
    );
}

#[test]
fn a_plugin_moves_and_closes_panes_and_tabs_and_their_events_tell_it() {
    use lokyr_plugin_sdk::host::{Direction as Lie, Side as Part};

    let mut app = App::default();
    let window = window(&mut app, "sleep 30");
    let first = app.workspace.active_pane(window).unwrap();
    let mut plugins = Plugins::new();
    let split = Rc::new(RefCell::new(None));
    let kept = Rc::clone(&split);
    plugins
        .add(
            "ws",
            scripted(
                |cx| {
                    cx.register_command(command("ws.split"))?;
                    cx.register_command(command("ws.out"))?;
                    cx.register_command(command("ws.close"))
                },
                move |cx, name, _| {
                    match name {
                        "ws.split" => {
                            let pane = cx.focused_pane().unwrap();
                            *kept.borrow_mut() =
                                Some(cx.split(pane, Lie::Vertical, Part::First)?);
                        }
                        "ws.out" => cx.pane_to_new_window(kept.borrow().unwrap())?,
                        _ => cx.close_pane(kept.borrow().unwrap(), false)?,
                    }
                    Ok(())
                },
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("ws.split", Args::new(), &mut app.context())
        .unwrap();
    let split = split.borrow().unwrap();
    let tab = app.workspace.tab_of(first).unwrap();
    changes(&mut app);
    // Out to a window of its own, in a tab of its own.
    plugins
        .run("ws.out", Args::new(), &mut app.context())
        .unwrap();
    let moved = app.workspace.tab_of(PaneId::of(split.0)).unwrap();
    let out = app.workspace.tab(moved).unwrap().window();
    assert_ne!(out, window);
    assert_eq!(app.workspace.windows().count(), 2);
    assert_eq!(
        changes(&mut app),
        [
            WorkspaceEvent::LayoutChanged { tab: tab.into() },
            WorkspaceEvent::WindowOpened { window: out.into() },
            WorkspaceEvent::TabCreated {
                tab: moved.into(),
                window: out.into(),
                index: 0
            },
            WorkspaceEvent::PaneMoved {
                pane: split,
                from: tab.into(),
                tab: moved.into()
            },
        ]
    );
    // Closed: its tab, then its window, close with it.
    plugins
        .run("ws.close", Args::new(), &mut app.context())
        .unwrap();
    assert!(app.workspace.pane(PaneId::of(split.0)).is_none());
    assert_eq!(app.workspace.windows().count(), 1);
    assert_eq!(
        changes(&mut app),
        [
            WorkspaceEvent::PaneClosed {
                pane: split,
                tab: moved.into()
            },
            WorkspaceEvent::TabClosed {
                tab: moved.into(),
                window: out.into()
            },
            WorkspaceEvent::WindowClosed { window: out.into() },
        ]
    );
}

#[test]
fn a_workspace_request_of_what_is_not_there_fails_and_writes_nothing() {
    let mut app = App::default();
    let window = window(&mut app, "sleep 30");
    let first = app.workspace.active_pane(window).unwrap();
    let tab = app.workspace.tab_of(first).unwrap();
    let mut plugins = Plugins::new();
    let codes = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&codes);
    let (window_id, tab_id, pane_id) = (window.get(), tab.get(), first.get());
    plugins
        .add(
            "ws",
            scripted(
                |cx| {
                    cx.register_command(command("ws.wrong"))?;
                    cx.register_command(command("ws.late"))
                },
                move |cx, name, _| {
                    let (window, tab, pane) = (
                        host::WindowId(window_id),
                        host::TabId(tab_id),
                        host::PaneId(pane_id),
                    );
                    let none = (host::WindowId(999), host::TabId(999), host::PaneId(999));
                    if name == "ws.late" {
                        // A tab of a window that an earlier write of the
                        // call closes starts no program.
                        cx.close_tab(tab, false)?;
                        cx.new_tab(window, None)?;
                        return Ok(());
                    }
                    let results = [
                        cx.new_tab(none.0, None).map(|_| ()),
                        cx.tabs(none.0).map(|_| ()),
                        cx.layout(none.1).map(|_| ()),
                        cx.close_tab(none.1, false),
                        cx.focus_pane(none.2),
                        cx.move_tab(tab, none.0, 0),
                        cx.zoom(tab, Some(none.2)),
                        cx.resize_pane(pane, host::Edge::Right, 2.0),
                        cx.set_ratio(tab, Vec::new(), Some(1.5)),
                    ];
                    seen.borrow_mut()
                        .extend(results.map(|result| result.unwrap_err().code.number()));
                    Ok(())
                },
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    changes(&mut app);
    plugins
        .run("ws.wrong", Args::new(), &mut app.context())
        .unwrap();
    assert_eq!(
        *codes.borrow(),
        [101, 101, 101, 101, 101, 101, 101, 201, 201]
    );
    assert!(changes(&mut app).is_empty());
    assert!(app.panes.0.is_empty());
    plugins
        .run("ws.late", Args::new(), &mut app.context())
        .unwrap();
    assert!(app.panes.0.is_empty());
    assert_eq!(app.workspace.windows().count(), 0);
}

/// What a [`Viewer`] asks at a click of `go`, of the view clicked, and the
/// answers.
type Answer = Rc<dyn Fn(&mut Cx<'_>, &ViewId) -> Vec<Result<(), Error>>>;

/// A plugin of views: each a text of its name and the view, a button `go`,
/// a field `find` and a list `rows`; it records its nodes' events and its
/// panels' closing, asks at a click of `go` what its `answer` gives, and
/// panics at its next render once `panics` holds.
struct Viewer {
    name: &'static str,
    log: Rc<RefCell<Vec<String>>>,
    answer: Answer,
    answers: Rc<RefCell<Vec<Result<(), Error>>>>,
    panics: Rc<std::cell::Cell<bool>>,
}

impl Plugin for Viewer {
    fn render(&mut self, _: &mut Cx<'_>, view: &ViewId) -> Option<Node> {
        assert!(!self.panics.get(), "a render's panic");
        let keyed = |key: &str, node: Node| Node {
            key: Some(Key::from(key)),
            ..node
        };
        Some(Node::container(vec![
            Node::text(format!("{} {view:?}", self.name)),
            keyed("go", Node::button("Go")),
            keyed("find", Node::field("Find")),
            keyed("rows", Node::list(Node::text("row"), 3, (0, Vec::new()))),
        ]))
    }

    fn on_ui_event(&mut self, cx: &mut Cx<'_>, event: &UiEvent) -> Result<(), Error> {
        self.log
            .borrow_mut()
            .push(format!("{:?} {} {:?}", event.view, event.key.0, event.kind));
        if event.key.0 == "go" {
            let answers = (self.answer)(cx, &event.view);
            self.answers.borrow_mut().extend(answers);
        }
        Ok(())
    }

    fn on_event(&mut self, _: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        for event in events {
            if let Event::PanelClosed(id) = event {
                self.log.borrow_mut().push(format!("closed {}", id.0));
            }
        }
        Ok(())
    }
}

/// A plugin's manifest of `name`, which fills `slots`.
fn filling(name: &str, slots: &[&str]) -> String {
    let slots: Vec<String> = slots.iter().map(|slot| format!("\"{slot}\"")).collect();
    format!(
        "[plugin]\nname = \"{name}\"\nkind = \"builtin\"\nlicense = \"MIT\"\n[contributes]\nslots = [{}]\n",
        slots.join(", ")
    )
}

/// The views' plugins of the tests, each a [`Viewer`] of its own log, all
/// of the same `answer`, and the `panics` of `lokyr/tabs`.
#[allow(clippy::type_complexity)]
fn viewers(
    answer: &Answer,
) -> (
    Plugins,
    Rc<RefCell<Vec<String>>>,
    Rc<RefCell<Vec<Result<(), Error>>>>,
    Rc<std::cell::Cell<bool>>,
) {
    let (log, answers) = (
        Rc::new(RefCell::new(Vec::new())),
        Rc::new(RefCell::new(Vec::new())),
    );
    let panics = Rc::new(std::cell::Cell::new(false));
    let mut plugins = Plugins::new();
    for (name, slots) in [
        ("lokyr/tabs", &["tab-bar", "pane-header"][..]),
        ("lokyr/bars", &["tab-bar", "status-bar", "window-strip"][..]),
        ("lokyr/clock", &["status-bar"][..]),
        ("lokyr/plain", &[][..]),
    ] {
        let viewer = Viewer {
            name,
            log: Rc::clone(&log),
            answer: Rc::clone(answer),
            answers: Rc::clone(&answers),
            panics: if name == "lokyr/tabs" {
                Rc::clone(&panics)
            } else {
                Rc::default()
            },
        };
        plugins
            .add_builtin(&filling(name, slots), Box::new(viewer))
            .unwrap();
    }
    (plugins, log, answers, panics)
}

#[test]
fn a_plugin_renders_each_slot_that_it_fills_and_its_trees_stay_as_it_is_disabled() {
    let mut app = App::default();
    let window = window(&mut app, "sleep 30");
    let pane = app.workspace.active_pane(window).unwrap();
    let answer: Answer = Rc::new(|_, _| Vec::new());
    let (mut plugins, _, _, panics) = viewers(&answer);
    assert!(plugins.activate(&mut app.context()).is_empty());
    // The slots of one plugin's, of the config's or the first; those of
    // many, in the plugins' order.
    assert_eq!(
        plugins.providers(Slot::TabBar),
        ["lokyr/tabs", "lokyr/bars"]
    );
    assert_eq!(plugins.provider(Slot::TabBar, ""), Some("lokyr/tabs"));
    assert_eq!(
        plugins.provider(Slot::TabBar, "lokyr/bars"),
        Some("lokyr/bars")
    );
    assert_eq!(
        plugins.provider(Slot::TabBar, "lokyr/none"),
        Some("lokyr/tabs")
    );
    assert_eq!(plugins.providers(Slot::PaneHeader), ["lokyr/tabs"]);
    assert_eq!(
        plugins.providers(Slot::StatusBar),
        ["lokyr/bars", "lokyr/clock"]
    );
    assert_eq!(plugins.providers(Slot::WindowStrip), ["lokyr/bars"]);
    let (window, pane) = (host::WindowId(window.get()), host::PaneId(pane.get()));
    let views = [
        ("lokyr/tabs", ViewId::TabBar(window)),
        ("lokyr/tabs", ViewId::PaneHeader(pane)),
        ("lokyr/bars", ViewId::WindowStrip(window)),
        ("lokyr/bars", ViewId::StatusBar(window)),
        ("lokyr/clock", ViewId::StatusBar(window)),
    ];
    for (name, view) in &views {
        plugins.render(name, view, &mut app.context()).unwrap();
        let tree = plugins.tree(name, view).unwrap();
        assert_eq!(
            tree.children[0].kind,
            lokyr_plugin_sdk::view::Kind::Text(format!("{name} {view:?}"))
        );
    }
    assert_eq!(plugins.views().len(), views.len());
    // Each render of a number of its own, which a host compares with its
    // surface's.
    let bar = ViewId::TabBar(window);
    let first = plugins.rendered("lokyr/tabs", &bar).unwrap();
    plugins
        .render("lokyr/tabs", &bar, &mut app.context())
        .unwrap();
    assert_ne!(plugins.rendered("lokyr/tabs", &bar), Some(first));
    assert_eq!(plugins.rendered("lokyr/plain", &bar), None);
    assert!(plugins.take_disabled().is_empty());
    // A render that panics disables its plugin, whose last tree stays, and
    // whose slot it still fills; the app hears it once.
    panics.set(true);
    let fault = plugins.render("lokyr/tabs", &bar, &mut app.context());
    assert_eq!(fault.unwrap_err().code, Code::Fault);
    assert_eq!(plugins.status("lokyr/tabs"), Some(Status::Disabled));
    let disabled = plugins.take_disabled();
    assert_eq!(disabled.len(), 1);
    assert_eq!(disabled[0].0, "lokyr/tabs");
    assert!(disabled[0].1.contains("panicked"), "{}", disabled[0].1);
    assert!(plugins.take_disabled().is_empty());
    assert!(plugins.tree("lokyr/tabs", &bar).is_some());
    plugins
        .render("lokyr/tabs", &bar, &mut app.context())
        .unwrap();
    assert!(plugins.tree("lokyr/tabs", &bar).is_some());
    assert_eq!(plugins.provider(Slot::TabBar, ""), Some("lokyr/tabs"));
    // The views of a window that closed, and of its pane, go with it.
    plugins.post(Event::Workspace(WorkspaceEvent::PaneClosed {
        pane,
        tab: host::TabId(0),
    }));
    plugins.post(Event::Workspace(WorkspaceEvent::WindowClosed { window }));
    assert!(plugins.views().is_empty());
}

#[test]
fn a_plugin_takes_its_nodes_events_and_its_requests_of_its_views_are_checked() {
    let mut app = App::default();
    let window = window(&mut app, "sleep 30");
    let pane = host::PaneId(app.workspace.active_pane(window).unwrap().get());
    let window = host::WindowId(window.get());
    let answer: Answer = Rc::new(move |cx, view| {
        vec![
            cx.set_text(view.clone(), "find", "x", 3),
            cx.select(view.clone(), "rows", 2),
            cx.request_render(view.clone()),
            // Of nodes that are not a field, a list, or there.
            cx.set_text(view.clone(), "go", "x", 3),
            cx.select(view.clone(), "find", 0),
            cx.set_text(view.clone(), "nowhere", "x", 0),
            // Of views that are not the plugin's, or not rendered.
            cx.request_render(ViewId::PaneHeader(pane)),
            cx.set_text(ViewId::TabBar(window), "find", "x", 0),
        ]
    });
    let (mut plugins, log, answers, _) = viewers(&answer);
    assert!(plugins.activate(&mut app.context()).is_empty());
    let view = ViewId::StatusBar(window);
    plugins
        .render("lokyr/bars", &view, &mut app.context())
        .unwrap();
    plugins.take_renders();
    let click = UiEvent {
        view: view.clone(),
        key: Key::from("go"),
        kind: lokyr_plugin_sdk::view::UiEventKind::Click,
    };
    plugins
        .ui_event("lokyr/bars", &click, &mut app.context())
        .unwrap();
    assert_eq!(*log.borrow(), [format!("{view:?} go Click")]);
    let codes: Vec<Option<u16>> = answers
        .borrow()
        .iter()
        .map(|answer| answer.as_ref().err().map(|error| error.code.number()))
        .collect();
    assert_eq!(
        codes,
        [
            None,
            None,
            None,
            Some(101),
            Some(101),
            Some(101),
            Some(101),
            Some(101)
        ]
    );
    assert_eq!(
        plugins.take_requests(),
        [
            Request::SetText {
                plugin: "lokyr/bars".to_owned(),
                view: view.clone(),
                key: Key::from("find"),
                text: "x".to_owned(),
                revision: 3,
            },
            Request::Select {
                plugin: "lokyr/bars".to_owned(),
                view: view.clone(),
                key: Key::from("rows"),
                item: 2,
            },
        ]
    );
    assert_eq!(plugins.take_renders(), [("lokyr/bars".to_owned(), view)]);
    // A plugin that is not active takes no event.
    plugins
        .ui_event("lokyr/none", &click, &mut app.context())
        .unwrap();
    assert_eq!(log.borrow().len(), 1);
}

#[test]
fn a_plugin_opens_and_closes_a_panel_which_tells_it_as_it_closes() {
    use lokyr_plugin_sdk::ui::{Corner, Role};

    let mut app = App::default();
    let window = window(&mut app, "sleep 30");
    let pane = host::PaneId(app.workspace.active_pane(window).unwrap().get());
    let window = host::WindowId(window.get());
    let options = |anchor| PanelOptions {
        anchor,
        role: Role::Menu,
        close_outside: true,
        close_escape: true,
        takes_keys: false,
        close_typing: false,
    };
    let answer: Answer = Rc::new(move |cx, view| {
        vec![
            cx.open_panel(
                "menu",
                options(Anchor::Node {
                    view: view.clone(),
                    key: Key::from("go"),
                }),
            ),
            cx.open_panel(
                "corner",
                options(Anchor::PaneCorner {
                    pane,
                    corner: Corner::TopRight,
                }),
            ),
            cx.open_panel("", options(Anchor::Pointer(window))),
            cx.open_panel("lost", options(Anchor::Cursor(host::PaneId(999)))),
            cx.open_panel(
                "nowhere",
                options(Anchor::Node {
                    view: view.clone(),
                    key: Key::from("nowhere"),
                }),
            ),
        ]
    });
    let (mut plugins, log, answers, _) = viewers(&answer);
    assert!(plugins.activate(&mut app.context()).is_empty());
    let view = ViewId::StatusBar(window);
    plugins
        .render("lokyr/bars", &view, &mut app.context())
        .unwrap();
    let click = UiEvent {
        view: view.clone(),
        key: Key::from("go"),
        kind: lokyr_plugin_sdk::view::UiEventKind::Click,
    };
    plugins
        .ui_event("lokyr/bars", &click, &mut app.context())
        .unwrap();
    let codes: Vec<Option<u16>> = answers
        .borrow()
        .iter()
        .map(|answer| answer.as_ref().err().map(|error| error.code.number()))
        .collect();
    assert_eq!(codes, [None, None, Some(105), Some(101), Some(101)]);
    let menu = PanelId::from("menu");
    let requests = plugins.take_requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0],
        Request::OpenPanel {
            plugin: "lokyr/bars".to_owned(),
            id: menu.clone(),
            window: WindowId::of(window.0),
            options: options(Anchor::Node {
                view: view.clone(),
                key: Key::from("go"),
            }),
        }
    );
    // Its render as it opens.
    let panel = ViewId::Panel(menu.clone());
    assert_eq!(
        plugins.take_renders(),
        [
            ("lokyr/bars".to_owned(), panel.clone()),
            (
                "lokyr/bars".to_owned(),
                ViewId::Panel(PanelId::from("corner"))
            )
        ]
    );
    plugins
        .render("lokyr/bars", &panel, &mut app.context())
        .unwrap();
    assert!(plugins.tree("lokyr/bars", &panel).is_some());
    // Closed as the app tells, which its plugin takes at the next delivery.
    plugins.panel_closed("lokyr/bars", &menu);
    assert!(plugins.tree("lokyr/bars", &panel).is_none());
    assert!(plugins.deliver(&mut app.context()).is_empty());
    assert_eq!(log.borrow().last().unwrap(), "closed menu");
    // A panel that is not open does not close, and the window's closing
    // closes those that it has.
    let mut closes = Vec::new();
    plugins
        .add(
            "closer",
            scripted(
                |cx| cx.register_command(command("closer.close")),
                |cx, _, _| {
                    let error = cx.close_panel("menu").unwrap_err();
                    assert_eq!(error.code, Code::NotFound);
                    Ok(())
                },
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("closer.close", Args::new(), &mut app.context())
        .unwrap();
    closes.extend(plugins.take_requests());
    assert!(closes.is_empty());
    plugins.post(Event::Workspace(WorkspaceEvent::WindowClosed { window }));
    assert!(plugins.deliver(&mut app.context()).is_empty());
    assert_eq!(log.borrow().last().unwrap(), "closed corner");
    assert!(plugins.views().is_empty());
}

/// A plugin whose `look.open` would open the panel `looked`, as it only
/// looks whether it applies.
struct Looker;

impl Plugin for Looker {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        cx.register_command(command("look.open"))
    }

    fn applies(&mut self, cx: &mut Cx<'_>, _: &str, _: &Args) -> bool {
        let window = cx.focused_window().unwrap();
        cx.open_panel(
            "looked",
            PanelOptions {
                anchor: Anchor::Pointer(window),
                role: lokyr_plugin_sdk::ui::Role::Menu,
                close_outside: true,
                close_escape: true,
                takes_keys: false,
                close_typing: false,
            },
        )
        .is_ok()
    }

    fn render(&mut self, _: &mut Cx<'_>, _: &ViewId) -> Option<Node> {
        Some(Node::text("looked"))
    }
}

#[test]
fn a_panel_opens_once_of_its_id_and_no_tree_of_a_closed_panel_or_window_stays() {
    let mut app = App::default();
    let window = window(&mut app, "sleep 30");
    let window = host::WindowId(window.get());
    let options = PanelOptions {
        anchor: Anchor::Pointer(window),
        role: lokyr_plugin_sdk::ui::Role::Menu,
        close_outside: true,
        close_escape: true,
        takes_keys: false,
        close_typing: false,
    };
    // Opened twice, closed, and opened again, in one call.
    let answer: Answer = Rc::new(move |cx, _| {
        vec![
            cx.open_panel("menu", options.clone()),
            cx.open_panel("menu", options.clone()),
            cx.close_panel("menu"),
            cx.open_panel("menu", options.clone()),
        ]
    });
    let (mut plugins, log, answers, _) = viewers(&answer);
    assert!(plugins.activate(&mut app.context()).is_empty());
    let bar = ViewId::StatusBar(window);
    plugins
        .render("lokyr/bars", &bar, &mut app.context())
        .unwrap();
    let click = UiEvent {
        view: bar.clone(),
        key: Key::from("go"),
        kind: lokyr_plugin_sdk::view::UiEventKind::Click,
    };
    plugins
        .ui_event("lokyr/bars", &click, &mut app.context())
        .unwrap();
    let codes: Vec<Option<u16>> = answers
        .borrow()
        .iter()
        .map(|answer| answer.as_ref().err().map(|error| error.code.number()))
        .collect();
    assert_eq!(codes, [None, Some(106), None, None]);
    // The close told at once, and one render, of the panel opened again,
    // which stays open whatever the app does of the close.
    let menu = PanelId::from("menu");
    let panel = ViewId::Panel(menu.clone());
    assert_eq!(
        plugins.take_renders(),
        [("lokyr/bars".to_owned(), panel.clone())]
    );
    let requests = plugins.take_requests();
    assert!(matches!(
        requests.as_slice(),
        [
            Request::OpenPanel { .. },
            Request::ClosePanel { .. },
            Request::OpenPanel { .. }
        ]
    ));
    assert!(plugins.deliver(&mut app.context()).is_empty());
    assert_eq!(log.borrow().last().unwrap(), "closed menu");
    plugins
        .render("lokyr/bars", &panel, &mut app.context())
        .unwrap();
    assert!(plugins.tree("lokyr/bars", &panel).is_some());
    // Closed as the app tells: a render of it after keeps nothing.
    plugins.panel_closed("lokyr/bars", &menu);
    plugins
        .render("lokyr/bars", &panel, &mut app.context())
        .unwrap();
    assert!(plugins.rendered("lokyr/bars", &panel).is_none());
    // Nor does one of a window that closed.
    plugins.post(Event::Workspace(WorkspaceEvent::WindowClosed { window }));
    app.workspace.close_window(WindowId::of(window.0));
    plugins
        .render("lokyr/bars", &bar, &mut app.context())
        .unwrap();
    assert!(plugins.views().is_empty(), "{:?}", plugins.views());
}

#[test]
fn a_look_leaves_the_panels_as_they_were() {
    let mut app = App::default();
    let shown = window(&mut app, "sleep 30");
    app.focused = Some(shown);
    let mut plugins = Plugins::new();
    plugins.add("looker", Box::new(Looker)).unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert!(
        plugins
            .applies("look.open", &Args::new(), &mut app.context())
            .unwrap()
    );
    assert!(plugins.take_requests().is_empty());
    let looked = ViewId::Panel(PanelId::from("looked"));
    plugins
        .render("looker", &looked, &mut app.context())
        .unwrap();
    assert!(plugins.views().is_empty(), "{:?}", plugins.views());
}

#[test]
fn a_tab_moved_to_a_window_of_its_own_at_a_place_asks_the_app_to_put_the_window_there() {
    use lokyr_plugin_sdk::host::{ScreenPoint, TabId as Tab};

    let mut app = App::default();
    let window = window(&mut app, "sleep 30");
    let first = app.workspace.window(window).unwrap().active_tab();
    let mut plugins = Plugins::new();
    let at = ScreenPoint { x: 300.0, y: 120.0 };
    plugins
        .add(
            "ws",
            scripted(
                |cx| cx.register_command(command("ws.out")),
                move |cx, _, args| {
                    let tab = match args.get("tab") {
                        Some(lokyr_plugin_sdk::command::Value::Integer(tab)) => Tab(*tab as u64),
                        _ => {
                            let window = cx.focused_window().unwrap();
                            cx.new_tab(window, None)?.0
                        }
                    };
                    cx.tab_to_new_window_at(tab, at)
                },
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    // A new tab of the window, out to a window of its own.
    plugins
        .run("ws.out", Args::new(), &mut app.context())
        .unwrap();
    let requests = plugins.take_requests();
    let [
        Request::PlaceWindow {
            window: placed,
            at: there,
        },
    ] = requests[..]
    else {
        panic!("a place: {requests:?}");
    };
    assert_ne!(placed, window);
    assert_eq!(there, at);
    assert_eq!(app.workspace.window(placed).unwrap().tabs().len(), 1);
    // The first, alone in its window now, stays, its window put there.
    let mut args = Args::new();
    args.insert(
        "tab".to_owned(),
        lokyr_plugin_sdk::command::Value::Integer(first.get() as i64),
    );
    plugins.run("ws.out", args, &mut app.context()).unwrap();
    let requests = plugins.take_requests();
    assert!(
        matches!(requests[..], [Request::PlaceWindow { window: placed, .. }] if placed == window),
        "{requests:?}"
    );
    assert_eq!(app.workspace.tab(first).unwrap().window(), window);
}

#[test]
fn a_tab_s_marks_are_read_and_what_runs_of_process_read_alone() {
    let mut app = App::default();
    window(&mut app, "sleep 10");
    let shown = app.focused.unwrap();
    let tab = app.workspace.window(shown).unwrap().active_tab();
    assert!(app.workspace.set_tab_running(tab, true, true));
    assert!(app.workspace.set_tab_secure(tab, true));
    assert!(
        app.workspace
            .set_tab_agent(tab, Some(lokyr_plugin_sdk::host::AgentState::Done))
    );
    let mut plugins = Plugins::new();
    let read = Rc::new(RefCell::new(Vec::new()));
    for (name, capabilities) in [("acme/blind", ""), ("acme/seeing", "process-read = true\n")] {
        let manifest = Manifest::parse(&format!(
            "[plugin]\nname = \"{name}\"\nversion = \"1.0.0\"\nkind = \"luau\"\napi = \"^0.1\"\nlicense = \"MIT\"\n[capabilities]\n{capabilities}"
        ))
        .unwrap();
        let namespace = name.rsplit('/').next().unwrap().to_owned();
        let seen = Rc::clone(&read);
        let command_name = format!("{namespace}.read");
        plugins
            .add_declared(
                manifest,
                scripted(
                    move |cx| cx.register_command(command(&command_name)),
                    move |cx, _, _| {
                        let window = cx.focused_window().unwrap();
                        let info = cx.tabs(window)?.remove(0);
                        seen.borrow_mut().push((
                            (info.running, info.alternate),
                            info.secure,
                            info.agent,
                            info.activity,
                        ));
                        Ok(())
                    },
                ),
            )
            .unwrap();
    }
    assert!(plugins.activate(&mut app.context()).is_empty());
    for command in ["blind.read", "seeing.read"] {
        plugins
            .run(command, Args::new(), &mut app.context())
            .unwrap();
    }
    let done = Some(lokyr_plugin_sdk::host::AgentState::Done);
    assert_eq!(
        *read.borrow(),
        [
            ((false, false), true, None, false),
            ((true, true), true, done, false)
        ]
    );
}

#[test]
fn a_program_s_progress_reads_as_a_state_and_a_share() {
    use lokyr_plugin_sdk::host::TabProgress;
    use lokyr_plugin_sdk::view::ProgressState;
    use lokyr_vt_core::terminal::Progress;

    let read = |progress, state, value| {
        assert_eq!(
            super::tab_progress(progress),
            Some(TabProgress { state, value }),
            "{progress:?}"
        );
    };
    read(Progress::Set(40), ProgressState::Normal, Some(0.4));
    read(Progress::Set(250), ProgressState::Normal, Some(1.0));
    read(Progress::Error(None), ProgressState::Error, None);
    read(Progress::Paused(Some(10)), ProgressState::Paused, Some(0.1));
    read(Progress::Indeterminate, ProgressState::Indeterminate, None);
    assert_eq!(super::tab_progress(Progress::Remove), None);
}

#[test]
fn a_window_s_bar_place_goes_as_the_window_closes() {
    use lokyr_plugin_sdk::host::BarPlace;
    let mut app = App::default();
    let window = window(&mut app, "sleep 10");
    let mut plugins = Plugins::new();
    plugins
        .add(
            "placer",
            scripted(
                |cx| cx.register_command(command("placer.place")),
                |cx, _, _| {
                    let window = cx.focused_window().unwrap();
                    cx.set_window_tab_bar_place(window, Some(BarPlace::Title))
                },
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("placer.place", Args::new(), &mut app.context())
        .unwrap();
    assert_eq!(
        plugins.window_tab_bar_place("placer", window.into()),
        BarPlace::Title
    );
    // Closed, its place goes with it.
    assert!(app.workspace.close_window(window));
    for event in app.workspace.take_events() {
        if let crate::workspace::Event::Changed(change) = event {
            plugins.post(Event::Workspace(change));
        }
    }
    assert_eq!(
        plugins.window_tab_bar_place("placer", window.into()),
        BarPlace::default()
    );
}

/// What the test plugin of the searches heard and read: the matches told,
/// by the searches' ids, and the last read of the matches.
#[derive(Default)]
struct Searched {
    events: Vec<Event>,
    read: Option<host::Found>,
    last: Option<u64>,
    /// What `finder.show` shows: every match or not, and the current one.
    show: (bool, Option<host::SearchMatch>),
    /// The current match as `finder.current` read it last.
    current: Option<host::Current>,
}

/// A plugin of commands that search the focused pane for `needle`, for
/// `x`, for what is nowhere, end its search, and read its matches, 10 000
/// at most, which `searched` keeps.
fn finder(searched: &Rc<RefCell<Searched>>) -> Box<dyn Plugin> {
    let (seen, told) = (Rc::clone(searched), Rc::clone(searched));
    Box::new(Scripted {
        activate: Box::new(|cx| {
            for name in ["needle", "x", "none", "end", "read", "show", "current"] {
                cx.register_command(command(&format!("finder.{name}")))?;
            }
            Ok(())
        }),
        on_command: Box::new(move |cx, name, _| {
            let pane = cx.focused_pane().unwrap();
            let text = match name {
                "finder.needle" => "needle",
                "finder.x" => "x",
                "finder.none" => "nowhere-at-all",
                "finder.end" => {
                    let last = seen.borrow().last.unwrap();
                    return cx.end_search(last);
                }
                "finder.show" => {
                    let (last, (all, current)) = {
                        let seen = seen.borrow();
                        (seen.last.unwrap(), seen.show)
                    };
                    return cx.show_matches(last, all, current);
                }
                "finder.current" => {
                    let last = seen.borrow().last.unwrap();
                    let current = cx.current_match(last)?;
                    seen.borrow_mut().current = Some(current);
                    return Ok(());
                }
                _ => {
                    let last = seen.borrow().last.unwrap();
                    let found = cx.search_matches(last, 0, 10_000)?;
                    seen.borrow_mut().read = Some(found);
                    return Ok(());
                }
            };
            let query = host::SearchQuery {
                text: text.to_owned(),
                ..host::SearchQuery::default()
            };
            let id = cx.search(pane, query)?;
            seen.borrow_mut().last = Some(id);
            Ok(())
        }),
        on_event: Some(Box::new(move |_, events| {
            told.borrow_mut().events.extend(events.iter().cloned());
            Ok(())
        })),
    })
}

/// Delivers until the search `id` told its last part, for ten seconds at
/// most, and returns its parts' matches, in their order, and its count.
fn searched_until_done(
    app: &mut App,
    plugins: &mut Plugins,
    searched: &Rc<RefCell<Searched>>,
    id: u64,
) -> (Vec<Vec<host::SearchMatch>>, usize) {
    let start = Instant::now();
    loop {
        let done =
            searched.borrow().events.iter().any(
                |event| matches!(event, Event::Searched { id: of, done: true, .. } if *of == id),
            );
        if done {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(10), "not done");
        assert!(plugins.deliver(&mut app.context()).is_empty());
        std::thread::sleep(Duration::from_millis(5));
    }
    let mut parts = Vec::new();
    let mut count = 0;
    for event in &searched.borrow().events {
        if let Event::Searched {
            id: of,
            matches,
            count: so_far,
            ..
        } = event
            && *of == id
        {
            parts.push(matches.clone());
            count = *so_far;
        }
    }
    (parts, count)
}

/// The text of `pane`'s row numbered `row`, if the pane holds it still, as
/// the scrollback's limit takes the oldest rows.
fn held_row_text(app: &App, pane: PaneId, row: i64) -> Option<String> {
    let reader = app.workspace.pane(pane).unwrap().reader().unwrap();
    let terminal = reader.lock();
    terminal.row_index(row)?;
    let rows = terminal.read_rows(row, 1);
    Some(
        rows.into_iter()
            .next()
            .map(|row| row.text)
            .unwrap_or_default(),
    )
}

/// A script that prints the rows `1` to `first`, a match on every tenth,
/// then `half`, then waits for the file at `flag`, then prints the rows to
/// `last`, then `ready`.
fn printing_in_two(first: u32, last: u32, flag: &std::path::Path) -> String {
    format!(
        "seq 1 {first} | sed 's/7$/7 needle/'; echo half; \
         while [ ! -e '{flag}' ]; do sleep 0.01; done; \
         seq {} {last} | sed 's/7$/7 needle/'; echo ready; sleep 30",
        first + 1,
        flag = flag.display()
    )
}

/// A flag's path of the test `name`, which no file holds yet.
fn flag(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("lokyr-{name}-{}", std::process::id()));
    let _ = std::fs::remove_file(&path);
    path
}

#[test]
fn a_search_finds_a_part_at_a_time_from_the_view_outward_its_matches_held_through_output_after_it()
{
    let mut app = App::default();
    // 5000 rows of 500 matches, then, once the search started, 5000 more,
    // within the scrollback's limit.
    let flag = flag("search-output");
    let window = window(&mut app, &printing_in_two(5000, 10_000, &flag));
    let pane = app.workspace.active_pane(window).unwrap();
    row_once(&app, pane, 5000, "half");
    let mut plugins = Plugins::new();
    let searched = Rc::new(RefCell::new(Searched::default()));
    plugins.add("finder", finder(&searched)).unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("finder.needle", Args::new(), &mut app.context())
        .unwrap();
    std::fs::write(&flag, "").unwrap();
    let id = searched.borrow().last.unwrap();
    let (parts, count) = searched_until_done(&mut app, &mut plugins, &searched, id);
    row_once(&app, pane, 10_001, "ready");
    let _ = std::fs::remove_file(&flag);
    // Of the rows that the pane held as it started, every part's matches
    // each once.
    let all: Vec<host::SearchMatch> = parts.iter().flatten().copied().collect();
    assert_eq!(count, 500);
    assert_eq!(all.len(), count);
    let mut unique = all.clone();
    unique.sort_by_key(|found| (found.first.1, found.first.0));
    unique.dedup();
    assert_eq!(unique.len(), all.len(), "a match told twice");
    // Each on its row, through the output that came after.
    for found in &all {
        assert!(found.first.1 < 5000, "{found:?}");
        assert_eq!(found.last, (found.first.0 + 5, found.first.1));
        let text = held_row_text(&app, pane, found.first.1);
        assert!(
            text.as_deref()
                .is_some_and(|text| text.ends_with("7 needle")),
            "{found:?}: {text:?}"
        );
    }
    // The view's part first, of the rows of a screen.
    assert!(parts.len() > 1, "{} parts", parts.len());
    let rows: Vec<i64> = parts[0].iter().map(|found| found.first.1).collect();
    let span = rows.iter().max().unwrap_or(&0) - rows.iter().min().unwrap_or(&0);
    assert!(span < 24, "{rows:?}");
    // The read of its matches gives them in the rows' order, where the
    // output after left them.
    plugins
        .run("finder.read", Args::new(), &mut app.context())
        .unwrap();
    let read = searched.borrow_mut().read.take().unwrap();
    assert_eq!(read.count, count);
    assert!(read.done);
    assert_eq!(read.matches, unique);
}

#[test]
fn a_search_s_matches_of_rows_that_the_scrollback_s_limit_took_go_and_the_others_stay() {
    let mut app = App::default();
    // 10 000 rows of 1000 matches, then, once the search ended, 10 000
    // more, which take about the older half of them past the scrollback's
    // limit of 10 MiB, of some 15 000 rows of 80 columns.
    let flag = flag("search-limit");
    let window = window(&mut app, &printing_in_two(10_000, 20_000, &flag));
    let pane = app.workspace.active_pane(window).unwrap();
    row_once(&app, pane, 10_000, "half");
    let mut plugins = Plugins::new();
    let searched = Rc::new(RefCell::new(Searched::default()));
    plugins.add("finder", finder(&searched)).unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("finder.needle", Args::new(), &mut app.context())
        .unwrap();
    let id = searched.borrow().last.unwrap();
    let (_, count) = searched_until_done(&mut app, &mut plugins, &searched, id);
    assert_eq!(count, 1000);
    std::fs::write(&flag, "").unwrap();
    text_once(&app, pane, "ready");
    let _ = std::fs::remove_file(&flag);
    plugins
        .run("finder.read", Args::new(), &mut app.context())
        .unwrap();
    let read = searched.borrow_mut().read.take().unwrap();
    // The count of what it found; the matches of the rows held alone, the
    // newer, each on its row, as the rows before them went.
    assert_eq!(read.count, count);
    assert!(read.done);
    assert!(
        (1..count).contains(&read.matches.len()),
        "{} held",
        read.matches.len()
    );
    for found in &read.matches {
        let text = held_row_text(&app, pane, found.first.1);
        assert!(
            text.as_deref()
                .is_some_and(|text| text.ends_with("7 needle")),
            "{found:?}: {text:?}"
        );
    }
    // Those of every row of a match that the pane holds still, and no
    // other: the row `n - 1` of the number `n`, its `needle` after it.
    let first = {
        let reader = app.workspace.pane(pane).unwrap().reader().unwrap();
        reader.lock().row_number(0)
    };
    let held: Vec<host::SearchMatch> = (first.max(0)..10_000)
        .filter(|row| (row + 1) % 10 == 7)
        .map(|row| {
            let digits = u16::try_from((row + 1).to_string().len()).unwrap();
            host::SearchMatch {
                first: (digits + 1, row),
                last: (digits + 6, row),
            }
        })
        .collect();
    assert_eq!(read.matches, held);
}

#[test]
fn a_search_that_the_next_one_ends_stops_and_lets_its_matches_go() {
    let mut app = App::default();
    // Of five parts, which the scrollback holds.
    let window = window(&mut app, "seq 1 5000 | sed 's/$/ x/'; echo ready; sleep 30");
    let pane = app.workspace.active_pane(window).unwrap();
    row_once(&app, pane, 5000, "ready");
    let pins = |app: &App| {
        let reader = app.workspace.pane(pane).unwrap().reader().unwrap();
        reader.lock().screen_rows().pins()
    };
    let before = pins(&app);
    let mut plugins = Plugins::new();
    let searched = Rc::new(RefCell::new(Searched::default()));
    plugins.add("finder", finder(&searched)).unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    // A search of 5000 matches, then at once another, of none.
    plugins
        .run("finder.x", Args::new(), &mut app.context())
        .unwrap();
    let first = searched.borrow().last.unwrap();
    plugins
        .run("finder.none", Args::new(), &mut app.context())
        .unwrap();
    let second = searched.borrow().last.unwrap();
    let (_, count) = searched_until_done(&mut app, &mut plugins, &searched, second);
    assert_eq!(count, 0);
    // The first told nothing, and its pins went, those that its thread
    // made after its end too.
    assert!(
        !searched
            .borrow()
            .events
            .iter()
            .any(|event| matches!(event, Event::Searched { id, .. } if *id == first)),
        "the first told"
    );
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(pins(&app), before);
    // A search ended lets its matches go; a read of it is refused.
    plugins
        .run("finder.x", Args::new(), &mut app.context())
        .unwrap();
    let third = searched.borrow().last.unwrap();
    let (_, count) = searched_until_done(&mut app, &mut plugins, &searched, third);
    assert_eq!(count, 5000);
    assert_eq!(pins(&app), before + 2 * 5000);
    plugins
        .run("finder.end", Args::new(), &mut app.context())
        .unwrap();
    assert_eq!(pins(&app), before);
    let refused = plugins.run("finder.read", Args::new(), &mut app.context());
    assert_eq!(refused.unwrap_err().code, Code::NotFound);
}

#[test]
fn a_search_s_matches_stay_with_their_cells_through_output_until_the_scrollback_lets_them_go() {
    let mut app = App::default();
    let window = window(
        &mut app,
        "printf 'a needle\\nanother needle\\n'; echo ready; read x; seq 1 100; echo more; read y; seq 1 20000; echo done; sleep 30",
    );
    let pane = app.workspace.active_pane(window).unwrap();
    row_once(&app, pane, 2, "ready");
    app.workspace
        .pane(pane)
        .unwrap()
        .set_scrollback_limit(64 * 1024);
    let mut plugins = Plugins::new();
    let searched = Rc::new(RefCell::new(Searched::default()));
    plugins.add("finder", finder(&searched)).unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("finder.needle", Args::new(), &mut app.context())
        .unwrap();
    let id = searched.borrow().last.unwrap();
    let (_, count) = searched_until_done(&mut app, &mut plugins, &searched, id);
    assert_eq!(count, 2);
    let wanted = vec![
        host::SearchMatch {
            first: (2, 0),
            last: (7, 0),
        },
        host::SearchMatch {
            first: (8, 1),
            last: (13, 1),
        },
    ];
    let read = |app: &mut App, plugins: &mut Plugins| {
        plugins
            .run("finder.read", Args::new(), &mut app.context())
            .unwrap();
        searched.borrow_mut().read.take().unwrap()
    };
    assert_eq!(read(&mut app, &mut plugins).matches, wanted);
    // Output that scrolls them into the scrollback, after the echo of the
    // line: the same rows.
    app.workspace.pane(pane).unwrap().text("\n");
    row_once(&app, pane, 104, "more");
    assert_eq!(read(&mut app, &mut plugins).matches, wanted);
    // Past the scrollback's limit, they go, the count staying.
    app.workspace.pane(pane).unwrap().text("\n");
    row_once(&app, pane, 20_106, "done");
    let found = read(&mut app, &mut plugins);
    assert_eq!((found.matches, found.count), (Vec::new(), 2));
}

#[test]
fn a_search_s_matches_show_over_the_view_s_cells_and_the_view_goes_to_the_current_one() {
    let mut app = App::default();
    // A match on every tenth row, of 300.
    let window = window(
        &mut app,
        "seq 1 300 | sed 's/7$/7 needle/'; echo ready; sleep 30",
    );
    let pane = app.workspace.active_pane(window).unwrap();
    row_once(&app, pane, 300, "ready");
    let mut plugins = Plugins::new();
    let searched = Rc::new(RefCell::new(Searched::default()));
    plugins.add("finder", finder(&searched)).unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("finder.needle", Args::new(), &mut app.context())
        .unwrap();
    let id = searched.borrow().last.unwrap();
    let (parts, count) = searched_until_done(&mut app, &mut plugins, &searched, id);
    assert_eq!(count, 30);
    let highlights = |app: &App| {
        let reader = app.workspace.pane(pane).unwrap().reader().unwrap();
        let mut terminal = reader.lock();
        let mut snapshot = lokyr_vt_core::terminal::Snapshot::default();
        terminal.snapshot(&mut snapshot);
        let top = terminal.origin().top;
        let shown: Vec<(i64, u16, u16, bool)> = snapshot
            .highlights()
            .iter()
            .map(|span| {
                (
                    top + i64::from(span.row),
                    span.first,
                    span.last,
                    span.current,
                )
            })
            .collect();
        (top, shown)
    };
    // Not shown until the plugin asks.
    assert!(highlights(&app).1.is_empty());
    // `needle` of `7 needle`, on the row numbered 6, the first, current.
    let first = parts
        .iter()
        .flatten()
        .min_by_key(|found| found.first.1)
        .copied()
        .unwrap();
    assert_eq!(first.first, (2, 6));
    searched.borrow_mut().show = (true, Some(first));
    plugins
        .run("finder.show", Args::new(), &mut app.context())
        .unwrap();
    let (top, shown) = highlights(&app);
    assert_eq!(top, 6);
    let rows = i64::from(app.workspace.pane(pane).unwrap().grid().rows);
    let wanted: Vec<(i64, u16, u16, bool)> = (16..top + rows)
        .step_by(10)
        .map(|row| (row, 3, 8, false))
        .chain([(6, 2, 7, true)])
        .collect();
    assert_eq!(shown, wanted);
    // The current one alone; then none, as the search ends.
    searched.borrow_mut().show = (false, Some(first));
    plugins
        .run("finder.show", Args::new(), &mut app.context())
        .unwrap();
    assert_eq!(highlights(&app).1, [(6, 2, 7, true)]);
    plugins
        .run("finder.end", Args::new(), &mut app.context())
        .unwrap();
    assert!(highlights(&app).1.is_empty());
    // A search that is not there is refused.
    let refused = plugins.run("finder.show", Args::new(), &mut app.context());
    assert_eq!(refused.unwrap_err().code, Code::NotFound);
}

#[test]
fn the_current_match_is_read_where_a_resize_moved_it_and_its_text_still_is() {
    let mut app = App::default();
    // A match on every tenth row, of 300.
    let window = window(
        &mut app,
        "seq 1 300 | sed 's/7$/7 needle/'; echo ready; sleep 30",
    );
    let pane = app.workspace.active_pane(window).unwrap();
    row_once(&app, pane, 300, "ready");
    let mut plugins = Plugins::new();
    let searched = Rc::new(RefCell::new(Searched::default()));
    plugins.add("finder", finder(&searched)).unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("finder.needle", Args::new(), &mut app.context())
        .unwrap();
    let id = searched.borrow().last.unwrap();
    let (parts, count) = searched_until_done(&mut app, &mut plugins, &searched, id);
    assert_eq!(count, 30);
    let mut told: Vec<host::SearchMatch> = parts.into_iter().flatten().collect();
    told.sort_by_key(|found| (found.first.1, found.first.0));
    // The match of `7 needle` on the row numbered 6, the first, current.
    searched.borrow_mut().show = (true, Some(told[0]));
    plugins
        .run("finder.show", Args::new(), &mut app.context())
        .unwrap();
    let current = |app: &mut App, plugins: &mut Plugins| {
        plugins
            .run("finder.current", Args::new(), &mut app.context())
            .unwrap();
        searched.borrow().current.unwrap()
    };
    assert_eq!(
        current(&mut app, &mut plugins),
        host::Current {
            current: Some((0, told[0])),
            kept: 30
        }
    );
    // A resize that renumbers the rows: the current match read where its
    // text is now, its place the same.
    let grid = Grid {
        columns: 60,
        rows: 20,
        cell_width: 8,
        cell_height: 16,
    };
    let found = app.workspace.pane_mut(pane).unwrap();
    found.resize(grid, Instant::now());
    found.end_live_resize(Instant::now() + Duration::from_secs(1));
    let now = current(&mut app, &mut plugins);
    let (place, moved) = now.current.unwrap();
    assert_eq!((place, now.kept), (0, 30));
    assert_ne!(moved, told[0]);
    let text = held_row_text(&app, pane, moved.first.1).unwrap();
    let chars: Vec<char> = text.chars().collect();
    let first = usize::from(moved.first.0);
    let word: String = chars[first..first + 6].iter().collect();
    assert_eq!(word, "needle");
    plugins
        .run("finder.end", Args::new(), &mut app.context())
        .unwrap();
    // Of a search that is not there, refused.
    let refused = plugins.run("finder.current", Args::new(), &mut app.context());
    assert_eq!(refused.unwrap_err().code, Code::NotFound);
}

#[test]
fn a_shown_search_s_matches_stay_until_the_next_one_of_the_pane_shows_its_own() {
    let mut app = App::default();
    let window = window(
        &mut app,
        "seq 1 300 | sed 's/7$/7 needle/'; echo ready; sleep 30",
    );
    let pane = app.workspace.active_pane(window).unwrap();
    row_once(&app, pane, 300, "ready");
    let reader = app.workspace.pane(pane).unwrap().reader().unwrap();
    let before = reader.lock().screen_rows().pins();
    let highlighted = || {
        let mut snapshot = lokyr_vt_core::terminal::Snapshot::default();
        reader.lock().snapshot(&mut snapshot);
        snapshot.highlights().len()
    };
    let mut plugins = Plugins::new();
    let searched = Rc::new(RefCell::new(Searched::default()));
    plugins.add("finder", finder(&searched)).unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    let run = |app: &mut App, plugins: &mut Plugins, name: &str| {
        plugins.run(name, Args::new(), &mut app.context()).unwrap();
    };
    // The needles shown, those of the view's rows.
    run(&mut app, &mut plugins, "finder.needle");
    let first = searched.borrow().last.unwrap();
    searched_until_done(&mut app, &mut plugins, &searched, first);
    searched.borrow_mut().show = (true, None);
    run(&mut app, &mut plugins, "finder.show");
    let shown = highlighted();
    assert!(shown > 0);
    // Another search of the pane, of none: the needles stay, through its
    // end, until it shows its own.
    run(&mut app, &mut plugins, "finder.none");
    assert_eq!(highlighted(), shown);
    let second = searched.borrow().last.unwrap();
    searched_until_done(&mut app, &mut plugins, &searched, second);
    assert_eq!(highlighted(), shown);
    run(&mut app, &mut plugins, "finder.show");
    assert_eq!(highlighted(), 0);
    // Shown, then another, then its end: none, and every pin let go.
    run(&mut app, &mut plugins, "finder.needle");
    let third = searched.borrow().last.unwrap();
    searched_until_done(&mut app, &mut plugins, &searched, third);
    run(&mut app, &mut plugins, "finder.show");
    run(&mut app, &mut plugins, "finder.none");
    assert_eq!(highlighted(), shown);
    run(&mut app, &mut plugins, "finder.end");
    assert_eq!(highlighted(), 0);
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(reader.lock().screen_rows().pins(), before);
}

#[test]
fn output_leaves_the_view_at_the_current_match_until_the_search_ends() {
    let mut app = App::default();
    // Matches, then, a second after, output on and on.
    let window = window(
        &mut app,
        "seq 1 300 | sed 's/7$/7 needle/'; echo ready; sleep 1; while true; do echo tick; sleep 0.005; done",
    );
    let pane = app.workspace.active_pane(window).unwrap();
    row_once(&app, pane, 300, "ready");
    app.workspace.pane(pane).unwrap().set_scroll_on_output(true);
    let mut plugins = Plugins::new();
    let searched = Rc::new(RefCell::new(Searched::default()));
    plugins.add("finder", finder(&searched)).unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("finder.needle", Args::new(), &mut app.context())
        .unwrap();
    let id = searched.borrow().last.unwrap();
    let (parts, _) = searched_until_done(&mut app, &mut plugins, &searched, id);
    let first = parts
        .iter()
        .flatten()
        .min_by_key(|found| found.first.1)
        .copied()
        .unwrap();
    searched.borrow_mut().show = (true, Some(first));
    plugins
        .run("finder.show", Args::new(), &mut app.context())
        .unwrap();
    let reader = app.workspace.pane(pane).unwrap().reader().unwrap();
    let top = || reader.lock().origin().top;
    assert_eq!(top(), 6);
    // The program prints on: the view stays at the match.
    row_once(&app, pane, 310, "tick");
    assert_eq!(top(), 6);
    // The search ended, the next output brings the view back to the screen.
    plugins
        .run("finder.end", Args::new(), &mut app.context())
        .unwrap();
    let start = Instant::now();
    while top() == 6 {
        assert!(start.elapsed() < Duration::from_secs(10), "the view stays");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(reader.lock().viewport_offset(), 0);
}

/// The lines of the measure of the search, of 80 columns, a match of
/// `needle` on one in a hundred.
const MEASURED_LINES: i64 = 100_000;

/// The time that a search of a literal takes over 100 000 lines of 80
/// columns in the scrollback, through the search service, its plugin and
/// its thread, as the find bar asks for it: until its first match shows,
/// and until it counted them all. Of a release build alone, as `just
/// bench-search` runs it: the median of five searches, which the search's
/// budget bounds at 150 ms.
#[test]
#[ignore = "a measure, of a release build: just bench-search"]
fn the_search_of_100k_lines_shows_its_first_match_and_counts_them_all_within_150_ms() {
    let mut app = App::default();
    let window = window(
        &mut app,
        "read x; awk 'BEGIN { for (i = 0; i < 100000; i++) printf \"%06d the quick brown fox %s %046d\\n\", i, (i % 100 == 7 ? \"needle\" : \"nobody\"), i }'; echo done; sleep 60",
    );
    let pane = app.workspace.active_pane(window).unwrap();
    let shown = app.workspace.pane(pane).unwrap();
    shown.set_scrollback_limit(256 << 20);
    shown.text("\n");
    let start = Instant::now();
    while app
        .workspace
        .pane(pane)
        .unwrap()
        .line_text(MEASURED_LINES + 1, 0)
        .is_none_or(|(line, _)| line.trim_end() != "done")
    {
        assert!(start.elapsed() < Duration::from_secs(60), "no `done`");
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut plugins = Plugins::new();
    let searched = Rc::new(RefCell::new(Searched::default()));
    plugins.add("finder", finder(&searched)).unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    let (mut first, mut all) = (Vec::new(), Vec::new());
    for _ in 0..5 {
        searched.borrow_mut().events.clear();
        let start = Instant::now();
        plugins
            .run("finder.needle", Args::new(), &mut app.context())
            .unwrap();
        let id = searched.borrow().last.unwrap();
        let mut shown_at = None;
        let count = loop {
            assert!(start.elapsed() < Duration::from_secs(10), "not done");
            assert!(plugins.deliver(&mut app.context()).is_empty());
            let (found, done) = {
                let told = searched.borrow();
                let mut found = None;
                let mut done = None;
                for event in &told.events {
                    if let Event::Searched {
                        id: of,
                        matches,
                        count,
                        done: ended,
                        ..
                    } = event
                        && *of == id
                    {
                        found = found.or(matches.last().copied());
                        if *ended {
                            done = Some(*count);
                        }
                    }
                }
                (found, done)
            };
            // Its first match shown, as the find bar shows it.
            if shown_at.is_none()
                && let Some(found) = found
            {
                searched.borrow_mut().show = (true, Some(found));
                plugins
                    .run("finder.show", Args::new(), &mut app.context())
                    .unwrap();
                shown_at = Some(start.elapsed());
            }
            if let Some(count) = done {
                all.push(start.elapsed());
                break count;
            }
            std::thread::sleep(Duration::from_micros(200));
        };
        assert_eq!(count, 1000);
        first.push(shown_at.expect("a match shown"));
    }
    first.sort();
    all.sort();
    let (first, all) = (first[2], all[2]);
    println!(
        "search of {MEASURED_LINES} lines of 80 columns: first match shown in {:.1} ms, all counted in {:.1} ms",
        first.as_secs_f64() * 1000.0,
        all.as_secs_f64() * 1000.0
    );
    assert!(all <= Duration::from_millis(150), "{all:?}");
}

#[test]
fn a_plugin_selects_cells_and_the_view_shows_the_first() {
    let mut app = App::default();
    let window = window(&mut app, "seq 1 200; echo ready; sleep 30");
    let pane = app.workspace.active_pane(window).unwrap();
    row_once(&app, pane, 200, "ready");
    let mut plugins = Plugins::new();
    plugins
        .add(
            "selector",
            scripted(
                |cx| cx.register_command(command("selector.select")),
                |cx, _, _| {
                    let pane = cx.focused_pane().unwrap();
                    // `11`, on the row numbered 10, in the scrollback.
                    cx.select_cells(pane, (0, 10), (1, 10))
                },
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("selector.select", Args::new(), &mut app.context())
        .unwrap();
    let shown = app.workspace.pane(pane).unwrap();
    assert_eq!(shown.selection_text().as_deref(), Some("11"));
    let top = shown.reader().unwrap().lock().origin().top;
    assert_eq!(top, 10);
}

#[test]
fn a_call_s_writes_are_bounded_as_they_wait_for_its_end() {
    let mut app = App::default();
    window(&mut app, "sleep 10");
    let mut plugins = Plugins::new();
    let codes = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&codes);
    plugins
        .add(
            "writer",
            scripted(
                |cx| cx.register_command(command("writer.go")),
                move |cx, _, _| {
                    let pane = cx.focused_pane().unwrap();
                    let mut seen = seen.borrow_mut();
                    for _ in 0..=MOST_WRITES {
                        seen.push(
                            cx.scroll(pane, host::Scroll::Top)
                                .err()
                                .map(|error| error.code),
                        );
                    }
                    Ok(())
                },
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("writer.go", Args::new(), &mut app.context())
        .unwrap();
    let codes = codes.borrow();
    assert!(codes[..MOST_WRITES].iter().all(Option::is_none));
    assert_eq!(codes[MOST_WRITES], Some(Code::Failed));
}

/// A plugin whose `leaver.mark` decorates a cell, marks the gutter, holds
/// the cursor, searches and reads its pane, and whose `leaver.boom`
/// panics.
fn leaver() -> Box<dyn Plugin> {
    scripted(
        |cx| {
            cx.register_command(command("leaver.mark"))?;
            cx.register_command(command("leaver.boom"))
        },
        |cx, name, _| {
            if name == "leaver.boom" {
                panic!("on purpose");
            }
            let pane = cx.focused_pane().unwrap();
            let look = host::DecorationLook {
                underline: Some(host::Underline::Single),
                underline_color: None,
                background: None,
                pointing: false,
            };
            cx.decorate(pane, (0, 0), (2, 0), look)?;
            cx.mark_gutter(pane, 0, (255, 0, 0))?;
            cx.show_cursor(pane, Some((1, 0)))?;
            cx.search(
                pane,
                host::SearchQuery {
                    text: "1".to_owned(),
                    ..host::SearchQuery::default()
                },
            )?;
            cx.read_lines(pane, 0, 3).map(drop)
        },
    )
}

#[test]
fn a_plugin_that_panics_leaves_nothing_of_its_own_in_the_panes() {
    let mut app = App::default();
    let window = window(&mut app, "seq 1 3; echo ready; sleep 30");
    let pane = app.workspace.active_pane(window).unwrap();
    row_once(&app, pane, 3, "ready");
    let mut plugins = Plugins::new();
    plugins.add("leaver", leaver()).unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("leaver.mark", Args::new(), &mut app.context())
        .unwrap();
    let left = |app: &mut App, plugins: &Plugins| {
        let found = app.workspace.pane_mut(pane).unwrap();
        found.update();
        let snapshot = found.snapshot();
        let terminal = found.terminal().unwrap();
        (
            snapshot.decorations().len(),
            snapshot
                .gutter()
                .iter()
                .filter(|mark| matches!(mark.kind, GutterKind::Laid(_)))
                .count(),
            terminal.held_cursor().is_some(),
            plugins.entries[0].state.searches.len(),
            plugins.out.reads.len(),
        )
    };
    assert_eq!(left(&mut app, &plugins), (1, 1, true, 1, 1));
    let fault = plugins
        .run("leaver.boom", Args::new(), &mut app.context())
        .unwrap_err();
    assert_eq!(fault.code, Code::Fault);
    assert_eq!(plugins.status("leaver"), Some(Status::Disabled));
    assert_eq!(left(&mut app, &plugins), (0, 0, false, 0, 0));
}

#[test]
fn a_call_that_fails_after_its_writes_leaves_the_workspace_as_it_was() {
    let mut app = App::default();
    let window = window(&mut app, "sleep 30");
    let tab = app.workspace.window(window).unwrap().tabs()[0];
    let mut plugins = Plugins::new();
    plugins
        .add(
            "failer",
            scripted(
                |cx| cx.register_command(command("failer.go")),
                |cx, _, _| {
                    let window = cx.focused_window().unwrap();
                    let tab = cx.tabs(window).unwrap()[0].id;
                    cx.set_tab_title(tab, Some("written"))?;
                    cx.new_tab(window, None)?;
                    cx.open("https://lokyr.dev")?;
                    Err(Error::new(Code::Failed, "after its writes"))
                },
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    let failed = plugins
        .run("failer.go", Args::new(), &mut app.context())
        .unwrap_err();
    assert_eq!(failed.code, Code::Failed);
    assert_eq!(app.workspace.tab(tab).unwrap().own_title(), None);
    assert_eq!(app.workspace.window(window).unwrap().tabs().len(), 1);
    assert!(plugins.take_requests().is_empty());
    // An activation that fails after a request leaves none either.
    let mut started = Plugins::new();
    started
        .add(
            "starter",
            scripted(
                |cx| {
                    cx.open("https://lokyr.dev")?;
                    Err(Error::new(Code::Failed, "after its request"))
                },
                |_, _, _| Ok(()),
            ),
        )
        .unwrap();
    assert_eq!(started.activate(&mut app.context()).len(), 1);
    assert!(started.take_requests().is_empty());
}

/// A plugin whose `applies` tries to register a command and bind a key,
/// as `tried` keeps their errors' codes.
struct Declarer(Rc<RefCell<Vec<Option<Code>>>>);

impl Plugin for Declarer {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        cx.register_command(command("declarer.go"))
    }

    fn applies(&mut self, cx: &mut Cx<'_>, _: &str, _: &Args) -> bool {
        let code = |done: Result<(), Error>| done.err().map(|error| error.code);
        let mut tried = self.0.borrow_mut();
        tried.push(code(cx.register_command(command("declarer.more"))));
        tried.push(code(cx.bind(binding("ctrl+l", "declarer.go", None))));
        true
    }
}

#[test]
fn a_look_declares_nothing() {
    let mut app = App::default();
    window(&mut app, "sleep 30");
    let tried = Rc::new(RefCell::new(Vec::new()));
    let mut plugins = Plugins::new();
    plugins
        .add("declarer", Box::new(Declarer(Rc::clone(&tried))))
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert!(
        plugins
            .applies("declarer.go", &Args::new(), &mut app.context())
            .unwrap()
    );
    assert_eq!(*tried.borrow(), [Some(Code::Failed), Some(Code::Failed)]);
    assert!(app.registry.get("declarer.more").is_none());
    assert_eq!(plugins.bindings().count(), 0);
}

#[test]
fn a_state_file_past_the_most_of_a_state_is_read_no_further_and_starts_anew() {
    let state = std::env::temp_dir().join(format!("lokyr-plugin-big-state-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    std::fs::create_dir_all(state.join("plugins")).unwrap();
    // A string of JSON one byte past the most.
    let big = format!("\"{}\"", "x".repeat(MOST_STATE - 1));
    std::fs::write(state.join("plugins/stater.json"), &big).unwrap();
    let mut app = App::default();
    window(&mut app, "sleep 10");
    let read = Rc::new(RefCell::new(None));
    let seen = Rc::clone(&read);
    let mut plugins = Plugins::new();
    plugins
        .add(
            "stater",
            scripted(
                |cx| cx.register_command(command("stater.go")),
                move |cx, _, _| {
                    *seen.borrow_mut() = Some(cx.state()?);
                    Ok(())
                },
            ),
        )
        .unwrap();
    plugins.set_state_directory(Some(state.clone()));
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("stater.go", Args::new(), &mut app.context())
        .unwrap();
    assert_eq!(*read.borrow(), Some(lokyr_plugin_sdk::message::Data::Nil));
    let faults = plugins.take_faults();
    assert!(
        faults
            .iter()
            .any(|fault| fault.message.contains("more than")),
        "{faults:?}"
    );
    // One within it reads.
    let fits = format!("\"{}\"", "x".repeat(MOST_STATE - 2));
    assert_eq!(
        read_capped(&state.join("x.json"), 4).unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
    std::fs::write(state.join("fits.json"), &fits).unwrap();
    assert_eq!(
        read_capped(&state.join("fits.json"), MOST_STATE)
            .unwrap()
            .map(|bytes| bytes.len()),
        Some(MOST_STATE)
    );
    let _ = std::fs::remove_dir_all(&state);
}

#[test]
fn a_window_s_point_of_no_number_is_refused() {
    let mut app = App::default();
    window(&mut app, "sleep 30");
    let codes = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&codes);
    let mut plugins = Plugins::new();
    plugins
        .add(
            "mover",
            scripted(
                |cx| cx.register_command(command("mover.go")),
                move |cx, _, _| {
                    let window = cx.focused_window().unwrap();
                    let tab = cx.tabs(window).unwrap()[0].id;
                    let mut seen = seen.borrow_mut();
                    for x in [f64::NAN, f64::INFINITY] {
                        let at = host::ScreenPoint { x, y: 10.0 };
                        let moved = cx.tab_to_new_window_at(tab, at);
                        seen.push(moved.err().map(|error| (error.code, error.message)));
                    }
                    Ok(())
                },
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("mover.go", Args::new(), &mut app.context())
        .unwrap();
    let codes = codes.borrow();
    assert!(
        codes
            .iter()
            .all(|code| code.as_ref().is_some_and(|(code, message)| {
                *code == Code::WrongKind && message.contains("finite")
            })),
        "{codes:?}"
    );
    assert!(plugins.take_requests().is_empty());
}

#[test]
fn a_plugin_of_packages_hears_the_plugins_change_and_another_cannot_subscribe() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    let log = Rc::new(RefCell::new(Vec::new()));
    plugins
        .add("manager", listening(&["plugins-changed"], &log))
        .unwrap();
    plugins
        .add(
            "crasher",
            scripted(
                |cx| cx.register_command(command("crasher.go")),
                |_, _, _| panic!("a fault of the test's"),
            ),
        )
        .unwrap();
    // A plugin of a manifest of no `packages` is refused the subscription.
    let manifest = Manifest::parse(concat!(
        "[plugin]\nname = \"acme/nosy\"\nversion = \"1.0.0\"\nkind = \"luau\"\n",
        "api = \"^0.1\"\nlicense = \"MIT\"\n",
    ))
    .unwrap();
    let refused = Rc::new(RefCell::new(None));
    let told = Rc::clone(&refused);
    plugins
        .add_declared(
            manifest,
            scripted(
                move |cx| {
                    *told.borrow_mut() = cx
                        .subscribe(&["plugins-changed"])
                        .err()
                        .map(|error| error.code);
                    Ok(())
                },
                |_, _, _| Ok(()),
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(*refused.borrow(), Some(Code::MissingCapability));
    // The first list tells nothing; a fault changes it.
    let _ = plugins.infos();
    assert!(plugins.deliver(&mut app.context()).is_empty());
    assert!(log.borrow().is_empty());
    let _ = plugins.infos();
    assert!(!plugins.has_events());
    let _ = plugins.run("crasher.go", Args::new(), &mut app.context());
    let _ = plugins.infos();
    assert!(plugins.deliver(&mut app.context()).is_empty());
    assert_eq!(*log.borrow(), ["plugins-changed"]);
}
