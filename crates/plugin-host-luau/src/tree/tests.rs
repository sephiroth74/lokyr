use lokyr_plugin_sdk::host::TabId;
use lokyr_plugin_sdk::view::{
    Direction, Distribution, Key, Layout, Length, ListItem, Progress, ProgressShape, ProgressState,
    Role, Style, Tab,
};
use mlua::Lua;

use super::*;
use crate::view_through_luau;

/// The view of Luau's `source`, of a plugin's render.
fn take(source: &str) -> Result<Node, ShapeError> {
    let lua = Lua::new();
    from_lua(
        lua.load(format!("return {source}")).eval().unwrap(),
        &Budget::default(),
    )
}

/// The error of the view of Luau's `source`.
fn refused(source: &str) -> String {
    take(source).unwrap_err().0
}

/// A node of every kind, and of every field of each, none of its default.
fn every() -> Node {
    let mut root = Node::container(Vec::new());
    root.key = Some(Key::from("root"));
    root.layout = Layout {
        direction: Direction::Column,
        gap: 2.0,
        min_width: Length::Points(40.0),
        ..Layout::default()
    };
    root.style = Style {
        radius: 4.0,
        hides_ring: true,
        font_size: Some(15.0),
        ..Style::default()
    };
    root.label = Some("Find".to_owned());
    root.role = Some(Role::Toolbar);
    let mut text = Node::text("Hello");
    text.role = Some(Role::Heading);
    let mut icon = Node::icon("lock");
    icon.label = Some("Locked".to_owned());
    let mut button = Node::new(Kind::Button(Button {
        label: "Go".to_owned(),
        icon: "play".to_owned(),
        disabled: true,
    }));
    button.key = Some(Key::from("go"));
    button.focusable = true;
    button.tooltip = Some("Run it".to_owned());
    let mut toggle = Node::new(Kind::Toggle(Toggle {
        label: "Aa".to_owned(),
        icon: "case".to_owned(),
        on: true,
        disabled: true,
    }));
    toggle.key = Some(Key::from("case"));
    let mut tab = Node::text("src");
    tab.key = Some(Key::from("tab-3"));
    tab.reorderable = true;
    tab.carries = Some(TabId(3));
    let mut row = Node::container(vec![Node::text(""), Node::icon("file")]);
    row.layout.gap = 4.0;
    let list = Node::new(Kind::List(List {
        row: Box::new(row),
        count: 100,
        first: 10,
        items: vec![ListItem {
            texts: vec!["a".to_owned()],
        }],
        row_height: Some(22.0),
        menu: true,
    }));
    let progress = Node::new(Kind::Progress(Progress {
        state: ProgressState::Indeterminate,
        value: 0.5,
        shape: ProgressShape::Ring,
        radius: Some(3.0),
        thickness: 1.5,
        segment: 0.25,
        period: Some(1.0),
    }));
    let mut tabs = Node::new(Kind::Tabs(Tabs {
        tabs: vec![
            Tab::new("General"),
            Tab {
                label: "Keys".to_owned(),
                icon: "keyboard".to_owned(),
                disabled: true,
            },
        ],
        chosen: 1,
        distribution: Distribution::FillEqually,
    }));
    tabs.children = vec![Node::text("page")];
    root.children = vec![
        text,
        icon,
        Node::new(Kind::Spacer),
        Node::new(Kind::Field(Field {
            placeholder: "Find".to_owned(),
        })),
        button,
        toggle,
        Node::new(Kind::Separator),
        list,
        Node::new(Kind::Scroll),
        progress,
        tabs,
        tab,
        Node::image(1001),
    ];
    root
}

/// [`every`], as a plugin writes it.
const EVERY: &str = r#"{
    kind = "container", key = "root", label = "Find", role = "toolbar",
    layout = { direction = "column", gap = 2, min_width = { points = 40 } },
    style = { radius = 4, hides_ring = true, font_size = 15 },
    children = {
        { kind = "text", text = "Hello", role = "heading" },
        { kind = "icon", icon = "lock", label = "Locked" },
        { kind = "spacer" },
        { kind = "field", placeholder = "Find" },
        { kind = "button", key = "go", text = "Go", icon = "play", disabled = true,
          focusable = true, tooltip = "Run it" },
        { kind = "toggle", key = "case", text = "Aa", icon = "case", on = true, disabled = true },
        { kind = "separator" },
        { kind = "list", count = 100, first = 10, items = { { texts = { "a" } } },
          row_height = 22, menu = true,
          row = { kind = "container", layout = { gap = 4 },
                  children = { { kind = "text", text = "" }, { kind = "icon", icon = "file" } } } },
        { kind = "scroll" },
        { kind = "progress", progress = { state = "indeterminate", value = 0.5, shape = "ring",
          radius = 3, thickness = 1.5, segment = 0.25, period = 1 } },
        { kind = "tabs", tabs = { { label = "General" }, { label = "Keys", icon = "keyboard", disabled = true } },
          chosen = 1, distribution = "fill-equally", children = { { kind = "text", text = "page" } } },
        { kind = "text", key = "tab-3", text = "src", reorderable = true, carries = 3 },
        { kind = "image", image = 1001 },
    },
}"#;

#[test]
fn a_view_s_nodes_are_flat_tables_of_their_kinds_fields_both_ways() {
    let every = every();
    assert_eq!(take(EVERY), Ok(every.clone()));
    // Back as it came, of every field.
    view_through_luau(&every).unwrap();
    // As the host gives it: a kind's fields beside the node's.
    let lua = Lua::new();
    lua.globals()
        .set("view", to_lua(&lua, &every).unwrap())
        .unwrap();
    lua.load(
        r#"
        assert(view.kind == "container" and view.key == "root" and view.role == "toolbar")
        assert(view.layout.min_width.points == 40 and view.style.hides_ring == true)
        local button = view.children[5]
        assert(button.kind == "button" and button.text == "Go" and button.disabled == true)
        assert(view.children[8].row.children[2].icon == "file")
        assert(view.children[11].children[1].text == "page")
        assert(view.children[13].kind == "image" and view.children[13].image == 1001)
        -- A field of its default left out.
        assert(view.children[3].layout == nil and view.children[3].focusable == nil)
        "#,
    )
    .exec()
    .unwrap();
    // A tree of no children nor row.
    view_through_luau(&Node::text("x")).unwrap();
}

#[test]
fn a_node_of_another_shape_is_refused_of_its_place() {
    for (source, error) in [
        (
            r#"{ kind = "text", text = "x", on = true }"#,
            "the view's root: no field `on` of a text node, of key, layout, style, \
             focusable, reorderable, carries, tooltip, label, role, children, text",
        ),
        (
            r#"{ kind = "label" }"#,
            "`label` is no kind of node, of container",
        ),
        (r#"{ text = "x" }"#, "a node takes its `kind`"),
        (r#"{ kind = 3 }"#, "`kind`: expected a text, not a number"),
        (r#"{ kind = "text" }"#, "`text`: expected a text, not nil"),
        (
            r#"{ kind = "list", count = 1 }"#,
            "a list node takes its `row`",
        ),
        (
            r#"{ kind = "button", text = "Go", icon = 1 }"#,
            "`icon`: expected a text, not a number",
        ),
        (
            r#"{ kind = "container", children = { { kind = "text", text = "a" },
                { kind = "list", count = 1, row = { kind = "text", text = 1 } } } }"#,
            "the node at children[2].row: `text`: expected a text, not a number",
        ),
        (
            r#"{ kind = "container", children = { "x" } }"#,
            "the node at children[1]: expected a node, a table, not a text",
        ),
        // Keys of the wrong kind.
        (
            r#"{ kind = "container", [1] = "x" }"#,
            "a node's field is named by a text, not a number",
        ),
        (
            r#"{ kind = "container", children = { a = { kind = "spacer" } } }"#,
            "`children`: expected a list of nodes, a table of keys from 1 to its length alone",
        ),
        (
            r#"{ kind = "container", children = { [1] = { kind = "spacer" }, [3] = { kind = "spacer" } } }"#,
            "`children`: expected a list of nodes",
        ),
        (r#""text""#, "expected a node, a table, not a text"),
    ] {
        let given = refused(source);
        assert!(given.contains(error), "{source}: {given}");
    }
}

/// A view nested `levels` deep, of containers about a text, as a plugin
/// makes it.
fn nested(levels: usize) -> String {
    format!(
        r#"(function()
            local node = {{ kind = "text", text = "x" }}
            for _ = 2, {levels} do
                node = {{ kind = "container", key = "k", style = {{ radius = 2 }}, children = {{ node }} }}
            end
            return node
        end)()"#
    )
}

#[test]
fn a_view_past_its_depth_or_that_holds_itself_is_refused_without_recursion() {
    // On a stack far smaller than a worker's: the tree converts a node at a
    // time, and drops as it was made.
    let ran = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let deepest = take(&nested(Node::DEEPEST)).unwrap();
            assert!(deepest.nests_within_deepest());
            let lua = Lua::new();
            let back = from_lua(
                Value::Table(to_lua(&lua, &deepest).unwrap()),
                &Budget::default(),
            )
            .unwrap();
            assert!(back == deepest);
            lokyr_plugin_sdk::view::dismantle(back);
            lokyr_plugin_sdk::view::dismantle(deepest);
            for source in [
                nested(Node::DEEPEST + 1),
                nested(100_000),
                // A list's row a level of its own.
                format!(
                    r#"{{ kind = "list", count = 0, row = {} }}"#,
                    nested(Node::DEEPEST)
                ),
                // A node of its own children, and a list of its own row.
                r#"(function() local node = { kind = "container" } node.children = { node } return node end)()"#
                    .to_owned(),
                r#"(function() local list = { kind = "list", count = 1 } list.row = list return list end)()"#
                    .to_owned(),
            ] {
                let error = refused(&source);
                assert!(error.contains("a view nests 64 levels of nodes at most"), "{error}");
            }
            assert!(
                take(&format!(
                    r#"{{ kind = "list", count = 0, row = {} }}"#,
                    nested(Node::DEEPEST - 1)
                ))
                .is_ok()
            );
        })
        .unwrap()
        .join();
    assert!(ran.is_ok());
}

#[test]
fn a_view_s_nodes_count_against_its_values() {
    // A node many times in a list of children converts each time: a tree
    // of 2^31 nodes of 31 tables is refused of its values.
    let error = refused(
        r#"(function()
            local node = { kind = "spacer" }
            for _ = 1, 30 do
                node = { kind = "container", children = { node, node } }
            end
            return node
        end)()"#,
    );
    assert!(error.contains("values at most"), "{error}");
    // A node that holds itself many times among its children: refused of
    // its values, before its depth, as the walk holds no more than it takes.
    let error = refused(
        r#"(function()
            local node = { kind = "container" }
            local children = {}
            for i = 1, 2^18 do children[i] = node end
            node.children = children
            return node
        end)()"#,
    );
    assert!(error.contains("values at most"), "{error}");
    // A few times, a copy each.
    let tree = take(
        r#"(function()
            local leaf = { kind = "text", text = "x" }
            return { kind = "container", children = { leaf, leaf, leaf } }
        end)()"#,
    )
    .unwrap();
    assert_eq!(tree.children, vec![Node::text("x"); 3]);
}
