//! The plugin against the SDK's fake host, whose focused pane has a line,
//! its link and the working directory of the test's own, and which records
//! what it opens and decorates.

use lokyr_plugin_sdk::fake::FakeHost;
use lokyr_plugin_sdk::host::{LineText, PaneId};
use lokyr_plugin_sdk::message::Request;

use super::*;

/// The plugin's name, as its calls give it.
const NAME: &str = "lokyr/links";

/// The focused pane.
const PANE: PaneId = PaneId(4);

/// The line's columns.
const COLUMNS: u16 = 10;

/// A host whose focused pane's view has one line, `line`, from its row 1
/// to its row 2, a cell each character, which wraps to the next row after
/// ten, in the directory `/work/lokyr`.
fn focused(line: &str) -> FakeHost {
    let cells: Vec<(u16, i64)> = (0..line.chars().count())
        .map(|at| {
            let at = u16::try_from(at).unwrap();
            (at % COLUMNS, i64::from(1 + at / COLUMNS))
        })
        .collect();
    on_cells(line, &cells)
}

/// A host whose focused pane's view has `line` on the cells `cells`, of
/// its rows 1 and 2, in the directory `/work/lokyr`.
fn on_cells(line: &str, cells: &[(u16, i64)]) -> FakeHost {
    let mut host = FakeHost::new(NAME).with_manifest(crate::MANIFEST);
    host.focus(Some(PANE));
    let pane = host.pane(PANE);
    pane.directory = Some("/work/lokyr".to_owned());
    for row in 1..=2 {
        pane.lines.insert(
            row,
            LineText {
                text: line.to_owned(),
                cells: cells.to_vec(),
            },
        );
    }
    host
}

/// `host`, whose every cell of its line is of an OSC 8 link to `target`.
fn linked(mut host: FakeHost, target: &str) -> FakeHost {
    let pane = host.pane(PANE);
    let cells = pane.lines[&1].cells.clone();
    for cell in cells {
        pane.links.insert(cell, target.to_owned());
    }
    host
}

/// The arguments of a click on the cell at `column` of `row`.
fn cell(column: i64, row: i64) -> Args {
    Args::from([
        ("column".to_owned(), Value::Integer(column)),
        ("row".to_owned(), Value::Integer(row)),
    ])
}

/// Clicks `host`'s cell at `column` of `row`.
fn click(host: &mut FakeHost, column: i64, row: i64) -> Result<(), Error> {
    host.command(&mut Links::default(), OPEN, cell(column, row))
}

/// Whether a click on `host`'s cell at `column` of `row` would open
/// something, which opens nothing.
fn applies(host: &mut FakeHost, column: i64, row: i64) -> bool {
    let applies = host.applies(&mut Links::default(), OPEN, &cell(column, row));
    assert!(opened(host).is_empty());
    applies
}

/// The targets that `host`'s plugin opened since the last look, a file's
/// line and column after it, as `path:42:7`.
fn opened(host: &mut FakeHost) -> Vec<String> {
    host.take_done()
        .into_iter()
        .filter_map(|request| match request {
            Request::Open(target) => Some(target),
            Request::OpenAt {
                target,
                line,
                column,
            } => Some(match column {
                Some(column) => format!("{target}:{line}:{column}"),
                None => format!("{target}:{line}"),
            }),
            _ => None,
        })
        .collect()
}

/// The cells of the decorations that `host`'s plugin laid and did not take
/// away.
fn underlined(host: &FakeHost) -> Vec<Span> {
    host.decorations()
        .iter()
        .map(|decoration| {
            assert_eq!(
                (decoration.pane, decoration.look.clone()),
                (PANE, UNDERLINED)
            );
            (decoration.from, decoration.to)
        })
        .collect()
}

#[test]
fn the_commands_of_a_cell_register_open_s_bound_to_cmd_click() {
    let mut host = FakeHost::new(NAME).with_manifest(crate::MANIFEST);
    host.activate(&mut Links::default()).unwrap();
    let registered: Vec<(&str, Category, Vec<String>)> = host
        .commands()
        .iter()
        .map(|command| {
            (
                command.name.as_str(),
                command.category,
                command
                    .capabilities
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
            )
        })
        .collect();
    let of = |capabilities: &[&str]| capabilities.iter().map(|&c| c.to_owned()).collect();
    assert_eq!(
        registered,
        [
            (
                OPEN,
                Category::View,
                of(&["terminal-read", "process-read", "open"])
            ),
            (
                COPY,
                Category::Clipboard,
                of(&["terminal-read", "process-read", "clipboard-write"])
            ),
            (
                NEW_WINDOW,
                Category::Windows,
                of(&["terminal-read", "process-read"])
            ),
            (
                NEW_TAB,
                Category::Tabs,
                of(&["terminal-read", "process-read"])
            ),
            (
                HINTS,
                Category::View,
                of(&[
                    "terminal-read",
                    "process-read",
                    "decorate",
                    "pane-view",
                    "open",
                    "clipboard-write",
                    "pty-write"
                ])
            ),
            // Of the hints' capabilities, as it moves what a pick does.
            (
                HINTS_ACTION,
                Category::View,
                of(&[
                    "terminal-read",
                    "process-read",
                    "decorate",
                    "pane-view",
                    "open",
                    "clipboard-write",
                    "pty-write"
                ])
            ),
        ]
    );
    for command in host.commands() {
        let params: Vec<(&str, Kind, Option<&Value>)> = command
            .params
            .iter()
            .map(|param| (param.name.as_str(), param.kind, param.default.as_ref()))
            .collect();
        // The hints of their action, open by default, and their action's
        // move of its delta.
        let open = Value::String("open".to_owned());
        let wanted: &[(&str, Kind, Option<&Value>)] = match command.name.as_str() {
            HINTS => &[("action", Kind::String, Some(&open))],
            HINTS_ACTION => &[("delta", Kind::Integer, None)],
            _ => &[
                ("column", Kind::Integer, None),
                ("row", Kind::Integer, None),
            ],
        };
        assert_eq!(params, wanted);
        // Every title in both languages.
        for (_, catalog) in CATALOGS {
            let message = format!("command-{} =", command.name.replace('.', "-"));
            assert!(catalog.contains(&message), "{message}");
        }
    }
    // The system's keymap binds its click.
    assert!(host.bindings().is_empty());
    assert_eq!(host.subscriptions(), ["pointer"]);
}

/// What `name` asked of `host` at the cell of `column` and `row`, with
/// whether it applied there.
fn asked(host: &mut FakeHost, name: &str, (column, row): (i64, i64)) -> (bool, Vec<Request>) {
    let applies = host.applies(&mut Links::default(), name, &cell(column, row));
    host.take_done();
    host.command(&mut Links::default(), name, cell(column, row))
        .unwrap();
    (applies, host.take_done())
}

#[test]
fn copy_link_address_copies_what_would_open() {
    let copied = |text: &str| vec![Request::SetClipboardText(text.to_owned())];
    let mut host = focused("cat /etc/hosts");
    assert_eq!(asked(&mut host, COPY, (6, 1)), (true, copied("/etc/hosts")));
    let mut host = focused("--> src/main.rs:42:7");
    assert_eq!(
        asked(&mut host, COPY, (8, 1)),
        (true, copied("/work/lokyr/src/main.rs"))
    );
    let mut host = linked(focused("see the docs"), "https://lokyr.dev/osc8");
    assert_eq!(
        asked(&mut host, COPY, (5, 1)),
        (true, copied("https://lokyr.dev/osc8"))
    );
    // Nothing that opens: nothing copied.
    let mut host = focused("v1.2 // 3.5");
    assert_eq!(asked(&mut host, COPY, (1, 1)), (false, Vec::new()));
    let mut host = focused("README.md");
    host.set_missing(&["/work/lokyr/README.md"]);
    assert_eq!(asked(&mut host, COPY, (2, 1)), (false, Vec::new()));
}

#[test]
fn a_window_or_a_tab_opens_in_the_folder_of_the_path_at_the_cell() {
    let window = |folder: &str| {
        vec![Request::OpenWindow {
            directory: Some(folder.to_owned()),
        }]
    };
    let mut host = focused("cd ~/notes");
    assert_eq!(
        asked(&mut host, NEW_WINDOW, (4, 1)),
        (true, window("~/notes"))
    );
    let mut host = focused("--> src/main.rs:42:7");
    assert_eq!(
        asked(&mut host, NEW_WINDOW, (8, 1)),
        (true, window("/work/lokyr/src/main.rs"))
    );
    // A tab, after the others, which shows.
    let mut host = focused("ls ../other");
    let in_window = host.pane(PANE).window;
    let (applies, done) = asked(&mut host, NEW_TAB, (5, 1));
    assert!(applies);
    let [
        Request::NewTab {
            window: opened_in,
            index: None,
            directory: Some(folder),
        },
        Request::ActivateTab(_),
    ] = &done[..]
    else {
        panic!("{done:?}");
    };
    assert_eq!((*opened_in, folder.as_str()), (in_window, "/work/other"));
    // An OSC 8 link of a local file's URL, its escapes decoded.
    let mut host = linked(focused("the folder"), "file:///Users/me/My%20Work");
    assert_eq!(
        asked(&mut host, NEW_WINDOW, (1, 1)),
        (true, window("/Users/me/My Work"))
    );
    // Of `localhost`, of either case, and of this machine's name, as
    // `ls --hyperlink`, ripgrep and `fd` write it.
    let named = format!("file://{}/tmp", lokyr_plugin_sdk::fake::HOST_NAME);
    for link in [
        "file://localhost/tmp",
        "file://LOCALHOST/tmp",
        named.as_str(),
    ] {
        let mut host = linked(focused("the folder"), link);
        assert_eq!(
            asked(&mut host, NEW_WINDOW, (1, 1)),
            (true, window("/tmp")),
            "{link}"
        );
        let mut host = linked(focused("the folder"), link);
        assert!(asked(&mut host, NEW_TAB, (1, 1)).0, "{link}");
    }
    // A URL of another place, or of another machine, and nothing there.
    for (line, link) in [
        ("see https://lokyr.dev/x", None),
        ("the folder", Some("file://elsewhere/tmp")),
        ("v1.2 // 3.5", None),
    ] {
        let mut host = focused(line);
        if let Some(link) = link {
            host = linked(host, link);
        }
        for name in [NEW_WINDOW, NEW_TAB] {
            assert_eq!(
                asked(&mut host, name, (6, 1)),
                (false, Vec::new()),
                "{line}"
            );
        }
    }
}

#[test]
fn a_local_path_is_a_path_or_a_file_url_s() {
    assert_eq!(local_path("/a/b").as_deref(), Some("/a/b"));
    assert_eq!(local_path("~").as_deref(), Some("~"));
    assert_eq!(local_path("FILE:///a%2Fb%zz").as_deref(), Some("/a/b%zz"));
    assert_eq!(local_path("file:///%FF"), None);
    // Of whatever host, as a URL of another machine's opens not.
    assert_eq!(local_path("file://mac.local/a").as_deref(), Some("/a"));
    assert_eq!(local_path("file:/a").as_deref(), Some("/a"));
    assert_eq!(local_path("file://host"), None);
    assert_eq!(local_path("https://lokyr.dev"), None);
    assert_eq!(local_path("~x"), None);
}

/// The pointer at the cell of `column` and `row`, or out of the pane, with
/// Command held or not.
fn pointer(at: Option<(u16, i64)>, command: bool) -> Event {
    Event::Pointer {
        pane: PANE,
        at,
        mods: lokyr_plugin_sdk::message::Mods {
            command,
            ..Default::default()
        },
    }
}

#[test]
fn command_underlines_the_link_under_the_pointer_while_both_stay() {
    // `see https://lokyr.dev/x here`, of rows of 10 from the row 1: the
    // URL from the cell (4, 1) to (2, 3).
    let mut host = focused("see https://lokyr.dev/x here");
    host.set_missing(&["/work/lokyr/see"]);
    let mut links = Links::default();
    let mut hover = |host: &mut FakeHost, events: &[Event]| {
        host.events(&mut links, events).unwrap();
        underlined(host)
    };
    let url = ((4, 1), (2, 3));
    // Without Command, nothing; with it, the URL.
    assert!(hover(&mut host, &[pointer(Some((6, 1)), false)]).is_empty());
    assert_eq!(hover(&mut host, &[pointer(Some((6, 1)), true)]), [url]);
    // Over another of its cells, the same underline, not laid again.
    assert_eq!(hover(&mut host, &[pointer(Some((9, 1)), true)]), [url]);
    let laid = host
        .take_done()
        .into_iter()
        .filter(|request| matches!(request, Request::Decorate { .. }))
        .count();
    assert_eq!(laid, 1);
    // Over a word that opens nothing, none; back, the URL again.
    assert!(hover(&mut host, &[pointer(Some((1, 1)), true)]).is_empty());
    assert_eq!(hover(&mut host, &[pointer(Some((6, 1)), true)]), [url]);
    // Command up, or the pointer out of the pane, takes it away; a batch
    // goes by its last pointer.
    assert!(hover(&mut host, &[pointer(Some((6, 1)), false)]).is_empty());
    assert_eq!(
        hover(
            &mut host,
            &[pointer(None, true), pointer(Some((6, 1)), true)]
        ),
        [url]
    );
    assert!(hover(&mut host, &[pointer(None, true)]).is_empty());
}

#[test]
fn command_underlines_an_osc_8_link_s_cells_and_a_path_with_its_line() {
    // An OSC 8 link over the whole line, as the host gives one.
    let mut host = linked(focused("the docs"), "https://lokyr.dev/osc8");
    let mut links = Links::default();
    host.events(&mut links, &[pointer(Some((5, 1)), true)])
        .unwrap();
    assert_eq!(underlined(&host), [((0, 1), (7, 1))]);
    // A path with its line and column, as `grep -n` writes it: the path
    // and its place, which open in the editor at them.
    let mut host = focused("at src/main.rs:42:7 now");
    let mut links = Links::default();
    host.events(&mut links, &[pointer(Some((5, 1)), true)])
        .unwrap();
    assert_eq!(underlined(&host), [((3, 1), (8, 2))]);
}

#[test]
fn a_path_of_escaped_spaces_opens_and_command_underlines_it_whole() {
    // As a shell's completion writes it: `My\ File.txt`, over two rows.
    let mut host = focused("open My\\ File.txt");
    assert!(applies(&mut host, 6, 1));
    click(&mut host, 6, 1).unwrap();
    assert_eq!(opened(&mut host), ["/work/lokyr/My File.txt"]);
    let mut links = Links::default();
    host.events(&mut links, &[pointer(Some((6, 1)), true)])
        .unwrap();
    assert_eq!(underlined(&host), [((5, 1), (6, 2))]);
}

#[test]
fn a_click_opens_the_cell_s_link_first() {
    let mut host = linked(
        focused("see https://lokyr.dev/text"),
        "https://lokyr.dev/osc8",
    );
    assert!(applies(&mut host, 0, 1));
    click(&mut host, 0, 1).unwrap();
    assert_eq!(opened(&mut host), ["https://lokyr.dev/osc8"]);
    // A target that does not open fails as the host says.
    let mut host = linked(host, "javascript:x");
    assert!(!applies(&mut host, 0, 1));
    assert_eq!(click(&mut host, 0, 1).unwrap_err().code, Code::Failed);
}

#[test]
fn a_click_opens_the_url_or_the_path_around_it() {
    for (line, (column, row), opened) in [
        // A URL that wraps, clicked on its second row.
        (
            "see https://lokyr.dev/docs.",
            (2, 2),
            "https://lokyr.dev/docs",
        ),
        // A path on the first row, as the second has another word there.
        ("/etc/hosts and more", (2, 1), "/etc/hosts"),
        // The paths: absolute, from `~`, and relative to the directory.
        ("cat /etc/hosts", (6, 1), "/etc/hosts"),
        ("ls ~/notes", (4, 1), "~/notes"),
        // A path's line and column, which open in the editor at them.
        (
            "--> src/main.rs:42:7",
            (8, 1),
            "/work/lokyr/src/main.rs:42:7",
        ),
        (
            "src/app.ts(12,3): error",
            (4, 1),
            "/work/lokyr/src/app.ts:12:3",
        ),
        ("see src/main.rs#L9", (6, 1), "/work/lokyr/src/main.rs:9"),
        ("cd ../other/./x", (5, 1), "/work/other/x"),
        ("open .", (5, 1), "/work/lokyr"),
    ] {
        let mut host = focused(line);
        assert!(applies(&mut host, column, row), "{line}");
        click(&mut host, column, row).unwrap();
        assert_eq!(self::opened(&mut host), [opened], "{line}");
    }
    // A wide character covers two cells, and the characters after it are
    // a cell further.
    let mut host = on_cells("🙂 a/b", &[(0, 1), (2, 1), (3, 1), (4, 1), (5, 1)]);
    for column in [1, 2] {
        assert!(!applies(&mut host, column, 1));
        click(&mut host, column, 1).unwrap();
    }
    assert!(self::opened(&mut host).is_empty());
    assert!(applies(&mut host, 3, 1));
    click(&mut host, 3, 1).unwrap();
    assert_eq!(self::opened(&mut host), ["/work/lokyr/a/b"]);
    // A click on a wide character's second cell.
    let mut host = on_cells("日本/x", &[(0, 1), (2, 1), (4, 1), (5, 1)]);
    click(&mut host, 1, 1).unwrap();
    assert_eq!(self::opened(&mut host), ["/work/lokyr/日本/x"]);
}

#[test]
fn a_click_on_nothing_opens_nothing() {
    // A version, a space, a word without a letter, a number, a cell past
    // the line, rows without one.
    let mut host = focused("v1.2 // 3.5");
    for (column, row) in [(1, 1), (4, 1), (5, 1), (8, 1), (9, 2), (0, 5), (0, 0)] {
        assert!(!applies(&mut host, column, row), "{column}, {row}");
        click(&mut host, column, row).unwrap();
    }
    assert!(!applies(&mut host, -1, 1));
    // Cells that no view has, and arguments that the registry would have
    // refused.
    click(&mut host, -1, 1).unwrap();
    click(&mut host, 0, 70_000).unwrap();
    host.command(&mut Links::default(), OPEN, Args::new())
        .unwrap();
    // A relative path without a directory, as after the program ended.
    let mut ended = focused("README.md");
    ended.pane(PANE).directory = None;
    assert!(!applies(&mut ended, 2, 1));
    click(&mut ended, 2, 1).unwrap();
    // Without a focused pane.
    let mut unfocused = focused("/etc/hosts");
    unfocused.focus(None);
    assert!(!applies(&mut unfocused, 2, 1));
    click(&mut unfocused, 2, 1).unwrap();
    assert!(opened(&mut host).is_empty());
    assert!(opened(&mut ended).is_empty() && opened(&mut unfocused).is_empty());
    assert_eq!(
        host.command(&mut Links::default(), "links.other", Args::new())
            .unwrap_err()
            .code,
        Code::NotFound
    );
    // A path that is not there, which a click tries all the same.
    let mut missing = focused("README.md");
    missing.set_missing(&["/work/lokyr/README.md"]);
    assert!(!applies(&mut missing, 2, 1));
    // Another command, at a cell with a path.
    let mut path = focused("/etc/hosts");
    assert!(!path.applies(&mut Links::default(), "links.other", &cell(2, 1)));
}

#[test]
fn a_quoted_sentence_opens_the_url_or_the_path_in_it() {
    /// A host whose line `line` is on the view's row 1 alone, the targets
    /// of `missing` not there.
    fn one_row(line: &str, missing: &[&str]) -> FakeHost {
        let cells: Vec<(u16, i64)> = (0..line.chars().count())
            .map(|at| (u16::try_from(at).unwrap(), 1))
            .collect();
        let mut host = on_cells(line, &cells);
        host.set_missing(missing);
        host
    }
    let log = r#"{"msg":"failed to read /etc/app/config.yaml: no such file"}"#;
    let quoted = "/work/lokyr/failed to read /etc/app/config.yaml";
    let config = log.find("config").unwrap();
    for (line, column, missing, opened, there) in [
        // A URL, which a space ends.
        (
            r#"{"msg":"see https://example.com/docs for details"}"#,
            30,
            vec![],
            "https://example.com/docs",
            true,
        ),
        // The quoted text is no path that is there: the word's.
        (log, config, vec![quoted], "/etc/app/config.yaml", true),
        // Neither is there: the word's, which the click tries.
        (
            log,
            config,
            vec![quoted, "/etc/app/config.yaml"],
            "/etc/app/config.yaml",
            false,
        ),
        // A path with a space that is there, whole.
        (
            r#"?? "My File.txt""#,
            8,
            vec![],
            "/work/lokyr/My File.txt",
            true,
        ),
    ] {
        let mut host = one_row(line, &missing);
        let column = i64::try_from(column).unwrap();
        assert_eq!(applies(&mut host, column, 1), there, "{line}");
        click(&mut host, column, 1).unwrap();
        assert_eq!(self::opened(&mut host), [opened], "{line}");
    }
}

#[test]
fn a_path_loses_its_dots_and_its_empty_parts() {
    for (path, normal) in [
        ("/a/./b//c/", "/a/b/c"),
        ("/a/../../b", "/b"),
        ("~/x/../y", "~/y"),
        ("~", "~"),
        ("~/..", "~/.."),
        ("~/../Shared", "~/../Shared"),
        ("~/a/../..", "~/.."),
        ("a/./b", "a/b"),
    ] {
        assert_eq!(super::normal(path), normal, "{path}");
    }
}

/// A host whose focused pane shows `lines`, each on its own row from the
/// first, a cell each character, of a grid of 40 rows, its cursor on the
/// row after the last, in the directory `/work/lokyr`.
fn screen(lines: &[&str]) -> FakeHost {
    let mut host = FakeHost::new(NAME).with_manifest(crate::MANIFEST);
    host.focus(Some(PANE));
    let pane = host.pane(PANE);
    pane.directory = Some("/work/lokyr".to_owned());
    pane.grid = (80, 40);
    pane.held = 0..40;
    for (row, line) in (0..).zip(lines) {
        let cells = (0..line.chars().count())
            .map(|at| (u16::try_from(at).unwrap(), row))
            .collect();
        pane.lines.insert(
            row,
            LineText {
                text: (*line).to_owned(),
                cells,
            },
        );
    }
    pane.cursor = (0, i64::try_from(lines.len()).unwrap());
    host
}

/// Fills `pane` of `host` with `lines`, as [`screen`] does its focused
/// pane's, of the same grid and folder.
fn fill(host: &mut FakeHost, pane: PaneId, lines: &[&str]) {
    let pane = host.pane(pane);
    pane.directory = Some("/work/lokyr".to_owned());
    pane.grid = (80, 40);
    pane.held = 0..40;
    for (row, line) in (0..).zip(lines) {
        let cells = (0..line.chars().count())
            .map(|at| (u16::try_from(at).unwrap(), row))
            .collect();
        pane.lines.insert(
            row,
            LineText {
                text: (*line).to_owned(),
                cells,
            },
        );
    }
    pane.cursor = (0, i64::try_from(lines.len()).unwrap());
}

/// The config of the hints' patterns `hints`, each of its name, its
/// pattern and its URL, if it has one.
fn hints_config(hints: &[(&str, &str, Option<&str>)]) -> lokyr_plugin_sdk::message::Data {
    use lokyr_plugin_sdk::message::Data;
    use std::collections::BTreeMap;

    let table = |pattern: &str, url: Option<&str>| {
        let mut table = BTreeMap::from([("pattern".to_owned(), Data::Text(pattern.to_owned()))]);
        if let Some(url) = url {
            table.insert("url".to_owned(), Data::Text(url.to_owned()));
        }
        Data::Map(table)
    };
    Data::Map(BTreeMap::from([(
        "hints".to_owned(),
        Data::Map(
            hints
                .iter()
                .map(|&(name, pattern, url)| (name.to_owned(), table(pattern, url)))
                .collect(),
        ),
    )]))
}

/// The hints' field's typing of `text`.
fn typing(text: &str) -> UiEvent {
    UiEvent {
        view: ViewId::Panel(PanelId::from(HINTS_PANEL)),
        key: HINTS_FIELD.into(),
        kind: UiEventKind::InputChanged {
            text: text.to_owned(),
            revision: 1,
        },
    }
}

/// The labels that `host`'s decorations show, by their first cells.
fn labels(host: &FakeHost) -> Vec<((u16, i64), String)> {
    let mut found: Vec<((u16, i64), String)> = host
        .decorations()
        .iter()
        .filter_map(|laid| Some((laid.from, laid.look.label.clone()?)))
        .collect();
    found.sort();
    found
}

#[test]
fn hints_label_what_the_screen_shows_the_nearest_the_cursor_first_and_a_label_picks_one() {
    let mut host = screen(&[
        "see https://lokyr.dev/docs ok",
        "   --> src/main.rs:42:7",
        "commit 4482b9e9 on 192.168.1.20",
        "nothing here",
    ]);
    let mut links = Links::default();
    host.take_done();
    host.command(&mut links, HINTS, Args::new()).unwrap();
    let opened = host.take_done();
    // The hash and the address, a row from the cursor, then the path, then
    // the URL.
    assert_eq!(
        labels(&host),
        [
            ((4, 0), "f".to_owned()),
            ((7, 1), "d".to_owned()),
            ((7, 2), "a".to_owned()),
            ((19, 2), "s".to_owned()),
        ]
    );
    // Each underlined over its whole text, and the panel of the letters.
    let url = host
        .decorations()
        .iter()
        .find(|laid| laid.from == (4, 0))
        .unwrap();
    assert_eq!(url.to, (25, 0));
    assert_eq!(url.look.underline, Some(Underline::Single));
    assert!(opened.iter().any(|request| matches!(
        request,
        Request::OpenPanel { id, options } if id.0 == HINTS_PANEL && options.close_escape
    )));
    // `s` picks the address, which opens nothing, so that its opening
    // copies it; its labels go.
    host.take_done();
    host.ui_event(&mut links, &typing("s")).unwrap();
    let done = host.take_done();
    assert!(
        done.contains(&Request::SetClipboardText("192.168.1.20".to_owned())),
        "{done:?}"
    );
    assert!(done.contains(&Request::ClosePanel(PanelId::from(HINTS_PANEL))));
    assert!(host.decorations().is_empty());
    assert!(links.hints.is_none());
}

#[test]
fn letters_narrow_the_labels_of_two_and_escape_leaves() {
    // Thirty hashes: 25 labels of one letter, and five of two of the last.
    let lines: Vec<String> = (0..30).map(|at| format!("c0ffee{at:02}a")).collect();
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    let mut host = screen(&lines);
    let mut links = Links::default();
    host.command(&mut links, HINTS, Args::new()).unwrap();
    host.render(&mut links, &ViewId::Panel(PanelId::from(HINTS_PANEL)))
        .expect("the hints' field");
    assert_eq!(labels(&host).len(), 30);
    let two: Vec<String> = labels(&host)
        .into_iter()
        .map(|(_, label)| label)
        .filter(|label| label.len() == 2)
        .collect();
    assert_eq!(two.len(), 5);
    assert!(two.iter().all(|label| label.starts_with('m')));
    // `m` leaves those five.
    host.ui_event(&mut links, &typing("m")).unwrap();
    assert_eq!(labels(&host).len(), 5);
    // Of none that starts with them, the field emptied and every label
    // shown again.
    host.take_done();
    host.ui_event(&mut links, &typing("mz")).unwrap();
    assert!(host.take_done().iter().any(|request| matches!(
        request,
        Request::SetText { text, .. } if text.is_empty()
    )));
    assert_eq!(labels(&host).len(), 30);
    // Escape closes the panel, which takes the labels away.
    host.post(Event::PanelClosed(PanelId::from(HINTS_PANEL)));
    host.deliver(&mut links).unwrap();
    assert!(host.decorations().is_empty());
    assert!(links.hints.is_none());
    // A screen of nothing to pick opens no panel.
    let mut host = screen(&["plain words alone"]);
    let mut links = Links::default();
    host.take_done();
    host.command(&mut links, HINTS, Args::new()).unwrap();
    assert!(
        !host
            .take_done()
            .iter()
            .any(|request| matches!(request, Request::OpenPanel { .. }))
    );
}

/// What a pick of the hint of `label` does, of `host`'s screen, the hints
/// shown of `args`, after `tabs` presses of Tab; the requests of the pick
/// alone, the decorations' and the panel's aside.
fn picked(host: &mut FakeHost, args: Args, tabs: i64, label: &str) -> Vec<Request> {
    let mut links = Links::default();
    host.activate(&mut links).unwrap();
    host.command(&mut links, HINTS, args).unwrap();
    host.render(&mut links, &ViewId::Panel(PanelId::from(HINTS_PANEL)))
        .expect("the hints' field");
    if tabs != 0 {
        let delta = Args::from([("delta".to_owned(), Value::Integer(tabs))]);
        host.command(&mut links, HINTS_ACTION, delta).unwrap();
    }
    host.take_done();
    host.ui_event(&mut links, &typing(label)).unwrap();
    host.take_done()
        .into_iter()
        .filter(|request| {
            !matches!(
                request,
                Request::Undecorate { .. } | Request::ClosePanel(_) | Request::RequestRender(_)
            )
        })
        .collect()
}

#[test]
fn a_pick_opens_copies_types_or_selects_as_its_action_and_a_capital_copies() {
    let lines = [
        "see https://lokyr.dev/docs ok",
        "   --> /work/lokyr/src/main.rs:42:7",
        "commit 4482b9e9",
    ];
    let action = |name: &str| Args::from([("action".to_owned(), Value::String(name.to_owned()))]);
    // The hash `a`, the path `s`, the URL `d`.
    let url = "https://lokyr.dev/docs".to_owned();
    assert_eq!(
        picked(&mut screen(&lines), Args::new(), 0, "d"),
        [Request::Open(url.clone())]
    );
    assert_eq!(
        picked(&mut screen(&lines), Args::new(), 0, "s"),
        [Request::OpenAt {
            target: "/work/lokyr/src/main.rs".to_owned(),
            line: 42,
            column: Some(7)
        }]
    );
    // A capital, as ⇧ types it, copies whatever the action.
    assert_eq!(
        picked(&mut screen(&lines), Args::new(), 0, "D"),
        [Request::SetClipboardText(url.clone())]
    );
    assert_eq!(
        picked(&mut screen(&lines), action("type"), 0, "d"),
        [Request::SendText {
            pane: PANE,
            text: url.clone()
        }]
    );
    assert_eq!(
        picked(&mut screen(&lines), action("select"), 0, "a"),
        [Request::SelectCells {
            pane: PANE,
            first: (7, 2),
            last: (14, 2),
            kind: lokyr_plugin_sdk::host::SelectionKind::Cells,
        }]
    );
    // Tab moves the action, around, as the field shows it: open, copy,
    // type, select; back of ⇧Tab.
    assert_eq!(
        picked(&mut screen(&lines), Args::new(), 2, "d"),
        [Request::SendText {
            pane: PANE,
            text: url.clone()
        }]
    );
    assert_eq!(
        picked(&mut screen(&lines), Args::new(), -3, "d"),
        [Request::SetClipboardText(url)]
    );
    let mut host = screen(&lines);
    let mut links = Links::default();
    host.command(&mut links, HINTS, Args::new()).unwrap();
    let delta = Args::from([("delta".to_owned(), Value::Integer(1))]);
    host.command(&mut links, HINTS_ACTION, delta).unwrap();
    let field = host
        .render(&mut links, &ViewId::Panel(PanelId::from(HINTS_PANEL)))
        .unwrap();
    assert!(
        matches!(&field.children[0].kind, lokyr_plugin_sdk::view::Kind::Text(text) if text == "Copy ▸")
    );
    // An action that is none refused.
    let error = screen(&lines)
        .command(&mut Links::default(), HINTS, action("paste"))
        .unwrap_err();
    assert_eq!(error.code, Code::Failed);
}

#[test]
fn a_pattern_of_the_config_labels_its_matches_which_open_its_url() {
    use lokyr_plugin_sdk::message::Data;

    let hint = |pattern: &str, url: Option<&str>| {
        let mut table = std::collections::BTreeMap::from([(
            "pattern".to_owned(),
            Data::Text(pattern.to_owned()),
        )]);
        if let Some(url) = url {
            table.insert("url".to_owned(), Data::Text(url.to_owned()));
        }
        Data::Map(table)
    };
    let config = Data::Map(std::collections::BTreeMap::from([(
        "hints".to_owned(),
        Data::Map(std::collections::BTreeMap::from([
            (
                "jira".to_owned(),
                hint(r"PROJ-(\d+)", Some("https://jira.example.com/browse/$0")),
            ),
            ("broken".to_owned(), hint("(", None)),
        ])),
    )]));
    let mut host = screen(&["fixed in PROJ-42 today"]);
    host.set_config(config);
    host.take_done();
    let done = picked(&mut host, Args::new(), 0, "a");
    assert_eq!(
        done,
        [Request::Open(
            "https://jira.example.com/browse/PROJ-42".to_owned()
        )]
    );
    // The pattern that does not read, told in a notice as the plugin
    // activated.
    let mut host = screen(&["x"]);
    host.set_config(Data::Map(std::collections::BTreeMap::from([(
        "hints".to_owned(),
        Data::Map(std::collections::BTreeMap::from([(
            "broken".to_owned(),
            hint("(", None),
        )])),
    )])));
    host.take_done();
    host.activate(&mut Links::default()).unwrap();
    assert!(host.take_done().iter().any(|request| matches!(
        request,
        Request::Notify { text, .. } if text.contains("`broken`")
    )));
}

#[test]
fn the_alphabet_of_the_config_gives_the_labels_and_one_that_is_none_is_told() {
    use lokyr_plugin_sdk::message::Data;

    let alphabet = |letters: &str| {
        Data::Map(std::collections::BTreeMap::from([(
            "alphabet".to_owned(),
            Data::Text(letters.to_owned()),
        )]))
    };
    let lines = ["commit 4482b9e9", "commit 5582b9e9", "commit 6682b9e9"];
    let mut host = screen(&lines);
    host.set_config(alphabet("jk"));
    let mut links = Links::default();
    host.activate(&mut links).unwrap();
    host.command(&mut links, HINTS, Args::new()).unwrap();
    // Of two letters, the nearest one of one, the others of two.
    let mut shown: Vec<String> = labels(&host).into_iter().map(|(_, label)| label).collect();
    shown.sort();
    assert_eq!(shown, ["j", "kj", "kk"]);
    // Of a letter twice or a capital, told, the default's letters.
    for wrong in ["aa", "aB", "a"] {
        let mut host = screen(&lines);
        host.set_config(alphabet(wrong));
        let mut links = Links::default();
        host.take_done();
        host.activate(&mut links).unwrap();
        assert!(
            host.take_done().iter().any(|request| matches!(
                request,
                Request::Notify { text, .. } if text.contains("alphabet")
            )),
            "{wrong}"
        );
        host.command(&mut links, HINTS, Args::new()).unwrap();
        let mut shown: Vec<String> = labels(&host).into_iter().map(|(_, label)| label).collect();
        shown.sort();
        assert_eq!(shown, ["a", "d", "s"], "{wrong}");
    }
}

#[test]
fn hints_shown_again_take_the_config_of_now_in_their_open_panel_and_escape_takes_them_all() {
    use lokyr_plugin_sdk::message::Data;

    let lines = ["commit 4482b9e9", "commit 5582b9e9", "commit 6682b9e9"];
    let mut host = screen(&lines);
    let mut links = Links::default();
    host.activate(&mut links).unwrap();
    host.command(&mut links, HINTS, Args::new()).unwrap();
    let panel = ViewId::Panel(PanelId::from(HINTS_PANEL));
    host.render(&mut links, &panel).expect("the hints' field");
    let sorted = |host: &FakeHost| {
        let mut shown: Vec<String> = labels(host).into_iter().map(|(_, label)| label).collect();
        shown.sort();
        shown
    };
    assert_eq!(sorted(&host), ["a", "d", "s"]);
    // The config changes while they show; shown again, of its alphabet,
    // in the panel open, its field emptied and of the keys again.
    host.set_config(Data::Map(std::collections::BTreeMap::from([(
        "alphabet".to_owned(),
        Data::Text("jkl".to_owned()),
    )])));
    host.deliver(&mut links).unwrap();
    host.take_done();
    host.command(&mut links, HINTS, Args::new()).unwrap();
    let done = host.take_done();
    assert!(
        !done
            .iter()
            .any(|request| matches!(request, Request::OpenPanel { .. })),
        "{done:?}"
    );
    assert!(done.iter().any(|request| matches!(
        request,
        Request::SetText { text, .. } if text.is_empty()
    )));
    assert!(
        done.iter()
            .any(|request| matches!(request, Request::FocusNode { .. }))
    );
    assert_eq!(sorted(&host), ["j", "k", "l"]);
    // Escape takes every label away, none of before left.
    host.post(Event::PanelClosed(PanelId::from(HINTS_PANEL)));
    host.deliver(&mut links).unwrap();
    assert!(host.decorations().is_empty());
    // Shown again over a screen of nothing to pick, their panel closes.
    let mut host = screen(&lines);
    let mut links = Links::default();
    host.command(&mut links, HINTS, Args::new()).unwrap();
    host.pane(PANE).lines.clear();
    host.take_done();
    host.command(&mut links, HINTS, Args::new()).unwrap();
    assert!(
        host.take_done()
            .contains(&Request::ClosePanel(PanelId::from(HINTS_PANEL)))
    );
    assert!(host.decorations().is_empty());
}

#[test]
fn a_pick_that_fails_is_told_and_its_labels_and_panel_go_all_the_same() {
    // A template of the match alone, which a scheme that does not open
    // starts.
    let mut host = screen(&["fixed in jira:PROJ-42 today"]);
    host.set_config(hints_config(&[("ticket", r"jira:PROJ-\d+", Some("$0"))]));
    let mut links = Links::default();
    host.activate(&mut links).unwrap();
    host.command(&mut links, HINTS, Args::new()).unwrap();
    assert_eq!(labels(&host), [((9, 0), "a".to_owned())]);
    host.take_done();
    host.ui_event(&mut links, &typing("a")).unwrap();
    let done = host.take_done();
    assert!(
        done.contains(&Request::ClosePanel(PanelId::from(HINTS_PANEL))),
        "{done:?}"
    );
    assert!(host.decorations().is_empty());
    assert!(links.hints.is_none());
    let notice = &host.notices().last().expect("the pick's notice").text;
    assert!(
        notice.contains("`jira:PROJ-42`") && notice.contains("`jira:` does not open"),
        "{notice}"
    );
    // Shown again before the close's event comes, the plugin's own, which
    // ends nothing more.
    host.command(&mut links, HINTS, Args::new()).unwrap();
    host.deliver(&mut links).unwrap();
    assert_eq!(labels(&host), [((9, 0), "a".to_owned())]);
    assert!(links.hints.is_some());
    assert_eq!(links.closing, 0);
}

#[test]
fn a_pattern_of_a_url_that_does_not_open_is_left_out_and_told_once_a_change() {
    let config = hints_config(&[
        ("editor", r"\w+\.rs", Some("vscode://file/work/$0")),
        (
            "jira",
            r"PROJ-\d+",
            Some("https://jira.example.com/browse/$0"),
        ),
        ("notes", r"note-\d+", Some("~/notes/$0.md")),
        ("matched", r"https?://\S+", Some("$0")),
    ]);
    let mut host = screen(&["main.rs PROJ-42"]);
    host.set_config(config.clone());
    let mut links = Links::default();
    host.activate(&mut links).unwrap();
    // Every pattern but the editor's, whose scheme does not open; one of
    // a path, and one of the match, left to their opening.
    assert_eq!(links.patterns.len(), 3);
    let notices: Vec<&str> = host
        .notices()
        .iter()
        .map(|notice| notice.text.as_str())
        .collect();
    assert_eq!(
        notices,
        [
            "The hint `editor` has a URL that doesn't open, `vscode://file/work/$0`: only http, https, mailto, file URLs open"
        ]
    );
    // The config changed, of the same problem: no notice.
    let lokyr_plugin_sdk::message::Data::Map(mut changed) = config else {
        unreachable!()
    };
    changed.insert(
        "alphabet".to_owned(),
        lokyr_plugin_sdk::message::Data::Text("jkl".to_owned()),
    );
    host.advance(std::time::Duration::from_secs(2));
    host.set_config(lokyr_plugin_sdk::message::Data::Map(changed));
    host.deliver(&mut links).unwrap();
    assert_eq!(links.alphabet.as_deref(), Some("jkl"));
    assert_eq!(host.notices().len(), 1);
    // Its URL changed, to another that does not open: told again.
    host.advance(std::time::Duration::from_secs(2));
    host.set_config(hints_config(&[(
        "editor",
        r"\w+\.rs",
        Some("vvsxcode://file/work/$0"),
    )]));
    host.deliver(&mut links).unwrap();
    assert_eq!(host.notices().len(), 2);
    assert!(host.notices()[1].text.contains("`vvsxcode://file/work/$0`"));
    // Of another problem too soon after, the host takes no notice: told at
    // the next change that keeps it.
    host.set_config(hints_config(&[
        ("editor", r"\w+\.rs", Some("vvsxcode://file/work/$0")),
        ("broken", "(", None),
    ]));
    host.deliver(&mut links).unwrap();
    assert_eq!(host.notices().len(), 2);
    host.advance(std::time::Duration::from_secs(2));
    host.set_config(hints_config(&[
        ("editor", r"\w+\.rs", Some("vvsxcode://file/work/$0")),
        ("broken", "(", None),
        (
            "jira",
            r"PROJ-\d+",
            Some("https://jira.example.com/browse/$0"),
        ),
    ]));
    host.deliver(&mut links).unwrap();
    assert_eq!(host.notices().len(), 3);
    // A pattern that does not read, of an error of several lines, beside
    // it: one notice of both, of one line.
    let notice = &host.notices().last().unwrap().text;
    assert!(
        notice.contains("`broken`") && notice.contains("`editor`") && !notice.contains('\n'),
        "{notice}"
    );
    // Of none left, no notice, and the next problem told again.
    host.advance(std::time::Duration::from_secs(2));
    host.set_config(hints_config(&[]));
    host.deliver(&mut links).unwrap();
    assert_eq!(host.notices().len(), 3);
    assert!(links.told.is_none());
}

#[test]
fn hints_shown_again_in_another_pane_survive_the_close_that_moved_them() {
    const OTHER: PaneId = PaneId(5);
    let mut host = screen(&["commit 4482b9e9"]);
    fill(&mut host, OTHER, &["commit 5582b9e9", "commit 6682b9e9"]);
    let mut links = Links::default();
    host.activate(&mut links).unwrap();
    host.command(&mut links, HINTS, Args::new()).unwrap();
    let panes = |host: &FakeHost| -> Vec<PaneId> {
        host.decorations().iter().map(|laid| laid.pane).collect()
    };
    assert_eq!(panes(&host), [PANE]);
    // In the other pane: the panel closed and opened again at its corner.
    host.focus(Some(OTHER));
    host.take_done();
    host.command(&mut links, HINTS, Args::new()).unwrap();
    let done = host.take_done();
    assert!(done.contains(&Request::ClosePanel(PanelId::from(HINTS_PANEL))));
    assert!(
        done.iter()
            .any(|request| matches!(request, Request::OpenPanel { .. }))
    );
    assert_eq!(panes(&host), [OTHER, OTHER]);
    // The close's event comes, of the hints of before: those of now stay.
    host.deliver(&mut links).unwrap();
    host.render(&mut links, &ViewId::Panel(PanelId::from(HINTS_PANEL)))
        .expect("the hints' field");
    assert_eq!(links.hints.as_ref().map(|shown| shown.pane), Some(OTHER));
    assert_eq!(panes(&host), [OTHER, OTHER]);
    // A third time, in the same pane: shown anew in the panel open.
    host.take_done();
    host.command(&mut links, HINTS, Args::new()).unwrap();
    let done = host.take_done();
    assert!(
        !done
            .iter()
            .any(|request| matches!(request, Request::OpenPanel { .. } | Request::ClosePanel(_))),
        "{done:?}"
    );
    assert_eq!(panes(&host), [OTHER, OTHER]);
    // Escape takes them away.
    host.close_panel(&PanelId::from(HINTS_PANEL));
    host.deliver(&mut links).unwrap();
    assert!(host.decorations().is_empty());
    assert!(links.hints.is_none());
}

#[test]
fn the_english_of_the_messages_is_english_s_catalog() {
    let catalog = CATALOGS[0].1;
    for (id, english) in ENGLISH {
        let line = format!("{id} = {english}");
        assert!(catalog.lines().any(|known| known == line), "{line}");
    }
}

#[test]
fn a_notice_s_text_is_of_one_line_of_its_bound() {
    assert_eq!(notice_text("a\n  b\tc\u{202e}d "), "a b c d");
    let long = notice_text(&"x ".repeat(LONGEST_NOTICE));
    assert_eq!(long.chars().count(), LONGEST_NOTICE);
    assert!(long.ends_with('…'));
    assert!(lokyr_plugin_sdk::host::check_notice(&long, None).is_ok());
}

#[test]
fn hints_shown_again_by_a_call_that_fails_stay_as_they_show() {
    let mut host = screen(&["commit 4482b9e9", "commit 5582b9e9"]);
    let mut links = Links::default();
    host.activate(&mut links).unwrap();
    host.command(&mut links, HINTS, Args::new()).unwrap();
    let ids = |links: &Links| -> Vec<u64> {
        let mut ids: Vec<u64> = links
            .hints
            .iter()
            .flat_map(|shown| shown.hints.iter().filter_map(|hint| hint.decoration))
            .collect();
        ids.sort_unstable();
        ids
    };
    let shown = |host: &FakeHost| -> Vec<u64> {
        let mut ids: Vec<u64> = host.decorations().iter().map(|laid| laid.id).collect();
        ids.sort_unstable();
        ids
    };
    let before = ids(&links);
    assert_eq!(before.len(), 2);
    assert_eq!(shown(&host), before);
    // Of a panel that drew no field yet, the focus fails the call, whose
    // writes go: the plugin keeps the hints that show.
    assert!(host.command(&mut links, HINTS, Args::new()).is_err());
    assert_eq!(ids(&links), before);
    assert_eq!(shown(&host), before);
    // Escape takes them away, every label with them.
    host.close_panel(&PanelId::from(HINTS_PANEL));
    host.deliver(&mut links).unwrap();
    assert!(host.decorations().is_empty());
}

/// Lays `text` out in `pane` of `host` as a logical line from the row
/// numbered `first`, of rows of `columns` characters that wrap into each
/// other, as a read of its rows gives them.
fn wrapped(host: &mut FakeHost, pane: PaneId, first: i64, columns: usize, text: &str) {
    let chars: Vec<char> = text.chars().collect();
    let rows: Vec<String> = chars
        .chunks(columns)
        .map(|row| row.iter().collect())
        .collect();
    let count = rows.len();
    let pane = host.pane(pane);
    for (at, row) in rows.into_iter().enumerate() {
        pane.rows
            .insert(first + i64::try_from(at).unwrap(), (row, at + 1 < count));
    }
}

#[test]
fn a_line_of_the_whole_view_is_read_once_of_no_hint_twice_nor_a_url_cut() {
    // A line of the view's rows of 80 columns, of URLs and hashes across
    // the rows' ends: a view of 60, and one taller than a read reaches,
    // which reads the line twice.
    for rows in [60_u16, 300] {
        let text: String = (0..)
            .map(|at| format!("see https://lokyr.dev/p/{at:05} or c0ffee{at:05}a; "))
            .take_while({
                let mut length = 0;
                move |piece: &String| {
                    length += piece.len();
                    length <= usize::from(rows) * 80
                }
            })
            .collect();
        let mut host = FakeHost::new(NAME).with_manifest(crate::MANIFEST);
        host.focus(Some(PANE));
        let pane = host.pane(PANE);
        pane.grid = (80, rows);
        pane.held = 0..i64::from(rows);
        pane.cursor = (0, i64::from(rows) - 1);
        wrapped(&mut host, PANE, 0, 80, &text);
        let mut links = Links::default();
        host.activate(&mut links).unwrap();
        host.command(&mut links, HINTS, Args::new()).unwrap();
        let shown = links.hints.as_ref().expect("the hints");
        let mut texts: Vec<&str> = shown.hints.iter().map(|hint| hint.text.as_str()).collect();
        texts.sort_unstable();
        let count = texts.len();
        texts.dedup();
        assert_eq!(texts.len(), count, "a hint twice, of {rows} rows");
        // Each URL whole, of its five digits.
        for hint in &shown.hints {
            if let Opens::Url(url) = &hint.opens {
                assert_eq!(url.len(), "https://lokyr.dev/p/00000".len(), "{url}");
            }
        }
        // Every one, but past the labels' most; a label on each, and none
        // on a cell twice.
        let pieces = text.matches("see ").count();
        assert_eq!(count, (2 * pieces).min(MOST_HINTS), "of {rows} rows");
        let mut cells: Vec<(u16, i64)> = host.decorations().iter().map(|laid| laid.from).collect();
        cells.sort_unstable();
        cells.dedup();
        assert_eq!(cells.len(), count);
    }
}

#[test]
fn a_hint_at_the_edge_of_a_read_is_whole_of_the_next_read() {
    // A line of 100 rows of 80 columns, in a view of 10: a read from the
    // view's first row reaches its 10 rows after, past which a URL of the
    // view's last row goes on.
    let mut text = "x".repeat(80 * 9 + 59);
    text.push_str(" https://lokyr.dev/");
    text.push_str(&"a".repeat(80 * 3));
    text.push_str(" 4482b9e9 ");
    text.push_str(&"y".repeat(80 * 87));
    let mut host = FakeHost::new(NAME).with_manifest(crate::MANIFEST);
    host.focus(Some(PANE));
    let pane = host.pane(PANE);
    pane.grid = (80, 10);
    pane.held = 0..100;
    pane.viewport = Some(lokyr_plugin_sdk::host::Viewport {
        top: 0,
        rows: 10,
        screen: 0,
        oldest: 0,
        epoch: 0,
        alternate: false,
    });
    wrapped(&mut host, PANE, 0, 80, &text);
    let mut links = Links::default();
    host.activate(&mut links).unwrap();
    host.take_done();
    host.command(&mut links, HINTS, Args::new()).unwrap();
    // The URL cut at the read's end is read whole by the next read, from
    // its row: one hint, of the whole URL.
    let url = format!("https://lokyr.dev/{}", "a".repeat(80 * 3));
    let shown = links.hints.as_ref().expect("the hints");
    let texts: Vec<&str> = shown.hints.iter().map(|hint| hint.text.as_str()).collect();
    assert_eq!(texts, [url.as_str()]);
    assert_eq!(shown.hints[0].span.0, (60, 9));
}

#[test]
fn a_hint_cut_after_a_full_stop_at_a_read_s_edge_opens_nothing_cut() {
    // A view of 10 rows scrolled back into a line of 100 rows, the URL from
    // its last row on, a `_` the read's last character: the scan ends the
    // URL before it, and the next read, past the view, takes it not.
    let mut text = "x".repeat(80 * 9 + 59);
    text.push_str(" https://lokyr.dev/");
    // The read of rows 0 to 19 ends at the row 19's end.
    let to_edge = 80 * 20 - text.chars().count() - 1;
    text.push_str(&"a".repeat(to_edge));
    text.push('_');
    text.push_str("rest/of/the/url ");
    text.push_str(&"y".repeat(80 * 70));
    let mut host = FakeHost::new(NAME).with_manifest(crate::MANIFEST);
    host.focus(Some(PANE));
    let pane = host.pane(PANE);
    pane.grid = (80, 10);
    pane.held = 0..100;
    pane.viewport = Some(lokyr_plugin_sdk::host::Viewport {
        top: 0,
        rows: 10,
        screen: 0,
        oldest: 0,
        epoch: 0,
        alternate: false,
    });
    wrapped(&mut host, PANE, 0, 80, &text);
    let mut links = Links::default();
    host.activate(&mut links).unwrap();
    host.command(&mut links, HINTS, Args::new()).unwrap();
    let texts: Vec<String> = links
        .hints
        .iter()
        .flat_map(|shown| shown.hints.iter().map(|hint| hint.text.clone()))
        .collect();
    assert!(
        texts.iter().all(|text| text.ends_with("rest/of/the/url")),
        "{texts:?}"
    );
}

#[test]
fn a_hint_across_two_reads_of_a_tall_view_is_found_once() {
    // A view of 300 rows, past a read's reach of 256: a URL from the row
    // 256's end into the row 257, and one before it on the same row.
    let mut text = "x".repeat(80 * 256 + 30);
    text.push_str(" https://lokyr.dev/a ");
    let pad = 80 * 257 - 20 - text.chars().count();
    text.push_str(&"x".repeat(pad));
    text.push_str(" https://lokyr.dev/");
    text.push_str(&"b".repeat(40));
    text.push(' ');
    text.push_str(&"y".repeat(80 * 300 - text.chars().count()));
    let mut host = FakeHost::new(NAME).with_manifest(crate::MANIFEST);
    host.focus(Some(PANE));
    let pane = host.pane(PANE);
    pane.grid = (80, 300);
    pane.held = 0..300;
    pane.viewport = Some(lokyr_plugin_sdk::host::Viewport {
        top: 0,
        rows: 300,
        screen: 0,
        oldest: 0,
        epoch: 0,
        alternate: false,
    });
    wrapped(&mut host, PANE, 0, 80, &text);
    let mut links = Links::default();
    host.activate(&mut links).unwrap();
    host.command(&mut links, HINTS, Args::new()).unwrap();
    let mut texts: Vec<String> = links
        .hints
        .iter()
        .flat_map(|shown| shown.hints.iter().map(|hint| hint.text.clone()))
        .collect();
    texts.sort();
    let long = format!("https://lokyr.dev/{}", "b".repeat(40));
    assert_eq!(texts, ["https://lokyr.dev/a".to_owned(), long]);
}
