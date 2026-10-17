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
    // The program ends before the harness reads its query, so that the
    // reply meets a terminal that no one has open.
    std::thread::sleep(Duration::from_millis(300));
    harness.exit(LIMIT).unwrap();
    harness.sleep(Duration::from_millis(10)).unwrap();
    harness.quiet(Duration::from_millis(10), LIMIT).unwrap();
    assert!(harness.shows("bye"));
    assert_eq!(inputs(&harness), [b""; 0], "the reply was never written");
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
    assert!(error.to_string().contains("ended before"), "{error:#}");
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
