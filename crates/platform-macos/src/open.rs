//! Opening URLs and paths with the system's handlers, through
//! `NSWorkspace`: a file that would run rather than open shows in the
//! Finder, selected.

use std::ffi::OsStr;
use std::fs::{self, Metadata};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use lokyr_platform_core::{Error, TargetKind};
use objc2::rc::Retained;
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSArray, NSBundle, NSString, NSURL};

/// The extensions of the files and the bundles that macOS runs, installs or
/// follows elsewhere, rather than opens: applications, Java archives,
/// Automator's workflows and actions, installers, preference panes, screen
/// savers, configuration profiles, and the Finder's locations, whose URL
/// can be of any scheme.
const RUNS: [&str; 15] = [
    "app",
    "command",
    "tool",
    "terminal",
    "jar",
    "workflow",
    "action",
    "pkg",
    "mpkg",
    "prefpane",
    "saver",
    "mobileconfig",
    "fileloc",
    "webloc",
    "inetloc",
];

/// The bundle identifier of the system's terminal, which runs what it opens.
const TERMINAL: &str = "com.apple.Terminal";

/// A target that opens: a URL of another scheme than `file`, or a path of
/// this machine, with its kind.
enum Resolved {
    Url(Retained<NSURL>),
    Path(Retained<NSURL>, TargetKind),
}

/// What `target`, a URL or an absolute path, is, as
/// [`Platform::target_kind`](lokyr_platform_core::Platform::target_kind)
/// tells.
fn resolve(workspace: &NSWorkspace, target: &str) -> Result<Resolved, Error> {
    let failed = |why: &str| Error::Open(target.to_owned(), why.to_owned());
    let path = if target.starts_with('/') {
        PathBuf::from(target)
    } else {
        // Foundation takes a text without a scheme as a relative URL.
        let url = NSURL::URLWithString(&NSString::from_str(target))
            .filter(|url| url.scheme().is_some())
            .ok_or_else(|| failed("it is not a URL"))?;
        if !url.isFileURL() {
            return Ok(Resolved::Url(url));
        }
        if let Some(host) = url.host()
            && !is_local(&host.to_string(), &host_name())
        {
            return Err(failed("it is on another machine"));
        }
        let path = url.path().ok_or_else(|| failed("it has no path"))?;
        PathBuf::from(path.to_string())
    };
    let metadata = fs::metadata(&path).map_err(|error| failed(&error.to_string()))?;
    let file = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    let opener = workspace
        .URLForApplicationToOpenURL(&file)
        .and_then(|application| NSBundle::bundleWithURL(&application))
        .and_then(|bundle| bundle.bundleIdentifier())
        .map(|identifier| identifier.to_string());
    let kind = if runs(&path, &metadata, opener.as_deref()) {
        TargetKind::Program
    } else if metadata.is_dir() {
        TargetKind::Directory
    } else {
        TargetKind::File
    };
    Ok(Resolved::Path(file, kind))
}

/// What `target` is, as
/// [`Platform::target_kind`](lokyr_platform_core::Platform::target_kind)
/// tells.
pub(crate) fn kind(target: &str) -> Result<TargetKind, Error> {
    Ok(match resolve(&NSWorkspace::sharedWorkspace(), target)? {
        Resolved::Url(_) => TargetKind::Url,
        Resolved::Path(_, kind) => kind,
    })
}

/// Opens `target`, as [`Platform::open`](lokyr_platform_core::Platform::open)
/// does.
pub(crate) fn open(target: &str) -> Result<(), Error> {
    let workspace = NSWorkspace::sharedWorkspace();
    let url = match resolve(&workspace, target)? {
        Resolved::Path(file, TargetKind::Program) => {
            workspace.activateFileViewerSelectingURLs(&NSArray::from_retained_slice(&[file]));
            return Ok(());
        }
        Resolved::Url(url) | Resolved::Path(url, _) => url,
    };
    if workspace.openURL(&url) {
        Ok(())
    } else {
        Err(Error::Open(
            target.to_owned(),
            "no application opens it".to_owned(),
        ))
    }
}

/// Shows `target` in the Finder, as
/// [`Platform::reveal`](lokyr_platform_core::Platform::reveal) does.
pub(crate) fn reveal(target: &str) -> Result<(), Error> {
    let workspace = NSWorkspace::sharedWorkspace();
    match resolve(&workspace, target)? {
        Resolved::Path(file, _) => {
            workspace.activateFileViewerSelectingURLs(&NSArray::from_retained_slice(&[file]));
            Ok(())
        }
        Resolved::Url(_) => Err(Error::Open(target.to_owned(), "it is no file".to_owned())),
    }
}

/// Whether opening `path`, of `metadata`, which the application of the
/// bundle identifier `opener` would open, runs it: a program, or a file of
/// one of [`RUNS`]' extensions, or one that the system's terminal opens; a
/// directory of one of those extensions, or a bundle with a program.
fn runs(path: &Path, metadata: &Metadata, opener: Option<&str>) -> bool {
    let extension = path
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| RUNS.iter().any(|runs| runs.eq_ignore_ascii_case(extension)));
    if metadata.is_dir() {
        return extension || path.join("Contents/MacOS").is_dir();
    }
    extension || metadata.permissions().mode() & 0o111 != 0 || opener == Some(TERMINAL)
}

/// Whether `host`, of a `file` URL, is this machine, whose name is `name`:
/// none, `localhost`, or its name, with or without the domain that one of
/// the two has.
fn is_local(host: &str, name: &str) -> bool {
    let short = |name: &'_ str| name.split('.').next().unwrap_or_default().to_owned();
    host.is_empty()
        || host.eq_ignore_ascii_case("localhost")
        || (!name.is_empty()
            && (host.eq_ignore_ascii_case(name)
                || host.eq_ignore_ascii_case(&short(name))
                || short(host).eq_ignore_ascii_case(name)))
}

/// This machine's name, as programs that write `file` URLs take it; empty
/// if it has none.
fn host_name() -> String {
    let mut name = [0u8; 256];
    // SAFETY: the buffer is as long as the length given, which leaves room
    // for the name's ending nul.
    let status = unsafe { libc::gethostname(name.as_mut_ptr().cast(), name.len() - 1) };
    if status != 0 {
        return String::new();
    }
    let end = name
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(name.len());
    String::from_utf8_lossy(&name[..end]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_that_would_run_shows_in_the_finder() {
        let directory = std::env::temp_dir().join(format!("lokyr-open-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(directory.join("Tool.bundle/Contents/MacOS")).unwrap();
        fs::create_dir_all(directory.join("Some.App")).unwrap();
        fs::create_dir_all(directory.join("notes")).unwrap();
        for file in ["a.txt", "b.command", "c.JAR", "d.webloc", "program", "e.sh"] {
            fs::write(directory.join(file), "").unwrap();
        }
        fs::set_permissions(directory.join("program"), fs::Permissions::from_mode(0o755)).unwrap();
        let runs = |name: &str, opener: Option<&str>| {
            let path = directory.join(name);
            runs(&path, &fs::metadata(&path).unwrap(), opener)
        };
        for name in [
            "Tool.bundle",
            "Some.App",
            "b.command",
            "c.JAR",
            "d.webloc",
            "program",
        ] {
            assert!(runs(name, None), "{name}");
        }
        for name in ["notes", "a.txt", "e.sh"] {
            assert!(!runs(name, None), "{name}");
        }
        // A file that the system's terminal opens runs in it.
        assert!(runs("e.sh", Some(TERMINAL)));
        assert!(!runs("e.sh", Some("com.apple.TextEdit")));
        fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn a_target_is_a_url_a_directory_a_file_or_a_program() {
        let directory = std::env::temp_dir().join(format!("lokyr-kind-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        let (file, program) = (directory.join("a.txt"), directory.join("b.command"));
        fs::write(&file, "").unwrap();
        fs::write(&program, "").unwrap();
        let path = |path: &Path| path.to_str().unwrap().to_owned();
        for (target, wanted) in [
            ("https://lokyr.dev".to_owned(), TargetKind::Url),
            ("mailto:me@lokyr.dev".to_owned(), TargetKind::Url),
            (path(&directory), TargetKind::Directory),
            (path(&file), TargetKind::File),
            (format!("file://{}", path(&file)), TargetKind::File),
            (format!("file://localhost{}", path(&file)), TargetKind::File),
            (path(&program), TargetKind::Program),
        ] {
            assert_eq!(kind(&target).ok(), Some(wanted), "{target}");
        }
        for target in [
            path(&directory.join("none")),
            format!("file://elsewhere.example{}", path(&file)),
            "not a url".to_owned(),
        ] {
            assert!(kind(&target).is_err(), "{target}");
        }
        assert!(reveal("https://lokyr.dev").is_err());
        fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn a_file_url_opens_on_this_machine_alone() {
        for host in [
            "",
            "localhost",
            "LOCALHOST",
            "mac.local",
            "Mac.local",
            "mac",
        ] {
            assert!(is_local(host, "mac.local"), "{host:?}");
        }
        for host in ["other", "other.local", "mac.example"] {
            assert!(!is_local(host, "mac.local"), "{host:?}");
        }
        assert!(is_local("mac.local", "mac"));
        assert!(!is_local("x", ""));
        assert!(!host_name().is_empty());
    }
}
