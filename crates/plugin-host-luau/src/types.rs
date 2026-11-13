//! The plugin API's types for Luau's type checker, `lokyr.d.luau`, made of
//! the SDK's own types as serde tells them, in the shapes that the front
//! end of Luau gives them: `cx`'s functions of each request, their
//! arguments and their answers, the events and the views' events of
//! their flat shapes, a view's node, and every type that they use, by its
//! Rust name.

mod sample;
mod trace;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use lokyr_plugin_sdk::host::{Edge, PaneId, WindowId};
use lokyr_plugin_sdk::message::{Event, Request, Response};
use lokyr_plugin_sdk::ui::ViewId;
use lokyr_plugin_sdk::view::{Node, UiEventKind};

use self::sample::describe;
use self::trace::{Field, Format, Id, Payload, Shape, Tracer};
use crate::{events, tree};

/// The types of one name that the API's types hold, each of a name of its
/// own in Luau, by its name and its first member.
const RENAMES: [(&str, &str, &str); 6] = [
    ("Direction", "row", "NodeDirection"),
    ("Kind", "container", "NodeKind"),
    ("Kind", "string", "ParamKind"),
    ("Layout", "direction", "NodeLayout"),
    ("Role", "group", "NodeRole"),
    ("Role", "tooltip", "PanelRole"),
];

/// The names of the parameters of the requests of a value, or of values,
/// which have no fields' names, where their types' do not tell them.
const PARAMS: [(&str, &[&str]); 19] = [
    ("subscribe", &["names"]),
    ("show_page", &["page"]),
    ("keep_awake", &["awake"]),
    ("packages", &["call"]),
    ("cancel_packages", &["job"]),
    ("drop_image", &["image"]),
    ("set_clipboard_image", &["png"]),
    ("preview_theme", &["name"]),
    ("preview_font", &["family"]),
    ("set_font", &["family"]),
    ("end_search", &["search"]),
    ("cancel_timer", &["timer"]),
    ("current_match", &["search"]),
    ("open", &["target"]),
    ("opens", &["target"]),
    ("set_font_size", &["size"]),
    ("set_tab_bar_place", &["place"]),
    ("set_inactive_dim", &["dim"]),
    ("set_state", &["state"]),
];

/// Luau's reserved words, which a table's field takes as a text alone, as
/// `["end"]`.
const RESERVED: [&str; 21] = [
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "if", "in", "local",
    "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

/// The callbacks of a plugin's table, by their names, of their parameters
/// past `cx` and of what they return.
const CALLBACKS: [(&str, &str, &str); 6] = [
    ("activate", "", "()"),
    ("on_command", "name: string, args: Args", "()"),
    ("applies", "name: string, args: Args", "boolean"),
    ("on_event", "events: { Event }", "()"),
    ("render", "view: ViewId", "Node?"),
    ("on_ui_event", "event: UiEvent", "()"),
];

/// The head of the file.
const HEAD: &str = "\
-- lokyr.d.luau: the types of Lokyr's plugin API, for Luau's type checker.
-- Made by `cargo xtask luau-types` of plugin-sdk's types: change those, not
-- this file.
--
-- A call of `cx` answers with its value, `true` of a write, or `nil`, the
-- error's message and its code, whose hundreds tell its category: 1xx
-- usage, 2xx failure, 3xx security, 4xx fault.

-- A plain value, as an event's, a state's or a config's.
export type Data = nil | boolean | number | string | { Data } | { [string]: Data }

-- A command's argument, and its arguments by their names.
export type Value = string | number | boolean
export type Args = { [string]: Value }

-- A capability, as a manifest writes it: \"pty-write\", or \"net:example.com\".
export type Capability = string

-- A failed call's error: its message, then its code.
export type Error = string
";

/// The Rust name of `name`, which serde gives: snake case for kebab case.
fn rust_name(name: &str) -> String {
    name.replace('-', "_")
}

/// A table's field of serde's `name`, its Rust name, as a text where Luau
/// reserves it.
fn field_name(name: &str) -> String {
    let name = rust_name(name);
    if RESERVED.contains(&name.as_str()) {
        format!("[\"{name}\"]")
    } else {
        name
    }
}

/// A parameter of serde's `name`, its Rust name, `_` after it where Luau
/// reserves it.
fn param(name: &str) -> String {
    let name = rust_name(name);
    if RESERVED.contains(&name.as_str()) {
        format!("{name}_")
    } else {
        name
    }
}

/// A named type of the registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Named {
    Type(Id),
    Newtype(&'static str),
}

/// The file's writer: the types traced, their names in Luau, and those
/// that the file uses, which it then declares.
struct Writer {
    shapes: BTreeMap<Id, Shape>,
    newtypes: BTreeMap<&'static str, Format>,
    names: BTreeMap<Named, String>,
    used: RefCell<BTreeSet<Named>>,
    /// The node's type, written of its flat shape rather than declared.
    flat: Named,
}

impl Writer {
    /// The writer of `tracer`'s types, each named in Luau, of its Rust
    /// name, or of [`RENAMES`] where two share one.
    fn new(tracer: &Tracer) -> Self {
        let shapes = tracer.shapes.borrow().clone();
        let newtypes = tracer.newtypes.borrow().clone();
        let mut names = BTreeMap::new();
        let mut by_name: BTreeMap<&str, Vec<Id>> = BTreeMap::new();
        for id in shapes.keys() {
            by_name.entry(id.name).or_default().push(*id);
        }
        for (name, ids) in &by_name {
            let renamed = |id: &Id| {
                RENAMES
                    .iter()
                    .find(|(old, first, _)| old == name && id.members.first() == Some(first))
                    .map(|(_, _, new)| *new)
            };
            // One of the name keeps it, the others of names of their own.
            let keeping = ids.iter().filter(|id| renamed(id).is_none()).count();
            assert!(
                keeping <= 1,
                "two types named {name}, of no name of their own in Luau: {ids:?}"
            );
            for id in ids {
                let luau = renamed(id).unwrap_or(name);
                names.insert(Named::Type(*id), luau.to_owned());
            }
        }
        for name in newtypes.keys() {
            assert!(
                !by_name.contains_key(name),
                "a newtype and a type both named {name}"
            );
            names.insert(Named::Newtype(name), (*name).to_owned());
        }
        let mut luau: Vec<&String> = names.values().collect();
        luau.sort();
        assert!(
            luau.windows(2).all(|pair| pair[0] != pair[1]),
            "two types of one name in Luau"
        );
        let flat = shapes
            .keys()
            .find(|id| id.name == "Node" && id.members.contains(&"kind"))
            .map(|id| Named::Type(*id))
            .expect("a node traced");
        Self {
            shapes,
            newtypes,
            names,
            used: RefCell::default(),
            flat,
        }
    }

    /// The shape of the struct or enum named `name` that holds `member`.
    fn find(&self, name: &str, member: &str) -> (Id, &Shape) {
        self.shapes
            .iter()
            .find(|(id, _)| id.name == name && id.members.contains(&member))
            .map_or_else(
                || panic!("no type {name} of {member} traced"),
                |(id, shape)| (*id, shape),
            )
    }

    /// The name in Luau of `named`, which the file then declares.
    fn name(&self, named: Named) -> String {
        self.used.borrow_mut().insert(named);
        self.names[&named].clone()
    }

    /// The type in Luau of `format`.
    fn ty(&self, format: &Format) -> String {
        match format {
            Format::Unit => "nil".to_owned(),
            Format::Bool => "boolean".to_owned(),
            Format::Number => "number".to_owned(),
            Format::Text => "string".to_owned(),
            Format::Bytes => "string | buffer".to_owned(),
            Format::Option(inner) => optional(&self.ty(inner)),
            Format::List(inner) => format!("{{ {} }}", self.ty(inner)),
            Format::Map(key, inner) => format!("{{ [{}]: {} }}", self.ty(key), self.ty(inner)),
            Format::Tuple(items) => {
                let types: BTreeSet<String> = items.iter().map(|item| self.ty(item)).collect();
                match types.len() {
                    1 => format!("{{ {} }}", types.into_iter().next().unwrap_or_default()),
                    _ => "{ any }".to_owned(),
                }
            }
            Format::Named(id) => self.name(Named::Type(*id)),
            Format::Newtype(name) => self.name(Named::Newtype(name)),
            Format::Plain(name) => (*name).to_owned(),
            Format::Unknown => "any".to_owned(),
        }
    }

    /// The fields of a table, `{ name: type, … }`, of `fields`, `extra`
    /// before them, each optional that its struct does not require, or
    /// that `optional` makes optional.
    fn table(&self, extra: &[String], fields: &[Field], all_optional: bool) -> String {
        let mut parts: Vec<String> = extra.to_vec();
        for field in fields {
            let ty = self.ty(&field.format);
            let ty = if field.required && !all_optional {
                ty
            } else {
                optional(&ty)
            };
            parts.push(format!("{}: {ty}", field_name(field.name)));
        }
        format!("{{ {} }}", parts.join(", "))
    }

    /// The type in Luau of a variant of `payload`, `name` serde's, of an
    /// enum of the SDK's shapes: its name's text, or a table of one field.
    fn variant(&self, name: &str, payload: &Payload) -> String {
        let key = field_name(name);
        match payload {
            Payload::Unit => format!("\"{name}\""),
            Payload::Value(format) => format!("{{ {key}: {} }}", self.ty(format)),
            Payload::Tuple(items) => {
                format!("{{ {key}: {} }}", self.ty(&Format::Tuple(items.clone())))
            }
            Payload::Fields(fields) => format!("{{ {key}: {} }}", self.table(&[], fields, false)),
        }
    }

    /// The declaration of `named`.
    fn declare(&self, named: Named) -> String {
        let name = self.names[&named].clone();
        match named {
            Named::Newtype(newtype) => {
                format!(
                    "export type {name} = {}\n",
                    self.ty(&self.newtypes[newtype])
                )
            }
            Named::Type(id) => match &self.shapes[&id] {
                Shape::Struct(fields) => {
                    let mut out = format!("export type {name} = {{\n");
                    for member in id.members {
                        let Some(field) = fields.iter().find(|field| field.name == *member) else {
                            continue;
                        };
                        let ty = self.ty(&field.format);
                        let ty = if field.required { ty } else { optional(&ty) };
                        let _ = writeln!(out, "    {}: {ty},", field_name(member));
                    }
                    out.push_str("}\n");
                    out
                }
                Shape::Enum(variants) => {
                    let mut out = format!("export type {name} =\n");
                    for (member, payload) in id.members.iter().zip(variants) {
                        let payload = payload
                            .as_ref()
                            .unwrap_or_else(|| panic!("{name}'s variant {member} untraced"));
                        let _ = writeln!(out, "    | {}", self.variant(member, payload));
                    }
                    out
                }
            },
        }
    }

    /// `cx`'s table, of a function of each request of `requests`, a value
    /// of each variant.
    fn cx(&self, requests: &[Request]) -> String {
        let (id, shape) = self.find("Request", "register-command");
        let Shape::Enum(variants) = shape else {
            panic!("a request is an enum");
        };
        let (response, _) = self.find("Response", "done");
        let Shape::Enum(answers) = &self.shapes[&response] else {
            panic!("a response is an enum");
        };
        let mut out = String::from("export type Cx = {\n");
        for (member, payload) in id.members.iter().zip(variants) {
            let payload = payload
                .as_ref()
                .unwrap_or_else(|| panic!("the request {member} untraced"));
            let request = requests
                .iter()
                .find(|request| variant_of(request) == rust_name(member))
                .unwrap_or_else(|| panic!("no request {member} traced"));
            let params = self.params(&rust_name(member), payload);
            let answer = request.answer();
            let at = response
                .members
                .iter()
                .position(|name| *name == answer)
                .unwrap_or_else(|| panic!("no response {answer}"));
            let value = match answers[at].as_ref() {
                Some(Payload::Unit) => "true".to_owned(),
                Some(Payload::Value(Format::Option(inner))) => self.ty(inner),
                Some(Payload::Value(format)) => self.ty(format),
                Some(Payload::Fields(fields)) => self.table(&[], fields, false),
                other => panic!("the response {answer} of {other:?}"),
            };
            let _ = writeln!(
                out,
                "    {}: ({params}) -> ({}, string?, number?),",
                rust_name(member),
                optional(&value)
            );
        }
        out.push_str("}\n");
        out
    }

    /// The parameters of the request `request` of `payload`, by their
    /// fields' names, by [`PARAMS`]', or by their types'.
    fn params(&self, request: &str, payload: &Payload) -> String {
        let named = PARAMS
            .iter()
            .find(|(name, _)| *name == request)
            .map(|(_, names)| *names);
        let values = |formats: &[Format]| {
            formats
                .iter()
                .enumerate()
                .map(|(at, format)| {
                    let ty = self.ty(format);
                    let name = named
                        .and_then(|names| names.get(at))
                        .map_or_else(|| param_name(&ty, at), |name| (*name).to_owned());
                    format!("{name}: {ty}")
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        match payload {
            Payload::Unit => String::new(),
            Payload::Value(format) => values(std::slice::from_ref(format)),
            Payload::Tuple(items) => values(items),
            Payload::Fields(fields) => fields
                .iter()
                .map(|field| {
                    let ty = self.ty(&field.format);
                    let ty = if field.required { ty } else { optional(&ty) };
                    format!("{}: {ty}", param(field.name))
                })
                .collect::<Vec<_>>()
                .join(", "),
        }
    }

    /// The events, of their flat shapes.
    fn events(&self) -> String {
        let (id, shape) = self.find("Event", events::WORKSPACE);
        let Shape::Enum(variants) = shape else {
            panic!("an event is an enum");
        };
        let mut out = String::from("export type Event =\n");
        for (member, payload) in id.members.iter().zip(variants) {
            let payload = payload
                .as_ref()
                .unwrap_or_else(|| panic!("the event {member} untraced"));
            let kind = |name: &str| format!("kind: \"{name}\"");
            match payload {
                Payload::Value(Format::Named(inner)) if *member == events::WORKSPACE => {
                    let Shape::Enum(changes) = &self.shapes[inner] else {
                        panic!("the workspace's events are an enum");
                    };
                    for (change, payload) in inner.members.iter().zip(changes) {
                        let Some(Payload::Fields(fields)) = payload else {
                            panic!("the workspace's event {change} of no fields");
                        };
                        let _ =
                            writeln!(out, "    | {}", self.table(&[kind(change)], fields, false));
                    }
                }
                Payload::Unit => {
                    let _ = writeln!(out, "    | {{ {} }}", kind(member));
                }
                Payload::Value(format) => {
                    let field = format!("{}: {}", events::value_field(member), self.ty(format));
                    let _ = writeln!(out, "    | {{ {}, {field} }}", kind(member));
                }
                Payload::Fields(fields) => {
                    let _ = writeln!(out, "    | {}", self.table(&[kind(member)], fields, false));
                }
                Payload::Tuple(_) => panic!("the event {member} of values"),
            }
        }
        out
    }

    /// The views' events, of their flat shapes, of a value of each kind.
    fn ui_events(&self, tracer: &Tracer) -> String {
        let mut out = String::from("export type UiEvent =\n");
        let head = [
            format!(
                "view: {}",
                self.ty(&Format::Named(self.find("ViewId", "panel").0))
            ),
            "key: string".to_owned(),
        ];
        for kind in ui_kinds() {
            let described =
                describe(&kind, tracer).unwrap_or_else(|error| panic!("{kind:?}: {error}"));
            let mut parts = head.to_vec();
            for (name, field) in &described.fields {
                if *name == "kind" {
                    let tag = field.text.clone().unwrap_or_default();
                    parts.push(format!("kind: \"{tag}\""));
                } else {
                    parts.push(format!("{}: {}", field_name(name), self.ty(&field.format)));
                }
            }
            let _ = writeln!(out, "    | {{ {} }}", parts.join(", "));
        }
        out
    }

    /// A view's node, a flat table of its fields and its kind's.
    fn node(&self) -> String {
        let (_, shape) = self.find("Node", "kind");
        let Shape::Struct(fields) = shape else {
            panic!("a node is a struct");
        };
        let (kind_id, kinds) = self.find("Kind", "container");
        let Shape::Enum(kinds) = kinds else {
            panic!("a node's kind is an enum");
        };
        let kinds_text: Vec<String> = tree::KINDS
            .iter()
            .map(|kind| format!("\"{kind}\""))
            .collect();
        let mut lines = vec![format!("    kind: {},", kinds_text.join(" | "))];
        for name in tree::NODE.iter().filter(|name| **name != "kind") {
            let field = fields
                .iter()
                .find(|field| rust_name(field.name) == *name)
                .unwrap_or_else(|| panic!("a node has no field {name}"));
            lines.push(format!(
                "    {name}: {},",
                optional(&self.ty(&field.format))
            ));
        }
        // Each kind's own fields, by their names in Luau, of its variant's
        // value: the value itself, of a field of the kind's name, as a
        // text's text, or the field of its struct's, a button's label its
        // text.
        let mut own: BTreeMap<&str, String> = BTreeMap::new();
        let mut order: Vec<&str> = Vec::new();
        for kind in tree::KINDS {
            let at = kind_id
                .members
                .iter()
                .position(|member| *member == kind)
                .unwrap_or_else(|| panic!("no kind {kind}"));
            let Some(payload) = &kinds[at] else {
                panic!("the kind {kind} untraced");
            };
            for field in tree::own(kind).unwrap_or_default() {
                let of = if *field == "text" { "label" } else { field };
                let inside = match payload {
                    Payload::Value(Format::Named(inner)) => match &self.shapes[inner] {
                        Shape::Struct(inside) => {
                            inside.iter().find(|inner| rust_name(inner.name) == of)
                        }
                        Shape::Enum(_) => None,
                    },
                    _ => None,
                };
                let ty = match (inside, payload) {
                    (Some(found), _) => match &found.format {
                        Format::Option(format) => self.ty(format),
                        format => self.ty(format),
                    },
                    (None, Payload::Value(format)) if *field == kind => self.ty(format),
                    (None, other) => panic!("the kind {kind}'s field {field} of {other:?}"),
                };
                match own.get(field) {
                    Some(known) => assert_eq!(known, &ty, "the field {field} of two types"),
                    None => {
                        own.insert(field, ty);
                        order.push(field);
                    }
                }
            }
        }
        for field in order {
            lines.push(format!("    {field}: {},", optional(&own[field])));
        }
        format!(
            "export type Node = {{\n{}\n}}\nexport type View = Node\n",
            lines.join("\n")
        )
    }
}

/// `ty`, optional: `ty?`, in parentheses where it is a union.
fn optional(ty: &str) -> String {
    if ty.ends_with('?') {
        ty.to_owned()
    } else if ty.contains(" | ") {
        format!("({ty})?")
    } else {
        format!("{ty}?")
    }
}

/// A parameter's name of its type `ty`, at `at` among its request's: an
/// id's of its object, as `pane`, or `value`.
fn param_name(ty: &str, at: usize) -> String {
    let base = match ty {
        "PaneId" => "pane",
        "TabId" => "tab",
        "WindowId" => "window",
        "PanelId" => "panel",
        "ViewId" => "view",
        "Command" => "command",
        "Binding" => "binding",
        "MenuItem" => "item",
        "ImageSpec" => "image",
        "string | buffer" => "bytes",
        "Data" => "value",
        "string" => "text",
        "number" => "number",
        "boolean" => "on",
        _ => "value",
    };
    if at == 0 {
        base.to_owned()
    } else {
        format!("{base}{}", at + 1)
    }
}

/// The variant's name of `request`, its Rust name in snake case.
fn variant_of(request: &Request) -> String {
    let debug = format!("{request:?}");
    let mut name = String::new();
    for char in debug.chars().take_while(char::is_ascii_alphanumeric) {
        if char.is_ascii_uppercase() && !name.is_empty() {
            name.push('_');
        }
        name.push(char.to_ascii_lowercase());
    }
    name
}

/// The number of the views' events' kinds.
const UI_KINDS: usize = 25;

/// A value of each kind of the views' events, which serde serializes
/// tagged inside, as no deserializer traces.
fn ui_kinds() -> Vec<UiEventKind> {
    let kinds = vec![
        UiEventKind::Click,
        UiEventKind::DoubleClick,
        UiEventKind::MiddleClick,
        UiEventKind::ContextMenu,
        UiEventKind::HoverEnter,
        UiEventKind::HoverLeave,
        UiEventKind::DragStart,
        UiEventKind::DragMove { x: 0.0, y: 0.0 },
        UiEventKind::DragEnd { x: 0.0, y: 0.0 },
        UiEventKind::DragCancel,
        UiEventKind::Reorder { index: 0 },
        UiEventKind::DropOnBar {
            window: WindowId(1),
            index: 0,
        },
        UiEventKind::DropOnPane {
            pane: PaneId(1),
            edge: Edge::Left,
        },
        UiEventKind::DropOutside { x: 0.0, y: 0.0 },
        UiEventKind::Scroll { x: 0.0, y: 0.0 },
        UiEventKind::Focus,
        UiEventKind::Blur,
        UiEventKind::SetValue {
            value: String::new(),
        },
        UiEventKind::InputChanged {
            text: String::new(),
            revision: 0,
        },
        UiEventKind::Submit {
            text: String::new(),
            shift: false,
        },
        UiEventKind::Toggle { on: false },
        UiEventKind::Select { item: 0 },
        UiEventKind::Activate { item: 0 },
        UiEventKind::ListRange { first: 0, count: 0 },
        UiEventKind::Cut { cut: false },
    ];
    let mut seen = [false; UI_KINDS];
    for kind in &kinds {
        seen[ui_index(kind)] = true;
    }
    assert!(seen.iter().all(|seen| *seen), "a view's event of no value");
    kinds
}

/// The place of `kind`'s among the views' events' kinds, from 0, which a
/// new kind's makes the build ask for.
fn ui_index(kind: &UiEventKind) -> usize {
    match kind {
        UiEventKind::Click => 0,
        UiEventKind::DoubleClick => 1,
        UiEventKind::MiddleClick => 2,
        UiEventKind::ContextMenu => 3,
        UiEventKind::HoverEnter => 4,
        UiEventKind::HoverLeave => 5,
        UiEventKind::DragStart => 6,
        UiEventKind::DragMove { .. } => 7,
        UiEventKind::DragEnd { .. } => 8,
        UiEventKind::DragCancel => 9,
        UiEventKind::Reorder { .. } => 10,
        UiEventKind::DropOnBar { .. } => 11,
        UiEventKind::DropOnPane { .. } => 12,
        UiEventKind::DropOutside { .. } => 13,
        UiEventKind::Scroll { .. } => 14,
        UiEventKind::Focus => 15,
        UiEventKind::Blur => 16,
        UiEventKind::SetValue { .. } => 17,
        UiEventKind::InputChanged { .. } => 18,
        UiEventKind::Submit { .. } => 19,
        UiEventKind::Toggle { .. } => 20,
        UiEventKind::Select { .. } => 21,
        UiEventKind::Activate { .. } => 22,
        UiEventKind::ListRange { .. } => 23,
        UiEventKind::Cut { .. } => 24,
    }
}

/// The host of a plugin's tests, which `sleip test` gives each test: its
/// functions, of the types that the definitions declare.
const TEST_HOST: &str = "
-- The host of a plugin's tests, which `sleip test` gives each test of
-- `tests/*.luau`: the plugin made anew, on a host of the tests, of a pane
-- that has the focus. A call that the plugin fails raises its error, which
-- fails the test.
export type TestHost = {
    -- Activates the plugin.
    activate: () -> (),
    -- Runs its command `name`, of `args`.
    command: (name: string, args: Args?) -> (),
    -- Whether its command `name`, of `args`, applies, as a menu asks.
    applies: (name: string, args: Args?) -> boolean,
    -- Gives it `events`, as the app posts them.
    events: (events: { Event }) -> (),
    -- Gives it the event of a node of one of its views.
    ui_event: (event: UiEvent) -> (),
    -- Its tree of `view`, as it renders it, if it renders one; a render that fails, or a tree that the host
    -- refuses, as a titlebar button that is none, raises its error.
    render: (view: ViewId) -> Node?,
    -- The ids of its panels that show, in their order.
    panels: () -> { PanelId },
    -- Moves the host's time on by `milliseconds`, its timers' events given.
    advance: (milliseconds: number) -> (),
    -- The writes that its calls made since the last look, each a table of
    -- its request's name, as `{ Notify = { ... } }`.
    done: () -> { { [string]: any } },
    -- The names of the commands that it registered.
    commands: () -> { string },
    -- The clipboard's text.
    clipboard: () -> string?,
    -- Sets the clipboard's text.
    set_clipboard: (text: string?) -> (),
    -- Gives its message `id` the text `text`, or takes it away.
    set_message: (id: string, text: string?) -> (),
    -- Its state, as it last wrote it.
    state: () -> Data,
    -- Sets its section of the config.
    set_config: (config: Data) -> (),
    -- Gives the pane's view the rows `rows`, from its top, as a read of the
    -- screen's cells gives them.
    set_screen: (rows: { CellRow }) -> (),
    -- Gives the pane a selection of the cells `rows`, or none.
    set_selection: (rows: { CellRow }?) -> (),
    -- The PNG on the clipboard, if the plugin put one there.
    clipboard_image: () -> string?,
    -- The saves whose panels show, in the order of their ids.
    saves: () -> { { id: number, name: string } },
    -- Closes the panel of the save `id` as the user chose `path`, whose
    -- file then holds its bytes, or cancelled, of none; the plugin hears of
    -- it.
    answer_save: (id: number, path: string?) -> (),
    -- The bytes of the file that a save wrote at `path`, if one did.
    saved_file: (path: string) -> string?,
    -- The ids of the images that the host keeps of the plugin's, the oldest
    -- first.
    kept_images: () -> { number },
    -- What the plugin keeps from idle sleep.
    awake: () -> Awake,
    -- The focused window, and the tab that it shows.
    window: () -> WindowId,
    tab: () -> TabId,
    -- Opens a tab after the focused window's others, as the user opens one; the plugin hears of it.
    open_tab: () -> TabId,
    -- Gives the tab `tab`, or the focused window's, the marks `marks` that a read of the tabs tells, as of a
    -- program that runs in it, `running`; the plugin hears `tab-changed` as they change.
    set_tab_marks: (marks: { activity: boolean?, bell: boolean?, printing: boolean?, running: boolean?,
        alternate: boolean?, secure: boolean? }, tab: TabId?) -> (),
    -- Its items of the menus, in their order.
    menu_items: () -> { MenuItem },
}
";

/// `lokyr.d.luau`, the plugin API's types for Luau's type checker, made of
/// the SDK's types.
pub fn definitions() -> String {
    let tracer = Tracer::default();
    let requests = tracer.trace::<Request>();
    tracer.trace::<Response>();
    tracer.trace::<Event>();
    tracer.trace::<Node>();
    tracer.trace::<ViewId>();
    let writer = Writer::new(&tracer);
    let mut out = String::from(HEAD);
    for part in [
        writer.cx(&requests),
        writer.events(),
        writer.ui_events(&tracer),
        writer.node(),
    ] {
        out.push('\n');
        out.push_str(&part);
    }
    let callbacks: Vec<String> = CALLBACKS
        .iter()
        .map(|(name, params, returns)| {
            let params = if params.is_empty() {
                "cx: Cx".to_owned()
            } else {
                format!("cx: Cx, {params}")
            };
            format!("    {name}: (({params}) -> {returns})?,")
        })
        .collect();
    let _ = write!(
        out,
        "\n-- A plugin's table of callbacks, which its entry returns.\nexport type Plugin = {{\n{}\n}}\n",
        callbacks.join("\n")
    );
    // Of the types that `Cx` and the callbacks use, declared after.
    out.push_str(TEST_HOST);
    // Every type that those use, and those that they use in turn, by name.
    let mut declared: BTreeMap<String, String> = BTreeMap::new();
    let mut done: BTreeSet<Named> = BTreeSet::new();
    loop {
        let pending: Vec<Named> = writer
            .used
            .borrow()
            .iter()
            .filter(|named| !done.contains(*named))
            .copied()
            .collect();
        if pending.is_empty() {
            break;
        }
        for named in pending {
            done.insert(named);
            if named != writer.flat {
                declared.insert(writer.names[&named].clone(), writer.declare(named));
            }
        }
    }
    for declaration in declared.values() {
        out.push('\n');
        out.push_str(declaration);
    }
    out
}

#[cfg(test)]
mod tests;
