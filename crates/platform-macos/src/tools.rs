//! The links of the app's command line tools, made on a thread of their
//! own: of the app's own rights where the folder lets the user write in it,
//! else of an administrator's, which `osascript`'s `do shell script … with
//! administrator privileges` asks the system for, the system asking the
//! user's password, as the folder of the shells' `PATH`, `/usr/local/bin`,
//! is the system's own. The outcome waits in [`DONE`] until the main
//! thread takes it at the wake that the thread gives.

use std::ffi::{CString, c_void};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;

use lokyr_platform_core::{AppEvent, LinkOutcome, ToolLink, Waker};
use objc2_foundation::{NSString, NSURL};

use crate::shared::{Pending, Shared};

/// The outcomes of the links made, which the main thread takes.
static DONE: Mutex<Vec<LinkOutcome>> = Mutex::new(Vec::new());

/// What `osascript` tells of a question that the user cancelled.
const CANCELLED: &str = "(-128)";

/// Whether `path` lies in a place that goes: a copy that Gatekeeper
/// translocated, or a read-only volume under `/Volumes`, as a disk
/// image's; none of a path that is empty or relative, of which no file's
/// URL is made.
pub(crate) fn is_temporary(path: &Path) -> bool {
    path.is_absolute() && (translocated(path) || on_read_only_volume(path))
}

/// `SecTranslocateIsTranslocatedURL` of the Security framework, of macOS
/// 10.12: whether its URL lies in a copy that Gatekeeper translocated.
type IsTranslocated = unsafe extern "C" fn(*const c_void, *mut bool, *mut *mut c_void) -> u8;

/// Whether `path` lies in a copy of an app that Gatekeeper translocated,
/// as it runs a quarantined app that the user did not move: as the
/// Security framework tells, whose function no header of the SDK declares,
/// so that it is found as the app runs; else as its path's folder of the
/// translocated copies tells.
fn translocated(path: &Path) -> bool {
    // SAFETY: the name is a text of a NUL at its end, and `RTLD_DEFAULT`
    // looks in every image loaded, the Security framework among them.
    let symbol = unsafe {
        libc::dlsym(
            libc::RTLD_DEFAULT,
            c"SecTranslocateIsTranslocatedURL".as_ptr(),
        )
    };
    if symbol.is_null() {
        return path
            .components()
            .any(|part| part.as_os_str() == "AppTranslocation");
    }
    // SAFETY: the symbol is the function of that signature, as the
    // Security framework's open sources declare it.
    let is_translocated: IsTranslocated = unsafe { std::mem::transmute(symbol) };
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    let mut translocated = false;
    // SAFETY: an `NSURL` is a `CFURL`, as Core Foundation bridges them; the
    // flag is written before the call returns; no error is asked for.
    let told = unsafe {
        is_translocated(
            objc2::rc::Retained::as_ptr(&url).cast(),
            &mut translocated,
            std::ptr::null_mut(),
        )
    };
    told != 0 && translocated
}

/// Whether `path` lies on a volume under `/Volumes` that is mounted
/// read-only, as a disk image's.
fn on_read_only_volume(path: &Path) -> bool {
    if !path.starts_with("/Volumes") {
        return false;
    }
    let Ok(text) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: the struct is plain data, which zeros make valid, and
    // `statfs` fills it in of a path of a NUL at its end.
    let mut volume: libc::statfs = unsafe { std::mem::zeroed() };
    // SAFETY: as above.
    let read = unsafe { libc::statfs(text.as_ptr(), &mut volume) };
    read == 0 && volume.f_flags & libc::MNT_RDONLY as u32 != 0
}

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

    #[test]
    fn a_disk_image_s_volume_is_a_place_that_goes_and_the_applications_folder_is_not() {
        assert!(!is_temporary(Path::new("/Applications")));
        assert!(!is_temporary(&std::env::temp_dir()));
        // Of no folder, as of a bundle moved while the app runs: none.
        assert!(!is_temporary(Path::new("")));
        assert!(!is_temporary(Path::new("Contents/MacOS")));
        // A read-only disk image, mounted under /Volumes as the Finder
        // mounts the app's.
        let base = std::env::temp_dir().join(format!("lokyr-dmg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("content/Lokyr.app/Contents/MacOS")).unwrap();
        let image = base.join("tools.dmg");
        let volume = format!("LokyrTools{}", std::process::id());
        let made = Command::new("hdiutil")
            .args([
                "create",
                "-quiet",
                "-format",
                "UDZO",
                "-volname",
                &volume,
                "-srcfolder",
            ])
            .arg(base.join("content"))
            .arg(&image)
            .status()
            .unwrap();
        assert!(made.success());
        let attached = Command::new("hdiutil")
            .args(["attach", "-quiet", "-nobrowse", "-readonly"])
            .arg(&image)
            .status()
            .unwrap();
        assert!(attached.success());
        let mounted = Path::new("/Volumes").join(&volume);
        let inside = mounted.join("Lokyr.app/Contents/MacOS");
        let told = is_temporary(&inside);
        let _ = Command::new("hdiutil")
            .args(["detach", "-quiet", "-force"])
            .arg(&mounted)
            .status();
        let _ = std::fs::remove_dir_all(&base);
        assert!(told, "{} is a disk image's", inside.display());
    }
}
