//! The view scrolled a share of a row past its first row, and the row past
//! its last that the snapshots then hold.

use crate::page::{Content, Scrollback};
use crate::parser::Parser;
use crate::terminal::decorations::Look;
use crate::terminal::{ImageFormat, Options, Placing, Scroll, Side, Snapshot, Terminal};

fn feed(terminal: &mut Terminal, text: &str) {
    Parser::new().advance(terminal, text.as_bytes());
}

/// The text of `snapshot`'s line `y`, without the spaces that end it.
fn text(snapshot: &Snapshot, y: u16) -> String {
    let text: String = snapshot
        .line(y)
        .cells()
        .iter()
        .map(|cell| match cell.content() {
            Content::Char(c) if c != '\0' => c,
            _ => ' ',
        })
        .collect();
    text.trim_end().to_owned()
}

/// A terminal of 20 columns and 4 rows of the lines `0` to `19`, then the
/// prompt's empty row: 17 rows of scrollback above the screen, numbered
/// from 0, the screen's of 17 to 20.
fn terminal() -> Terminal {
    let options = Options {
        scrollback: Scrollback {
            limit: 1 << 30,
            budget: None,
        },
        ..Options::default()
    };
    let mut terminal = Terminal::new(20, 4, options);
    for at in 0..20 {
        feed(&mut terminal, &format!("{at}\r\n"));
    }
    terminal
}

/// Whether `a` and `b` are the same share, within a float's rounding.
fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

#[test]
fn a_scroll_of_a_share_of_a_row_moves_whole_rows_and_keeps_the_rest() {
    let mut terminal = terminal();
    assert_eq!(terminal.scrollback_rows(), 17);
    // Up a row and a quarter from the screen: the view's first row is 15,
    // three quarters of it past.
    terminal.scroll_viewport_by(1.25);
    assert_eq!(terminal.origin().top, 15);
    assert!(near(terminal.viewport_fraction(), 0.75));
    // Down half a row: the row of 16, a quarter of it past.
    terminal.scroll_viewport_by(-0.5);
    assert_eq!(terminal.origin().top, 16);
    assert!(near(terminal.viewport_fraction(), 0.25));
    // Steps that add up to whole rows leave none.
    terminal.scroll_viewport_by(-0.75);
    assert_eq!(terminal.origin().top, 17);
    assert_eq!(terminal.viewport_offset(), 0);
    assert_eq!(terminal.viewport_fraction(), 0.0);
    // Many small steps, as a trackpad's, up three rows and a half.
    for _ in 0..35 {
        terminal.scroll_viewport_by(0.1);
    }
    assert_eq!(terminal.origin().top, 13);
    assert!(near(terminal.viewport_fraction(), 0.5));
    // A share too small to draw is none.
    terminal.scroll_viewport_by(0.5 - 1.0 / 4096.0);
    assert_eq!(terminal.origin().top, 13);
    assert_eq!(terminal.viewport_fraction(), 0.0);
    // Not a number moves nothing.
    terminal.scroll_viewport_by(0.5);
    terminal.scroll_viewport_by(f64::NAN);
    assert_eq!(terminal.origin().top, 12);
    assert!(near(terminal.viewport_fraction(), 0.5));
    // Nor is a share just short of a whole row: that row, as a glide's
    // steps that add up to whole rows leave it.
    terminal.scroll_viewport_by(0.5 + 1.0 / 4096.0);
    assert_eq!(terminal.origin().top, 12);
    assert_eq!(terminal.viewport_fraction(), 0.0);
}

#[test]
fn the_view_stops_at_the_scrollback_s_first_row_and_at_the_screen_of_no_share() {
    let mut terminal = terminal();
    // Past the scrollback's first row: there, of no share.
    terminal.scroll_viewport_by(100.3);
    assert_eq!(terminal.origin().top, 0);
    assert_eq!(terminal.viewport_fraction(), 0.0);
    // A share below it.
    terminal.scroll_viewport_by(-0.4);
    assert_eq!(terminal.origin().top, 0);
    assert!(near(terminal.viewport_fraction(), 0.4));
    // Past the screen: the view follows it, of no share, as output shows.
    terminal.scroll_viewport_by(-100.0);
    assert_eq!(terminal.viewport_offset(), 0);
    assert_eq!(terminal.viewport_fraction(), 0.0);
    feed(&mut terminal, "20\r\n21\r\n");
    assert_eq!(terminal.viewport_offset(), 0);
    // Down at the screen moves nothing.
    terminal.scroll_viewport_by(-0.5);
    assert_eq!(terminal.viewport_offset(), 0);
    assert_eq!(terminal.viewport_fraction(), 0.0);
}

#[test]
fn any_other_move_of_the_view_lets_the_share_go() {
    let mut terminal = terminal();
    // The keys' scrolls and jumps.
    terminal.scroll_viewport_by(2.5);
    terminal.scroll_viewport(Scroll::Up(1));
    assert_eq!(terminal.viewport_fraction(), 0.0);
    terminal.scroll_viewport_by(0.5);
    terminal.scroll_viewport(Scroll::PageDown);
    assert_eq!(terminal.viewport_fraction(), 0.0);
    // A scroll to the row where the view is already.
    terminal.scroll_viewport_by(3.5);
    let top = terminal.origin().top;
    terminal.scroll_viewport(Scroll::Row(top));
    assert_eq!(terminal.origin().top, top);
    assert_eq!(terminal.viewport_fraction(), 0.0);
    // Output keeps the view's rows, and its share, as it scrolls the
    // screen under it.
    terminal.scroll_viewport_by(0.5);
    let top = terminal.origin().top;
    feed(&mut terminal, "more\r\n");
    assert_eq!(terminal.origin().top, top);
    assert!(near(terminal.viewport_fraction(), 0.5));
    // Output that brings the view back to the screen, as the session
    // scrolls it there.
    terminal.scroll_viewport(Scroll::Bottom);
    assert_eq!(terminal.viewport_offset(), 0);
    assert_eq!(terminal.viewport_fraction(), 0.0);
    // Scrolling by pixels turned off: no share, the rows where they are.
    terminal.scroll_viewport_by(1.5);
    let top = terminal.origin().top;
    terminal.drop_viewport_fraction();
    assert_eq!(terminal.origin().top, top);
    assert_eq!(terminal.viewport_fraction(), 0.0);
    // A resize, which numbers the rows anew.
    terminal.scroll_viewport_by(2.5);
    terminal.resize(20, 5);
    assert_eq!(terminal.viewport_fraction(), 0.0);
    // The alternate screen, and the primary's view as it comes back.
    terminal.scroll_viewport_by(2.5);
    feed(&mut terminal, "\x1b[?1049h");
    assert_eq!(terminal.viewport_fraction(), 0.0);
    terminal.scroll_viewport_by(1.5);
    assert_eq!(terminal.viewport_fraction(), 0.0);
    feed(&mut terminal, "\x1b[?1049l");
    assert_eq!(terminal.viewport_fraction(), 0.0);
}

#[test]
fn a_snapshot_of_a_share_holds_the_row_past_the_view_s_last_and_its_cursor() {
    let mut terminal = terminal();
    let mut snapshot = Snapshot::default();
    // At the screen, of no share: the screen's four rows alone.
    terminal.snapshot(&mut snapshot);
    assert_eq!(snapshot.rows(), 4);
    assert_eq!(snapshot.drawn_rows(), 4);
    assert!(snapshot.below().is_none());
    assert_eq!(snapshot.fraction(), 0.0);
    // Up of a share, from the row of `10`: its four rows, then `14`.
    terminal.scroll_viewport(Scroll::Row(10));
    terminal.scroll_viewport_by(-0.25);
    terminal.snapshot(&mut snapshot);
    assert_eq!(snapshot.rows(), 4);
    assert!(near(snapshot.fraction(), 0.25));
    let shown: Vec<String> = (0..4).map(|y| text(&snapshot, y)).collect();
    assert_eq!(shown, ["10", "11", "12", "13"]);
    assert!(snapshot.below().is_some());
    assert_eq!(text(&snapshot, 4), "14");
    assert_eq!(snapshot.view_y(14), Some(4));
    // A decoration of the row below is told at its row; one past it not.
    let look = Look::default();
    assert!(terminal.decorate(1, (0, 14), (1, 14), look));
    assert!(terminal.decorate(2, (0, 15), (1, 15), look));
    terminal.snapshot(&mut snapshot);
    let rows: Vec<(u64, u16)> = snapshot
        .decorations()
        .iter()
        .map(|decorated| (decorated.id, decorated.row))
        .collect();
    assert_eq!(rows, [(1, 4)]);
    // Moved by whole rows of a share, the lines move with their rows.
    terminal.scroll_viewport_by(-2.0);
    terminal.snapshot(&mut snapshot);
    let shown: Vec<String> = (0..5).map(|y| text(&snapshot, y)).collect();
    assert_eq!(shown, ["12", "13", "14", "15", "16"]);
    // Of the screen's rows below too, the view a row above it.
    terminal.scroll_viewport(Scroll::Bottom);
    terminal.scroll_viewport_by(0.5);
    terminal.snapshot(&mut snapshot);
    assert_eq!(terminal.origin().top, 16);
    let shown: Vec<String> = (0..5).map(|y| text(&snapshot, y)).collect();
    assert_eq!(shown, ["16", "17", "18", "19", ""]);
    assert_eq!(snapshot.drawn_rows(), 5);
    // The cursor, on the screen's last row, shows on the row below.
    let cursor = snapshot.cursor();
    assert!(cursor.visible);
    assert_eq!((cursor.position.x, cursor.position.y), (0, 4));
    // A row further up, it is past the row below, and hidden.
    terminal.scroll_viewport_by(1.0);
    terminal.snapshot(&mut snapshot);
    assert!(!snapshot.cursor().visible);
    terminal.scroll_viewport_by(-1.0);
    // Back at the screen, the row below goes.
    terminal.scroll_viewport_by(-0.5);
    terminal.snapshot(&mut snapshot);
    assert_eq!(snapshot.rows(), 4);
    assert!(snapshot.below().is_none());
    let shown: Vec<String> = (0..4).map(|y| text(&snapshot, y)).collect();
    assert_eq!(shown, ["17", "18", "19", ""]);
}

#[test]
fn a_snapshot_of_a_share_holds_the_images_of_the_row_below() {
    let options = Options {
        scrollback: Scrollback {
            limit: 1 << 30,
            budget: None,
        },
        ..Options::default()
    };
    let mut terminal = Terminal::new(20, 4, options);
    for at in 0..14 {
        feed(&mut terminal, &format!("{at}\r\n"));
    }
    // An image of a row on the row of `14`.
    let id = terminal
        .keep_image(None, (ImageFormat::Rgb, 1, 1), vec![0; 3], true)
        .unwrap();
    let placing = Placing {
        width: Side::Cells(2),
        height: Side::Cells(1),
        move_cursor: false,
        ..Placing::default()
    };
    assert!(terminal.place_image(id, placing));
    for at in 14..20 {
        feed(&mut terminal, &format!("{at}\r\n"));
    }
    let mut snapshot = Snapshot::default();
    let rows = |snapshot: &Snapshot| -> Vec<i32> {
        snapshot.images().iter().map(|placed| placed.row).collect()
    };
    // From the row of `10`, of no share: past the view.
    terminal.scroll_viewport(Scroll::Row(10));
    terminal.snapshot(&mut snapshot);
    assert_eq!(rows(&snapshot), [] as [i32; 0]);
    // Of a share, on the row below.
    terminal.scroll_viewport_by(-0.25);
    terminal.snapshot(&mut snapshot);
    assert_eq!(text(&snapshot, 4), "14");
    assert_eq!(rows(&snapshot), [4]);
}

#[test]
fn a_scroll_to_a_place_takes_its_row_and_its_share() {
    let mut terminal = terminal();
    // The row numbered 10, and half of it past.
    terminal.scroll_viewport_to(10.5);
    assert_eq!(terminal.origin().top, 10);
    assert!(near(terminal.viewport_fraction(), 0.5));
    // From there, another place, up or down.
    terminal.scroll_viewport_to(3.25);
    assert_eq!(terminal.origin().top, 3);
    assert!(near(terminal.viewport_fraction(), 0.25));
    terminal.scroll_viewport_to(16.5);
    assert_eq!(terminal.origin().top, 16);
    assert!(near(terminal.viewport_fraction(), 0.5));
    // Past the ends: the scrollback's first row, and the screen, which the
    // view then follows, of no share.
    terminal.scroll_viewport_to(-5.0);
    assert_eq!(terminal.origin().top, 0);
    assert_eq!(terminal.viewport_fraction(), 0.0);
    terminal.scroll_viewport_to(100.0);
    assert_eq!(terminal.viewport_offset(), 0);
    assert_eq!(terminal.viewport_fraction(), 0.0);
    feed(&mut terminal, "20\r\n");
    assert_eq!(terminal.viewport_offset(), 0);
    // Not a number moves nothing.
    terminal.scroll_viewport_to(4.5);
    terminal.scroll_viewport_to(f64::NAN);
    assert_eq!(terminal.origin().top, 4);
    assert!(near(terminal.viewport_fraction(), 0.5));
    // By the rows' numbers, which the scrollback's limit leaves them as it
    // takes the oldest rows.
    let options = Options {
        scrollback: Scrollback {
            limit: 1,
            budget: None,
        },
        ..Options::default()
    };
    let mut kept = Terminal::new(20, 4, options);
    for at in 0..20_000 {
        feed(&mut kept, &format!("{at}\r\n"));
    }
    let first = kept.row_number(0);
    assert!(first > 0, "the oldest rows went");
    kept.scroll_viewport_to(first as f64 + 2.5);
    assert_eq!(kept.origin().top, first + 2);
    assert!(near(kept.viewport_fraction(), 0.5));
    // The alternate screen's view never moves.
    feed(&mut terminal, "\x1b[?1049h");
    terminal.scroll_viewport_to(2.0);
    assert_eq!(terminal.viewport_offset(), 0);
}
