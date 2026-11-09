//! A view's tree as Luau's tables, both ways, of a node's flat shape: a
//! table of its fields, its `kind` a text, as `"button"`, and its kind's
//! own fields beside the node's, as `{ kind = "button", key = "go", text =
//! "Go", icon = "play" }`; its children a list of nodes, and a list's row a
//! node. A text's text and a button's or a toggle's label are its `text`,
//! an icon's name its `icon`, as the node's own `label` is what
//! accessibility calls it.
//!
//! A tree converts without recursion, a node at a time, bounded at
//! [`Node::DEEPEST`] levels as it converts, so that no tree deeper than the
//! host takes is ever made, as a tree's drop, clone and comparison recurse;
//! a table that holds itself is refused as a tree too deep. Its nodes count
//! against the conversion's values, as a list of children that holds one
//! table many times converts it each time. A node's other fields, a layout,
//! a style, a progress, a list's items or a view of tabs' tabs, convert as
//! the SDK's other types do.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use lokyr_plugin_sdk::view::{Button, Field, Kind, List, Node, Tabs, Toggle};
use mlua::Value;
use serde::de::DeserializeOwned;

use crate::shape::{self, Budget, ShapeError, Taking};

/// The fields of a node of every kind, by their names in Luau.
const NODE: [&str; 11] = [
    "key",
    "kind",
    "layout",
    "style",
    "focusable",
    "reorderable",
    "carries",
    "tooltip",
    "label",
    "role",
    "children",
];

/// The kinds of node, by their names in Luau.
const KINDS: [&str; 12] = [
    "container",
    "text",
    "icon",
    "spacer",
    "field",
    "button",
    "toggle",
    "separator",
    "list",
    "scroll",
    "progress",
    "tabs",
];

/// The fields of a node of `kind`'s own, by their names in Luau, if it is
/// a kind of node.
fn own(kind: &str) -> Option<&'static [&'static str]> {
    Some(match kind {
        "container" | "spacer" | "separator" | "scroll" => &[],
        "text" => &["text"],
        "icon" => &["icon"],
        "field" => &["placeholder"],
        "button" => &["text", "icon", "disabled"],
        "toggle" => &["text", "icon", "on", "disabled"],
        "list" => &["row", "count", "first", "items", "row_height", "menu"],
        "progress" => &["progress"],
        "tabs" => &["tabs", "chosen", "distribution"],
        _ => return None,
    })
}

/// The name in Luau of `kind`.
/// The error of a tree deeper than the host takes.
fn too_deep() -> ShapeError {
    ShapeError(format!(
        "a view nests {} levels of nodes at most, a list's row a level of its own, \
         and holds no node that holds itself",
        Node::DEEPEST
    ))
}

/// Where a node goes in its parent's.
#[derive(Clone, Copy)]
enum Slot {
    /// Among its children, at a place from 1.
    Child(usize),
    /// Its row's template, a list's.
    Row,
}

/// A node's place in its tree, as an error tells it: the root's, or the
/// path from it, as `children[2].row`.
fn place(parents: &[Option<(usize, Slot)>], mut parent: Option<(usize, Slot)>) -> String {
    let mut slots = Vec::new();
    while let Some((at, slot)) = parent {
        slots.push(slot);
        parent = parents.get(at).copied().flatten();
    }
    if slots.is_empty() {
        return "the view's root".to_owned();
    }
    let mut path = String::from("the node at ");
    for (n, slot) in slots.iter().rev().enumerate() {
        if n > 0 {
            path.push('.');
        }
        match slot {
            Slot::Child(at) => {
                let _ = write!(path, "children[{at}]");
            }
            Slot::Row => path.push_str("row"),
        }
    }
    path
}

/// The tree of `value`, a plugin's view: why not, of a node of another
/// shape, a tree too deep, or too many values.
pub(crate) fn from_lua(value: Value) -> Result<Node, ShapeError> {
    let budget = Budget::default();
    // The nodes, each before those inside it, without their children and
    // their rows, and where each goes in its parent's.
    let mut nodes = Vec::new();
    let mut parents: Vec<Option<(usize, Slot)>> = Vec::new();
    let mut left = vec![(value, 1, None)];
    budget.hold(1)?;
    while let Some((value, depth, parent)) = left.pop() {
        budget.release();
        if depth > Node::DEEPEST {
            return Err(too_deep().within(place(&parents, parent)));
        }
        let (node, children, row) =
            one(value, &budget).map_err(|error| error.within(place(&parents, parent)))?;
        let at = nodes.len();
        nodes.push(node);
        parents.push(parent);
        budget.hold(children.len() + usize::from(row.is_some()))?;
        if let Some(row) = row {
            left.push((row, depth + 1, Some((at, Slot::Row))));
        }
        // In reverse, so that the walk meets them in their order.
        for (n, child) in children.into_iter().enumerate().rev() {
            left.push((child, depth + 1, Some((at, Slot::Child(n + 1)))));
        }
    }
    // Each node into its parent, the last first: a node's children all
    // come after it, so that each is whole as it goes into its parent, and
    // its children went in from the last.
    let mut nodes: Vec<Option<Node>> = nodes.into_iter().map(Some).collect();
    for at in (1..nodes.len()).rev() {
        let (Some(mut node), Some((parent, slot))) = (nodes[at].take(), parents[at]) else {
            continue;
        };
        node.children.reverse();
        let Some(parent) = nodes[parent].as_mut() else {
            continue;
        };
        match (slot, &mut parent.kind) {
            (Slot::Row, Kind::List(list)) => *list.row = node,
            _ => parent.children.push(node),
        }
    }
    let mut root = nodes
        .into_iter()
        .next()
        .flatten()
        .ok_or_else(|| ShapeError("a view is a node".to_owned()))?;
    root.children.reverse();
    Ok(root)
}

/// The value of `T` of `value`, a node's field `name`.
fn field<T: DeserializeOwned>(value: Value, name: &str, budget: &Budget) -> Result<T, ShapeError> {
    T::deserialize(Taking::new(value, budget)).map_err(|error| error.within(format!("`{name}`")))
}

/// The value of `T` of `value`, a node's field `name`, its default of
/// `nil`.
fn or_default<T: DeserializeOwned + Default>(
    value: Value,
    name: &str,
    budget: &Budget,
) -> Result<T, ShapeError> {
    if value.is_nil() {
        Ok(T::default())
    } else {
        field(value, name, budget)
    }
}

/// The node of `value`, without its children and its row, which it gives
/// as they are.
fn one(value: Value, budget: &Budget) -> Result<(Node, Vec<Value>, Option<Value>), ShapeError> {
    budget.count()?;
    let Value::Table(table) = value else {
        return Err(ShapeError(format!(
            "expected a node, a table, not {}",
            shape::what(&value)
        )));
    };
    let mut fields = BTreeMap::new();
    for (key, value) in shape::entries(&table, budget.room())? {
        let Value::String(key) = &key else {
            return Err(ShapeError(format!(
                "a node's field is named by a text, not {}",
                shape::what(&key)
            )));
        };
        let key = key
            .to_str()
            .map_err(|_| ShapeError("a node's field is named by a text of UTF-8".to_owned()))?
            .to_owned();
        fields.insert(key, value);
    }
    let kind = match fields.remove("kind") {
        Some(Value::String(kind)) => kind
            .to_str()
            .map_err(|_| ShapeError("a node's `kind` is a text of UTF-8".to_owned()))?
            .to_owned(),
        Some(other) => {
            return Err(ShapeError(format!(
                "`kind`: expected a text, not {}",
                shape::what(&other)
            )));
        }
        None => {
            return Err(ShapeError(format!(
                "a node takes its `kind`, of {}",
                KINDS.join(", ")
            )));
        }
    };
    let own = own(&kind).ok_or_else(|| {
        ShapeError(format!(
            "`{kind}` is no kind of node, of {}",
            KINDS.join(", ")
        ))
    })?;
    if let Some(unknown) = fields
        .keys()
        .find(|key| !NODE.contains(&key.as_str()) && !own.contains(&key.as_str()))
    {
        let names: Vec<&str> = NODE
            .iter()
            .chain(own)
            .copied()
            .filter(|name| *name != "kind")
            .collect();
        return Err(ShapeError(format!(
            "no field `{unknown}` of a {kind} node, of {}",
            names.join(", ")
        )));
    }
    let mut take = |name: &str| fields.remove(name).unwrap_or(Value::Nil);
    let mut row = None;
    let kind = match kind.as_str() {
        "text" => Kind::Text(field(take("text"), "text", budget)?),
        "icon" => Kind::Icon(field(take("icon"), "icon", budget)?),
        "spacer" => Kind::Spacer,
        "field" => Kind::Field(Field {
            placeholder: or_default(take("placeholder"), "placeholder", budget)?,
        }),
        "button" => Kind::Button(Button {
            label: or_default(take("text"), "text", budget)?,
            icon: or_default(take("icon"), "icon", budget)?,
            disabled: or_default(take("disabled"), "disabled", budget)?,
        }),
        "toggle" => Kind::Toggle(Toggle {
            label: or_default(take("text"), "text", budget)?,
            icon: or_default(take("icon"), "icon", budget)?,
            on: or_default(take("on"), "on", budget)?,
            disabled: or_default(take("disabled"), "disabled", budget)?,
        }),
        "separator" => Kind::Separator,
        "list" => {
            let given = take("row");
            if given.is_nil() {
                return Err(ShapeError(
                    "a list node takes its `row`, the node of a row's template".to_owned(),
                ));
            }
            row = Some(given);
            Kind::List(List {
                // Its row's node goes in as the tree is whole.
                row: Box::new(Node::new(Kind::Spacer)),
                count: field(take("count"), "count", budget)?,
                first: or_default(take("first"), "first", budget)?,
                items: or_default(take("items"), "items", budget)?,
                row_height: field(take("row_height"), "row_height", budget)?,
                menu: or_default(take("menu"), "menu", budget)?,
            })
        }
        "scroll" => Kind::Scroll,
        "progress" => Kind::Progress(or_default(take("progress"), "progress", budget)?),
        "tabs" => Kind::Tabs(Tabs {
            tabs: or_default(take("tabs"), "tabs", budget)?,
            chosen: or_default(take("chosen"), "chosen", budget)?,
            distribution: or_default(take("distribution"), "distribution", budget)?,
        }),
        // A container, the kind left, as `own` took it.
        _ => Kind::Container,
    };
    let children = match take("children") {
        Value::Nil => Vec::new(),
        Value::Table(list) => shape::sequence(&list, budget.room())?.ok_or_else(|| {
            ShapeError(
                "`children`: expected a list of nodes, a table of keys from 1 to its length alone"
                    .to_owned(),
            )
        })?,
        other => {
            return Err(ShapeError(format!(
                "`children`: expected a list of nodes, not {}",
                shape::what(&other)
            )));
        }
    };
    let node = Node {
        key: field(take("key"), "key", budget)?,
        kind,
        layout: or_default(take("layout"), "layout", budget)?,
        style: or_default(take("style"), "style", budget)?,
        focusable: or_default(take("focusable"), "focusable", budget)?,
        reorderable: or_default(take("reorderable"), "reorderable", budget)?,
        carries: field(take("carries"), "carries", budget)?,
        tooltip: field(take("tooltip"), "tooltip", budget)?,
        label: field(take("label"), "label", budget)?,
        role: field(take("role"), "role", budget)?,
        children: Vec::new(),
    };
    Ok((node, children, row))
}

#[cfg(any(test, feature = "testing"))]
pub(crate) use giving::to_lua;

/// A tree as Luau's tables, which the host gives the tests.
#[cfg(any(test, feature = "testing"))]
mod giving {
    use lokyr_plugin_sdk::view::{Button, Field, Kind, Layout, List, Node, Style, Tabs, Toggle};
    use mlua::{Lua, Table};
    use serde::Serialize;

    use crate::shape::{self, ShapeError};

    fn kind_name(kind: &Kind) -> &'static str {
        match kind {
            Kind::Container => "container",
            Kind::Text(_) => "text",
            Kind::Icon(_) => "icon",
            Kind::Spacer => "spacer",
            Kind::Field(_) => "field",
            Kind::Button(_) => "button",
            Kind::Toggle(_) => "toggle",
            Kind::Separator => "separator",
            Kind::List(_) => "list",
            Kind::Scroll => "scroll",
            Kind::Progress(_) => "progress",
            Kind::Tabs(_) => "tabs",
        }
    }

    /// Sets `table`'s field `name` to `value`, of a table of its fields or its
    /// value, left out of `nil`.
    fn set<T: Serialize + ?Sized>(
        lua: &Lua,
        table: &Table,
        name: &str,
        value: &T,
    ) -> Result<(), ShapeError> {
        table.raw_set(name, shape::to_lua(lua, value)?)?;
        Ok(())
    }

    /// The table of `node`'s tree, which the host gives a plugin, as its tests
    /// read a view: a node's fields of their defaults left out, but a kind's
    /// own.
    pub(crate) fn to_lua(lua: &Lua, node: &Node) -> Result<Table, ShapeError> {
        let root = lua.create_table()?;
        let mut left = vec![(node, root.clone())];
        while let Some((node, table)) = left.pop() {
            let Node {
                key,
                kind,
                layout,
                style,
                focusable,
                reorderable,
                carries,
                tooltip,
                label,
                role,
                children,
            } = node;
            set(lua, &table, "key", key)?;
            table.raw_set("kind", kind_name(kind))?;
            match kind {
                Kind::Container | Kind::Spacer | Kind::Separator | Kind::Scroll => {}
                Kind::Text(text) => table.raw_set("text", text.as_str())?,
                Kind::Icon(icon) => table.raw_set("icon", icon.as_str())?,
                Kind::Field(Field { placeholder }) => {
                    table.raw_set("placeholder", placeholder.as_str())?;
                }
                Kind::Button(Button {
                    label,
                    icon,
                    disabled,
                }) => {
                    table.raw_set("text", label.as_str())?;
                    table.raw_set("icon", icon.as_str())?;
                    table.raw_set("disabled", *disabled)?;
                }
                Kind::Toggle(Toggle {
                    label,
                    icon,
                    on,
                    disabled,
                }) => {
                    table.raw_set("text", label.as_str())?;
                    table.raw_set("icon", icon.as_str())?;
                    table.raw_set("on", *on)?;
                    table.raw_set("disabled", *disabled)?;
                }
                Kind::List(List {
                    row,
                    count,
                    first,
                    items,
                    row_height,
                    menu,
                }) => {
                    set(lua, &table, "count", count)?;
                    set(lua, &table, "first", first)?;
                    set(lua, &table, "items", items)?;
                    set(lua, &table, "row_height", row_height)?;
                    table.raw_set("menu", *menu)?;
                    let inside = lua.create_table()?;
                    table.raw_set("row", &inside)?;
                    left.push((row, inside));
                }
                Kind::Progress(progress) => set(lua, &table, "progress", progress)?,
                Kind::Tabs(Tabs {
                    tabs,
                    chosen,
                    distribution,
                }) => {
                    set(lua, &table, "tabs", tabs)?;
                    set(lua, &table, "chosen", chosen)?;
                    set(lua, &table, "distribution", distribution)?;
                }
            }
            if *layout != Layout::default() {
                set(lua, &table, "layout", layout)?;
            }
            if *style != Style::default() {
                set(lua, &table, "style", style)?;
            }
            if *focusable {
                table.raw_set("focusable", true)?;
            }
            if *reorderable {
                table.raw_set("reorderable", true)?;
            }
            set(lua, &table, "carries", carries)?;
            set(lua, &table, "tooltip", tooltip)?;
            set(lua, &table, "label", label)?;
            set(lua, &table, "role", role)?;
            if !children.is_empty() {
                let list = lua.create_table_with_capacity(children.len(), 0)?;
                for child in children {
                    let inside = lua.create_table()?;
                    list.raw_push(&inside)?;
                    left.push((child, inside));
                }
                table.raw_set("children", list)?;
            }
        }
        Ok(root)
    }
}

#[cfg(test)]
mod tests;
