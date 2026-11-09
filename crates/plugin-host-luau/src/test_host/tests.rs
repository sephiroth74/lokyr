use super::*;

/// The template of `sleip new`'s files, by their paths.
const TEMPLATE_ENTRY: &str = include_str!("../../luau/template/src/init.luau");
const TEMPLATE_TEST: &str = include_str!("../../luau/template/tests/hello.luau");
const TEMPLATE_MANIFEST: &str = include_str!("../../luau/template/plugin.toml");

/// The outcomes of the tests of a package of the template's entry and
/// manifest and of the test files of `tests`, within `deadline`.
fn outcomes(tests: &[(&str, &str)], deadline: Duration) -> Vec<Outcome> {
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
            manifest: TEMPLATE_MANIFEST,
            package: &package,
            messages: &messages,
        },
        deadline,
    )
}

#[test]
fn the_template_s_tests_pass() {
    let ran = outcomes(&[("tests/hello.luau", TEMPLATE_TEST)], DEADLINE);
    assert_eq!(ran.len(), 1, "{ran:?}");
    assert_eq!(ran[0].file, "tests/hello.luau");
    assert_eq!(
        ran[0].name,
        "the_command_shows_its_notice_which_goes_by_itself"
    );
    assert_eq!(ran[0].failure, None);
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
        manifest: TEMPLATE_MANIFEST,
        package: &package,
        messages: &[],
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
