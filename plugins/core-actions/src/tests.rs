//! The plugin against the SDK's fake host, which records what it does.

use lokyr_plugin_sdk::fake::FakeHost;
use lokyr_plugin_sdk::host::PaneId;
use lokyr_plugin_sdk::message::Request;

use super::*;

/// The plugin's name, as its calls give it.
const NAME: &str = "lokyr/core-actions";

/// A host of the plugin's, whose focused pane, if any, is `pane`.
fn host(pane: Option<u64>) -> FakeHost {
    let mut host = FakeHost::new(NAME);
    host.focus(pane.map(PaneId));
    host
}

/// Runs `name` on `host`.
fn run(host: &mut FakeHost, name: &str) -> Result<(), Error> {
    host.command(&mut CoreActions, name, Args::new())
}

#[test]
fn the_commands_register_with_their_keys() {
    let mut host = host(None);
    host.activate(&mut CoreActions).unwrap();
    let registered: Vec<(&str, Category, Vec<String>)> = host
        .commands()
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
        .commands()
        .iter()
        .filter(|command| command.when.is_some())
        .map(|command| (command.name.as_str(), command.when.as_deref()))
        .collect();
    assert_eq!(whens, [("pane.copy", Some("selection"))]);
    // Sending text takes its text, and has no keys of its own.
    let send = host.commands().last().unwrap();
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
        host.bindings()
            .iter()
            .all(|binding| binding.command != "pane.send-text")
    );
    let bound: Vec<(&str, &str)> = host
        .bindings()
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
    let mut host = host(Some(7));
    host.pane(PaneId(7)).selection = Some("ls -la".to_owned());
    run(&mut host, COPY).unwrap();
    assert_eq!(host.clipboard(), Some("ls -la"));
    run(&mut host, PASTE).unwrap();
    assert_eq!(
        host.take_done(),
        [
            Request::SetClipboardText("ls -la".to_owned()),
            Request::Paste {
                pane: PaneId(7),
                text: "ls -la".to_owned(),
            },
        ]
    );
    // Without a selection, or with an empty one, the clipboard keeps its
    // text; an empty clipboard pastes nothing.
    for selection in [None, Some(String::new())] {
        host.pane(PaneId(7)).selection = selection;
        run(&mut host, COPY).unwrap();
        assert_eq!(host.clipboard(), Some("ls -la"));
    }
    for clipboard in [None, Some("")] {
        host.set_clipboard(clipboard);
        run(&mut host, PASTE).unwrap();
    }
    assert!(host.take_done().is_empty());
}

#[test]
fn select_all_and_clear_act_on_the_focused_pane() {
    let mut host = host(Some(3));
    run(&mut host, SELECT_ALL).unwrap();
    run(&mut host, CLEAR).unwrap();
    assert_eq!(
        host.take_done(),
        [Request::SelectAll(PaneId(3)), Request::Clear(PaneId(3))]
    );
    // Without a focused pane, nothing.
    let mut host = self::host(None);
    host.set_clipboard(Some("x"));
    for name in [COPY, PASTE, SELECT_ALL, CLEAR] {
        run(&mut host, name).unwrap();
    }
    assert!(host.take_done().is_empty());
    let mut host = self::host(Some(3));
    assert_eq!(
        run(&mut host, "pane.other").unwrap_err().code,
        Code::NotFound
    );
}

#[test]
fn the_font_grows_and_shrinks_a_point_within_its_sizes() {
    let mut host = host(None);
    host.set_font_size(13.0);
    for name in [FONT_BIGGER, FONT_SMALLER, FONT_RESET] {
        run(&mut host, name).unwrap();
    }
    // A size between points goes to the next whole one.
    host.set_font_size(13.5);
    run(&mut host, FONT_BIGGER).unwrap();
    run(&mut host, FONT_SMALLER).unwrap();
    // At the ends, nothing more; past them, back within.
    for (size, name) in [
        (200.0, FONT_BIGGER),
        (4.0, FONT_SMALLER),
        (199.5, FONT_BIGGER),
        (4.5, FONT_SMALLER),
    ] {
        host.set_font_size(size);
        run(&mut host, name).unwrap();
    }
    let sizes = [14.0, 12.0, -1.0, 14.0, 13.0, 200.0, 4.0]
        .map(|size: f32| Request::SetFontSize((size > 0.0).then_some(size)));
    assert_eq!(host.take_done(), sizes);
}

#[test]
fn the_view_scrolls_by_lines_pages_and_to_its_ends() {
    let mut host = host(Some(5));
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
    let scrolls = [
        Scroll::Up(1),
        Scroll::Down(1),
        Scroll::PageUp,
        Scroll::PageDown,
        Scroll::Top,
        Scroll::Bottom,
    ]
    .map(|scroll| Request::Scroll {
        pane: PaneId(5),
        scroll,
    });
    assert_eq!(host.take_done(), scrolls);
    // Without a focused pane, nothing.
    let mut host = self::host(None);
    run(&mut host, SCROLL_TOP).unwrap();
    assert!(host.take_done().is_empty());
}

#[test]
fn sending_text_sends_it_as_it_is() {
    let mut host = host(Some(9));
    let args = Args::from([("text".to_owned(), Value::String("\u{17}\u{1b}b".to_owned()))]);
    host.command(&mut CoreActions, SEND_TEXT, args.clone())
        .unwrap();
    assert_eq!(
        host.take_done(),
        [Request::SendText {
            pane: PaneId(9),
            text: "\u{17}\u{1b}b".to_owned(),
        }]
    );
    // Without a focused pane, nothing.
    let mut host = self::host(None);
    host.command(&mut CoreActions, SEND_TEXT, args).unwrap();
    assert!(host.take_done().is_empty());
}
