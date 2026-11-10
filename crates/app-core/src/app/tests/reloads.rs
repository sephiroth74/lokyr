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
    // A plugin made anew would activate on its thread, of a VM of its own,
    // after the reload: its activation waited for first.
    run_until(&mut fake, &mut app, |_, app| {
        app.plugins.activated("acme/grown")
    });
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
    changed(&mut fake, &mut app);
    settled(&mut fake, &mut app);
    run_until(&mut fake, &mut app, |fake, _| opens(fake, "dev2") == 1);
    // Saved twice within a burst: once made anew, as it settles.
    std::fs::write(&entry, opening("activate", "dev3", "")).unwrap();
    changed(&mut fake, &mut app);
    std::fs::write(&entry, opening("activate", "dev4", "")).unwrap();
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

#[test]
fn a_plugin_installed_after_a_start_of_no_config_folder_loads_without_a_restart() {
    let _luau = LUAU.lock().unwrap_or_else(PoisonError::into_inner);
    let folder = shelf("reloads-fresh");
    // A new account's: neither the config's folder nor its parent.
    let config = folder.join("home/.config/lokyr");
    let store = Store::new(folder.join("store"));
    let mut fake = Fake::default();
    let links = Config::parse("[links]\nurls = \"open\"\n", Path::new("config.toml")).unwrap();
    let mut app = app(links, quiet())
        .with_installed(
            config.join("plugins.lock"),
            Store::new(folder.join("store")),
        )
        .with_state(Some(folder.join("state")));
    fake.start(&mut app);
    assert!(config.is_dir());
    // Sleip installs a plugin as the app runs: its watches tell it.
    grant_open(&config, &["fresh"]);
    let fresh = manifest("fresh", &[], "^0.1", "");
    lock(
        &config,
        BTreeMap::from([stored(
            &store,
            "fresh",
            &fresh,
            &opening("activate", "fresh", ""),
        )]),
    );
    run_until(&mut fake, &mut app, |fake, _| opens(fake, "fresh") == 1);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_change_of_no_file_of_a_dev_folder_s_package_makes_no_plugin_anew() {
    let _luau = LUAU.lock().unwrap_or_else(PoisonError::into_inner);
    let folder = shelf("reloads-swap");
    grant_open(&folder, &["swapped"]);
    let swapped = folder.join("swapped");
    std::fs::create_dir_all(swapped.join("src")).unwrap();
    std::fs::write(
        swapped.join("plugin.toml"),
        manifest("swapped", &[], "^0.1", ""),
    )
    .unwrap();
    let entry = swapped.join("src/init.luau");
    std::fs::write(&entry, opening("activate", "swap1", "")).unwrap();
    lock(
        &folder,
        BTreeMap::from([(
            "acme/swapped".to_owned(),
            Locked {
                version: "1.0.0".to_owned(),
                kind: Kind::Luau,
                source: Source::Dev(swapped.display().to_string()),
                checksum: None,
                capabilities: Vec::new(),
                dependencies: BTreeMap::new(),
            },
        )]),
    );
    let mut fake = Fake::default();
    let mut app = reloading(&folder);
    fake.start(&mut app);
    run_until(&mut fake, &mut app, |fake, _| opens(fake, "swap1") == 1);
    let before = lokyr_plugin_host_luau::vms_made();
    // An editor's swap file, the Finder's, a repository's and a test's.
    std::fs::write(swapped.join("src/.init.luau.swp"), "swap").unwrap();
    std::fs::write(swapped.join(".DS_Store"), "finder").unwrap();
    std::fs::create_dir_all(swapped.join("tests")).unwrap();
    std::fs::write(swapped.join("tests/more.luau"), "return {}").unwrap();
    changed(&mut fake, &mut app);
    settled(&mut fake, &mut app);
    // A plugin made anew would activate on its thread, of a VM of its own,
    // after the reload: its activation waited for first.
    run_until(&mut fake, &mut app, |_, app| {
        app.plugins.activated("acme/swapped")
    });
    assert_eq!(lokyr_plugin_host_luau::vms_made(), before);
    // Its source saved: made anew.
    std::fs::write(&entry, opening("activate", "swap2", "")).unwrap();
    changed(&mut fake, &mut app);
    settled(&mut fake, &mut app);
    run_until(&mut fake, &mut app, |fake, _| opens(fake, "swap2") == 1);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_permissions_file_linked_elsewhere_after_the_start_is_watched_there() {
    let _luau = LUAU.lock().unwrap_or_else(PoisonError::into_inner);
    let folder = shelf("reloads-stowed");
    let store = Store::new(folder.join("store"));
    grant_open(&folder, &["stowed"]);
    let stowed = manifest("stowed", &[], "^0.1", "");
    lock(
        &folder,
        BTreeMap::from([stored(
            &store,
            "stowed",
            &stowed,
            &opening("activate", "stowed", ""),
        )]),
    );
    let mut fake = Fake::default();
    let mut app = reloading(&folder);
    fake.start(&mut app);
    run_until(&mut fake, &mut app, |fake, _| opens(fake, "stowed") == 1);
    // GNU Stow links the file to a folder of dotfiles, of the same
    // decisions.
    let dotfiles = folder.join("dotfiles");
    std::fs::create_dir_all(&dotfiles).unwrap();
    let file = lokyr_pkg_model::permissions::FILE;
    std::fs::rename(folder.join(file), dotfiles.join(file)).unwrap();
    std::os::unix::fs::symlink(dotfiles.join(file), folder.join(file)).unwrap();
    // Its watch reads it, and watches the folder of dotfiles from then.
    std::thread::sleep(Duration::from_millis(300));
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(500) {
        fake.deliver_wake(&mut app);
        fake.fire_timers(&mut app, Instant::now() + Duration::from_secs(1));
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        app.plugins.status("acme/stowed"),
        Some(plugins::Status::Active)
    );
    // The user revokes `open` in the dotfiles' copy: it stops.
    let mut permissions =
        lokyr_pkg_model::permissions::Permissions::read(&dotfiles.join(file)).unwrap();
    permissions
        .plugins
        .get_mut("acme/stowed")
        .unwrap()
        .decide(&[lokyr_kernel::commands::Capability::Open], false);
    std::fs::write(dotfiles.join(file), permissions.text()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.plugins.status("acme/stowed") != Some(plugins::Status::Disabled) {
        assert!(Instant::now() < deadline, "the revocation never came");
        fake.deliver_wake(&mut app);
        fake.fire_timers(&mut app, Instant::now() + Duration::from_secs(1));
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_plugin_saved_under_sleip_dev_runs_its_new_command_without_a_restart() {
    let _luau = LUAU.lock().unwrap_or_else(PoisonError::into_inner);
    let folder = shelf("reloads-dev-command");
    grant_open(&folder, &["grown"]);
    let grown = folder.join("grown");
    std::fs::create_dir_all(grown.join("src")).unwrap();
    std::fs::write(
        grown.join("plugin.toml"),
        manifest("grown", &[], "^0.1", ""),
    )
    .unwrap();
    let entry = grown.join("src/init.luau");
    std::fs::write(&entry, opening("activate", "first", "")).unwrap();
    // The entry that `sleip dev` writes: its folder, of no checksum.
    lock(
        &folder,
        BTreeMap::from([(
            "acme/grown".to_owned(),
            Locked {
                version: "1.0.0".to_owned(),
                kind: Kind::Luau,
                source: Source::Dev(grown.display().to_string()),
                checksum: None,
                capabilities: vec![lokyr_kernel::commands::Capability::Open],
                dependencies: BTreeMap::new(),
            },
        )]),
    );
    let mut fake = Fake::default();
    let mut app = reloading(&folder);
    fake.start(&mut app);
    run_until(&mut fake, &mut app, |fake, _| opens(fake, "first") == 1);
    assert!(app.commands.get("grown.new").is_none());
    // Saved of a new command, which the app runs once it reloads.
    let register =
        r#"assert(cx.register_command({ name = "grown.new", title = "New", category = "view" }))"#;
    std::fs::write(&entry, opening("on_command", "new", register)).unwrap();
    changed(&mut fake, &mut app);
    settled(&mut fake, &mut app);
    run_until(&mut fake, &mut app, |_, app| {
        app.commands.get("grown.new").is_some()
    });
    fake.send_app(&mut app, AppEvent::Command("grown.new".to_owned()));
    run_until(&mut fake, &mut app, |fake, _| opens(fake, "new") == 1);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_lockfile_that_does_not_read_is_told_once_however_many_reloads_read_it() {
    let folder = shelf("reloads-unread");
    lock(&folder, BTreeMap::new());
    let mut fake = Fake::default();
    let mut app = reloading(&folder);
    fake.start(&mut app);
    std::fs::write(folder.join("plugins.lock"), "api = [\n").unwrap();
    for _ in 0..10 {
        changed(&mut fake, &mut app);
        settled(&mut fake, &mut app);
    }
    let told = super::installed::notices(&mut app);
    assert_eq!(
        told.iter()
            .filter(|text| text.contains("plugins.lock"))
            .count(),
        1,
        "{told:?}"
    );
    // Read again, its notice goes.
    changed(&mut fake, &mut app);
    settled(&mut fake, &mut app);
    assert!(app.diagnostics.tells(&About::InstalledPlugins));
    lock(&folder, BTreeMap::new());
    changed(&mut fake, &mut app);
    settled(&mut fake, &mut app);
    assert!(!app.diagnostics.tells(&About::InstalledPlugins));
    let _ = std::fs::remove_dir_all(&folder);
}
