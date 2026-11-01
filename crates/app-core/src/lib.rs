//! The window's controller of the Lokyr terminal, over any platform,
//! renderer and font backend: what each window holds, the input that it
//! routes to its panes, its questions, notices and menus, the frames that
//! it composes, and the benches that measure them. The app gives it the
//! platform's backend, which runs it, and its drawing's.

use std::sync::atomic::AtomicBool;

mod app;
mod drawing;
mod environment;
#[cfg(test)]
mod fakes;
mod icon;
mod keys;
mod latency;
mod mouse;
mod notice;
mod output_bench;
mod performance;
mod quick;
mod size_label;
mod watch;
mod window;

pub use app::{App, check};
pub use drawing::{Backend, Drawing};
pub use environment::bundle_executable;

/// Whether the output bench could not start its program, which the app's
/// exit code tells; the other benches that give up tell it on stderr alone.
pub static BENCH_FAILED: AtomicBool = AtomicBool::new(false);
