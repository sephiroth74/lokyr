//! A click in the command line of zsh, bash and fish, of Lokyr's shell
//! integration, moves the shell's cursor there, and a key that deletes
//! takes a selection of it away.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use lokyr_pty::Program;
use lokyr_session::{Options, Session};
use lokyr_vt_core::input::{Key, KeyAction, KeyEvent, KeyOptions, Mods};
use lokyr_vt_core::terminal::{SelectionShape, SelectionUnit, Terminal};

fn scripts() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../pty/shell-integration")
}

fn found(paths: &[&str]) -> Option<PathBuf> {
    paths.iter().map(PathBuf::from).find(|path| path.is_file())
}

/// A home of `files`, made empty, of the test's own.
fn home(test: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("lokyr-clicks-{test}-{}", std::process::id()));
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

/// Whether a row of the screen is `text`, its trailing spaces aside.
fn shows(terminal: &Terminal, text: &str) -> bool {
    (0..terminal.rows()).any(|y| terminal.row_text(y).trim_end() == text)
}

/// The column and the row's number of the first `needle` of the cursor's
/// row, as a click there gives them.
fn place_of(terminal: &Terminal, needle: &str) -> (u16, i64) {
    let y = terminal.cursor().y;
    let text = terminal.row_text(y);
    let at = text.find(needle).expect("the needle on the cursor's row");
    let col = u16::try_from(text[..at].chars().count()).unwrap();
    (col, terminal.origin().row(y))
}

/// Types `echo hello` at the prompt `prompt`, clicks on its second word's
/// first letter, types `X` and Return: `Xhello` comes. Then types
/// `echo hello`, selects `hel`, takes it away by Backspace and runs it:
/// `lo` comes.
fn clicks_and_deletes(session: &Session, prompt: &str) {
    until(session, "the prompt", |terminal| {
        terminal.prompt_clicks().is_some()
            && terminal
                .row_text(terminal.cursor().y)
                .starts_with(prompt.trim_end())
    });
    session.send("echo hello");
    until(session, "the command", |terminal| {
        terminal.row_text(terminal.cursor().y).trim_end() == format!("{prompt}echo hello")
    });
    let (col, row) = place_of(&session.lock(), "hello");
    assert!(session.prompt_click(col, row));
    // The shell moves its cursor there before it takes the next key.
    until(session, "the cursor at the click", |terminal| {
        terminal.cursor().x == col
    });
    session.send("X\r");
    until(session, "Xhello", |terminal| shows(terminal, "Xhello"));
    until(session, "the next prompt", |terminal| {
        terminal.prompt_clicks().is_some()
            && terminal.row_text(terminal.cursor().y).trim_end() == prompt.trim_end()
    });
    session.send("echo hello");
    until(session, "the command", |terminal| {
        terminal.row_text(terminal.cursor().y).trim_end() == format!("{prompt}echo hello")
    });
    {
        let mut terminal = session.lock();
        let (col, row) = place_of(&terminal, "hello");
        terminal.select(
            (col, row),
            (col + 2, row),
            SelectionShape::Cells,
            SelectionUnit::Cell,
            "",
        );
    }
    let backspace = KeyEvent {
        key: Key::Backspace,
        action: KeyAction::Press,
        mods: Mods::default(),
        text: "",
        unmodified: "",
        base_layout: None,
    };
    assert!(session.replace_selection(&backspace, KeyOptions::default()));
    until(session, "the selection taken away", |terminal| {
        terminal.row_text(terminal.cursor().y).trim_end() == format!("{prompt}echo lo")
    });
    session.send("\r");
    until(session, "lo", |terminal| shows(terminal, "lo"));
}

#[test]
fn zsh_s_cursor_goes_where_a_click_falls_and_a_selection_goes() {
    let Some(zsh) = found(&["/bin/zsh", "/usr/bin/zsh", "/opt/homebrew/bin/zsh"]) else {
        eprintln!("no zsh here");
        return;
    };
    let home = home("zsh", &[(".zshrc", "PS1='zsh> '\n")]);
    let session = shell(&zsh, &home);
    clicks_and_deletes(&session, "zsh> ");
}

#[test]
fn bash_s_cursor_goes_where_a_click_falls_and_a_selection_goes() {
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
    clicks_and_deletes(&session, "bash> ");
}

#[test]
fn fish_s_cursor_goes_where_a_click_falls_and_a_selection_goes() {
    let Some(fish) = found(&[
        "/opt/homebrew/bin/fish",
        "/usr/local/bin/fish",
        "/usr/bin/fish",
    ]) else {
        eprintln!("no fish here");
        return;
    };
    let home = home(
        "fish",
        &[(
            ".config/fish/config.fish",
            "function fish_prompt; echo -n 'fish> '; end\nfunction fish_greeting; end\n",
        )],
    );
    let session = shell(&fish, &home);
    clicks_and_deletes(&session, "fish> ");
}

/// The first column of `needle` on the cursor's row of `terminal`.
fn column_of(terminal: &Terminal, needle: &str) -> u16 {
    let text = terminal.row_text(terminal.cursor().y);
    let at = text.find(needle).expect("the needle on the cursor's row");
    u16::try_from(text[..at].chars().count()).unwrap()
}

#[test]
fn zsh_s_right_prompt_is_no_part_of_its_command_line() {
    let Some(zsh) = found(&["/bin/zsh", "/usr/bin/zsh", "/opt/homebrew/bin/zsh"]) else {
        eprintln!("no zsh here");
        return;
    };
    let home = home("zsh-right", &[(".zshrc", "PS1='zsh> '\nRPROMPT='main'\n")]);
    let session = shell(&zsh, &home);
    until(&session, "the prompt", |terminal| {
        let text = terminal.row_text(terminal.cursor().y);
        terminal.prompt_clicks().is_some()
            && text.starts_with("zsh> ")
            && text.trim_end().ends_with("main")
    });
    session.send("echo hello");
    until(&session, "the command", |terminal| {
        terminal
            .row_text(terminal.cursor().y)
            .starts_with("zsh> echo hello")
    });
    // A selection of the right prompt is no command line's.
    {
        let mut terminal = session.lock();
        let y = terminal.cursor().y;
        let col = column_of(&terminal, "main");
        let row = terminal.origin().row(y);
        terminal.select(
            (col, row),
            (col + 3, row),
            SelectionShape::Cells,
            SelectionUnit::Cell,
            "",
        );
        assert_eq!(terminal.command_line_selection(), None);
        terminal.clear_selection();
    }
    // A click on `hello` moves the cursor there, past the right prompt.
    let (col, row) = place_of(&session.lock(), "hello");
    assert!(session.prompt_click(col, row));
    until(&session, "the cursor at the click", |terminal| {
        terminal.cursor().x == col
    });
    session.send("X\r");
    until(&session, "Xhello", |terminal| shows(terminal, "Xhello"));
}

#[test]
fn bash_of_no_output_mark_announces_no_clicks() {
    let Some(bash) = found(if cfg!(target_os = "macos") {
        &["/opt/homebrew/bin/bash", "/usr/local/bin/bash"]
    } else {
        &["/bin/bash", "/usr/bin/bash"]
    }) else {
        eprintln!("no bash of 4 or later here");
        return;
    };
    // Without `promptvars`, PS0 cannot mark a command's output, which
    // would leave the terminal sending arrows to the command that runs.
    let home = home(
        "bash-no-promptvars",
        &[(".bashrc", "PS1='bash> '\nshopt -u promptvars\n")],
    );
    let session = shell(&bash, &home);
    until(&session, "the prompt", |terminal| {
        !terminal.commands().is_empty()
            && terminal.row_text(terminal.cursor().y).starts_with("bash>")
    });
    assert_eq!(session.lock().prompt_clicks(), None);
}
