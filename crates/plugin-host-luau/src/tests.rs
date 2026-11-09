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
    assert(cx.set_state({ count = #events, first = events[1], value = events[2].custom.value }))
end

function Plugin.render(cx, view)
    return { key = "label", kind = { text = "hi " .. view.panel } }
end

function Plugin.on_ui_event(cx, event)
    assert(cx.set_state({ key = event.key, view = event.view.panel, kind = event.kind.kind }))
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
            return { kind = { text = 3 } }
        end
        return { kind = { text = "fine" } }
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
