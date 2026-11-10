//! The diagnostics: what goes wrong that the user is to learn of, each of a
//! code, whose hundreds tell its category, and of a text. The log takes
//! them all, and the windows' notice those that the user can act on; later,
//! the audit log and MCP take them too.

use crate::commands::{Code, ErrorCategory};

/// What a diagnostic is about, which a later one about the same takes the
/// place of, and a fix takes away.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum About {
    /// The config file, which a config that reads fixes.
    Config,
    /// The font, which a font that is there fixes.
    Font,
    /// The global keys, which the system gives once no other app takes
    /// them, or the config takes others.
    GlobalKeys,
    /// The `TERM` that the programs take, which a name of a terminfo entry
    /// fixes.
    Term,
    /// The themes of `[theme]`, which themes that resolve fix.
    Theme,
    /// A plugin's section of the config, `[plugins.<namespace>]`, by the
    /// namespace, which a section that its plugin takes fixes.
    PluginConfig(String),
    /// The windows kept as the app last quit, whose file does not read,
    /// which the next quit's file takes the place of.
    KeptWindows,
    /// The user's decisions on the plugins' capabilities,
    /// `permissions.toml`, which a file that reads fixes.
    Permissions,
    /// The installed plugins' lockfile, `plugins.lock`, which a lockfile
    /// that reads fixes.
    InstalledPlugins,
    /// Anything else, as a plugin or a command, of which each diagnostic
    /// is its own.
    Other,
}

/// Something that went wrong, for the user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// What went wrong, and how serious it is.
    pub code: Code,
    /// What went wrong, for a person, in English, as the log is.
    pub text: String,
    /// What it is about.
    pub about: About,
}

impl Diagnostic {
    /// A diagnostic of `code` about `about`, which `text` tells.
    pub fn new(code: Code, text: impl Into<String>, about: About) -> Self {
        Self {
            code,
            text: text.into(),
            about,
        }
    }

    /// How serious it is, as its code's hundreds tell.
    pub fn category(&self) -> ErrorCategory {
        self.code.category()
    }

    /// Whether a notice shows it: of the config or of the font, which the
    /// user fixes, and the failures and the faults; the other errors of
    /// usage go back to their callers, and those of security to the log,
    /// and later the audit log.
    pub fn noticed(&self) -> bool {
        self.about != About::Other
            || matches!(
                self.category(),
                ErrorCategory::Failure | ErrorCategory::Fault
            )
    }

    /// Its line of the log.
    pub fn log_line(&self) -> String {
        format!(
            "lokyr: error {} ({}): {}",
            self.code.number(),
            self.category(),
            self.text
        )
    }
}

/// The sink of the diagnostics, which keeps those that the notices show.
#[derive(Clone, Debug, Default)]
pub struct Diagnostics {
    /// The diagnostics that the notices show, the latest last.
    notices: Vec<Diagnostic>,
}

impl Diagnostics {
    /// Tells `diagnostic`: the log's line, and, where a notice shows it,
    /// the notice, in place of the one about the same. Whether the notices
    /// changed.
    pub fn report(&mut self, diagnostic: Diagnostic) -> bool {
        eprintln!("{}", diagnostic.log_line());
        if !diagnostic.noticed() {
            return false;
        }
        if diagnostic.about != About::Other {
            self.notices.retain(|shown| shown.about != diagnostic.about);
        }
        self.notices.push(diagnostic);
        true
    }

    /// Takes away the notice about `about`, as a fix does. Whether there
    /// was one.
    pub fn resolve(&mut self, about: &About) -> bool {
        let before = self.notices.len();
        self.notices.retain(|shown| shown.about != *about);
        self.notices.len() != before
    }

    /// Takes away the notices about what `of` holds for, as the plugins'
    /// sections that a config's reload checks again. Whether there were
    /// any.
    pub fn resolve_all(&mut self, of: impl Fn(&About) -> bool) -> bool {
        let before = self.notices.len();
        self.notices.retain(|shown| !of(&shown.about));
        self.notices.len() != before
    }

    /// Whether a notice tells about `about`.
    pub fn tells(&self, about: &About) -> bool {
        self.notices.iter().any(|shown| shown.about == *about)
    }

    /// The notice that shows: the latest.
    pub fn shown(&self) -> Option<&Diagnostic> {
        self.notices.last()
    }

    /// Closes the notice that shows, as a click does, so that the one
    /// before shows. Whether there was one.
    pub fn dismiss(&mut self) -> bool {
        self.notices.pop().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_notices_show_what_the_user_can_act_on_the_latest_first() {
        let mut sink = Diagnostics::default();
        // A usage error of a command, and a security one, the log alone.
        assert!(!sink.report(Diagnostic::new(Code::NotFound, "no `x.y`", About::Other)));
        assert!(!sink.report(Diagnostic::new(
            Code::Undeclared,
            "runs `a.b`",
            About::Other
        )));
        assert_eq!(sink.shown(), None);
        // The config's error, of usage, shows; a later one takes its place.
        let config = |text| Diagnostic::new(Code::InvalidConfig, text, About::Config);
        assert!(sink.report(config("line 1")));
        assert!(sink.report(Diagnostic::new(Code::Unavailable, "no Menlo", About::Font)));
        assert!(sink.report(config("line 2")));
        assert_eq!(
            sink.shown().map(|shown| shown.text.as_str()),
            Some("line 2")
        );
        // A failure and a fault each show, of their own.
        let failed = Diagnostic::new(Code::Failed, "it failed", About::Other);
        assert!(sink.report(failed.clone()));
        assert!(sink.report(failed.clone()));
        assert_eq!(sink.notices.len(), 4);
        // A click closes the one that shows; a fix, the config's.
        assert!(sink.dismiss());
        assert_eq!(sink.shown(), Some(&failed));
        assert!(sink.resolve(&About::Config));
        assert!(!sink.resolve(&About::Config));
        let texts: Vec<&str> = sink
            .notices
            .iter()
            .map(|shown| shown.text.as_str())
            .collect();
        assert_eq!(texts, ["no Menlo", "it failed"]);
        assert_eq!(config("x").log_line(), "lokyr: error 109 (usage): x");
    }
}
