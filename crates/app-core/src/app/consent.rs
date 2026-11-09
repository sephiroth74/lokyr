//! The user's consent to the capabilities of the installed plugins: as a
//! plugin first activates, one question of the capabilities that its
//! manifest declares and that no decision grants or refuses yet, each told
//! in the language shown, with its risk; the answer kept in
//! `permissions.toml`, beside the config file, which the user edits too,
//! and told to the audit log, in the state's folder; and the file's
//! changes, a capability revoked stopping its plugin.

use std::io::Write as _;

use lokyr_kernel::commands::{Capability, Code};
use lokyr_kernel::diagnostics::{About, Diagnostic};
use lokyr_kernel::l10n::{Arg, Texts};
use lokyr_pkg_model::permissions::Permissions;
use lokyr_platform_core::{Alert, AlertStyle, Platform};

use super::App;
use crate::drawing::Backend;

/// The audit log's file, in the state's folder.
pub(super) const AUDIT: &str = "audit.log";

/// The risk of `capability`, as the plugins' docs tell it: the catalogs'
/// message of it.
fn risk(capability: &Capability) -> &'static str {
    match capability {
        Capability::PtyWrite
        | Capability::ProcessSpawn
        | Capability::InputRead
        | Capability::UserCommands => "consent-risk-very-high",
        Capability::TerminalRead
        | Capability::ClipboardRead
        | Capability::Net(_)
        | Capability::FsRead(_)
        | Capability::FsWrite(_)
        | Capability::EnvRead(_)
        | Capability::HistoryRead
        | Capability::WorkspaceWrite => "consent-risk-high",
        Capability::ClipboardWrite
        | Capability::Open
        | Capability::GlobalHotkey
        | Capability::ProcessRead
        | Capability::PaneView
        | Capability::Decorate => "consent-risk-medium",
        Capability::Notifications | Capability::PointerRead => "consent-risk-low",
    }
}

/// What `capability` allows, in the language of `texts`, of its parameter,
/// as a host or a path, if it takes one.
pub(super) fn capability_text(texts: &Texts, capability: &Capability) -> String {
    let written = capability.to_string();
    let (name, parameter) = written.split_once(':').unwrap_or((&written, ""));
    texts.text_with(
        &format!("consent-capability-{name}"),
        &[("parameter", Arg::Text(parameter))],
    )
}

/// The question of whether the plugin `plugin` may hold `capabilities`, in
/// the language of `texts`: its default button refuses.
pub(super) fn question(texts: &Texts, plugin: &str, capabilities: &[Capability]) -> Alert {
    let lines: Vec<String> = capabilities
        .iter()
        .map(|capability| {
            let what = capability_text(texts, capability);
            let risk = texts.text(risk(capability));
            texts.text_with(
                "consent-line",
                &[("what", Arg::Text(&what)), ("risk", Arg::Text(&risk))],
            )
        })
        .collect();
    let detail = format!("{}\n\n{}", lines.join("\n"), texts.text("consent-detail"));
    Alert {
        style: AlertStyle::Warning,
        message: texts.text_with("consent-message", &[("name", Arg::Text(plugin))]),
        detail,
        buttons: vec![texts.text("consent-refuse"), texts.text("consent-allow")],
        suppression: None,
        input: None,
    }
}

impl<B: Backend> App<B> {
    /// The user's decisions, of `permissions.toml`: none of no installed
    /// plugins, and none of a file that does not read, as a notice tells.
    pub(super) fn permissions(&mut self, platform: &mut dyn Platform) -> Permissions {
        let Some(path) = self
            .installed
            .as_ref()
            .map(|installed| installed.permissions.clone())
        else {
            return Permissions::default();
        };
        match Permissions::read(&path) {
            Ok(permissions) => permissions,
            Err(error) => {
                let text = self.texts.text_with(
                    "permissions-unread",
                    &[("error", Arg::Text(&error.to_string()))],
                );
                self.report(
                    platform,
                    Diagnostic::new(Code::Unavailable, text, About::Other),
                );
                Permissions::default()
            }
        }
    }

    /// Asks the user whether the plugin `plugin` may hold `capabilities`,
    /// in an alert of the app's, whose answer
    /// [`App::consent_answered`] takes.
    pub(super) fn ask_consent(
        &mut self,
        platform: &mut dyn Platform,
        plugin: String,
        capabilities: Vec<Capability>,
    ) {
        let alert = question(&self.texts, &plugin, &capabilities);
        let asked = platform.show_alert(None, &alert);
        self.consent_asks.insert(asked, (plugin, capabilities));
    }

    /// Takes the user's answer to the question of whether `plugin` may hold
    /// `capabilities`: the decisions kept in `permissions.toml`, the audit
    /// log told, and the plugin activated, of them or without them.
    pub(super) fn consent_answered(
        &mut self,
        platform: &mut dyn Platform,
        (plugin, capabilities): (String, Vec<Capability>),
        allowed: bool,
    ) {
        let mut permissions = self.permissions(platform);
        permissions
            .plugins
            .entry(plugin.clone())
            .or_default()
            .decide(&capabilities, allowed);
        if let Some(installed) = &self.installed
            && let Err(error) = permissions.write(&installed.permissions)
        {
            eprintln!(
                "lokyr: the decisions on the plugins' capabilities cannot be written to {}: {error}",
                installed.permissions.display()
            );
        }
        self.decisions = permissions;
        self.audit(
            if allowed { "granted" } else { "refused" },
            &plugin,
            &capabilities,
        );
        for error in self.with_plugins(platform, |plugins, context| {
            plugins.consent(&plugin, allowed, context)
        }) {
            eprintln!("lokyr: {error}");
        }
        self.after_plugins(platform);
    }

    /// Reads `permissions.toml` again, as it changed: each installed plugin
    /// takes its decisions, one that lost a capability that it held, as
    /// the user revoked it, stopping, as the audit log and the notice of
    /// the plugins that stopped tell.
    pub(super) fn permissions_changed(&mut self, platform: &mut dyn Platform) {
        let permissions = self.permissions(platform);
        if permissions == self.decisions {
            return;
        }
        let names: Vec<String> = self.reloads.loaded.keys().cloned().collect();
        for name in names {
            let decisions = permissions.plugins.get(&name).cloned().unwrap_or_default();
            let before = self
                .decisions
                .plugins
                .get(&name)
                .map(|decisions| decisions.granted.clone())
                .unwrap_or_default();
            let stopped = self.with_plugins(platform, |plugins, context| {
                plugins.set_consent(&name, (&decisions.granted, &decisions.refused), context)
            });
            let revoked: Vec<Capability> = before
                .into_iter()
                .filter(|capability| !decisions.granted.contains(capability))
                .collect();
            if stopped || !revoked.is_empty() {
                self.audit("revoked", &name, &revoked);
            }
        }
        self.decisions = permissions;
        self.after_plugins(platform);
    }

    /// Adds a line to the audit log, in the state's folder: the time, what
    /// the user did, the plugin, its version and the capabilities.
    fn audit(&self, action: &str, plugin: &str, capabilities: &[Capability]) {
        let Some(state) = &self.state else {
            return;
        };
        let version = self
            .plugins
            .manifest(plugin)
            .and_then(|manifest| manifest.version.clone())
            .unwrap_or_default();
        let capabilities: Vec<String> = capabilities.iter().map(ToString::to_string).collect();
        let line = format!(
            "{} {action} {plugin} {version} {}\n",
            crate::log::timestamp(std::time::SystemTime::now()),
            capabilities.join(" ")
        );
        let written = std::fs::create_dir_all(state).and_then(|()| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(state.join(AUDIT))?
                .write_all(line.as_bytes())
        });
        if let Err(error) = written {
            eprintln!(
                "lokyr: the audit log cannot be written in {}: {error}",
                state.display()
            );
        }
    }
}
