//! The headless terminal, driving programs as a test does.

use std::path::PathBuf;
use std::time::Duration;

use bolt_replay::harness::{Event, Harness, HarnessOptions};

const LIMIT: Duration = Duration::from_secs(10);

/// A harness of `cols` by `rows` that runs `script` with `sh`.
fn harness(cols: u16, rows: u16, script: &str) -> Harness {
    let options = HarnessOptions {
        program: vec!["sh".to_owned(), "-c".to_owned(), script.to_owned()],
        env: vec![("PATH".to_owned(), "/usr/bin:/bin".to_owned())],
        term: "xterm-256color".to_owned(),
        dir: PathBuf::from("/"),
        cols,
        rows,
    };
    Harness::spawn(&options).unwrap()
}

#[test]
fn a_program_that_sends_da1_reads_the_reply() {
    // In raw mode, the reply comes without a newline; ESC shows as E.
    let script = r#"stty raw -echo; printf '\033[c'; r=$(dd bs=1 count=9 2>/dev/null); stty sane; printf 'reply:%s\n' "$r" | tr '\033' E"#;
    let mut harness = harness(40, 3, script);
    harness.wait("reply:E[?62;22c", LIMIT).unwrap();
    harness.exit(LIMIT).unwrap();
    let replies: Vec<&[u8]> = harness
        .events()
        .iter()
        .filter_map(|(_, event)| match event {
            Event::Input(bytes) => Some(&bytes[..]),
            _ => None,
        })
        .collect();
    assert_eq!(replies, [b"\x1b[?62;22c"]);
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
    assert!(String::from_utf8_lossy(harness.output()).contains("got hello"));
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
