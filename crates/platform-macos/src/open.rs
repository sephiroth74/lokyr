//! Opening URLs and paths with the system's handlers, through
//! `NSWorkspace`: a file that would run rather than open shows in the
//! Finder, selected. A path's symbolic links and the Finder's aliases are
//! resolved first, so that what is checked is what opens.

use std::fs;
use std::path::{Path, PathBuf};

use lokyr_platform_core::{Error, TargetKind, runs_by_itself};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{
    NSArray, NSBundle, NSDictionary, NSString, NSURL, NSURLBookmarkResolutionOptions,
};

/// The applications that run, or install, the documents that they open:
/// Java archives, installers' packages, Automator's workflows, and Python's
/// scripts. The applications that declare a shell's role for documents,
/// as terminals do for scripts, run theirs too.
const RUNNERS: [&str; 4] = [
    "com.apple.JavaLauncher",
    "com.apple.installer",
    "com.apple.AutomatorInstaller",
    "org.python.PythonLauncher",
];

/// How many aliases a path can lead through, each to the next.
const ALIASES: usize = 8;

/// A target that opens: a URL of another scheme than `file`, or the file
/// that a path of this machine leads to, with its kind.
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
    let path = resolved(&path).map_err(|why| failed(&why))?;
    let metadata = fs::metadata(&path).map_err(|error| failed(&error.to_string()))?;
    let file = file_url(&path);
    let opener = workspace
        .URLForApplicationToOpenURL(&file)
        .and_then(|application| NSBundle::bundleWithURL(&application));
    let kind = if runs_by_itself(&path, &metadata) || opener.as_deref().is_some_and(runs_documents)
    {
        TargetKind::Program
    } else if metadata.is_dir() {
        TargetKind::Directory
    } else {
        TargetKind::File
    };
    Ok(Resolved::Path(file, kind))
}

/// `path` with its symbolic links and the Finder's aliases resolved, each
/// in turn, as the system resolves them as it opens a file: Foundation's
/// resolution of an alias follows the links too.
fn resolved(path: &Path) -> Result<PathBuf, String> {
    let mut path = path.to_path_buf();
    for _ in 0..ALIASES {
        let options = NSURLBookmarkResolutionOptions::WithoutUI
            | NSURLBookmarkResolutionOptions::WithoutMounting;
        let target = NSURL::URLByResolvingAliasFileAtURL_options_error(&file_url(&path), options)
            .map_err(|error| {
            format!("its alias leads nowhere: {}", error.localizedDescription())
        })?;
        let target = target
            .path()
            .map(|target| PathBuf::from(target.to_string()))
            .ok_or("its alias leads to no path")?;
        if target == path {
            return Ok(path);
        }
        path = target;
    }
    Err("its aliases lead to aliases".to_owned())
}

/// The `file` URL of `path`.
fn file_url(path: &Path) -> Retained<NSURL> {
    NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()))
}

/// Whether the application of `bundle` runs the documents that it opens:
/// one of [`RUNNERS`], or one that declares a shell's role for a type of
/// document.
fn runs_documents(bundle: &NSBundle) -> bool {
    if bundle
        .bundleIdentifier()
        .is_some_and(|identifier| RUNNERS.contains(&identifier.to_string().as_str()))
    {
        return true;
    }
    let types = bundle
        .objectForInfoDictionaryKey(&NSString::from_str("CFBundleDocumentTypes"))
        .and_then(|types| types.downcast::<NSArray>().ok());
    let role = NSString::from_str("CFBundleTypeRole");
    types.is_some_and(|types| {
        (0..types.count()).any(|index| {
            let kind: Retained<AnyObject> = types.objectAtIndex(index);
            kind.downcast::<NSDictionary>()
                .ok()
                .and_then(|kind| kind.objectForKey(&role))
                .and_then(|role| role.downcast::<NSString>().ok())
                .is_some_and(|role| role.to_string() == "Shell")
        })
    })
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
/// does: the file that its links and aliases lead to.
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
/// [`Platform::reveal`](lokyr_platform_core::Platform::reveal) does: the
/// file that its links and aliases lead to.
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

    /// A directory of the test's own, made afresh.
    fn directory(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!("lokyr-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    /// Makes `alias` a Finder's alias of `original`.
    fn make_alias(original: &Path, alias: &Path) {
        let bookmark = file_url(original)
            .bookmarkDataWithOptions_includingResourceValuesForKeys_relativeToURL_error(
                objc2_foundation::NSURLBookmarkCreationOptions::SuitableForBookmarkFile,
                None,
                None,
            )
            .unwrap();
        NSURL::writeBookmarkData_toURL_options_error(
            &bookmark,
            &file_url(alias),
            objc2_foundation::NSURLBookmarkCreationOptions::SuitableForBookmarkFile.0,
        )
        .unwrap();
    }

    #[test]
    fn a_link_or_an_alias_to_a_program_leads_to_the_finder() {
        use std::os::unix::fs::symlink;

        let directory = directory("links");
        for name in ["x.jar", "x.pkg", "x.fileloc", "notes.txt"] {
            fs::write(directory.join(name), "").unwrap();
        }
        // An application of its own, which opens itself.
        fs::create_dir_all(directory.join("Tool.app/Contents/MacOS")).unwrap();
        let path = |name: &str| directory.join(name).to_str().unwrap().to_owned();
        for (link, original) in [
            ("a.txt", "x.jar"),
            ("b.txt", "x.pkg"),
            ("c.txt", "x.fileloc"),
            ("d.txt", "notes.txt"),
        ] {
            symlink(directory.join(original), directory.join(link)).unwrap();
        }
        make_alias(&directory.join("x.jar"), &directory.join("alias.txt"));
        make_alias(&directory.join("Tool.app"), &directory.join("tool-alias"));
        make_alias(&directory.join("notes.txt"), &directory.join("note-alias"));
        for name in ["a.txt", "b.txt", "c.txt", "alias.txt", "tool-alias"] {
            assert_eq!(kind(&path(name)).ok(), Some(TargetKind::Program), "{name}");
        }
        for name in ["d.txt", "note-alias", "notes.txt"] {
            assert_eq!(kind(&path(name)).ok(), Some(TargetKind::File), "{name}");
        }
        // A link that leads nowhere does not open.
        symlink(directory.join("none"), directory.join("e.txt")).unwrap();
        assert!(kind(&path("e.txt")).is_err());
        fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn a_file_that_a_runner_opens_by_its_own_choice_leads_to_the_finder() {
        let directory = directory("opener");
        let file = directory.join("ow.txt");
        fs::write(&file, "").unwrap();
        assert_eq!(kind(file.to_str().unwrap()).ok(), Some(TargetKind::File));
        // The file's own choice of Jar Launcher, as the Finder's "Open
        // With" keeps it, a binary property list.
        let xml = directory.join("openwith.plist");
        fs::write(
            &xml,
            concat!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
                "<plist version=\"1.0\"><dict>",
                "<key>bundleidentifier</key><string>com.apple.JavaLauncher</string>",
                "<key>path</key><string>/System/Library/CoreServices/JavaLauncher.app</string>",
                "<key>version</key><integer>0</integer>",
                "</dict></plist>",
            ),
        )
        .unwrap();
        let converted = std::process::Command::new("plutil")
            .args(["-convert", "binary1"])
            .arg(&xml)
            .status()
            .unwrap();
        assert!(converted.success());
        let value = fs::read(&xml).unwrap();
        let name = c"com.apple.LaunchServices.OpenWith";
        let path = std::ffi::CString::new(file.to_str().unwrap()).unwrap();
        // SAFETY: the name and the path are terminated strings, and the value
        // is as long as the length given.
        let set = unsafe {
            libc::setxattr(
                path.as_ptr(),
                name.as_ptr(),
                value.as_ptr().cast(),
                value.len(),
                0,
                0,
            )
        };
        assert_eq!(set, 0);
        if Path::new("/System/Library/CoreServices/JavaLauncher.app").exists() {
            assert_eq!(kind(file.to_str().unwrap()).ok(), Some(TargetKind::Program));
        }
        fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn terminals_and_the_runners_run_their_documents() {
        let bundle = |path: &str| {
            NSBundle::bundleWithURL(&NSURL::fileURLWithPath(&NSString::from_str(path)))
        };
        for (path, runs) in [
            ("/System/Applications/Utilities/Terminal.app", true),
            ("/System/Library/CoreServices/Installer.app", true),
            ("/System/Applications/TextEdit.app", false),
            ("/System/Applications/Preview.app", false),
            ("/System/Library/CoreServices/Finder.app", false),
        ] {
            if let Some(bundle) = bundle(path) {
                assert_eq!(runs_documents(&bundle), runs, "{path}");
            }
        }
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
