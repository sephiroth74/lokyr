use super::*;

/// The plugin API's definitions.
const DEFINITIONS: &str = include_str!("../../../crates/plugin-host-luau/luau/lokyr.d.luau");

#[test]
fn the_template_s_plugin_is_of_no_problem() {
    let template = include_str!("../../../crates/plugin-host-luau/luau/template/src/init.luau");
    let problems = check(
        &[("src/init.luau".to_owned(), template.to_owned())],
        DEFINITIONS,
    );
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn a_plugin_s_wrong_fields_are_told_of_their_lines() {
    let wrong = include_str!("../../../crates/plugin-host-luau/luau/checks/wrong.luau");
    let problems = check(
        &[("src/init.luau".to_owned(), wrong.to_owned())],
        DEFINITIONS,
    );
    assert_eq!(problems.len(), 4, "{problems:#?}");
    assert!(
        problems
            .iter()
            .all(|problem| problem.file == "src/init.luau" && problem.kind == Kind::Type)
    );
}

#[test]
fn a_module_required_within_the_package_is_checked_of_its_types() {
    let entry = r#"
--!strict
local view = require("./ui/view")
local Plugin: Plugin = {}
function Plugin.render(cx: Cx, id: ViewId): Node?
    return view.tree(cx)
end
return Plugin
"#;
    let good = r#"
--!strict
local view = {}
function view.tree(cx: Cx): Node
    return { kind = "text", text = "hi" }
end
return view
"#;
    let sources = |view: &str| {
        vec![
            ("src/init.luau".to_owned(), entry.to_owned()),
            ("src/ui/view.luau".to_owned(), view.to_owned()),
        ]
    };
    assert_eq!(check(&sources(good), DEFINITIONS), []);
    // A module that returns a number where a node goes, told in its file.
    let wrong = good.replace(r#"return { kind = "text", text = "hi" }"#, "return 3");
    let problems = check(&sources(&wrong), DEFINITIONS);
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert_eq!(problems[0].file, "src/ui/view.luau");
    assert_eq!(problems[0].line, 5);
    // A module out of the package, or not there, is unknown.
    for path in ["../../etc/x", "./nowhere"] {
        let missing = entry.replace("./ui/view", path);
        let problems = check(
            &sources(good)[..1]
                .iter()
                .cloned()
                .map(|(name, _)| (name, missing.clone()))
                .collect::<Vec<_>>(),
            DEFINITIONS,
        );
        assert!(!problems.is_empty(), "{path}");
        assert_eq!(problems[0].line, 3, "{path}: {problems:#?}");
    }
}

#[test]
fn a_source_that_does_not_parse_is_told_of_its_line_and_column() {
    let broken = "--!strict\nlocal Plugin = {}\nfunction Plugin.activate(cx: Cx)\n    if true then\nend\nreturn Plugin\n";
    let problems = check(
        &[("src/init.luau".to_owned(), broken.to_owned())],
        DEFINITIONS,
    );
    assert!(!problems.is_empty());
    assert_eq!(problems[0].kind, Kind::Syntax);
    assert_eq!(problems[0].file, "src/init.luau");
    let zero = check(
        &[("src/init.luau".to_owned(), "return\0{}".to_owned())],
        DEFINITIONS,
    );
    assert_eq!(zero[0].message, "the file holds a zero byte");
}
