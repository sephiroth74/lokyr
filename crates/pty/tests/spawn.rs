//! Programs started in a PTY, as a shell sees them.

use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::sync::mpsc;
use std::time::Duration;

use bolt_pty::{Exit, Options, Program, PtyReader, ReadOutcome, Size, spawn};

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
    // `[ -t 0 ]` rather than `tty`: macOS's `ttyname`, which `tty` calls,
    // looks for the terminal's name by reading `/dev` an entry at a time,
    // and at times misses one that is there. The name is checked below,
    // through the device that it opens.
    let script = r#"[ -t 0 ] && echo terminal; echo "$TERM"; echo controlling > /dev/tty; read line; echo "got $line""#;
    let pty = spawn(&Options::new(sh(script), "xterm-test")).unwrap();
    let mut output = Output::new(pty.reader().unwrap());
    let text = output.until("controlling\r\n").to_owned();
    let lines: Vec<&str> = text.split("\r\n").collect();
    assert_eq!(lines[0], "terminal", "the shell's input is a terminal");
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
    // The name is the terminal's: what is written to it comes out of the
    // master.
    assert!(pty.tty_name().starts_with("/dev/"));
    let mut named = std::fs::OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NOCTTY)
        .open(pty.tty_name())
        .unwrap();
    named.write_all(b"through its name\n").unwrap();
    drop(named);
    // What is typed reaches the shell's input, which the terminal is.
    pty.writer().unwrap().write_all(b"typed\n").unwrap();
    let rest = output.rest();
    assert!(rest.contains("through its name\r\n"), "{rest:?}");
    assert!(rest.contains("got typed\r\n"), "{rest:?}");
}

#[test]
fn the_program_gets_the_environment_and_the_size() {
    let dir = std::env::temp_dir().canonicalize().unwrap();
    let mut options = Options::new(
        sh(r#"echo "$COLORTERM $TERM_PROGRAM $BOLT_RESOURCES_DIR"; pwd; stty size"#),
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
    assert_eq!(lines[0], "truecolor bolt /resources");
    assert_eq!(lines[1], dir.to_str().unwrap());
    assert_eq!(lines[2], "30 100");
}

/// The soft limit of this process's descriptors.
fn descriptor_limit() -> u64 {
    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: `getrlimit` writes the limits into `limit`.
    let read = unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &raw mut limit) };
    assert_eq!(read, 0, "{}", std::io::Error::last_os_error());
    limit.rlim_cur
}

/// A copy of `file`'s descriptor at `at` or the lowest free one above it,
/// without the mark that closes it on exec, as one that a CI runner or a
/// launcher leaves open; closed when it drops.
struct Inherited(libc::c_int);

impl Inherited {
    fn new(file: &std::fs::File, at: libc::c_int) -> Self {
        use std::os::fd::AsRawFd;
        // SAFETY: `fcntl` duplicates the file's descriptor, which stays
        // open, and the copy has no mark.
        let fd = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_DUPFD, at) };
        assert!(fd >= at, "{}", std::io::Error::last_os_error());
        Self(fd)
    }
}

impl Drop for Inherited {
    fn drop(&mut self) {
        // SAFETY: the descriptor is this value's, and nothing else uses it.
        unsafe { libc::close(self.0) };
    }
}

#[test]
fn the_program_starts_at_home_with_only_the_terminal_open() {
    let limit = descriptor_limit();
    let file = std::fs::File::open("/dev/null").unwrap();
    // A descriptor below the soft limit, which is 256 in the shells that
    // launchd starts, and one past 65 536 where the limit allows it.
    let low = Inherited::new(&file, libc::c_int::try_from(limit.min(500) - 1).unwrap());
    let high = (limit > 70_000).then(|| Inherited::new(&file, 70_000));
    // Every descriptor that the program could have, past the standard ones,
    // as the tests running at once can open many, and the high one.
    let high_fd = high.as_ref().map_or(70_000, |high| high.0);
    let script = format!(
        r#"pwd; for fd in $(seq 3 1023) {high_fd}; do [ -e /dev/fd/$fd ] && echo "open $fd"; done; echo done"#
    );
    let pty = spawn(&Options::new(sh(&script), "xterm-256color")).unwrap();
    drop((low, high));
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
    // The message leaves the cause to the source, so that a chain of errors
    // tells it once.
    assert_eq!(error.to_string(), "cannot start /nonexistent/program");
    let source = std::error::Error::source(&error)
        .and_then(|source| source.downcast_ref::<std::io::Error>())
        .map(std::io::Error::kind);
    assert_eq!(source, Some(std::io::ErrorKind::NotFound), "{error:?}");
}

#[test]
fn a_pseudo_terminal_that_cannot_open_leaves_the_cause_to_the_source() {
    let error = bolt_pty::Error::Open(std::io::Error::other("no terminals"));
    assert_eq!(error.to_string(), "cannot open a pseudo-terminal");
    let source = std::error::Error::source(&error).map(ToString::to_string);
    assert_eq!(source.as_deref(), Some("no terminals"));
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
fn a_write_to_a_program_that_has_ended_fails() {
    let mut pty = spawn(&Options::new(sh("exit 0"), "xterm-256color")).unwrap();
    let mut text = String::new();
    pty.reader().unwrap().read_to_string(&mut text).unwrap();
    assert_eq!(pty.wait().unwrap(), Exit::Code(0));
    // No process has the terminal open: the bytes would go to no one, on
    // Linux as on macOS.
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    let error = pty
        .writer()
        .unwrap()
        .write_until(b"x", Some(deadline))
        .unwrap_err();
    assert_eq!(error.raw_os_error(), Some(libc::EIO), "{error}");
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
    // terminal's foreground. The shell catches SIGHUP, and a reader that
    // reads nothing keeps the terminal open, so only the hang-up of the job
    // ends it.
    let script = "trap : HUP; set -m; sleep 30; echo after";
    // bash, whose job control works without a prompt, where dash's may not.
    let bash = Program::command("/bin/bash", ["-c", script]);
    let pty = spawn(&Options::new(bash, "xterm-256color")).unwrap();
    let idle = pty.reader().unwrap();
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
    drop(idle);
}

#[test]
fn dropping_the_terminal_ends_its_readers_and_writers() {
    // A program that ignores SIGHUP and reads nothing, so that its input
    // fills up.
    let script = "trap '' HUP; stty raw -echo; echo ready; sleep 30";
    let pty = spawn(&Options::new(sh(script), "xterm-256color")).unwrap();
    let mut output = Output::new(pty.reader().unwrap());
    output.until("ready");
    let mut writer = pty.writer().unwrap();
    let writing = std::thread::spawn(move || writer.write_all(&[b'x'; 1 << 20]));
    std::thread::sleep(Duration::from_millis(100));
    assert!(!writing.is_finished(), "the program reads nothing");
    let pid = pty.process_id();
    drop(pty);
    // The blocked write fails, and the reader reads the end.
    let error = writing.join().unwrap().unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
    output.rest();
    // SAFETY: ending a test's own child, which would outlive the test.
    unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) };
}

#[test]
fn a_read_with_a_deadline_times_out() {
    let pty = spawn(&Options::new(sh("echo ready; sleep 30"), "xterm-256color")).unwrap();
    let mut reader = pty.reader().unwrap();
    let mut buffer = [0; 64];
    let mut text = Vec::new();
    while !text.ends_with(b"ready\r\n") {
        match reader.read_until(&mut buffer, None).unwrap() {
            ReadOutcome::Read(n) => text.extend_from_slice(&buffer[..n]),
            outcome => panic!("{outcome:?}"),
        }
    }
    let start = std::time::Instant::now();
    let deadline = start + Duration::from_millis(200);
    assert_eq!(
        reader.read_until(&mut buffer, Some(deadline)).unwrap(),
        ReadOutcome::TimedOut
    );
    assert!(start.elapsed() >= Duration::from_millis(200));
    assert_eq!(
        reader.read_until(&mut [], None).unwrap(),
        ReadOutcome::Read(0)
    );
}

#[test]
fn a_signal_blocked_where_the_program_starts_is_not_blocked_in_it() {
    let pty = std::thread::spawn(|| {
        // SAFETY: blocking SIGUSR2 in this thread alone, which ends here.
        unsafe {
            let mut set: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut set);
            libc::sigaddset(&mut set, libc::SIGUSR2);
            libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
        }
        spawn(&Options::new(
            sh("echo started; kill -USR2 $$; echo survived"),
            "xterm-256color",
        ))
        .unwrap()
    })
    .join()
    .unwrap();
    let text = Output::new(pty.reader().unwrap()).rest();
    assert!(text.contains("started"));
    assert!(!text.contains("survived"), "SIGUSR2 ended the shell");
}

#[test]
fn many_threads_open_terminals_at_once() {
    let threads: Vec<_> = (0..16)
        .map(|_| {
            std::thread::spawn(|| {
                for _ in 0..4 {
                    let mut pty = spawn(&Options::new(sh("exit 0"), "xterm-256color")).unwrap();
                    assert_eq!(pty.wait().unwrap(), Exit::Code(0));
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
}

#[test]
fn a_write_to_a_program_that_does_not_read_times_out() {
    let pty = spawn(&Options::new(
        sh("stty raw -echo; echo armed; sleep 30"),
        "xterm-256color",
    ))
    .unwrap();
    let mut output = Output::new(pty.reader().unwrap());
    output.until("armed");
    let mut writer = pty.writer().unwrap();
    // More than the terminal's input holds.
    let bytes = vec![b'x'; 1 << 20];
    let started = std::time::Instant::now();
    let mut written = 0;
    loop {
        let deadline = std::time::Instant::now() + Duration::from_millis(200);
        match writer
            .write_until(&bytes[written..], Some(deadline))
            .unwrap()
        {
            Some(n) => written += n,
            None => break,
        }
        assert!(written < bytes.len(), "the program took it all");
    }
    assert!(written > 0, "some of it fitted");
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(writer.write_until(&[], None).unwrap(), Some(0));
}

#[test]
fn a_deadline_that_has_passed_still_reads_and_writes_what_it_can() {
    let pty = spawn(&Options::new(
        sh("stty raw -echo; echo armed; exec cat"),
        "xterm-256color",
    ))
    .unwrap();
    let mut reader = pty.reader().unwrap();
    let mut buffer = [0; 64];
    let mut read = Vec::new();
    while !read.ends_with(b"armed\n") {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        match reader.read_until(&mut buffer, Some(deadline)).unwrap() {
            ReadOutcome::Read(n) => read.extend_from_slice(&buffer[..n]),
            outcome => panic!("{outcome:?} before the program was ready"),
        }
    }
    let past = std::time::Instant::now();
    std::thread::sleep(Duration::from_millis(5));
    let mut writer = pty.writer().unwrap();
    assert_eq!(writer.write_until(b"x", Some(past)).unwrap(), Some(1));
    // `cat` sends it back, which a read with the past deadline then takes.
    let started = std::time::Instant::now();
    loop {
        match reader.read_until(&mut buffer, Some(past)).unwrap() {
            ReadOutcome::Read(n) => {
                assert_eq!(&buffer[..n], b"x");
                break;
            }
            ReadOutcome::TimedOut => {}
            ReadOutcome::End => panic!("the end"),
        }
        assert!(started.elapsed() < Duration::from_secs(5), "never read");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// A new directory of the temporary one, named for the test.
fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("bolt-pty-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir(&dir).unwrap();
    dir
}

#[test]
fn a_working_directory_that_can_be_searched_but_not_read_is_entered() {
    use std::os::unix::fs::PermissionsExt;
    let dir = temp_dir("search");
    std::fs::write(dir.join("marker"), "inside\n").unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o100)).unwrap();
    let mut options = Options::new(sh("cat marker"), "xterm-256color");
    options.working_directory = Some(dir.clone());
    let read = spawn(&options).map(|pty| Output::new(pty.reader().unwrap()).rest());
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(read.unwrap().contains("inside"));
}

#[test]
fn a_working_directory_that_is_a_fifo_fails_without_waiting_for_a_writer() {
    let dir = temp_dir("fifo");
    let fifo = dir.join("fifo");
    let path = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
    // SAFETY: `path` is a C string, which lives through the call.
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    let (sender, receiver) = mpsc::channel();
    let working_directory = fifo.clone();
    std::thread::spawn(move || {
        let mut options = Options::new(sh("true"), "xterm-256color");
        options.working_directory = Some(working_directory);
        sender.send(spawn(&options).map(drop)).unwrap();
    });
    let failed = receiver.recv_timeout(Duration::from_secs(5));
    if failed.is_err() {
        // Lets the open that waits go, so that the thread ends.
        let _ = std::fs::OpenOptions::new().write(true).open(&fifo);
    }
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(
        matches!(failed, Ok(Err(bolt_pty::Error::WorkingDirectory { .. }))),
        "{failed:?}"
    );
}

#[test]
fn a_program_found_in_path_keeps_its_name_as_given() {
    let pty = spawn(&Options::new(
        Program::command("sh", ["-c", r#"echo "arg0:$0""#]),
        "xterm-256color",
    ))
    .unwrap();
    let text = Output::new(pty.reader().unwrap()).rest();
    assert!(text.contains("arg0:sh\r\n"), "{text:?}");
}

#[test]
fn a_wait_for_either_tells_output_and_room() {
    let pty = spawn(&Options::new(
        sh("stty raw -echo; echo armed; exec cat"),
        "xterm-256color",
    ))
    .unwrap();
    let mut reader = pty.reader().unwrap();
    let mut writer = pty.writer().unwrap();
    let mut buffer = [0; 64];
    let mut read = Vec::new();
    while !read.ends_with(b"armed\n") {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        match reader.read_until(&mut buffer, Some(deadline)).unwrap() {
            ReadOutcome::Read(n) => read.extend_from_slice(&buffer[..n]),
            outcome => panic!("{outcome:?} before the program was ready"),
        }
    }
    let soon = || Some(std::time::Instant::now() + Duration::from_millis(200));
    // Room, and nothing to read yet.
    let ready = reader.wait_either(&writer, soon()).unwrap();
    assert!(ready.room && !ready.output, "{ready:?}");
    // `cat` sends back what is typed; room comes at once, and the output
    // after it.
    assert_eq!(writer.write_until(b"x", None).unwrap(), Some(1));
    let started = std::time::Instant::now();
    while !reader.wait_either(&writer, soon()).unwrap().output {
        assert!(started.elapsed() < Duration::from_secs(5), "no output");
        std::thread::sleep(Duration::from_millis(10));
    }
    // Both, once the PTY has dropped.
    drop(pty);
    let ready = reader.wait_either(&writer, soon()).unwrap();
    assert!(ready.room && ready.output, "{ready:?}");
}
