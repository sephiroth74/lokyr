//! The windows on screen, as the window server lists them.

use objc2_core_foundation::{CFDictionary, CFNumber, CFString, CFType};
use objc2_core_graphics::{
    CGWindowListCopyWindowInfo, CGWindowListOption, kCGNullWindowID, kCGWindowLayer,
    kCGWindowOwnerPID,
};

/// Whether the process `pid` has a window on screen at the normal level: the
/// level of document windows, below menus, panels and the menu bar. The owner
/// and the level of a window can be read without the Screen Recording
/// permission.
pub fn has_window(pid: libc::pid_t) -> bool {
    let options =
        CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements;
    let Some(windows) = CGWindowListCopyWindowInfo(options, kCGNullWindowID) else {
        return false;
    };
    // SAFETY: the window server describes each window with a dictionary
    // whose keys are strings.
    #[allow(unsafe_code)]
    let windows = unsafe { windows.cast_unchecked::<CFDictionary<CFString, CFType>>() };
    // SAFETY: reading constants that CoreGraphics defines.
    #[allow(unsafe_code)]
    let (owner, layer) = unsafe { (kCGWindowOwnerPID, kCGWindowLayer) };
    windows
        .iter()
        .any(|window| number(&window, owner) == Some(pid) && number(&window, layer) == Some(0))
}

/// The integer at `key` in a window's description.
fn number(window: &CFDictionary<CFString, CFType>, key: &CFString) -> Option<i32> {
    window.get(key)?.downcast::<CFNumber>().ok()?.as_i32()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_test_process_has_no_window() {
        let pid = libc::pid_t::try_from(std::process::id()).unwrap();
        assert!(!has_window(pid));
    }
}
