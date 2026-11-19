//! The plugin against the SDK's fake host, whose pane holds three commands
//! and tells the views that the test gives. The header's own behavior is
//! `lokyr/command-header`'s tests'; these test what `lokyr/blocks` adds:
//! the button to a block's end, and the plugin's replacing of the plain
//! header, of the same keys.

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
    };
    vec![
        command(1, 0, Some(30), Some(0)),
        command(2, 30, Some(60), Some(1)),
        command(3, 60, None, None),
    ]
}

/// A host of a pane of 100 rows and the three commands, of the end
/// button's texts, and the plugin, activated.
fn host() -> (FakeHost, Blocks) {
    let mut host = FakeHost::new(NAME).with_manifest(crate::MANIFEST);
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
            [("align".to_owned(), Data::Text(value.to_owned()))]
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
    // The same keys, of the same defaults, bounds and texts.
    let keys = |schema: &'static str| {
        schema
            .split_once("\n  \"properties\": {\n")
            .and_then(|(_, rest)| rest.split_once("\n  \"x-guide\": "))
            .map(|(keys, _)| keys)
            .expect("the schema's keys")
    };
    assert_eq!(
        keys(include_str!("../blocks.schema.json")),
        keys(include_str!(
            "../../command-header/command-header.schema.json"
        ))
    );
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
    assert_eq!(keys.len(), 11, "{keys:?}");
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
    for (_, catalog) in CATALOGS {
        for id in ["header-label", "header-tooltip", "header-end"] {
            assert!(catalog.contains(&format!("\n{id} = ")), "{id}");
        }
    }
}
