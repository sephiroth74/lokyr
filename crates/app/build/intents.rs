//! The App Intents of the feature `app-intents`: their declarations of
//! Swift in `intents/`, which `swiftc` compiles into a static library that
//! the app links whole, and their metadata, which the system reads of the
//! bundle, made of the compiler's constant values by the toolchain's
//! `appintentsmetadataprocessor` into `OUT_DIR/Metadata.appintents`, which
//! the bundle copies.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The intents' Swift module, and the static library's name.
const MODULE: &str = "LokyrIntents";

/// The system that the intents take at least, the app's.
const DEPLOYMENT: &str = "26.0";

/// The protocols whose conformances the compiler tells the constant values
/// of, as Xcode asks for the App Intents.
const PROTOCOLS: &str = r#"["AppIntent","EntityQuery","AppEntity","TransientEntity","AppEnum","AppShortcutProviding","AppShortcutsProvider","AnyResolverProviding","AppIntentsPackage","DynamicOptionsProvider"]"#;

/// Builds the intents of the sources in `manifest/intents` into `out`, and
/// tells Cargo to link them.
pub fn build(manifest: &Path, out: &Path) {
    let folder = manifest.join("intents");
    println!("cargo:rerun-if-changed={}", folder.display());
    let mut sources: Vec<PathBuf> = std::fs::read_dir(&folder)
        .unwrap_or_else(|error| panic!("{}: {error}", folder.display()))
        .filter_map(|entry| Some(entry.ok()?.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "swift")
        })
        .collect();
    sources.sort();
    for source in &sources {
        println!("cargo:rerun-if-changed={}", source.display());
    }
    let sdk = PathBuf::from(xcrun(&["--show-sdk-path"]));
    let swiftc = PathBuf::from(xcrun(&["--find", "swiftc"]));
    // `…/XcodeDefault.xctoolchain/usr/bin/swiftc`.
    let toolchain = swiftc
        .ancestors()
        .nth(3)
        .expect("swiftc is in a toolchain's usr/bin")
        .to_path_buf();
    let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        Ok(other) => panic!("the App Intents build for Apple silicon alone, not {other}"),
        Err(_) => panic!("Cargo gives the target's architecture"),
    };
    let target = format!("{arch}-apple-macos{DEPLOYMENT}");
    let protocols = out.join("protocols.json");
    write(&protocols, PROTOCOLS);
    let library = out.join(format!("lib{MODULE}.a"));
    let values = out.join(format!("{MODULE}.swiftconstvalues"));
    let mut compile = Command::new(&swiftc);
    compile
        .args(["-module-name", MODULE, "-parse-as-library", "-wmo", "-O"])
        .args(["-target", &target])
        .arg("-sdk")
        .arg(&sdk)
        .args(["-emit-library", "-static", "-o"])
        .arg(&library)
        .arg("-emit-const-values-path")
        .arg(&values)
        .args(["-Xfrontend", "-const-gather-protocols-file", "-Xfrontend"])
        .arg(&protocols)
        // A module cache of the build's own, which a sandbox may require.
        .arg("-module-cache-path")
        .arg(out.join("module-cache"))
        .args(&sources);
    run(&mut compile, "compiling the App Intents");
    let source_list = out.join("sources.txt");
    let lines: Vec<String> = sources
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    write(&source_list, &(lines.join("\n") + "\n"));
    let value_list = out.join("values.txt");
    write(&value_list, &format!("{}\n", values.display()));
    let metadata = out.join("metadata");
    let _ = std::fs::remove_dir_all(&metadata);
    std::fs::create_dir_all(&metadata)
        .unwrap_or_else(|error| panic!("{}: {error}", metadata.display()));
    let processor = xcrun(&["--find", "appintentsmetadataprocessor"]);
    let mut extract = Command::new(processor);
    extract
        .arg("--output")
        .arg(&metadata)
        .arg("--toolchain-dir")
        .arg(&toolchain)
        .args(["--module-name", MODULE])
        .arg("--sdk-root")
        .arg(&sdk)
        .args(["--xcode-version", &xcode_build()])
        .args(["--platform-family", "macOS"])
        .args(["--deployment-target", DEPLOYMENT])
        .args(["--target-triple", &target])
        .arg("--source-file-list")
        .arg(&source_list)
        .arg("--swift-const-vals-list")
        .arg(&value_list)
        .args(["--force", "--deployment-aware-processing"]);
    run(&mut extract, "extracting the App Intents' metadata");
    let made = metadata.join("Metadata.appintents");
    assert!(
        made.join("extract.actionsdata").is_file(),
        "the App Intents' metadata were not made in {}",
        made.display()
    );
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static:+whole-archive={MODULE}");
    // The Swift libraries, of the system's from macOS 26 on.
    println!(
        "cargo:rustc-link-search=native={}",
        sdk.join("usr/lib/swift").display()
    );
    println!(
        "cargo:rustc-link-search=native={}",
        toolchain.join("usr/lib/swift/macosx").display()
    );
    println!("cargo:rustc-link-arg-bins=-Wl,-rpath,/usr/lib/swift");
}

/// What `xcrun` answers of `args`, trimmed; the build fails without it.
fn xcrun(args: &[&str]) -> String {
    let output = Command::new("xcrun")
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("the feature app-intents needs Xcode's xcrun: {error}"));
    assert!(
        output.status.success(),
        "the feature app-intents needs Xcode: `xcrun {}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// Xcode's build version, as `27B52`, which the metadata tell.
fn xcode_build() -> String {
    let output = Command::new("xcodebuild")
        .arg("-version")
        .output()
        .unwrap_or_else(|error| panic!("the feature app-intents needs xcodebuild: {error}"));
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("Build version "))
        .map_or_else(
            || panic!("`xcodebuild -version` tells no build version"),
            str::to_owned,
        )
}

fn write(path: &Path, text: &str) {
    std::fs::write(path, text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
}

fn run(command: &mut Command, what: &str) {
    let status = command
        .status()
        .unwrap_or_else(|error| panic!("{what}: {error}"));
    assert!(status.success(), "{what} failed: {status}");
}
