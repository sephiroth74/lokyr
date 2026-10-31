//! The tools of the command line: the config written out whole, and the
//! check of a config file, as scripts run them.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A directory of the test's own, empty, as `XDG_CONFIG_HOME`.
fn directory(test: &str) -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("lokyr-command-line-{test}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).unwrap();
    directory
}

/// Runs the app with `args`, its config in `config_home`.
fn lokyr(config_home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lokyr"))
        .args(args)
        .env("XDG_CONFIG_HOME", config_home)
        .output()
        .unwrap()
}

fn text(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).unwrap()
}

#[test]
fn the_written_defaults_check_as_valid() {
    let home = directory("defaults");
    for args in [
        &["+show-config", "--default"][..],
        &["+show-config", "--docs", "--default"],
    ] {
        let shown = lokyr(&home, args);
        assert!(shown.status.success(), "{}", text(&shown.stderr));
        let file = home.join("written.toml");
        fs::write(&file, &shown.stdout).unwrap();
        let checked = lokyr(&home, &["+validate-config", file.to_str().unwrap()]);
        assert!(checked.status.success(), "{}", text(&checked.stderr));
        assert_eq!(
            text(&checked.stdout),
            format!("{}: valid\n", file.display())
        );
    }
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn the_config_shows_with_the_file_s_values() {
    let home = directory("file");
    fs::create_dir_all(home.join("lokyr")).unwrap();
    let file = home.join("lokyr/config.toml");
    fs::write(
        &file,
        "[font]\nsize = 22\n\n[window]\nbackground-opacity = 0.85\n",
    )
    .unwrap();
    let shown = lokyr(&home, &["+show-config"]);
    assert!(shown.status.success(), "{}", text(&shown.stderr));
    let shown = text(&shown.stdout);
    assert!(shown.contains("\nsize = 22\n"), "{shown}");
    assert!(shown.contains("\nbackground-opacity = 0.85\n"), "{shown}");
    assert!(shown.contains("\nline-height = 1\n"), "{shown}");
    let checked = lokyr(&home, &["+validate-config"]);
    assert!(checked.status.success(), "{}", text(&checked.stderr));
    // An invalid file fails both, at its place.
    fs::write(&file, "config_version = 1\n\n[font]\nsise = 14\n").unwrap();
    let error = format!(
        "lokyr: {}:4:1: unknown key `sise` in `[font]`; did you mean `size`?\n",
        file.display()
    );
    for args in [&["+show-config"][..], &["+validate-config"]] {
        let failed = lokyr(&home, args);
        assert_eq!(failed.status.code(), Some(1));
        assert_eq!(text(&failed.stderr), error);
        assert!(failed.stdout.is_empty());
    }
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_file_fails_its_check_with_its_place() {
    let home = directory("invalid");
    let file = home.join("other.toml");
    fs::write(&file, "[terminal]\nambiguous-width = 3\n").unwrap();
    let failed = lokyr(&home, &["+validate-config", file.to_str().unwrap()]);
    assert_eq!(failed.status.code(), Some(1));
    let error = text(&failed.stderr);
    assert!(
        error.starts_with(&format!("lokyr: {}:2:19: ", file.display())),
        "{error}"
    );
    // A font's style that the family lacks fails it, as the app refuses it.
    fs::write(&file, "[font]\nfamily = \"Menlo\"\nstyle = \"Nonesuch\"\n").unwrap();
    let failed = lokyr(&home, &["+validate-config", file.to_str().unwrap()]);
    assert_eq!(failed.status.code(), Some(1));
    assert!(text(&failed.stderr).contains("no style `Nonesuch`"));
    // A language that Lokyr lacks only warns, as the rest applies.
    fs::write(&file, "[app]\nlanguage = \"fr\"\n").unwrap();
    let warned = lokyr(&home, &["+validate-config", file.to_str().unwrap()]);
    assert!(warned.status.success());
    assert!(text(&warned.stderr).contains("the language `fr` is not one of Lokyr's"));
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_missing_file_fails_only_when_named() {
    let home = directory("missing");
    let named = lokyr(&home, &["+validate-config", "/nonexistent/config.toml"]);
    assert_eq!(named.status.code(), Some(1));
    assert_eq!(
        text(&named.stderr),
        "lokyr: /nonexistent/config.toml: there is no such file\n"
    );
    // The app's own path without a file gives the defaults.
    let own = lokyr(&home, &["+validate-config"]);
    assert!(own.status.success(), "{}", text(&own.stderr));
    assert!(text(&own.stdout).ends_with("no file, and so the defaults\n"));
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_wrong_command_line_tells_the_usage() {
    let home = directory("usage");
    for args in [
        &["+show-config", "--doc"][..],
        &["+validate-config", "a", "b"],
        &["+validate-config", "--help"],
        &["--version", "x"],
        &["+other"],
    ] {
        let failed = lokyr(&home, args);
        assert_eq!(failed.status.code(), Some(1), "{args:?}");
        assert!(text(&failed.stderr).starts_with("Usage: lokyr"), "{args:?}");
    }
    let version = lokyr(&home, &["--version"]);
    assert_eq!(
        text(&version.stdout),
        format!("lokyr {}\n", env!("CARGO_PKG_VERSION"))
    );
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn the_startup_s_measure_tells_the_first_frame_and_quits() {
    let home = directory("startup");
    let measured = lokyr(&home, &["--bench", "startup"]);
    assert!(measured.status.success(), "{}", text(&measured.stderr));
    let line = text(&measured.stdout).trim();
    // Without a GPU, no frame shows, which it tells instead.
    if line.is_empty() {
        assert!(text(&measured.stderr).contains("without a GPU"));
        return;
    }
    let ms: f64 = line
        .split("\"first_frame_ms\":")
        .nth(1)
        .and_then(|rest| rest.trim_end_matches('}').parse().ok())
        .unwrap_or_else(|| panic!("no measure in {line}"));
    assert!(ms > 0.0 && ms < 30_000.0, "{line}");
    assert!(line.starts_with("{\"pid\":"), "{line}");
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_config_whose_font_fails_starts_with_the_defaults() {
    let home = directory("failing-font");
    fs::create_dir_all(home.join("lokyr")).unwrap();
    fs::write(
        home.join("lokyr/config.toml"),
        "[font]\nstyle = \"No Such Style\"\n",
    )
    .unwrap();
    let measured = lokyr(&home, &["--bench", "startup"]);
    assert!(measured.status.success(), "{}", text(&measured.stderr));
    // Its first frame shows, of the defaults, as the log tells, unless no
    // GPU draws one.
    let stderr = text(&measured.stderr);
    if !stderr.contains("without a GPU") {
        assert!(
            text(&measured.stdout).contains("first_frame_ms"),
            "{stderr}"
        );
        assert!(stderr.contains("the app takes the defaults"), "{stderr}");
    }
    fs::remove_dir_all(home).unwrap();
}
