use std::sync::atomic::AtomicUsize;

use saetta_pty::Program;
use saetta_vt_core::input::{Key, KeyAction, Mods};

use super::*;

fn options(program: &str, args: &[&str]) -> Options {
    Options::new(saetta_pty::Options::new(
        Program::command(program, args.iter().copied()),
        "xterm-256color",
    ))
}

fn sh(script: &str) -> Options {
    options("/bin/sh", &["-c", script])
}

/// Waits, ten seconds at most, until `done` holds of the terminal.
fn wait_for(session: &Session, what: &str, done: impl Fn(&Terminal) -> bool) {
    let start = Instant::now();
    while !done(&session.lock()) {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "{what} never came"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

/// Whether the screen shows `text` in one of its rows.
fn shows(terminal: &Terminal, text: &str) -> bool {
    (0..terminal.rows()).any(|y| terminal.row_text(y).contains(text))
}

#[test]
fn under_a_flood_another_thread_takes_the_lock_between_slices() {
    let session = Session::spawn(&options("yes", &[]), || {}).unwrap();
    wait_for(&session, "the flood", |terminal| shows(terminal, "y"));
    let start = session.slices();
    let mut most = 0;
    for _ in 0..500 {
        // Counted from the moment this thread has its place.
        let mut before = 0;
        let terminal = session
            .shared
            .terminal
            .lock_after(|| before = session.slices());
        let after = session.slices();
        drop(terminal);
        most = most.max(after - before);
        // As a renderer would, now and then.
        thread::sleep(Duration::from_micros(200));
    }
    // At most the slice under way went first.
    assert!(most <= 1, "waited {most} slices");
    assert!(session.slices() - start > 10, "the flood went on");
}

#[test]
fn the_program_gets_the_reply_to_its_query() {
    // In raw mode, the reply comes without a newline; ESC shows as E.
    let script = r#"stty raw -echo; printf '\033[c'; reply=$(dd bs=1 count=9 2>/dev/null); stty sane; printf 'reply:%s\n' "$reply" | tr '\033' E"#;
    let session = Session::spawn(&sh(script), || {}).unwrap();
    wait_for(&session, "the reply", |terminal| {
        shows(terminal, "reply:E[?62;22c")
    });
}

#[test]
fn a_synchronized_update_ends_at_its_deadline_which_each_update_restarts() {
    let mut options = sh(r#"printf '\033[?2026h'; sleep 0.6; printf '\033[?2026h'; sleep 30"#);
    options.sync_timeout = Duration::from_secs(1);
    let session = Session::spawn(&options, || {}).unwrap();
    wait_for(&session, "the first update", |terminal| {
        terminal.synchronized_updates() == 1
    });
    wait_for(&session, "the second update", |terminal| {
        terminal.synchronized_updates() == 2
    });
    let second = Instant::now();
    // The first update's deadline has passed by now, but the second
    // started its own.
    thread::sleep(Duration::from_millis(600));
    assert!(session.lock().synchronized_output(), "the update holds");
    wait_for(&session, "the deadline", |terminal| {
        !terminal.synchronized_output()
    });
    let held = second.elapsed();
    assert!(held >= Duration::from_millis(900), "ended after {held:?}");
}

#[test]
fn the_main_thread_is_woken_once_until_it_looks() {
    let wakes = Arc::new(AtomicUsize::new(0));
    let session = {
        let wakes = Arc::clone(&wakes);
        // Several slices of output before the main thread looks.
        let script = "echo one; sleep 0.05; echo more; sleep 0.05; echo done; read line; echo two; read line";
        Session::spawn(&sh(script), move || {
            wakes.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap()
    };
    wait_for(&session, "the output", |terminal| shows(terminal, "done"));
    assert!(session.slices() >= 3, "{} slices", session.slices());
    assert_eq!(wakes.load(Ordering::SeqCst), 1, "once, however many slices");
    session.clear_wake();
    session.write("\n");
    wait_for(&session, "two", |terminal| shows(terminal, "two"));
    let start = Instant::now();
    while wakes.load(Ordering::SeqCst) < 2 {
        assert!(start.elapsed() < Duration::from_secs(10));
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn the_end_of_the_output_is_told_with_the_exit() {
    let wakes = Arc::new(AtomicUsize::new(0));
    let mut session = {
        let wakes = Arc::clone(&wakes);
        Session::spawn(&sh("exit 3"), move || {
            wakes.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap()
    };
    let start = Instant::now();
    while !session.has_ended() {
        assert!(start.elapsed() < Duration::from_secs(10));
        thread::sleep(Duration::from_millis(10));
    }
    assert!(wakes.load(Ordering::SeqCst) >= 1);
    let start = Instant::now();
    loop {
        if let Some(exit) = session.exit().unwrap() {
            assert_eq!(exit, Exit::Code(3));
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(10));
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn keys_pastes_and_resizes_reach_the_program() {
    // In raw mode, which keeps a long line whole, where the canonical mode
    // cuts it at 1024 bytes on macOS.
    let script = r#"stty raw -echo; echo armed; data=$(head -c 12290); stty sane -echo; printf 'got:%s:%s\n' "$(printf %s "$data" | tr -d b | tr '\r' R)" "${#data}"; trap 'echo "size:$(stty size)"' WINCH; echo ready; while :; do sleep 0.05; done"#;
    let session = Session::spawn(&sh(script), || {}).unwrap();
    // Input before raw mode would meet the canonical mode's limit.
    wait_for(&session, "raw mode", |terminal| shows(terminal, "armed"));
    let key = |c| KeyEvent {
        key: Key::Char(c),
        action: KeyAction::Press,
        mods: Mods::NONE,
        text: "a",
        unmodified: "a",
        base_layout: None,
    };
    session.key(&key('a'), KeyOptions::default());
    // A paste longer than one write, and a line break, which a paste
    // without the bracket sends as CR.
    session.paste(&format!("{}\n", "b".repeat(3 * PASTE_CHUNK)));
    wait_for(&session, "the input", |terminal| {
        shows(terminal, "got:aR:12290")
    });
    wait_for(&session, "ready", |terminal| shows(terminal, "ready"));
    session
        .resize(Size {
            cols: 100,
            rows: 30,
            width: 0,
            height: 0,
        })
        .unwrap();
    {
        let terminal = session.lock();
        assert_eq!((terminal.cols(), terminal.rows()), (100, 30));
    }
    wait_for(&session, "the new size", |terminal| {
        shows(terminal, "size:30 100")
    });
}

#[test]
fn output_during_a_synchronized_update_does_not_put_off_its_deadline() {
    let mut options = sh(r#"printf '\033[?2026h'; while :; do echo x; sleep 0.05; done"#);
    options.sync_timeout = Duration::from_millis(300);
    let session = Session::spawn(&options, || {}).unwrap();
    wait_for(&session, "the update", |terminal| {
        terminal.synchronized_updates() == 1
    });
    let start = Instant::now();
    wait_for(&session, "the deadline", |terminal| {
        !terminal.synchronized_output()
    });
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[test]
fn dropping_the_session_ends_its_threads() {
    let session = Session::spawn(&sh("echo ready; while :; do sleep 0.05; done"), || {}).unwrap();
    wait_for(&session, "ready", |terminal| shows(terminal, "ready"));
    session.write("queued, never read\n");
    let shared = Arc::downgrade(&session.shared);
    drop(session);
    let start = Instant::now();
    while shared.upgrade().is_some() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "a thread is still there"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_write_that_blocks_does_not_keep_an_update_from_ending() {
    // A program in raw mode that starts an update, then reads nothing.
    let mut options = sh(r#"stty raw -echo; printf '\033[?2026h'; sleep 30"#);
    options.sync_timeout = Duration::from_millis(300);
    let session = Session::spawn(&options, || {}).unwrap();
    wait_for(&session, "the update", |terminal| {
        terminal.synchronized_updates() == 1
    });
    // More than the terminal's input holds.
    session.write(vec![b'x'; 64 * 1024]);
    wait_for(&session, "the deadline", |terminal| {
        !terminal.synchronized_output()
    });
}

#[test]
fn an_update_that_an_exited_program_left_ends_with_its_output() {
    // A deadline that the test does not wait for.
    let mut options = sh(r#"printf 'last words\033[?2026h'; exit 0"#);
    options.sync_timeout = Duration::from_secs(60);
    let session = Session::spawn(&options, || {}).unwrap();
    let start = Instant::now();
    while !session.has_ended() {
        assert!(start.elapsed() < Duration::from_secs(10));
        thread::sleep(Duration::from_millis(5));
    }
    assert!(!session.lock().synchronized_output());
    // A key for a program that has gone changes nothing.
    session.write("q");
    thread::sleep(Duration::from_millis(50));
    assert!(shows(&session.lock(), "last words"));
}

#[test]
fn a_reply_never_lands_inside_a_bracketed_paste() {
    let out = std::env::temp_dir().join(format!("saetta-paste-{}", std::process::id()));
    let _ = std::fs::remove_file(&out);
    // Bracketed paste on, raw mode, what is typed saved to `out`, and a
    // stream of DA1 queries, as a program that polls the terminal sends.
    let script = format!(
        r#"stty raw -echo; printf '\033[?2004h'; (i=0; while [ $i -lt 100000 ]; do printf '\033[c'; i=$((i+1)); done) & echo armed; exec cat > '{}'"#,
        out.display()
    );
    let session = Session::spawn(&sh(&script), || {}).unwrap();
    wait_for(&session, "armed", |terminal| shows(terminal, "armed"));
    session.paste(&"b".repeat(1 << 20));
    let start = Instant::now();
    let typed = loop {
        let typed = std::fs::read(&out).unwrap_or_default();
        if typed.windows(6).any(|w| w == b"\x1b[201~") {
            break typed;
        }
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "the paste never ended"
        );
        thread::sleep(Duration::from_millis(20));
    };
    drop(session);
    let _ = std::fs::remove_file(&out);
    let open = typed.windows(6).position(|w| w == b"\x1b[200~").unwrap() + 6;
    let close = typed.windows(6).position(|w| w == b"\x1b[201~").unwrap();
    assert_eq!(
        close - open,
        1 << 20,
        "nothing but the paste between its brackets"
    );
    assert!(typed[open..close].iter().all(|&b| b == b'b'));
}

/// Tells, as it drops, that the session's shared state, which holds the
/// `wake` closure that owns it, has gone: both threads have ended.
struct Freed(std::sync::mpsc::Sender<()>);

impl Drop for Freed {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

#[test]
fn dropping_a_session_ends_a_program_that_ignores_sighup() {
    let (sender, freed) = std::sync::mpsc::channel();
    let guard = Freed(sender);
    // A program that survives SIGHUP but ends once its reads see the end,
    // as on a terminal that closes.
    let script = r#"trap '' HUP; echo ready; while read line; do :; done"#;
    let session = Session::spawn(&sh(script), move || {
        let _ = &guard;
    })
    .unwrap();
    wait_for(&session, "ready", |terminal| shows(terminal, "ready"));
    drop(session);
    assert!(
        freed.recv_timeout(Duration::from_secs(10)).is_ok(),
        "a thread is still there"
    );
}
