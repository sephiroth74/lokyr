use super::*;

/// The template of `sleip new`'s files, by their paths.
const TEMPLATE_ENTRY: &str = include_str!("../../luau/template/src/init.luau");
const TEMPLATE_TEST: &str = include_str!("../../luau/template/tests/hello.luau");

/// The outcomes of the tests of a package of the template's entry, of a
/// manifest of nothing more than its command, and of the test files of
/// `tests`, within `deadline`.
fn outcomes(tests: &[(&str, &str)], deadline: Duration) -> Vec<Outcome> {
    outcomes_of(tests, deadline, Execution::Native)
}

/// The outcomes of [`outcomes`], the plugin's VM of `execution`.
fn outcomes_of(tests: &[(&str, &str)], deadline: Duration, execution: Execution) -> Vec<Outcome> {
    let mut sources = vec![("src/init.luau".to_owned(), TEMPLATE_ENTRY.to_owned())];
    sources.extend(
        tests
            .iter()
            .map(|(path, text)| ((*path).to_owned(), (*text).to_owned())),
    );
    let package = Package::new(sources);
    let messages = [(
        "hello-notice".to_owned(),
        "Hello from your plugin!".to_owned(),
    )];
    run_within(
        &Plugin {
            name: "acme/hello",
            capabilities: &[],
            uses: &[],
            slots: &[],
            package: &package,
            messages: &messages,
            execution,
        },
        deadline,
    )
}

#[test]
fn the_template_s_tests_pass_of_the_native_code_generator_and_of_the_interpreter() {
    for execution in [Execution::Native, Execution::Interpreter] {
        let ran = outcomes_of(&[("tests/hello.luau", TEMPLATE_TEST)], DEADLINE, execution);
        assert_eq!(ran.len(), 1, "{execution:?}: {ran:?}");
        assert_eq!(ran[0].file, "tests/hello.luau");
        assert_eq!(
            ran[0].name,
            "the_command_shows_its_notice_which_goes_by_itself"
        );
        assert_eq!(ran[0].failure, None, "{execution:?}");
    }
}

#[test]
fn a_failing_test_tells_its_line_and_why_each_test_of_a_host_of_its_own() {
    let tests = r#"--!strict
local test = {}

function test.a_assert_fails(host: TestHost)
    host.activate()
    assert(#host.commands() == 2, "two commands")
end

function test.b_call_fails(host: TestHost)
    host.activate()
    host.render({ nope = "x" } :: any)
end

function test.c_activates_anew(host: TestHost)
    assert(#host.commands() == 0, "a host of its own")
    host.activate()
    host.command("hello.say")
    assert(host.panels()[1] == "notice")
    local view = host.render({ panel = "notice" })
    assert(view and view.children and view.children[1].text == "Hello from your plugin!")
    -- Its writes, each of its request's name.
    local done = host.done()
    assert(#done > 0, "its writes")
    for _, request in done do
        assert(type(next(request)) == "string", "a request's name")
    end
    assert(#host.done() == 0, "taken once")
end

function test.d_loops(host: TestHost)
    while true do end
end

return test
"#;
    let ran = outcomes(&[("tests/many.luau", tests)], Duration::from_millis(300));
    let names: Vec<&str> = ran.iter().map(|outcome| outcome.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "a_assert_fails",
            "b_call_fails",
            "c_activates_anew",
            "d_loops"
        ]
    );
    assert_eq!(
        ran[0].failure,
        Some(Failure {
            line: Some(6),
            message: "two commands".to_owned()
        })
    );
    let call = ran[1].failure.as_ref().unwrap();
    assert_eq!(call.line, Some(11), "{call:?}");
    assert!(call.message.contains("nope"), "{call:?}");
    assert_eq!(ran[2].failure, None);
    let looped = ran[3].failure.as_ref().unwrap();
    assert!(looped.message.contains("past its deadline"), "{looped:?}");
}

#[test]
fn a_test_file_reads_the_plugin_s_modules_and_one_of_no_tests_fails_whole() {
    let util = "return { twice = function(n: number): number return n * 2 end }";
    // A module of its folders, which a test requires and is no test file.
    let tests = r#"local util = require("../src/util")
local four = require("./helpers/four")
return {
    doubles = function(host)
        assert(util.twice(2) == four)
    end,
}
"#;
    let mut sources = vec![
        ("src/init.luau".to_owned(), TEMPLATE_ENTRY.to_owned()),
        ("src/util.luau".to_owned(), util.to_owned()),
        ("tests/util.luau".to_owned(), tests.to_owned()),
        ("tests/helpers/four.luau".to_owned(), "return 4".to_owned()),
        ("tests/wrong.luau".to_owned(), "return 1".to_owned()),
        ("tests/broken.luau".to_owned(), "return {".to_owned()),
    ];
    sources.sort();
    let package = Package::new(sources);
    let ran = run(&Plugin {
        name: "acme/hello",
        capabilities: &[],
        uses: &[],
        slots: &[],
        package: &package,
        messages: &[],
        execution: Execution::Native,
    });
    let summary: Vec<(&str, &str, bool)> = ran
        .iter()
        .map(|outcome| {
            (
                outcome.file.as_str(),
                outcome.name.as_str(),
                outcome.failure.is_none(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("tests/broken.luau", "(file)", false),
            ("tests/util.luau", "doubles", true),
            ("tests/wrong.luau", "(file)", false),
        ]
    );
    let wrong = ran[2].failure.as_ref().unwrap();
    assert!(wrong.message.contains("not integer"), "{wrong:?}");
    assert_eq!(ran[0].failure.as_ref().unwrap().line, Some(1));
}

/// A plugin that keeps what it sees in its state: its command's runs, the
/// events that it hears, and the calls that its commands make.
const PROBE: &str = r#"--!strict
local Plugin: Plugin = {}
local seen: { string } = {}

local function note(cx: Cx, what: string)
    table.insert(seen, what)
    assert(cx.set_state(seen))
end

function Plugin.activate(cx: Cx)
    assert(cx.register_command({
        name = "probe.say",
        title = "Say",
        category = "view",
        params = { { name = "times", kind = "integer", default = 2 } },
    }))
    for _, name in { "probe.listen", "probe.bind", "probe.render", "probe.require" } do
        assert(cx.register_command({ name = name, title = name, category = "view" }))
    end
end

function Plugin.on_command(cx: Cx, name: string, args: Args)
    if name == "probe.say" then
        note(cx, "say " .. tostring(args.times))
    elseif name == "probe.listen" then
        assert(cx.subscribe({ "pane-focused" }))
    elseif name == "probe.bind" then
        assert(cx.bind({ keys = "cmd+shift+k", command = "pane.clear" }))
    elseif name == "probe.render" then
        assert(cx.request_render({ status_bar = 1 }))
    elseif name == "probe.require" then
        local helper: any = require("../tests/helpers/four")
        note(cx, tostring(helper))
    end
end

function Plugin.on_event(cx: Cx, events: { Event })
    for _, event in events do
        note(cx, event.kind)
    end
end

return Plugin
"#;

/// The tests of the probe, which check what each of its calls does.
const PROBE_TESTS: &str = r#"--!strict
local test = {}

local function seen(host: TestHost): string
    local state: any = host.state()
    if type(state) ~= "table" then
        return ""
    end
    return table.concat(state, ",")
end

function test.an_event_is_heard_once_subscribed_to(host: TestHost)
    host.activate()
    host.events({ { kind = "pane-focused", pane = 1, tab = 1 } })
    assert(seen(host) == "", "unheard: " .. seen(host))
    host.command("probe.listen")
    host.events({ { kind = "pane-focused", pane = 1, tab = 1 } })
    assert(seen(host) == "pane-focused", "heard: " .. seen(host))
end

function test.a_command_s_arguments_are_checked_and_its_defaults_given(host: TestHost)
    host.activate()
    host.command("probe.say")
    host.command("probe.say", { times = 3 })
    assert(seen(host) == "say 2,say 3", seen(host))
    assert(not pcall(host.command, "probe.say", { times = "x" }))
    assert(not pcall(host.command, "probe.say", { loud = true }))
    assert(not pcall(host.command, "probe.nothing"))
    assert(not host.applies("probe.say", { times = "x" }))
    assert(host.applies("probe.say", {}))
    assert(seen(host) == "say 2,say 3", seen(host))
end

function test.a_binding_of_the_app_s_command_that_it_declares(host: TestHost)
    host.activate()
    host.command("probe.bind")
end

function test.a_view_of_a_slot_that_it_fills(host: TestHost)
    host.activate()
    host.command("probe.render")
end

function test.its_sources_require_no_test_file(host: TestHost)
    host.activate()
    assert(not pcall(host.command, "probe.require"))
end

return test
"#;

/// The outcomes of the probe's tests, of `uses` and `slots`, beside a
/// helper of its tests and a test file of no test.
fn probed(uses: &[String], slots: &[Slot]) -> Vec<(String, Option<String>)> {
    let package = Package::new([
        ("src/init.luau".to_owned(), PROBE.to_owned()),
        ("tests/probe.luau".to_owned(), PROBE_TESTS.to_owned()),
        ("tests/helpers/four.luau".to_owned(), "return 4".to_owned()),
        ("tests/empty.luau".to_owned(), "return {}".to_owned()),
    ]);
    run(&Plugin {
        name: "acme/probe",
        capabilities: &[],
        uses,
        slots,
        package: &package,
        messages: &[],
        execution: Execution::Native,
    })
    .into_iter()
    .map(|outcome| (outcome.name, outcome.failure.map(|failure| failure.message)))
    .collect()
}

#[test]
fn the_plugin_is_set_up_as_the_app_sets_it_up() {
    let ran = probed(&["pane.clear".to_owned()], &[Slot::StatusBar]);
    let failed: Vec<&(String, Option<String>)> = ran
        .iter()
        .filter(|(_, failure)| failure.is_some())
        .collect();
    // A file of no test fails, alone.
    assert_eq!(failed.len(), 1, "{ran:?}");
    assert_eq!(failed[0].0, "(file)");
    assert!(
        failed[0].1.as_deref().unwrap().contains("no test"),
        "{failed:?}"
    );
    assert_eq!(ran.len(), 6, "{ran:?}");
    // Of no `[uses]` nor slot, the binding and the view are refused.
    let ran = probed(&[], &[]);
    let failure = |name: &str| {
        ran.iter()
            .find(|(test, _)| test == name)
            .and_then(|(_, failure)| failure.clone())
    };
    assert!(
        failure("a_binding_of_the_app_s_command_that_it_declares")
            .is_some_and(|message| message.contains("pane.clear")),
        "{ran:?}"
    );
    assert!(
        failure("a_view_of_a_slot_that_it_fills").is_some(),
        "{ran:?}"
    );
    assert!(
        failure("an_event_is_heard_once_subscribed_to").is_none(),
        "{ran:?}"
    );
}

#[test]
fn the_app_s_commands_read() {
    let names: Vec<&str> = registry()
        .iter()
        .map(|registered| registered.command.name.as_str())
        .collect();
    assert!(
        names.contains(&"pane.clear") && names.contains(&"window.new"),
        "{names:?}"
    );
    assert!(
        registry()
            .iter()
            .any(|registered| registered.plugin.as_deref() == Some("lokyr/core-actions"))
    );
}

#[test]
fn a_timer_s_event_posted_fails_as_the_host_gives_it_alone() {
    let ran = outcomes(
        &[(
            "tests/timer.luau",
            "return { posted = function(host) host.events({ { kind = \"timer\", id = 1 } }) end }",
        )],
        DEADLINE,
    );
    assert_eq!(ran.len(), 1, "{ran:?}");
    let failure = ran[0].failure.as_ref().expect("a failure");
    assert!(
        failure.message.contains("a timer's event is the host's"),
        "{failure:?}"
    );
}

#[test]
fn a_test_gives_the_pane_s_cells_and_reads_the_clipboard_s_image_the_saves_and_the_kept_images() {
    // A plugin that snaps the selection's cells, or the screen's, keeps the
    // image, copies its PNG and saves it.
    const ENTRY: &str = r#"
    local kept = nil
    return {
        activate = function(cx)
            assert(cx.register_command({ name = "snapper.go", title = "Go", category = "view" }))
        end,
        on_command = function(cx, name, args)
            local pane = assert(cx.focused_pane())
            local rows = cx.selection_cells(pane) or assert(cx.screen_cells(pane))
            local image = assert(cx.render_image({ rows = { rows[1] }, keep = true }))
            kept = image.id
            assert(cx.set_clipboard_image(image.png))
            assert(cx.save_file(nil, rows[1].text .. ".png", image.png))
        end,
        on_event = function(cx, events)
            for _, event in events do
                if event.kind == "saved" and event.path then
                    assert(cx.drop_image(kept))
                end
            end
        end,
    }
    "#;
    const TEST: &str = r#"
    return {
        snaps_the_screen_then_the_selection = function(host)
            host.activate()
            host.set_screen({ { text = "ls -la", runs = {} } })
            host.command("snapper.go")
            assert(host.clipboard_image():sub(2, 4) == "PNG")
            local saves = host.saves()
            assert(#saves == 1 and saves[1].name == "ls -la.png")
            assert(#host.kept_images() == 1)
            host.answer_save(saves[1].id, "/tmp/x.png")
            assert(host.saved_file("/tmp/x.png"):sub(2, 4) == "PNG")
            assert(#host.saves() == 0 and #host.kept_images() == 0)
            host.set_selection({ { text = "selected" } })
            host.command("snapper.go")
            assert(host.saves()[1].name == "selected.png")
            host.answer_save(host.saves()[1].id, nil)
            assert(host.saved_file("/tmp/y.png") == nil)
        end,
    }
    "#;
    let package = Package::new([
        ("src/init.luau".to_owned(), ENTRY.to_owned()),
        ("tests/snapper.luau".to_owned(), TEST.to_owned()),
    ]);
    let ran = run_within(
        &Plugin {
            name: "acme/snapper",
            capabilities: &[Capability::TerminalRead, Capability::ClipboardWrite],
            uses: &[],
            slots: &[],
            package: &package,
            messages: &[],
            execution: Execution::Native,
        },
        DEADLINE,
    );
    assert_eq!(ran.len(), 1, "{ran:?}");
    assert_eq!(ran[0].failure, None, "{ran:?}");
}
