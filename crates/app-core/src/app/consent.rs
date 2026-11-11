//! The user's consent to the capabilities of the installed plugins: as a
//! plugin first activates, one question of the capabilities that its
//! manifest declares and that no decision grants or refuses yet, each told
//! in the language shown, with its risk; the answer kept in
//! `permissions.toml`, beside the config file, which the user edits too,
//! and told to the audit log, in the state's folder; and the file's
//! changes, a capability revoked stopping its plugin.

use std::io::Write as _;

use lokyr_kernel::commands::Capability;
use lokyr_kernel::diagnostics::About;
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
        | Capability::UserCommands
        | Capability::Packages => "consent-risk-very-high",
        Capability::TerminalRead
        | Capability::ClipboardRead
        | Capability::Net(_)
        | Capability::FsRead(_)
        | Capability::FsWrite(_)
        | Capability::EnvRead(_)
        | Capability::HistoryRead
        | Capability::WorkspaceWrite
        | Capability::KeepAwake => "consent-risk-high",
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
/// as a host or a path, if it takes one: `*` of every environment variable
/// its own words.
pub(super) fn capability_text(texts: &Texts, capability: &Capability) -> String {
    let written = capability.to_string();
    let (name, parameter) = written.split_once(':').unwrap_or((&written, ""));
    if name == "env-read" && parameter == "*" {
        return texts.text("consent-capability-env-read-any");
    }
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
    /// plugins, or of no file; `None` of a file that does not read, as a
    /// notice tells, one however many reads tell it, whose decisions in
    /// force stay, as the config's do, and which nothing writes over.
    pub(super) fn permissions(&mut self, platform: &mut dyn Platform) -> Option<Permissions> {
        let Some(path) = self
            .installed
            .as_ref()
            .map(|installed| installed.permissions.clone())
        else {
            return Some(Permissions::default());
        };
        match Permissions::read(&path) {
            Ok(permissions) => {
                self.resolve_unread(platform, &About::Permissions);
                Some(permissions)
            }
            Err(error) => {
                let text = self.texts.text_with(
                    "permissions-unread",
                    &[("error", Arg::Text(&error.to_string()))],
                );
                self.report_unread(platform, About::Permissions, text);
                None
            }
        }
    }

    /// Forgets the questions of the plugin `plugin`, which went, as a
    /// reload took it away or made it anew: each withdrawn, and an answer
    /// that comes of one all the same taken by none.
    pub(super) fn forget_consent(&mut self, platform: &mut dyn Platform, plugin: &str) {
        let asked: Vec<_> = self
            .consent_asks
            .iter()
            .filter(|(_, (asker, _))| asker == plugin)
            .map(|(alert, _)| *alert)
            .collect();
        for alert in asked {
            self.consent_asks.remove(&alert);
            platform.dismiss_alert(alert);
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
    /// `capabilities`: the decisions kept in `permissions.toml`, after the
    /// file's own changes that its watch has not told yet apply, so that
    /// none is lost; the audit log told; and the plugin activated, of those
    /// of them that it asks for still, or without them. A file that does
    /// not read is never written over: the answer waits, in force, until
    /// it reads again, when it is written there.
    pub(super) fn consent_answered(
        &mut self,
        platform: &mut dyn Platform,
        (plugin, capabilities): (String, Vec<Capability>),
        allowed: bool,
    ) {
        let readable = self.permissions_changed(platform);
        // Of the capabilities that the decisions just read leave undecided
        // alone: one that the file decided meanwhile, as it was fixed while
        // the question showed, stays the file's.
        let held = self.decisions.plugins.get(&plugin);
        let answered: Vec<Capability> = capabilities
            .iter()
            .filter(|capability| {
                !held.is_some_and(|held| {
                    held.granted.contains(capability) || held.refused.contains(capability)
                })
            })
            .cloned()
            .collect();
        if !answered.is_empty() {
            self.decisions
                .plugins
                .entry(plugin.clone())
                .or_default()
                .decide(&answered, allowed);
            if !readable {
                self.unwritten_answers
                    .push((plugin.clone(), answered.clone(), allowed));
            } else if let Some(installed) = &self.installed
                && let Err(error) = self.decisions.write(&installed.permissions)
            {
                eprintln!(
                    "lokyr: the decisions on the plugins' capabilities cannot be written to {}: {error}",
                    installed.permissions.display()
                );
                // Waiting, in force, for the next write.
                self.unwritten_answers
                    .push((plugin.clone(), answered.clone(), allowed));
            }
            self.audit(
                if allowed { "granted" } else { "refused" },
                &plugin,
                &answered,
            );
        }
        for error in self.with_plugins(platform, |plugins, context| {
            plugins.consent(&plugin, &capabilities, allowed, context)
        }) {
            eprintln!("lokyr: {error}");
        }
        self.after_plugins(platform);
    }

    /// Reads `permissions.toml` again, as it changed: the answers given
    /// while it did not read merged over it and written, then each
    /// installed plugin takes its decisions, one that lost a capability
    /// that it held, as the user revoked it, stopping, as the audit log and
    /// the notice of the plugins that stopped tell. A file that does not
    /// read changes nothing, as a notice tells: whether it read.
    pub(super) fn permissions_changed(&mut self, platform: &mut dyn Platform) -> bool {
        let Some(mut permissions) = self.permissions(platform) else {
            return false;
        };
        let answers = std::mem::take(&mut self.unwritten_answers);
        if !answers.is_empty() {
            // Of the capabilities that the file leaves undecided alone: a
            // question asked as it did not read, as at a start, asked of
            // what it decided too, whose decision stays.
            let decided = permissions.clone();
            for (plugin, capabilities, allowed) in &answers {
                let held = decided.plugins.get(plugin);
                let undecided: Vec<Capability> = capabilities
                    .iter()
                    .filter(|capability| {
                        !held.is_some_and(|held| {
                            held.granted.contains(capability) || held.refused.contains(capability)
                        })
                    })
                    .cloned()
                    .collect();
                if !undecided.is_empty() {
                    permissions
                        .plugins
                        .entry(plugin.clone())
                        .or_default()
                        .decide(&undecided, *allowed);
                }
            }
            if permissions != decided
                && let Some(installed) = &self.installed
                && let Err(error) = permissions.write(&installed.permissions)
            {
                eprintln!(
                    "lokyr: the decisions on the plugins' capabilities cannot be written to {}: {error}",
                    installed.permissions.display()
                );
                // Waiting, in force, for the next write.
                self.unwritten_answers = answers;
            }
        }
        if permissions == self.decisions {
            return true;
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
        true
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
