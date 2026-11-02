//! The plugin against a host of the tests', with a line, its link and the
//! working directory of the test's own, which records what it opens.

use lokyr_plugin_sdk::host::{LineText, PaneId, WindowId};
use lokyr_plugin_sdk::message::{Call, Request, Response};

use super::*;

/// A host whose focused pane's view has one line, `line`, from its row 1,
/// a cell each character, which wraps to the next row after ten, or in
/// `cells`, with `link` at every cell; every target is there but those of
/// `missing`.
#[derive(Default)]
struct Host {
    focused: bool,
    line: &'static str,
    cells: Option<Vec<(u16, i64)>>,
    link: Option<&'static str>,
    /// The targets that are not there, which open nothing.
    missing: Vec<&'static str>,
    directory: Option<&'static str>,
    commands: Vec<Command>,
    bindings: Vec<Binding>,
    subscribed: Vec<String>,
    opened: Vec<String>,
    /// The decorations that the plugin laid and has not taken away, by
    /// their ids, with their cells.
    decorations: Vec<(u64, Span)>,
    /// How many decorations it laid.
    laid: u64,
}

/// The line's columns.
const COLUMNS: u16 = 10;

impl Host {
    /// The line's text, by its row's number, as the host's reads give it.
    fn line(&self, pane: PaneId, row: i64, reach: usize) -> Option<LineText> {
        assert_eq!((pane, reach), (PaneId(4), super::REACH));
        if !(1..=2).contains(&row) {
            return None;
        }
        let cells = self.cells.clone().unwrap_or_else(|| {
            (0..self.line.chars().count())
                .map(|at| {
                    let at = u16::try_from(at).unwrap();
                    (at % COLUMNS, i64::from(1 + at / COLUMNS))
                })
                .collect()
        });
        Some(LineText {
            text: self.line.to_owned(),
            cells,
        })
    }
}

impl lokyr_plugin_sdk::host::Host for Host {
    fn handle(&mut self, call: Call) -> Response {
        assert_eq!(call.caller, "links");
        let text = |text: Option<&str>| Response::Text(text.map(str::to_owned));
        match call.request {
            Request::RegisterCommand(command) => {
                self.commands.push(command);
                Response::Done
            }
            Request::Bind(binding) => {
                self.bindings.push(binding);
                Response::Done
            }
            Request::Subscribe(names) => {
                self.subscribed.extend(names);
                Response::Done
            }
            Request::Decorate {
                pane,
                from,
                to,
                look,
            } => {
                assert_eq!((pane, look), (PaneId(4), UNDERLINED));
                self.laid += 1;
                self.decorations.push((self.laid, (from, to)));
                Response::Decoration(self.laid)
            }
            Request::Undecorate { pane, id } => {
                assert_eq!(pane, PaneId(4));
                self.decorations.retain(|&(laid, _)| laid != id);
                Response::Done
            }
            Request::FocusedWindow => Response::Window(self.focused.then_some(WindowId(1))),
            Request::FocusedPane => Response::Pane(self.focused.then_some(PaneId(4))),
            Request::LineText { pane, row, reach } => Response::Line(self.line(pane, row, reach)),
            Request::LinkAt { .. } => text(self.link),
            Request::WorkingDirectory(_) => text(self.directory),
            Request::Open(target) if target.starts_with("javascript:") => {
                Response::Failed(Error::new(Code::Failed, "refused"))
            }
            Request::Open(target) => {
                self.opened.push(target);
                Response::Done
            }
            Request::Opens(target) => Response::Boolean(
                !target.starts_with("javascript:") && !self.missing.contains(&target.as_str()),
            ),
            other => panic!("links asked {other:?}"),
        }
    }
}

/// A host whose focused pane has `line`.
fn focused(line: &'static str) -> Host {
    Host {
        focused: true,
        line,
        directory: Some("/work/lokyr"),
        ..Host::default()
    }
}

/// The arguments of a click on the cell at `column` of `row`.
fn cell(column: i64, row: i64) -> Args {
    Args::from([
        ("column".to_owned(), Value::Integer(column)),
        ("row".to_owned(), Value::Integer(row)),
    ])
}

/// Clicks `host`'s cell at `column` of `row`.
fn click(host: &mut Host, column: i64, row: i64) -> Result<(), Error> {
    Links::default().on_command(&mut Cx::new(host, "links"), OPEN, cell(column, row))
}

/// Whether a click on `host`'s cell at `column` of `row` would open
/// something, which opens nothing.
fn applies(host: &mut Host, column: i64, row: i64) -> bool {
    let opened = host.opened.len();
    let applies = Links::default().applies(&mut Cx::new(host, "links"), OPEN, &cell(column, row));
    assert_eq!(host.opened.len(), opened);
    applies
}

#[test]
fn the_command_registers_with_cmd_click() {
    let mut host = Host::default();
    Links::default()
        .activate(&mut Cx::new(&mut host, "links"))
        .unwrap();
    let [command] = &host.commands[..] else {
        panic!("one command: {:?}", host.commands);
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
    assert_eq!(
        host.bindings,
        [Binding {
            keys: "cmd+click".to_owned(),
            command: "links.open".to_owned(),
            args: Args::new(),
            when: None,
        }]
    );
    assert_eq!(host.subscribed, ["pointer"]);
}

/// The pointer at the cell of `column` and `row`, or out of the pane, with
/// Command held or not.
fn pointer(at: Option<(u16, i64)>, command: bool) -> Event {
    Event::Pointer {
        pane: PaneId(4),
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
    host.missing = vec!["/work/lokyr/see"];
    let mut links = Links::default();
    let mut hover = |host: &mut Host, events: &[Event]| {
        links.on_event(&mut Cx::new(host, "links"), events).unwrap();
        host.decorations
            .iter()
            .map(|&(_, span)| span)
            .collect::<Vec<_>>()
    };
    let url = ((4, 1), (2, 3));
    // Without Command, nothing; with it, the URL.
    assert!(hover(&mut host, &[pointer(Some((6, 1)), false)]).is_empty());
    assert_eq!(hover(&mut host, &[pointer(Some((6, 1)), true)]), [url]);
    // Over another of its cells, the same underline, not laid again.
    assert_eq!(hover(&mut host, &[pointer(Some((9, 1)), true)]), [url]);
    assert_eq!(host.laid, 1);
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
    let mut host = Host {
        link: Some("https://lokyr.dev/osc8"),
        ..focused("the docs")
    };
    let mut links = Links::default();
    links
        .on_event(
            &mut Cx::new(&mut host, "links"),
            &[pointer(Some((5, 1)), true)],
        )
        .unwrap();
    assert_eq!(host.decorations[0].1, ((0, 1), (7, 1)));
    // A path with its line and column, as `grep -n` writes it: the path
    // alone.
    let mut host = focused("at src/main.rs:42:7 now");
    let mut links = Links::default();
    links
        .on_event(
            &mut Cx::new(&mut host, "links"),
            &[pointer(Some((5, 1)), true)],
        )
        .unwrap();
    assert_eq!(host.decorations[0].1, ((3, 1), (3, 2)));
}

#[test]
fn a_path_of_escaped_spaces_opens_and_command_underlines_it_whole() {
    // As a shell's completion writes it: `My\ File.txt`, over two rows.
    let mut host = focused("open My\\ File.txt");
    assert!(applies(&mut host, 6, 1));
    click(&mut host, 6, 1).unwrap();
    assert_eq!(host.opened, ["/work/lokyr/My File.txt"]);
    let mut links = Links::default();
    links
        .on_event(
            &mut Cx::new(&mut host, "links"),
            &[pointer(Some((6, 1)), true)],
        )
        .unwrap();
    assert_eq!(host.decorations[0].1, ((5, 1), (6, 2)));
}

#[test]
fn a_click_opens_the_cell_s_link_first() {
    let mut host = Host {
        link: Some("https://lokyr.dev/osc8"),
        ..focused("see https://lokyr.dev/text")
    };
    assert!(applies(&mut host, 0, 1));
    click(&mut host, 0, 1).unwrap();
    assert_eq!(host.opened, ["https://lokyr.dev/osc8"]);
    // A target that does not open fails as the host says.
    host.link = Some("javascript:x");
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
        assert_eq!(host.opened, [opened], "{line}");
    }
    // A wide character covers two cells, and the characters after it are
    // a cell further.
    let mut host = Host {
        cells: Some(vec![(0, 1), (2, 1), (3, 1), (4, 1), (5, 1)]),
        ..focused("🙂 a/b")
    };
    for column in [1, 2] {
        assert!(!applies(&mut host, column, 1));
        click(&mut host, column, 1).unwrap();
    }
    assert!(host.opened.is_empty());
    assert!(applies(&mut host, 3, 1));
    click(&mut host, 3, 1).unwrap();
    assert_eq!(host.opened, ["/work/lokyr/a/b"]);
    // A click on a wide character's second cell.
    let mut host = Host {
        cells: Some(vec![(0, 1), (2, 1), (4, 1), (5, 1)]),
        ..focused("日本/x")
    };
    click(&mut host, 1, 1).unwrap();
    assert_eq!(host.opened, ["/work/lokyr/日本/x"]);
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
    Links::default()
        .on_command(&mut Cx::new(&mut host, "links"), OPEN, Args::new())
        .unwrap();
    // A relative path without a directory, as after the program ended.
    let mut ended = Host {
        directory: None,
        ..focused("README.md")
    };
    assert!(!applies(&mut ended, 2, 1));
    click(&mut ended, 2, 1).unwrap();
    // Without a focused pane.
    let mut unfocused = Host {
        focused: false,
        ..focused("/etc/hosts")
    };
    assert!(!applies(&mut unfocused, 2, 1));
    click(&mut unfocused, 2, 1).unwrap();
    assert!(host.opened.is_empty() && ended.opened.is_empty() && unfocused.opened.is_empty());
    assert_eq!(
        Links::default()
            .on_command(&mut Cx::new(&mut host, "links"), "links.other", Args::new())
            .unwrap_err()
            .code,
        Code::NotFound
    );
    // A path that is not there, which a click tries all the same.
    let mut missing = Host {
        missing: vec!["/work/lokyr/README.md"],
        ..focused("README.md")
    };
    assert!(!applies(&mut missing, 2, 1));
    // Another command, at a cell with a path.
    let mut path = focused("/etc/hosts");
    assert!(!Links::default().applies(
        &mut Cx::new(&mut path, "links"),
        "links.other",
        &cell(2, 1)
    ));
}

#[test]
fn a_quoted_sentence_opens_the_url_or_the_path_in_it() {
    /// A host whose line `line` is on the view's row 1 alone, the targets
    /// of `missing` not there.
    fn one_row(line: &'static str, missing: Vec<&'static str>) -> Host {
        let cells = (0..line.chars().count())
            .map(|at| (u16::try_from(at).unwrap(), 1))
            .collect();
        Host {
            cells: Some(cells),
            missing,
            ..focused(line)
        }
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
        let mut host = one_row(line, missing);
        let column = i64::try_from(column).unwrap();
        assert_eq!(applies(&mut host, column, 1), there, "{line}");
        click(&mut host, column, 1).unwrap();
        assert_eq!(host.opened, [opened], "{line}");
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
