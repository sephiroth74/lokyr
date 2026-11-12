//! The Mac kept awake as the plugins ask: the strongest that an active one
//! asks, of their names, and none of a plugin that fails, lets go or is
//! taken away.

use lokyr_plugin_sdk::host::Awake;

use super::*;

/// A plugin whose command `<name>.display` keeps the display awake,
/// `<name>.system` the system, `<name>.off` lets it sleep, and
/// `<name>.fails` keeps the display awake and fails.
fn keeper(name: &'static str) -> Box<dyn Plugin> {
    scripted(
        move |cx| {
            for action in ["display", "system", "off", "fails"] {
                cx.register_command(command(&format!("{name}.{action}")))?;
            }
            Ok(())
        },
        move |cx, run, _| match run.rsplit('.').next() {
            Some("display") => cx.keep_awake(Awake::Display),
            Some("system") => cx.keep_awake(Awake::System),
            Some("off") => cx.keep_awake(Awake::Off),
            _ => {
                cx.keep_awake(Awake::Display)?;
                Err(Error::new(Code::Failed, "on purpose"))
            }
        },
    )
}

#[test]
fn the_strongest_that_an_active_plugin_asks_is_kept_and_let_go_with_its_plugin() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    plugins.add("one", keeper("one")).unwrap();
    plugins.add("two", keeper("two")).unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(plugins.keep_awake(), (Awake::Off, Vec::new()));
    let run = |plugins: &mut Plugins, app: &mut App, name: &str| {
        plugins.run(name, Args::new(), &mut app.context())
    };
    run(&mut plugins, &mut app, "one.system").unwrap();
    assert_eq!(plugins.keep_awake(), (Awake::System, vec!["one"]));
    // The stronger of the two, of both their names.
    run(&mut plugins, &mut app, "two.display").unwrap();
    assert_eq!(plugins.keep_awake(), (Awake::Display, vec!["one", "two"]));
    run(&mut plugins, &mut app, "two.off").unwrap();
    assert_eq!(plugins.keep_awake(), (Awake::System, vec!["one"]));
    // A call that fails keeps nothing.
    assert!(run(&mut plugins, &mut app, "two.fails").is_err());
    assert_eq!(plugins.keep_awake(), (Awake::System, vec!["one"]));
    // A plugin taken away lets go.
    assert!(plugins.remove("one", &mut app.context()));
    assert_eq!(plugins.keep_awake(), (Awake::Off, Vec::new()));
}

#[test]
fn a_plugin_without_the_capability_keeps_nothing_awake() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    let manifest = Manifest::parse(
        "[plugin]\nname = \"lokyr/sleepy\"\nkind = \"builtin\"\nlicense = \"MIT\"\n",
    )
    .unwrap();
    plugins.add_declared(manifest, keeper("sleepy")).unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    let refused = plugins
        .run("sleepy.display", Args::new(), &mut app.context())
        .unwrap_err();
    assert_eq!(refused.code, Code::MissingCapability);
    assert_eq!(plugins.keep_awake(), (Awake::Off, Vec::new()));
}
