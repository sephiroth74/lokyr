use std::sync::atomic::AtomicUsize;
use std::sync::mpsc;

use lokyr_pty::Program;
use lokyr_vt_core::input::{Key, KeyAction, ModifierKey, Mods};

use super::*;
use crate::queue::REPLY_BYTES;

fn options(program: &str, args: &[&str]) -> Options {
    Options::new(lokyr_pty::Options::new(
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
    // From before the program starts, so that the bound below holds however
    // late the test sees the first update start.
    let start = Instant::now();
    let session = Session::spawn(&options, || {}).unwrap();
    wait_for(&session, "the update", |terminal| {
        terminal.synchronized_output()
    });
    // The deadline ends the update, and the program's next start, at most
    // 0.2 s later, is a new one: the screen shows between them for too
    // short a time to be looked for, while the count of updates stays.
    // Starts that counted as updates would make the second come at once,
    // and starts that put the deadline off would keep it from coming.
    wait_for(&session, "the second update", |terminal| {
        terminal.synchronized_updates() == 2
    });
    let held = start.elapsed();
    assert!(held >= Duration::from_millis(900), "ended after {held:?}");
    assert!(held < Duration::from_secs(3), "ended after {held:?}");
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
fn the_output_s_bytes_are_counted() {
    let session =
        Session::spawn(&sh("head -c 1000 /dev/zero | tr '\\0' x; sleep 5"), || {}).unwrap();
    let start = Instant::now();
    while session.output_bytes() < 1000 {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "the output never came"
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(session.output_bytes(), 1000);
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
fn the_program_s_own_output_wakes_the_main_thread_again_once_found_quiet() {
    let wakes = Arc::new(AtomicUsize::new(0));
    let session = {
        let wakes = Arc::clone(&wakes);
        let script = "echo one; sleep 0.3; echo two; sleep 0.3; echo three; sleep 30";
        Session::spawn(&sh(script), move || {
            wakes.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap()
    };
    let wait_wakes = |count: usize| {
        let start = Instant::now();
        while wakes.load(Ordering::SeqCst) < count {
            assert!(start.elapsed() < Duration::from_secs(10), "no wake");
            thread::sleep(Duration::from_millis(10));
        }
    };
    // Its output of its own, as nothing asked it.
    wait_for(&session, "one", |terminal| shows(terminal, "one"));
    wait_wakes(1);
    let first = session.printed_at().expect("the first output's time");
    // The main thread takes what was told, and draws nothing; the program
    // prints on, of its own, which wakes no one while the main thread
    // has not found it quiet.
    session.clear_told();
    wait_for(&session, "two", |terminal| shows(terminal, "two"));
    thread::sleep(Duration::from_millis(50));
    assert_eq!(wakes.load(Ordering::SeqCst), 1);
    let second = session.printed_at().expect("the second output's time");
    assert!(second > first, "{second:?} after {first:?}");
    // Found quiet, its next output wakes it.
    session.clear_told();
    session.clear_printed();
    wait_for(&session, "three", |terminal| shows(terminal, "three"));
    wait_wakes(2);
    assert!(session.printed_at().is_some_and(|third| third > second));
}

#[test]
fn the_echo_of_input_and_the_answer_to_a_new_size_are_not_the_program_s_own() {
    let script = r#"trap 'echo resized' WINCH; read line; echo "got $line"; sleep 0.8; echo late; while :; do sleep 0.05; done"#;
    let session = Session::spawn(&sh(script), || {}).unwrap();
    // The line discipline's echo of the line, and the program's answer to
    // it.
    session.write("hello\n");
    wait_for(&session, "the answer", |terminal| {
        shows(terminal, "got hello")
    });
    assert_eq!(session.printed_at(), None);
    // What it prints a while after is its own.
    wait_for(&session, "late", |terminal| shows(terminal, "late"));
    let late = session.printed_at().expect("the late output's time");
    // Its redraw at a new size is not.
    session
        .resize(Size {
            cols: 100,
            rows: 30,
            width: 0,
            height: 0,
        })
        .unwrap();
    wait_for(&session, "the redraw", |terminal| {
        shows(terminal, "resized")
    });
    assert_eq!(session.printed_at(), Some(late));
}

/// A session whose program prints 60 lines, then `ready`, and waits for a
/// line before it prints `more`; its terminal has 80 columns and 24 rows.
fn scrolled_back(scroll_on_output: bool) -> Session {
    let mut options = sh("seq 1 60; echo ready; read line; echo more; sleep 10");
    options.terminal.scroll_on_output = scroll_on_output;
    let session = Session::spawn(&options, || {}).unwrap();
    wait_for(&session, "ready", |terminal| shows(terminal, "ready"));
    session.scroll(Scroll::Up(5));
    assert_eq!(session.lock().viewport_offset(), 5);
    session
}

#[test]
fn input_brings_the_view_back_to_the_screen() {
    let session = scrolled_back(false);
    let key = |key, text| KeyEvent {
        key,
        action: KeyAction::Press,
        mods: Mods::NONE,
        text,
        unmodified: text,
        base_layout: None,
    };
    // A modifier alone sends nothing, and leaves the view.
    session.key(
        &key(Key::Modifier(ModifierKey::LeftShift), ""),
        KeyOptions::default(),
    );
    assert_eq!(session.lock().viewport_offset(), 5);
    session.key(&key(Key::Char('a'), "a"), KeyOptions::default());
    assert_eq!(session.lock().viewport_offset(), 0);
    session.scroll(Scroll::Up(5));
    session.text("é");
    assert_eq!(session.lock().viewport_offset(), 0);
    session.scroll(Scroll::Up(5));
    session.paste("x");
    assert_eq!(session.lock().viewport_offset(), 0);
}

#[test]
fn output_brings_the_view_back_only_when_the_option_says() {
    for scroll_on_output in [false, true] {
        let session = scrolled_back(scroll_on_output);
        // A line that the program reads, written as it is, which leaves the
        // view: the program's output, which it echoes too, comes next.
        session.write(b"\n".to_vec());
        wait_for(&session, "more", |terminal| shows(terminal, "more"));
        let offset = session.lock().viewport_offset();
        if scroll_on_output {
            assert_eq!(offset, 0);
        } else {
            // The view keeps its rows, which the output pushed up.
            assert!(offset > 5, "{offset}");
        }
    }
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
fn output_that_never_stops_does_not_put_off_an_updates_deadline() {
    let mut options = sh(r#"printf '\033[?2026h'; exec yes"#);
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
    // More than the terminal's input holds: macOS's holds a few KiB,
    // Linux's some 640 KiB.
    session.write(vec![b'x'; 2 << 20]);
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
    let out = std::env::temp_dir().join(format!("lokyr-paste-{}", std::process::id()));
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
    // 8 MiB of queries, whose replies, two and a half times their bytes,
    // would take twenty times the limit, whatever the slices' sizes.
    let start = Instant::now();
    while session.output_bytes() < 8 << 20 {
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "{} bytes",
            session.output_bytes()
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

fn line_text(line: &lokyr_vt_core::terminal::Line) -> String {
    use lokyr_vt_core::page::Content;
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
    // Output of some 250 KB, past what a pseudo-terminal holds and past a
    // slice, so that it comes in slices however fast the system reads it.
    let script = r#"i=0; while [ $i -lt 30000 ]; do echo "n $i"; i=$((i+1)); done; echo done"#;
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
fn what_the_terminal_tells_wakes_again_once_taken_without_a_snapshot() {
    let wakes = Arc::new(AtomicUsize::new(0));
    let session = {
        let wakes = Arc::clone(&wakes);
        let script = "echo one; read line; echo two; read line; printf '\\a'; read line";
        Session::spawn(&sh(script), move || {
            wakes.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap()
    };
    let wait_wakes = |count: usize| {
        let start = Instant::now();
        while wakes.load(Ordering::SeqCst) < count {
            assert!(start.elapsed() < Duration::from_secs(10), "no wake");
            thread::sleep(Duration::from_millis(10));
        }
    };
    wait_for(&session, "one", |terminal| shows(terminal, "one"));
    wait_wakes(1);
    // The main thread takes what was told, and draws nothing: the grid's
    // change wakes no one, as no snapshot looked.
    {
        let mut terminal = session.lock();
        session.clear_told();
        terminal.take_events();
    }
    session.write("\n");
    wait_for(&session, "two", |terminal| shows(terminal, "two"));
    thread::sleep(Duration::from_millis(100));
    assert_eq!(wakes.load(Ordering::SeqCst), 1);
    // A bell does.
    session.write("\n");
    wait_wakes(2);
    assert_eq!(session.lock().bells(), 1);
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

#[test]
fn a_clipboard_answer_reaches_a_program_that_waits_for_it_in_silence() {
    // In raw mode, a read of the clipboard, and the answer read back, with
    // ESC as E and BEL as B.
    let script = r#"stty raw -echo; printf '\033]52;c;?\007'; r=$(dd bs=1 count=12 2>/dev/null); stty sane; printf 'got:%s\n' "$r" | tr '\033\007' EB"#;
    let mut options = sh(script);
    options.terminal.clipboard_reads = true;
    let session = Session::spawn(&options, || {}).unwrap();
    let start = Instant::now();
    let request = loop {
        let events = session.lock().take_events();
        if let Some(&terminal::Event::ClipboardRead(request)) = events.first() {
            break request;
        }
        assert!(start.elapsed() < Duration::from_secs(10), "no read");
        thread::sleep(Duration::from_millis(10));
    };
    session.reply_clipboard(request, b"hi");
    wait_for(&session, "the answer", |terminal| {
        shows(terminal, "got:E]52;c;aGk=B")
    });
}

#[test]
fn errors_leave_their_causes_to_their_sources() {
    let error = Error::Threads(io::Error::other("no threads"));
    assert_eq!(error.to_string(), "cannot start the session's threads");
    let source = std::error::Error::source(&error).map(ToString::to_string);
    assert_eq!(source.as_deref(), Some("no threads"));
    // The PTY's error, as the PTY tells it.
    let error = Error::from(lokyr_pty::Error::Open(io::Error::other("no terminals")));
    assert_eq!(error.to_string(), "cannot open a pseudo-terminal");
    let source = std::error::Error::source(&error).map(ToString::to_string);
    assert_eq!(source.as_deref(), Some("no terminals"));
}

#[test]
fn a_new_theme_s_scheme_reaches_a_program_that_asked_in_silence() {
    // In raw mode, the reports of the color scheme on, and the report read
    // back, with ESC as E, once the program tells that it waits.
    let script = r#"stty raw -echo; printf '\033[?2031hwaiting\r\n'; r=$(dd bs=1 count=9 2>/dev/null); stty sane; printf 'got:%s\n' "$r" | tr '\033' E"#;
    let session = Session::spawn(&sh(script), || {}).unwrap();
    wait_for(&session, "the program's wait", |terminal| {
        shows(terminal, "waiting")
    });
    // A light theme, as its background tells.
    let light = Colors {
        background: lokyr_vt_core::page::Rgb {
            r: 0xfd,
            g: 0xf6,
            b: 0xe3,
        },
        ..Colors::default()
    };
    session.set_default_colors(light);
    wait_for(&session, "the report", |terminal| {
        shows(terminal, "got:E[?997;2n")
    });
}
