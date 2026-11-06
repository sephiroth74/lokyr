//! macOS platform backend, built on AppKit.
//!
//! [`run`] starts the application and hands the app's
//! [`Handler`](lokyr_platform_core::Handler) what happens in it, on the main
//! thread's run loop: its windows, and their frames at their displays' pace;
//! its main menu, the clipboard and alerts; and the wakes of other threads,
//! through a source of that run loop.

#![cfg(target_os = "macos")]

mod about;
mod access;
mod accessory;
mod alert;
mod app;
mod appearance;
mod backend;
mod definition;
mod drag_image;
mod frame;
mod global_keys;
mod keys;
mod layer;
mod menu;
mod notify;
mod open;
mod quick;
mod screens;
mod secure;
mod shared;
mod sweep;
mod text_input;
mod timer;
mod updates;
mod wake;
mod window;

pub use app::{run, run_with};
