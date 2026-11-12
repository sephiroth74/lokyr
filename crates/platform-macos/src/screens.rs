//! The screens' plane, as the app gives places on it, from the top left
//! corner of the menu bar's screen, downwards; and AppKit's coordinates,
//! from that screen's bottom left corner, upwards.

use lokyr_platform_core::{ScreenPoint, ScreenRect};
use objc2::MainThreadMarker;
use objc2_app_kit::NSScreen;
use objc2_foundation::{NSPoint, NSRect, NSSize};

/// The top of the menu bar's screen in AppKit's coordinates, the plane's
/// top; 0 without a screen.
pub(crate) fn top(mtm: MainThreadMarker) -> f64 {
    NSScreen::screens(mtm).firstObject().map_or(0.0, |first| {
        let frame = first.frame();
        frame.origin.y + frame.size.height
    })
}

/// `rect`, of AppKit's coordinates, whose origin is the bottom left corner
/// of the menu bar's screen, upwards, on the screens' plane, downwards from
/// that screen's top, `top` in AppKit's.
pub(crate) fn on_screens(rect: NSRect, top: f64) -> ScreenRect {
    ScreenRect {
        x: rect.origin.x,
        y: top - rect.origin.y - rect.size.height,
        width: rect.size.width,
        height: rect.size.height,
    }
}

/// `rect`, of the screens' plane, in AppKit's coordinates.
pub(crate) fn in_appkit(rect: ScreenRect, top: f64) -> NSRect {
    NSRect::new(
        NSPoint::new(rect.x, top - rect.y - rect.height),
        NSSize::new(rect.width, rect.height),
    )
}

/// `point`, of the screens' plane, in AppKit's coordinates.
pub(crate) fn point_in_appkit(point: ScreenPoint, top: f64) -> NSPoint {
    NSPoint::new(point.x, top - point.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rectangle_goes_between_appkit_s_coordinates_and_the_screens_plane() {
        // A screen of 900 points high, whose menu bar takes 25: its area
        // for windows, in AppKit's coordinates, from the bottom, and on the
        // plane, from the top.
        let area = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1440.0, 875.0));
        let plane = on_screens(area, 900.0);
        assert_eq!(
            plane,
            ScreenRect {
                x: 0.0,
                y: 25.0,
                width: 1440.0,
                height: 875.0
            }
        );
        assert_eq!(in_appkit(plane, 900.0), area);
        // A screen above it, past the plane's top.
        let above = NSRect::new(NSPoint::new(100.0, 900.0), NSSize::new(1920.0, 1080.0));
        assert_eq!(on_screens(above, 900.0).y, -1080.0);
        assert_eq!(in_appkit(on_screens(above, 900.0), 900.0), above);
    }

    #[test]
    fn a_point_of_the_plane_goes_up_from_the_menu_bar_s_screen_s_bottom() {
        assert_eq!(
            point_in_appkit(ScreenPoint { x: 10.0, y: 25.0 }, 900.0),
            NSPoint::new(10.0, 875.0)
        );
    }
}
