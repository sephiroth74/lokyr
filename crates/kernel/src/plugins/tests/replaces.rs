//! A plugin that replaces another: the other off while the one stands in,
//! of no key written, and on again as it goes; chains and cycles; the
//! replaced plugin's section, which the settings tell of its replacer; a
//! replacer turned off by its faults; and a third-party replacement, which
//! the user grants.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::config::Config;

/// A plugin whose activation registers its command, `<namespace>.go`, and
/// whose command faults, as a limit of a plugin's VM gives a fault.
struct Named(&'static str);

impl Plugin for Named {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        cx.register_command(command(&format!("{}.go", self.0)))
    }

    fn on_command(&mut self, _: &mut Cx<'_>, _: &str, _: Args) -> Result<(), Error> {
        Err(Error::new(Code::Fault, "ran past its deadline"))
    }
}

/// The manifest of the built-in plugin `lokyr/<namespace>`, which replaces
/// the plugins of `replaces`.
fn manifest(namespace: &str, replaces: &[&str]) -> String {
    let replaces: Vec<String> = replaces.iter().map(|name| format!("\"{name}\"")).collect();
    format!(
        "[plugin]\nname = \"lokyr/{namespace}\"\nkind = \"builtin\"\nlicense = \"MIT\"\nreplaces = [{}]\n",
        replaces.join(", ")
    )
}

/// How many times the header and the blocks were made.
static HEADERS: AtomicUsize = AtomicUsize::new(0);
static BLOCKS: AtomicUsize = AtomicUsize::new(0);

const HEADER: &str = "[plugin]\nname = \"lokyr/header\"\nkind = \"builtin\"\nlicense = \"MIT\"\n";
const BLOCKS_MANIFEST: &str = "[plugin]\nname = \"lokyr/blocks\"\nkind = \"builtin\"\nlicense = \"MIT\"\nreplaces = [\"lokyr/header\"]\n";

fn make_header() -> Box<dyn Plugin + Send> {
    HEADERS.fetch_add(1, Ordering::Relaxed);
    Box::new(Named("header"))
}

fn make_blocks() -> Box<dyn Plugin + Send> {
    BLOCKS.fetch_add(1, Ordering::Relaxed);
    Box::new(Named("blocks"))
}

/// The config of `text`.
fn config(text: &str) -> Config {
    Config::parse(text, Path::new("config.toml")).unwrap_or_else(|error| panic!("{error}"))
}

/// The plugins configured of `text` and activated, of the notices of their
/// sections.
fn applied(plugins: &mut Plugins, app: &mut App, text: &str) -> Vec<String> {
    let notices = plugins
        .configure(&config(text).plugins)
        .into_iter()
        .map(|notice| notice.text)
        .collect();
    let errors = plugins.activate(&mut app.context());
    assert!(errors.is_empty(), "{errors:?}");
    notices
}

/// The state of `name` as a read of the plugins tells it, and its replacer.
fn told(plugins: &mut Plugins, name: &str) -> (host::PluginState, Option<String>) {
    let infos = plugins.infos();
    let info = infos.iter().find(|info| info.name == name).unwrap();
    (info.state, info.replaced_by.clone())
}

#[test]
fn a_plugin_that_an_enabled_one_replaces_is_off_and_on_again_as_that_one_goes() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    for (manifest, make) in [
        (HEADER, make_header as fn() -> Box<dyn Plugin + Send>),
        (BLOCKS_MANIFEST, make_blocks),
    ] {
        plugins
            .add_registered(Registration { manifest, make })
            .unwrap();
    }
    // Replaced from the start: never activated, of no key written.
    assert!(applied(&mut plugins, &mut app, "").is_empty());
    assert_eq!(plugins.status("lokyr/header"), Some(Status::Off));
    assert_eq!(plugins.status("lokyr/blocks"), Some(Status::Active));
    assert!(app.registry.get("header.go").is_none());
    assert_eq!(
        told(&mut plugins, "lokyr/header"),
        (host::PluginState::Replaced, Some("lokyr/blocks".to_owned()))
    );
    // The replacer turned off: the other starts, as its own.
    let off = "[plugins.blocks]\nenabled = false\n";
    assert!(applied(&mut plugins, &mut app, off).is_empty());
    assert_eq!(plugins.status("lokyr/header"), Some(Status::Active));
    assert!(app.registry.get("header.go").is_some());
    assert_eq!(
        told(&mut plugins, "lokyr/header"),
        (host::PluginState::Active, None)
    );
    assert_eq!(told(&mut plugins, "lokyr/blocks").0, host::PluginState::Off);
    // On again: the other stops, and starts anew as the replacer goes.
    let made = HEADERS.load(Ordering::Relaxed);
    assert!(applied(&mut plugins, &mut app, "").is_empty());
    assert_eq!(plugins.status("lokyr/header"), Some(Status::Off));
    assert!(app.registry.get("header.go").is_none());
    assert!(applied(&mut plugins, &mut app, off).is_empty());
    assert_eq!(plugins.status("lokyr/header"), Some(Status::Active));
    assert_eq!(HEADERS.load(Ordering::Relaxed), made + 1);
    // A replaced plugin turned off by its own section is off, not
    // replaced; its keys, while it is replaced, tell nothing of it, as the
    // settings show its section's replacer.
    let both = "[plugins.header]\nenabled = false\n";
    assert!(applied(&mut plugins, &mut app, both).is_empty());
    assert_eq!(
        told(&mut plugins, "lokyr/header"),
        (host::PluginState::Off, Some("lokyr/blocks".to_owned()))
    );
    let keyed = applied(&mut plugins, &mut app, "[plugins.header]\nsize = 2\n");
    assert!(
        keyed.iter().all(|notice| !notice.contains("lokyr/blocks")),
        "{keyed:?}"
    );
    // The replacer removed: the other starts.
    assert!(plugins.remove("lokyr/blocks", &mut app.context()));
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(plugins.status("lokyr/header"), Some(Status::Active));
    assert_eq!(
        told(&mut plugins, "lokyr/header"),
        (host::PluginState::Active, None)
    );
}

#[test]
fn a_chain_replaces_as_it_goes_and_a_cycle_replaces_nothing_of_a_notice() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    // `a` replaces `b`, which would replace `c`; `d` and `e` replace each
    // other; `f` would replace the plugins' page.
    for (namespace, replaces) in [
        ("a", &["lokyr/b"][..]),
        ("b", &["lokyr/c"]),
        ("c", &[]),
        ("d", &["lokyr/e"]),
        ("e", &["lokyr/d"]),
    ] {
        plugins
            .add_builtin(&manifest(namespace, replaces), Box::new(Idle))
            .unwrap();
    }
    let notices = applied(&mut plugins, &mut app, "");
    assert_eq!(
        notices,
        ["the plugins `lokyr/d`, `lokyr/e` replace each other, and none of them replaces another"]
    );
    let states: Vec<(host::PluginState, Option<String>)> = ["a", "b", "c", "d", "e"]
        .iter()
        .map(|namespace| told(&mut plugins, &format!("lokyr/{namespace}")))
        .collect();
    assert_eq!(
        states,
        [
            (host::PluginState::Active, None),
            (host::PluginState::Replaced, Some("lokyr/a".to_owned())),
            (host::PluginState::Active, None),
            (host::PluginState::Active, None),
            (host::PluginState::Active, None),
        ]
    );
    // `a` turned off: `b` runs, and replaces `c`.
    applied(&mut plugins, &mut app, "[plugins.a]\nenabled = false\n");
    assert_eq!(plugins.status("lokyr/b"), Some(Status::Active));
    assert_eq!(
        told(&mut plugins, "lokyr/c"),
        (host::PluginState::Replaced, Some("lokyr/b".to_owned()))
    );
}

/// How many times the faulty replacer was made.
static FAULTY: AtomicUsize = AtomicUsize::new(0);

const FAULTY_MANIFEST: &str = "[plugin]\nname = \"lokyr/faulty\"\nkind = \"builtin\"\nlicense = \"MIT\"\nreplaces = [\"lokyr/header\"]\n";

fn make_faulty() -> Box<dyn Plugin + Send> {
    FAULTY.fetch_add(1, Ordering::Relaxed);
    Box::new(Named("faulty"))
}

#[test]
fn a_replacer_that_waits_to_start_again_replaces_still_and_one_turned_off_by_its_faults_does_not() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    for (manifest, make) in [
        (
            HEADER,
            make_uncounted_header as fn() -> Box<dyn Plugin + Send>,
        ),
        (FAULTY_MANIFEST, make_faulty),
    ] {
        plugins
            .add_registered(Registration { manifest, make })
            .unwrap();
    }
    assert!(applied(&mut plugins, &mut app, "").is_empty());
    assert_eq!(plugins.status("lokyr/header"), Some(Status::Off));
    let start = Instant::now();
    let fault = |plugins: &mut Plugins, app: &mut App, at: u64| {
        let _ = plugins.run("faulty.go", Args::new(), &mut app.context());
        assert_eq!(plugins.status("lokyr/faulty"), Some(Status::Disabled));
        let freed = plugins.schedule_restarts(start + Duration::from_secs(at));
        if freed {
            assert!(plugins.activate(&mut app.context()).is_empty());
        }
        let _ = plugins.restart_due(start + Duration::from_secs(at + 10), &mut app.context());
        freed
    };
    // Its first two faults: it starts again, and the other stays off.
    assert!(!fault(&mut plugins, &mut app, 0));
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(plugins.status("lokyr/header"), Some(Status::Off));
    assert!(!fault(&mut plugins, &mut app, 1));
    assert_eq!(plugins.status("lokyr/header"), Some(Status::Off));
    // Its third within a minute turns it off: the other starts.
    assert!(fault(&mut plugins, &mut app, 2));
    assert_eq!(plugins.status("lokyr/faulty"), Some(Status::Disabled));
    assert_eq!(plugins.status("lokyr/header"), Some(Status::Active));
    assert_eq!(
        told(&mut plugins, "lokyr/header"),
        (host::PluginState::Active, None)
    );
}

/// An installed plugin that does nothing of its own.
struct Idle;

impl Plugin for Idle {}

/// The header, of no count, for a test that runs beside the one that counts.
fn make_uncounted_header() -> Box<dyn Plugin + Send> {
    Box::new(Named("header"))
}

#[test]
fn a_third_party_replacement_is_asked_with_the_capabilities_and_holds_once_granted() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    plugins
        .add_registered(Registration {
            manifest: HEADER,
            make: make_uncounted_header,
        })
        .unwrap();
    let replacer = Manifest::parse(
        "[plugin]\nname = \"acme/tabs-plus\"\nversion = \"1.0.0\"\nkind = \"luau\"\napi = \"^0.1\"\nlicense = \"MIT\"\nreplaces = [\"lokyr/header\"]\n",
    )
    .unwrap();
    let replacement = Capability::Replaces("lokyr/header".to_owned());
    plugins
        .add_installed(replacer, Box::new(|| Box::new(Idle)), (&[], &[]))
        .unwrap();
    // Until the answer, the other runs; the question tells the replacement.
    assert!(applied(&mut plugins, &mut app, "").is_empty());
    assert_eq!(plugins.status("lokyr/header"), Some(Status::Active));
    let asked: Vec<Vec<Capability>> = plugins
        .take_requests()
        .into_iter()
        .filter_map(|request| match request {
            Request::AskConsent { capabilities, .. } => Some(capabilities),
            _ => None,
        })
        .collect();
    assert_eq!(asked, [vec![replacement.clone()]]);
    // Granted: the other stops.
    let errors = plugins.consent(
        "acme/tabs-plus",
        std::slice::from_ref(&replacement),
        true,
        &mut app.context(),
    );
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(plugins.status("lokyr/header"), Some(Status::Off));
    assert_eq!(
        told(&mut plugins, "lokyr/header"),
        (
            host::PluginState::Replaced,
            Some("acme/tabs-plus".to_owned())
        )
    );
    // Revoked: the replacer runs on, and the other starts again.
    let stopped = plugins.set_consent(
        "acme/tabs-plus",
        (&[], std::slice::from_ref(&replacement)),
        &mut app.context(),
    );
    assert!(!stopped);
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(plugins.status("lokyr/header"), Some(Status::Active));
}

/// A plugin of a config schema of one key, `size`, whose section the
/// settings tell.
struct Schemed;

impl Plugin for Schemed {
    fn config_schema(&self) -> Option<std::borrow::Cow<'static, str>> {
        Some(
            r#"{"type": "object", "properties": {"size": {"type": "integer", "default": 1}}}"#
                .into(),
        )
    }
}

fn make_schemed() -> Box<dyn Plugin + Send> {
    Box::new(Schemed)
}

#[test]
fn the_settings_tell_a_replaced_plugin_s_section_of_its_replacer_while_it_is_replaced() {
    const SCHEMA: &str = "[contributes]\nconfig-schema = \"own.schema.json\"\n";
    let mut app = App::default();
    let mut plugins = Plugins::new();
    for manifest in [
        format!("{HEADER}{SCHEMA}"),
        format!("{BLOCKS_MANIFEST}{SCHEMA}"),
    ] {
        let manifest: &'static str = Box::leak(manifest.into_boxed_str());
        plugins
            .add_registered(Registration {
                manifest,
                make: make_schemed,
            })
            .unwrap();
    }
    let replacer = |plugins: &mut Plugins, name: &str| {
        plugins
            .settings_sections()
            .iter()
            .find(|section| section.name == name)
            .map(|section| section.replaced_by.clone())
            .expect("its section")
    };
    assert!(applied(&mut plugins, &mut app, "").is_empty());
    assert_eq!(
        replacer(&mut plugins, "header").as_deref(),
        Some("lokyr/blocks")
    );
    assert_eq!(replacer(&mut plugins, "blocks"), None);
    // The replacer off: the other's section, of none.
    let off = "[plugins.blocks]\nenabled = false\n";
    assert!(applied(&mut plugins, &mut app, off).is_empty());
    assert_eq!(replacer(&mut plugins, "header"), None);
}
