//! The app's accessibility tree, as an assistive app reads it from another
//! process: its windows, and each element's role, title, description,
//! value and children, as AppKit's adapter of AccessKit gives them.
//!
//! Reading another app's tree takes the Accessibility permission of the app
//! that runs the checks, as the terminal or the editor that starts them.

use std::ffi::c_void;
use std::ptr::NonNull;

use objc2_core_foundation::{CFArray, CFRetained, CFString, CFType};

/// An element of an app's accessibility tree, `AXUIElementRef`.
pub struct Element(CFRetained<CFType>);

#[allow(unsafe_code)]
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C-unwind" {
    fn AXIsProcessTrusted() -> bool;
    fn AXUIElementCreateApplication(pid: libc::pid_t) -> *mut c_void;
    fn AXUIElementCopyAttributeValue(
        element: *const c_void,
        attribute: &CFString,
        value: *mut *mut c_void,
    ) -> i32;
}

/// Whether the system lets this process read other apps' trees and post
/// events, as the Accessibility permission of the app that runs it says.
#[allow(unsafe_code)]
pub fn trusted() -> bool {
    // SAFETY: a call of no argument.
    unsafe { AXIsProcessTrusted() }
}

/// The element of an element's value, if it is one: a retained `CFType`.
#[allow(unsafe_code)]
fn retained(raw: *mut c_void) -> Option<CFRetained<CFType>> {
    // SAFETY: the system gives a value that the caller owns, a CF type, or
    // none.
    NonNull::new(raw).map(|raw| unsafe { CFRetained::from_raw(raw.cast::<CFType>()) })
}

impl Element {
    /// The app of the process `pid`.
    #[allow(unsafe_code)]
    pub fn application(pid: libc::pid_t) -> Option<Self> {
        // SAFETY: a call of a process id, which gives an element that the
        // caller owns.
        retained(unsafe { AXUIElementCreateApplication(pid) }).map(Self)
    }

    /// The value of the attribute `name`, if the element has one.
    fn attribute(&self, name: &str) -> Option<CFRetained<CFType>> {
        let mut value = std::ptr::null_mut();
        let name = CFString::from_str(name);
        let raw: *const CFType = &*self.0;
        // SAFETY: the element lives through the call, and the value that it
        // gives is owned by the caller.
        #[allow(unsafe_code)]
        let error = unsafe { AXUIElementCopyAttributeValue(raw.cast(), &name, &mut value) };
        if error != 0 {
            return None;
        }
        retained(value)
    }

    /// The text of the attribute `name`, if it has one of text.
    pub fn text(&self, name: &str) -> Option<String> {
        let value = self.attribute(name)?;
        let text = value.downcast_ref::<CFString>()?;
        Some(text.to_string())
    }

    /// The elements of the attribute `name`, as `AXChildren`'s and
    /// `AXWindows`'.
    fn elements(&self, name: &str) -> Vec<Element> {
        let Some(value) = self.attribute(name) else {
            return Vec::new();
        };
        let Some(array) = value.downcast_ref::<CFArray>() else {
            return Vec::new();
        };
        // SAFETY: an array of elements, CF types, which each retain takes
        // a share of.
        #[allow(unsafe_code)]
        let array: &CFArray<CFType> = unsafe { array.cast_unchecked() };
        (0..array.len())
            .filter_map(|index| array.get(index))
            .map(Element)
            .collect()
    }

    /// Its role, as `AXWindow` or `AXTextArea`.
    pub fn role(&self) -> String {
        self.text("AXRole").unwrap_or_default()
    }

    /// Its children.
    pub fn children(&self) -> Vec<Element> {
        self.elements("AXChildren")
    }

    /// The app's windows, as its element's.
    pub fn windows(&self) -> Vec<Element> {
        self.elements("AXWindows")
    }

    /// The element and those under it, depth first, `most` deep at most.
    pub fn all(self, most: usize) -> Vec<Element> {
        let mut found = Vec::new();
        let mut waiting = vec![(self, 0)];
        while let Some((element, depth)) = waiting.pop() {
            if depth < most {
                let children = element.children();
                waiting.extend(children.into_iter().rev().map(|child| (child, depth + 1)));
            }
            found.push(element);
        }
        found
    }

    /// A line of the element, for a person: its role, and its title, its
    /// description and its value, those that it has, cut at 120
    /// characters each.
    pub fn line(&self) -> String {
        let mut line = self.role();
        for name in ["AXSubrole", "AXTitle", "AXDescription", "AXValue"] {
            if let Some(text) = self.text(name).filter(|text| !text.is_empty()) {
                let text: String = text.chars().take(120).collect();
                line.push_str(&format!(" {name}={text:?}"));
            }
        }
        line
    }
}
