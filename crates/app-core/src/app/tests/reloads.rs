//! The tests of the installed plugins reloaded at once: one installed,
//! updated and removed, a folder that `sleip dev` links saved, each
//! reloaded without a restart; a burst of changes one reload; and no wake
//! while nothing changes.

use std::collections::BTreeMap;
use std::path::Path;

use lokyr_pkg_model::lock::{Lock, Locked, Source};
use lokyr_pkg_model::manifest::Kind;
use lokyr_pkg_model::store::Store;

use super::installed::{grant_open, manifest, opening, shelf, stored};
use super::*;

/// The app of the installed plugins of `folder`, which opens URLs without
/// a question.
fn reloading(folder: &Path) -> App {
    let config = Config::parse("[links]\nurls = \"open\"\n", Path::new("config.toml")).unwrap();
    app(config, quiet())
        .with_installed(
            folder.join("plugins.lock"),
            Store::new(folder.join("store")),
        )
        .with_state(Some(folder.join("state")))
}

/// Writes the lockfile of `packages` in `folder`.
fn lock(folder: &Path, packages: BTreeMap<String, Locked>) {
    Lock {
        api: lokyr_kernel::plugins::API_VERSION.to_owned(),
        packages,
    }
    .write(&folder.join("plugins.lock"))
    .unwrap();
}

/// A change of the installed plugins, as a watch tells it, and the reload
/// once it settles.
fn changed(fake: &mut Fake, app: &mut App) {
    app.reloads.changed.store(true, Ordering::Release);
    fake.send_app(app, AppEvent::Wake);
}

/// The reload, its burst settled.
fn settled(fake: &mut Fake, app: &mut App) {
    fake.fire_timers(app, Instant::now() + Duration::from_secs(1));
}

/// How many times `fake` opened `https://lokyr.dev/<what>`.
fn opens(fake: &Fake, what: &str) -> usize {
    let target = format!("https://lokyr.dev/{what}");
    fake.opened()
        .iter()
        .filter(|opened| **opened == target)
        .count()
}

#[test]
fn a_plugin_installed_updated_and_removed_is_reloaded_without_a_restart() {
    let _luau = LUAU.lock().unwrap_or_else(PoisonError::into_inner);
    let folder = shelf("reloads");
    let store = Store::new(folder.join("store"));
    grant_open(&folder, &["grown"]);
    let mut fake = Fake::default();
    let mut app = reloading(&folder);
    fake.start(&mut app);
    assert_eq!(app.plugins.status("acme/grown"), None);
    // Installed: it comes, and activates at its trigger, the start's.
    let first = manifest("grown", &[], "^0.1", "");
    lock(
        &folder,
        BTreeMap::from([stored(
            &store,
            "grown",
            &first,
            &opening("activate", "v1", ""),
        )]),
    );
    changed(&mut fake, &mut app);
    settled(&mut fake, &mut app);
    run_until(&mut fake, &mut app, |fake, _| opens(fake, "v1") == 1);
    assert_eq!(
        app.plugins.status("acme/grown"),
        Some(plugins::Status::Active)
    );
    // Updated: made anew, of a new VM, the old one gone.
    let before = lokyr_plugin_host_luau::vms_made();
    let second = first.replace("version = \"1.0.0\"", "version = \"1.0.1\"");
    let (name, mut locked) = stored(&store, "grown", &second, &opening("activate", "v2", ""));
    locked.version = "1.0.1".to_owned();
    lock(&folder, BTreeMap::from([(name, locked)]));
    changed(&mut fake, &mut app);
    settled(&mut fake, &mut app);
    run_until(&mut fake, &mut app, |fake, _| opens(fake, "v2") == 1);
    assert_eq!(lokyr_plugin_host_luau::vms_made(), before + 1);
    assert_eq!(
        app.plugins
            .manifest("acme/grown")
            .and_then(|manifest| manifest.version.clone()),
        Some("1.0.1".to_owned())
    );
    assert_eq!(opens(&fake, "v1"), 1);
    // A change of nothing of it reloads nothing, and a plugin refused is
    // told once while it stays so.
    let old = manifest("old", &[], "^9.0", "");
    let mut packages = BTreeMap::from([stored(
        &store,
        "grown",
        &second,
        &opening("activate", "v2", ""),
    )]);
    packages.get_mut("acme/grown").unwrap().version = "1.0.1".to_owned();
    packages.extend([stored(&store, "old", &old, &opening("activate", "old", ""))]);
    lock(&folder, packages);
    let _ = super::installed::notices(&mut app);
    changed(&mut fake, &mut app);
    settled(&mut fake, &mut app);
    let told = super::installed::notices(&mut app);
    assert_eq!(
        told.iter().filter(|text| text.contains("acme/old")).count(),
        1,
        "{told:?}"
    );
    changed(&mut fake, &mut app);
    settled(&mut fake, &mut app);
    assert!(super::installed::notices(&mut app).is_empty());
    assert_eq!(lokyr_plugin_host_luau::vms_made(), before + 1);
    // Removed: it goes.
    lock(&folder, BTreeMap::new());
    changed(&mut fake, &mut app);
    settled(&mut fake, &mut app);
    assert_eq!(app.plugins.status("acme/grown"), None);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_folder_that_sleip_dev_links_is_reloaded_as_it_is_saved_once_a_burst_settles() {
    let _luau = LUAU.lock().unwrap_or_else(PoisonError::into_inner);
    let folder = shelf("reloads-dev");
    grant_open(&folder, &["devved"]);
    let devved = folder.join("devved");
    std::fs::create_dir_all(devved.join("src")).unwrap();
    std::fs::write(
        devved.join("plugin.toml"),
        manifest("devved", &[], "^0.1", ""),
    )
    .unwrap();
    let entry = devved.join("src/init.luau");
    std::fs::write(&entry, opening("activate", "dev1", "")).unwrap();
    lock(
        &folder,
        BTreeMap::from([(
            "acme/devved".to_owned(),
            Locked {
                version: "1.0.0".to_owned(),
                kind: Kind::Luau,
                source: Source::Dev(devved.display().to_string()),
                checksum: None,
                capabilities: Vec::new(),
                dependencies: BTreeMap::new(),
            },
        )]),
    );
    let mut fake = Fake::default();
    let mut app = reloading(&folder);
    fake.start(&mut app);
    run_until(&mut fake, &mut app, |fake, _| opens(fake, "dev1") == 1);
    assert_eq!(app.reloads.dev.len(), 1);
    // Saved, as its watch tells it: made anew.
    std::fs::write(&entry, opening("activate", "dev2", "")).unwrap();
    app.reloads.dev[0].changed.store(true, Ordering::Release);
    changed(&mut fake, &mut app);
    settled(&mut fake, &mut app);
    run_until(&mut fake, &mut app, |fake, _| opens(fake, "dev2") == 1);
    // Saved twice within a burst: once made anew, as it settles.
    std::fs::write(&entry, opening("activate", "dev3", "")).unwrap();
    app.reloads.dev[0].changed.store(true, Ordering::Release);
    changed(&mut fake, &mut app);
    std::fs::write(&entry, opening("activate", "dev4", "")).unwrap();
    app.reloads.dev[0].changed.store(true, Ordering::Release);
    changed(&mut fake, &mut app);
    assert!(app.reloads.timer.is_some());
    settled(&mut fake, &mut app);
    run_until(&mut fake, &mut app, |fake, _| opens(fake, "dev4") == 1);
    assert_eq!(opens(&fake, "dev3"), 0);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn the_watches_of_the_installed_plugins_wake_nothing_while_nothing_changes() {
    let _luau = LUAU.lock().unwrap_or_else(PoisonError::into_inner);
    let folder = shelf("reloads-quiet");
    let store = Store::new(folder.join("store"));
    let quiet_plugin = manifest("still", &["command:still.go"], "^0.1", "");
    lock(
        &folder,
        BTreeMap::from([stored(
            &store,
            "still",
            &quiet_plugin,
            &opening("on_command", "still", ""),
        )]),
    );
    // What the setup wrote, which a watch might tell late, goes first.
    std::thread::sleep(Duration::from_millis(500));
    let mut fake = Fake::default();
    let mut app = reloading(&folder);
    fake.start(&mut app);
    let settle = Instant::now() + Duration::from_millis(1500);
    while Instant::now() < settle {
        fake.deliver_wake(&mut app);
        settled(&mut fake, &mut app);
        std::thread::sleep(Duration::from_millis(50));
    }
    // Watched, the lockfile and the store, and nothing changes.
    std::thread::sleep(Duration::from_millis(700));
    assert!(!fake.deliver_wake(&mut app), "a watch woke the app");
    assert!(app.reloads.timer.is_none());
    assert_eq!(
        app.plugins.status("acme/still"),
        Some(plugins::Status::Inactive)
    );
    // A plugin installed, as Sleip writes its package and the lockfile:
    // the watches alone tell it.
    let more = manifest("more", &["command:more.go"], "^0.1", "");
    let mut packages = BTreeMap::from([stored(
        &store,
        "still",
        &quiet_plugin,
        &opening("on_command", "still", ""),
    )]);
    packages.extend([stored(
        &store,
        "more",
        &more,
        &opening("on_command", "more", ""),
    )]);
    lock(&folder, packages);
    run_until(&mut fake, &mut app, |_, app| {
        app.plugins.status("acme/more").is_some()
    });
    // A package of the store changed, as by hand: the store's watch tells
    // it, and the plugin goes, refused.
    let digest = app.reloads.loaded["acme/still"].checksum.unwrap();
    std::fs::write(store.folder(&digest).join("src/init.luau"), "return {}").unwrap();
    run_until(&mut fake, &mut app, |_, app| {
        app.plugins.status("acme/still").is_none()
    });
    let told = super::installed::notices(&mut app);
    assert!(told.iter().any(|text| text.contains("acme/still") && text.contains("changed since it was stored")), "{told:?}");
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn an_installed_plugin_turned_off_and_on_again_is_made_anew_of_its_package() {
    let _luau = LUAU.lock().unwrap_or_else(PoisonError::into_inner);
    let folder = shelf("reloads-enabled");
    let store = Store::new(folder.join("store"));
    grant_open(&folder, &["toggled"]);
    let toggled = manifest("toggled", &[], "^0.1", "");
    lock(
        &folder,
        BTreeMap::from([stored(
            &store,
            "toggled",
            &toggled,
            &opening("activate", "on", ""),
        )]),
    );
    let path = folder.join("config.toml");
    let links = "[links]\nurls = \"open\"\n";
    std::fs::write(&path, links).unwrap();
    let mut fake = Fake::default();
    let mut app = App::new(None, Config::read(&path).unwrap(), Some(path.clone()))
        .with_program(quiet())
        .with_installed(
            folder.join("plugins.lock"),
            Store::new(folder.join("store")),
        )
        .with_state(Some(folder.join("state")));
    fake.start(&mut app);
    run_until(&mut fake, &mut app, |fake, _| opens(fake, "on") == 1);
    std::fs::write(
        &path,
        format!("{links}[plugins.toggled]\nenabled = false\n"),
    )
    .unwrap();
    super::configs::reload(&mut fake, &mut app);
    assert_eq!(
        app.plugins.status("acme/toggled"),
        Some(plugins::Status::Off)
    );
    // On again: a new VM of its package, which activates anew.
    std::fs::write(&path, links).unwrap();
    super::configs::reload(&mut fake, &mut app);
    run_until(&mut fake, &mut app, |fake, _| opens(fake, "on") == 2);
    assert_eq!(
        app.plugins.status("acme/toggled"),
        Some(plugins::Status::Active)
    );
    let _ = std::fs::remove_dir_all(&folder);
}
