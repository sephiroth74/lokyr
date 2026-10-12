//! The windows on screen, as the window server lists them.

use objc2_core_foundation::{CFDictionary, CFNumber, CFRetained, CFString, CFType};
use objc2_core_graphics::{
    CGWindowListCopyWindowInfo, CGWindowListOption, kCGNullWindowID, kCGWindowBounds,
    kCGWindowLayer, kCGWindowOwnerPID,
};
use serde::Serialize;

/// A window's description, as the window server gives it.
type Description = CFDictionary<CFString, CFType>;

/// The size of a window, in points.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

/// Whether the process `pid` has a window on screen at the normal level: the
/// level of document windows, below menus, panels and the menu bar.
pub fn has_window(pid: libc::pid_t) -> bool {
    !normal_windows(pid).is_empty()
}

/// The sizes of the windows that the process `pid` has on screen at the
/// normal level.
pub fn sizes(pid: libc::pid_t) -> Vec<Size> {
    normal_windows(pid)
        .iter()
        .filter_map(|window| size(window))
        .collect()
}

/// The windows of `pid` on screen at the normal level. A window's owner,
/// level and bounds can be read without the Screen Recording permission.
fn normal_windows(pid: libc::pid_t) -> Vec<CFRetained<Description>> {
    let options =
        CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements;
    let Some(windows) = CGWindowListCopyWindowInfo(options, kCGNullWindowID) else {
        return Vec::new();
    };
    // SAFETY: the window server describes each window with a dictionary
    // whose keys are strings.
    #[allow(unsafe_code)]
    let windows = unsafe { windows.cast_unchecked::<Description>() };
    // SAFETY: reading constants that CoreGraphics defines.
    #[allow(unsafe_code)]
    let (owner, layer) = unsafe { (kCGWindowOwnerPID, kCGWindowLayer) };
    windows
        .iter()
        .filter(|window| integer(window, owner) == Some(pid) && integer(window, layer) == Some(0))
        .collect()
}

/// The integer at `key` in a window's description.
fn integer(window: &Description, key: &CFString) -> Option<i32> {
    window.get(key)?.downcast::<CFNumber>().ok()?.as_i32()
}

/// The size in a window's bounds, a dictionary with `Width` and `Height`.
fn size(window: &Description) -> Option<Size> {
    // SAFETY: reading a constant that CoreGraphics defines.
    #[allow(unsafe_code)]
    let key = unsafe { kCGWindowBounds };
    let bounds = window.get(key)?.downcast::<CFDictionary>().ok()?;
    // SAFETY: the bounds are a dictionary of numbers keyed by strings.
    #[allow(unsafe_code)]
    let bounds = unsafe { bounds.cast_unchecked::<CFString, CFNumber>() };
    let dimension = |name: &'static str| bounds.get(&CFString::from_static_str(name))?.as_f64();
    Some(Size {
        width: dimension("Width")?,
        height: dimension("Height")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_test_process_has_no_window() {
        let pid = libc::pid_t::try_from(std::process::id()).unwrap();
        assert!(!has_window(pid));
        assert!(sizes(pid).is_empty());
    }
}
