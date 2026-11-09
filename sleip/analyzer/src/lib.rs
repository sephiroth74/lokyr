//! Luau's type checker for Sleip: a plugin's sources checked against the
//! plugin API's definitions, `lokyr.d.luau`, in strict mode, of the solver
//! that luau-lsp takes by default, the old one, as it checks them in an
//! editor; each problem of its file, line and column. Luau's `Analysis`
//! library, of the vendored sources of `luau/`, built against the headers
//! of Luau's other libraries, whose code `mlua` links, of the same version.
//! The app never links it.

#![deny(unsafe_code)]

use std::ffi::{CStr, CString, c_char};

// The libraries of Luau that the type checker uses.
use mlua as _;

// The shim's two functions, of C's types alone.
#[allow(unsafe_code)]
unsafe extern "C" {
    fn sleip_luau_check(
        names: *const *const c_char,
        texts: *const *const c_char,
        count: usize,
        definitions: *const c_char,
        definitions_size: usize,
        taken: *const *const c_char,
        taken_count: usize,
        problems: *mut usize,
    ) -> *mut c_char;
    fn sleip_luau_free(text: *mut c_char);
}

/// The globals of Luau's that the plugins' host takes away, which a check
/// tells a call of as of an unknown global: code of text, a function's
/// environment, the collector's control and `print`; `require` stays, each
/// module's own, of its package.
pub const TAKEN: [&str; 6] = [
    "loadstring",
    "getfenv",
    "setfenv",
    "collectgarbage",
    "gcinfo",
    "print",
];

/// What kind of problem a check found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A source that does not parse.
    Syntax,
    /// A type that does not hold.
    Type,
    /// A fault of the type checker itself.
    Internal,
}

/// A problem that a check found: its file, by its path in the package, its
/// line and column, both from 1, its kind, and what it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    /// The file.
    pub file: String,
    /// Its line, from 1.
    pub line: u32,
    /// Its column, from 1.
    pub column: u32,
    /// Its kind.
    pub kind: Kind,
    /// What it is.
    pub message: String,
}

/// Checks `sources`, each its path in the package, as `src/init.luau`, and
/// its text, against `definitions`, the plugin API's: the problems found,
/// in the files' order and then the problems'. A source of a zero byte, as
/// no Luau holds, is a problem of its own.
pub fn check(sources: &[(String, String)], definitions: &str) -> Vec<Problem> {
    let mut problems = Vec::new();
    let mut texts = Vec::new();
    let mut names = Vec::new();
    for (name, text) in sources {
        match (CString::new(name.as_str()), CString::new(text.as_str())) {
            (Ok(name), Ok(text)) => {
                names.push(name);
                texts.push(text);
            }
            _ => problems.push(Problem {
                file: name.clone(),
                line: 1,
                column: 1,
                kind: Kind::Syntax,
                message: "the file holds a zero byte".to_owned(),
            }),
        }
    }
    let taken: Vec<CString> = TAKEN
        .iter()
        .filter_map(|name| CString::new(*name).ok())
        .collect();
    let taken_pointers: Vec<*const c_char> = taken.iter().map(|name| name.as_ptr()).collect();
    let name_pointers: Vec<*const c_char> = names.iter().map(|name| name.as_ptr()).collect();
    let text_pointers: Vec<*const c_char> = texts.iter().map(|text| text.as_ptr()).collect();
    let mut count = 0;
    // SAFETY: the pointers are of NUL-terminated texts that outlive the call,
    // as many as their counts, and the definitions are of
    // `definitions.len()` bytes; the result, a NUL-terminated text that the shim allocated or
    // none, is read once and freed by the shim.
    #[allow(unsafe_code)]
    let records = unsafe {
        let found = sleip_luau_check(
            name_pointers.as_ptr(),
            text_pointers.as_ptr(),
            name_pointers.len(),
            definitions.as_ptr().cast(),
            definitions.len(),
            taken_pointers.as_ptr(),
            taken_pointers.len(),
            &raw mut count,
        );
        if found.is_null() {
            return problems;
        }
        let text = CStr::from_ptr(found).to_string_lossy().into_owned();
        sleip_luau_free(found);
        text
    };
    for record in records.split('\u{1e}').filter(|record| !record.is_empty()) {
        let fields: Vec<&str> = record.split('\u{1f}').collect();
        let [file, line, column, kind, message] = fields[..] else {
            continue;
        };
        problems.push(Problem {
            file: file.to_owned(),
            line: line.parse().unwrap_or(1),
            column: column.parse().unwrap_or(1),
            kind: match kind {
                "SyntaxError" => Kind::Syntax,
                "TypeError" => Kind::Type,
                _ => Kind::Internal,
            },
            message: message.to_owned(),
        });
    }
    let _ = count;
    problems
}

#[cfg(test)]
mod tests;
