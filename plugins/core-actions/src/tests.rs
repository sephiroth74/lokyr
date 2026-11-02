//! The plugin against a host of the tests', which records what it asks.

use lokyr_plugin_sdk::host::{PaneId, WindowId};
use lokyr_plugin_sdk::message::{Call, Request, Response};

use super::*;

/// A host that records the plugin's calls, with a pane, a selection and a
/// clipboard of the test's own.
#[derive(Default)]
struct Host {
    pane: Option<PaneId>,
    selection: Option<String>,
    clipboard: Option<String>,
    commands: Vec<Command>,
    bindings: Vec<Binding>,
    calls: Vec<String>,
    font: f32,
}

impl lokyr_plugin_sdk::host::Host for Host {
    fn handle(&mut self, call: Call) -> Response {
        assert_eq!(call.caller, "core-actions");
        let mut done = |call: String| {
            self.calls.push(call);
            Response::Done
        };
        match call.request {
            Request::RegisterCommand(command) => {
                self.commands.push(command);
                Response::Done
            }
            Request::Bind(binding) => {
                self.bindings.push(binding);
                Response::Done
            }
            Request::ClipboardText => Response::Text(self.clipboard.clone()),
            Request::SetClipboardText(text) => {
                self.clipboard = Some(text);
                Response::Done
            }
            Request::FocusedWindow => Response::Window(self.pane.map(|_| WindowId(1))),
            Request::FocusedPane => Response::Pane(self.pane),
            Request::SelectionText(_) => Response::Text(self.selection.clone()),
            Request::SelectAll(pane) => done(format!("select all {}", pane.0)),
            Request::ClearScrollback(pane) => done(format!("clear scrollback {}", pane.0)),
            Request::Clear(pane) => done(format!("clear {}", pane.0)),
            Request::Paste { pane, text } => done(format!("paste {} {text:?}", pane.0)),
            Request::SendText { pane, text } => done(format!("send {} {text:?}", pane.0)),
            Request::Scroll { pane, scroll } => done(format!("scroll {} {scroll:?}", pane.0)),
            Request::FontSize => Response::FontSize(self.font),
            Request::SetFontSize(size) => done(format!("font {size:?}")),
            other => panic!("core-actions asked {other:?}"),
        }
    }
}

/// Runs `name` on `host`.
fn run(host: &mut Host, name: &str) -> Result<(), Error> {
    CoreActions.on_command(&mut Cx::new(host, "core-actions"), name, Args::new())
}

#[test]
fn the_commands_register_with_their_keys() {
    let mut host = Host::default();
    CoreActions
        .activate(&mut Cx::new(&mut host, "core-actions"))
        .unwrap();
    let registered: Vec<(&str, Category, Vec<String>)> = host
        .commands
        .iter()
        .map(|command| {
            assert!(command.params.is_empty() || command.name == "pane.send-text");
            let capabilities = command.capabilities.iter().map(ToString::to_string);
            (
                command.name.as_str(),
                command.category,
                capabilities.collect(),
            )
        })
        .collect();
    let names = |names: &[&str]| -> Vec<String> { names.iter().map(|&n| n.to_owned()).collect() };
    let clipboard = Category::Clipboard;
    let view = |name| (name, Category::View, Vec::new());
    assert_eq!(
        registered,
        [
            (
                "pane.copy",
                clipboard,
                names(&["terminal-read", "clipboard-write"])
            ),
            (
                "pane.paste",
                clipboard,
                names(&["clipboard-read", "pty-write"])
            ),
            ("pane.select-all", Category::Edit, Vec::new()),
            ("pane.clear", Category::Edit, Vec::new()),
            view("font.bigger"),
            view("font.smaller"),
            view("font.reset"),
            view("pane.scroll-line-up"),
            view("pane.scroll-line-down"),
            view("pane.scroll-page-up"),
            view("pane.scroll-page-down"),
            view("pane.scroll-top"),
            view("pane.scroll-bottom"),
            ("pane.send-text", Category::Input, names(&["pty-write"])),
        ]
    );
    // Copying takes a selection; the others run in any context.
    let whens: Vec<(&str, Option<&str>)> = host
        .commands
        .iter()
        .filter(|command| command.when.is_some())
        .map(|command| (command.name.as_str(), command.when.as_deref()))
        .collect();
    assert_eq!(whens, [("pane.copy", Some("selection"))]);
    // Sending text takes its text, and has no keys of its own.
    let send = host.commands.last().unwrap();
    assert_eq!(send.params.len(), 1);
    assert_eq!(
        (
            send.params[0].name.as_str(),
            send.params[0].kind,
            &send.params[0].default
        ),
        ("text", lokyr_plugin_sdk::command::Kind::String, &None)
    );
    assert!(
        host.bindings
            .iter()
            .all(|binding| binding.command != "pane.send-text")
    );
    let bound: Vec<(&str, &str)> = host
        .bindings
        .iter()
        .map(|binding| {
            assert_eq!((binding.args.len(), &binding.when), (0, &None));
            (binding.keys.as_str(), binding.command.as_str())
        })
        .collect();
    assert_eq!(
        bound,
        [
            ("cmd+c", "pane.copy"),
            ("cmd+v", "pane.paste"),
            ("cmd+a", "pane.select-all"),
            ("cmd+k", "pane.clear"),
            ("cmd+=", "font.bigger"),
            ("cmd+shift+=", "font.bigger"),
            ("cmd++", "font.bigger"),
            ("cmd+-", "font.smaller"),
            ("cmd+0", "font.reset"),
            ("cmd+up", "pane.scroll-line-up"),
            ("cmd+down", "pane.scroll-line-down"),
            ("cmd+pageup", "pane.scroll-page-up"),
            ("shift+pageup", "pane.scroll-page-up"),
            ("cmd+pagedown", "pane.scroll-page-down"),
            ("shift+pagedown", "pane.scroll-page-down"),
            ("cmd+home", "pane.scroll-top"),
            ("cmd+end", "pane.scroll-bottom"),
        ]
    );
}

#[test]
fn copy_and_paste_go_through_the_clipboard() {
    let mut host = Host {
        pane: Some(PaneId(7)),
        selection: Some("ls -la".to_owned()),
        ..Host::default()
    };
    run(&mut host, COPY).unwrap();
    assert_eq!(host.clipboard.as_deref(), Some("ls -la"));
    run(&mut host, PASTE).unwrap();
    assert_eq!(host.calls, ["paste 7 \"ls -la\""]);
    // Without a selection, or with an empty one, the clipboard keeps its
    // text; an empty clipboard pastes nothing.
    for selection in [None, Some(String::new())] {
        host.selection = selection;
        run(&mut host, COPY).unwrap();
        assert_eq!(host.clipboard.as_deref(), Some("ls -la"));
    }
    host.calls.clear();
    for clipboard in [None, Some(String::new())] {
        host.clipboard = clipboard;
        run(&mut host, PASTE).unwrap();
    }
    assert!(host.calls.is_empty());
}

#[test]
fn select_all_and_clear_act_on_the_focused_pane() {
    let mut host = Host {
        pane: Some(PaneId(3)),
        ..Host::default()
    };
    run(&mut host, SELECT_ALL).unwrap();
    run(&mut host, CLEAR).unwrap();
    assert_eq!(host.calls, ["select all 3", "clear 3"]);
    // Without a focused pane, nothing.
    let mut host = Host::default();
    for name in [COPY, PASTE, SELECT_ALL, CLEAR] {
        run(&mut host, name).unwrap();
    }
    assert!(host.calls.is_empty() && host.clipboard.is_none());
    let mut host = Host {
        pane: Some(PaneId(3)),
        ..Host::default()
    };
    assert_eq!(
        run(&mut host, "pane.other").unwrap_err().code,
        Code::NotFound
    );
}

#[test]
fn the_font_grows_and_shrinks_a_point_within_its_sizes() {
    let mut host = Host {
        font: 13.0,
        ..Host::default()
    };
    for name in [FONT_BIGGER, FONT_SMALLER, FONT_RESET] {
        run(&mut host, name).unwrap();
    }
    // A size between points goes to the next whole one.
    host.font = 13.5;
    run(&mut host, FONT_BIGGER).unwrap();
    run(&mut host, FONT_SMALLER).unwrap();
    // At the ends, nothing more; past them, back within.
    host.font = 200.0;
    run(&mut host, FONT_BIGGER).unwrap();
    host.font = 4.0;
    run(&mut host, FONT_SMALLER).unwrap();
    host.font = 199.5;
    run(&mut host, FONT_BIGGER).unwrap();
    host.font = 4.5;
    run(&mut host, FONT_SMALLER).unwrap();
    assert_eq!(
        host.calls,
        [
            "font Some(14.0)",
            "font Some(12.0)",
            "font None",
            "font Some(14.0)",
            "font Some(13.0)",
            "font Some(200.0)",
            "font Some(4.0)",
        ]
    );
}

#[test]
fn the_view_scrolls_by_lines_pages_and_to_its_ends() {
    let mut host = Host {
        pane: Some(PaneId(5)),
        ..Host::default()
    };
    for name in [
        SCROLL_LINE_UP,
        SCROLL_LINE_DOWN,
        SCROLL_PAGE_UP,
        SCROLL_PAGE_DOWN,
        SCROLL_TOP,
        SCROLL_BOTTOM,
    ] {
        run(&mut host, name).unwrap();
    }
    assert_eq!(
        host.calls,
        [
            "scroll 5 Up(1)",
            "scroll 5 Down(1)",
            "scroll 5 PageUp",
            "scroll 5 PageDown",
            "scroll 5 Top",
            "scroll 5 Bottom",
        ]
    );
    // Without a focused pane, nothing.
    let mut host = Host::default();
    run(&mut host, SCROLL_TOP).unwrap();
    assert!(host.calls.is_empty());
}

#[test]
fn sending_text_sends_it_as_it_is() {
    let mut host = Host {
        pane: Some(PaneId(9)),
        ..Host::default()
    };
    let args = Args::from([("text".to_owned(), Value::String("\u{17}\u{1b}b".to_owned()))]);
    CoreActions
        .on_command(
            &mut Cx::new(&mut host, "core-actions"),
            SEND_TEXT,
            args.clone(),
        )
        .unwrap();
    assert_eq!(host.calls, ["send 9 \"\\u{17}\\u{1b}b\""]);
    // Without a focused pane, nothing.
    let mut host = Host::default();
    CoreActions
        .on_command(&mut Cx::new(&mut host, "core-actions"), SEND_TEXT, args)
        .unwrap();
    assert!(host.calls.is_empty());
}
