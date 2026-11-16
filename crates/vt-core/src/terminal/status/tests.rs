//! OSC 7501's reports read, kept and dropped, and its query answered.

use super::*;
use crate::parser::Parser;
use crate::terminal::{Options, Terminal};

/// A terminal fed `bytes`, of its events taken.
fn fed(terminal: &mut Terminal, bytes: &str) -> Vec<Event> {
    let mut parser = Parser::new();
    parser.advance(terminal, bytes.as_bytes());
    terminal.take_events()
}

fn terminal() -> Terminal {
    Terminal::new(40, 5, Options::default())
}

/// `text` in base64, of no padding.
fn base64(text: &str) -> String {
    clipboard::tests_encode(text.as_bytes())
        .trim_end_matches('=')
        .to_owned()
}

#[test]
fn a_report_of_every_key_is_kept_and_told() {
    let mut terminal = terminal();
    let msg = base64("Run `cargo test`?");
    let title = base64("Fix the build");
    let events = fed(
        &mut terminal,
        &format!(
            "\x1b]7501;state=blocked:kind=permission:progress=40:app=claude:title={title}:msg={msg}\x1b\\"
        ),
    );
    assert_eq!(events, [Event::StatusChanged]);
    assert_eq!(
        terminal.statuses(),
        [Status {
            id: String::new(),
            state: StatusState::Blocked,
            kind: Some(StatusKind::Permission),
            progress: Some(40),
            app: Some("claude".to_owned()),
            title: Some("Fix the build".to_owned()),
            msg: Some("Run `cargo test`?".to_owned()),
        }]
    );
    // Each state, of BEL too; whitespace round the keys and values.
    for (word, state) in [
        ("idle", StatusState::Idle),
        ("working", StatusState::Working),
        ("done", StatusState::Done),
        ("error", StatusState::Error),
    ] {
        fed(&mut terminal, &format!("\x1b]7501; state = {word} \x07"));
        assert_eq!(terminal.statuses()[0].state, state, "{word}");
    }
}

#[test]
fn a_report_replaces_its_record_whole_and_a_repeated_key_takes_its_last_value() {
    let mut terminal = terminal();
    fed(
        &mut terminal,
        "\x1b]7501;state=working:app=codex:progress=10\x1b\\",
    );
    fed(
        &mut terminal,
        "\x1b]7501;state=working:progress=20:progress=30\x1b\\",
    );
    let status = &terminal.statuses()[0];
    assert_eq!((status.app.as_deref(), status.progress), (None, Some(30)));
}

#[test]
fn values_out_of_their_use_or_range_are_absent_and_malformed_pairs_skipped() {
    let mut terminal = terminal();
    // A kind of a state that is not blocked, a progress past 100 or of
    // done, an app of a space's character, a pair of no `=` and one of a
    // key not lowercase: each absent, the rest applying.
    fed(
        &mut terminal,
        "\x1b]7501;state=working:kind=question:progress=101:app=a b:junk:Key=x:unknown=1\x1b\\",
    );
    let status = &terminal.statuses()[0];
    assert_eq!(status.state, StatusState::Working);
    assert_eq!(
        (status.kind, status.progress, &status.app),
        (None, None, &None)
    );
    fed(&mut terminal, "\x1b]7501;state=done:progress=50\x1b\\");
    assert_eq!(terminal.statuses()[0].progress, None);
    fed(&mut terminal, "\x1b]7501;state=blocked:kind=nope\x1b\\");
    assert_eq!(terminal.statuses()[0].kind, None);
}

#[test]
fn a_report_of_no_state_or_past_a_bound_or_of_bad_text_is_ignored_whole() {
    let mut terminal = terminal();
    let control = base64("two\nlines");
    let long_title = base64(&"t".repeat(193));
    let long_msg = base64(&"m".repeat(2049));
    for report in [
        "state=sleeping".to_owned(),
        "app=claude".to_owned(),
        format!("state=working:{}=1", "k".repeat(17)),
        format!("state=working:app={}", "a".repeat(33)),
        "state=working:msg=A=BC".to_owned(),
        "state=working:msg=A".to_owned(),
        format!("state=working:msg={control}"),
        format!("state=working:title={long_title}"),
        format!("state=working:msg={long_msg}"),
        format!("state=working:id={}", "s".repeat(33)),
        format!("state=working:id={}", ["a"; 9].join("/")),
        "state=working:id=a//b".to_owned(),
        format!("state=working:msg={}", "A".repeat(4100)),
    ] {
        assert_eq!(
            fed(&mut terminal, &format!("\x1b]7501;{report}\x1b\\")),
            [],
            "{report}"
        );
    }
    assert!(terminal.statuses().is_empty());
    // A value of a character that no value takes is a malformed pair,
    // skipped, the rest applying.
    fed(&mut terminal, "\x1b]7501;state=working:msg=@@@@\x1b\\");
    assert_eq!(terminal.statuses()[0].msg, None);
    // Text of a padding, and of none: both read.
    fed(
        &mut terminal,
        &format!(
            "\x1b]7501;state=working:msg={}\x1b\\",
            clipboard::tests_encode(b"ok")
        ),
    );
    assert_eq!(terminal.statuses()[0].msg.as_deref(), Some("ok"));
}

#[test]
fn records_of_ids_form_a_tree_whose_clear_takes_a_branch_and_whose_app_is_inherited() {
    let mut terminal = terminal();
    fed(&mut terminal, "\x1b]7501;state=working:app=claude\x1b\\");
    fed(&mut terminal, "\x1b]7501;state=working:id=build\x1b\\");
    fed(&mut terminal, "\x1b]7501;state=blocked:id=build/test\x1b\\");
    fed(&mut terminal, "\x1b]7501;state=done:id=builder\x1b\\");
    assert_eq!(terminal.statuses().len(), 4);
    let test = terminal
        .statuses()
        .iter()
        .find(|status| status.id == "build/test")
        .unwrap()
        .clone();
    assert_eq!(terminal.status_app(&test), Some("claude"));
    // `build` and what is under it go; `builder` is another branch.
    assert_eq!(
        fed(&mut terminal, "\x1b]7501;state=clear:id=build\x1b\\"),
        [Event::StatusChanged]
    );
    let ids: Vec<&str> = terminal
        .statuses()
        .iter()
        .map(|status| status.id.as_str())
        .collect();
    assert_eq!(ids, ["", "builder"]);
    // A clear of nothing tells nothing; of the root, everything.
    assert_eq!(
        fed(&mut terminal, "\x1b]7501;state=clear:id=none\x1b\\"),
        []
    );
    fed(&mut terminal, "\x1b]7501;state=clear\x1b\\");
    assert!(terminal.statuses().is_empty());
}

#[test]
fn the_least_recently_updated_record_goes_past_the_most() {
    let mut terminal = terminal();
    for task in 0..=MOST_STATUSES {
        fed(
            &mut terminal,
            &format!("\x1b]7501;state=working:id=t{task}\x1b\\"),
        );
    }
    assert_eq!(terminal.statuses().len(), MOST_STATUSES);
    assert_eq!(terminal.statuses()[0].id, "t1");
    // An update moves a record last, which the next to go is not.
    fed(&mut terminal, "\x1b]7501;state=done:id=t1\x1b\\");
    fed(&mut terminal, "\x1b]7501;state=working:id=more\x1b\\");
    assert!(terminal.statuses().iter().any(|status| status.id == "t1"));
    assert!(!terminal.statuses().iter().any(|status| status.id == "t2"));
}

#[test]
fn a_prompt_drops_the_work_and_the_waits_and_a_reset_drops_everything() {
    let mut terminal = terminal();
    fed(&mut terminal, "\x1b]7501;state=working:id=a\x1b\\");
    fed(&mut terminal, "\x1b]7501;state=blocked:id=b\x1b\\");
    fed(&mut terminal, "\x1b]7501;state=idle:id=c\x1b\\");
    fed(&mut terminal, "\x1b]7501;state=done:id=d\x1b\\");
    fed(&mut terminal, "\x1b]7501;state=error:id=e\x1b\\");
    assert_eq!(
        fed(&mut terminal, "\x1b]133;A\x1b\\"),
        [Event::StatusChanged]
    );
    let ids: Vec<&str> = terminal
        .statuses()
        .iter()
        .map(|status| status.id.as_str())
        .collect();
    assert_eq!(ids, ["d", "e"]);
    // Another prompt, of nothing to drop, tells nothing.
    assert_eq!(fed(&mut terminal, "\x1b]133;A\x1b\\"), []);
    // The alternate screen keeps them; RIS takes them away.
    fed(&mut terminal, "\x1b[?1049h");
    assert_eq!(terminal.statuses().len(), 2);
    let events = fed(&mut terminal, "\x1bc");
    assert!(events.contains(&Event::StatusChanged), "{events:?}");
    assert!(terminal.statuses().is_empty());
}

#[test]
fn the_embedder_forgets_the_records_that_it_no_longer_shows() {
    let mut terminal = terminal();
    fed(&mut terminal, "\x1b]7501;state=done:id=a\x1b\\");
    fed(&mut terminal, "\x1b]7501;state=error:id=b\x1b\\");
    assert!(terminal.forget_statuses(|status| status.id != "a"));
    assert_eq!(terminal.take_events(), [Event::StatusChanged]);
    assert!(!terminal.forget_statuses(|_| true));
    assert_eq!(terminal.take_events(), []);
    assert_eq!(terminal.statuses().len(), 1);
}

#[test]
fn the_query_is_answered_of_the_states_and_kinds_as_it_ended() {
    let mut terminal = terminal();
    fed(&mut terminal, "\x1b]7501;?\x1b\\");
    assert_eq!(
        String::from_utf8(terminal.take_replies()).unwrap(),
        "\x1b]7501;?:states=idle,working,done,blocked,error:kinds=permission,question,auth\x1b\\"
    );
    fed(&mut terminal, "\x1b]7501; ? \x07");
    assert!(
        String::from_utf8(terminal.take_replies())
            .unwrap()
            .ends_with('\x07')
    );
    assert!(terminal.statuses().is_empty());
}
