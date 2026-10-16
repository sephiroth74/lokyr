//! Programs started in a PTY, as a shell sees them.

use std::io::{Read, Write};
use std::sync::mpsc;
use std::time::Duration;

use saetta_pty::{Options, Program, PtyReader, Size, spawn};

/// What the program writes, read on a thread of its own so that a test
/// gives up rather than hang.
struct Output {
    chunks: mpsc::Receiver<Vec<u8>>,
    text: String,
}

impl Output {
    fn new(mut reader: PtyReader) -> Self {
        let (sender, chunks) = mpsc::channel();
        std::thread::spawn(move || {
            let mut buffer = [0; 4096];
            while let Ok(n @ 1..) = reader.read(&mut buffer) {
                if sender.send(buffer[..n].to_vec()).is_err() {
                    break;
                }
            }
        });
        Self {
            chunks,
            text: String::new(),
        }
    }

    /// Everything read until `needle` came.
    fn until(&mut self, needle: &str) -> &str {
        while !self.text.contains(needle) {
            match self.chunks.recv_timeout(Duration::from_secs(10)) {
                Ok(chunk) => self.text.push_str(&String::from_utf8_lossy(&chunk)),
                Err(_) => panic!("{needle:?} never came, after {:?}", self.text),
            }
        }
        &self.text
    }

    /// Everything read until the end, once the program has gone.
    fn rest(mut self) -> String {
        while let Ok(chunk) = self.chunks.recv_timeout(Duration::from_secs(10)) {
            self.text.push_str(&String::from_utf8_lossy(&chunk));
        }
        self.text
    }
}

fn sh(script: &str) -> Program {
    Program::command("/bin/sh", ["-c", script])
}

#[test]
fn a_shell_reads_its_terminal_and_term() {
    let script = r#"tty; echo "$TERM"; echo controlling > /dev/tty; read line; echo "got $line""#;
    let pty = spawn(&Options::new(sh(script), "xterm-test")).unwrap();
    let mut output = Output::new(pty.reader().unwrap());
    let text = output.until("controlling\r\n").to_owned();
    let lines: Vec<&str> = text.split("\r\n").collect();
    assert_eq!(lines[0], pty.tty_name().to_str().unwrap(), "the tty");
    assert!(lines[0].starts_with("/dev/"));
    assert_eq!(lines[1], "xterm-test");
    // Writing to /dev/tty works only with a controlling terminal, which
    // is the PTY, since it came back here.
    assert_eq!(lines[2], "controlling");
    let pid = pty.process_id();
    // SAFETY: `getsid` only reads the session of a process.
    let session = unsafe { libc::getsid(pid as libc::pid_t) };
    assert_eq!(session, pid as libc::pid_t, "the shell leads its session");
    assert_eq!(
        pty.foreground_process_group(),
        Some(pid),
        "and its terminal's foreground"
    );
    pty.writer().unwrap().write_all(b"typed\n").unwrap();
    assert!(output.rest().contains("got typed\r\n"));
}

#[test]
fn the_program_gets_the_environment_and_the_size() {
    let dir = std::env::temp_dir().canonicalize().unwrap();
    let mut options = Options::new(
        sh(r#"echo "$COLORTERM $TERM_PROGRAM $SAETTA_RESOURCES_DIR"; pwd; stty size"#),
        "xterm-256color",
    );
    options.working_directory = Some(dir.clone());
    options.resources_dir = Some("/resources".into());
    options.size = Size {
        cols: 100,
        rows: 30,
        width: 800,
        height: 600,
    };
    let pty = spawn(&options).unwrap();
    // The end reads as the end, once the program has gone.
    let mut text = String::new();
    pty.reader().unwrap().read_to_string(&mut text).unwrap();
    let lines: Vec<&str> = text.split("\r\n").collect();
    assert_eq!(lines[0], "truecolor saetta /resources");
    assert_eq!(lines[1], dir.to_str().unwrap());
    assert_eq!(lines[2], "30 100");
}

#[test]
fn the_program_starts_at_home_with_only_the_terminal_open() {
    let script = r#"pwd; for fd in 3 4 5 6 7 8 9; do (: <&$fd) 2>/dev/null && echo "open $fd"; done; echo done"#;
    let pty = spawn(&Options::new(sh(script), "xterm-256color")).unwrap();
    let mut text = String::new();
    pty.reader().unwrap().read_to_string(&mut text).unwrap();
    let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
    let home = home.canonicalize().unwrap();
    // No descriptor past standard input, output and error: neither side of
    // the terminal leaks into the program.
    assert_eq!(text, format!("{}\r\ndone\r\n", home.display()));
}

#[test]
fn signals_ignored_here_have_their_default_action_in_the_program() {
    // SAFETY: ignoring SIGUSR1 installs no handler, and no test sends it to
    // this process.
    let before = unsafe { libc::signal(libc::SIGUSR1, libc::SIG_IGN) };
    let pty = spawn(&Options::new(
        sh(r#"echo started; kill -USR1 $$; echo survived"#),
        "xterm-256color",
    ));
    // SAFETY: as above, putting back what was there.
    unsafe { libc::signal(libc::SIGUSR1, before) };
    let text = Output::new(pty.unwrap().reader().unwrap()).rest();
    assert!(text.contains("started"));
    assert!(!text.contains("survived"), "SIGUSR1 ended the shell");
}

#[test]
fn a_program_that_cannot_start_is_an_error() {
    let options = Options::new(Program::command("/nonexistent/program", ["x"]), "xterm");
    let error = spawn(&options).unwrap_err();
    assert!(
        error.to_string().contains("/nonexistent/program"),
        "{error}"
    );
}

#[test]
#[ignore = "starts the user's login shell, with their configuration"]
fn the_login_shell_is_a_login_shell() {
    let pty = spawn(&Options::new(Program::LoginShell, "xterm-256color")).unwrap();
    let mut output = Output::new(pty.reader().unwrap());
    pty.writer()
        .unwrap()
        .write_all(b"echo \"login:$0\"; exit\r")
        .unwrap();
    let text = output.until("login:-").to_owned();
    assert!(text.contains("login:-"), "{text}");
}
