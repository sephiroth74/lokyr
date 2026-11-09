//! The tests of the plugins that Sleip installed, as the app starts: those
//! of the lockfile, of their packages in the store, each activated at its
//! trigger alone, a package changed since it was stored, a plugin of
//! another plugin API, and one of the folder that `sleip dev` links; and
//! the app of an empty store, or of plugins that wait, of no VM.

use std::collections::BTreeMap;
use std::path::Path;

use lokyr_kernel::plugins::{Event, WorkspaceEvent};
use lokyr_pkg_model::lock::{Lock, Locked, Source};
use lokyr_pkg_model::manifest::Kind;
use lokyr_pkg_model::store::{Store, digest_of};

use super::*;

/// The folder of a test's lockfile and store, made anew.
pub(super) fn shelf(name: &str) -> PathBuf {
    let folder =
        std::env::temp_dir().join(format!("lokyr-installed-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

/// The manifest of the plugin `acme/<name>`, of the activation `on`, the
/// API `api` and what follows, `more`.
pub(super) fn manifest(name: &str, on: &[&str], api: &str, more: &str) -> String {
    let on: Vec<String> = on.iter().map(|on| format!("\"{on}\"")).collect();
    format!(
        "[plugin]\nname = \"acme/{name}\"\nversion = \"1.0.0\"\nkind = \"luau\"\napi = \"{api}\"\nlicense = \"MIT\"\n[activation]\non = [{}]\n[capabilities]\nopen = true\n{more}",
        on.join(", ")
    )
}

/// Writes the package of `manifest` and `entry` into `folder`.
fn package(folder: &Path, manifest: &str, entry: &str) {
    std::fs::create_dir_all(folder.join("src")).unwrap();
    std::fs::write(folder.join("plugin.toml"), manifest).unwrap();
    std::fs::write(folder.join("src/init.luau"), entry).unwrap();
}

/// Stores the package of `manifest` and `entry` in `store`, as Sleip does:
/// the lockfile's entry of `acme/<name>`.
pub(super) fn stored(store: &Store, name: &str, manifest: &str, entry: &str) -> (String, Locked) {
    let unpacked = std::env::temp_dir().join(format!(
        "lokyr-installed-{}-unpacked-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&unpacked);
    package(&unpacked, manifest, entry);
    let digest = digest_of(&unpacked).unwrap();
    std::fs::create_dir_all(store.root()).unwrap();
    let _ = std::fs::remove_dir_all(store.folder(&digest));
    std::fs::rename(&unpacked, store.folder(&digest)).unwrap();
    let locked = Locked {
        version: "1.0.0".to_owned(),
        kind: Kind::Luau,
        source: Source::Registry("https://pkg.lokyr.dev".to_owned()),
        checksum: Some(digest),
        capabilities: vec![lokyr_kernel::commands::Capability::Open],
        dependencies: BTreeMap::new(),
    };
    (format!("acme/{name}"), locked)
}

/// The entry of a plugin that opens `https://lokyr.dev/<what>` as its
/// callback `callback` runs, of `before` first in its activation.
pub(super) fn opening(callback: &str, what: &str, before: &str) -> String {
    let open = format!(r#"assert(cx.open("https://lokyr.dev/{what}"))"#);
    if callback == "activate" {
        format!("return {{ activate = function(cx) {before} {open} end }}")
    } else {
        format!(
            "return {{ activate = function(cx) {before} end, {callback} = function(cx, ...) {open} end }}"
        )
    }
}

/// Grants `open` to the plugins `acme/<name>` of `names`, as the user
/// answered, in the decisions beside the lockfile in `folder`.
pub(super) fn grant_open(folder: &Path, names: &[&str]) {
    let mut permissions = lokyr_pkg_model::permissions::Permissions::default();
    for name in names {
        permissions
            .plugins
            .entry(format!("acme/{name}"))
            .or_default()
            .decide(&[lokyr_kernel::commands::Capability::Open], true);
    }
    permissions
        .write(&folder.join(lokyr_pkg_model::permissions::FILE))
        .unwrap();
}

/// Every notice that `app` shows, the latest first, which it dismisses.
pub(super) fn notices(app: &mut App) -> Vec<String> {
    let mut texts = Vec::new();
    while let Some(shown) = app.diagnostics.shown() {
        texts.push(shown.text.clone());
        app.diagnostics.dismiss();
    }
    texts
}

/// Whether `fake` opened `https://lokyr.dev/<what>`.
pub(super) fn opened(fake: &Fake, what: &str) -> bool {
    fake.opened()
        .iter()
        .any(|target| target == &format!("https://lokyr.dev/{what}"))
}

#[test]
fn installed_plugins_load_from_the_store_and_activate_each_at_its_trigger_alone() {
    let _luau = LUAU.lock().unwrap_or_else(PoisonError::into_inner);
    let folder = shelf("triggers");
    let store = Store::new(folder.join("store"));
    let mut packages = BTreeMap::new();
    let early = manifest("early", &[], "^0.1", "");
    packages.extend([stored(
        &store,
        "early",
        &early,
        &opening("activate", "early", ""),
    )]);
    let late = manifest(
        "late",
        &["command:late.*"],
        "^0.1",
        "[contributes]\ncommands = [{ name = \"late.go\", title = \"Go Late\" }]\n",
    );
    let register = r#"assert(cx.register_command({ name = "late.go", title = "Go Late", category = "view" }))"#;
    packages.extend([stored(
        &store,
        "late",
        &late,
        &opening("on_command", "late", register),
    )]);
    let watcher = manifest("watcher", &["event:window-closed"], "^0.1", "");
    let subscribe = r#"assert(cx.subscribe({ "window-closed" }))"#;
    packages.extend([stored(
        &store,
        "watcher",
        &watcher,
        &opening("on_event", "watched", subscribe),
    )]);
    // A plugin of another plugin API, and a package changed since it was
    // stored.
    let old = manifest("old", &[], "^9.0", "");
    packages.extend([stored(&store, "old", &old, &opening("activate", "old", ""))]);
    let changed = manifest("changed", &[], "^0.1", "");
    let (name, locked) = stored(
        &store,
        "changed",
        &changed,
        &opening("activate", "changed", ""),
    );
    let tampered = store
        .folder(&locked.checksum.unwrap())
        .join("src/init.luau");
    std::fs::write(
        &tampered,
        "return { activate = function(cx) cx.open('https://evil') end }",
    )
    .unwrap();
    packages.insert(name, locked);
    // One of the folder that `sleip dev` links, of no checksum.
    let devved = folder.join("devved");
    package(
        &devved,
        &manifest("devved", &["startup"], "^0.1", ""),
        &opening("activate", "devved", ""),
    );
    packages.insert(
        "acme/devved".to_owned(),
        Locked {
            version: "1.0.0".to_owned(),
            kind: Kind::Luau,
            source: Source::Dev(devved.display().to_string()),
            checksum: None,
            capabilities: Vec::new(),
            dependencies: BTreeMap::new(),
        },
    );
    let lock = folder.join("plugins.lock");
    Lock {
        api: lokyr_kernel::plugins::API_VERSION.to_owned(),
        packages,
    }
    .write(&lock)
    .unwrap();
    grant_open(&folder, &["early", "late", "watcher", "devved"]);
    // URLs that open without a question, as the plugins open them.
    let config = Config::parse("[links]\nurls = \"open\"\n", Path::new("config.toml")).unwrap();
    let mut fake = Fake::default();
    let mut app = app(config, quiet()).with_installed(lock, store);
    fake.start(&mut app);
    // The plugins of the start activate, and open what they open.
    run_until(&mut fake, &mut app, |fake, _| {
        opened(fake, "early") && opened(fake, "devved")
    });
    let status = |app: &App, name: &str| app.plugins.status(&format!("acme/{name}"));
    assert_eq!(status(&app, "early"), Some(plugins::Status::Active));
    assert_eq!(status(&app, "devved"), Some(plugins::Status::Active));
    // The others wait, the command's showing.
    assert_eq!(status(&app, "late"), Some(plugins::Status::Inactive));
    assert_eq!(status(&app, "watcher"), Some(plugins::Status::Inactive));
    assert_eq!(
        app.commands
            .get("late.go")
            .map(|command| command.title.as_str()),
        Some("Go Late")
    );
    // Refused, as the notices tell.
    assert_eq!(status(&app, "old"), None);
    assert_eq!(status(&app, "changed"), None);
    let told = notices(&mut app);
    assert!(
        told.iter().any(|text| text.contains("acme/old")
            && text.contains("^9.0")
            && text.contains(lokyr_kernel::plugins::API_VERSION)),
        "{told:?}"
    );
    assert!(
        told.iter()
            .any(|text| text.contains("acme/changed")
                && text.contains("changed since it was stored")),
        "{told:?}"
    );
    assert!(
        !opened(&fake, "changed") && !fake.opened().iter().any(|target| target == "https://evil")
    );
    // The command's run activates its plugin, which runs it.
    fake.send_app(&mut app, AppEvent::Command("late.go".to_owned()));
    run_until(&mut fake, &mut app, |fake, _| opened(fake, "late"));
    assert_eq!(status(&app, "late"), Some(plugins::Status::Active));
    assert_eq!(status(&app, "watcher"), Some(plugins::Status::Inactive));
    // The event activates its plugin, which hears it.
    app.plugins
        .post(Event::Workspace(WorkspaceEvent::WindowClosed {
            window: lokyr_kernel::plugins::host::WindowId(999_999),
        }));
    fake.send_app(&mut app, AppEvent::Wake);
    run_until(&mut fake, &mut app, |fake, _| opened(fake, "watched"));
    assert_eq!(status(&app, "watcher"), Some(plugins::Status::Active));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn an_app_of_an_empty_store_or_of_plugins_that_wait_makes_no_vm() {
    let _luau = LUAU.lock().unwrap_or_else(PoisonError::into_inner);
    let before = lokyr_plugin_host_luau::vms_made();
    // Of no lockfile.
    let folder = shelf("waiting");
    let store = Store::new(folder.join("store"));
    let mut fake = Fake::default();
    let mut empty =
        app(Config::default(), quiet()).with_installed(folder.join("plugins.lock"), store.clone());
    fake.start(&mut empty);
    assert_eq!(lokyr_plugin_host_luau::vms_made(), before);
    // Of a plugin that waits for its command.
    let late = manifest(
        "late",
        &["command:late.go"],
        "^0.1",
        "[contributes]\ncommands = [{ name = \"late.go\", title = \"Go Late\" }]\n",
    );
    let lock = folder.join("plugins.lock");
    Lock {
        api: lokyr_kernel::plugins::API_VERSION.to_owned(),
        packages: BTreeMap::from([stored(
            &store,
            "late",
            &late,
            &opening("on_command", "late", ""),
        )]),
    }
    .write(&lock)
    .unwrap();
    let mut fake = Fake::default();
    let mut waiting = app(Config::default(), quiet()).with_installed(lock, store);
    fake.start(&mut waiting);
    fake.send_app(&mut waiting, AppEvent::Command("window.new".to_owned()));
    assert_eq!(
        waiting.plugins.status("acme/late"),
        Some(plugins::Status::Inactive)
    );
    assert_eq!(lokyr_plugin_host_luau::vms_made(), before);
    // A lockfile that does not read tells it, and loads nothing.
    let broken = folder.join("broken.lock");
    std::fs::write(&broken, "version = 7\napi = \"0.1.0\"\n").unwrap();
    let mut fake = Fake::default();
    let mut unread =
        app(Config::default(), quiet()).with_installed(broken, Store::new(folder.join("store")));
    fake.start(&mut unread);
    let told = notices(&mut unread);
    assert!(
        told.iter()
            .any(|text| text.contains("plugins.lock does not read")),
        "{told:?}"
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_plugin_of_the_status_bar_s_trigger_activates_as_the_bar_first_shows() {
    use lokyr_kernel::plugins::ui::Slot;
    use lokyr_text_core::FontConfig;

    let _luau = LUAU.lock().unwrap_or_else(PoisonError::into_inner);
    let folder = shelf("slot");
    let store = Store::new(folder.join("store"));
    let bar = manifest(
        "bar",
        &["slot:status-bar"],
        "^0.1",
        "[contributes]\nslots = [\"status-bar\"]\n",
    );
    let entry = r#"
        return {
            render = function(cx, view)
                return {
                    kind = "text",
                    text = "lazy",
                    layout = { width = { points = 120 }, height = { points = 20 } },
                }
            end,
        }
    "#;
    let lock = folder.join("plugins.lock");
    Lock {
        api: lokyr_kernel::plugins::API_VERSION.to_owned(),
        packages: BTreeMap::from([stored(&store, "bar", &bar, entry)]),
    }
    .write(&lock)
    .unwrap();
    grant_open(&folder, &["bar"]);
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let mut fake = Fake::default();
    fake.set_bar(28.0);
    let mut app = App::new(Some(drawing), unpadded(), None)
        .with_program(idle())
        .with_installed(lock, store);
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(crate::fakes::Surface::default());
    let shown = |app: &App| {
        app.windows[&window]
            .chrome
            .iter()
            .find(|view| view.view.slot() == Some(Slot::StatusBar))
            .and_then(|view| view.frame)
            .is_some()
    };
    run_until(&mut fake, &mut app, |_, app| shown(app));
    assert_eq!(
        app.plugins.status("acme/bar"),
        Some(plugins::Status::Active)
    );
    let _ = std::fs::remove_dir_all(&folder);
}
