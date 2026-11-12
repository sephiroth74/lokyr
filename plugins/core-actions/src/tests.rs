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
    host.command(&mut CoreActions::default(), name, Args::new())
}

#[test]
fn the_commands_register_without_keys() {
    let mut host = host(None);
    host.activate(&mut CoreActions::default()).unwrap();
    let registered: Vec<(&str, Category, Vec<String>)> = host
        .commands()
        .iter()
        .map(|command| {
            assert!(
                command.params.is_empty()
                    || ["pane.send-text", COPY_OUTPUT, SELECT_OUTPUT]
                        .contains(&command.name.as_str())
            );
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
            view("pane.prompt-previous"),
            view("pane.prompt-next"),
            ("pane.send-text", Category::Input, names(&["pty-write"])),
            (
                COPY_OUTPUT,
                clipboard,
                names(&["history-read", "terminal-read", "clipboard-write"])
            ),
            (SELECT_OUTPUT, Category::Edit, names(&["history-read"])),
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
    // Sending text takes its text.
    let send = host
        .commands()
        .iter()
        .find(|command| command.name == SEND_TEXT)
        .unwrap();
    assert_eq!(send.params.len(), 1);
    assert_eq!(
        (
            send.params[0].name.as_str(),
            send.params[0].kind,
            &send.params[0].default
        ),
        ("text", lokyr_plugin_sdk::command::Kind::String, &None)
    );
    // The system's keymap binds their keys.
    assert!(host.bindings().is_empty());
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
fn the_view_scrolls_by_lines_pages_to_its_ends_and_to_the_prompts() {
    let mut host = host(Some(5));
    for name in [
        SCROLL_LINE_UP,
        SCROLL_LINE_DOWN,
        SCROLL_PAGE_UP,
        SCROLL_PAGE_DOWN,
        SCROLL_TOP,
        SCROLL_BOTTOM,
        PROMPT_PREVIOUS,
        PROMPT_NEXT,
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
        Scroll::PreviousPrompt,
        Scroll::NextPrompt,
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
    host.command(&mut CoreActions::default(), SEND_TEXT, args.clone())
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
    host.command(&mut CoreActions::default(), SEND_TEXT, args)
        .unwrap();
    assert!(host.take_done().is_empty());
}

/// A host of a focused pane 7, of 10 columns and rows 0 to 9 held, whose
/// shell ran `ls`, its output on rows 2 to 4, the last wrapping into a row
/// 5 of `x`, and then `false`, of no output, then a prompt at row 7.
fn commands_host() -> FakeHost {
    use lokyr_plugin_sdk::host::ShellCommand;
    let mut host = host(Some(7));
    let pane = host.pane(PaneId(7));
    pane.held = 0..10;
    pane.grid = (10, 10);
    for (row, text, wrapped) in [
        (0, "$ ls", false),
        (1, "", false),
        (2, "a.txt", false),
        (3, "b.txt", false),
        (4, "0123456789", true),
        (5, "x", false),
        (6, "$ false", false),
        (7, "$ ", false),
    ] {
        pane.rows.insert(row, (text.to_owned(), wrapped));
    }
    let command = |id, prompt: i64, output: Option<i64>, end: i64| ShellCommand {
        id,
        prompt: Some((0, prompt)),
        input: Some((2, prompt)),
        output: output.map(|row| (0, row)),
        end: Some((0, end)),
        status: Some(0),
    };
    pane.commands = vec![command(1, 0, Some(1), 6), command(2, 6, Some(7), 7)];
    host
}

/// Runs `name` of `args` on `host` with `plugin`, its events delivered.
fn run_with(host: &mut FakeHost, plugin: &mut CoreActions, name: &str, args: Args) -> Vec<Request> {
    host.take_done();
    host.command(plugin, name, args).unwrap();
    host.deliver(plugin).unwrap();
    host.take_done()
}

/// The arguments of the cell of `column` and `row`.
fn cell(column: i64, row: i64) -> Args {
    Args::from([
        ("column".to_owned(), Value::Integer(column)),
        ("row".to_owned(), Value::Integer(row)),
    ])
}

#[test]
fn the_last_command_s_output_is_copied_and_selected_of_its_marks() {
    let mut host = commands_host();
    let mut plugin = CoreActions::default();
    host.activate(&mut plugin).unwrap();
    // The last command that ended of some output is `ls`: `false` printed
    // nothing. Its rows from its output's mark to its end's, a wrapped row
    // joined to the next, the blank rows around it left out.
    assert!(host.applies(&mut plugin, COPY_OUTPUT, &Args::new()));
    run_with(&mut host, &mut plugin, COPY_OUTPUT, Args::new());
    assert_eq!(host.clipboard(), Some("a.txt\nb.txt\n0123456789x"));
    let selected = run_with(&mut host, &mut plugin, SELECT_OUTPUT, Args::new());
    assert!(
        selected.contains(&Request::SelectCells {
            pane: PaneId(7),
            first: (0, 1),
            last: (9, 5),
            kind: lokyr_plugin_sdk::host::SelectionKind::Cells,
        }),
        "{selected:?}"
    );
}

#[test]
fn a_cell_s_command_is_the_one_whose_rows_hold_it() {
    let mut host = commands_host();
    let mut plugin = CoreActions::default();
    host.activate(&mut plugin).unwrap();
    // A cell of `ls`' prompt, or of its output: `ls`'.
    for row in [0, 3] {
        assert!(host.applies(&mut plugin, COPY_OUTPUT, &cell(1, row)));
        host.set_clipboard(None);
        run_with(&mut host, &mut plugin, COPY_OUTPUT, cell(1, row));
        assert_eq!(host.clipboard(), Some("a.txt\nb.txt\n0123456789x"), "{row}");
    }
    // `false`'s, of no output, and a pane of no marks: nothing applies.
    assert!(!host.applies(&mut plugin, COPY_OUTPUT, &cell(1, 6)));
    host.pane(PaneId(7)).commands.clear();
    assert!(!host.applies(&mut plugin, SELECT_OUTPUT, &Args::new()));
    assert!(run_with(&mut host, &mut plugin, SELECT_OUTPUT, Args::new()).is_empty());
}

/// A host of a focused pane 7, of 10 columns, whose shell ran a command of
/// `rows`, each its text and whether it wraps, its output from `output`
/// to its end at `end`.
fn output_host(rows: &[(&str, bool)], output: (u16, i64), end: (u16, i64)) -> FakeHost {
    use lokyr_plugin_sdk::host::ShellCommand;
    let mut host = host(Some(7));
    let pane = host.pane(PaneId(7));
    pane.held = 0..10;
    pane.grid = (10, 10);
    for (row, &(text, wrapped)) in rows.iter().enumerate() {
        pane.rows.insert(row as i64, (text.to_owned(), wrapped));
    }
    pane.commands = vec![ShellCommand {
        id: 1,
        prompt: Some((0, 0)),
        input: Some((2, 0)),
        output: Some(output),
        end: Some(end),
        status: Some(0),
    }];
    host
}

/// What a copy of the last command's output gives, and the cells that its
/// selection takes.
fn copied_and_selected(host: &mut FakeHost) -> (Option<String>, Vec<Request>) {
    let mut plugin = CoreActions::default();
    host.activate(&mut plugin).unwrap();
    run_with(host, &mut plugin, COPY_OUTPUT, Args::new());
    let copied = host.clipboard().map(str::to_owned);
    let selected = run_with(host, &mut plugin, SELECT_OUTPUT, Args::new())
        .into_iter()
        .filter(|request| matches!(request, Request::SelectCells { .. }))
        .collect();
    (copied, selected)
}

/// A selection of the cells from `first` to `last` of pane 7.
fn selection(first: (u16, i64), last: (u16, i64)) -> Request {
    Request::SelectCells {
        pane: PaneId(7),
        first,
        last,
        kind: lokyr_plugin_sdk::host::SelectionKind::Cells,
    }
}

#[test]
fn zsh_s_mark_of_a_missing_newline_is_neither_copied_nor_selected() {
    // `printf foo` in zsh: the mark and the spaces that wrap the prompt to
    // a row of its own, before the command's end.
    let mut host = output_host(
        &[("$ printf foo", false), ("foo%      ", true), ("$ ", false)],
        (0, 1),
        (0, 2),
    );
    assert_eq!(
        copied_and_selected(&mut host),
        (Some("foo".to_owned()), vec![selection((0, 1), (2, 1))])
    );
    // An output that fills its row: the mark alone on the next.
    let mut host = output_host(
        &[
            ("$ printf", false),
            ("0123456789", true),
            ("%         ", true),
            ("$ ", false),
        ],
        (0, 1),
        (0, 3),
    );
    assert_eq!(
        copied_and_selected(&mut host),
        (
            Some("0123456789".to_owned()),
            vec![selection((0, 1), (9, 1))]
        )
    );
}

#[test]
fn an_output_s_rows_are_cut_at_their_cells_of_wide_characters_and_clusters() {
    // A woman technologist, of three characters in two cells, and the end
    // past the row's tenth cell.
    let row = "ok 👩\u{200d}💻 done";
    let mut host = output_host(&[("$ say", false), (row, false)], (0, 1), (10, 1));
    assert_eq!(copied_and_selected(&mut host).0.as_deref(), Some(row));
    // Of an output that starts past the emoji's cells.
    let mut host = output_host(&[("$ say", false), (row, false)], (5, 1), (10, 1));
    assert_eq!(copied_and_selected(&mut host).0.as_deref(), Some(" done"));
    let mut host = output_host(&[("$ say", false), (row, false)], (3, 1), (5, 1));
    assert_eq!(
        copied_and_selected(&mut host).0.as_deref(),
        Some("👩\u{200d}💻")
    );
}

#[test]
fn no_output_applies_while_the_alternate_screen_shows() {
    let mut host = output_host(&[("$ ls", false), ("a", false)], (0, 1), (0, 2));
    let mut plugin = CoreActions::default();
    host.activate(&mut plugin).unwrap();
    assert!(host.applies(&mut plugin, COPY_OUTPUT, &Args::new()));
    host.pane(PaneId(7)).viewport = Some(lokyr_plugin_sdk::host::Viewport {
        top: 0,
        rows: 10,
        screen: 0,
        oldest: 0,
        epoch: 0,
        alternate: true,
    });
    assert!(!host.applies(&mut plugin, COPY_OUTPUT, &Args::new()));
    assert!(!host.applies(&mut plugin, SELECT_OUTPUT, &cell(0, 1)));
}
