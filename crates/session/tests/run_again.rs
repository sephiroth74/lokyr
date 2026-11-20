//! A command of several lines, typed at the prompt of zsh and fish, of
//! Lokyr's shell integration, runs again whole: its line, as the shell told
//! it, pasted at the next prompt of an empty command line, and Return.
//! bash tells no line, which the terminal reads of the command's first row
//! alone.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use lokyr_pty::Program;
use lokyr_session::{Options, Session};
use lokyr_vt_core::terminal::Terminal;

fn scripts() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../pty/shell-integration")
}

fn found(paths: &[&str]) -> Option<PathBuf> {
    paths.iter().map(PathBuf::from).find(|path| path.is_file())
}

/// A home of `files`, made empty, of the test's own.
fn home(test: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("lokyr-again-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for (path, text) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();
    std::fs::canonicalize(root).unwrap()
}

/// The shell at `path`, of Lokyr's integration, in `home`.
fn shell(path: &Path, home: &Path) -> Session {
    let mut pty = lokyr_pty::Options::new(
        Program::command(path, Vec::<String>::new()),
        "xterm-256color",
    );
    pty.shell_integration = Some(scripts());
    pty.working_directory = Some(home.to_owned());
    pty.env = vec![
        ("HOME".into(), home.into()),
        ("BASH_SILENCE_DEPRECATION_WARNING".into(), "1".into()),
        ("ZDOTDIR".into(), home.into()),
    ];
    Session::spawn(&Options::new(pty), || {}).unwrap()
}

/// Waits until `done` holds of the terminal, for ten seconds at most.
fn until(session: &Session, what: &str, done: impl Fn(&Terminal) -> bool) {
    let start = Instant::now();
    loop {
        {
            let terminal = session.lock();
            if done(&terminal) {
                return;
            }
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "{what} never came: {:?}",
                (0..terminal.rows())
                    .map(|y| terminal.row_text(y))
                    .collect::<Vec<_>>()
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// How many rows of the screen are `text`, their trailing spaces aside.
fn rows_of(terminal: &Terminal, text: &str) -> usize {
    (0..terminal.rows())
        .filter(|&y| terminal.row_text(y).trim_end() == text)
        .count()
}

/// Types `first` and `second`, each of a Return, at the shell's prompt, a
/// command that prints `one` and `two`, and gives its line as the terminal
/// keeps it.
fn typed(session: &Session, first: &str, second: &str) -> String {
    until(session, "the prompt", |terminal| {
        terminal.edited_command_line().as_deref() == Some("")
    });
    session.send(&format!("{first}\r"));
    std::thread::sleep(Duration::from_millis(200));
    session.send(&format!("{second}\r"));
    until(session, "its end and the next prompt", |terminal| {
        rows_of(terminal, "two") == 1
            && terminal
                .last_command()
                .is_some_and(|last| last.output.is_none())
            && terminal.edited_command_line().as_deref() == Some("")
    });
    let terminal = session.lock();
    let commands = terminal.commands();
    let ran = commands
        .iter()
        .rev()
        .find(|command| command.end.is_some())
        .unwrap();
    terminal.command_line(ran.id).unwrap()
}

/// Types the command of `first` and `second`, whose line the shell tells
/// as `told`, then runs that line again, as Run Command Again does: `one`
/// and `two` come again.
fn runs_again(session: &Session, first: &str, second: &str, told: &str) {
    let line = typed(session, first, second);
    assert_eq!(line, told);
    session.paste(&line);
    session.send("\r");
    until(session, "the command run again", |terminal| {
        rows_of(terminal, "two") == 2
    });
}

#[test]
fn zsh_runs_a_command_of_two_lines_again_whole() {
    let Some(zsh) = found(&["/bin/zsh", "/usr/bin/zsh", "/opt/homebrew/bin/zsh"]) else {
        eprintln!("no zsh here");
        return;
    };
    let home = home("zsh", &[(".zshrc", "PS1='zsh> '\n")]);
    let session = shell(&zsh, &home);
    runs_again(
        &session,
        "for word in one two",
        "do echo $word; done",
        "for word in one two\ndo echo $word; done",
    );
}

#[test]
fn bash_tells_no_line_of_a_command_of_two_lines_but_its_first_row_s() {
    // macOS's own bash, 3.2, takes no integration.
    let Some(bash) = found(if cfg!(target_os = "macos") {
        &["/opt/homebrew/bin/bash", "/usr/local/bin/bash"]
    } else {
        &["/bin/bash", "/usr/bin/bash"]
    }) else {
        eprintln!("no bash of 4 or later here");
        return;
    };
    let home = home("bash", &[(".bashrc", "PS1='bash> '\n")]);
    let session = shell(&bash, &home);
    assert_eq!(
        typed(&session, "for word in one two", "do echo $word; done"),
        "for word in one two"
    );
}

#[test]
fn fish_runs_a_command_of_two_lines_again_whole() {
    let Some(fish) = found(&[
        "/opt/homebrew/bin/fish",
        "/usr/local/bin/fish",
        "/usr/bin/fish",
    ]) else {
        eprintln!("no fish here");
        return;
    };
    let home = home("fish", &[(".config/fish/config.fish", "")]);
    let session = shell(&fish, &home);
    runs_again(
        &session,
        "for word in one two",
        "echo $word; end",
        "for word in one two\necho $word; end",
    );
}
