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
    let mut host = FakeHost::new(NAME);
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

/// The targets that `host`'s plugin opened since the last look.
fn opened(host: &mut FakeHost) -> Vec<String> {
    host.take_done()
        .into_iter()
        .filter_map(|request| match request {
            Request::Open(target) => Some(target),
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
            assert_eq!((decoration.pane, decoration.look), (PANE, UNDERLINED));
            (decoration.from, decoration.to)
        })
        .collect()
}

#[test]
fn the_command_registers_with_cmd_click() {
    let mut host = FakeHost::new(NAME);
    host.activate(&mut Links::default()).unwrap();
    let [command] = host.commands() else {
        panic!("one command: {:?}", host.commands());
    };
    assert_eq!(
        (command.name.as_str(), command.category),
        ("links.open", Category::View)
    );
    let capabilities: Vec<String> = command
        .capabilities
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(capabilities, ["terminal-read", "open", "process-read"]);
    let params: Vec<(&str, Kind, Option<&Value>)> = command
        .params
        .iter()
        .map(|param| (param.name.as_str(), param.kind, param.default.as_ref()))
        .collect();
    assert_eq!(
        params,
        [
            ("column", Kind::Integer, None),
            ("row", Kind::Integer, None)
        ]
    );
    // The system's keymap binds its click.
    assert!(host.bindings().is_empty());
    assert_eq!(host.subscriptions(), ["pointer"]);
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
fn command_underlines_an_osc_8_link_s_cells_and_a_path_without_its_line() {
    // An OSC 8 link over the whole line, as the host gives one.
    let mut host = linked(focused("the docs"), "https://lokyr.dev/osc8");
    let mut links = Links::default();
    host.events(&mut links, &[pointer(Some((5, 1)), true)])
        .unwrap();
    assert_eq!(underlined(&host), [((0, 1), (7, 1))]);
    // A path with its line and column, as `grep -n` writes it: the path
    // alone.
    let mut host = focused("at src/main.rs:42:7 now");
    let mut links = Links::default();
    host.events(&mut links, &[pointer(Some((5, 1)), true)])
        .unwrap();
    assert_eq!(underlined(&host), [((3, 1), (3, 2))]);
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
        ("--> src/main.rs:42:7", (8, 1), "/work/lokyr/src/main.rs"),
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
