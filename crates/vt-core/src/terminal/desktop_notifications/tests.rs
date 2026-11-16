//! OSC 99's notifications put together of their parts, and its query
//! answered.

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

fn notification(title: &str, body: &str) -> Event {
    Event::Notification {
        title: title.to_owned(),
        body: body.to_owned(),
    }
}

#[test]
fn a_notification_of_one_part_shows_its_title() {
    let mut terminal = terminal();
    let events = fed(&mut terminal, "\x1b]99;;Hello world\x1b\\");
    assert_eq!(events, [notification("Hello world", "")]);
    // Of no metadata but its semicolons alone: one semicolon is not enough.
    assert_eq!(fed(&mut terminal, "\x1b]99;Hello\x1b\\"), []);
}

#[test]
fn its_parts_of_an_id_make_one_notification_at_the_last() {
    let mut terminal = terminal();
    // As the protocol's example: a title and a body, the title in two
    // parts, the body in base64.
    assert_eq!(fed(&mut terminal, "\x1b]99;i=1:d=0;Hello \x1b\\"), []);
    assert_eq!(fed(&mut terminal, "\x1b]99;i=1:d=0;world\x1b\\"), []);
    // Another's parts between them.
    assert_eq!(fed(&mut terminal, "\x1b]99;i=2:d=0;Other\x1b\\"), []);
    let events = fed(
        &mut terminal,
        "\x1b]99;i=1:p=body:e=1;VGhpcyBpcyBhIHRlc3Q=\x1b\\",
    );
    assert_eq!(events, [notification("Hello world", "This is a test")]);
    let events = fed(&mut terminal, "\x1b]99;i=2:p=body;Its text\x1b\\");
    assert_eq!(events, [notification("Other", "Its text")]);
}

#[test]
fn a_part_of_its_icon_may_end_it_and_close_and_alive_are_ignored() {
    let mut terminal = terminal();
    fed(&mut terminal, "\x1b]99;i=a:d=0;Built\x1b\\");
    assert_eq!(fed(&mut terminal, "\x1b]99;i=a:p=close;\x1b\\"), []);
    assert_eq!(fed(&mut terminal, "\x1b]99;i=a:p=alive;\x1b\\"), []);
    let events = fed(&mut terminal, "\x1b]99;i=a:p=icon:n=ZXJyb3I=;\x1b\\");
    assert_eq!(events, [notification("Built", "")]);
    // A payload of a kind not known is ignored whole, its `d` too.
    fed(&mut terminal, "\x1b]99;i=b:d=0;Held\x1b\\");
    assert_eq!(fed(&mut terminal, "\x1b]99;i=b:p=other;x\x1b\\"), []);
    let events = fed(&mut terminal, "\x1b]99;i=b:p=body;Done\x1b\\");
    assert_eq!(events, [notification("Held", "Done")]);
}

#[test]
fn the_query_is_answered_of_its_sanitized_id() {
    let mut terminal = terminal();
    fed(&mut terminal, "\x1b]99;i=abc<>/1:p=?;\x1b\\");
    assert_eq!(
        String::from_utf8(terminal.take_replies()).unwrap(),
        "\x1b]99;i=abc1:p=?;a=focus:o=unfocused:p=title,body\x1b\\"
    );
    fed(&mut terminal, "\x1b]99;p=?;\x07");
    assert_eq!(
        String::from_utf8(terminal.take_replies()).unwrap(),
        "\x1b]99;i=:p=?;a=focus:o=unfocused:p=title,body\x07"
    );
}

#[test]
fn its_parts_are_bounded_and_of_no_control_character() {
    let mut terminal = terminal();
    // A part past the protocol's bound is ignored.
    let long = "x".repeat(4097);
    assert_eq!(fed(&mut terminal, &format!("\x1b]99;;{long}\x1b\\")), []);
    // The parts of a title stop at its bound.
    for _ in 0..3 {
        fed(
            &mut terminal,
            &format!("\x1b]99;i=t:d=0;{}\x1b\\", "y".repeat(400)),
        );
    }
    let events = fed(&mut terminal, "\x1b]99;i=t;\x1b\\");
    let [Event::Notification { title, .. }] = &events[..] else {
        panic!("{events:?}");
    };
    assert_eq!(title.len(), 1024);
    // A base64 part that does not read is ignored; its control characters
    // go.
    assert_eq!(fed(&mut terminal, "\x1b]99;e=1;@@@@\x1b\\"), []);
    let events = fed(&mut terminal, "\x1b]99;e=1;YQpi\x1b\\");
    assert_eq!(events, [notification("ab", "")]);
    // Of neither title nor body: nothing.
    assert_eq!(fed(&mut terminal, "\x1b]99;p=body;\x1b\\"), []);
}

#[test]
fn it_holds_sixteen_notifications_at_most_and_a_reset_drops_them() {
    let mut terminal = terminal();
    for id in 0..17 {
        fed(&mut terminal, &format!("\x1b]99;i={id}:d=0;N{id}\x1b\\"));
    }
    // The oldest went: its last part shows its own alone.
    let events = fed(&mut terminal, "\x1b]99;i=0:p=body;late\x1b\\");
    assert_eq!(events, [notification("", "late")]);
    let events = fed(&mut terminal, "\x1b]99;i=16;!\x1b\\");
    assert_eq!(events, [notification("N16!", "")]);
    fed(&mut terminal, "\x1b]99;i=r:d=0;Before\x1b\\");
    fed(&mut terminal, "\x1bc");
    let events = fed(&mut terminal, "\x1b]99;i=r;After\x1b\\");
    assert_eq!(events, [notification("After", "")]);
}
