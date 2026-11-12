//! The Dock tile plug-in of Lokyr's bundle, which the Dock loads while the
//! app is kept in it and not running, and which draws the icon's variant
//! that the app last showed rather than the bundle's own: the app writes
//! that variant's image's path in its defaults, under [`ICON_KEY`], and
//! posts [`ICON_CHANGED`] to the distributed notification center as it
//! changes, which the plug-in follows.
//!
//! The Dock finds the plug-in's class by the bundle's `NSPrincipalClass`,
//! [`CLASS`], as `NSDockTilePlugIn` tells
//! (<https://developer.apple.com/documentation/appkit/nsdocktileplugin>):
//! the class is registered as the plug-in loads, before the Dock looks.

#![cfg(target_os = "macos")]

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::{NSObject, NSObjectProtocol};
use objc2::{AnyThread, ClassType, MainThreadMarker, MainThreadOnly, Message, define_class, sel};
use objc2_app_kit::{NSDockTile, NSDockTilePlugIn, NSImage, NSImageView};
use objc2_foundation::{
    NSDistributedNotificationCenter, NSNotification, NSPoint, NSRect, NSSize, NSString,
    NSUserDefaults,
};

/// The app's bundle identifier, whose defaults hold the icon's path.
pub const IDENTIFIER: &str = "io.github.sephiroth74.lokyr";

/// The key of the app's defaults that holds the path of the image of the
/// icon's variant that the app shows, none of the bundle's own.
pub const ICON_KEY: &str = "DockTileIcon";

/// The distributed notification that the app posts as its icon changes.
pub const ICON_CHANGED: &str = "io.github.sephiroth74.lokyr.DockTileIconChanged";

/// The plug-in's class, as the bundle's `NSPrincipalClass` names it.
pub const CLASS: &str = "LokyrDockTilePlugIn";

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

/// Draws `tile` of the image whose path the app's defaults hold, or of the
/// bundle's own icon where they hold none or an image that does not read.
fn draw(tile: &NSDockTile) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let image = icon_path().and_then(|path| {
        NSImage::initWithContentsOfFile(NSImage::alloc(), &NSString::from_str(&path))
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

/// The path of the image of the icon's variant, as the app's defaults hold
/// it, read anew each time, as the app writes them from its own process.
fn icon_path() -> Option<String> {
    let defaults = NSUserDefaults::initWithSuiteName(
        NSUserDefaults::alloc(),
        Some(&NSString::from_str(IDENTIFIER)),
    )?;
    defaults.synchronize();
    let path = defaults.stringForKey(&NSString::from_str(ICON_KEY))?;
    Some(path.to_string()).filter(|path| !path.is_empty())
}

/// Registers the plug-in's class with the runtime as the Dock loads the
/// plug-in, so that the bundle's `NSPrincipalClass` names a class that is
/// there.
extern "C" fn register() {
    let _ = Plugin::class();
}

/// The loader's initializer of the image, which runs [`register`] as the
/// plug-in loads.
// SAFETY: `__mod_init_func` holds the initializers that dyld calls once the
// image is loaded, each a function of no argument, as `register` is.
#[used]
#[unsafe(link_section = "__DATA,__mod_init_func")]
static REGISTER: extern "C" fn() = register;
