//! The links of the app's command line tools, made on a thread of their
//! own: of the app's own rights where the folder lets the user write in it,
//! else of an administrator's, which `osascript`'s `do shell script … with
//! administrator privileges` asks the system for, the system asking the
//! user's password, as the folder of the shells' `PATH`, `/usr/local/bin`,
//! is the system's own. The outcome waits in [`DONE`] until the main
//! thread takes it at the wake that the thread gives.

use std::io;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;

use lokyr_platform_core::{AppEvent, LinkOutcome, ToolLink, Waker};

use crate::shared::{Pending, Shared};

/// The outcomes of the links made, which the main thread takes.
static DONE: Mutex<Vec<LinkOutcome>> = Mutex::new(Vec::new());

/// What `osascript` tells of a question that the user cancelled.
const CANCELLED: &str = "(-128)";

/// Makes `links` on a thread of its own, telling `reason` if the system
/// asks for an administrator's password, and wakes the main thread of
/// `waker` once they are made, or not.
pub(crate) fn link(links: Vec<ToolLink>, reason: String, waker: Waker) {
    let started = std::thread::Builder::new()
        .name("command-line-tools".to_owned())
        .spawn(move || {
            let outcome = make(&links, &reason);
            DONE.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(outcome);
            waker.wake();
        });
    if let Err(error) = started {
        DONE.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(LinkOutcome::Failed(error.to_string()));
    }
}

/// Posts the app's event of each outcome, on the main thread.
pub(crate) fn take_linked(shared: &Shared) {
    let done = std::mem::take(
        &mut *DONE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    );
    for outcome in done {
        shared.post(Pending::App(AppEvent::ToolsLinked(outcome)));
    }
}

/// Makes `links`: directly, else, as the user may not write in their
/// folder, of an administrator's rights.
pub(crate) fn make(links: &[ToolLink], reason: &str) -> LinkOutcome {
    match directly(links) {
        Ok(()) => LinkOutcome::Linked,
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
            as_administrator(links, reason)
        }
        Err(error) => LinkOutcome::Failed(error.to_string()),
    }
}

/// Makes each of `links` of the app's own rights: its folder made, a link
/// beside it renamed over what was there, so that a link is whole or not
/// there.
fn directly(links: &[ToolLink]) -> io::Result<()> {
    for link in links {
        let folder = link.link.parent().unwrap_or(Path::new("/"));
        std::fs::create_dir_all(folder)?;
        let name = link
            .link
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let beside = folder.join(format!(".{name}.lokyr-{}", std::process::id()));
        let _ = std::fs::remove_file(&beside);
        std::os::unix::fs::symlink(&link.target, &beside)?;
        if let Err(error) = std::fs::rename(&beside, &link.link) {
            let _ = std::fs::remove_file(&beside);
            return Err(error);
        }
    }
    Ok(())
}

/// Makes `links` of an administrator's rights, through `osascript`, which
/// the system asks the user's password for, telling `reason`.
fn as_administrator(links: &[ToolLink], reason: &str) -> LinkOutcome {
    let ran = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(administrator_script(links, reason))
        .output();
    match ran {
        Ok(output) if output.status.success() => LinkOutcome::Linked,
        Ok(output) => {
            let told = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            if told.contains(CANCELLED) {
                LinkOutcome::Cancelled
            } else {
                LinkOutcome::Failed(told)
            }
        }
        Err(error) => LinkOutcome::Failed(error.to_string()),
    }
}

/// The AppleScript that makes `links` of an administrator's rights,
/// telling `reason` in the system's question: each link's folder made, and
/// the link made over what is there, without following one that is there.
pub(crate) fn administrator_script(links: &[ToolLink], reason: &str) -> String {
    let shell = links
        .iter()
        .map(|link| {
            let folder = link.link.parent().unwrap_or(Path::new("/"));
            format!(
                "/bin/mkdir -p {} && /bin/ln -shf {} {}",
                shell_quoted(folder),
                shell_quoted(&link.target),
                shell_quoted(&link.link)
            )
        })
        .collect::<Vec<_>>()
        .join(" && ");
    format!(
        "do shell script {} with prompt {} with administrator privileges",
        apple_string(&shell),
        apple_string(reason)
    )
}

/// `path` in single quotes for the shell, of its own single quotes closed,
/// escaped and opened again.
fn shell_quoted(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', r"'\''"))
}

/// `text` as an AppleScript string, of its backslashes and quotes escaped.
fn apple_string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', r"\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(link: &Path, target: &Path) -> ToolLink {
        ToolLink {
            link: link.to_owned(),
            target: target.to_owned(),
        }
    }

    #[test]
    fn the_administrator_s_script_quotes_each_path_and_the_reason() {
        let links = [link(
            Path::new("/usr/local/bin/it's"),
            Path::new("/Applications/A \"B\".app/Contents/MacOS/sleip"),
        )];
        let script = administrator_script(&links, "Lokyr wants \"this\"");
        assert_eq!(
            script,
            r#"do shell script "/bin/mkdir -p '/usr/local/bin' && /bin/ln -shf '/Applications/A \"B\".app/Contents/MacOS/sleip' '/usr/local/bin/it'\\''s'" with prompt "Lokyr wants \"this\"" with administrator privileges"#
        );
    }

    #[test]
    fn links_in_a_folder_of_the_user_s_are_made_directly_over_what_is_there() {
        let base = std::env::temp_dir().join(format!("lokyr-tools-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (bin, bundle) = (base.join("bin"), base.join("bundle"));
        std::fs::create_dir_all(&bundle).unwrap();
        for name in ["lokyr", "sleip"] {
            std::fs::write(bundle.join(name), name).unwrap();
        }
        // A file of another program where a link goes, and a folder to make.
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("sleip"), "another").unwrap();
        let links = [
            link(&bin.join("lokyr"), &bundle.join("lokyr")),
            link(&bin.join("sleip"), &bundle.join("sleip")),
            link(&base.join("new/lokyr"), &bundle.join("lokyr")),
        ];
        assert_eq!(make(&links, "reason"), LinkOutcome::Linked);
        for link in &links {
            assert_eq!(std::fs::read_link(&link.link).unwrap(), link.target);
        }
        assert_eq!(std::fs::read_to_string(bin.join("sleip")).unwrap(), "sleip");
        // Nothing left beside them.
        assert_eq!(std::fs::read_dir(&bin).unwrap().count(), 2);
        // Again, the same.
        assert_eq!(make(&links, "reason"), LinkOutcome::Linked);
        std::fs::remove_dir_all(&base).unwrap();
    }
}
