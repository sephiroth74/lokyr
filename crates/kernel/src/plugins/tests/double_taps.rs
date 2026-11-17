//! The double taps of modifiers that the plugins hear: each that an active
//! plugin watches, once, and its tap to the plugins that watch it alone,
//! none of a plugin that stops or lacks the capability.

use lokyr_plugin_sdk::host::Modifier;

use super::*;

/// A plugin whose commands `<name>.ctrl`, `<name>.cmd` and `<name>.none`
/// watch Control's double tap, Command's or none, and `<name>.panic`
/// panics, and which records the modifiers of the taps that it hears in
/// `log`, as `<name> Ctrl`.
fn watcher(name: &'static str, log: &Rc<RefCell<Vec<String>>>) -> Box<dyn Plugin> {
    let log = Rc::clone(log);
    Box::new(Scripted {
        activate: Box::new(move |cx| {
            for action in ["ctrl", "cmd", "none", "panic"] {
                cx.register_command(command(&format!("{name}.{action}")))?;
            }
            Ok(())
        }),
        on_command: Box::new(|cx, run, _| match run.rsplit('.').next() {
            Some("ctrl") => cx.watch_double_tap(Some(Modifier::Ctrl)),
            Some("cmd") => cx.watch_double_tap(Some(Modifier::Cmd)),
            Some("panic") => panic!("on purpose"),
            _ => cx.watch_double_tap(None),
        }),
        on_event: Some(Box::new(move |_, events| {
            for event in events {
                if let Event::DoubleTapped { modifier } = event {
                    log.borrow_mut().push(format!("{name} {modifier:?}"));
                }
            }
            Ok(())
        })),
    })
}

#[test]
fn a_double_tap_is_watched_as_an_active_plugin_asks_and_heard_by_its_watchers_alone() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    let log = Rc::new(RefCell::new(Vec::new()));
    plugins.add("one", watcher("one", &log)).unwrap();
    plugins.add("two", watcher("two", &log)).unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert!(plugins.double_taps().is_empty());
    let run = |plugins: &mut Plugins, app: &mut App, name: &str| {
        plugins.run(name, Args::new(), &mut app.context()).unwrap();
    };
    run(&mut plugins, &mut app, "one.ctrl");
    run(&mut plugins, &mut app, "two.ctrl");
    assert_eq!(plugins.double_taps(), [Modifier::Ctrl]);
    // Each modifier once, in their order.
    run(&mut plugins, &mut app, "two.cmd");
    assert_eq!(plugins.double_taps(), [Modifier::Ctrl, Modifier::Cmd]);
    // A tap to its watchers alone, at the next delivery.
    plugins.double_tapped(Modifier::Ctrl);
    plugins.double_tapped(Modifier::Shift);
    assert!(log.borrow().is_empty());
    assert!(plugins.deliver(&mut app.context()).is_empty());
    assert_eq!(*log.borrow(), ["one Ctrl"]);
    plugins.double_tapped(Modifier::Cmd);
    assert!(plugins.deliver(&mut app.context()).is_empty());
    assert_eq!(*log.borrow(), ["one Ctrl", "two Cmd"]);
    // None watches nothing; a plugin that a fault stops, nor one taken
    // away, watches nothing.
    run(&mut plugins, &mut app, "two.ctrl");
    assert!(
        plugins
            .run("two.panic", Args::new(), &mut app.context())
            .is_err()
    );
    assert_eq!(plugins.status("two"), Some(Status::Disabled));
    assert_eq!(plugins.double_taps(), [Modifier::Ctrl]);
    plugins.double_tapped(Modifier::Ctrl);
    assert!(plugins.deliver(&mut app.context()).is_empty());
    assert_eq!(*log.borrow(), ["one Ctrl", "two Cmd", "one Ctrl"]);
    assert!(plugins.remove("one", &mut app.context()));
    assert!(plugins.double_taps().is_empty());
    plugins.double_tapped(Modifier::Ctrl);
    assert!(plugins.deliver(&mut app.context()).is_empty());
    assert_eq!(log.borrow().len(), 3);
}

#[test]
fn a_plugin_without_the_capability_watches_no_double_tap() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    let log = Rc::new(RefCell::new(Vec::new()));
    let manifest = Manifest::parse(
        "[plugin]\nname = \"lokyr/untapped\"\nkind = \"builtin\"\nlicense = \"MIT\"\n",
    )
    .unwrap();
    plugins
        .add_declared(manifest, watcher("untapped", &log))
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    let refused = plugins
        .run("untapped.ctrl", Args::new(), &mut app.context())
        .unwrap_err();
    assert_eq!(refused.code, Code::MissingCapability);
    assert!(plugins.double_taps().is_empty());
}
