//! What URLs and paths are, and opening them with the system's handlers,
//! through `NSWorkspace`: a path's symbolic links and the Finder's aliases
//! are resolved first, so that what the app decides by is what opens; a
//! file that would run rather than open does not open.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use block2::RcBlock;
use lokyr_platform_core::{Error, Target, TargetKind, Waker, runs_by_itself};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{NSRunningApplication, NSWorkspace, NSWorkspaceOpenConfiguration};
use objc2_foundation::{
    NSArray, NSBundle, NSDictionary, NSError, NSString, NSURL, NSURLBookmarkResolutionOptions,
};
use objc2_uniform_type_identifiers::UTTypePlainText;

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

/// The opens that failed once the system took them on, which the main
/// thread takes as a waker wakes it, as the system tells their end on a
/// thread of its own: each one's target, and what went wrong.
static FAILED: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

/// Takes the opens that failed since the last take.
pub(crate) fn take_failed() -> Vec<(String, String)> {
    FAILED
        .lock()
        .map(|mut failed| std::mem::take(&mut *failed))
        .unwrap_or_default()
}

/// How many aliases a path can lead through, each to the next.
const ALIASES: usize = 8;

/// What `target`, a URL or an absolute path, is, and where it leads, as
/// [`Platform::resolve_target`](lokyr_platform_core::Platform::resolve_target)
/// tells.
pub(crate) fn resolve(target: &str) -> Result<Target, Error> {
    let workspace = NSWorkspace::sharedWorkspace();
    let failed = |why: &str| Error::Open(target.to_owned(), why.to_owned());
    let path = if target.starts_with('/') {
        PathBuf::from(target)
    } else {
        // Foundation takes a text without a scheme as a relative URL.
        let url = NSURL::URLWithString(&NSString::from_str(target))
            .filter(|url| url.scheme().is_some())
            .ok_or_else(|| failed("it is not a URL"))?;
        if !url.isFileURL() {
            return Ok(Target {
                kind: TargetKind::Url,
                location: target.to_owned(),
            });
        }
        if let Some(host) = url.host()
            && !is_local(&host.to_string(), &host_name())
        {
            return Err(failed("it is on another machine"));
        }
        // A URL of this machine without a path, as `file://localhost`, has
        // an empty one.
        let path = url
            .path()
            .map(|path| path.to_string())
            .filter(|path| !path.is_empty())
            .ok_or_else(|| failed("it has no path"))?;
        PathBuf::from(path)
    };
    let path = resolved(&path).map_err(|why| failed(&why))?;
    let metadata = fs::metadata(&path).map_err(|error| failed(&error.to_string()))?;
    let file = file_url(&path).ok_or_else(|| failed("it has no path"))?;
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
    Ok(Target {
        kind,
        location: path.display().to_string(),
    })
}

/// `path` with its symbolic links and the Finder's aliases resolved, each
/// in turn, as the system resolves them as it opens a file: Foundation's
/// resolution of an alias follows the links too.
fn resolved(path: &Path) -> Result<PathBuf, String> {
    let mut path = path.to_path_buf();
    for _ in 0..ALIASES {
        let options = NSURLBookmarkResolutionOptions::WithoutUI
            | NSURLBookmarkResolutionOptions::WithoutMounting;
        let file = file_url(&path).ok_or("its path is not absolute")?;
        let target =
            NSURL::URLByResolvingAliasFileAtURL_options_error(&file, options).map_err(|error| {
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

/// The `file` URL of `path`, an absolute one; none of another, as
/// Foundation gives none of an empty path, which would panic.
fn file_url(path: &Path) -> Option<Retained<NSURL>> {
    path.is_absolute()
        .then(|| NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy())))
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

/// The URL that opens `target`: its own, or its path's.
fn url_of(target: &Target) -> Result<Retained<NSURL>, Error> {
    if target.kind != TargetKind::Url {
        return file_url(Path::new(&target.location))
            .ok_or_else(|| Error::Open(target.location.clone(), "it has no path".to_owned()));
    }
    NSURL::URLWithString(&NSString::from_str(&target.location))
        .ok_or_else(|| Error::Open(target.location.clone(), "it is not a URL".to_owned()))
}

/// Opens `target`, as [`Platform::open`](lokyr_platform_core::Platform::open)
/// does: at its location, which a program's fails before the system is
/// asked.
pub(crate) fn open(target: &Target) -> Result<(), Error> {
    if target.kind == TargetKind::Program {
        return Err(Error::Open(
            target.location.clone(),
            "it would run".to_owned(),
        ));
    }
    let url = url_of(target)?;
    if NSWorkspace::sharedWorkspace().openURL(&url) {
        Ok(())
    } else {
        Err(Error::Open(
            target.location.clone(),
            "no application opens it".to_owned(),
        ))
    }
}

/// Shows `target` in the Finder, as
/// [`Platform::reveal`](lokyr_platform_core::Platform::reveal) does: at its
/// location; a URL's fails.
pub(crate) fn reveal(target: &Target) -> Result<(), Error> {
    if target.kind == TargetKind::Url {
        return Err(Error::Open(
            target.location.clone(),
            "it is no file".to_owned(),
        ));
    }
    let file = file_url(Path::new(&target.location))
        .ok_or_else(|| Error::Open(target.location.clone(), "it has no path".to_owned()))?;
    NSWorkspace::sharedWorkspace()
        .activateFileViewerSelectingURLs(&NSArray::from_retained_slice(&[file]));
    Ok(())
}

/// Opens the file `path` in the application that opens plain text, as
/// TextEdit or the editor that the user chose for it, whatever the file's
/// own type opens in, as a `.toml` that no application may claim. What
/// fails after, as an editor that does not start, `waker` tells the main
/// thread, which takes it with [`take_failed`].
pub(crate) fn in_text_editor(path: &Path, waker: Waker) -> Result<(), Error> {
    let failed = |why: &str| Error::Open(path.display().to_string(), why.to_owned());
    let workspace = NSWorkspace::sharedWorkspace();
    // SAFETY: the framework's constant, a type that it keeps.
    let text = unsafe { UTTypePlainText };
    let editor = workspace
        .URLForApplicationToOpenContentType(text)
        .ok_or_else(|| failed("no application opens text"))?;
    let file = NSURL::fileURLWithPath(&NSString::from_str(
        path.to_str()
            .ok_or_else(|| failed("its path is not UTF-8"))?,
    ));
    open_with(&workspace, (path, &file), &editor, waker);
    Ok(())
}

/// Opens `file`, of `path`, in the application `editor`, and keeps what
/// fails once the system took the open on, which `waker` tells.
fn open_with(workspace: &NSWorkspace, (path, file): (&Path, &NSURL), editor: &NSURL, waker: Waker) {
    let target = path.display().to_string();
    let completion = RcBlock::new(move |_: *mut NSRunningApplication, error: *mut NSError| {
        // SAFETY: the system gives an error that lives through the call, or
        // none.
        let Some(error) = (unsafe { error.as_ref() }) else {
            return;
        };
        let why = error.localizedDescription().to_string();
        let error = Error::Open(target.clone(), why).to_string();
        if let Ok(mut failed) = FAILED.lock() {
            failed.push((target.clone(), error));
            waker.wake();
        }
    });
    workspace.openURLs_withApplicationAtURL_configuration_completionHandler(
        &NSArray::from_slice(&[file]),
        editor,
        &NSWorkspaceOpenConfiguration::configuration(),
        Some(&completion),
    );
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

    /// What `target` is, as [`resolve`] tells.
    fn kind(target: &str) -> Result<TargetKind, Error> {
        resolve(target).map(|target| target.kind)
    }

    /// A directory of the test's own, made afresh.
    fn directory(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!("lokyr-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn an_editor_that_does_not_open_tells_the_main_thread_after() {
        let directory = directory("editor-fails");
        let path = directory.join("config.toml");
        fs::write(&path, "").unwrap();
        let file = NSURL::fileURLWithPath(&NSString::from_str(path.to_str().unwrap()));
        let gone = NSURL::fileURLWithPath(&NSString::from_str(
            directory.join("Gone.app").to_str().unwrap(),
        ));
        let woken = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let waker = {
            let woken = std::sync::Arc::clone(&woken);
            Waker::new(move || woken.store(true, std::sync::atomic::Ordering::Release))
        };
        let workspace = NSWorkspace::sharedWorkspace();
        open_with(&workspace, (&path, &file), &gone, waker);
        // LaunchServices answers within seconds, and later under the load
        // of the other tests.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let mut failed = Vec::new();
        while failed.is_empty() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
            failed = take_failed();
        }
        let _ = fs::remove_dir_all(&directory);
        assert!(woken.load(std::sync::atomic::Ordering::Acquire));
        let target = path.display().to_string();
        assert!(
            matches!(failed.as_slice(), [(of, error)] if *of == target && error.starts_with(&format!("{target} could not be opened: "))),
            "{failed:?}"
        );
    }

    /// Makes `alias` a Finder's alias of `original`.
    fn make_alias(original: &Path, alias: &Path) {
        let bookmark = file_url(original)
            .unwrap()
            .bookmarkDataWithOptions_includingResourceValuesForKeys_relativeToURL_error(
                objc2_foundation::NSURLBookmarkCreationOptions::SuitableForBookmarkFile,
                None,
                None,
            )
            .unwrap();
        NSURL::writeBookmarkData_toURL_options_error(
            &bookmark,
            &file_url(alias).unwrap(),
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
    fn a_file_of_an_extension_that_runs_is_a_program_and_an_alias_leads_to_its_original() {
        let directory = directory("kind");
        let (notes, program) = (directory.join("a.txt"), directory.join("b.command"));
        fs::write(&notes, "").unwrap();
        fs::write(&program, "").unwrap();
        assert_eq!(
            kind(program.to_str().unwrap()).ok(),
            Some(TargetKind::Program)
        );
        make_alias(&notes, &directory.join("alias"));
        let original = resolve(directory.join("alias").to_str().unwrap()).unwrap();
        assert_eq!(original.kind, TargetKind::File);
        assert_eq!(
            fs::canonicalize(&original.location).unwrap(),
            fs::canonicalize(&notes).unwrap()
        );
        // A program's target does not open.
        assert!(open(&resolve(program.to_str().unwrap()).unwrap()).is_err());
        fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn a_file_url_without_a_path_resolves_to_nothing_and_does_not_panic() {
        let host = host_name();
        for target in [
            "file://localhost".to_owned(),
            "file://localhost?x".to_owned(),
            "file://localhost#x".to_owned(),
            format!("file://{host}"),
            "file:".to_owned(),
        ] {
            assert!(resolve(&target).is_err(), "{target}");
        }
        // A target of no path, whoever made it, opens nothing.
        let empty = |kind| Target {
            kind,
            location: String::new(),
        };
        assert!(open(&empty(TargetKind::File)).is_err());
        assert!(reveal(&empty(TargetKind::Directory)).is_err());
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
