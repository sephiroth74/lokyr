//! macOS platform backend, built on AppKit.
//!
//! [`run`] starts the application and hands the app's
//! [`Handler`](lokyr_platform_core::Handler) what happens in it, on the main
//! thread's run loop: its windows, and their frames at their displays' pace;
//! its main menu, the clipboard and alerts; and the wakes of other threads,
//! through a source of that run loop.

#![cfg(target_os = "macos")]

mod alert;
mod app;
mod appearance;
mod backend;
mod frame;
mod keys;
mod menu;
mod notify;
mod open;
mod shared;
mod text_input;
mod timer;
mod wake;
mod window;

pub use app::run;
