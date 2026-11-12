//! Lokyr › Install Claude Code Integration…: the hooks by which Claude
//! Code tells an agent's state exactly, added to the `settings.json` of each
//! of its configurations that the home holds, `~/.claude` and the folders
//! of `CLAUDE_CONFIG_DIR` beside it, as `~/.claude-work`: the user asked of
//! each in turn, what it adds told, a copy of the file kept beside it, the
//! rest of the file as it was.

use std::path::{Path, PathBuf};

use lokyr_kernel::claude_code;
use lokyr_kernel::l10n::Arg;
use lokyr_platform_core::{Alert, AlertId, AlertStyle, Platform};

use super::App;
use crate::drawing::Backend;

/// The suffix of the copy of a `settings.json` that the hooks change.
const BACKUP: &str = "lokyr-backup";

/// The integration under way: the settings asked about now, the alert that
/// asks, those left to ask about, and those that took the hooks.
#[derive(Debug)]
pub(super) struct Install {
    asked: Option<(AlertId, PathBuf)>,
    left: Vec<PathBuf>,
    added: Vec<PathBuf>,
}

/// The `settings.json` of each of Claude Code's configurations in `home`:
/// its folders `.claude` and `.claude-*`, and `configured`, the folder of
/// `CLAUDE_CONFIG_DIR` where the app's start gives one; `.claude`'s
/// whether it is there or not, as Claude Code makes it.
pub(super) fn settings_in(home: &Path, configured: Option<&Path>) -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = std::fs::read_dir(home)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            (name == ".claude" || name.starts_with(".claude-")) && entry.path().is_dir()
        })
        .map(|entry| entry.path())
        .collect();
    folders.extend(configured.map(Path::to_path_buf));
    folders.push(home.join(".claude"));
    folders.sort();
    folders.dedup();
    folders
        .into_iter()
        .map(|folder| folder.join("settings.json"))
        .collect()
}

/// `path` of the home written as `~`.
fn shown(path: &Path) -> String {
    match std::env::home_dir().and_then(|home| path.strip_prefix(home).ok().map(Path::to_path_buf))
    {
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

impl<B: Backend> App<B> {
    /// Asks, of each of Claude Code's configurations in turn, whether its
    /// settings take the hooks.
    pub(super) fn install_claude_code(&mut self, platform: &mut dyn Platform) {
        // A home given, as a test's, reads nothing of the environment.
        let (home, configured) = match self.claude_home.clone() {
            Some(home) => (home, None),
            None => {
                let Some(home) = std::env::home_dir() else {
                    return;
                };
                let configured = std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from);
                (home, configured)
            }
        };
        let mut left = settings_in(&home, configured.as_deref());
        left.reverse();
        self.claude_install = Some(Install {
            asked: None,
            left,
            added: Vec::new(),
        });
        self.ask_next_claude_settings(platform);
    }

    /// Asks of the next settings that lack the hooks, or tells what was
    /// done once none is left.
    fn ask_next_claude_settings(&mut self, platform: &mut dyn Platform) {
        let Some(install) = self.claude_install.as_mut() else {
            return;
        };
        while let Some(path) = install.left.pop() {
            let text = std::fs::read_to_string(&path).ok();
            if text.as_deref().is_some_and(claude_code::has_hooks) {
                continue;
            }
            if let Some(text) = &text
                && let Err(error) = claude_code::merged(Some(text))
            {
                let place = shown(&path);
                let alert = Alert {
                    style: AlertStyle::Warning,
                    message: self
                        .texts
                        .text_with("claude-code-unreadable", &[("path", Arg::Text(&place))]),
                    detail: error.to_string(),
                    buttons: Vec::new(),
                    suppression: None,
                    input: None,
                };
                platform.show_alert(None, &alert);
                continue;
            }
            let place = shown(&path);
            let alert = Alert {
                style: AlertStyle::Informational,
                message: self
                    .texts
                    .text_with("claude-code-ask", &[("path", Arg::Text(&place))]),
                detail: self.texts.text("claude-code-ask-detail"),
                buttons: vec![
                    self.texts.text("claude-code-add"),
                    self.texts.text("claude-code-skip"),
                ],
                suppression: None,
                input: None,
            };
            let id = platform.show_alert(None, &alert);
            if let Some(install) = self.claude_install.as_mut() {
                install.asked = Some((id, path));
            }
            return;
        }
        let Some(install) = self.claude_install.take() else {
            return;
        };
        let (key, places) = if install.added.is_empty() {
            ("claude-code-none", String::new())
        } else {
            let places: Vec<String> = install.added.iter().map(|path| shown(path)).collect();
            ("claude-code-added", places.join("\n"))
        };
        let alert = Alert {
            style: AlertStyle::Informational,
            message: self.texts.text(&format!("{key}-message")),
            detail: self
                .texts
                .text_with(&format!("{key}-detail"), &[("places", Arg::Text(&places))]),
            buttons: Vec::new(),
            suppression: None,
            input: None,
        };
        platform.show_alert(None, &alert);
    }

    /// The answer to the question of `alert`, if it asked of a settings'
    /// hooks: its first button adds them; whether it was one.
    pub(super) fn claude_code_answered(
        &mut self,
        platform: &mut dyn Platform,
        alert: AlertId,
        button: usize,
    ) -> bool {
        let Some(install) = self.claude_install.as_mut() else {
            return false;
        };
        let Some((_, path)) = install.asked.take_if(|(asked, _)| *asked == alert) else {
            return false;
        };
        if button == 0 {
            match add_hooks(&path) {
                Ok(()) => install.added.push(path),
                Err(error) => {
                    let place = shown(&path);
                    let alert = Alert {
                        style: AlertStyle::Warning,
                        message: self
                            .texts
                            .text_with("claude-code-failed", &[("path", Arg::Text(&place))]),
                        detail: error,
                        buttons: Vec::new(),
                        suppression: None,
                        input: None,
                    };
                    platform.show_alert(None, &alert);
                }
            }
        }
        self.ask_next_claude_settings(platform);
        true
    }
}

/// Adds the hooks to the settings at `path`, a copy of the file as it was
/// kept beside it, `settings.json.lokyr-backup`; the error of a failure,
/// which leaves the file as it was.
pub(super) fn add_hooks(path: &Path) -> Result<(), String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.to_string()),
    };
    let merged = claude_code::merged(text.as_deref()).map_err(|error| error.to_string())?;
    if let Some(text) = &text {
        let mut backup = path.as_os_str().to_owned();
        backup.push(".");
        backup.push(BACKUP);
        lokyr_pkg_model::write_whole(Path::new(&backup), text)
            .map_err(|error| error.to_string())?;
    }
    lokyr_pkg_model::write_whole(path, &merged).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of its own under the temporary one.
    fn folder(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("lokyr-claude-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn the_settings_are_each_configuration_s_and_the_default_one_s() {
        let home = folder("home");
        std::fs::create_dir_all(home.join(".claude-work")).unwrap();
        std::fs::create_dir_all(home.join(".claude-personal")).unwrap();
        std::fs::create_dir_all(home.join(".cache")).unwrap();
        std::fs::write(home.join(".claude-file"), "not a folder").unwrap();
        let elsewhere = home.join("elsewhere");
        let found = settings_in(&home, Some(&elsewhere));
        let names: Vec<String> = found
            .iter()
            .map(|path| path.strip_prefix(&home).unwrap().display().to_string())
            .collect();
        assert_eq!(
            names,
            [
                ".claude/settings.json",
                ".claude-personal/settings.json",
                ".claude-work/settings.json",
                "elsewhere/settings.json"
            ]
        );
    }

    #[test]
    fn the_hooks_go_in_with_a_copy_of_the_file_and_a_wrong_file_stays() {
        let home = folder("add");
        let path = home.join(".claude").join("settings.json");
        // A file not there yet, and its folder, made.
        add_hooks(&path).unwrap();
        assert!(claude_code::has_hooks(
            &std::fs::read_to_string(&path).unwrap()
        ));
        // A file of its own, copied first.
        std::fs::write(&path, "{\"model\": \"opus\"}").unwrap();
        add_hooks(&path).unwrap();
        let backup = home.join(".claude").join("settings.json.lokyr-backup");
        assert_eq!(
            std::fs::read_to_string(&backup).unwrap(),
            "{\"model\": \"opus\"}"
        );
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"model\": \"opus\"") && claude_code::has_hooks(&text));
        // Not JSON: told, and left as it is.
        std::fs::write(&path, "{ broken").unwrap();
        assert!(add_hooks(&path).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ broken");
    }
}
