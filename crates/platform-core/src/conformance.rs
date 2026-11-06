//! What every [`Platform`] tells alike, so that the app's decisions, which
//! take the platform's facts, hold on each: the targets that open, what
//! they are and where they lead, and the windows that are not there. A
//! backend's tests run [`check`] on it, as the fake's do, and fail with the
//! checks that it fails.
//!
//! The checks open nothing and show nothing: a program that would open,
//! or a URL that would show in the file manager, fails before the system
//! is asked.

use std::fs;
use std::path::{Path, PathBuf};

use crate::{
    Layer, Platform, Point, PointerShape, ScreenPoint, Size, Target, TargetKind, WindowId,
};

/// Runs every check on `platform`, with the files that it makes in
/// `directory`, which it empties first and removes after; returns what
/// failed, each a sentence, empty when everything passed.
pub fn check(platform: &mut dyn Platform, directory: &Path) -> Vec<String> {
    let mut failed = Vec::new();
    match targets(platform, directory) {
        Ok(found) => failed.extend(found),
        Err(error) => failed.push(format!("the files of the checks: {error}")),
    }
    let _ = fs::remove_dir_all(directory);
    failed.extend(absent_window(platform));
    failed.extend(no_drag_image(platform));
    failed
}

/// The files of the targets' checks, in `directory`.
struct Files {
    directory: PathBuf,
    notes: PathBuf,
    spaced: PathBuf,
    #[cfg(unix)]
    program: PathBuf,
}

/// Makes `directory` afresh, with a text file, one whose name has a
/// space, and a program by its mode; with links to them, and one that
/// leads nowhere.
fn files(directory: &Path) -> std::io::Result<Files> {
    let _ = fs::remove_dir_all(directory);
    fs::create_dir_all(directory.join("folder"))?;
    let directory = fs::canonicalize(directory)?;
    let files = Files {
        notes: directory.join("notes.txt"),
        spaced: directory.join("my notes.txt"),
        #[cfg(unix)]
        program: directory.join("program"),
        directory,
    };
    fs::write(&files.notes, "notes")?;
    fs::write(&files.spaced, "notes")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{PermissionsExt, symlink};

        fs::write(&files.program, "#!/bin/sh\n")?;
        fs::set_permissions(&files.program, fs::Permissions::from_mode(0o755))?;
        symlink(&files.program, files.directory.join("program-link.txt"))?;
        symlink(&files.notes, files.directory.join("notes-link"))?;
        symlink(
            files.directory.join("none"),
            files.directory.join("nowhere"),
        )?;
    }
    // What macOS runs by its extension or its bundle, of no mode.
    #[cfg(target_os = "macos")]
    {
        fs::create_dir_all(files.directory.join("Tool.app/Contents/MacOS"))?;
        fs::write(files.directory.join("run.command"), "echo ran\n")?;
        fs::write(files.directory.join("site.webloc"), "")?;
    }
    Ok(files)
}

/// The checks of the targets: what each is, and where it leads.
fn targets(platform: &mut dyn Platform, directory: &Path) -> std::io::Result<Vec<String>> {
    let files = files(directory)?;
    let mut failed = Vec::new();
    let text = |path: &Path| path.display().to_string();
    let mut expect = |target: String, kind: TargetKind, leads: Option<&Path>| match platform
        .resolve_target(&target)
    {
        Ok(found) if found.kind != kind => failed.push(format!(
            "`{target}` is a {:?}, where it is a {kind:?}",
            found.kind
        )),
        Ok(found) => {
            let location = match leads {
                Some(path) => fs::canonicalize(&found.location).ok() == Some(path.to_owned()),
                None => found.location == target,
            };
            if !location {
                failed.push(format!(
                    "`{target}` leads to `{}`, where it leads to `{}`",
                    found.location,
                    leads.map_or(target.clone(), text)
                ));
            }
        }
        Err(error) => failed.push(format!("`{target}` is a {kind:?}, not an error: {error}")),
    };
    // A URL of another scheme than `file` is itself.
    for url in ["https://lokyr.dev/x?a=1#b", "mailto:me@lokyr.dev"] {
        expect(url.to_owned(), TargetKind::Url, None);
    }
    // A directory and files, by their paths and their `file` URLs, of
    // this machine.
    let directory = &files.directory;
    expect(text(directory), TargetKind::Directory, Some(directory));
    let folder = directory.join("folder");
    expect(text(&folder), TargetKind::Directory, Some(&folder));
    expect(text(&files.notes), TargetKind::File, Some(&files.notes));
    let escaped = text(&files.spaced).replace(' ', "%20");
    expect(
        format!("file://{escaped}"),
        TargetKind::File,
        Some(&files.spaced),
    );
    expect(
        format!("file://localhost{escaped}"),
        TargetKind::File,
        Some(&files.spaced),
    );
    // A program, by its mode; a link leads to what it links to, whose kind
    // it takes.
    #[cfg(unix)]
    {
        expect(
            text(&files.program),
            TargetKind::Program,
            Some(&files.program),
        );
        let link = directory.join("program-link.txt");
        expect(text(&link), TargetKind::Program, Some(&files.program));
        expect(
            format!("file://{}", text(&files.program)),
            TargetKind::Program,
            Some(&files.program),
        );
        expect(
            text(&directory.join("notes-link")),
            TargetKind::File,
            Some(&files.notes),
        );
    }
    // An application's bundle, and files that the system runs by their
    // extensions.
    #[cfg(target_os = "macos")]
    for name in ["Tool.app", "run.command", "site.webloc"] {
        let path = directory.join(name);
        expect(text(&path), TargetKind::Program, Some(&path));
    }
    // What leads nowhere, is elsewhere, has no path, or is no URL, fails.
    let mut refused = vec![
        text(&directory.join("none")),
        format!("file://elsewhere.example{}", text(&files.notes)),
        "file://localhost".to_owned(),
        "file://localhost?x".to_owned(),
        "file://localhost#x".to_owned(),
        "file:".to_owned(),
        "not a url".to_owned(),
        "relative/path".to_owned(),
    ];
    #[cfg(unix)]
    refused.push(text(&directory.join("nowhere")));
    for target in refused {
        if let Ok(found) = platform.resolve_target(&target) {
            failed.push(format!("`{target}` is a {:?}, not an error", found.kind));
        }
    }
    // A program does not open, nor does a URL show in the file manager.
    #[cfg(unix)]
    {
        let program = Target {
            kind: TargetKind::Program,
            location: text(&files.program),
        };
        if platform.open(&program).is_ok() {
            failed.push("a program opens".to_owned());
        }
    }
    for kind in [TargetKind::File, TargetKind::Directory] {
        let empty = Target {
            kind,
            location: String::new(),
        };
        if platform.open(&empty).is_ok() || platform.reveal(&empty).is_ok() {
            failed.push(format!("a {kind:?} of no path opens or shows"));
        }
    }
    let url = Target {
        kind: TargetKind::Url,
        location: "https://lokyr.dev".to_owned(),
    };
    if platform.reveal(&url).is_ok() {
        failed.push("a URL shows in the file manager".to_owned());
    }
    Ok(failed)
}

/// The checks of a window that never was, which takes nothing and gives
/// nothing.
fn absent_window(platform: &mut dyn Platform) -> Vec<String> {
    let window = WindowId::new(u64::MAX);
    let mut failed = Vec::new();
    platform.set_title(window, "none");
    platform.request_frame(window);
    platform.set_text_area(window, Point { x: 0.0, y: 0.0 }, Size::default());
    platform.set_pointer_shape(window, PointerShape::Text);
    platform.track_mouse_moves(window, true);
    platform.set_pointer_areas(window, &[]);
    platform.place_layer(window, Layer::Cursor, None);
    platform.set_translucent(window, None);
    platform.set_window_appearance(window, None);
    platform.set_window_remember(window, false);
    platform.set_window_origin(window, ScreenPoint { x: 10.0, y: 10.0 });
    platform.set_fills(window, &[]);
    platform.close_window(window);
    if platform.window_size(window).is_some() {
        failed.push("a window that never was has a size".to_owned());
    }
    if platform.refresh_interval(window).is_some() {
        failed.push("a window that never was has a refresh interval".to_owned());
    }
    if platform.pointer_position(window).is_some() {
        failed.push("a window that never was has a pointer".to_owned());
    }
    if platform.window_handle(window).is_some() {
        failed.push("a window that never was has a handle".to_owned());
    }
    if platform.layer_handle(window, Layer::Cursor).is_some() {
        failed.push("a window that never was has a layer".to_owned());
    }
    if platform.focused_window() == Some(window) {
        failed.push("a window that never was has the focus".to_owned());
    }
    if platform.window_frame(window).is_some() {
        failed.push("a window that never was has a frame".to_owned());
    }
    let off = ScreenPoint {
        x: -1.0e7,
        y: -1.0e7,
    };
    if platform.window_at(off).is_some() {
        failed.push("a point off every screen shows a window".to_owned());
    }
    failed
}

/// The checks of the drag's image before it shows, and once it hid: no
/// handle, and a hide that does nothing.
fn no_drag_image(platform: &mut dyn Platform) -> Vec<String> {
    let mut failed = Vec::new();
    if platform.drag_image_handle().is_some() {
        failed.push("the drag's image has a handle before it shows".to_owned());
    }
    platform.hide_drag_image(None);
    if platform.drag_image_handle().is_some() {
        failed.push("the drag's image has a handle once it hid".to_owned());
    }
    failed
}
