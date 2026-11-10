//! The tests of the consent to the installed plugins' capabilities: the
//! question at a plugin's first activation, granted or refused, kept and
//! told to the audit log; a command that waits for the answer; a new
//! version's new capability asked again; a capability revoked in the file,
//! which stops its plugin; and the question in Italian.

use std::collections::BTreeMap;
use std::path::Path;

use lokyr_kernel::commands::Capability;
use lokyr_pkg_model::lock::Lock;
use lokyr_pkg_model::permissions::{FILE, Permissions};
use lokyr_pkg_model::store::Store;
use lokyr_platform_core::fake::FakeAlert;

use super::installed::{manifest, notices, opened, opening, shelf, stored};
use super::*;

/// The lockfile, in `folder`, of the plugins of `packages`, each its name,
/// its manifest and its entry, stored in the store of `folder`: the app of
/// them, which opens URLs without a question, in `language`.
fn installed(folder: &Path, packages: &[(&str, String, String)], language: &str) -> App {
    let store = Store::new(folder.join("store"));
    let lock = folder.join("plugins.lock");
    Lock {
        api: lokyr_kernel::plugins::API_VERSION.to_owned(),
        packages: packages
            .iter()
            .map(|(name, manifest, entry)| stored(&store, name, manifest, entry))
            .collect::<BTreeMap<_, _>>(),
    }
    .write(&lock)
    .unwrap();
    let text = format!("[app]\nlanguage = \"{language}\"\n[links]\nurls = \"open\"\n");
    let config = Config::parse(&text, Path::new("config.toml")).unwrap();
    app(config, quiet())
        .with_installed(lock, store)
        .with_state(Some(folder.join("state")))
}

/// The alert that `fake` shows of the plugins' consent, the last one.
fn question(fake: &Fake) -> Option<FakeAlert> {
    fake.alerts()
        .iter()
        .rev()
        .find(|open| open.alert.buttons.len() == 2 && open.alert.message.contains("acme/"))
        .cloned()
}

/// The decisions kept beside the lockfile in `folder`.
fn kept(folder: &Path) -> Permissions {
    Permissions::read(&folder.join(FILE)).unwrap()
}

/// The audit log's lines in the state's folder of `folder`, each of its
/// time left out.
fn audited(folder: &Path) -> Vec<String> {
    std::fs::read_to_string(folder.join("state").join("audit.log"))
        .unwrap_or_default()
        .lines()
        .map(|line| {
            line.split_once(' ')
                .map_or(line, |(_, rest)| rest)
                .to_owned()
        })
        .collect()
}

/// The manifest of a plugin of `open` and `clipboard-write`, of `on`.
fn asking(name: &str, on: &[&str], more: &str) -> String {
    manifest(name, on, "^0.1", &format!("clipboard-write = true\n{more}"))
}

#[test]
fn a_plugin_s_first_activation_asks_once_and_the_answer_is_kept() {
    let _luau = LUAU.lock().unwrap_or_else(PoisonError::into_inner);
    let folder = shelf("consent-granted");
    let packages = [(
        "asker",
        asking("asker", &[], ""),
        opening("activate", "asker", ""),
    )];
    let mut fake = Fake::default();
    let mut app = installed(&folder, &packages, "en");
    fake.start(&mut app);
    // It waits for the answer, which nothing else asked.
    let asked = question(&fake).expect("a question");
    assert!(asked.window.is_none());
    assert_eq!(
        asked.alert.message,
        "Allow acme/asker to use these capabilities?"
    );
    assert!(
        asked
            .alert
            .detail
            .starts_with("• write to the clipboard: medium risk\n• open web pages and files with their apps: medium risk\n\n"),
        "{}",
        asked.alert.detail
    );
    assert_eq!(asked.alert.buttons, ["Don't Allow", "Allow"]);
    assert_eq!(
        app.plugins.status("acme/asker"),
        Some(plugins::Status::Inactive)
    );
    assert!(!opened(&fake, "asker"));
    // Allowed: it activates, of its capabilities, and the answer is kept.
    fake.answer_alert(&mut app, asked.id, 1);
    run_until(&mut fake, &mut app, |fake, _| opened(fake, "asker"));
    assert_eq!(
        kept(&folder).plugins["acme/asker"].granted,
        [Capability::ClipboardWrite, Capability::Open]
    );
    assert_eq!(
        audited(&folder),
        ["granted acme/asker 1.0.0 clipboard-write open"]
    );
    // The app's next start asks nothing.
    let mut again = Fake::default();
    let mut next = installed(&folder, &packages, "en");
    again.start(&mut next);
    run_until(&mut again, &mut next, |fake, _| opened(fake, "asker"));
    assert!(question(&again).is_none());
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn refused_capabilities_leave_the_plugin_running_without_them() {
    let _luau = LUAU.lock().unwrap_or_else(PoisonError::into_inner);
    let folder = shelf("consent-refused");
    // Of no capability, it tells it through its state.
    let entry = r#"
        return {
            activate = function(cx)
                local done, message, code = cx.open("https://lokyr.dev/refused")
                assert(cx.set_state({ code = code }))
            end,
        }
    "#;
    let packages = [("refuser", asking("refuser", &[], ""), entry.to_owned())];
    let mut fake = Fake::default();
    let mut app = installed(&folder, &packages, "en");
    fake.start(&mut app);
    let asked = question(&fake).expect("a question");
    fake.answer_alert(&mut app, asked.id, 0);
    let state = folder.join("state/plugins/refuser.json");
    run_until(&mut fake, &mut app, |_, _| state.exists());
    assert_eq!(std::fs::read_to_string(&state).unwrap(), "{\"code\":302}");
    assert_eq!(
        app.plugins.status("acme/refuser"),
        Some(plugins::Status::Active)
    );
    assert!(!opened(&fake, "refused"));
    assert_eq!(
        kept(&folder).plugins["acme/refuser"].refused,
        [Capability::ClipboardWrite, Capability::Open]
    );
    assert_eq!(
        audited(&folder),
        ["refused acme/refuser 1.0.0 clipboard-write open"]
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_command_that_asks_runs_once_the_user_allows() {
    let _luau = LUAU.lock().unwrap_or_else(PoisonError::into_inner);
    let folder = shelf("consent-command");
    let register = r#"assert(cx.register_command({ name = "late.go", title = "Go Late", category = "view" }))"#;
    let packages = [(
        "late",
        asking(
            "late",
            &["command:late.go"],
            "[contributes]\ncommands = [{ name = \"late.go\", title = \"Go Late\" }]\n",
        ),
        opening("on_command", "late", register),
    )];
    let mut fake = Fake::default();
    let mut app = installed(&folder, &packages, "en");
    fake.start(&mut app);
    assert!(question(&fake).is_none());
    fake.send_app(&mut app, AppEvent::Command("late.go".to_owned()));
    let asked = question(&fake).expect("a question");
    // Run again as the question waits, it runs once the answer comes, and
    // asks nothing more.
    fake.send_app(&mut app, AppEvent::Command("late.go".to_owned()));
    assert_eq!(fake.alerts().len(), 1);
    assert!(!opened(&fake, "late"));
    fake.answer_alert(&mut app, asked.id, 1);
    run_until(&mut fake, &mut app, |fake, _| {
        fake.opened()
            .iter()
            .filter(|target| target.ends_with("/late"))
            .count()
            == 2
    });
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_new_version_asks_for_its_new_capabilities_alone() {
    let _luau = LUAU.lock().unwrap_or_else(PoisonError::into_inner);
    let folder = shelf("consent-version");
    super::installed::grant_open(&folder, &["grown"]);
    let packages = [(
        "grown",
        asking("grown", &[], ""),
        opening("activate", "grown", ""),
    )];
    let mut fake = Fake::default();
    let mut app = installed(&folder, &packages, "en");
    fake.start(&mut app);
    let asked = question(&fake).expect("a question");
    assert!(
        asked
            .alert
            .detail
            .starts_with("• write to the clipboard: medium risk\n\n"),
        "{}",
        asked.alert.detail
    );
    assert!(!asked.alert.detail.contains("open web pages"));
    fake.answer_alert(&mut app, asked.id, 1);
    run_until(&mut fake, &mut app, |fake, _| opened(fake, "grown"));
    assert_eq!(
        kept(&folder).plugins["acme/grown"].granted,
        [Capability::ClipboardWrite, Capability::Open]
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_capability_revoked_in_the_file_stops_its_plugin() {
    let _luau = LUAU.lock().unwrap_or_else(PoisonError::into_inner);
    let folder = shelf("consent-revoked");
    let mut permissions = Permissions::default();
    permissions
        .plugins
        .entry("acme/kept".to_owned())
        .or_default()
        .decide(&[Capability::Open, Capability::ClipboardWrite], true);
    permissions.write(&folder.join(FILE)).unwrap();
    let packages = [(
        "kept",
        asking("kept", &[], ""),
        opening("activate", "kept", ""),
    )];
    let mut fake = Fake::default();
    let mut app = installed(&folder, &packages, "en");
    fake.start(&mut app);
    run_until(&mut fake, &mut app, |fake, _| opened(fake, "kept"));
    assert!(question(&fake).is_none());
    // The user takes `open` away, as the file's watch tells.
    permissions
        .plugins
        .get_mut("acme/kept")
        .unwrap()
        .granted
        .retain(|capability| *capability != Capability::Open);
    permissions.write(&folder.join(FILE)).unwrap();
    app.permissions_file
        .as_ref()
        .unwrap()
        .changed
        .store(true, Ordering::Release);
    fake.send_app(&mut app, AppEvent::Wake);
    fake.fire_timers(&mut app, Instant::now() + Duration::from_secs(1));
    assert_eq!(
        app.plugins.status("acme/kept"),
        Some(plugins::Status::Disabled)
    );
    let told = notices(&mut app);
    assert!(
        told.iter().any(|text| text.contains("acme/kept")
            && text.contains("lost `open`, which the user revoked")),
        "{told:?}"
    );
    assert_eq!(audited(&folder), ["revoked acme/kept 1.0.0 open"]);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn the_question_tells_each_capability_and_its_risk_in_italian() {
    let _luau = LUAU.lock().unwrap_or_else(PoisonError::into_inner);
    let folder = shelf("consent-italian");
    let packages = [(
        "asker",
        manifest(
            "asker",
            &[],
            "^0.1",
            "pty-write = true\nenv-read = [\"GITHUB_TOKEN\"]\n",
        ),
        opening("activate", "asker", ""),
    )];
    let mut fake = Fake::default();
    let mut app = installed(&folder, &packages, "it");
    fake.start(&mut app);
    let asked = question(&fake).expect("a question");
    assert_eq!(
        asked.alert.message,
        "Consentire a acme/asker di usare queste capability?"
    );
    assert!(
        asked.alert.detail.starts_with(concat!(
            "• leggere la variabile d'ambiente GITHUB_TOKEN: rischio alto\n",
            "• aprire pagine web e file con le loro app: rischio medio\n",
            "• scrivere nei programmi dei pannelli, come nella shell: rischio molto alto\n\n",
            "Lokyr lo chiede una volta sola",
        )),
        "{}",
        asked.alert.detail
    );
    assert_eq!(asked.alert.buttons, ["Non consentire", "Consenti"]);
    // Every capability has its texts, in both languages.
    for language in ["en", "it"] {
        let texts = lokyr_kernel::l10n::Texts::new(language, &crate::app::CATALOGS);
        for capability in [
            Capability::TerminalRead,
            Capability::PtyWrite,
            Capability::ClipboardRead,
            Capability::ClipboardWrite,
            Capability::Notifications,
            Capability::Open,
            Capability::ProcessSpawn,
            Capability::Net("x".to_owned()),
            Capability::FsRead("/x".to_owned()),
            Capability::FsWrite("/x".to_owned()),
            Capability::GlobalHotkey,
            Capability::EnvRead("X".to_owned()),
            Capability::InputRead,
            Capability::HistoryRead,
            Capability::ProcessRead,
            Capability::UserCommands,
            Capability::WorkspaceWrite,
            Capability::PaneView,
            Capability::Decorate,
            Capability::PointerRead,
        ] {
            let text = crate::app::consent::capability_text(&texts, &capability);
            assert!(
                !text.starts_with("consent-"),
                "{language}: {capability} has no text"
            );
        }
    }
    let _ = std::fs::remove_dir_all(&folder);
}

/// Tells `app` that the lockfile changed, as its watch does, and reloads
/// once the burst settles.
fn reloaded(fake: &mut Fake, app: &mut App) {
    app.reloads.changed.store(true, Ordering::Release);
    fake.send_app(app, AppEvent::Wake);
    fake.fire_timers(app, Instant::now() + Duration::from_secs(1));
}

/// Tells `app` that `permissions.toml` changed, as its watch does.
fn permissions_told(app: &App) {
    app.permissions_file
        .as_ref()
        .unwrap()
        .changed
        .store(true, Ordering::Release);
}

#[test]
fn an_answer_to_a_question_of_a_version_gone_grants_nothing_of_the_new_one() {
    let _luau = LUAU.lock().unwrap_or_else(PoisonError::into_inner);
    let folder = shelf("consent-gone");
    // Its first version asks for `open`.
    let packages = [(
        "grown",
        manifest("grown", &[], "^0.1", ""),
        opening("activate", "grown", ""),
    )];
    let mut fake = Fake::default();
    let mut app = installed(&folder, &packages, "en");
    fake.start(&mut app);
    let first = question(&fake).expect("a question");
    // A new version, of `clipboard-write` too, as its question is open:
    // the question goes, and the new version asks of both.
    let store = Store::new(folder.join("store"));
    Lock {
        api: lokyr_kernel::plugins::API_VERSION.to_owned(),
        packages: BTreeMap::from([stored(
            &store,
            "grown",
            &asking("grown", &[], ""),
            &opening(
                "activate",
                "grown",
                r#"assert(cx.set_clipboard_text("v2"))"#,
            ),
        )]),
    }
    .write(&folder.join("plugins.lock"))
    .unwrap();
    reloaded(&mut fake, &mut app);
    assert!(fake.alerts().iter().all(|open| open.id != first.id));
    let second = question(&fake).expect("the new version's question");
    assert!(second.alert.detail.contains("write to the clipboard"));
    // The first's answer, which an alert of its own still running gives,
    // grants nothing, and writes nothing.
    fake.send_app(
        &mut app,
        AppEvent::AlertClosed {
            alert: first.id,
            button: 1,
            suppressed: false,
            text: None,
        },
    );
    assert_eq!(
        app.plugins.status("acme/grown"),
        Some(plugins::Status::Inactive)
    );
    assert!(!folder.join(FILE).exists());
    // The second's refusal keeps both refused, and the plugin runs
    // without them.
    fake.answer_alert(&mut app, second.id, 0);
    assert_eq!(
        kept(&folder).plugins["acme/grown"].refused,
        [Capability::ClipboardWrite, Capability::Open]
    );
    run_until(&mut fake, &mut app, |_, app| {
        app.plugins.status("acme/grown") == Some(plugins::Status::Disabled)
    });
    assert!(!opened(&fake, "grown"));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_permissions_file_that_does_not_read_stops_no_plugin_and_is_never_written_over() {
    let _luau = LUAU.lock().unwrap_or_else(PoisonError::into_inner);
    let folder = shelf("consent-typo");
    super::installed::grant_open(&folder, &["kept"]);
    let packages = [
        (
            "kept",
            manifest("kept", &[], "^0.1", ""),
            opening("activate", "kept", ""),
        ),
        (
            "late",
            manifest(
                "late",
                &["command:late.go"],
                "^0.1",
                "[contributes]\ncommands = [{ name = \"late.go\", title = \"Go\" }]\n",
            ),
            opening(
                "on_command",
                "late",
                r#"assert(cx.register_command({ name = "late.go", title = "Go", category = "view" }))"#,
            ),
        ),
    ];
    let mut fake = Fake::default();
    let mut app = installed(&folder, &packages, "en");
    fake.start(&mut app);
    run_until(&mut fake, &mut app, |fake, _| opened(fake, "kept"));
    // A typo as the user edits the file: told, and nothing stops.
    let typo = std::fs::read_to_string(folder.join(FILE))
        .unwrap()
        .replace("\"open\"", "\"opne\"");
    std::fs::write(folder.join(FILE), &typo).unwrap();
    permissions_told(&app);
    fake.send_app(&mut app, AppEvent::Wake);
    fake.fire_timers(&mut app, Instant::now() + Duration::from_secs(1));
    assert_eq!(
        app.plugins.status("acme/kept"),
        Some(plugins::Status::Active)
    );
    assert!(notices(&mut app).iter().any(|text| text.contains("opne")));
    assert!(
        audited(&folder)
            .iter()
            .all(|line| !line.starts_with("revoked"))
    );
    // Told once however many reloads read it, as each save of `sleip dev`.
    for _ in 0..10 {
        reloaded(&mut fake, &mut app);
    }
    let told = notices(&mut app);
    assert_eq!(
        told.iter().filter(|text| text.contains("opne")).count(),
        1,
        "{told:?}"
    );
    // A question answered meanwhile holds for this run, and the file stays
    // the user's.
    fake.send_app(&mut app, AppEvent::Command("late.go".to_owned()));
    let asked = question(&fake).expect("a question");
    fake.answer_alert(&mut app, asked.id, 1);
    run_until(&mut fake, &mut app, |fake, _| opened(fake, "late"));
    assert_eq!(std::fs::read_to_string(folder.join(FILE)).unwrap(), typo);
    // The file repaired: the answer is written there, and nothing is
    // revoked, its plugin running on.
    std::fs::write(folder.join(FILE), typo.replace("\"opne\"", "\"open\"")).unwrap();
    permissions_told(&app);
    fake.send_app(&mut app, AppEvent::Wake);
    fake.fire_timers(&mut app, Instant::now() + Duration::from_secs(1));
    assert_eq!(
        app.plugins.status("acme/late"),
        Some(plugins::Status::Active)
    );
    let decisions = kept(&folder);
    assert!(
        decisions.plugins["acme/late"]
            .granted
            .contains(&Capability::Open),
        "{decisions:?}"
    );
    assert!(
        decisions.plugins["acme/kept"]
            .granted
            .contains(&Capability::Open),
        "{decisions:?}"
    );
    assert!(
        audited(&folder)
            .iter()
            .all(|line| !line.starts_with("revoked")),
        "{:?}",
        audited(&folder)
    );
    assert!(!app.diagnostics.tells(&About::Permissions));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_revocation_that_a_reload_reads_first_reaches_its_plugin() {
    let _luau = LUAU.lock().unwrap_or_else(PoisonError::into_inner);
    let folder = shelf("consent-reload");
    super::installed::grant_open(&folder, &["kept"]);
    let packages = [(
        "kept",
        manifest("kept", &[], "^0.1", ""),
        opening("activate", "kept", ""),
    )];
    let mut fake = Fake::default();
    let mut app = installed(&folder, &packages, "en");
    fake.start(&mut app);
    run_until(&mut fake, &mut app, |fake, _| opened(fake, "kept"));
    // The user revokes `open`, and a reload comes before the file's watch
    // tells it.
    let mut permissions = kept(&folder);
    permissions
        .plugins
        .get_mut("acme/kept")
        .unwrap()
        .decide(&[Capability::Open], false);
    permissions.write(&folder.join(FILE)).unwrap();
    reloaded(&mut fake, &mut app);
    assert_eq!(
        app.plugins.status("acme/kept"),
        Some(plugins::Status::Disabled)
    );
    assert_eq!(audited(&folder), ["revoked acme/kept 1.0.0 open"]);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn every_environment_variable_has_its_own_words() {
    for (language, every, one) in [
        (
            "en",
            "read every environment variable",
            "read the environment variable HOME",
        ),
        (
            "it",
            "leggere tutte le variabili d'ambiente",
            "leggere la variabile d'ambiente HOME",
        ),
    ] {
        let texts = lokyr_kernel::l10n::Texts::new(language, &crate::app::CATALOGS);
        let text = |name: &str| {
            crate::app::consent::capability_text(&texts, &Capability::EnvRead(name.to_owned()))
        };
        assert_eq!(text("*"), every);
        assert_eq!(text("HOME"), one);
    }
}

#[test]
fn a_revocation_that_an_answer_reads_first_reaches_its_plugin() {
    let _luau = LUAU.lock().unwrap_or_else(PoisonError::into_inner);
    let folder = shelf("consent-answer-read");
    super::installed::grant_open(&folder, &["kept"]);
    let packages = [
        (
            "kept",
            manifest("kept", &[], "^0.1", ""),
            opening("activate", "kept", ""),
        ),
        (
            "late",
            manifest(
                "late",
                &["command:late.go"],
                "^0.1",
                "[contributes]\ncommands = [{ name = \"late.go\", title = \"Go\" }]\n",
            ),
            opening(
                "on_command",
                "late",
                r#"assert(cx.register_command({ name = "late.go", title = "Go", category = "view" }))"#,
            ),
        ),
    ];
    let mut fake = Fake::default();
    let mut app = installed(&folder, &packages, "en");
    fake.start(&mut app);
    run_until(&mut fake, &mut app, |fake, _| opened(fake, "kept"));
    fake.send_app(&mut app, AppEvent::Command("late.go".to_owned()));
    let asked = question(&fake).expect("a question");
    // The user revokes `kept`'s `open`, and answers before the file's
    // watch tells it.
    let mut permissions = kept(&folder);
    permissions
        .plugins
        .get_mut("acme/kept")
        .unwrap()
        .decide(&[Capability::Open], false);
    permissions.write(&folder.join(FILE)).unwrap();
    fake.answer_alert(&mut app, asked.id, 1);
    assert_eq!(
        app.plugins.status("acme/kept"),
        Some(plugins::Status::Disabled)
    );
    let now = kept(&folder);
    assert_eq!(now.plugins["acme/kept"].refused, [Capability::Open]);
    assert_eq!(now.plugins["acme/late"].granted, [Capability::Open]);
    let _ = std::fs::remove_dir_all(&folder);
}
