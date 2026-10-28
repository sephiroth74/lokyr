//! The plugin against a host of the tests', with a line, its link and the
//! working directory of the test's own, which records what it opens.

use lokyr_plugin_sdk::host::{HostApi, LineText, PaneId, Scroll, WindowId};

use super::*;

/// A host whose focused pane's view has one line, `line`, from its row 1,
/// a cell each character, which wraps to the next row after ten, or in
/// `cells`, with `link` at every cell.
#[derive(Default)]
struct Host {
    focused: bool,
    line: &'static str,
    cells: Option<Vec<(u16, i64)>>,
    link: Option<&'static str>,
    directory: Option<&'static str>,
    commands: Vec<Command>,
    bindings: Vec<Binding>,
    opened: Vec<String>,
}

/// The line's columns.
const COLUMNS: u16 = 10;

impl HostApi for Host {
    fn register_command(&mut self, command: Command) -> Result<(), Error> {
        self.commands.push(command);
        Ok(())
    }

    fn run_command(&mut self, _: &str, _: Args) -> Result<(), Error> {
        Ok(())
    }

    fn bind(&mut self, binding: Binding) -> Result<(), Error> {
        self.bindings.push(binding);
        Ok(())
    }

    fn clipboard_text(&mut self) -> Result<Option<String>, Error> {
        Ok(None)
    }

    fn set_clipboard_text(&mut self, _: &str) -> Result<(), Error> {
        Ok(())
    }

    fn focused_window(&self) -> Option<WindowId> {
        self.focused.then_some(WindowId(1))
    }

    fn focused_pane(&self) -> Option<PaneId> {
        self.focused.then_some(PaneId(4))
    }

    fn selection_text(&mut self, _: PaneId) -> Result<Option<String>, Error> {
        Ok(None)
    }

    fn line_text(&mut self, pane: PaneId, row: u16) -> Result<Option<LineText>, Error> {
        assert_eq!(pane, PaneId(4));
        if !(1..=2).contains(&row) {
            return Ok(None);
        }
        let cells = self.cells.clone().unwrap_or_else(|| {
            (0..self.line.chars().count())
                .map(|at| {
                    let at = u16::try_from(at).unwrap();
                    (at % COLUMNS, i64::from(1 + at / COLUMNS))
                })
                .collect()
        });
        Ok(Some(LineText {
            text: self.line.to_owned(),
            cells,
        }))
    }

    fn link_at(&mut self, _: PaneId, _: u16, _: u16) -> Result<Option<String>, Error> {
        Ok(self.link.map(str::to_owned))
    }

    fn working_directory(&mut self, _: PaneId) -> Result<Option<String>, Error> {
        Ok(self.directory.map(str::to_owned))
    }

    fn open(&mut self, target: &str) -> Result<(), Error> {
        if target.starts_with("javascript:") {
            return Err(Error::new(Code::Failed, "refused"));
        }
        self.opened.push(target.to_owned());
        Ok(())
    }

    fn select_all(&mut self, _: PaneId) -> Result<(), Error> {
        Ok(())
    }

    fn clear_scrollback(&mut self, _: PaneId) -> Result<(), Error> {
        Ok(())
    }

    fn clear(&mut self, _: PaneId) -> Result<(), Error> {
        Ok(())
    }

    fn paste(&mut self, _: PaneId, _: &str) -> Result<(), Error> {
        Ok(())
    }

    fn send_text(&mut self, _: PaneId, _: &str) -> Result<(), Error> {
        Ok(())
    }

    fn scroll(&mut self, _: PaneId, _: Scroll) -> Result<(), Error> {
        Ok(())
    }

    fn font_size(&self) -> f32 {
        13.0
    }

    fn set_font_size(&mut self, _: Option<f32>) -> Result<(), Error> {
        Ok(())
    }

    fn open_window(&mut self) -> Result<(), Error> {
        Ok(())
    }

    fn close_window(&mut self, _: WindowId) -> Result<(), Error> {
        Ok(())
    }

    fn quit(&mut self) -> Result<(), Error> {
        Ok(())
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

/// Clicks `host`'s cell at `column` of `row`.
fn click(host: &mut Host, column: i64, row: i64) -> Result<(), Error> {
    let args = Args::from([
        ("column".to_owned(), Value::Integer(column)),
        ("row".to_owned(), Value::Integer(row)),
    ]);
    Links.on_command(&mut Cx::new(host), OPEN, args)
}

#[test]
fn the_command_registers_with_cmd_click() {
    let mut host = Host::default();
    Links.activate(&mut Cx::new(&mut host)).unwrap();
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
}

#[test]
fn a_click_opens_the_cell_s_link_first() {
    let mut host = Host {
        link: Some("https://lokyr.dev/osc8"),
        ..focused("see https://lokyr.dev/text")
    };
    click(&mut host, 0, 1).unwrap();
    assert_eq!(host.opened, ["https://lokyr.dev/osc8"]);
    // A target that does not open fails as the host says.
    host.link = Some("javascript:x");
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
        click(&mut host, column, 1).unwrap();
    }
    assert!(host.opened.is_empty());
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
        click(&mut host, column, row).unwrap();
    }
    // Cells that no view has, and arguments that the registry would have
    // refused.
    click(&mut host, -1, 1).unwrap();
    click(&mut host, 0, 70_000).unwrap();
    Links
        .on_command(&mut Cx::new(&mut host), OPEN, Args::new())
        .unwrap();
    // A relative path without a directory, as after the program ended.
    let mut ended = Host {
        directory: None,
        ..focused("README.md")
    };
    click(&mut ended, 2, 1).unwrap();
    // Without a focused pane.
    let mut unfocused = Host {
        focused: false,
        ..focused("/etc/hosts")
    };
    click(&mut unfocused, 2, 1).unwrap();
    assert!(host.opened.is_empty() && ended.opened.is_empty() && unfocused.opened.is_empty());
    assert_eq!(
        Links
            .on_command(&mut Cx::new(&mut host), "links.other", Args::new())
            .unwrap_err()
            .code,
        Code::NotFound
    );
}

#[test]
fn a_path_loses_its_dots_and_its_empty_parts() {
    for (path, normal) in [
        ("/a/./b//c/", "/a/b/c"),
        ("/a/../../b", "/b"),
        ("~/x/../y", "~/y"),
        ("~", "~"),
        ("~/..", "~/"),
        ("a/./b", "a/b"),
    ] {
        assert_eq!(super::normal(path), normal, "{path}");
    }
}
