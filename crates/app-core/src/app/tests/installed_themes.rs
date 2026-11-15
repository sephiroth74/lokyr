//! The themes of the installed packages of themes: listed of their
//! package's name, named by `[theme]`, applied as their package comes and
//! kept with a notice as it goes, and taking no other theme's name.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use lokyr_kernel::plugins::host::{ThemeOrigin, Themes};
use lokyr_kernel::themes::ThemeVariant;
use lokyr_pkg_model::lock::{Lock, Locked, Source};
use lokyr_pkg_model::manifest::Kind;
use lokyr_pkg_model::store::{Store, digest_of};
use lokyr_vt_core::page::Rgb;

use super::installed::{notices, shelf};
use super::*;

/// The 16 colors of a theme's file.
const PALETTE: &str = r##"palette = ["#000000", "#110000", "#001100", "#111100", "#000011", "#110011", "#001111", "#111111",
           "#222222", "#330000", "#003300", "#333300", "#000033", "#330033", "#003333", "#ffffff"]"##;

/// A theme's file of `background`, dark.
fn theme(background: &str) -> String {
    format!("[terminal]\nbackground = \"{background}\"\nforeground = \"#e0e0e0\"\n{PALETTE}\n")
}

fn rgb(hex: u32) -> Rgb {
    let [_, r, g, b] = hex.to_be_bytes();
    Rgb { r, g, b }
}

/// Stores the package of themes `acme/<name>` of `themes`, each a theme's
/// name and its text, in `store`, as Sleip does, its manifest naming
/// `named`: the lockfile's entry.
fn stored(store: &Store, name: &str, named: &str, themes: &[(&str, String)]) -> (String, Locked) {
    let unpacked = std::env::temp_dir().join(format!(
        "lokyr-installed-themes-{}-unpacked-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&unpacked);
    std::fs::create_dir_all(unpacked.join("themes")).unwrap();
    std::fs::write(
        unpacked.join("plugin.toml"),
        format!(
            "[plugin]\nname = \"acme/{named}\"\nversion = \"1.0.0\"\nkind = \"themes\"\nlicense = \"MIT\"\n"
        ),
    )
    .unwrap();
    for (theme, text) in themes {
        std::fs::write(unpacked.join("themes").join(format!("{theme}.toml")), text).unwrap();
    }
    let digest = digest_of(&unpacked).unwrap();
    std::fs::create_dir_all(store.root()).unwrap();
    let _ = std::fs::remove_dir_all(store.folder(&digest));
    std::fs::rename(&unpacked, store.folder(&digest)).unwrap();
    let locked = Locked {
        version: "1.0.0".to_owned(),
        kind: Kind::Themes,
        source: Source::Registry("https://pkg.lokyr.dev".to_owned()),
        checksum: Some(digest),
        capabilities: Vec::new(),
        dependencies: BTreeMap::new(),
    };
    (format!("acme/{name}"), locked)
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

/// A change of the installed packages, as a watch tells it, and the
/// reload once it settles.
fn reloaded(fake: &mut Fake, app: &mut App) {
    app.reloads.changed.store(true, Ordering::Release);
    fake.send_app(app, AppEvent::Wake);
    fake.fire_timers(app, Instant::now() + Duration::from_secs(1));
}

/// The themes that a picker lists, once a read of them that `wanted` takes
/// ended.
fn listed(fake: &mut Fake, app: &mut App, wanted: impl Fn(&Themes) -> bool) -> Themes {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        app.take_themes(fake);
        let list = pickers::theme_list(
            &app.theme_catalog,
            (&app.theme_places(), app.waker.as_ref()),
            fake.appearance(),
            &app.config.theme,
        );
        if app.theme_catalog.borrow().finished() && wanted(&list) {
            return list;
        }
        assert!(std::time::Instant::now() < deadline, "{list:?}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// The installed theme `name` of `list`: its package.
fn package_of(list: &Themes, name: &str) -> Option<Option<String>> {
    list.themes
        .iter()
        .find(|theme| theme.name == name && theme.origin == ThemeOrigin::Installed)
        .map(|theme| theme.package.clone())
}

#[test]
fn an_installed_package_s_theme_is_listed_named_applied_and_kept_as_it_goes() {
    let folder: PathBuf = shelf("themes");
    let store = Store::new(folder.join("store"));
    let config = Config::parse("[theme]\ndark = \"Tides\"\n", Path::new("config.toml")).unwrap();
    let mut fake = Fake::default();
    let mut app = app(config, quiet())
        .with_installed(folder.join("plugins.lock"), store.clone())
        .with_state(Some(folder.join("state")));
    fake.start(&mut app);
    // Of no package yet: told, the built-in theme in force.
    let told = notices(&mut app);
    assert!(
        told.iter()
            .any(|text| text.contains("there is no theme `Tides`")),
        "{told:?}"
    );
    let builtin = Theme::builtin().dark;
    assert_eq!(app.theme.dark, builtin);
    let _ = listed(&mut fake, &mut app, |_| true);
    // Installed: applied at once, listed of its package; its theme of a
    // built-in theme's name none, so that the install changes no theme in
    // force.
    let tides = stored(
        &store,
        "tides",
        "tides",
        &[
            ("Tides", theme("#102030")),
            ("Lokyr Dark", theme("#010101")),
        ],
    );
    lock(&folder, BTreeMap::from([tides]));
    reloaded(&mut fake, &mut app);
    assert!(
        !app.theme_catalog.borrow().finished(),
        "a read of the new list at once"
    );
    assert_eq!(app.theme.dark.colors.background, rgb(0x102030));
    assert_eq!(app.diagnostics.shown_diagnostic(), None);
    let list = listed(&mut fake, &mut app, |list| {
        package_of(list, "Tides").is_some()
    });
    assert_eq!(
        package_of(&list, "Tides"),
        Some(Some("acme/tides".to_owned()))
    );
    let dark: Vec<(&str, ThemeOrigin)> = list
        .themes
        .iter()
        .filter(|theme| theme.name == "Lokyr Dark")
        .map(|theme| (theme.name.as_str(), theme.origin))
        .collect();
    assert_eq!(dark, [("Lokyr Dark", ThemeOrigin::BuiltIn)]);
    assert!(
        list.themes
            .iter()
            .any(|theme| theme.name == "Tides" && theme.variant == ThemeVariant::Dark)
    );
    // Its preview's colors, of the package's file.
    app.keep_previews(vec!["Tides".to_owned()]);
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while app.previews.get("Tides").is_none_or(Option::is_none) {
        assert!(std::time::Instant::now() < deadline);
        app.take_themes(&mut fake);
        app.keep_previews(vec!["Tides".to_owned()]);
        std::thread::sleep(Duration::from_millis(5));
    }
    // Removed: the theme in force stays, told; gone from the list.
    lock(&folder, BTreeMap::new());
    reloaded(&mut fake, &mut app);
    assert_eq!(app.theme.dark.colors.background, rgb(0x102030));
    let told = notices(&mut app);
    assert!(
        told.iter()
            .any(|text| text.contains("there is no theme `Tides`")
                && text.contains("the themes in force stay")),
        "{told:?}"
    );
    let list = listed(&mut fake, &mut app, |list| {
        package_of(list, "Tides").is_none()
    });
    assert!(list.themes.iter().all(|theme| theme.name != "Tides"));
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn a_package_of_themes_that_names_another_is_refused_and_told() {
    let folder = shelf("themes-refused");
    let store = Store::new(folder.join("store"));
    let wrong = stored(&store, "tides", "other", &[("Tides", theme("#102030"))]);
    lock(&folder, BTreeMap::from([wrong]));
    let config = Config::parse("", Path::new("config.toml")).unwrap();
    let mut fake = Fake::default();
    let mut app = app(config, quiet())
        .with_installed(folder.join("plugins.lock"), store)
        .with_state(Some(folder.join("state")));
    fake.start(&mut app);
    let told = notices(&mut app);
    assert!(
        told.iter()
            .any(|text| text
                == "The themes of acme/tides do not load: its package holds `acme/other`"),
        "{told:?}"
    );
    assert!(app.installed_themes.is_empty());
    let _ = std::fs::remove_dir_all(folder);
}
