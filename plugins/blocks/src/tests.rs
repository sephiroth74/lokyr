//! The plugin against the SDK's fake host, whose pane holds three commands
//! and tells the views that the test gives. The header's own behavior is
//! `lokyr/command-header`'s tests'; these test what `lokyr/blocks` adds:
//! the button to a block's end, and the plugin's replacing of the plain
//! header, of the same keys.

use lokyr_plugin_sdk::command::{Args, Category, Value};
use lokyr_plugin_sdk::fake::FakeHost;
use lokyr_plugin_sdk::host::{PaneId, Scroll, ShellCommand, TabId, Viewport};
use lokyr_plugin_sdk::message::{Data, Request};
use lokyr_plugin_sdk::ui::{PanelId, UiEvent, ViewId};
use lokyr_plugin_sdk::view::{Key, Kind, Length, Node, Token, UiEventKind};

use super::header::{BETWEEN, END, LEAD, PILL};
use super::*;

/// The plugin's name, as its calls give it.
const NAME: &str = "lokyr/blocks";

/// The pane of the tests.
const PANE: PaneId = PaneId(1);

/// Three commands, `ls -l`, `make` and `seq 100`, of their prompts on the
/// rows 0, 30 and 60, their outputs on the rows below, the last running.
fn commands() -> Vec<ShellCommand> {
    let command = |id: u64, row: i64, end: Option<i64>, status: Option<i32>| ShellCommand {
        id,
        prompt: Some((0, row)),
        input: Some((2, row)),
        output: Some((0, row + 1)),
        end: end.map(|end| (0, end)),
        status,
        ..ShellCommand::default()
    };
    vec![
        command(1, 0, Some(30), Some(0)),
        command(2, 30, Some(60), Some(1)),
        command(3, 60, None, None),
    ]
}

/// The commands of a command's output, as `lokyr/core-actions` registers
/// them, which the pane's context menu's submenu of a block's actions
/// holds.
fn outputs() -> Vec<lokyr_plugin_sdk::command::Command> {
    use lokyr_plugin_sdk::command::{Capability, Command, Kind, Param};
    let cell = |name: &str| Param {
        name: name.to_owned(),
        kind: Kind::Integer,
        default: Some(Value::Integer(-1)),
    };
    [
        (COPY_OUTPUT, "Copy Command Output", Category::Clipboard),
        (SELECT_OUTPUT, "Select Command Output", Category::Edit),
    ]
    .into_iter()
    .map(|(name, title, category)| Command {
        name: name.to_owned(),
        title: title.to_owned(),
        category,
        capabilities: vec![Capability::HistoryRead],
        params: vec![cell("column"), cell("row")],
        when: Some(PANE_FOCUSED.to_owned()),
    })
    .collect()
}

/// The context of the commands of a command's output.
const PANE_FOCUSED: &str = "pane.focused";

/// A host of a pane of 100 rows and the three commands, of the end
/// button's texts, and the plugin, activated, of the pill's header.
fn host() -> (FakeHost, Blocks) {
    let mut host = FakeHost::new(NAME)
        .with_manifest(crate::MANIFEST)
        .with_commands(outputs());
    host.set_config(Data::Map(
        [("header".to_owned(), Data::Text("pill".to_owned()))].into(),
    ));
    let pane = host.pane(PANE);
    pane.tab = TabId(1);
    pane.held = 0..100;
    pane.commands = commands();
    let lines = [(1, "ls -l"), (2, "make"), (3, "seq 100")];
    for (id, line) in lines {
        pane.command_lines.insert(id, line.to_owned());
    }
    for (_, line) in lines {
        host.set_message(
            "header-end",
            &[("line", Data::Text(line.to_owned()))],
            Some(&format!("Scroll to the end of {line}")),
        );
    }
    let mut plugin = Blocks::default();
    host.activate(&mut plugin).unwrap();
    (host, plugin)
}

/// The id of `pane`'s header's panel.
fn panel(pane: PaneId) -> PanelId {
    PanelId(format!("{PANEL}{}", pane.0))
}

/// Tells the plugin of a view of 24 rows from `top`, of the screen from
/// the row 76, and returns the header that the pane shows then, if one.
fn scrolled(host: &mut FakeHost, plugin: &mut Blocks, top: i64) -> Option<Node> {
    host.post(Event::ViewportChanged {
        pane: PANE,
        viewport: Viewport {
            top,
            rows: 24,
            screen: 76,
            oldest: 0,
            epoch: 1,
            alternate: false,
            folds: Vec::new(),
        },
    });
    host.deliver(plugin).unwrap();
    host.panels().get(&panel(PANE))?;
    host.render(plugin, &ViewId::Panel(panel(PANE)))
}

/// The keys of a header's children, of its hairlines' widths: `lead` of
/// the short one at the pill's side, `rest` of the one that grows, and
/// `between` of the one between the pill and the button to the end.
fn beads(header: &Node) -> Vec<String> {
    header
        .children
        .iter()
        .map(|child| match (&child.key, child.layout.width) {
            (Some(key), _) => key.0.clone(),
            (None, Length::Points(points)) if points == LEAD => "lead".to_owned(),
            (None, Length::Points(points)) if points == BETWEEN => "between".to_owned(),
            (None, _) => "rest".to_owned(),
        })
        .collect()
}

fn click(key: &str) -> UiEvent {
    UiEvent {
        view: ViewId::Panel(panel(PANE)),
        key: Key::from(key),
        kind: UiEventKind::Click,
    }
}

#[test]
fn the_header_shows_as_the_plain_one_and_its_click_scrolls_to_the_prompt() {
    let (mut host, mut plugin) = host();
    assert!(scrolled(&mut host, &mut plugin, 0).is_none());
    // Its end in the view: the plain header's pill alone.
    let header = scrolled(&mut host, &mut plugin, 40).expect("a header");
    assert_eq!(beads(&header), ["lead", PILL, "rest"]);
    let Kind::Text(line) = &header.children[1].children[0].kind else {
        panic!("{header:?}");
    };
    assert_eq!(line, "make");
    host.take_done();
    host.ui_event(&mut plugin, &click(PILL)).unwrap();
    assert_eq!(
        host.take_done(),
        [Request::Scroll {
            pane: PANE,
            scroll: Scroll::Row(30),
        }]
    );
    // A click on a button to the end that does not show does nothing.
    host.ui_event(&mut plugin, &click(END)).unwrap();
    assert!(host.take_done().is_empty());
}

#[test]
fn a_button_beside_the_pill_scrolls_to_the_block_s_end_while_the_view_does_not_show_it() {
    let (mut host, mut plugin) = host();
    // `ls -l`'s output ends on the row 29, below a view of the rows 2 to
    // 25: the button shows, of its tooltip and label, the pill's look.
    let header = scrolled(&mut host, &mut plugin, 2).expect("a header");
    assert_eq!(beads(&header), ["lead", PILL, "between", END, "rest"]);
    let (pill, end) = (&header.children[1], &header.children[3]);
    assert_eq!(end.children[0].kind, Kind::Icon("arrow-down".to_owned()));
    assert_eq!(end.tooltip.as_deref(), Some("Scroll to the end of ls -l"));
    assert_eq!(end.label.as_deref(), Some("Scroll to the end of ls -l"));
    assert_eq!(
        (&end.style.background, &end.style.border, &end.style.hover),
        (
            &pill.style.background,
            &pill.style.border,
            &pill.style.hover
        )
    );
    assert_eq!(end.style.radius, pill.style.radius);
    // Between them, the hairline's own color.
    assert_eq!(
        header.children[2].style.background,
        Some(Token::from("panel.border"))
    );
    // Its click puts the block's last row at the view's bottom.
    host.take_done();
    host.ui_event(&mut plugin, &click(END)).unwrap();
    assert_eq!(
        host.take_done(),
        [Request::Scroll {
            pane: PANE,
            scroll: Scroll::Row(6),
        }]
    );
    // The view moving within the same block asks no render; to its end,
    // the button goes, of a render.
    host.take_renders();
    assert!(scrolled(&mut host, &mut plugin, 3).is_some());
    host.take_renders();
    scrolled(&mut host, &mut plugin, 4);
    assert!(host.take_renders().is_empty());
    let header = scrolled(&mut host, &mut plugin, 6).expect("a header");
    assert_eq!(host.take_renders(), [ViewId::Panel(panel(PANE))]);
    assert_eq!(beads(&header), ["lead", PILL, "rest"]);
    // An end within its row, of an output of no last new line, is the
    // block's last row.
    host.pane(PANE).commands[0].end = Some((5, 29));
    let header = scrolled(&mut host, &mut plugin, 5).expect("a header");
    assert_eq!(beads(&header), ["lead", PILL, "between", END, "rest"]);
    host.take_done();
    host.ui_event(&mut plugin, &click(END)).unwrap();
    assert_eq!(
        host.take_done(),
        [Request::Scroll {
            pane: PANE,
            scroll: Scroll::Row(6),
        }]
    );
}

#[test]
fn a_command_of_several_lines_heads_its_block_on_one() {
    let (mut host, mut plugin) = host();
    host.pane(PANE)
        .command_lines
        .insert(2, "for i in 1 2\ndo make\ndone".to_owned());
    let header = scrolled(&mut host, &mut plugin, 40).expect("a header");
    let Kind::Text(line) = &header.children[1].children[0].kind else {
        panic!("{header:?}");
    };
    assert_eq!(line, "for i in 1 2 do make done");
}

#[test]
fn a_running_command_s_end_is_the_screen_s_which_the_view_then_follows() {
    let (mut host, mut plugin) = host();
    let header = scrolled(&mut host, &mut plugin, 62).expect("a header");
    assert_eq!(beads(&header), ["lead", PILL, "between", END, "rest"]);
    host.take_done();
    host.ui_event(&mut plugin, &click(END)).unwrap();
    assert_eq!(
        host.take_done(),
        [Request::Scroll {
            pane: PANE,
            scroll: Scroll::Bottom,
        }]
    );
    // At the screen, it shows no button.
    let header = scrolled(&mut host, &mut plugin, 76).expect("a header");
    assert_eq!(beads(&header), ["lead", PILL, "rest"]);
}

#[test]
fn the_button_follows_the_pill_wherever_it_sits() {
    let (mut host, mut plugin) = host();
    let align = |value: &str| {
        Data::Map(
            [
                ("align".to_owned(), Data::Text(value.to_owned())),
                ("header".to_owned(), Data::Text("pill".to_owned())),
            ]
            .into_iter()
            .collect(),
        )
    };
    host.set_config(align("right"));
    host.deliver(&mut plugin).unwrap();
    let header = scrolled(&mut host, &mut plugin, 2).expect("a header");
    assert_eq!(beads(&header), ["rest", PILL, "between", END, "lead"]);
    host.set_config(align("center"));
    host.deliver(&mut plugin).unwrap();
    let header = host
        .render(&mut plugin, &ViewId::Panel(panel(PANE)))
        .expect("a header");
    assert_eq!(beads(&header), ["rest", PILL, "between", END, "rest"]);
}

#[test]
fn it_replaces_the_plain_header_of_the_same_keys() {
    assert!(
        crate::MANIFEST.contains("\nreplaces = [\"lokyr/command-header\"]\n"),
        "{}",
        crate::MANIFEST
    );
    // The same keys, of the same defaults, bounds and texts, then those of
    // the blocks' own looks.
    let keys = |schema: &'static str| {
        schema
            .split_once("\n  \"properties\": {\n")
            .and_then(|(_, rest)| rest.split_once("\n  },\n  \"x-guide\": "))
            .map(|(keys, _)| keys)
            .expect("the schema's keys")
    };
    let header = keys(include_str!(
        "../../command-header/command-header.schema.json"
    ));
    let own = keys(include_str!("../blocks.schema.json"));
    assert!(own.starts_with(&format!("{header},\n")), "{own}");
    assert!(own.contains("\n    \"dividers\": {\n") && own.contains("\n    \"failed-tint\": {\n"));
}

#[test]
fn the_guide_tells_every_key_of_the_schema_in_its_example_and_its_section() {
    let guide = include_str!("../../../docs/CONFIGURATION.md");
    let schema = include_str!("../blocks.schema.json");
    let keys: Vec<&str> = schema
        .lines()
        .filter_map(|line| line.strip_prefix("    \"")?.split_once("\": {"))
        .map(|(key, _)| key)
        .collect();
    assert_eq!(keys.len(), 15, "{keys:?}");
    let example = guide
        .split_once("\n[plugins.blocks]\n")
        .and_then(|(_, rest)| rest.split_once("\n\n"))
        .map(|(example, _)| example)
        .expect("the complete example's section");
    let section = guide
        .split_once("\n## `[plugins.blocks]`\n")
        .map(|(_, rest)| {
            rest.split_once("\n## ")
                .map_or(rest, |(section, _)| section)
        })
        .expect("the guide's section");
    for key in keys {
        assert!(
            example
                .lines()
                .any(|line| line.starts_with(&format!("{key} = "))),
            "`{key}` in the example"
        );
        assert!(
            section
                .lines()
                .filter(|line| line.starts_with("### "))
                .any(|line| line.contains(&format!("`{key}`"))),
            "`{key}` in the section"
        );
    }
}

#[test]
fn the_texts_are_in_both_languages() {
    let details = [
        "details-label",
        "details-line",
        "details-status",
        "details-started",
        "details-duration",
        "details-folder",
        "details-copy-line",
        "details-copy-status",
        "details-copy-started",
        "details-copy-duration",
        "details-copy-folder",
        "details-running",
        "details-succeeded",
        "details-failed",
        "details-ended",
        "details-milliseconds",
        "details-seconds",
        "details-minutes",
        "details-hours",
        "rerun-title",
        "rerun-no-line",
        "rerun-busy",
        "rerun-typed",
    ];
    for (_, catalog) in CATALOGS {
        for id in ["header-label", "header-tooltip", "header-end"]
            .into_iter()
            .chain(details)
        {
            assert!(catalog.contains(&format!("\n{id} = ")), "{id}");
        }
    }
}

/// A host of a focused pane 1 of 20 columns and 12 rows, whose shell ran
/// `ls`, of two rows of output, `true`, of none, and `sleep 60`, which
/// runs: their prompts on the rows 0, 4 and 6, their lines as the host
/// reads them, `$ ` and the line on each prompt's row.
fn commands_host() -> (FakeHost, Blocks) {
    let mut host = FakeHost::new(NAME)
        .with_manifest(crate::MANIFEST)
        .with_commands(outputs());
    host.focus(Some(PANE));
    let pane = host.pane(PANE);
    pane.tab = TabId(1);
    pane.held = 0..12;
    pane.grid = (20, 12);
    for (row, text) in [
        (0, "$ ls"),
        (1, "a.txt"),
        (2, "b.txt"),
        (3, ""),
        (4, "$ true"),
        (5, "$ sleep 60"),
        (6, ""),
    ] {
        pane.rows.insert(row, (text.to_owned(), false));
    }
    let command = |id: u64, row: i64, output: i64, end: Option<i64>| ShellCommand {
        id,
        prompt: Some((0, row)),
        input: Some((2, row)),
        output: Some((0, output)),
        end: end.map(|end| (0, end)),
        status: end.map(|_| 0),
        ..ShellCommand::default()
    };
    // `ls`' output ends of a blank row; `true`'s end is the next prompt's
    // row, which its output's mark shares.
    pane.commands = vec![
        command(1, 0, 1, Some(4)),
        command(2, 4, 5, Some(5)),
        command(3, 5, 6, None),
    ];
    for (id, line) in [(1, "ls"), (2, "true"), (3, "sleep 60")] {
        pane.command_lines.insert(id, line.to_owned());
    }
    let mut plugin = Blocks::default();
    host.activate(&mut plugin).unwrap();
    (host, plugin)
}

/// The arguments of the cell of `column` and `row`.
fn cell(column: i64, row: i64) -> Args {
    Args::from([
        ("column".to_owned(), Value::Integer(column)),
        ("row".to_owned(), Value::Integer(row)),
    ])
}

/// Runs `name` of `args`, its events delivered, and returns its requests.
fn run(host: &mut FakeHost, plugin: &mut Blocks, name: &str, args: Args) -> Vec<Request> {
    host.take_done();
    host.set_clipboard(None);
    host.command(plugin, name, args).unwrap();
    host.deliver(plugin).unwrap();
    host.take_done()
}

/// The cells that a run of `select-block` of `args` selects.
fn selected_cells(host: &mut FakeHost, plugin: &mut Blocks, args: Args) -> Vec<Request> {
    run(host, plugin, SELECT_BLOCK, args)
        .into_iter()
        .filter(|request| matches!(request, Request::SelectCells { .. }))
        .collect()
}

fn selection(first: (u16, i64), last: (u16, i64)) -> Request {
    Request::SelectCells {
        pane: PANE,
        first,
        last,
        kind: lokyr_plugin_sdk::host::SelectionKind::Cells,
    }
}

#[test]
fn the_commands_register_of_a_cell_in_the_clipboard_s_and_the_edit_s_categories() {
    let (host, _) = commands_host();
    // The block's selection's, of no cell, in the view's category.
    let (of_cells, selecting) = host.commands().split_at(5);
    let selection: Vec<(&str, Category, usize)> = selecting
        .iter()
        .map(|command| {
            assert_eq!(command.when.as_deref(), Some(super::PANE));
            (
                command.name.as_str(),
                command.category,
                command.params.len(),
            )
        })
        .collect();
    assert_eq!(
        selection,
        [
            (SELECT_PREVIOUS, Category::View, 0),
            (SELECT_NEXT, Category::View, 0),
            (DESELECT, Category::View, 0),
        ]
    );
    let registered: Vec<(&str, Category, Vec<String>)> = of_cells
        .iter()
        .map(|command| {
            let params: Vec<&str> = command
                .params
                .iter()
                .map(|param| param.name.as_str())
                .collect();
            assert_eq!(params, ["column", "row"], "{}", command.name);
            assert_eq!(command.when.as_deref(), Some(super::PANE));
            let capabilities = command.capabilities.iter().map(ToString::to_string);
            (
                command.name.as_str(),
                command.category,
                capabilities.collect(),
            )
        })
        .collect();
    let strings = |names: &[&str]| names.iter().map(|name| (*name).to_owned()).collect();
    assert_eq!(
        registered,
        [
            (
                COPY_COMMAND,
                Category::Clipboard,
                strings(&["history-read", "clipboard-write"])
            ),
            (
                COPY_BLOCK,
                Category::Clipboard,
                strings(&["history-read", "terminal-read", "clipboard-write"])
            ),
            (
                SELECT_BLOCK,
                Category::Edit,
                strings(&["history-read", "terminal-read"])
            ),
            (
                DETAILS,
                Category::Edit,
                strings(&["history-read", "process-read", "clipboard-write"])
            ),
            (
                RERUN,
                Category::Edit,
                strings(&["history-read", "terminal-read", "pty-write"])
            ),
        ]
    );
}

#[test]
fn each_part_of_a_cell_s_block_is_copied_and_the_block_selected() {
    let (mut host, mut plugin) = commands_host();
    // A cell of `ls`' prompt, of its line or of its output: `ls`' block.
    for row in [0, 1, 3] {
        run(&mut host, &mut plugin, COPY_COMMAND, cell(3, row));
        assert_eq!(host.clipboard(), Some("ls"), "{row}");
        run(&mut host, &mut plugin, COPY_BLOCK, cell(3, row));
        assert_eq!(host.clipboard(), Some("ls\na.txt\nb.txt"), "{row}");
        assert_eq!(
            selected_cells(&mut host, &mut plugin, cell(3, row)),
            [selection((0, 0), (19, 3))],
            "{row}"
        );
    }
    // `true`'s, of no output: its line alone, and its prompt's row.
    run(&mut host, &mut plugin, COPY_BLOCK, cell(1, 4));
    assert_eq!(host.clipboard(), Some("true"));
    assert_eq!(
        selected_cells(&mut host, &mut plugin, cell(1, 4)),
        [selection((0, 4), (19, 4))]
    );
}

#[test]
fn a_running_command_s_line_is_copied_and_its_block_waits_for_its_end() {
    let (mut host, mut plugin) = commands_host();
    assert!(host.applies(&mut plugin, COPY_COMMAND, &cell(0, 6)));
    run(&mut host, &mut plugin, COPY_COMMAND, cell(0, 6));
    assert_eq!(host.clipboard(), Some("sleep 60"));
    for name in [COPY_BLOCK, SELECT_BLOCK] {
        assert!(!host.applies(&mut plugin, name, &cell(0, 6)), "{name}");
        assert!(run(&mut host, &mut plugin, name, cell(0, 6)).is_empty());
    }
    // Of no cell: the last command, the running one's line, and the last
    // block that ended, `true`'s.
    run(&mut host, &mut plugin, COPY_COMMAND, Args::new());
    assert_eq!(host.clipboard(), Some("sleep 60"));
    run(&mut host, &mut plugin, COPY_BLOCK, Args::new());
    assert_eq!(host.clipboard(), Some("true"));
}

#[test]
fn a_block_whose_prompt_left_the_scrollback_is_its_rows_that_the_pane_holds() {
    let (mut host, mut plugin) = commands_host();
    // `ls`' prompt and line gone, its first output row too.
    let pane = host.pane(PANE);
    pane.held = 2..12;
    pane.command_lines.remove(&1);
    pane.viewport = Some(Viewport {
        top: 2,
        rows: 10,
        screen: 2,
        oldest: 2,
        epoch: 1,
        alternate: false,
        folds: Vec::new(),
    });
    assert!(!host.applies(&mut plugin, COPY_COMMAND, &cell(0, 2)));
    run(&mut host, &mut plugin, COPY_BLOCK, cell(0, 2));
    assert_eq!(host.clipboard(), Some("b.txt"));
    assert_eq!(
        selected_cells(&mut host, &mut plugin, cell(0, 2)),
        [selection((0, 2), (19, 3))]
    );
}

#[test]
fn nothing_applies_on_the_alternate_screen_nor_of_a_pane_of_no_marks() {
    let (mut host, mut plugin) = commands_host();
    host.pane(PANE).viewport = Some(Viewport {
        top: 0,
        rows: 12,
        screen: 0,
        oldest: 0,
        epoch: 1,
        alternate: true,
        folds: Vec::new(),
    });
    for name in [COPY_COMMAND, COPY_BLOCK, SELECT_BLOCK, DETAILS, RERUN] {
        assert!(!host.applies(&mut plugin, name, &cell(0, 1)), "{name}");
    }
    let (mut host, mut plugin) = commands_host();
    host.pane(PANE).commands.clear();
    for name in [COPY_COMMAND, COPY_BLOCK, SELECT_BLOCK, DETAILS, RERUN] {
        assert!(!host.applies(&mut plugin, name, &Args::new()), "{name}");
        assert!(run(&mut host, &mut plugin, name, Args::new()).is_empty());
    }
}

/// The started time of the details' tests' commands: the fake host's time,
/// 11 November 2026 at noon in UTC, 13:00 of its time zone.
const STARTED: u64 = lokyr_plugin_sdk::fake::LOCAL_TIME.time;

/// The host of the commands, `ls` of a start, an end 1.25 s later and a
/// folder, and `sleep 60` of a start, of a remote machine's folder, its
/// messages of the duration and the statuses written, and its time 5 s
/// after `ls`' start.
fn details_host() -> (FakeHost, Blocks) {
    let (mut host, plugin) = commands_host();
    let pane = host.pane(PANE);
    pane.commands[0].started = Some(STARTED);
    pane.commands[0].ended = Some(STARTED + 1250);
    pane.commands[0].folder = Some("/Users/me/src".to_owned());
    pane.commands[2].started = Some(STARTED + 3000);
    pane.commands[2].folder = Some("/srv".to_owned());
    pane.commands[2].host = Some("box".to_owned());
    let number = |value: i64| Data::Integer(value);
    host.set_message(
        "details-seconds",
        &[("whole", number(1)), ("tenths", number(2))],
        Some("1.2 s"),
    );
    host.set_message(
        "details-succeeded",
        &[("status", number(0))],
        Some("0 · OK"),
    );
    host.set_message(
        "details-failed",
        &[("status", number(1))],
        Some("1 · Failed"),
    );
    host.set_local_time(lokyr_plugin_sdk::host::LocalTime {
        time: STARTED + 5000,
        offset: 3600,
    });
    (host, plugin)
}

/// The id of the details' panel that opened `nth`.
fn details_panel(nth: u64) -> PanelId {
    PanelId(format!("{DETAILS_PANEL}{nth}"))
}

/// The rows of the details that `panel` shows, as the plugin renders them:
/// each of its key, if it copies, its texts, and its button's icon, if it
/// has one.
fn details_rows(
    host: &mut FakeHost,
    plugin: &mut Blocks,
    panel: &PanelId,
) -> Vec<(Option<String>, Vec<String>, Option<String>)> {
    let tree = host
        .render(plugin, &ViewId::Panel(panel.clone()))
        .expect("the details");
    tree.children
        .iter()
        .filter(|row| !row.children.is_empty())
        .map(|row| {
            let texts = row.children.iter().filter_map(|child| match &child.kind {
                Kind::Text(text) => Some(text.clone()),
                _ => None,
            });
            let button = row.children.iter().find_map(|child| match &child.kind {
                Kind::Button(button) => Some(button.icon.clone()),
                _ => None,
            });
            (
                row.key.as_ref().map(|key| key.0.clone()),
                texts.collect(),
                button,
            )
        })
        .collect()
}

fn details_click(panel: &PanelId, key: &str) -> UiEvent {
    UiEvent {
        view: ViewId::Panel(panel.clone()),
        key: Key::from(key),
        kind: UiEventKind::Click,
    }
}

#[test]
fn a_cell_s_block_shows_its_details_each_copied_of_a_click() {
    let (mut host, mut plugin) = details_host();
    assert!(host.applies(&mut plugin, DETAILS, &cell(3, 1)));
    run(&mut host, &mut plugin, DETAILS, cell(3, 1));
    let panel = details_panel(1);
    let options = host.panels().get(&panel).expect("the details' panel");
    assert_eq!(
        options.anchor,
        Anchor::Cell {
            pane: PANE,
            column: 3,
            row: 1
        }
    );
    assert_eq!(options.role, PanelRole::Dialog);
    assert!(options.close_outside && options.close_escape);
    let copy = || Some("copy".to_owned());
    let texts = |texts: &[&str]| texts.iter().map(|text| (*text).to_owned()).collect();
    let key = |key: &str| Some(key.to_owned());
    assert_eq!(
        details_rows(&mut host, &mut plugin, &panel),
        [
            (key("line"), texts(&["ls"]), copy()),
            (key("status"), texts(&["details-status", "0 · OK"]), copy()),
            // Noon in UTC, of a time zone an hour east, today.
            (
                key("started"),
                texts(&["details-started", "13:00:00"]),
                copy()
            ),
            (
                key("duration"),
                texts(&["details-duration", "1.2 s"]),
                copy()
            ),
            (
                key("folder"),
                texts(&["details-folder", "/Users/me/src"]),
                copy()
            ),
        ]
    );
    // A click on a part's button, or on its row, copies it, its button's
    // icon a tick since.
    for (clicked, copied) in [
        ("copy-duration", "1.2 s"),
        ("folder", "/Users/me/src"),
        ("copy-status", "0"),
        ("line", "ls"),
        ("started", "13:00:00"),
    ] {
        host.ui_event(&mut plugin, &details_click(&panel, clicked))
            .unwrap();
        assert_eq!(host.clipboard(), Some(copied), "{clicked}");
    }
    let rows = details_rows(&mut host, &mut plugin, &panel);
    let icons: Vec<_> = rows.iter().map(|(_, _, icon)| icon.as_deref()).collect();
    assert_eq!(
        icons,
        [
            Some("copy"),
            Some("copy"),
            Some("check"),
            Some("copy"),
            Some("copy")
        ]
    );
    // Escape closes it, and the plugin forgets it.
    host.close_panel(&panel);
    host.deliver(&mut plugin).unwrap();
    assert!(plugin.details.is_none());
}

#[test]
fn a_running_block_s_details_tell_it_runs_until_its_end_comes() {
    let (mut host, mut plugin) = details_host();
    // Of no cell, as the Edit menu runs it: the last command, which runs,
    // below the cursor of a view of the screen.
    run(&mut host, &mut plugin, DETAILS, Args::new());
    let panel = details_panel(1);
    assert_eq!(
        host.panels().get(&panel).map(|options| &options.anchor),
        Some(&Anchor::Cursor(PANE))
    );
    let texts = |texts: &[&str]| texts.iter().map(|text| (*text).to_owned()).collect();
    let key = |key: &str| Some(key.to_owned());
    let copy = || Some("copy".to_owned());
    assert_eq!(
        details_rows(&mut host, &mut plugin, &panel),
        [
            (key("line"), texts(&["sleep 60"]), copy()),
            // Running: nothing to copy, and no duration yet.
            (None, texts(&["details-status", "details-running"]), None),
            (
                key("started"),
                texts(&["details-started", "13:00:03"]),
                copy()
            ),
            (
                key("folder"),
                texts(&["details-folder", "box:/srv"]),
                copy()
            ),
        ]
    );
    // It ends of the status 1, 1.25 s after its start.
    let pane = host.pane(PANE);
    pane.commands[2].end = Some((0, 7));
    pane.commands[2].status = Some(1);
    pane.commands[2].ended = Some(STARTED + 3000 + 1250);
    host.post(Event::CommandEnded {
        pane: PANE,
        command: 3,
        status: Some(1),
    });
    host.take_renders();
    host.deliver(&mut plugin).unwrap();
    assert_eq!(host.take_renders(), [ViewId::Panel(panel.clone())]);
    let rows = details_rows(&mut host, &mut plugin, &panel);
    assert_eq!(
        rows[1],
        (
            key("status"),
            texts(&["details-status", "1 · Failed"]),
            copy()
        )
    );
    assert_eq!(
        rows[3],
        (
            key("duration"),
            texts(&["details-duration", "1.2 s"]),
            copy()
        )
    );
}

#[test]
fn a_block_s_details_open_in_place_of_others_and_of_another_day_s_date() {
    let (mut host, mut plugin) = details_host();
    run(&mut host, &mut plugin, DETAILS, cell(0, 0));
    // The view scrolled up: the pane's top, rather than the cursor.
    host.pane(PANE).viewport = Some(Viewport {
        top: 0,
        rows: 12,
        screen: 40,
        oldest: 0,
        epoch: 1,
        alternate: false,
        folds: Vec::new(),
    });
    // A day later.
    host.set_local_time(lokyr_plugin_sdk::host::LocalTime {
        time: STARTED + 86_400_000,
        offset: 3600,
    });
    run(&mut host, &mut plugin, DETAILS, Args::new());
    // The first closed, its closing told after the second opened.
    assert!(!host.panels().contains_key(&details_panel(1)));
    let second = details_panel(2);
    assert_eq!(
        host.panels().get(&second).map(|options| &options.anchor),
        Some(&Anchor::PaneCorner {
            pane: PANE,
            corner: Corner::Top
        })
    );
    host.deliver(&mut plugin).unwrap();
    // The last that started, which runs, its start of its date, as a day
    // went since.
    let rows = details_rows(&mut host, &mut plugin, &second);
    assert_eq!(rows[0].1, ["sleep 60"]);
    assert_eq!(rows[2].1, ["details-started", "2026-11-11 13:00:03"]);
    run(&mut host, &mut plugin, DETAILS, cell(3, 1));
    host.deliver(&mut plugin).unwrap();
    let rows = details_rows(&mut host, &mut plugin, &details_panel(3));
    assert_eq!(rows[2].1, ["details-started", "2026-11-11 13:00:00"]);
}

#[test]
fn a_duration_is_of_its_unit_s_message() {
    let mut text = |id: &str, args: &[(&str, Data)]| {
        let args: Vec<String> = args
            .iter()
            .map(|(name, value)| match value {
                Data::Integer(value) => format!("{name}={value}"),
                other => format!("{name}={other:?}"),
            })
            .collect();
        format!("{id}({})", args.join(","))
    };
    for (millis, written) in [
        (0, "details-milliseconds(count=0)"),
        (999, "details-milliseconds(count=999)"),
        (1000, "details-seconds(whole=1,tenths=0)"),
        (59_999, "details-seconds(whole=59,tenths=9)"),
        (60_000, "details-minutes(minutes=1,seconds=0)"),
        (3_599_999, "details-minutes(minutes=59,seconds=59)"),
        (3_600_000, "details-hours(hours=1,minutes=0)"),
        (90_061_000, "details-hours(hours=25,minutes=1)"),
    ] {
        assert_eq!(duration(&mut text, millis), written, "{millis}");
    }
}

/// The notice of a run again that `why` refuses, as the host's catalogs
/// give no text in the tests: its message's id.
fn refused(why: &str) -> Request {
    Request::Notify {
        text: why.to_owned(),
        level: lokyr_plugin_sdk::host::NoticeLevel::Warning,
        title: Some("rerun-title".to_owned()),
    }
}

fn ran(line: &str) -> Request {
    Request::RunLine {
        pane: PANE,
        line: line.to_owned(),
    }
}

#[test]
fn a_block_runs_again_at_the_shell_s_prompt_of_an_empty_command_line() {
    let (mut host, mut plugin) = commands_host();
    host.pane(PANE).edited_line = Some(String::new());
    // A cell of `ls`' output, or of `true`'s prompt: their lines.
    assert!(host.applies(&mut plugin, RERUN, &cell(1, 2)));
    assert_eq!(run(&mut host, &mut plugin, RERUN, cell(1, 2)), [ran("ls")]);
    assert_eq!(
        run(&mut host, &mut plugin, RERUN, cell(0, 4)),
        [ran("true")]
    );
    // A command of several lines, as zsh tells it, whole.
    let lines = "for i in 1 2\ndo echo $i\ndone";
    host.pane(PANE).command_lines.insert(1, lines.to_owned());
    assert_eq!(run(&mut host, &mut plugin, RERUN, cell(1, 2)), [ran(lines)]);
    // Of no cell, as the Edit menu runs it: the last command that started.
    host.pane(PANE).commands[2].end = Some((0, 7));
    assert_eq!(
        run(&mut host, &mut plugin, RERUN, Args::new()),
        [ran("sleep 60")]
    );
}

#[test]
fn a_block_does_not_run_again_while_a_command_runs_or_the_line_holds_text() {
    let (mut host, mut plugin) = commands_host();
    // `sleep 60` runs: the shell edits no command line.
    assert!(host.applies(&mut plugin, RERUN, &cell(0, 6)));
    assert_eq!(
        run(&mut host, &mut plugin, RERUN, cell(1, 2)),
        [refused("rerun-busy")]
    );
    // Again within the second: the notice shows still, and the command
    // fails not.
    assert!(run(&mut host, &mut plugin, RERUN, cell(1, 2)).is_empty());
    // A second later, at the prompt, of text typed.
    host.advance(std::time::Duration::from_secs(1));
    host.pane(PANE).edited_line = Some("git st".to_owned());
    assert_eq!(
        run(&mut host, &mut plugin, RERUN, cell(1, 2)),
        [refused("rerun-typed")]
    );
    // A block of no line, at an empty command line.
    host.advance(std::time::Duration::from_secs(1));
    host.pane(PANE).edited_line = Some(String::new());
    host.pane(PANE).command_lines.remove(&1);
    assert_eq!(
        run(&mut host, &mut plugin, RERUN, cell(1, 2)),
        [refused("rerun-no-line")]
    );
}

#[test]
fn the_host_draws_the_blocks_as_the_config_says() {
    let (mut host, mut plugin) = host();
    // By default, their bars along them and their dividers; of the pill,
    // as `host` asks, no prompt rows.
    assert_eq!(
        host.block_looks(),
        BlockLooks {
            bars: true,
            dividers: true,
            failed_tint: false,
            prompt_rows: false,
            spacing: false,
        }
    );
    let config = |pairs: &[(&str, bool)]| {
        Data::Map(
            pairs
                .iter()
                .map(|(key, on)| ((*key).to_owned(), Data::Boolean(*on)))
                .collect(),
        )
    };
    host.set_config(config(&[
        ("dividers", false),
        ("failed-tint", true),
        ("spacing", true),
    ]));
    host.deliver(&mut plugin).unwrap();
    assert_eq!(
        host.block_looks(),
        BlockLooks {
            bars: true,
            dividers: false,
            failed_tint: true,
            prompt_rows: true,
            spacing: true,
        }
    );
    // A change of other keys asks nothing anew.
    host.take_done();
    host.set_config(config(&[
        ("dividers", false),
        ("failed-tint", true),
        ("spacing", true),
        ("x", true),
    ]));
    host.deliver(&mut plugin).unwrap();
    assert!(
        !host
            .take_done()
            .iter()
            .any(|request| matches!(request, Request::SetBlockLooks(_)))
    );
}

/// Runs `name` of no arguments in the commands' host, its events
/// delivered, and gives the block that it selected then and the view's
/// scroll that it asked for, if any.
fn selected_after(
    host: &mut FakeHost,
    plugin: &mut Blocks,
    name: &str,
) -> (Option<u64>, Option<Scroll>) {
    let done = run(host, plugin, name, Args::new());
    let scroll = done.iter().find_map(|request| match request {
        Request::Scroll { scroll, .. } => Some(*scroll),
        _ => None,
    });
    (host.selected_block(PANE), scroll)
}

/// The commands' host of a view of 12 rows from `top`, of the screen from
/// the row `screen`, the rows above it the scrollback's.
fn viewed(host: &mut FakeHost, top: i64, screen: i64) {
    host.pane(PANE).viewport = Some(Viewport {
        top,
        rows: 12,
        screen,
        oldest: 0,
        epoch: 1,
        alternate: false,
        folds: Vec::new(),
    });
}

#[test]
fn the_keyboard_selects_the_last_block_and_moves_the_selection() {
    let (mut host, mut plugin) = commands_host();
    // The view at the screen, of every block: the last one, which runs, as
    // the prompt where the user types follows it.
    viewed(&mut host, 0, 0);
    assert!(!host.applies(&mut plugin, DESELECT, &Args::new()));
    assert_eq!(
        selected_after(&mut host, &mut plugin, SELECT_PREVIOUS),
        (Some(3), Some(Scroll::Row(5)))
    );
    // The ones before, each at the view's top, as far as the first.
    assert_eq!(
        selected_after(&mut host, &mut plugin, SELECT_PREVIOUS),
        (Some(2), Some(Scroll::Row(4)))
    );
    assert_eq!(
        selected_after(&mut host, &mut plugin, SELECT_PREVIOUS),
        (Some(1), Some(Scroll::Row(0)))
    );
    assert_eq!(
        selected_after(&mut host, &mut plugin, SELECT_PREVIOUS),
        (Some(1), None)
    );
    // The ones after, then, past the last, none, and the view at the
    // screen.
    assert_eq!(
        selected_after(&mut host, &mut plugin, SELECT_NEXT),
        (Some(2), Some(Scroll::Row(4)))
    );
    assert_eq!(
        selected_after(&mut host, &mut plugin, SELECT_NEXT),
        (Some(3), Some(Scroll::Row(5)))
    );
    assert_eq!(
        selected_after(&mut host, &mut plugin, SELECT_NEXT),
        (None, Some(Scroll::Bottom))
    );
    // Escape's command selects none.
    selected_after(&mut host, &mut plugin, SELECT_PREVIOUS);
    assert!(host.applies(&mut plugin, DESELECT, &Args::new()));
    assert_eq!(
        selected_after(&mut host, &mut plugin, DESELECT),
        (None, None)
    );
}

#[test]
fn of_a_view_scrolled_up_the_keyboard_selects_after_a_prompt_s_jump() {
    let (mut host, mut plugin) = commands_host();
    // The view's top at `true`'s prompt, above the screen: the block of
    // the prompt above it, `ls`', at the view's top, as ⌘↑ jumps.
    viewed(&mut host, 4, 6);
    assert_eq!(
        selected_after(&mut host, &mut plugin, SELECT_PREVIOUS),
        (Some(1), Some(Scroll::Row(0)))
    );
    // The view's top at `ls`' output: the next prompt's block, `true`'s.
    host.end_block_selection(PANE);
    viewed(&mut host, 1, 6);
    assert_eq!(
        selected_after(&mut host, &mut plugin, SELECT_NEXT),
        (Some(2), Some(Scroll::Row(4)))
    );
    // At the view's top, the first's prompt: none above it.
    host.end_block_selection(PANE);
    viewed(&mut host, 0, 6);
    assert_eq!(
        selected_after(&mut host, &mut plugin, SELECT_PREVIOUS),
        (None, None)
    );
}

#[test]
fn the_actions_of_no_cell_take_the_selected_block() {
    let (mut host, mut plugin) = commands_host();
    viewed(&mut host, 0, 0);
    for _ in 0..3 {
        selected_after(&mut host, &mut plugin, SELECT_PREVIOUS);
    }
    assert_eq!(host.selected_block(PANE), Some(1));
    // `ls`, rather than the last command.
    run(&mut host, &mut plugin, COPY_COMMAND, Args::new());
    assert_eq!(host.clipboard(), Some("ls"));
    run(&mut host, &mut plugin, COPY_BLOCK, Args::new());
    assert_eq!(host.clipboard(), Some("ls\na.txt\nb.txt"));
    // A cell's block before the selection.
    run(&mut host, &mut plugin, COPY_COMMAND, cell(0, 4));
    assert_eq!(host.clipboard(), Some("true"));
    // The selection gone, as a key typed ends it: the last command again.
    host.end_block_selection(PANE);
    run(&mut host, &mut plugin, COPY_COMMAND, Args::new());
    assert_eq!(host.clipboard(), Some("sleep 60"));
}

#[test]
fn rows_of_no_command_before_the_first_prompt_leave_the_selection_whole() {
    let (mut host, mut plugin) = commands_host();
    // A login's line and an empty row before `ls`' prompt, as a login
    // shell's first rows: every command two rows down.
    let pane = host.pane(PANE);
    for command in &mut pane.commands {
        for mark in [
            &mut command.prompt,
            &mut command.input,
            &mut command.output,
            &mut command.end,
        ] {
            *mark = mark.map(|(column, row)| (column, row + 2));
        }
    }
    viewed(&mut host, 0, 0);
    assert_eq!(
        selected_after(&mut host, &mut plugin, SELECT_PREVIOUS),
        (Some(3), Some(Scroll::Row(7)))
    );
    selected_after(&mut host, &mut plugin, SELECT_PREVIOUS);
    assert_eq!(
        selected_after(&mut host, &mut plugin, SELECT_PREVIOUS),
        (Some(1), Some(Scroll::Row(2)))
    );
    // Of none selected, the view's top at the login's line: the first
    // block below it.
    host.end_block_selection(PANE);
    viewed(&mut host, 0, 6);
    assert_eq!(
        selected_after(&mut host, &mut plugin, SELECT_NEXT),
        (Some(1), Some(Scroll::Row(2)))
    );
}

#[test]
fn a_click_on_a_block_selects_it_and_one_past_the_blocks_selects_none() {
    let (mut host, mut plugin) = commands_host();
    viewed(&mut host, 0, 0);
    let mut click = |host: &mut FakeHost, row: i64| {
        host.post(Event::Click {
            pane: PANE,
            at: (3, row),
        });
        host.deliver(&mut plugin).unwrap();
        host.selected_block(PANE)
    };
    // `ls`' output, then `true`'s prompt; `sleep`'s, which runs, none, as
    // a click that gives a running program the keyboard.
    assert_eq!(click(&mut host, 2), Some(1));
    assert_eq!(click(&mut host, 4), Some(2));
    assert_eq!(click(&mut host, 6), None);
    assert_eq!(click(&mut host, 5), None);
    // Above every command, none.
    host.pane(PANE).commands.remove(0);
    assert_eq!(click(&mut host, 0), None);
    // The alternate screen, which hides the commands' rows, keeps it.
    assert_eq!(click(&mut host, 4), Some(2));
    host.pane(PANE).viewport.as_mut().unwrap().alternate = true;
    assert_eq!(click(&mut host, 2), Some(2));
}

#[test]
fn the_prompt_rows_pin_by_default_in_place_of_the_pill_which_header_brings_back() {
    let (mut host, mut plugin) = host();
    // Of the pill, as `host` asks: the host pins nothing, and the pill
    // shows as the view's top shows `make`'s output.
    assert!(!host.block_looks().prompt_rows);
    assert!(scrolled(&mut host, &mut plugin, 6).is_some());
    // Of the config's default, the rows: the host pins them, and the pill
    // goes.
    host.set_config(Data::Map(std::collections::BTreeMap::new()));
    host.deliver(&mut plugin).unwrap();
    assert!(host.block_looks().prompt_rows);
    assert!(
        host.take_done()
            .iter()
            .any(|request| matches!(request, Request::ClosePanel(id) if *id == panel(PANE)))
    );
    assert!(scrolled(&mut host, &mut plugin, 7).is_none());
}

/// The items of the block's menu that shows, of `host`: each button's key,
/// title and whether it is dimmed, and a line as `-`.
fn menu_items(host: &mut FakeHost, plugin: &mut Blocks) -> Vec<(String, String, bool)> {
    let tree = host
        .render(plugin, &ViewId::Panel(PanelId::from(MENU)))
        .expect("the block's menu");
    tree.children
        .iter()
        .map(|node| match &node.kind {
            Kind::Button(button) => (
                node.key
                    .as_ref()
                    .map(|key| key.0.clone())
                    .unwrap_or_default(),
                button.label.clone(),
                button.disabled,
            ),
            _ => ("-".to_owned(), String::new(), false),
        })
        .collect()
}

/// A click on the item `name` of the block's menu, its events delivered.
fn choose(host: &mut FakeHost, plugin: &mut Blocks, name: &str) {
    host.set_clipboard(None);
    let event = UiEvent {
        view: ViewId::Panel(PanelId::from(MENU)),
        key: Key::from(name),
        kind: UiEventKind::Click,
    };
    host.ui_event(plugin, &event).unwrap();
    host.deliver(plugin).unwrap();
}

#[test]
fn the_context_menu_holds_a_block_s_actions_in_a_submenu_of_their_own() {
    let (host, _) = commands_host();
    let items: Vec<(&str, Option<&str>, bool)> = host
        .menu_items()
        .iter()
        .map(|item| {
            (
                item.command.as_str(),
                item.submenu.as_deref(),
                item.place == MenuPlace::PaneContext && item.args.is_empty(),
            )
        })
        .collect();
    assert_eq!(items, ACTIONS.map(|name| (name, Some(BLOCK_SUBMENU), true)));
}

#[test]
fn a_context_click_on_the_pinned_rows_opens_the_block_s_menu_which_acts_on_its_pane() {
    let (mut host, mut plugin) = commands_host();
    // Another pane has the keyboard: the menu acts on its own.
    host.focus(Some(PaneId(2)));
    host.pane(PaneId(2)).tab = TabId(1);
    host.post(Event::PromptRowsMenu {
        pane: PANE,
        window: lokyr_plugin_sdk::host::WindowId(1),
        command: 1,
    });
    host.deliver(&mut plugin).unwrap();
    let options = host
        .panels()
        .get(&PanelId::from(MENU))
        .expect("the block's menu");
    assert_eq!(
        (&options.anchor, options.role),
        (
            &Anchor::Pointer(lokyr_plugin_sdk::host::WindowId(1)),
            PanelRole::Menu
        )
    );
    // `ls`, which ended of some output: each action, none dimmed.
    let item = |name: &str, title: &str, dimmed: bool| (name.to_owned(), title.to_owned(), dimmed);
    assert_eq!(
        menu_items(&mut host, &mut plugin),
        [
            item(COPY_COMMAND, "Copy Command", false),
            item(COPY_OUTPUT, "Copy Command Output", false),
            item(COPY_BLOCK, "Copy Command and Output", false),
            item(SELECT_OUTPUT, "Select Command Output", false),
            item(SELECT_BLOCK, "Select Block", false),
            ("-".to_owned(), String::new(), false),
            item(DETAILS, "Block Details", false),
            item(RERUN, "Run Command Again", false),
        ]
    );
    choose(&mut host, &mut plugin, COPY_OUTPUT);
    assert_eq!(host.clipboard(), Some("a.txt\nb.txt"));
    assert!(!host.panels().contains_key(&PanelId::from(MENU)));
    host.post(Event::PromptRowsMenu {
        pane: PANE,
        window: lokyr_plugin_sdk::host::WindowId(1),
        command: 1,
    });
    host.deliver(&mut plugin).unwrap();
    choose(&mut host, &mut plugin, COPY_COMMAND);
    assert_eq!(host.clipboard(), Some("ls"));
    // Its details at the pane's top.
    host.post(Event::PromptRowsMenu {
        pane: PANE,
        window: lokyr_plugin_sdk::host::WindowId(1),
        command: 1,
    });
    host.deliver(&mut plugin).unwrap();
    choose(&mut host, &mut plugin, DETAILS);
    let details = host
        .panels()
        .iter()
        .find(|(id, _)| id.0.starts_with(DETAILS_PANEL))
        .map(|(_, options)| options.anchor.clone());
    assert_eq!(
        details,
        Some(Anchor::PaneCorner {
            pane: PANE,
            corner: Corner::Top
        })
    );
}

#[test]
fn a_running_block_s_menu_dims_what_waits_for_its_end() {
    let (mut host, mut plugin) = commands_host();
    host.post(Event::PromptRowsMenu {
        pane: PANE,
        window: lokyr_plugin_sdk::host::WindowId(1),
        command: 3,
    });
    host.deliver(&mut plugin).unwrap();
    let dimmed: Vec<String> = menu_items(&mut host, &mut plugin)
        .into_iter()
        .filter(|(_, _, dimmed)| *dimmed)
        .map(|(name, ..)| name)
        .collect();
    assert_eq!(
        dimmed,
        [COPY_OUTPUT, COPY_BLOCK, SELECT_OUTPUT, SELECT_BLOCK]
    );
}

#[test]
fn a_context_click_on_the_pill_opens_its_block_s_menu_at_the_pointer() {
    let (mut host, mut plugin) = host();
    // `make`'s output at the view's top: its pill.
    assert!(scrolled(&mut host, &mut plugin, 35).is_some());
    let event = UiEvent {
        view: ViewId::Panel(panel(PANE)),
        key: Key::from(PILL),
        kind: UiEventKind::ContextMenu,
    };
    host.ui_event(&mut plugin, &event).unwrap();
    let options = host
        .panels()
        .get(&PanelId::from(MENU))
        .expect("the block's menu");
    assert_eq!(
        options.anchor,
        Anchor::ViewPointer {
            view: ViewId::Panel(panel(PANE))
        }
    );
    assert_eq!(plugin.menu, Some((PANE, 2)));
}

#[test]
fn a_copy_of_the_selected_block_alone_copies_nothing_of_none() {
    let (mut host, mut plugin) = commands_host();
    let selected = Args::from([("selected".to_owned(), Value::Boolean(true))]);
    // None selected: nothing read, the clipboard kept.
    assert!(run(&mut host, &mut plugin, COPY_BLOCK, selected.clone()).is_empty());
    assert_eq!(host.clipboard(), None);
    // `ls` selected of a click: its line and its output.
    host.post(Event::Click {
        pane: PANE,
        at: (0, 1),
    });
    host.deliver(&mut plugin).unwrap();
    assert_eq!(host.selected_block(PANE), Some(1));
    run(&mut host, &mut plugin, COPY_BLOCK, selected);
    assert_eq!(host.clipboard(), Some("ls\na.txt\nb.txt"));
}
