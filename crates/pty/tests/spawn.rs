//! Programs started in a PTY, as a shell sees them.

use std::io::{Read, Write};
use std::sync::mpsc;
use std::time::Duration;

use saetta_pty::{Exit, Options, Program, PtyReader, Size, spawn};

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

#[test]
fn a_resize_reaches_the_program() {
    let script = r#"trap 'echo "winch $(stty size)"' WINCH; echo "ready $(stty size)"; while :; do sleep 0.05; done"#;
    let pty = spawn(&Options::new(sh(script), "xterm-256color")).unwrap();
    let mut output = Output::new(pty.reader().unwrap());
    output.until("ready 24 80\r\n");
    let size = Size {
        cols: 120,
        rows: 40,
        width: 960,
        height: 640,
    };
    pty.resize(size).unwrap();
    assert_eq!(pty.size().unwrap(), size, "in pixels too");
    output.until("winch 40 120\r\n");
}

#[test]
fn the_exit_status_is_reported() {
    let mut pty = spawn(&Options::new(sh("exit 3"), "xterm-256color")).unwrap();
    // The end that the master reads, then the status.
    let mut text = String::new();
    pty.reader().unwrap().read_to_string(&mut text).unwrap();
    assert_eq!(pty.wait().unwrap(), Exit::Code(3));
    assert!(!Exit::Code(3).success());
    // Waiting again tells the same.
    assert_eq!(pty.try_wait().unwrap(), Some(Exit::Code(3)));
    // The session has ended: the terminal has no foreground group.
    assert_eq!(pty.foreground_process_group(), None);
    let mut pty = spawn(&Options::new(sh("kill -TERM $$"), "xterm-256color")).unwrap();
    assert_eq!(pty.wait().unwrap(), Exit::Signal(libc::SIGTERM));
}

#[test]
fn a_program_still_running_has_no_status() {
    let mut pty = spawn(&Options::new(sh("echo ready; read line"), "xterm-256color")).unwrap();
    let mut output = Output::new(pty.reader().unwrap());
    output.until("ready");
    assert_eq!(pty.try_wait().unwrap(), None);
    pty.writer().unwrap().write_all(b"\n").unwrap();
    assert_eq!(pty.wait().unwrap(), Exit::Code(0));
    assert!(Exit::Code(0).success());
}

/// Whether the process `pid` is gone, waited for: neither running nor a
/// zombie.
fn gone(pid: u32) -> bool {
    // SAFETY: signal 0 only checks that the process exists.
    let alive = unsafe { libc::kill(pid as libc::pid_t, 0) } == 0;
    !alive
}

#[test]
fn dropping_the_terminal_hangs_the_program_up_and_waits_for_it() {
    let pty = spawn(&Options::new(
        sh("echo ready; while :; do sleep 0.05; done"),
        "xterm-256color",
    ))
    .unwrap();
    let pid = pty.process_id();
    // A reader that still has the terminal open does not keep the program
    // alive.
    let mut output = Output::new(pty.reader().unwrap());
    output.until("ready");
    drop(pty);
    let start = std::time::Instant::now();
    while !gone(pid) {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "{pid} is still there"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    // And the reader reads the end.
    output.rest();
}

#[test]
fn dropping_the_terminal_hangs_up_the_job_in_its_foreground() {
    // With job control, the shell runs `sleep` in a group of its own, in the
    // terminal's foreground. The shell catches SIGHUP, and a reader keeps the
    // terminal open, so only the hang-up of the job ends it.
    let script = "trap : HUP; set -m; sleep 30; echo after";
    // bash, whose job control works without a prompt, where dash's may not.
    let bash = Program::command("/bin/bash", ["-c", script]);
    let pty = spawn(&Options::new(bash, "xterm-256color")).unwrap();
    let mut output = Output::new(pty.reader().unwrap());
    let pid = pty.process_id();
    let start = std::time::Instant::now();
    let job = loop {
        match pty.foreground_process_group() {
            Some(group) if group != pid => break group,
            _ => {
                assert!(start.elapsed() < Duration::from_secs(10), "no job");
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    };
    drop(pty);
    // SAFETY: signal 0 only checks that the group has a process.
    let job_alive = || unsafe { libc::kill(-(job as libc::pid_t), 0) } == 0;
    let start = std::time::Instant::now();
    while job_alive() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "the job is still there"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    // The shell went on past its job.
    output.until("after");
}
