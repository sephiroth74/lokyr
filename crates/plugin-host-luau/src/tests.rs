use std::collections::BTreeMap;

use lokyr_plugin_sdk::command::{Args, Code, Value as Arg};
use lokyr_plugin_sdk::fake::FakeHost;
use lokyr_plugin_sdk::message::{Data, Event};
use lokyr_plugin_sdk::plugin::Plugin;
use lokyr_plugin_sdk::ui::{PanelId, UiEvent, ViewId};
use lokyr_plugin_sdk::view::{Key, Kind, Node, UiEventKind};

use super::*;

/// A host that answers each request as done, for a call made outside the
/// fake host, whose failure the test takes.
struct Nowhere;

impl lokyr_plugin_sdk::host::Host for Nowhere {
    fn handle(
        &mut self,
        _: lokyr_plugin_sdk::message::Call,
    ) -> lokyr_plugin_sdk::message::Response {
        lokyr_plugin_sdk::message::Response::Done
    }
}

/// Runs `call` of a context on [`Nowhere`].
fn nowhere<T>(call: impl FnOnce(&mut Cx<'_>) -> T) -> T {
    call(&mut Cx::new(&mut Nowhere, "acme/test"))
}

/// The plugin of the entry `entry`.
fn plugin(entry: &str) -> LuauPlugin {
    LuauPlugin::new(
        "acme/test",
        Package::new([(ENTRY.to_owned(), entry.to_owned())]),
        Execution::Native,
    )
}

/// A plugin of every callback, each of requests to its host.
const EVERY: &str = r#"
--!strict
local Plugin = {}

function Plugin.activate(cx)
    assert(cx.register_command({ name = "acme.copy", title = "Copy", category = "clipboard" }))
    assert(cx.bind({ keys = "cmd+shift+c", command = "acme.copy" }))
    assert(cx.subscribe({ "keys-changed", "acme.done" }))
end

function Plugin.on_command(cx, name, args)
    if name ~= "acme.copy" then
        error("no command " .. name)
    end
    assert(cx.set_clipboard_text(args.text .. "!"))
end

function Plugin.applies(cx, name, args)
    return args.on == true
end

function Plugin.on_event(cx, events)
    assert(events[2].kind == "custom" and events[2].name == "acme.done")
    assert(cx.set_state({ count = #events, first = events[1].kind, value = events[2].value }))
end

function Plugin.render(cx, view)
    return { key = "label", kind = "text", text = "hi " .. view.panel }
end

function Plugin.on_ui_event(cx, event)
    assert(cx.set_state({ key = event.key, view = event.view.panel, kind = event.kind }))
end

return Plugin
"#;

#[test]
fn a_plugin_s_callbacks_run_as_the_host_calls_them_of_their_arguments() {
    let mut plugin = plugin(EVERY);
    let mut host = FakeHost::new("acme/test");
    host.activate(&mut plugin).unwrap();
    assert_eq!(host.commands()[0].name, "acme.copy");
    assert_eq!(host.bindings()[0].keys, "cmd+shift+c");
    assert_eq!(host.subscriptions(), ["keys-changed", "acme.done"]);
    // A command of its arguments.
    let args = Args::from([("text".to_owned(), Arg::String("x".to_owned()))]);
    host.command(&mut plugin, "acme.copy", args).unwrap();
    assert_eq!(host.clipboard(), Some("x!"));
    let error = host
        .command(&mut plugin, "acme.paste", Args::new())
        .unwrap_err();
    assert_eq!(error.code, Code::Failed);
    assert!(error.message.contains("no command acme.paste"), "{error}");
    // A look, as a menu asks.
    let on = |on: bool| Args::from([("on".to_owned(), Arg::Boolean(on))]);
    assert!(host.applies(&mut plugin, "acme.copy", &on(true)));
    assert!(!host.applies(&mut plugin, "acme.copy", &on(false)));
    // Events, as a batch.
    host.events(
        &mut plugin,
        &[
            Event::KeysChanged,
            Event::Custom {
                name: "acme.done".to_owned(),
                schema: "acme.done/1".to_owned(),
                value: Data::List(vec![Data::Integer(1), Data::Text("two".to_owned())]),
            },
        ],
    )
    .unwrap();
    assert_eq!(
        host.state(),
        &Data::Map(BTreeMap::from([
            ("count".to_owned(), Data::Integer(2)),
            ("first".to_owned(), Data::Text("keys-changed".to_owned())),
            (
                "value".to_owned(),
                Data::List(vec![Data::Integer(1), Data::Text("two".to_owned())])
            ),
        ]))
    );
    // A render, and a node's event.
    let view = ViewId::Panel(PanelId::from("find"));
    let tree = host.render(&mut plugin, &view).unwrap();
    assert_eq!(tree.key, Some(Key::from("label")));
    assert_eq!(tree.kind, Kind::Text("hi find".to_owned()));
    host.ui_event(
        &mut plugin,
        &UiEvent {
            view: view.clone(),
            key: Key::from("label"),
            kind: UiEventKind::Click,
        },
    )
    .unwrap();
    assert_eq!(
        host.state(),
        &Data::Map(BTreeMap::from([
            ("key".to_owned(), Data::Text("label".to_owned())),
            ("view".to_owned(), Data::Text("find".to_owned())),
            ("kind".to_owned(), Data::Text("click".to_owned())),
        ]))
    );
}

#[test]
fn a_plugin_of_no_callbacks_does_what_the_sdk_s_defaults_do() {
    let mut plugin = plugin("return {}");
    let mut host = FakeHost::new("acme/test");
    host.activate(&mut plugin).unwrap();
    assert!(host.applies(&mut plugin, "acme.go", &Args::new()));
    host.events(&mut plugin, &[Event::KeysChanged]).unwrap();
    assert_eq!(
        host.render(&mut plugin, &ViewId::Panel(PanelId::from("x"))),
        None
    );
    let error = host
        .command(&mut plugin, "acme.go", Args::new())
        .unwrap_err();
    assert_eq!(error.code, Code::NotFound);
    assert!(error.message.contains("no `on_command`"), "{error}");
}

#[test]
fn a_render_or_a_look_that_fails_tells_it_after_it_its_last_tree_staying() {
    let entry = r#"
local renders = 0
return {
    render = function(cx)
        renders += 1
        if renders == 2 then
            error("on purpose")
        elseif renders == 3 then
            return { kind = "text", text = 3 }
        end
        return { kind = "text", text = "fine" }
    end,
    applies = function(cx, name)
        if name == "acme.fails" then
            error("on purpose")
        elseif name == "acme.nil" then
            return nil
        end
        return true
    end,
}
"#;
    let mut plugin = plugin(entry);
    let mut host = FakeHost::new("acme/test");
    host.activate(&mut plugin).unwrap();
    let view = ViewId::Panel(PanelId::from("x"));
    let fine = Node::text("fine");
    assert_eq!(host.render(&mut plugin, &view), Some(fine.clone()));
    // A Lua error, and a tree of the wrong shape: the last stays.
    assert_eq!(host.render(&mut plugin, &view), Some(fine.clone()));
    assert_eq!(plugin.take_failure(), None, "the host took it");
    nowhere(|cx| plugin.render(cx, &view));
    let failure = plugin.take_failure().unwrap();
    assert_eq!(failure.code, Code::Failed);
    assert!(failure.message.contains("returns a view"), "{failure}");
    assert!(
        failure.message.contains("expected a text, not a number"),
        "{failure}"
    );
    assert_eq!(host.render(&mut plugin, &view), Some(fine));
    // A look that fails, or returns no boolean, does not apply.
    assert!(host.applies(&mut plugin, "acme.go", &Args::new()));
    assert!(!host.applies(&mut plugin, "acme.fails", &Args::new()));
    assert_eq!(plugin.take_failure(), None, "the host took it");
    assert!(!host.applies(&mut plugin, "acme.nil", &Args::new()));
    assert_eq!(plugin.take_failure(), None, "the host took it");
    assert!(!nowhere(|cx| plugin.applies(cx, "acme.nil", &Args::new())));
    let failure = plugin.take_failure().unwrap();
    assert!(
        failure.message.contains("returns true or false"),
        "{failure}"
    );
}

#[test]
fn a_render_past_its_deadline_is_a_fault() {
    let mut plugin =
        plugin("return { render = function() while true do end end }").with_deadlines(Deadlines {
            render: std::time::Duration::from_millis(20),
            ..Deadlines::default()
        });
    let mut host = FakeHost::new("acme/test");
    host.activate(&mut plugin).unwrap();
    let view = ViewId::Panel(PanelId::from("x"));
    assert_eq!(nowhere(|cx| plugin.render(cx, &view)), None);
    let failure = plugin.take_failure().unwrap();
    assert_eq!(failure.code, Code::Fault);
    assert!(
        failure
            .message
            .contains("past its deadline of 20 ms in `render`"),
        "{failure}"
    );
}

#[test]
fn a_view_s_conversion_stops_at_its_render_s_deadline() {
    // 500 000 children of one table, which the render returns at once and
    // the host would convert for long past its 16 ms.
    let entry = r#"
local nodes = {}
return {
    activate = function()
        local node = { kind = "spacer" }
        for i = 1, 500000 do nodes[i] = node end
    end,
    render = function() return { kind = "container", children = nodes } end,
}
"#;
    let mut plugin = plugin(entry).with_deadlines(Deadlines {
        activate: std::time::Duration::from_secs(10),
        ..Deadlines::default()
    });
    let mut host = FakeHost::new("acme/test");
    host.activate(&mut plugin).unwrap();
    let view = ViewId::Panel(PanelId::from("x"));
    let start = std::time::Instant::now();
    assert_eq!(nowhere(|cx| plugin.render(cx, &view)), None);
    let took = start.elapsed();
    let failure = plugin.take_failure().unwrap();
    assert_eq!(failure.code, Code::Fault, "{failure}");
    assert!(failure.message.contains("past its deadline"), "{failure}");
    assert!(took < std::time::Duration::from_millis(250), "{took:?}");
    // A request's arguments alike, of a million values, at a command's
    // deadline.
    let entry = r#"
local big = {}
return {
    activate = function(cx)
        for i = 1, 1000000 do big[i] = i end
        cx.register_command({ name = "acme.big", title = "Big", category = "view" })
    end,
    on_command = function(cx) cx.set_state(big) end,
}
"#;
    let mut plugin = self::plugin(entry).with_deadlines(Deadlines {
        activate: std::time::Duration::from_secs(10),
        command: std::time::Duration::from_millis(20),
        ..Deadlines::default()
    });
    host.activate(&mut plugin).unwrap();
    let start = std::time::Instant::now();
    let error = host
        .command(&mut plugin, "acme.big", Args::new())
        .unwrap_err();
    let took = start.elapsed();
    assert_eq!(error.code, Code::Fault, "{error}");
    assert!(took < std::time::Duration::from_millis(250), "{took:?}");
    // A common view as fast as before, well within its deadline.
    let common = r#"
return {
    render = function()
        local rows = {}
        for i = 1, 200 do
            rows[i] = { kind = "container", key = "row-" .. i, layout = { gap = 4 },
                children = { { kind = "icon", icon = "file" }, { kind = "text", text = "row " .. i } } }
        end
        return { kind = "scroll", key = "rows", children = rows }
    end,
}
"#;
    let mut plugin = self::plugin(common);
    host.activate(&mut plugin).unwrap();
    let tree = nowhere(|cx| plugin.render(cx, &view)).unwrap();
    assert_eq!(tree.children.len(), 200);
    assert_eq!(plugin.take_failure(), None);
}

#[test]
fn a_value_s_texts_are_bounded_by_their_bytes() {
    // One text of 4 MiB, which the VM holds once, 8 times in a view and in
    // a state: refused as they would copy 32 MiB, as a plugin's tables
    // could hold it millions of times.
    let entry = r#"
local s = string.rep("x", 4 * 1024 * 1024)
local t, nodes = {}, {}
local node = { kind = "text", text = s }
for i = 1, 8 do t[i] = s; nodes[i] = node end
return {
    activate = function(cx)
        cx.register_command({ name = "acme.big", title = "Big", category = "view" })
    end,
    on_command = function(cx) cx.set_state({ big = t }) end,
    render = function() return { kind = "container", children = nodes } end,
}
"#;
    let mut plugin = plugin(entry).with_deadlines(Deadlines {
        render: std::time::Duration::from_secs(5),
        command: std::time::Duration::from_secs(5),
        ..Deadlines::default()
    });
    let mut host = FakeHost::new("acme/test");
    host.activate(&mut plugin).unwrap();
    let view = ViewId::Panel(PanelId::from("x"));
    assert_eq!(nowhere(|cx| plugin.render(cx, &view)), None);
    let failure = plugin.take_failure().unwrap();
    assert!(
        failure.message.contains("16 MiB of texts at most"),
        "{failure}"
    );
    let error = host
        .command(&mut plugin, "acme.big", Args::new())
        .unwrap_err();
    assert!(error.message.contains("16 MiB of texts at most"), "{error}");
}

#[test]
fn a_number_that_is_not_finite_is_refused_in_a_view_and_a_request() {
    let entry = r#"
return {
    activate = function(cx)
        cx.register_command({ name = "acme.nan", title = "NaN", category = "view" })
    end,
    on_command = function(cx) cx.set_font_size(0/0) end,
    render = function() return { kind = "container", layout = { gap = 0/0 } } end,
}
"#;
    let mut plugin = plugin(entry);
    let mut host = FakeHost::new("acme/test");
    host.activate(&mut plugin).unwrap();
    let view = ViewId::Panel(PanelId::from("x"));
    assert_eq!(nowhere(|cx| plugin.render(cx, &view)), None);
    let failure = plugin.take_failure().unwrap();
    assert!(
        failure.message.contains("expected a finite number"),
        "{failure}"
    );
    let error = host
        .command(&mut plugin, "acme.nan", Args::new())
        .unwrap_err();
    assert!(
        error.message.contains("expected a finite number"),
        "{error}"
    );
}

#[test]
fn the_template_s_plugin_runs_as_its_types_tell() {
    use lokyr_plugin_sdk::view::Role as NodeRole;

    let mut plugin = plugin(include_str!("../luau/template/src/init.luau"));
    let mut host = FakeHost::new("acme/hello");
    host.pane(lokyr_plugin_sdk::host::PaneId(1));
    host.focus(Some(lokyr_plugin_sdk::host::PaneId(1)));
    host.activate(&mut plugin).unwrap();
    assert_eq!(host.commands()[0].name, "hello.say");
    // Its command shows its notice, which goes by itself.
    host.command(&mut plugin, "hello.say", Args::new()).unwrap();
    let notice = PanelId::from("notice");
    assert!(host.panels().contains_key(&notice));
    let tree = host
        .render(&mut plugin, &ViewId::Panel(notice.clone()))
        .unwrap();
    assert_eq!(tree.role, Some(NodeRole::Alert));
    assert_eq!(tree.children[0].kind, Kind::Text("Hello!".to_owned()));
    host.advance(std::time::Duration::from_millis(3000));
    host.deliver(&mut plugin).unwrap();
    assert!(!host.panels().contains_key(&notice));
    assert_eq!(plugin.take_failure(), None);
}

#[test]
fn a_plugin_s_errors_of_long_texts_are_told_short() {
    // The long texts made as it activates, whose deadline is longer than a
    // render's.
    let entry = r#"
local long, kind, field
return {
    activate = function(cx)
        long = string.rep("x", 4 * 1024 * 1024)
        kind = string.rep("k", 17 * 1024 * 1024)
        field = string.rep("f", 1024 * 1024)
    end,
    on_command = function(cx, name)
        error(long)
    end,
    render = function(cx, view)
        if view.panel == "kind" then
            return { kind = kind }
        elseif view.panel == "field" then
            return { kind = "text", text = "t", [field] = 1 }
        end
        return { kind = field }
    end,
}
"#;
    let mut plugin = plugin(entry);
    let mut host = FakeHost::new("acme/test");
    host.activate(&mut plugin).unwrap();
    // A message of megabytes, cut.
    let error = host
        .command(&mut plugin, "acme.go", Args::new())
        .unwrap_err();
    assert_eq!(error.code, Code::Failed);
    assert!(
        error.message.len() <= MOST_TOLD + 256,
        "{}",
        error.message.len()
    );
    assert!(error.message.ends_with('…'));
    // A node's kind past the texts' bytes, refused as such; a kind and a
    // field of no name of a node's, told of their first characters.
    for (panel, told) in [
        ("kind", "MiB of texts at most"),
        ("short", "is no kind of node"),
        ("field", "no field `fff"),
    ] {
        nowhere(|cx| plugin.render(cx, &ViewId::Panel(PanelId::from(panel))));
        let failure = plugin.take_failure().unwrap();
        assert!(
            failure.message.contains(told),
            "{panel}: {}",
            &failure.message[..200.min(failure.message.len())]
        );
        assert!(
            failure.message.len() < 1024,
            "{panel}: {}",
            failure.message.len()
        );
    }
}

#[test]
fn a_package_gives_its_catalogs_of_each_language_and_its_named_config_schema() {
    let package = Package::new([
        ("src/init.luau".to_owned(), "return {}".to_owned()),
        ("locales/en/a.ftl".to_owned(), "a = A".to_owned()),
        ("locales/en/b.ftl".to_owned(), "b = B".to_owned()),
        ("locales/it/a.ftl".to_owned(), "a = Ah".to_owned()),
        ("locales/it/deep/c.ftl".to_owned(), "c = C".to_owned()),
        ("locales/it/notes.txt".to_owned(), "x".to_owned()),
        ("snap.schema.json".to_owned(), "{}".to_owned()),
    ]);
    let catalogs: Vec<(String, String)> = package
        .catalogs()
        .into_iter()
        .map(|(language, source)| (language.into_owned(), source.into_owned()))
        .collect();
    assert_eq!(
        catalogs,
        [
            ("en".to_owned(), "a = A\nb = B".to_owned()),
            ("it".to_owned(), "a = Ah".to_owned()),
        ]
    );
    let plugin = LuauPlugin::new("acme/snap", package.clone(), Execution::Native);
    assert_eq!(plugin.config_schema(), None);
    assert_eq!(plugin.catalogs().len(), 2);
    let named = plugin.with_config_schema(Some("snap.schema.json".to_owned()));
    assert_eq!(named.config_schema().as_deref(), Some("{}"));
    let missing = LuauPlugin::new("acme/snap", package, Execution::Native)
        .with_config_schema(Some("other.json".to_owned()));
    assert_eq!(missing.config_schema(), None);
}
