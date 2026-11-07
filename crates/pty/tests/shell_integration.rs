//! The shells' integration, of each shell that the machine has: zsh, bash
//! and fish started in a PTY load it, the user's own files too, of a home
//! of the test's, and tell OSC 133's marks and OSC 7's folder.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use lokyr_pty::{Options, Program, Pty, PtyWriter, spawn};

/// The scripts, as the source tree has them.
fn scripts() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("shell-integration")
}

/// A home of the test's, made anew, of `files`, each of its path in it and
/// its text, and a folder of a space in its name.
fn home(test: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("lokyr-shells-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("a b")).unwrap();
    for (path, text) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    std::fs::canonicalize(root).unwrap()
}

/// The first of `paths` that is there.
fn found(paths: &[&str]) -> Option<PathBuf> {
    paths.iter().map(PathBuf::from).find(|path| path.is_file())
}

/// What a shell writes, read on a thread of its own; the terminal's
/// answer to its query of the primary device attributes, which fish waits
/// for before its prompt, is written back.
struct Shell {
    _pty: Pty,
    writer: PtyWriter,
    chunks: mpsc::Receiver<Vec<u8>>,
    text: String,
    /// Where the text of the last line typed starts.
    from: usize,
}

impl Shell {
    /// The shell at `path`, of the integration, in `home`, its folder too.
    fn start(path: &Path, home: &Path) -> Self {
        let mut options = Options::new(
            Program::command(path, Vec::<String>::new()),
            "xterm-256color",
        );
        options.shell_integration = Some(scripts());
        options.working_directory = Some(home.to_owned());
        options.env = vec![
            ("HOME".into(), home.into()),
            ("BASH_SILENCE_DEPRECATION_WARNING".into(), "1".into()),
            ("ZDOTDIR".into(), home.into()),
        ];
        let pty = spawn(&options).unwrap();
        let mut reader = pty.reader().unwrap();
        let mut answer = pty.writer().unwrap();
        let (sender, chunks) = mpsc::channel();
        std::thread::spawn(move || {
            let mut buffer = [0; 4096];
            while let Ok(n @ 1..) = reader.read(&mut buffer) {
                if buffer[..n]
                    .windows(4)
                    .any(|query| query == b"\x1b[0c" || query == b"\x1b[c\x1b")
                {
                    let _ = answer.write_all(b"\x1b[?62;22c");
                }
                if sender.send(buffer[..n].to_vec()).is_err() {
                    break;
                }
            }
        });
        let writer = pty.writer().unwrap();
        Self {
            _pty: pty,
            writer,
            chunks,
            text: String::new(),
            from: 0,
        }
    }

    /// Everything read since the last line typed, until `needle` came.
    fn until(&mut self, needle: &str) -> &str {
        while !self.text[self.from..].contains(needle) {
            match self.chunks.recv_timeout(Duration::from_secs(10)) {
                Ok(chunk) => self.text.push_str(&String::from_utf8_lossy(&chunk)),
                Err(_) => panic!("{needle:?} never came, after {:?}", &self.text[self.from..]),
            }
        }
        &self.text[self.from..]
    }

    /// Types `line` and Return, past what came before.
    fn run(&mut self, line: &str) {
        self.from = self.text.len();
        self.writer.write_all(line.as_bytes()).unwrap();
        self.writer.write_all(b"\r").unwrap();
    }
}

/// This machine's name, as the shells' URLs of OSC 7 give it.
fn host() -> String {
    lokyr_pty::host_name()
}

/// The marks of a command that ended of `status`, then of the next prompt,
/// in `folder`, its URL's, are in `text`, in their order.
fn marks_in_order(text: &str, status: i32, folder: &str) {
    let marks = [
        "\x1b]133;C".to_owned(),
        format!("\x1b]133;D;{status}"),
        format!("\x1b]7;file://{}{folder}", host()),
        "\x1b]133;A".to_owned(),
        "\x1b]133;B".to_owned(),
    ];
    let mut at = 0;
    for mark in &marks {
        let Some(found) = text[at..].find(mark.as_str()) else {
            panic!("{mark:?} after {at} in {text:?}");
        };
        at += found + mark.len();
    }
}

/// `folder`'s path as a `file` URL's, its space escaped.
fn url_path(folder: &Path) -> String {
    folder.display().to_string().replace(' ', "%20")
}

#[test]
fn zsh_loads_the_integration_after_the_user_s_own_files() {
    let Some(zsh) = found(&["/bin/zsh", "/usr/bin/zsh", "/opt/homebrew/bin/zsh"]) else {
        eprintln!("no zsh here");
        return;
    };
    // The user's files: a variable, an alias, and a theme's hook that sets
    // the prompt anew before each.
    let home = home(
        "zsh",
        &[
            (".zshenv", "export FROM_ZSHENV=env-read\n"),
            (
                ".zshrc",
                "alias hello='echo from-rc'\nprecmd() { PS1='theme> ' }\n",
            ),
        ],
    );
    let mut shell = Shell::start(&zsh, &home);
    shell.until("theme> ");
    shell.run("echo $FROM_ZSHENV; hello; echo ${LOKYR_ZSH_ZDOTDIR-none} $ZDOTDIR; false");
    shell.until("from-rc");
    let text = shell.until("\x1b]133;D;1").to_owned();
    assert!(text.contains("env-read"), "{text:?}");
    // The user's own ZDOTDIR back, as the test gave it.
    assert!(
        text.contains(&format!("none {}", home.display())),
        "{text:?}"
    );
    let text = shell.until("theme> \x1b]133;B").to_owned();
    marks_in_order(&text, 1, &url_path(&home));
    // A folder of a space, escaped in its URL.
    shell.run("cd 'a b'");
    let folder = url_path(&home.join("a b"));
    shell.until(&format!("\x1b]7;file://{}{folder}\x07", host()));
}

#[test]
fn bash_reads_the_user_s_files_out_of_posix_mode_and_marks_its_prompts() {
    // A bash of 4 or later, as macOS's own, 3.2, gets no integration.
    let paths: &[&str] = if cfg!(target_os = "macos") {
        &["/opt/homebrew/bin/bash", "/usr/local/bin/bash"]
    } else {
        &["/bin/bash", "/usr/bin/bash"]
    };
    let Some(bash) = found(paths) else {
        eprintln!("no bash of 4 or later here");
        return;
    };
    // An interactive bash that is no login shell reads `.bashrc`, of a
    // prompt command of the user's.
    let home = home(
        "bash",
        &[(
            ".bashrc",
            "PS1='bash> '\nalias hello='echo from-rc'\nPROMPT_COMMAND='USER_PROMPT=yes'\n",
        )],
    );
    let mut shell = Shell::start(&bash, &home);
    shell.until("bash> ");
    shell.run("hello; set -o | grep '^posix'; echo ${ENV-unset} $USER_PROMPT; false");
    shell.until("from-rc");
    let text = shell.until("\x1b]133;D;1").to_owned();
    // Out of POSIX mode, as the user's bash is.
    let posix = text
        .split("\r\n")
        .find(|line| line.starts_with("posix"))
        .unwrap_or_default();
    assert!(posix.ends_with("off"), "{text:?}");
    assert!(text.contains("unset yes"), "{text:?}");
    let text = shell.until("bash> \x1b]133;B").to_owned();
    marks_in_order(&text, 1, &url_path(&home));
    shell.run("cd 'a b'");
    let folder = url_path(&home.join("a b"));
    shell.until(&format!("\x1b]7;file://{}{folder}\x07", host()));
}

#[test]
fn fish_takes_the_folder_out_of_its_data_dirs_and_marks_its_prompts() {
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
            "function fish_prompt; echo -n 'fish> '; end\nalias hello 'echo from-rc'\n",
        )],
    );
    let mut shell = Shell::start(&fish, &home);
    shell.until("fish> ");
    shell.run("hello; set -q LOKYR_SHELL_INTEGRATION_XDG_DIR; or echo dirs=(string join , $XDG_DATA_DIRS)=; false");
    shell.until("from-rc");
    let text = shell.until("\x1b]133;D;1").to_owned();
    // The terminal's folder gone from the data's folders, its variable too.
    let dirs = text.split("dirs=").nth(1).unwrap_or_default();
    let dirs = dirs.split('=').next().unwrap_or_default();
    assert!(!dirs.is_empty() || text.contains("dirs=="), "{text:?}");
    assert!(!dirs.contains("shell-integration"), "{dirs:?}");
    let text = shell.until("fish> ").to_owned();
    marks_in_order(&text, 1, &url_path(&home));
    // One mark of each, not fish's and the integration's both.
    let prompt = text.split("\x1b]133;D;1").nth(1).unwrap_or_default();
    assert_eq!(prompt.matches("\x1b]133;A").count(), 1, "{prompt:?}");
}

#[test]
fn macos_s_own_bash_loads_nothing() {
    if !cfg!(target_os = "macos") {
        return;
    }
    let home = home("old-bash", &[(".bashrc", "PS1='old> '\n")]);
    let mut shell = Shell::start(Path::new("/bin/bash"), &home);
    shell.until("old> ");
    shell.run("echo ${ENV-unset} ${LOKYR_BASH_INJECT-unset}");
    let text = shell.until("old> ").to_owned();
    assert!(text.contains("unset unset"), "{text:?}");
    assert!(!shell.text.contains("\x1b]133;"), "{:?}", shell.text);
}

#[test]
fn another_shell_or_a_shell_of_arguments_loads_nothing() {
    let home = home("other", &[]);
    let mut options = Options::new(
        Program::command(
            "/bin/sh",
            [
                "-c",
                "echo ${ZDOTDIR-none} ${ENV-none} ${LOKYR_BASH_INJECT-none}",
            ],
        ),
        "xterm-256color",
    );
    options.shell_integration = Some(scripts());
    options.working_directory = Some(home.clone());
    let pty = spawn(&options).unwrap();
    let mut reader = pty.reader().unwrap();
    let mut text = String::new();
    let mut buffer = [0; 4096];
    while let Ok(n @ 1..) = reader.read(&mut buffer) {
        text.push_str(&String::from_utf8_lossy(&buffer[..n]));
        if text.contains('\n') {
            break;
        }
    }
    assert!(text.starts_with("none none none"), "{text:?}");
    let _ = (pty, Path::new(&home));
}
