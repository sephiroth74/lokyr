//! The command line tools, `lokyr` and `sleip`, linked into a folder of the
//! shells' `PATH`, `/usr/local/bin`, so that another terminal and the
//! scripts run them as the app's panes do, whose `PATH` ends in the
//! folder of the app's binaries: what is there looked at, the user asked
//! before what is not this app's is replaced, a folder left alone, and the
//! platform making the links, of an administrator's rights where the user's
//! are not enough.

use std::path::{Path, PathBuf};

use lokyr_kernel::l10n::Arg;
use lokyr_platform_core::{Alert, AlertStyle, LinkOutcome, Platform, ToolLink};

use super::App;
use crate::drawing::Backend;

/// The command line tools, the binaries of the app's folder.
const TOOLS: [&str; 2] = ["lokyr", "sleip"];

/// Where the links go and where their binaries are.
#[derive(Clone, Debug)]
pub(super) struct Places {
    /// The folder of the links, of the shells' `PATH`.
    pub(super) folder: PathBuf,
    /// The folder of the binaries; the app's own executable's, its links
    /// resolved, if none.
    pub(super) source: Option<PathBuf>,
}

impl Default for Places {
    fn default() -> Self {
        Self {
            folder: PathBuf::from("/usr/local/bin"),
            source: None,
        }
    }
}

/// What is at a link's path.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Found {
    /// Nothing.
    Nothing,
    /// A link to the app's own binary, through other links or not.
    Ours,
    /// A link to another file, of what it points to.
    Link(PathBuf),
    /// A file of another kind.
    File,
    /// A folder, which the app leaves alone.
    Folder,
}

/// What is at `link`, of `target` the app's binary.
fn found(link: &Path, target: &Path) -> Found {
    let Ok(metadata) = std::fs::symlink_metadata(link) else {
        return Found::Nothing;
    };
    if metadata.is_symlink() {
        let same = matches!(
            (std::fs::canonicalize(link), std::fs::canonicalize(target)),
            (Ok(to), Ok(own)) if to == own
        );
        return if same {
            Found::Ours
        } else {
            Found::Link(std::fs::read_link(link).unwrap_or_default())
        };
    }
    if metadata.is_dir() {
        Found::Folder
    } else {
        Found::File
    }
}

/// The folder of the app's executable, its links resolved: the bundle's
/// `Contents/MacOS`.
fn own_folder() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()?
        .canonicalize()
        .ok()?
        .parent()
        .map(Path::to_path_buf)
}

impl<B: Backend> App<B> {
    /// Links the command line tools into their folder: an alert tells that
    /// they are there already, as this app's; a question asks first where
    /// what is there is another program's; an alert refuses where a folder
    /// stands in their way, a binary is not beside the app, or the app runs
    /// from a place that goes, as its disk image, whose links would lead
    /// nowhere once it goes.
    pub(super) fn install_tools(&mut self, platform: &mut dyn Platform) {
        let folder = self.tools_places.folder.clone();
        let source = self
            .tools_places
            .source
            .clone()
            .or_else(own_folder)
            .unwrap_or_default();
        if platform.is_temporary_place(&source) {
            self.tools_alert(platform, AlertStyle::Warning, "tools-temporary", "");
            return;
        }
        let links: Vec<ToolLink> = TOOLS
            .iter()
            .map(|name| ToolLink {
                link: folder.join(name),
                target: source.join(name),
            })
            .collect();
        if let Some(missing) = links.iter().find(|link| !link.target.is_file()) {
            let path = missing.target.display().to_string();
            self.tools_alert(platform, AlertStyle::Warning, "tools-missing", &path);
            return;
        }
        let found: Vec<Found> = links
            .iter()
            .map(|link| found(&link.link, &link.target))
            .collect();
        let shown = folder.display().to_string();
        if found.iter().all(|found| *found == Found::Ours) {
            self.tools_alert(platform, AlertStyle::Informational, "tools-there", &shown);
            return;
        }
        if let Some((link, _)) = links
            .iter()
            .zip(&found)
            .find(|(_, found)| **found == Found::Folder)
        {
            let path = link.link.display().to_string();
            self.tools_alert(platform, AlertStyle::Warning, "tools-folder", &path);
            return;
        }
        let others: Vec<String> = links
            .iter()
            .zip(&found)
            .filter_map(|(link, found)| {
                let path = link.link.display().to_string();
                match found {
                    Found::Link(to) => Some(self.texts.text_with(
                        "tools-replace-link",
                        &[
                            ("path", Arg::Text(&path)),
                            ("target", Arg::Text(&to.display().to_string())),
                        ],
                    )),
                    Found::File => Some(
                        self.texts
                            .text_with("tools-replace-file", &[("path", Arg::Text(&path))]),
                    ),
                    _ => None,
                }
            })
            .collect();
        if others.is_empty() {
            self.link_tools(platform, links);
            return;
        }
        let alert = Alert {
            style: AlertStyle::Warning,
            message: self
                .texts
                .text_with("tools-replace-message", &[("folder", Arg::Text(&shown))]),
            detail: others.join("\n"),
            buttons: vec![
                self.texts.text("tools-replace-replace"),
                self.texts.text("tools-replace-cancel"),
            ],
            suppression: None,
            input: None,
        };
        let alert = platform.show_alert(None, &alert);
        self.tools_ask = Some((alert, links));
    }

    /// Has the platform make `links`, telling why if the system asks for an
    /// administrator's password.
    fn link_tools(&mut self, platform: &mut dyn Platform, links: Vec<ToolLink>) {
        let folder = self.tools_places.folder.display().to_string();
        let reason = self
            .texts
            .text_with("tools-reason", &[("folder", Arg::Text(&folder))]);
        platform.link_tools(links, &reason);
    }

    /// The question of the links that replace what is in their folder
    /// closed of `button`: they are made of the first, Replace.
    pub(super) fn tools_answered(
        &mut self,
        platform: &mut dyn Platform,
        links: Vec<ToolLink>,
        button: usize,
    ) {
        if button == 0 {
            self.link_tools(platform, links);
        }
    }

    /// The links were made, or not, as `outcome` tells, which an alert
    /// says; nothing of a question of the system's that the user cancelled.
    pub(super) fn tools_linked(&mut self, platform: &mut dyn Platform, outcome: &LinkOutcome) {
        match outcome {
            LinkOutcome::Linked => {
                let folder = self.tools_places.folder.display().to_string();
                self.tools_alert(platform, AlertStyle::Informational, "tools-linked", &folder);
            }
            LinkOutcome::Cancelled => {}
            LinkOutcome::Failed(reason) => {
                self.tools_alert(platform, AlertStyle::Warning, "tools-failed", reason);
            }
        }
    }

    /// Shows the alert of `key`, its message's and its detail's texts,
    /// `<key>-message` and `<key>-detail`, of the argument `value`, as a
    /// path, a folder or a reason, and an OK button.
    fn tools_alert(&self, platform: &mut dyn Platform, style: AlertStyle, key: &str, value: &str) {
        let args = [("value", Arg::Text(value))];
        let alert = Alert {
            style,
            message: self.texts.text_with(&format!("{key}-message"), &args),
            detail: self.texts.text_with(&format!("{key}-detail"), &args),
            buttons: Vec::new(),
            suppression: None,
            input: None,
        };
        platform.show_alert(None, &alert);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_s_path_tells_what_stands_there() {
        let base = std::env::temp_dir().join(format!("lokyr-found-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("folder")).unwrap();
        let own = base.join("own");
        std::fs::write(&own, "").unwrap();
        std::fs::write(base.join("file"), "").unwrap();
        std::os::unix::fs::symlink(&own, base.join("ours")).unwrap();
        // Through another link, the same binary.
        std::os::unix::fs::symlink(base.join("ours"), base.join("again")).unwrap();
        std::os::unix::fs::symlink("/nowhere/else", base.join("other")).unwrap();
        assert_eq!(found(&base.join("none"), &own), Found::Nothing);
        assert_eq!(found(&base.join("ours"), &own), Found::Ours);
        assert_eq!(found(&base.join("again"), &own), Found::Ours);
        assert_eq!(
            found(&base.join("other"), &own),
            Found::Link(PathBuf::from("/nowhere/else"))
        );
        assert_eq!(found(&base.join("file"), &own), Found::File);
        assert_eq!(found(&base.join("folder"), &own), Found::Folder);
        std::fs::remove_dir_all(&base).unwrap();
    }
}
