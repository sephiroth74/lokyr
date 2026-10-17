use std::sync::atomic::AtomicUsize;
use std::sync::mpsc;

use bolt_pty::Program;
use bolt_vt_core::input::{Key, KeyAction, Mods};

use super::*;
use crate::queue::REPLY_BYTES;

fn options(program: &str, args: &[&str]) -> Options {
    Options::new(bolt_pty::Options::new(
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
fn each_synchronized_update_has_a_deadline_of_its_own() {
    let mut options =
        sh(r#"printf '\033[?2026h'; sleep 0.6; printf '\033[?2026l\033[?2026h'; sleep 30"#);
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
fn a_start_during_an_update_does_not_put_its_deadline_off() {
    // A program that starts an update again and again, never ending it.
    let mut options = sh(r#"while :; do printf '\033[?2026h'; sleep 0.2; done"#);
    options.sync_timeout = Duration::from_secs(1);
    let session = Session::spawn(&options, || {}).unwrap();
    wait_for(&session, "the update", |terminal| {
        terminal.synchronized_output()
    });
    let start = Instant::now();
    wait_for(&session, "the deadline", |terminal| {
        !terminal.synchronized_output()
    });
    let held = start.elapsed();
    assert!(held < Duration::from_secs(3), "ended after {held:?}");
    assert_eq!(session.lock().synchronized_updates(), 1, "one update");
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
    let out = std::env::temp_dir().join(format!("bolt-paste-{}", std::process::id()));
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
    let pid = session.pty.process_id().to_string();
    drop(session);
    assert!(
        freed.recv_timeout(Duration::from_secs(10)).is_ok(),
        "a thread is still there"
    );
    // The program goes too, and is waited for: `ps` no longer finds it.
    let start = Instant::now();
    let running = || {
        std::process::Command::new("ps")
            .args(["-p", &pid])
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap()
            .success()
    };
    while running() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "the program is still there"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn the_replies_to_a_program_that_never_reads_wait_up_to_a_limit() {
    // In raw mode, which keeps what is typed until the program reads it, a
    // stream of DA1 queries, without a read.
    let script = r#"stty raw -echo; q=$(printf '\033[c'); exec yes "$q""#;
    let session = Session::spawn(&sh(script), || {}).unwrap();
    // Thousands of slices, whose replies would take MiBs.
    let start = Instant::now();
    while session.slices() < 5000 {
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "{} slices",
            session.slices()
        );
        thread::sleep(Duration::from_millis(10));
    }
    // A slice's replies take a few times its bytes at most.
    let waiting = session.shared.queue.waiting();
    assert!(waiting <= REPLY_BYTES + 4 * SLICE, "{waiting} bytes wait");
}

#[test]
fn a_panic_on_the_io_thread_ends_the_session() {
    let wakes = Arc::new(AtomicUsize::new(0));
    let session = {
        let wakes = Arc::clone(&wakes);
        Session::spawn(&sh("echo hello; sleep 30"), move || {
            assert!(
                wakes.fetch_add(1, Ordering::SeqCst) > 0,
                "the first wake panics"
            );
        })
        .unwrap()
    };
    let start = Instant::now();
    while !session.has_ended() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "the session goes on"
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        wakes.load(Ordering::SeqCst),
        2,
        "woken again as the thread ended"
    );
}

/// A renderer that draws slowly: it takes a snapshot when woken, then
/// spends `drawing` on it, without the lock, until it has drawn `frames`
/// frames or one for which `last` holds.
fn slow_renderer(
    session: &Arc<Session>,
    woken: mpsc::Receiver<()>,
    drawing: Duration,
    frames: usize,
    last: impl Fn(&[String]) -> bool + Send + 'static,
) -> thread::JoinHandle<Vec<Vec<String>>> {
    let session = Arc::clone(session);
    thread::spawn(move || {
        let mut snapshot = Snapshot::default();
        let mut seen: Vec<Vec<String>> = Vec::new();
        while seen.len() < frames && woken.recv_timeout(Duration::from_secs(10)).is_ok() {
            if session.snapshot(&mut snapshot).is_some() {
                let rows = (0..snapshot.rows())
                    .map(|y| line_text(snapshot.line(y)))
                    .collect();
                seen.push(rows);
                if last(seen.last().unwrap()) {
                    break;
                }
            }
            thread::sleep(drawing);
        }
        seen
    })
}

fn line_text(line: &bolt_vt_core::terminal::Line) -> String {
    use bolt_vt_core::page::Content;
    line.cells()
        .iter()
        .map(|cell| match cell.content() {
            Content::Char(c) | Content::Grapheme(c) if c != '\0' => c,
            _ => ' ',
        })
        .collect::<String>()
        .trim_end()
        .to_owned()
}

#[test]
fn a_renderer_that_falls_behind_draws_the_latest_state() {
    let (sender, woken) = mpsc::channel();
    let script = r#"i=0; while [ $i -lt 3000 ]; do echo "n $i"; i=$((i+1)); done; echo done"#;
    let session = Arc::new(
        Session::spawn(&sh(script), move || {
            let _ = sender.send(());
        })
        .unwrap(),
    );
    let renderer = slow_renderer(&session, woken, Duration::from_millis(30), 1000, |rows| {
        rows.iter().any(|row| row == "done")
    });
    wait_for(&session, "the end", |terminal| shows(terminal, "done"));
    let slices = session.slices();
    // Woken once more by the last change, the renderer catches up.
    let frames = renderer.join().unwrap();
    assert!(
        frames.len() < slices as usize,
        "{} frames for {slices} slices",
        frames.len()
    );
    // Each frame is one moment of the screen: its numbered rows follow
    // each other, with no row from another moment between them.
    for rows in &frames {
        let numbers: Vec<u32> = rows
            .iter()
            .filter_map(|row| row.strip_prefix("n ")?.parse().ok())
            .collect();
        assert!(
            numbers.windows(2).all(|pair| pair[1] == pair[0] + 1),
            "{rows:?}"
        );
    }
    let last = frames.last().unwrap();
    assert!(last.iter().any(|row| row == "done"), "{last:?}");
}

#[test]
fn the_renderer_does_not_hold_up_the_output() {
    let (sender, woken) = mpsc::channel();
    let session = Arc::new(
        Session::spawn(&options("yes", &[]), move || {
            let _ = sender.send(());
        })
        .unwrap(),
    );
    // A renderer that takes half a second to draw a frame, drawing as the
    // output goes on.
    let renderer = slow_renderer(&session, woken, Duration::from_millis(500), 2, |_| false);
    wait_for(&session, "the flood", |terminal| shows(terminal, "y"));
    let before = session.slices();
    thread::sleep(Duration::from_millis(300));
    assert!(
        session.slices() > before,
        "the output went on while the renderer drew"
    );
    drop(session);
    renderer.join().unwrap();
}

#[test]
fn a_snapshot_waits_for_a_synchronized_update_and_copies_only_changes() {
    let mut options = sh(
        r#"echo before; read line; printf '\033[?2026hhidden'; read line; printf '\033[?2026l'; read line"#,
    );
    options.sync_timeout = Duration::from_secs(60);
    let session = Session::spawn(&options, || {}).unwrap();
    wait_for(&session, "before", |terminal| shows(terminal, "before"));
    let mut snapshot = Snapshot::default();
    assert_eq!(
        session.snapshot(&mut snapshot),
        Some(24),
        "every row, at first"
    );
    assert_eq!(session.snapshot(&mut snapshot), Some(0), "nothing changed");
    session.write("\n");
    wait_for(&session, "the update", |terminal| {
        terminal.synchronized_output()
    });
    wait_for(&session, "hidden", |terminal| shows(terminal, "hidden"));
    assert_eq!(session.snapshot(&mut snapshot), None, "held");
    assert!((0..snapshot.rows()).all(|y| line_text(snapshot.line(y)) != "hidden"));
    session.write("\n");
    wait_for(&session, "the end of the update", |terminal| {
        !terminal.synchronized_output()
    });
    // The echo of the line break only moved the cursor.
    assert_eq!(
        session.snapshot(&mut snapshot),
        Some(1),
        "the row that changed"
    );
    assert!((0..snapshot.rows()).any(|y| line_text(snapshot.line(y)) == "hidden"));
}

#[test]
fn a_snapshot_lets_the_next_change_wake_the_main_thread() {
    let wakes = Arc::new(AtomicUsize::new(0));
    let session = {
        let wakes = Arc::clone(&wakes);
        Session::spawn(&sh("echo one; read line; echo two; read line"), move || {
            wakes.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap()
    };
    wait_for(&session, "one", |terminal| shows(terminal, "one"));
    let mut snapshot = Snapshot::default();
    session.snapshot(&mut snapshot);
    assert_eq!(wakes.load(Ordering::SeqCst), 1);
    session.write("\n");
    let start = Instant::now();
    while wakes.load(Ordering::SeqCst) < 2 {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "the change woke no one"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_wake_that_always_panics_ends_the_session_without_aborting() {
    // As a wake that sends to an event loop that has gone would.
    let session = Session::spawn(&sh("echo hello; sleep 30"), || {
        panic!("the event loop has gone");
    })
    .unwrap();
    let start = Instant::now();
    while !session.has_ended() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "the session goes on"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn replies_far_longer_than_their_queries_wait_up_to_a_limit() {
    // A title of 60 KiB, reported again and again, and never read.
    let script = r#"stty raw -echo; printf '\033]2;%s\007' "$(head -c 61440 /dev/zero | tr '\0' t)"; q=$(printf '\033[21t'); exec yes "$q""#;
    let mut options = sh(script);
    options.terminal.title_reports = true;
    let session = Session::spawn(&options, || {}).unwrap();
    let start = Instant::now();
    let mut most = 0;
    // Each slice's replies, before the limits, take megabytes: a hundred
    // fill the queue many times over.
    while session.slices() < 100 {
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "{} slices",
            session.slices()
        );
        // The replies that wait, and the one that the writer writes.
        most = most.max(session.shared.queue.replies());
        thread::sleep(Duration::from_millis(5));
    }
    // The queue's limit, and a take of the terminal's replies at most.
    assert!(most <= 2 * REPLY_BYTES + 4 * SLICE, "{most} bytes waited");
}
