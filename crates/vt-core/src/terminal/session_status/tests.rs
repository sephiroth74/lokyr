//! OSC 21337's session status read, kept and cleared.

use super::*;
use crate::parser::Parser;
use crate::terminal::Options;

/// A terminal fed `bytes`, of its events taken.
fn fed(terminal: &mut Terminal, bytes: &str) -> Vec<Event> {
    let mut parser = Parser::new();
    parser.advance(terminal, bytes.as_bytes());
    terminal.take_events()
}

fn terminal() -> Terminal {
    Terminal::new(40, 5, Options::default())
}

#[test]
fn every_part_is_kept_and_told_once() {
    let mut terminal = terminal();
    let events = fed(
        &mut terminal,
        "\x1b]21337;status=Working;indicator=#ffa500;status-color=rgb:ff/ff/ff;detail=Running the tests\x07",
    );
    assert_eq!(events, [Event::SessionStatusChanged]);
    assert_eq!(
        *terminal.session_status(),
        SessionStatus {
            status: Some("Working".to_owned()),
            indicator: Some(Rgb::new(0xff, 0xa5, 0x00)),
            status_color: Some(Rgb::new(0xff, 0xff, 0xff)),
            detail: Some("Running the tests".to_owned()),
        }
    );
    // The same again tells nothing.
    assert_eq!(fed(&mut terminal, "\x1b]21337;status=Working\x1b\\"), []);
}

#[test]
fn a_key_left_out_keeps_its_part_and_an_empty_value_clears_it() {
    let mut terminal = terminal();
    fed(
        &mut terminal,
        "\x1b]21337;status=Working;indicator=#ffa500\x07",
    );
    fed(&mut terminal, "\x1b]21337;status=Waiting\x07");
    let status = terminal.session_status();
    assert_eq!(
        (status.status.as_deref(), status.indicator),
        (Some("Waiting"), Some(Rgb::new(0xff, 0xa5, 0x00)))
    );
    // As Claude Code clears it as it exits.
    let events = fed(
        &mut terminal,
        "\x1b]21337;indicator=;status=;status-color=\x07",
    );
    assert_eq!(events, [Event::SessionStatusChanged]);
    assert!(terminal.session_status().is_empty());
}

#[test]
fn a_value_s_escaped_semicolons_and_backslashes_are_its_own() {
    let mut terminal = terminal();
    fed(
        &mut terminal,
        r"]21337;status=a\;b\\c;detail=x\ny"
            .replace(']', "\x1b]")
            .replace("y", "y\x07")
            .as_str(),
    );
    let status = terminal.session_status();
    assert_eq!(status.status.as_deref(), Some(r"a;b\c"));
    // A backslash before another character stays.
    assert_eq!(status.detail.as_deref(), Some(r"x\ny"));
}

#[test]
fn what_does_not_read_is_ignored_and_the_rest_applies() {
    let mut terminal = terminal();
    let long = "x".repeat(257);
    fed(
        &mut terminal,
        &format!(
            "\x1b]21337;status={long};indicator=orange;status-color=#00ff00;other=1;novalue\x07"
        ),
    );
    let status = terminal.session_status();
    assert_eq!(status.status, None);
    assert_eq!(status.indicator, None);
    assert_eq!(status.status_color, Some(Rgb::new(0, 0xff, 0)));
    // A control character refuses its text: a C1 one, as the parser leaves
    // the C0 ones out of the string.
    fed(&mut terminal, "\x1b]21337;status=a\u{85}b\x07");
    assert_eq!(terminal.session_status().status, None);
    // The detail's bound is longer than the status's.
    let detail = "y".repeat(1024);
    fed(&mut terminal, &format!("\x1b]21337;detail={detail}\x07"));
    assert_eq!(
        terminal.session_status().detail.as_deref(),
        Some(detail.as_str())
    );
}

#[test]
fn the_embedder_clears_it_and_a_reset_does_too() {
    let mut terminal = terminal();
    fed(&mut terminal, "\x1b]21337;status=Working\x07");
    assert!(terminal.clear_session_status());
    assert_eq!(terminal.take_events(), []);
    assert!(!terminal.clear_session_status());
    fed(&mut terminal, "\x1b]21337;status=Working\x07");
    let events = fed(&mut terminal, "\x1bc");
    assert!(events.contains(&Event::SessionStatusChanged), "{events:?}");
    assert!(terminal.session_status().is_empty());
}
