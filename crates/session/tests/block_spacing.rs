//! The blank row that the terminal puts before each prompt that follows a
//! command, of zsh, bash and fish of Lokyr's integration and of a prompt
//! of two rows: the shells draw their command lines, and their prompts
//! again at a new size, on the rows where the prompts are.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use lokyr_pty::{Program, Size};
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
    let root = std::env::temp_dir().join(format!("lokyr-spacing-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for (path, text) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();
    std::fs::canonicalize(root).unwrap()
}

/// The shell at `path`, of Lokyr's integration, in `home`, its terminal
/// putting blank rows before the prompts.
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
    let session = Session::spawn(&Options::new(pty), || {}).unwrap();
    session.lock().set_block_spacing(true);
    session
}

/// The screen's rows, without the spaces that end them, to the last that
/// holds text.
fn screen(terminal: &Terminal) -> Vec<String> {
    let mut rows: Vec<String> = (0..terminal.rows())
        .map(|y| terminal.row_text(y).trim_end().to_owned())
        .collect();
    while rows.last().is_some_and(String::is_empty) {
        rows.pop();
    }
    rows
}

/// Waits until the screen is `rows`, for ten seconds at most.
fn until_screen(session: &Session, rows: &[&str]) {
    let start = Instant::now();
    loop {
        let shown = screen(&session.lock());
        if shown == rows {
            return;
        }
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "{rows:?} never came: {shown:?}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Runs `echo one`, types `echo two`, has the terminal narrower, then
/// runs it: a blank row before each prompt after a command, none before
/// the first, the command lines on the prompts' second rows; the second
/// prompt's first row `top` after the new size, as the shell draws it
/// again.
fn spaces_its_blocks(session: &Session, top: &str) {
    until_screen(session, &["top", ">"]);
    session.send("echo one\r");
    until_screen(session, &["top", "> echo one", "one", "", "top", ">"]);
    session.send("echo two");
    until_screen(
        session,
        &["top", "> echo one", "one", "", "top", "> echo two"],
    );
    // The shell draws its prompt again at the new size, from its row.
    let size = Size {
        cols: 70,
        rows: 24,
        width: 0,
        height: 0,
    };
    session.resize(size).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    until_screen(
        session,
        &["top", "> echo one", "one", "", top, "> echo two"],
    );
    session.send("\r");
    until_screen(
        session,
        &[
            "top",
            "> echo one",
            "one",
            "",
            top,
            "> echo two",
            "two",
            "",
            "top",
            ">",
        ],
    );
    // The blank rows are no command's, and the blocks keep them through the
    // prompt that the shell drew again at the new size.
    let mut terminal = session.lock();
    let mut snapshot = lokyr_vt_core::terminal::Snapshot::default();
    terminal.snapshot(&mut snapshot);
    let rooms: Vec<(bool, bool)> = snapshot
        .blocks()
        .iter()
        .map(|block| (block.room_above, block.room_below))
        .collect();
    assert_eq!(rooms, [(false, true), (true, true), (true, false)]);
    let commands = terminal.commands();
    let prompts: Vec<i64> = commands
        .iter()
        .filter_map(|command| command.prompt.map(|(_, row)| row))
        .map(|row| row - terminal.origin().row(0))
        .collect();
    assert_eq!(prompts, [0, 4, 8]);
    assert_eq!(terminal.command_at(terminal.origin().row(3)), None);
}

#[test]
fn zsh_draws_its_lines_and_its_prompts_past_the_blank_rows() {
    let Some(zsh) = found(&["/bin/zsh", "/usr/bin/zsh", "/opt/homebrew/bin/zsh"]) else {
        eprintln!("no zsh here");
        return;
    };
    let home = home("zsh", &[(".zshrc", "PS1=$'top\\n> '\n")]);
    spaces_its_blocks(&shell(&zsh, &home), "top");
}

#[test]
fn bash_draws_its_lines_and_its_prompts_past_the_blank_rows() {
    // macOS's own bash, 3.2, takes no integration.
    let Some(bash) = found(if cfg!(target_os = "macos") {
        &["/opt/homebrew/bin/bash", "/usr/local/bin/bash"]
    } else {
        &["/bin/bash", "/usr/bin/bash"]
    }) else {
        eprintln!("no bash of 4 or later here");
        return;
    };
    let home = home("bash", &[(".bashrc", "PS1='top\\n> '\n")]);
    // Readline draws a prompt's last row alone again at a new size, and
    // clears its first, of a blank row before it or not.
    spaces_its_blocks(&shell(&bash, &home), "");
}

#[test]
fn fish_draws_its_lines_and_its_prompts_past_the_blank_rows() {
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
            "function fish_prompt; echo top; echo -n '> '; end\nfunction fish_greeting; end\n",
        )],
    );
    spaces_its_blocks(&shell(&fish, &home), "top");
}
