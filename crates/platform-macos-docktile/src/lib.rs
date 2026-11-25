//! The Dock tile plug-in of Lokyr's bundle, which the Dock loads while the
//! app is kept in it and not running, and which draws the icon's variant
//! that the app last showed rather than the bundle's own: the app writes
//! that variant's name in its defaults, under [`ICON_KEY`], and posts
//! [`ICON_CHANGED`] to the distributed notification center as it changes,
//! which the plug-in follows. The plug-in draws the variant's image of its
//! own app's bundle alone, `Contents/Resources/icons/lokyr-<name>.icns`, so
//! that the defaults, which any process of the user can write, name no
//! other file. It finds that bundle of its own executable's path, as the
//! loader tells it: `NSBundle`'s bundle of its class is the process's main
//! bundle, the Dock's, as the class is made at runtime.
//!
//! The Dock finds the plug-in's class by the bundle's `NSPrincipalClass`,
//! [`CLASS`], as `NSDockTilePlugIn` tells
//! (<https://developer.apple.com/documentation/appkit/nsdocktileplugin>):
//! the class is registered as the plug-in loads, before the Dock looks,
//! unless a class of its name is there already, as another Lokyr's bundle's
//! plug-in, loaded into the same process, registered it.

#![cfg(target_os = "macos")]

use std::cell::RefCell;
use std::ffi::{CStr, OsStr};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, NSObject, NSObjectProtocol};
use objc2::{AnyThread, ClassType, MainThreadMarker, MainThreadOnly, Message, define_class, sel};
use objc2_app_kit::{NSDockTile, NSDockTilePlugIn, NSImage, NSImageView};
use objc2_foundation::{
    NSDistributedNotificationCenter, NSNotification, NSObjectNSThreadPerformAdditions, NSPoint,
    NSRect, NSSize, NSString, NSUserDefaults,
};

/// The app's bundle identifier, whose defaults hold the icon's path.
pub const IDENTIFIER: &str = "io.github.sephiroth74.lokyr";

/// The key of the app's defaults that holds the name of the icon's variant
/// that the app shows, none of the bundle's own.
pub const ICON_KEY: &str = "DockTileIcon";

/// The distributed notification that the app posts as its icon changes.
pub const ICON_CHANGED: &str = "io.github.sephiroth74.lokyr.DockTileIconChanged";

/// The plug-in's class, as the bundle's `NSPrincipalClass` names it.
pub const CLASS: &str = "LokyrDockTilePlugIn";

/// [`CLASS`], as the runtime takes it.
const CLASS_NAME: &CStr = c"LokyrDockTilePlugIn";

thread_local! {
    /// The tile that the Dock gave, while it gives one: the Dock calls the
    /// plug-in on its main thread alone.
    static TILE: RefCell<Option<Retained<NSDockTile>>> = const { RefCell::new(None) };
}

define_class!(
    // SAFETY: NSObject's subclassing holds no requirement of its own; the
    // class adds no instance variables, the Dock making it with `init`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "LokyrDockTilePlugIn"]
    struct Plugin;

    unsafe impl NSObjectProtocol for Plugin {}

    unsafe impl NSDockTilePlugIn for Plugin {
        /// The tile to draw, or none as the app is taken out of the Dock:
        /// drawn of the icon's variant, and drawn anew as it changes.
        #[unsafe(method(setDockTile:))]
        fn set_dock_tile(&self, tile: Option<&NSDockTile>) {
            // The tile on the main thread alone, where its view lives: a
            // call from another thread comes again there.
            if MainThreadMarker::new().is_none() {
                let tile: Option<&AnyObject> = tile.map(AsRef::as_ref);
                // SAFETY: the selector is this method's own, of one object
                // or none, which the runtime keeps until the main thread
                // runs it.
                unsafe {
                    self.performSelectorOnMainThread_withObject_waitUntilDone(
                        sel!(setDockTile:),
                        tile,
                        false,
                    );
                }
                return;
            }
            let center = NSDistributedNotificationCenter::defaultCenter();
            match tile {
                Some(tile) => {
                    let first = TILE.with_borrow(Option::is_none);
                    TILE.set(Some(tile.retain()));
                    if first {
                        // SAFETY: the plug-in stays loaded while the Dock
                        // holds it, and removes itself as it lets the tile
                        // go; the selector is the class's own, of one
                        // notification argument.
                        unsafe {
                            center.addObserver_selector_name_object(
                                self,
                                sel!(iconChanged:),
                                Some(&NSString::from_str(ICON_CHANGED)),
                                None,
                            );
                        }
                    }
                    draw(tile);
                }
                None => {
                    TILE.set(None);
                    // SAFETY: the observer is this plug-in, added above.
                    unsafe { center.removeObserver(self) };
                }
            }
        }
    }

    impl Plugin {
        /// The app changed its icon: the tile draws it anew.
        #[unsafe(method(iconChanged:))]
        fn icon_changed(&self, _notification: &NSNotification) {
            TILE.with_borrow(|tile| {
                if let Some(tile) = tile {
                    draw(tile);
                }
            });
        }
    }
);

/// Draws `tile` of the variant that the app's defaults name, or of the
/// bundle's own icon where they name none, or one that its bundle lacks.
fn draw(tile: &NSDockTile) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let image = variant()
        .and_then(|name| icon_file(&name))
        .and_then(|path| {
            NSImage::initWithContentsOfFile(
                NSImage::alloc(),
                &NSString::from_str(&path.to_string_lossy()),
            )
            .filter(|image| image.isValid())
        });
    match image {
        Some(image) => {
            let size = tile.size();
            let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(size.width, size.height));
            let view = NSImageView::initWithFrame(NSImageView::alloc(mtm), frame);
            view.setImage(Some(&image));
            tile.setContentView(Some(&view));
        }
        None => tile.setContentView(None),
    }
    tile.display();
}

/// The name of the icon's variant, as the app's defaults hold it, read
/// anew each time, as the app writes them from its own process; none of
/// a name that is not a variant's.
fn variant() -> Option<String> {
    let defaults = NSUserDefaults::initWithSuiteName(
        NSUserDefaults::alloc(),
        Some(&NSString::from_str(IDENTIFIER)),
    )?;
    defaults.synchronize();
    let name = defaults.stringForKey(&NSString::from_str(ICON_KEY))?;
    Some(name.to_string()).filter(|name| is_variant(name))
}

/// Whether `name` can be a variant's: of lower case letters, digits and
/// hyphens alone, 64 at most, so that it names a file of the icons' folder
/// and no other.
fn is_variant(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// The image of the variant `name` in the plug-in's own app's bundle, if it
/// is there: the plug-in lies in `Contents/PlugIns`, the variants in
/// `Contents/Resources/icons`.
fn icon_file(name: &str) -> Option<PathBuf> {
    let image = own_image()?;
    let file = app_contents(&image)?
        .join("Resources/icons")
        .join(format!("lokyr-{name}.icns"));
    file.is_file().then_some(file)
}

/// The path of the plug-in's own executable, as the loader loaded it.
fn own_image() -> Option<PathBuf> {
    let address = own_image as fn() -> Option<PathBuf> as *const libc::c_void;
    // SAFETY: a `Dl_info` of no pointers set is a valid value, which
    // `dladdr` fills.
    let mut info: libc::Dl_info = unsafe { std::mem::zeroed() };
    // SAFETY: the address is of this image's code, which `dladdr` looks up
    // in the loader's tables.
    if unsafe { libc::dladdr(address, &mut info) } == 0 || info.dli_fname.is_null() {
        return None;
    }
    // SAFETY: the loader's path of the image, a C string that lives as long
    // as the image, which is this code's.
    let path = unsafe { CStr::from_ptr(info.dli_fname) };
    Some(PathBuf::from(OsStr::from_bytes(path.to_bytes())))
}

/// The `Contents` folder of the app whose bundle holds the plug-in's
/// executable `image`, as
/// `…/Lokyr.app/Contents/PlugIns/LokyrDockTile.docktileplugin/Contents/MacOS/LokyrDockTile`
/// gives `…/Lokyr.app/Contents`; none of a file elsewhere.
fn app_contents(image: &Path) -> Option<&Path> {
    let macos = image
        .parent()
        .filter(|folder| folder.ends_with("Contents/MacOS"))?;
    let plugin = macos.parent()?.parent()?;
    if plugin.extension()? != "docktileplugin" {
        return None;
    }
    plugin
        .parent()
        .filter(|folder| folder.ends_with("Contents/PlugIns"))?
        .parent()
}

/// Runs `register` unless the runtime holds a class of `name`, and keeps
/// its panic within it, as it runs from the loader, of no unwinding.
fn register_once(name: &CStr, register: impl FnOnce() + std::panic::UnwindSafe) {
    if AnyClass::get(name).is_none() {
        let _ = std::panic::catch_unwind(register);
    }
}

/// Registers the plug-in's class with the runtime as the Dock loads the
/// plug-in, so that the bundle's `NSPrincipalClass` names a class that is
/// there: another Lokyr's bundle's plug-in, in the same process, may have
/// registered one of its name, which serves, as the runtime takes one
/// class of a name alone.
extern "C" fn register() {
    register_once(CLASS_NAME, || {
        let _ = Plugin::class();
    });
}

/// The loader's initializer of the image, which runs [`register`] as the
/// plug-in loads.
// SAFETY: `__mod_init_func` holds the initializers that dyld calls once the
// image is loaded, each a function of no argument, as `register` is.
#[used]
#[unsafe(link_section = "__DATA,__mod_init_func")]
static REGISTER: extern "C" fn() = register;

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use objc2::runtime::ClassBuilder;

    use super::*;

    #[test]
    fn a_class_registers_once_and_its_panic_stays_within() {
        // A name that a class holds already: nothing runs.
        let taken = c"LokyrDockTileTestTaken";
        ClassBuilder::new(taken, NSObject::class())
            .expect("a free name")
            .register();
        let ran = AtomicBool::new(false);
        register_once(taken, || ran.store(true, Ordering::Relaxed));
        assert!(!ran.load(Ordering::Relaxed));
        // A free one: it runs, and a panic stays within.
        register_once(c"LokyrDockTileTestFree", || panic!("a second registration"));
        // The plug-in's own class, registered as the image loaded.
        assert!(AnyClass::get(CLASS_NAME).is_some());
        assert_eq!(CLASS_NAME.to_str(), Ok(CLASS));
    }

    #[test]
    fn the_app_s_contents_are_four_folders_up_of_the_plug_in_s_executable() {
        let image = Path::new(
            "/Applications/Lokyr.app/Contents/PlugIns/LokyrDockTile.docktileplugin/Contents/MacOS/LokyrDockTile",
        );
        assert_eq!(
            app_contents(image),
            Some(Path::new("/Applications/Lokyr.app/Contents"))
        );
        for elsewhere in [
            "/usr/lib/libSystem.B.dylib",
            "/Applications/Lokyr.app/Contents/MacOS/lokyr",
            "/tmp/LokyrDockTile.docktileplugin/Contents/MacOS/LokyrDockTile",
            "/Applications/Lokyr.app/Contents/PlugIns/Other.bundle/Contents/MacOS/Other",
            "LokyrDockTile",
        ] {
            assert_eq!(app_contents(Path::new(elsewhere)), None, "{elsewhere}");
        }
        // The loader names this code's image: the tests' executable, of no
        // app's bundle.
        let own = own_image().expect("the image's path");
        assert!(own.is_file(), "{}", own.display());
        assert_eq!(app_contents(&own), None);
    }

    #[test]
    fn a_variant_s_name_names_a_file_of_the_icons_folder_alone() {
        assert!(is_variant("jormungandr-sticker"));
        assert!(is_variant("laguz2"));
        for name in ["", "../lokyr", "/tmp/x", "Laguz", "a b", &"a".repeat(65)] {
            assert!(!is_variant(name), "{name}");
        }
    }
}
