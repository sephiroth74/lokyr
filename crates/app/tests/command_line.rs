//! The tools of the command line: the config written out whole, and the
//! check of a config file, as scripts run them.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// How long a run of the app may take, past which the test ends it and
/// fails, rather than hold CI's runner until its job's limit.
const DEADLINE: Duration = Duration::from_secs(120);

/// A directory of the test's own, empty, as `XDG_CONFIG_HOME`.
fn directory(test: &str) -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("lokyr-command-line-{test}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).unwrap();
    directory
}

/// Runs the app with `args`, its config in `config_home`, for
/// [`DEADLINE`] at most.
fn lokyr(config_home: &Path, args: &[&str]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_lokyr"))
        .args(args)
        .env("XDG_CONFIG_HOME", config_home)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = drain(child.stdout.take().unwrap());
    let stderr = drain(child.stderr.take().unwrap());
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if start.elapsed() > DEADLINE {
            let _ = child.kill();
            let _ = child.wait();
            panic!("lokyr {args:?} ran past {DEADLINE:?}");
        }
        thread::sleep(Duration::from_millis(20));
    };
    Output {
        status,
        stdout: stdout.join().unwrap(),
        stderr: stderr.join().unwrap(),
    }
}

/// The bytes of `pipe`, read to its end on a thread of their own.
fn drain(mut pipe: impl Read + Send + 'static) -> JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = pipe.read_to_end(&mut bytes);
        bytes
    })
}

/// Whether the app told that no frame showed, without a GPU, or with a
/// display that sleeps or that the machine lacks, as some CI's.
fn showed_no_frame(stderr: &str) -> bool {
    stderr.contains("without a GPU")
        || stderr.contains("no Metal device")
        || stderr.contains("no frame showed on the display")
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
    // The crate's version, with the date of the commit built, in UTC.
    let version = lokyr(&home, &["--version"]);
    let committed = Command::new("git")
        .args(["log", "-1", "--date=format-local:%Y%m%d", "--format=%cd"])
        .env("TZ", "UTC")
        .output()
        .unwrap();
    let date = text(&committed.stdout).trim().to_owned();
    assert_eq!(date.len(), 8, "{date}");
    assert_eq!(
        text(&version.stdout),
        format!("lokyr {}-{date}\n", env!("CARGO_PKG_VERSION"))
    );
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn the_startup_s_measure_tells_the_first_frame_and_quits() {
    let home = directory("startup");
    let measured = lokyr(&home, &["--bench", "startup"]);
    assert!(measured.status.success(), "{}", text(&measured.stderr));
    let line = text(&measured.stdout).trim();
    // Without a GPU, or a display that shows its frames, no frame shows,
    // which it tells instead.
    if line.is_empty() {
        assert!(showed_no_frame(text(&measured.stderr)));
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
    // GPU draws one or no display shows it.
    let stderr = text(&measured.stderr);
    if !showed_no_frame(stderr) {
        assert!(
            text(&measured.stdout).contains("first_frame_ms"),
            "{stderr}"
        );
        assert!(stderr.contains("the app takes the defaults"), "{stderr}");
    }
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn the_latency_s_bench_types_its_keys_and_tells_them() {
    let home = directory("latency");
    let measured = lokyr(&home, &["--bench", "latency", "--keys", "10"]);
    assert!(measured.status.success(), "{}", text(&measured.stderr));
    let line = text(&measured.stdout).trim();
    assert!(line.starts_with("{\"refresh_ms\":"), "{line}");
    assert!(line.contains("\"keys\":10,"), "{line}");
    // Without a GPU, or a display that shows its frames, no frame shows a
    // key.
    if !showed_no_frame(text(&measured.stderr)) {
        assert!(!line.contains("\"samples\":0,"), "{line}");
    }
    let wrong = lokyr(&home, &["--bench", "latency", "--keys", "many"]);
    assert!(!wrong.status.success());
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn the_output_s_bench_times_the_frames_of_a_program_s_output() {
    let home = directory("output");
    let measured = lokyr(
        &home,
        &[
            "--bench",
            "output",
            "--window",
            "640x400",
            "--",
            "/bin/sh",
            "-c",
            "seq 1 20000",
        ],
    );
    assert!(measured.status.success(), "{}", text(&measured.stderr));
    let line = text(&measured.stdout).trim();
    assert!(line.starts_with("{\"pixels\":["), "{line}");
    assert!(line.contains("\"total_ms\":{\"p50\":"), "{line}");
    // A program that cannot start ends the bench, failing.
    let missing = lokyr(
        &home,
        &[
            "--bench",
            "output",
            "--window",
            "640x400",
            "--",
            "/no/such/program",
        ],
    );
    assert_eq!(missing.status.code(), Some(1), "{}", text(&missing.stderr));
    assert!(
        text(&missing.stderr).contains("the output's program cannot start"),
        "{}",
        text(&missing.stderr)
    );
    for wrong in [
        &["--bench", "output"][..],
        &["--bench", "output", "--"][..],
        &["--bench", "output", "--window", "640", "--", "/bin/echo"][..],
    ] {
        assert!(!lokyr(&home, wrong).status.success(), "{wrong:?}");
    }
    fs::remove_dir_all(home).unwrap();
}
