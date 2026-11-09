use super::*;
use crate::requests::{self, kinds};

/// The definitions, made once for the tests.
fn made() -> &'static str {
    static MADE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    MADE.get_or_init(definitions)
}

#[test]
fn the_definitions_file_is_the_sdk_s() {
    let held = include_str!("../../luau/lokyr.d.luau");
    assert!(
        held == made(),
        "crates/plugin-host-luau/luau/lokyr.d.luau is not the SDK's: run `cargo xtask luau-types`"
    );
    // Made alike each time.
    assert_eq!(definitions(), made());
}

#[test]
fn every_request_is_a_function_of_cx_of_its_arguments_and_its_answer() {
    let cx = made()
        .split_once("export type Cx = {\n")
        .and_then(|(_, rest)| rest.split_once("\n}\n"))
        .map(|(cx, _)| cx)
        .unwrap();
    for kind in kinds() {
        assert!(
            cx.contains(&format!("    {}: (", kind.name)),
            "no function of cx {}",
            kind.name
        );
    }
    assert_eq!(cx.lines().count(), kinds().len());
    for line in [
        "    line_text: (pane: PaneId, row: number, reach: number) -> (LineText?, string?, number?),",
        "    focused_pane: () -> (PaneId?, string?, number?),",
        "    new_tab: (window: WindowId, index: number?, directory: string?) -> ({ tab: TabId, pane: PaneId }?, string?, number?),",
        "    register_command: (command: Command) -> (true?, string?, number?),",
        "    subscribe: (names: { string }) -> (true?, string?, number?),",
        "    emit: (name: string, schema: string, value: Data) -> (true?, string?, number?),",
        "    open_panel: (id: PanelId, options: PanelOptions) -> (true?, string?, number?),",
        "    config: () -> (Data?, string?, number?),",
    ] {
        assert!(cx.contains(line), "{line}");
    }
    // The names of the parameters of values, each of a request of as many.
    for (name, names) in PARAMS {
        let kind = kinds()
            .iter()
            .find(|kind| kind.name == name)
            .unwrap_or_else(|| panic!("no request {name}"));
        let count = match kind.params {
            requests::Params::One => 1,
            requests::Params::Values(count) => count,
            other => panic!("{name} of {other:?}"),
        };
        assert_eq!(names.len(), count, "{name}");
    }
}

#[test]
fn every_event_view_s_event_and_node_s_kind_is_of_its_flat_shape() {
    let made = made();
    for kind in [
        "    | { kind: \"tab-created\", tab: TabId, window: WindowId, index: number }",
        "    | { kind: \"keys-changed\" }",
        "    | { kind: \"panel-closed\", id: PanelId }",
        "    | { kind: \"custom\", name: string, schema: string, value: Data }",
        "    | { view: ViewId, key: string, kind: \"submit\", text: string, shift: boolean }",
        "    | { view: ViewId, key: string, kind: \"drop-on-pane\", pane: PaneId, edge: Edge }",
    ] {
        assert!(made.contains(kind), "{kind}");
    }
    let Some((_, node)) = made.split_once("export type Node = {\n") else {
        panic!("no node");
    };
    for name in tree::KINDS {
        assert!(node.contains(&format!("\"{name}\"")), "{name}");
        for field in tree::own(name).unwrap_or_default() {
            assert!(node.contains(&format!("    {field}: ")), "{field}");
        }
    }
    for field in tree::NODE {
        assert!(node.contains(&format!("    {field}: ")), "{field}");
    }
    for line in [
        "    text: string?,",
        "    row: Node?,",
        "    tabs: { Tab }?,",
        "    progress: Progress?,",
    ] {
        assert!(node.contains(line), "{line}");
    }
    // Each type that the file names, declared once.
    let declared: Vec<&str> = made
        .lines()
        .filter_map(|line| line.strip_prefix("export type "))
        .filter_map(|line| line.split_whitespace().next())
        .collect();
    let mut unique = declared.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), declared.len(), "{declared:?}");
    for name in [
        "Data",
        "Value",
        "Args",
        "Capability",
        "Cx",
        "Event",
        "UiEvent",
        "Node",
        "Plugin",
        "ViewId",
        "NodeLayout",
        "NodeRole",
        "PanelRole",
    ] {
        assert!(declared.contains(&name), "{name}");
    }
    // A table's field of a reserved word, as a text.
    assert!(made.contains("    [\"end\"]: { number }?,"));
}
