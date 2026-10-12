//! The escape-sequence parser.
//!
//! It turns the bytes of the PTY into calls on a [`Handler`]: text to print,
//! control characters to execute, escape and control sequences to dispatch,
//! and control strings (OSC, DCS and APC), whose payloads arrive in chunks.

mod handler;
mod params;
mod recorder;

pub use handler::{Handler, StringEnd};
pub use params::{MAX_PARAMS, Params};
pub use recorder::{Action, Recorder};
