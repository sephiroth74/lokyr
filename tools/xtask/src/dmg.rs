//! `cargo xtask dmg`: builds `target/Lokyr-<version>.dmg`, the disk image
//! that the app is downloaded as, of `target/Lokyr.app`, which it has Apple
//! notarize first, as `cargo xtask notarize` does, unless the bundle has
//! its ticket stapled already: a window of the
//! app and of a link to Applications, over a background, the app dragged
//! onto the link to install it. The image is signed with the bundle's
//! identity, the Developer ID that `LOKYR_SIGN_IDENTITY` names, in the
//! environment or in the untracked `private/lokyr.env`, notarized
//! and stapled in its turn, so that Gatekeeper
//! opens it as a download without its warning. `--no-notarize` builds it of
//! the bundle as it is, without Apple, to look at it.
//!
//! The window's look is the Finder's own `.DS_Store`, which `cargo xtask
//! dmg layout` has the Finder write once, on an image of the same volume's
//! name, and keeps as `crates/app/resources/dmg/DS_Store`: the window's
//! place and size, the icons' size and places, and its background,
//! `.background/background.tiff`, which `layout` renders from
//! `background.svg` at one and two pixels a point, with `rsvg-convert`. The
//! Finder finds the background by its path on the volume, so a new
//! background needs no new layout; a new size or new places do.

use std::fs;
use std::path::Path;
use std::process::{Command, ExitCode};
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};

use crate::bundle;
use crate::notarize::{self, status};

/// The image's resources, relative to the workspace root.
const RESOURCES: &str = "crates/app/resources/dmg";

/// The background's source, in [`RESOURCES`].
const BACKGROUND_SOURCE: &str = "background.svg";

/// The background, of one and two pixels a point, in [`RESOURCES`] and in
/// the volume's `.background`.
const BACKGROUND: &str = "background.tiff";

/// The Finder's `.DS_Store` of the volume, in [`RESOURCES`].
const LAYOUT: &str = "DS_Store";

/// The size of the window's content, in points, which the background fills.
const WINDOW: (u32, u32) = (660, 420);

/// The height of a window's title bar, in points, on macOS 26 and later:
/// the Finder's bounds of a window hold it.
const TITLE_BAR: u32 = 32;

/// The window's top left corner on the screen, in points.
const WINDOW_PLACE: (u32, u32) = (200, 120);

/// The icons' size, in points.
const ICON_SIZE: u32 = 128;

/// The center of the app's icon in the window's content, in points, at the
/// start of the background's arrow.
const APP_PLACE: (u32, u32) = (170, 200);

/// The center of the link to Applications, at the arrow's end.
const APPLICATIONS_PLACE: (u32, u32) = (490, 200);

/// The link to the folder of the apps.
const APPLICATIONS: &str = "Applications";

/// The start of a `.DS_Store`, of its allocator: macOS drops, without a
/// word, a file of that name without it from an image that it makes or
/// mounts.
const STORE_MAGIC: &[u8; 8] = b"\0\0\0\x01Bud1";

/// The image's format: read only, compressed with LZFSE.
const FORMAT: &str = "ULFO";

/// The Finder's information of the volume's root, 32 bytes in hex, with the
/// flag of a custom icon, `kHasCustomIcon`, in the Finder's flags at the
/// eighth byte: the Finder shows the volume with its `.VolumeIcon.icns`.
const CUSTOM_ICON: &str = "0000000000000000040000000000000000000000000000000000000000000000";

pub fn run(mut args: impl Iterator<Item = String>) -> Result<ExitCode> {
    let (layout, notarized) = match args.next().as_deref() {
        None => (false, true),
        Some("--no-notarize") => (false, false),
        Some("layout") => (true, false),
        Some(arg) => bail!("dmg takes `layout` or `--no-notarize`, not `{arg}`"),
    };
    if let Some(arg) = args.next() {
        bail!("dmg takes `layout` or `--no-notarize` alone, not `{arg}`");
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .context("the workspace's root")?;
    let app = root.join("target").join(format!("{}.app", bundle::NAME));
    if !app.exists() {
        bail!("{} is not there: `just bundle` builds it", app.display());
    }
    let scratch = root.join("target/dmg");
    if scratch.exists() {
        fs::remove_dir_all(&scratch).with_context(|| format!("removing {}", scratch.display()))?;
    }
    fs::create_dir_all(&scratch).with_context(|| format!("making {}", scratch.display()))?;
    let done = if layout {
        lay_out(&root, &app, &scratch)
    } else {
        build(&root, &app, &scratch, notarized)
    };
    if done.is_ok() {
        let _ = fs::remove_dir_all(&scratch);
    }
    done.map(|()| ExitCode::SUCCESS)
}

/// Builds the image of `app`, signed, notarized and stapled if `notarized`,
/// through the folder `scratch`.
fn build(root: &Path, app: &Path, scratch: &Path, notarized: bool) -> Result<()> {
    let resources = root.join(RESOURCES);
    let layout = resources.join(LAYOUT);
    let store = fs::read(&layout).with_context(|| {
        format!(
            "reading {}, which `cargo xtask dmg layout` has the Finder write",
            layout.display()
        )
    })?;
    ensure!(
        laid_out(&store),
        "{} is not the Finder's layout of the window: `cargo xtask dmg layout` has the Finder write it",
        layout.display()
    );
    if notarized {
        notarize::ensure_developer_id(app)?;
        let stapled = Command::new("xcrun")
            .args(["stapler", "validate", "-q"])
            .arg(app)
            .status()
            .is_ok_and(|status| status.success());
        if !stapled {
            notarize::notarize_app(app)?;
        }
    }
    let version = bundle::told_version(&app.join("Contents/MacOS").join(bundle::EXECUTABLE))?;
    let dmg = root
        .join("target")
        .join(format!("{}-{version}.dmg", bundle::NAME));
    let staging = scratch.join("staging");
    stage(app, &resources.join(BACKGROUND), Some(&layout), &staging)?;
    image(&staging, scratch, &dmg)?;
    let identity = bundle::identity()?;
    if identity == "-" {
        ensure!(
            !notarized,
            "LOKYR_SIGN_IDENTITY names no Developer ID, which notarization needs"
        );
        println!("dmg: LOKYR_SIGN_IDENTITY names no identity: the image is not signed");
    } else {
        status(
            Command::new("codesign")
                .args(["--force", "--timestamp", "--sign", &identity])
                .arg(&dmg),
            "signing the image",
        )?;
        status(
            Command::new("codesign")
                .args(["--verify", "--strict", "--verbose=2"])
                .arg(&dmg),
            "checking the image's signature",
        )?;
    }
    attached(&dmg, scratch, false, |volume| {
        ensure!(
            fs::read(volume.join(".DS_Store"))? == store,
            "the image's .DS_Store is not the Finder's layout"
        );
        let app = volume.join(format!("{}.app", bundle::NAME));
        status(
            Command::new("codesign")
                .args(["--verify", "--deep", "--strict"])
                .arg(&app),
            "checking the signature of the image's app",
        )?;
        if notarized {
            status(
                Command::new("xcrun")
                    .args(["stapler", "validate"])
                    .arg(&app),
                "checking the ticket of the image's app",
            )?;
        }
        Ok(())
    })?;
    if notarized {
        notarize::submit(&dmg)?;
        notarize::staple(&dmg)?;
        status(
            Command::new("spctl")
                .args([
                    "--assess",
                    "--type",
                    "open",
                    "--context",
                    "context:primary-signature",
                    "--verbose=2",
                ])
                .arg(&dmg),
            "assessing the image as Gatekeeper does",
        )?;
    }
    println!(
        "dmg: {} built, of {}, {}",
        dmg.display(),
        bundle::megabytes(bundle::size_of(&dmg)),
        if notarized {
            "notarized, its ticket stapled"
        } else {
            "not notarized"
        }
    );
    Ok(())
}

/// Has the Finder lay out the window of an image of `app`, through the
/// folder `scratch`, and keeps its `.DS_Store` in the resources, with the
/// background rendered anew.
fn lay_out(root: &Path, app: &Path, scratch: &Path) -> Result<()> {
    let resources = root.join(RESOURCES);
    let background = resources.join(BACKGROUND);
    render_background(&resources.join(BACKGROUND_SOURCE), scratch, &background)?;
    // The Finder tells the volume by its name, the downloaded image's.
    let volume = Path::new("/Volumes").join(bundle::NAME);
    ensure!(
        !volume.exists(),
        "{} is mounted: eject it, since the Finder would lay out the one that it finds first",
        volume.display()
    );
    let staging = scratch.join("staging");
    stage(app, &background, None, &staging)?;
    let writable = scratch.join("layout.dmg");
    create(&staging, &writable)?;
    status(
        Command::new("hdiutil")
            .args(["attach", "-quiet", "-noautoopen", "-readwrite"])
            .arg(&writable),
        "attaching the image for the Finder",
    )?;
    let written = if volume.exists() {
        finder_layout(&volume, scratch)
    } else {
        Err(anyhow::anyhow!(
            "the image is not at {}, where the Finder looks for it",
            volume.display()
        ))
    };
    let detached = detach(&volume);
    let written = written?;
    detached?;
    let layout = resources.join(LAYOUT);
    fs::write(&layout, written).with_context(|| format!("writing {}", layout.display()))?;
    println!(
        "dmg: the Finder laid out the window, in {}, over {}",
        layout.display(),
        background.display()
    );
    Ok(())
}

/// Renders the background `svg` at one and two pixels a point into the
/// TIFF `tiff`, which holds both, as the Finder takes them, through the
/// folder `scratch`.
fn render_background(svg: &Path, scratch: &Path, tiff: &Path) -> Result<()> {
    let (width, height) = WINDOW;
    let mut renderings = Vec::new();
    for (scale, name) in [(1, "background.png"), (2, "background@2x.png")] {
        let png = scratch.join(name);
        status(
            Command::new("rsvg-convert")
                .arg(format!("--width={}", width * scale))
                .arg(format!("--height={}", height * scale))
                .arg("--output")
                .arg(&png)
                .arg(svg),
            "rendering the background with rsvg-convert, of librsvg",
        )?;
        renderings.push(png);
    }
    status(
        Command::new("tiffutil")
            .arg("-cathidpicheck")
            .args(&renderings)
            .arg("-out")
            .arg(tiff),
        "joining the background's renderings into a TIFF",
    )
}

/// Lays out in `staging` the volume's files: a copy of `app`, the link to
/// Applications, the background, the app's icon as the volume's, and the
/// Finder's `layout`, if any.
fn stage(app: &Path, background: &Path, layout: Option<&Path>, staging: &Path) -> Result<()> {
    let hidden = staging.join(".background");
    fs::create_dir_all(&hidden).with_context(|| format!("making {}", hidden.display()))?;
    let name = app.file_name().context("the bundle's name")?;
    status(
        Command::new("ditto").arg(app).arg(staging.join(name)),
        "copying the bundle",
    )?;
    std::os::unix::fs::symlink("/Applications", staging.join(APPLICATIONS))
        .context("linking to Applications")?;
    fs::copy(background, hidden.join(BACKGROUND))
        .with_context(|| format!("copying {}", background.display()))?;
    let icon = app.join("Contents/Resources/AppIcon.icns");
    fs::copy(&icon, staging.join(".VolumeIcon.icns"))
        .with_context(|| format!("copying {}", icon.display()))?;
    if let Some(layout) = layout {
        fs::copy(layout, staging.join(".DS_Store"))
            .with_context(|| format!("copying {}", layout.display()))?;
    }
    Ok(())
}

/// Makes a writable image, `writable`, of the files of `staging`, as the
/// volume [`bundle::NAME`].
fn create(staging: &Path, writable: &Path) -> Result<()> {
    status(
        Command::new("hdiutil")
            .args(["create", "-quiet", "-ov", "-srcfolder"])
            .arg(staging)
            .args(["-volname", bundle::NAME, "-fs", "HFS+", "-format", "UDRW"])
            .arg(writable),
        "making a writable image",
    )
}

/// Builds the image `dmg` of the files of `staging`, with the volume's own
/// icon, through a writable image in `scratch`.
fn image(staging: &Path, scratch: &Path, dmg: &Path) -> Result<()> {
    let writable = scratch.join("writable.dmg");
    create(staging, &writable)?;
    // The flag belongs to the volume's root, which the source's folder does
    // not give it.
    attached(&writable, scratch, true, |volume| {
        status(
            Command::new("xattr")
                .args(["-wx", "com.apple.FinderInfo", CUSTOM_ICON])
                .arg(volume),
            "giving the volume its icon",
        )
    })?;
    if dmg.exists() {
        fs::remove_file(dmg).with_context(|| format!("removing {}", dmg.display()))?;
    }
    status(
        Command::new("hdiutil")
            .args(["convert", "-quiet"])
            .arg(&writable)
            .args(["-format", FORMAT, "-o"])
            .arg(dmg),
        "compressing the image",
    )?;
    fs::remove_file(&writable).with_context(|| format!("removing {}", writable.display()))
}

/// Attaches the image `dmg` at a mount point in `scratch`, hidden from the
/// Finder, writable if `writable`, runs `with` on its volume, and detaches
/// it, even after `with` fails.
fn attached(
    dmg: &Path,
    scratch: &Path,
    writable: bool,
    with: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    let mount = scratch.join("mount");
    fs::create_dir_all(&mount).with_context(|| format!("making {}", mount.display()))?;
    status(
        Command::new("hdiutil")
            .args(["attach", "-quiet", "-nobrowse", "-noautoopen"])
            .arg(if writable { "-readwrite" } else { "-readonly" })
            .arg("-mountpoint")
            .arg(&mount)
            .arg(dmg),
        "attaching the image",
    )?;
    let done = with(&mount);
    let detached = detach(&mount);
    done.and(detached)
}

/// Detaches the volume at `mount`, by force once the Finder or Spotlight
/// has kept it busy for a few seconds.
fn detach(mount: &Path) -> Result<()> {
    for _ in 0..5 {
        let detached = Command::new("hdiutil")
            .args(["detach", "-quiet"])
            .arg(mount)
            .status();
        if detached.is_ok_and(|status| status.success()) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    status(
        Command::new("hdiutil")
            .args(["detach", "-quiet", "-force"])
            .arg(mount),
        "detaching the image",
    )
}

/// Has the Finder lay out the window of the volume at `volume`, through a
/// script in `scratch`, and gives the `.DS_Store` that it writes.
fn finder_layout(volume: &Path, scratch: &Path) -> Result<Vec<u8>> {
    let script = scratch.join("layout.applescript");
    fs::write(&script, layout_script()).context("writing the Finder's script")?;
    status(
        Command::new("osascript").arg(&script),
        "having the Finder lay out the window, which needs the terminal's permission to control the Finder, in System Settings › Privacy & Security › Automation",
    )?;
    // The Finder writes it in its own time, once the window has closed.
    let store = volume.join(".DS_Store");
    for _ in 0..20 {
        if let Ok(bytes) = fs::read(&store)
            && laid_out(&bytes)
        {
            return Ok(bytes);
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    bail!(
        "the Finder did not write the icons' places in {}",
        store.display()
    )
}

/// The Finder's script that lays out the window of the volume: icons
/// without a toolbar, a status bar or a path bar, the window's size and
/// place, the icons' size and places, and the background.
fn layout_script() -> String {
    let volume = bundle::NAME;
    let (left, top) = WINDOW_PLACE;
    let (right, bottom) = (left + WINDOW.0, top + WINDOW.1 + TITLE_BAR);
    let (app_x, app_y) = APP_PLACE;
    let (link_x, link_y) = APPLICATIONS_PLACE;
    format!(
        r#"tell application "Finder"
    tell disk "{volume}"
        open
        set current view of container window to icon view
        set toolbar visible of container window to false
        set statusbar visible of container window to false
        try
            set pathbar visible of container window to false
        end try
        set the bounds of container window to {{{left}, {top}, {right}, {bottom}}}
        set viewOptions to the icon view options of container window
        set arrangement of viewOptions to not arranged
        set icon size of viewOptions to {ICON_SIZE}
        set background picture of viewOptions to file ".background:{BACKGROUND}"
        set position of item "{volume}.app" of container window to {{{app_x}, {app_y}}}
        set position of item "{APPLICATIONS}" of container window to {{{link_x}, {link_y}}}
        close
        open
        update without registering applications
        delay 2
        close
    end tell
end tell
"#
    )
}

/// Whether the bytes `store` are a `.DS_Store` of the Finder that holds the
/// window's options of its icons, of the background, the record `icvp` of
/// the volume's root, `.`, and the icons' places, the records `Iloc` of the
/// app and of the link, at [`APP_PLACE`] and [`APPLICATIONS_PLACE`].
fn laid_out(store: &[u8]) -> bool {
    store.starts_with(STORE_MAGIC)
        && value(store, ".", *b"icvp").is_some()
        && place(store, &format!("{}.app", bundle::NAME)) == Some(APP_PLACE)
        && place(store, APPLICATIONS) == Some(APPLICATIONS_PLACE)
}

/// The bytes of `store` after the start of the record `kind` of the file
/// `name`, of its value's type and its value: a record starts with its
/// file's name, its length in UTF-16's units and the units, big-endian,
/// then its kind.
fn value<'a>(store: &'a [u8], name: &str, kind: [u8; 4]) -> Option<&'a [u8]> {
    let units: Vec<u16> = name.encode_utf16().collect();
    let mut record = u32::try_from(units.len()).ok()?.to_be_bytes().to_vec();
    record.extend(units.iter().flat_map(|unit| unit.to_be_bytes()));
    record.extend_from_slice(&kind);
    let at = store
        .windows(record.len())
        .position(|bytes| bytes == record)?;
    store.get(at + record.len()..)
}

/// The center of the icon of the file `name` in the window, as `store`
/// holds it: its record `Iloc`, of a `blob` of 16 bytes that starts with
/// the center's two coordinates, big-endian.
fn place(store: &[u8], name: &str) -> Option<(u32, u32)> {
    let value = value(store, name, *b"Iloc")?;
    let word = |at: usize| Some(u32::from_be_bytes(value.get(at..at + 4)?.try_into().ok()?));
    (value.get(..8)? == b"blob\0\0\0\x10").then_some(())?;
    Some((word(8)?, word(12)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notarize::output;

    fn root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn the_background_is_of_the_window_s_size() {
        let svg = fs::read_to_string(root().join(RESOURCES).join(BACKGROUND_SOURCE)).unwrap();
        let (width, height) = WINDOW;
        assert!(svg.contains(&format!(
            r#"viewBox="0 0 {width} {height}" width="{width}" height="{height}""#
        )));
    }

    #[test]
    fn the_finder_s_script_places_the_window_and_the_icons() {
        let script = layout_script();
        // The bounds hold the title bar over the background's 420 points.
        assert!(script.contains("set the bounds of container window to {200, 120, 860, 572}"));
        assert!(script.contains("set icon size of viewOptions to 128"));
        // The icons' centers, at the arrow's height.
        assert!(
            script
                .contains(r#"set position of item "Lokyr.app" of container window to {170, 200}"#)
        );
        assert!(
            script.contains(
                r#"set position of item "Applications" of container window to {490, 200}"#
            )
        );
        assert!(script.contains(r#"file ".background:background.tiff""#));
    }

    /// A record of the `.DS_Store`: the name's length and units, its kind,
    /// and some bytes of its value.
    fn record(name: &str, kind: [u8; 4]) -> Vec<u8> {
        let units: Vec<u16> = name.encode_utf16().collect();
        let mut bytes = u32::try_from(units.len()).unwrap().to_be_bytes().to_vec();
        bytes.extend(units.iter().flat_map(|unit| unit.to_be_bytes()));
        bytes.extend_from_slice(&kind);
        bytes.extend_from_slice(b"blob\0\0\0\x10");
        bytes
    }

    /// The record of the place of the icon of `name`, its center at `x` and
    /// `y`.
    fn place_record(name: &str, (x, y): (u32, u32)) -> Vec<u8> {
        let mut bytes = record(name, *b"Iloc");
        bytes.extend(x.to_be_bytes());
        bytes.extend(y.to_be_bytes());
        bytes.extend([0xff; 8]);
        bytes
    }

    #[test]
    fn a_layout_holds_the_icons_at_their_places_and_the_window_s_options() {
        let whole = [
            STORE_MAGIC.to_vec(),
            record(".", *b"bwsp"),
            record(".", *b"icvp"),
            place_record("Applications", (490, 200)),
            place_record("Lokyr.app", (170, 200)),
        ]
        .concat();
        assert!(laid_out(&whole));
        assert_eq!(place(&whole, "Lokyr.app"), Some((170, 200)));
        // An icon elsewhere than its place, as a layout of other places.
        let elsewhere = [
            STORE_MAGIC.to_vec(),
            record(".", *b"icvp"),
            place_record("Applications", (490, 200)),
            place_record("Lokyr.app", (170, 245)),
        ]
        .concat();
        assert!(!laid_out(&elsewhere));
        // Without the allocator's start, as macOS drops it.
        assert!(!laid_out(&whole[STORE_MAGIC.len()..]));
        // Without the app's place, as before the Finder writes it.
        let without_app = [
            STORE_MAGIC.to_vec(),
            record(".", *b"icvp"),
            place_record("Applications", (490, 200)),
            record("Lokyr.app", *b"lsvp"),
        ]
        .concat();
        assert!(!laid_out(&without_app));
        // Without the options of its icons.
        let without_options = [
            STORE_MAGIC.to_vec(),
            record(".", *b"bwsp"),
            place_record("Applications", (490, 200)),
            place_record("Lokyr.app", (170, 200)),
        ]
        .concat();
        assert!(!laid_out(&without_options));
    }

    #[test]
    #[ignore = "builds and attaches disk images with hdiutil"]
    fn the_image_holds_the_app_the_link_the_background_the_layout_and_the_volume_s_icon() {
        let scratch = std::env::temp_dir().join(format!("lokyr-dmg-{}", std::process::id()));
        let _ = fs::remove_dir_all(&scratch);
        let app = scratch.join("Lokyr.app");
        let resources = app.join("Contents/Resources");
        fs::create_dir_all(&resources).unwrap();
        fs::copy(
            root().join("crates/app/resources/icons/lokyr-jormungandr.icns"),
            resources.join("AppIcon.icns"),
        )
        .unwrap();
        // macOS keeps a `.DS_Store` of the allocator's start alone.
        let layout = scratch.join("DS_Store");
        let store = [STORE_MAGIC.as_slice(), b"the Finder's"].concat();
        fs::write(&layout, &store).unwrap();
        let staging = scratch.join("staging");
        let background = root().join(RESOURCES).join(BACKGROUND);
        stage(&app, &background, Some(&layout), &staging).unwrap();
        let dmg = scratch.join("Lokyr.dmg");
        image(&staging, &scratch, &dmg).unwrap();
        assert!(!scratch.join("writable.dmg").exists());
        attached(&dmg, &scratch, false, |volume| {
            assert!(
                volume
                    .join("Lokyr.app/Contents/Resources/AppIcon.icns")
                    .is_file()
            );
            assert_eq!(
                fs::read_link(volume.join("Applications")).unwrap(),
                Path::new("/Applications")
            );
            assert_eq!(
                fs::read(volume.join(".background/background.tiff")).unwrap(),
                fs::read(&background).unwrap()
            );
            assert_eq!(fs::read(volume.join(".DS_Store")).unwrap(), store);
            assert!(volume.join(".VolumeIcon.icns").is_file());
            let info = output(
                Command::new("xattr")
                    .args(["-px", "com.apple.FinderInfo"])
                    .arg(volume),
                "reading the volume's Finder information",
            )
            .unwrap();
            let hex: String = info.split_whitespace().collect();
            assert_eq!(hex.to_lowercase(), CUSTOM_ICON);
            Ok(())
        })
        .unwrap();
        fs::remove_dir_all(&scratch).unwrap();
    }
}
