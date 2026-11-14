use lokyr_plugin_sdk::event::WorkspaceEvent;
use lokyr_plugin_sdk::host::{Edge, Line, PaneId, SearchMatch, TabId, Viewport, WindowId};
use lokyr_plugin_sdk::message::{Data, Mods};
use lokyr_plugin_sdk::ui::{PanelId, ViewId};
use lokyr_plugin_sdk::view::{Key, UiEventKind};
use serde::Deserialize;

use super::*;
use crate::requests::{Traced, Tracing};
use crate::{events_through_luau, ui_event_through_luau};

/// An event of each of the workspace's kinds.
fn workspace() -> Vec<WorkspaceEvent> {
    let (window, tab, pane) = (WindowId(1), TabId(2), PaneId(3));
    vec![
        WorkspaceEvent::WindowOpened { window },
        WorkspaceEvent::WindowClosed { window },
        WorkspaceEvent::TabCreated {
            tab,
            window,
            index: 0,
        },
        WorkspaceEvent::TabClosed { tab, window },
        WorkspaceEvent::TabActivated { tab, window },
        WorkspaceEvent::TabMoved {
            tab,
            from: WindowId(4),
            window,
            index: 1,
        },
        WorkspaceEvent::PaneCreated { pane, tab },
        WorkspaceEvent::PaneClosed { pane, tab },
        WorkspaceEvent::PaneFocused { pane, tab },
        WorkspaceEvent::PaneMoved {
            pane,
            from: TabId(5),
            tab,
        },
        WorkspaceEvent::LayoutChanged { tab },
        WorkspaceEvent::TabChanged { tab },
        WorkspaceEvent::PaneChanged { pane, tab },
    ]
}

/// An event of each kind.
fn every() -> Vec<Event> {
    let pane = PaneId(3);
    let mut every: Vec<Event> = workspace().into_iter().map(Event::Workspace).collect();
    every.extend([
        Event::Pointer {
            pane,
            at: Some((4, -2)),
            mods: Mods {
                shift: true,
                command: true,
                ..Mods::default()
            },
        },
        Event::Pointer {
            pane,
            at: None,
            mods: Mods::default(),
        },
        Event::Lines {
            id: 7,
            pane,
            lines: vec![Line {
                row: -1,
                text: "日x".to_owned(),
                wrapped: true,
            }],
            complete: false,
        },
        Event::Searched {
            id: 8,
            pane,
            matches: vec![SearchMatch {
                first: (0, -3),
                last: (4, -3),
            }],
            count: 120_000,
            done: true,
        },
        Event::PanelClosed(PanelId::from("find")),
        Event::KeysChanged,
        Event::WindowFocus {
            window: WindowId(1),
            focused: false,
        },
        Event::ViewportChanged {
            pane,
            viewport: Viewport {
                top: -40,
                rows: 24,
                screen: 0,
                oldest: -1000,
                epoch: 2,
                alternate: true,
            },
        },
        Event::CommandStarted { pane, command: 4 },
        Event::CommandEnded {
            pane,
            command: 4,
            status: Some(1),
        },
        Event::CommandEnded {
            pane,
            command: 5,
            status: None,
        },
        Event::TabBarResized { width: 180.5 },
        Event::Answered {
            id: 9,
            text: Some("name".to_owned()),
        },
        Event::Answered { id: 9, text: None },
        Event::ConfigChanged,
        Event::Timer { id: 12 },
        Event::Custom {
            name: "acme.done".to_owned(),
            schema: "acme.done/1".to_owned(),
            value: Data::List(vec![Data::Integer(1), Data::Map(Default::default())]),
        },
        Event::Saved {
            id: 10,
            path: Some("/Users/me/Lokyr.png".to_owned()),
            error: None,
        },
        Event::Saved {
            id: 11,
            path: None,
            error: Some("it cannot be written".to_owned()),
        },
        Event::Package {
            id: 12,
            line: lokyr_plugin_sdk::message::Data::Map(
                [
                    (
                        "event".to_owned(),
                        lokyr_plugin_sdk::message::Data::Text("removed".to_owned()),
                    ),
                    (
                        "name".to_owned(),
                        lokyr_plugin_sdk::message::Data::Text("acme/hello".to_owned()),
                    ),
                ]
                .into(),
            ),
        },
        Event::PackagesEnded { id: 12, ok: true },
        Event::PluginsChanged,
        Event::SettingsChanged,
        Event::PageShown("plugins".to_owned()),
        Event::PageHidden("plugins".to_owned()),
    ]);
    every
}

#[test]
fn every_event_is_a_flat_table_of_its_kind_and_its_fields_both_ways() {
    let every = every();
    events_through_luau(&every).unwrap();
    // Of every kind of the SDK's, the workspace's too.
    let lua = Lua::new();
    let list = list_to_lua(&lua, &every).unwrap();
    let kinds: Vec<String> = list
        .sequence_values::<Table>()
        .map(|event| event.unwrap().raw_get::<String>("kind").unwrap())
        .collect();
    for traced in [
        Event::deserialize(Tracing).unwrap_err(),
        WorkspaceEvent::deserialize(Tracing).unwrap_err(),
    ] {
        let Traced::Variants(variants) = traced else {
            panic!("an event is a variant");
        };
        for variant in variants.iter().filter(|variant| **variant != "workspace") {
            assert!(kinds.iter().any(|kind| kind == variant), "{variant}");
        }
    }
    // As a plugin reads them.
    lua.globals().set("events", list).unwrap();
    lua.load(
        r#"
        local created = events[3]
        assert(created.kind == "tab-created" and created.tab == 2 and created.window == 1 and created.index == 0)
        local pointer = events[14]
        assert(pointer.kind == "pointer" and pointer.at[1] == 4 and pointer.at[2] == -2 and pointer.mods.command)
        assert(events[15].at == nil)
        assert(events[18].kind == "panel-closed" and events[18].id == "find")
        local shown, hidden = events[#events - 1], events[#events]
        assert(shown.kind == "page-shown" and shown.page == "plugins")
        assert(hidden.kind == "page-hidden" and hidden.page == "plugins")
        local keys = events[19]
        assert(keys.kind == "keys-changed" and next(keys, next(keys)) == nil)
        assert(events[21].viewport.alternate == true)
        assert(events[24].kind == "command-ended" and events[24].status == nil)
        assert(events[29].kind == "timer" and events[29].id == 12)
        local custom = events[30]
        assert(custom.kind == "custom" and custom.name == "acme.done" and custom.value[1] == 1)
        local saved, failed = events[31], events[32]
        assert(saved.kind == "saved" and saved.path == "/Users/me/Lokyr.png" and saved.error == nil)
        assert(failed.path == nil and failed.error == "it cannot be written")
        "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn an_event_of_another_shape_is_refused() {
    let lua = Lua::new();
    for (source, error) in [
        ("{ tab = 1 }", "an event takes its `kind`"),
        ("{ kind = 1 }", "`kind`: expected a text, not a number"),
        (
            r#"{ kind = "tab-created", tab = "x", window = 1, index = 0 }"#,
            "expected a whole number",
        ),
        (
            r#"{ kind = "panel-closed", id = "x", extra = 1 }"#,
            "no field `extra` of a panel-closed event",
        ),
        (r#"{ kind = "tab-opened", tab = 1 }"#, "is no variant"),
        ("3", "expected an event, a table, not a number"),
    ] {
        let value = lua.load(format!("return {source}")).eval().unwrap();
        let given = from_lua(&lua, value).unwrap_err().0;
        assert!(given.contains(error), "{source}: {given}");
    }
}

/// The number of `UiEventKind`'s kinds.
const UI_KINDS: usize = 25;

/// The place of `kind`'s among `UiEventKind`'s kinds, from 0.
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

#[test]
fn a_view_s_event_is_a_flat_table_of_its_view_its_key_its_kind_and_its_fields() {
    let kinds = vec![
        UiEventKind::Click,
        UiEventKind::DoubleClick,
        UiEventKind::MiddleClick,
        UiEventKind::ContextMenu,
        UiEventKind::HoverEnter,
        UiEventKind::HoverLeave,
        UiEventKind::DragStart,
        UiEventKind::DragMove { x: 1.5, y: -2.0 },
        UiEventKind::DragEnd { x: 3.0, y: 4.25 },
        UiEventKind::DragCancel,
        UiEventKind::Reorder { index: 2 },
        UiEventKind::DropOnBar {
            window: WindowId(2),
            index: 1,
        },
        UiEventKind::DropOnPane {
            pane: PaneId(3),
            edge: Edge::Left,
        },
        UiEventKind::DropOutside { x: 100.5, y: -20.0 },
        UiEventKind::Scroll { x: 0.0, y: -12.5 },
        UiEventKind::Focus,
        UiEventKind::Blur,
        UiEventKind::SetValue {
            value: "x".to_owned(),
        },
        UiEventKind::InputChanged {
            text: "ab".to_owned(),
            revision: 3,
        },
        UiEventKind::Submit {
            text: "ab".to_owned(),
            shift: true,
        },
        UiEventKind::Toggle { on: true },
        UiEventKind::Select { item: 4 },
        UiEventKind::Activate { item: 5 },
        UiEventKind::ListRange {
            first: 10,
            count: 20,
        },
        UiEventKind::Cut { cut: true },
    ];
    let mut seen = [false; UI_KINDS];
    for kind in kinds {
        seen[ui_index(&kind)] = true;
        let event = UiEvent {
            view: ViewId::Panel(PanelId::from("find")),
            key: Key::from("query"),
            kind,
        };
        ui_event_through_luau(&event).unwrap();
    }
    assert!(seen.iter().all(|seen| *seen), "{seen:?}");
    // As a plugin reads it.
    let lua = Lua::new();
    let event = UiEvent {
        view: ViewId::PaneHeader(PaneId(3)),
        key: Key::from("query"),
        kind: UiEventKind::Submit {
            text: "x".to_owned(),
            shift: true,
        },
    };
    lua.globals()
        .set("event", ui_to_lua(&lua, &event).unwrap())
        .unwrap();
    lua.load(
        r#"
        assert(event.view.pane_header == 3 and event.key == "query")
        assert(event.kind == "submit" and event.text == "x" and event.shift == true)
        "#,
    )
    .exec()
    .unwrap();
}
