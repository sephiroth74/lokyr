//! The headless terminal, driving programs as a test does.

use std::path::PathBuf;
use std::time::Duration;

use bolt_replay::harness::{Event, Exit, Harness, HarnessOptions};
use bolt_vt_core::terminal::{Event as TerminalEvent, Options};

const LIMIT: Duration = Duration::from_secs(10);

/// The options of a harness of `cols` by `rows` that runs `script` with
/// `sh`.
fn options(cols: u16, rows: u16, script: &str) -> HarnessOptions {
    HarnessOptions {
        program: vec!["sh".to_owned(), "-c".to_owned(), script.to_owned()],
        env: vec![("PATH".to_owned(), "/usr/bin:/bin".to_owned())],
        terminal: Options::default(),
        dir: PathBuf::from("/"),
        cols,
        rows,
    }
}

/// A harness of `cols` by `rows` that runs `script` with `sh`.
fn harness(cols: u16, rows: u16, script: &str) -> Harness {
    Harness::spawn(&options(cols, rows, script)).unwrap()
}

/// The bytes typed into the program, as they were written.
fn inputs(harness: &Harness) -> Vec<&[u8]> {
    harness
        .events()
        .iter()
        .filter_map(|(_, event)| match event {
            Event::Input(bytes) => Some(&bytes[..]),
            _ => None,
        })
        .collect()
}

#[test]
fn a_program_that_sends_da1_reads_the_reply() {
    // In raw mode, the reply comes without a newline; ESC shows as E.
    let script = r#"stty raw -echo; printf '\033[c'; r=$(dd bs=1 count=9 2>/dev/null); stty sane; printf 'reply:%s\n' "$r" | tr '\033' E"#;
    let mut harness = harness(40, 3, script);
    harness.wait("reply:E[?62;22c", LIMIT).unwrap();
    harness.exit(LIMIT).unwrap();
    assert_eq!(inputs(&harness), [b"\x1b[?62;22c"]);
}

#[test]
fn keys_go_to_the_program_and_events_come_in_order() {
    let mut harness = harness(20, 3, "echo ready; read line; echo \"got $line\"");
    harness.wait("ready", LIMIT).unwrap();
    harness.mark("typing").unwrap();
    harness.send(b"hello\r", LIMIT).unwrap();
    harness.wait("got hello", LIMIT).unwrap();
    harness.exit(LIMIT).unwrap();
    assert!(harness.shows("got hello"));
    assert_eq!(harness.screen().split('\n').count(), 3, "a line a row");
    let kinds: Vec<&str> = harness
        .events()
        .iter()
        .map(|(_, event)| match event {
            Event::Output(_) => "output",
            Event::Input(_) => "input",
            Event::Mark(_) => "mark",
        })
        .collect();
    let mark = kinds.iter().position(|&kind| kind == "mark").unwrap();
    assert_eq!(kinds[mark + 1], "input");
    assert!(kinds[..mark].contains(&"output"));
    assert!(kinds[mark + 1..].contains(&"output"));
    let times: Vec<Duration> = harness.events().iter().map(|&(at, _)| at).collect();
    assert!(times.windows(2).all(|pair| pair[0] <= pair[1]));
    assert!(String::from_utf8_lossy(&harness.output()).contains("got hello"));
}

#[test]
fn a_screen_that_shows_a_forbidden_text_fails_the_read() {
    let mut harness = harness(20, 3, "echo ready; sleep 0.2; echo secret; sleep 5");
    harness.forbid(vec!["secret".to_owned()]);
    harness.wait("ready", LIMIT).unwrap();
    let error = harness.sleep(Duration::from_secs(2)).unwrap_err();
    assert!(error.to_string().contains("showed `secret`"), "{error:#}");
}

#[test]
fn a_program_that_goes_on_is_hung_up_and_its_screen_stays() {
    let mut harness = harness(20, 3, "echo still here; sleep 30");
    harness.wait("still here", LIMIT).unwrap();
    harness.hang_up();
    assert!(harness.shows("still here"));
    // The output ends, as the terminal has closed.
    harness.exit(Duration::from_secs(5)).unwrap();
}

#[test]
fn a_program_that_ends_with_a_reply_queued_ends_the_steps_cleanly() {
    let mut harness = harness(20, 3, r"printf 'bye\033[c'");
    // The program ends as the harness reads its query, so that the reply
    // meets, most of the time, a terminal that no one has open; whether it
    // does depends on the timing, which a unit test of the harness takes
    // out.
    std::thread::sleep(Duration::from_millis(300));
    harness.exit(LIMIT).unwrap();
    harness.sleep(Duration::from_millis(10)).unwrap();
    harness.quiet(Duration::from_millis(10), LIMIT).unwrap();
    assert!(harness.shows("bye"));
    let error = harness.send(b"x", LIMIT).unwrap_err();
    assert!(error.to_string().contains("ended before"), "{error:#}");
}

#[test]
fn a_program_hung_up_with_a_reply_queued_still_exits() {
    let mut harness = harness(20, 3, r"stty raw -echo; printf 'ready\033[c'; sleep 30");
    harness.wait("ready", LIMIT).unwrap();
    harness.hang_up();
    harness.exit(LIMIT).unwrap();
    assert_eq!(inputs(&harness), [b""; 0], "the reply was never written");
    let error = harness.send(b"x", LIMIT).unwrap_err();
    assert!(error.to_string().contains("hung up"), "{error:#}");
}

#[test]
fn the_exit_status_is_told() {
    let mut harness = harness(20, 3, "echo bye; exit 3");
    harness.exit(LIMIT).unwrap();
    assert_eq!(harness.exit_status(LIMIT).unwrap(), Exit::Code(3));
    let mut hung_up = self::harness(20, 3, "echo still here; sleep 30");
    hung_up.wait("still here", LIMIT).unwrap();
    hung_up.hang_up();
    assert!(hung_up.exit_status(LIMIT).is_err());
}

#[test]
fn a_resize_reaches_the_terminal_and_the_program() {
    let mut harness = harness(40, 5, "echo ready; read line; stty size");
    harness.wait("ready", LIMIT).unwrap();
    harness.resize(100, 30).unwrap();
    assert_eq!(
        (harness.terminal().cols(), harness.terminal().rows()),
        (100, 30)
    );
    harness.send(b"\r", LIMIT).unwrap();
    harness.wait("30 100", LIMIT).unwrap();
}

#[test]
fn the_size_that_the_program_sets_reaches_the_pty() {
    // The program asks for the size, then waits for a key, so that the
    // harness has read the request before `stty` looks.
    let script = r"printf '\033[8;10;30t'; echo ready; read line; stty size";
    let mut options = options(40, 5, script);
    options.terminal.window_ops = true;
    let mut harness = Harness::spawn(&options).unwrap();
    harness.wait("ready", LIMIT).unwrap();
    harness.send(b"\r", LIMIT).unwrap();
    harness.wait("10 30", LIMIT).unwrap();
    assert_eq!(
        (harness.terminal().cols(), harness.terminal().rows()),
        (30, 10)
    );
}

#[test]
fn the_terminals_options_answer_and_its_events_are_taken() {
    // The title reported, then the size of a cell, with ESC shown as E.
    let script = r#"stty raw -echo; printf '\033]2;hi\007\033[21t\033[16t'; r=$(dd bs=1 count=16 2>/dev/null); stty sane; printf 'reply:%s\n' "$r" | tr '\033' E"#;
    let mut options = options(40, 3, script);
    options.terminal.title_reports = true;
    let mut harness = Harness::spawn(&options).unwrap();
    harness.terminal_mut().set_cell_size(8, 16);
    harness.wait(r"reply:E]lhiE\E[6;16;8t", LIMIT).unwrap();
    assert_eq!(harness.terminal().title(), "hi");
    assert!(
        harness
            .take_terminal_events()
            .contains(&TerminalEvent::TitleChanged)
    );
    assert!(harness.take_terminal_events().is_empty(), "taken");
}

#[test]
fn replies_go_out_while_the_test_does_something_else() {
    let dir = std::env::temp_dir().join(format!("bolt-harness-serve-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir(&dir).unwrap();
    // The program says that it got the reply in a file, which the test
    // looks for without driving the harness.
    let script = r"stty raw -echo; echo armed; printf '\033[c'; dd bs=1 count=9 >/dev/null 2>&1; : > got; sleep 30";
    let mut options = options(20, 3, script);
    options.dir.clone_from(&dir);
    let mut harness = Harness::spawn(&options).unwrap();
    harness.wait("armed", LIMIT).unwrap();
    let got = dir.join("got");
    let found = harness
        .serve(|| {
            let started = std::time::Instant::now();
            while !got.exists() && started.elapsed() < Duration::from_secs(5) {
                std::thread::sleep(Duration::from_millis(10));
            }
            got.exists()
        })
        .unwrap();
    harness.hang_up();
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(found, "the program never got its reply");
    assert_eq!(inputs(&harness), [b"\x1b[?62;22c"]);
}

#[test]
fn a_step_ends_at_its_limit_while_the_program_writes_all_the_time() {
    let mut harness = harness(20, 3, "exec yes");
    let started = std::time::Instant::now();
    harness.sleep(Duration::from_millis(300)).unwrap();
    assert!(started.elapsed() < Duration::from_secs(3), "slept on");
    let error = harness
        .wait("never", Duration::from_millis(300))
        .unwrap_err();
    assert!(error.to_string().contains("did not show"), "{error:#}");
    assert!(started.elapsed() < Duration::from_secs(6), "waited on");
    harness.hang_up();
}

#[test]
fn a_reply_goes_once_the_program_has_room_for_it() {
    // The program fills its input, asks for DA1, then reads what fills its
    // input and waits for the reply, in silence.
    let script = r"stty raw -echo; echo armed; sleep 0.5; printf '\033[c'; sleep 0.5; head -c 1022 >/dev/null; dd bs=1 count=9 >/dev/null 2>&1; echo got";
    let mut harness = harness(20, 3, script);
    harness.wait("armed", LIMIT).unwrap();
    harness.send(&[b'x'; 1022], LIMIT).unwrap();
    harness.wait("got", Duration::from_secs(5)).unwrap();
}

#[test]
fn a_send_that_the_program_takes_whole_succeeds_though_it_ends() {
    // Each program reads what is typed, and ends: the last bytes can go as
    // the harness reads, which then meets the end, for some of the sizes.
    for n in (1500..9730).step_by(211) {
        let script = format!("stty raw -echo; echo armed; exec head -c {n} >/dev/null");
        let mut harness = harness(20, 3, &script);
        harness.wait("armed", LIMIT).unwrap();
        harness.send(&vec![b'x'; n], LIMIT).unwrap();
        harness.exit(LIMIT).unwrap();
    }
}

#[test]
fn a_send_that_times_out_takes_back_what_it_did_not_write() {
    let mut harness = harness(
        20,
        3,
        "stty raw -echo; echo armed; sleep 1; exec cat >/dev/null",
    );
    harness.wait("armed", LIMIT).unwrap();
    let typed = |harness: &Harness| {
        inputs(harness)
            .iter()
            .map(|bytes| bytes.len())
            .sum::<usize>()
    };
    let error = harness
        .send(&[b'x'; 70_000], Duration::from_millis(200))
        .unwrap_err();
    assert!(error.to_string().contains("did not take"), "{error:#}");
    let before = typed(&harness);
    assert!(before < 70_000, "{before} bytes typed");
    harness.sleep(Duration::from_secs(2)).unwrap();
    assert_eq!(typed(&harness), before, "nothing more typed");
    harness.hang_up();
}

#[test]
fn keys_typed_do_not_count_against_the_replies_that_wait() {
    // More than a MiB typed, which waits while the program sleeps, and then
    // a query, whose reply waits after the keys.
    let script = r"stty raw -echo; echo armed; sleep 1; printf '\033[c'; exec cat >/dev/null";
    let mut harness = harness(20, 3, script);
    harness.wait("armed", LIMIT).unwrap();
    harness
        .send(&vec![b'x'; (1 << 20) + 4096], Duration::from_secs(20))
        .unwrap();
    let started = std::time::Instant::now();
    while !inputs(&harness).contains(&&b"\x1b[?62;22c"[..]) {
        assert!(started.elapsed() < LIMIT, "the reply never went");
        harness.sleep(Duration::from_millis(50)).unwrap();
    }
    assert_eq!(harness.dropped_replies(), 0);
    harness.hang_up();
}

#[test]
fn a_wait_says_when_the_program_ends_inside_an_update() {
    let mut harness = harness(20, 3, r"printf '\033[?2026hshown'");
    let error = harness.wait("shown", LIMIT).unwrap_err();
    assert!(
        error.to_string().contains("inside a synchronized update"),
        "{error:#}"
    );
}

#[test]
fn a_reply_that_the_test_makes_reaches_a_program_that_waits_in_silence() {
    // The program asks for the clipboard, then waits for the answer.
    let script = r"stty raw -echo; echo armed; printf '\033]52;c;?\007'; dd bs=1 count=12 >/dev/null 2>&1; echo got";
    let mut options = options(20, 3, script);
    options.terminal.clipboard_reads = true;
    let mut harness = Harness::spawn(&options).unwrap();
    harness.wait("armed", LIMIT).unwrap();
    let started = std::time::Instant::now();
    let request = loop {
        let events = harness.take_terminal_events();
        if let Some(TerminalEvent::ClipboardRead(request)) = events.into_iter().next() {
            break request;
        }
        assert!(started.elapsed() < LIMIT, "no read came");
        harness.sleep(Duration::from_millis(20)).unwrap();
    };
    harness.terminal_mut().reply_clipboard(request, b"hi");
    harness.wait("got", Duration::from_secs(3)).unwrap();
}

#[test]
fn quiet_reads_with_no_quiet_time_and_keeps_its_limit() {
    // An update that ends after 0.2 s, which a quiet time of 0 waits for.
    let mut harness = harness(
        20,
        3,
        r"printf '\033[?2026hx'; sleep 0.2; printf '\033[?2026ly'; sleep 5",
    );
    let started = std::time::Instant::now();
    while !harness.terminal().synchronized_output() {
        assert!(started.elapsed() < LIMIT, "the update never started");
        harness.sleep(Duration::from_millis(10)).unwrap();
    }
    harness.quiet(Duration::ZERO, LIMIT).unwrap();
    assert!(harness.shows("y"), "{}", harness.screen());
    // Silent inside an update: the limit, not the quiet time, ends it.
    let mut silent = self::harness(20, 3, r"printf '\033[?2026hx'; sleep 5");
    let started = std::time::Instant::now();
    assert!(
        silent
            .quiet(Duration::from_secs(2), Duration::from_millis(300))
            .is_err()
    );
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );
    harness.hang_up();
    silent.hang_up();
}

#[test]
fn a_send_that_fails_on_an_error_takes_back_its_keys() {
    let typed = |harness: &Harness| {
        inputs(harness)
            .iter()
            .map(|bytes| bytes.len())
            .sum::<usize>()
    };
    let mut harness = harness(
        20,
        3,
        "stty raw -echo; echo armed; sleep 0.3; echo secret; sleep 1; exec cat >/dev/null",
    );
    harness.wait("armed", LIMIT).unwrap();
    harness.forbid(vec!["secret".to_owned()]);
    let error = harness.send(&[b'x'; 70_000], LIMIT).unwrap_err();
    assert!(error.to_string().contains("showed `secret`"), "{error:#}");
    harness.forbid(Vec::new());
    let before = typed(&harness);
    harness.sleep(Duration::from_secs(2)).unwrap();
    assert_eq!(typed(&harness), before, "nothing more typed");
    harness.hang_up();
}

/// How long `bytes` take to go straight to a program in a PTY that reads
/// them without writing, written as fast as the PTY takes them.
fn straight(bytes: &[u8]) -> Duration {
    let script = "stty raw -echo; echo armed; exec cat >/dev/null";
    let pty = bolt_pty::spawn(&bolt_pty::Options::new(
        bolt_pty::Program::command("/bin/sh", ["-c", script]),
        "xterm-256color",
    ))
    .unwrap();
    let mut reader = pty.reader().unwrap();
    let mut writer = pty.writer().unwrap();
    let (mut buffer, mut read) = ([0; 64], Vec::new());
    while !read.ends_with(b"armed\n") {
        let deadline = std::time::Instant::now() + LIMIT;
        match reader.read_until(&mut buffer, Some(deadline)).unwrap() {
            bolt_pty::ReadOutcome::Read(n) => read.extend_from_slice(&buffer[..n]),
            outcome => panic!("{outcome:?} before the program was ready"),
        }
    }
    let started = std::time::Instant::now();
    let mut left = bytes;
    while !left.is_empty() {
        let written = writer.write_until(left, None).unwrap().unwrap();
        left = &left[written..];
    }
    started.elapsed()
}

#[test]
fn keys_go_as_fast_as_a_silent_program_reads_them() {
    let keys = vec![b'x'; 512 << 10];
    let mut harness = harness(20, 3, "stty raw -echo; echo armed; exec cat >/dev/null");
    harness.wait("armed", LIMIT).unwrap();
    let started = std::time::Instant::now();
    harness.send(&keys, LIMIT).unwrap();
    let took = started.elapsed();
    harness.hang_up();
    // A turn of 10 ms for each write, as the harness took when it waited
    // for output alone, against the same bytes written straight, which a
    // busy machine slows too: three quarters of a turn a write, for what
    // the harness does besides when the machine is busy.
    let straight = straight(&keys);
    let writes = u32::try_from(inputs(&harness).len()).unwrap();
    let turns = Duration::from_micros(7_500) * writes;
    assert!(
        took < straight + turns,
        "{took:?} for {writes} writes, {straight:?} straight"
    );
}

#[test]
fn a_reply_that_the_test_makes_goes_before_the_keys_that_follow() {
    // The program asks for the clipboard, then reads the answer and a key.
    let script = r#"stty raw -echo; echo armed; printf '\033]52;c;?\007'; a=$(dd bs=1 count=12 2>/dev/null); k=$(dd bs=1 count=1 2>/dev/null); stty sane; printf 'got:%s|%s\n' "$a" "$k" | tr '\033\007' 'EB'"#;
    let mut options = options(40, 3, script);
    options.terminal.clipboard_reads = true;
    let mut harness = Harness::spawn(&options).unwrap();
    harness.wait("armed", LIMIT).unwrap();
    let started = std::time::Instant::now();
    let request = loop {
        let events = harness.take_terminal_events();
        if let Some(TerminalEvent::ClipboardRead(request)) = events.into_iter().next() {
            break request;
        }
        assert!(started.elapsed() < LIMIT, "no read came");
        harness.sleep(Duration::from_millis(20)).unwrap();
    };
    harness.terminal_mut().reply_clipboard(request, b"hi");
    harness.send(b"Z", LIMIT).unwrap();
    harness.wait("got:E]52;c;aGk=B|Z", LIMIT).unwrap();
}

#[test]
fn quiet_for_as_long_as_its_limit_succeeds_on_a_silent_program() {
    let mut harness = harness(20, 3, "echo armed; sleep 30");
    harness.wait("armed", LIMIT).unwrap();
    for _ in 0..50 {
        harness
            .quiet(Duration::from_millis(20), Duration::from_millis(20))
            .unwrap();
    }
    harness
        .quiet(Duration::ZERO, Duration::from_millis(10))
        .unwrap();
    harness.hang_up();
}

#[test]
fn a_send_after_a_hang_up_says_so() {
    let mut harness = harness(20, 3, "echo armed; sleep 30");
    harness.wait("armed", LIMIT).unwrap();
    harness.hang_up();
    let error = harness.send(b"x", LIMIT).unwrap_err();
    assert!(error.to_string().contains("hung up"), "{error:#}");
}

#[test]
fn a_mark_and_a_resize_write_the_replies_that_the_test_made() {
    let script = r"stty raw -echo; echo armed; printf '\033]52;c;?\007\033]52;c;?\007'; sleep 30";
    let mut options = options(20, 3, script);
    options.terminal.clipboard_reads = true;
    let mut harness = Harness::spawn(&options).unwrap();
    harness.wait("armed", LIMIT).unwrap();
    let started = std::time::Instant::now();
    let mut requests = Vec::new();
    while requests.len() < 2 {
        for event in harness.take_terminal_events() {
            if let TerminalEvent::ClipboardRead(request) = event {
                requests.push(request);
            }
        }
        assert!(started.elapsed() < LIMIT, "no reads came");
        harness.sleep(Duration::from_millis(20)).unwrap();
    }
    let answer = b"\x1b]52;c;aGk=\x07";
    harness.terminal_mut().reply_clipboard(requests[0], b"hi");
    harness.mark("answered").unwrap();
    assert_eq!(inputs(&harness), [&answer[..]], "written by the mark");
    harness.terminal_mut().reply_clipboard(requests[1], b"hi");
    harness.resize(30, 4).unwrap();
    assert_eq!(
        inputs(&harness),
        [&answer[..], &answer[..]],
        "and by the resize"
    );
    harness.hang_up();
}
