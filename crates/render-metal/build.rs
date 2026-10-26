//! Compiles the Metal shaders into a library that the crate embeds, so that
//! nothing compiles at startup.

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let source = PathBuf::from("shaders/cells.metal");
    println!("cargo::rerun-if-changed={}", source.display());
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let out = PathBuf::from(env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    let air = out.join("cells.air");
    let library = out.join("cells.metallib");
    run(Command::new("xcrun")
        .args([
            "-sdk",
            "macosx",
            "metal",
            "-c",
            "-O3",
            "-mmacosx-version-min=26.0",
        ])
        .arg(&source)
        .arg("-o")
        .arg(&air));
    run(Command::new("xcrun")
        .args(["-sdk", "macosx", "metallib"])
        .arg(&air)
        .arg("-o")
        .arg(&library));
}

/// Runs `command`, and fails the build with what it said if it fails.
fn run(command: &mut Command) {
    let output = command.output().unwrap_or_else(|error| {
        panic!("{command:?} did not start: {error}; the Metal toolchain needs Xcode")
    });
    if !output.status.success() {
        panic!(
            "{command:?} failed: {}\nIf the Metal toolchain is missing, install it with \
             `xcodebuild -downloadComponent MetalToolchain`.",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
