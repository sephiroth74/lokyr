//! Folds of a command's output and of its whole block: the view's rows and
//! placeholders, its scrolls and jumps over them, and their going.

use super::*;
use crate::page::{Content, Scrollback};
use crate::parser::Parser;
use crate::terminal::{Options, Scroll, Snapshot};

fn feed(terminal: &mut Terminal, text: &str) {
    Parser::new().advance(terminal, text.as_bytes());
}

/// A command of the shell's marks, its line `line` and its output `lines`
/// rows of numbers from 1, ended of the status 0.
fn command(line: &str, lines: usize) -> String {
    let mut text = format!("\x1b]133;A\x07$ \x1b]133;B\x07{line}\r\n\x1b]133;C\x07");
    for n in 1..=lines {
        text.push_str(&format!("{line} {n}\r\n"));
    }
    text.push_str("\x1b]133;D;0\x07");
    text
}

/// A terminal of 40 columns and 10 rows whose scrollback holds about
/// 10 000 rows: `one`, of 100 rows of output, `two`, of 9 800, `three`, of
/// 50, then the prompt where the user types.
fn terminal() -> Terminal {
    let options = Options {
        scrollback: Scrollback {
            limit: 1 << 30,
            budget: None,
        },
        ..Options::default()
    };
    let mut terminal = Terminal::new(40, 10, options);
    feed(&mut terminal, &command("one", 100));
    feed(&mut terminal, &command("two", 9_800));
    feed(&mut terminal, &command("three", 50));
    feed(&mut terminal, "\x1b]133;A\x07$ \x1b]133;B\x07");
    terminal
}

/// The snapshot of `terminal` updated now, into `snapshot`, of its rows'
/// texts, a placeholder's as `[⋯ N]` and a hidden block's as `[hidden N]`.
fn shown(terminal: &mut Terminal, snapshot: &mut Snapshot) -> Vec<String> {
    terminal.snapshot(snapshot);
    (0..snapshot.rows())
        .map(|y| match snapshot.view_row(y) {
            ViewRow::Folded {
                whole: false, rows, ..
            } => format!("[⋯ {rows}]"),
            ViewRow::Folded {
                whole: true, rows, ..
            } => format!("[hidden {rows}]"),
            ViewRow::Past => "[past]".to_owned(),
            ViewRow::Row(_) => {
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
        })
        .collect()
}

/// The ids of `terminal`'s commands `one`, `two` and `three`.
fn ids(terminal: &Terminal) -> [u64; 3] {
    let commands = terminal.commands();
    [commands[0].id, commands[1].id, commands[2].id]
}

#[test]
fn an_output_folded_is_one_placeholder_row_that_scrolls_and_jumps_as_one() {
    let mut terminal = terminal();
    let mut snapshot = Snapshot::default();
    let [_, two, three] = ids(&terminal);
    assert!(terminal.fold(two, false));
    assert_eq!(
        terminal.folds(),
        [FoldMark {
            command: two,
            whole: false,
            first: terminal.command(two).unwrap().output.unwrap().1,
            last: terminal.command(two).unwrap().end.unwrap().1 - 1,
        }]
    );
    // At the screen: `three`'s output's last rows and the prompt, of no
    // fold, as before.
    let bottom = shown(&mut terminal, &mut snapshot);
    assert_eq!(bottom.last().map(String::as_str), Some("$"));
    assert_eq!(bottom[0], "three 42");
    // Up: `two`'s placeholder in place of its 9 800 rows, between its line
    // and `three`'s, one view's row.
    terminal.scroll_viewport(Scroll::Up(46));
    assert_eq!(
        shown(&mut terminal, &mut snapshot),
        [
            "one 99",
            "one 100",
            "$ two",
            "[⋯ 9800]",
            "$ three",
            "three 1",
            "three 2",
            "three 3",
            "three 4",
            "three 5"
        ]
    );
    // The list's rows that it takes out, all and above the view.
    assert_eq!(snapshot.folded(), (9_799, 0));
    // A row up and a row down cross it whole.
    terminal.scroll_viewport(Scroll::Down(3));
    assert_eq!(shown(&mut terminal, &mut snapshot)[0], "[⋯ 9800]");
    assert_eq!(snapshot.folded(), (9_799, 0));
    terminal.scroll_viewport(Scroll::Down(1));
    assert_eq!(shown(&mut terminal, &mut snapshot)[0], "$ three");
    assert_eq!(snapshot.folded(), (9_799, 9_799));
    terminal.scroll_viewport(Scroll::Up(2));
    assert_eq!(shown(&mut terminal, &mut snapshot)[0], "$ two");
    // The jumps: from `two`'s prompt up to `one`'s, then down over the
    // fold to `two`'s and `three`'s.
    terminal.scroll_viewport(Scroll::PreviousPrompt);
    assert_eq!(shown(&mut terminal, &mut snapshot)[0], "$ one");
    terminal.scroll_viewport(Scroll::NextPrompt);
    assert_eq!(shown(&mut terminal, &mut snapshot)[0], "$ two");
    terminal.scroll_viewport(Scroll::NextPrompt);
    assert_eq!(shown(&mut terminal, &mut snapshot)[0], "$ three");
    // A row in the fold takes the view to its placeholder.
    let inside = terminal.command(two).unwrap().output.unwrap().1 + 500;
    terminal.scroll_viewport(Scroll::Row(inside));
    assert_eq!(shown(&mut terminal, &mut snapshot)[0], "[⋯ 9800]");
    // The top, and the screen again.
    terminal.scroll_viewport(Scroll::Top);
    assert_eq!(shown(&mut terminal, &mut snapshot)[0], "$ one");
    terminal.scroll_viewport(Scroll::Bottom);
    assert_eq!(shown(&mut terminal, &mut snapshot), bottom);
    // Unfolded, the rows come back, every line copied anew.
    terminal.scroll_viewport(Scroll::Up(46));
    shown(&mut terminal, &mut snapshot);
    assert!(terminal.unfold(two));
    assert!(terminal.folds().is_empty());
    let rows = shown(&mut terminal, &mut snapshot);
    assert!(snapshot.view().is_empty());
    assert!(!rows.iter().any(|row| row.starts_with('[')), "{rows:?}");
    assert_eq!(snapshot.folded(), (0, 0));
    // `three`, whose output ends past the screen's top.
    assert!(terminal.fold(three, false));
}

#[test]
fn a_block_hidden_is_one_row_of_its_prompt_its_line_and_its_output() {
    let mut terminal = terminal();
    let mut snapshot = Snapshot::default();
    let [one, two, _] = ids(&terminal);
    assert!(terminal.fold(two, true));
    // The output's fold before gives way to the block's.
    assert!(terminal.fold(one, false));
    assert!(terminal.fold(one, true));
    assert_eq!(terminal.folds().len(), 2);
    terminal.scroll_viewport(Scroll::Top);
    assert_eq!(
        shown(&mut terminal, &mut snapshot)[..4],
        ["[hidden 101]", "[hidden 9801]", "$ three", "three 1"]
    );
    // The jumps land on the hidden blocks' rows, their prompts'.
    terminal.scroll_viewport(Scroll::Row(
        terminal.command(two).unwrap().prompt.unwrap().1,
    ));
    assert_eq!(shown(&mut terminal, &mut snapshot)[0], "[hidden 9801]");
    terminal.scroll_viewport(Scroll::PreviousPrompt);
    assert_eq!(shown(&mut terminal, &mut snapshot)[0], "[hidden 101]");
}

#[test]
fn no_fold_takes_the_prompt_in_force_a_command_that_runs_or_the_alternate_screen() {
    let mut terminal = terminal();
    let [one, ..] = ids(&terminal);
    // The prompt where the user types, the last command's.
    let last = terminal.last_command().unwrap().id;
    assert!(!terminal.fold(last, false));
    assert!(!terminal.fold(last, true));
    // A command that runs, of no end.
    feed(&mut terminal, "ls\r\n\x1b]133;C\x07a\r\nb\r\n");
    assert!(!terminal.fold(last, false));
    // An id of none, and the alternate screen.
    assert!(!terminal.fold(9_999, false));
    feed(&mut terminal, "\x1b[?1049h");
    assert!(!terminal.fold(one, false));
    feed(&mut terminal, "\x1b[?1049l");
    assert!(terminal.fold(one, false));
}

#[test]
fn a_fold_goes_with_its_rows_at_a_clear_and_follows_a_reflow() {
    let mut terminal = terminal();
    let mut snapshot = Snapshot::default();
    let [one, two, _] = ids(&terminal);
    assert!(terminal.fold(two, false));
    // Narrower, the rows wrap, and the fold keeps its command's rows.
    terminal.resize(20, 10);
    let fold = terminal.folds()[0];
    assert_eq!(fold.command, two);
    assert_eq!(fold.first, terminal.command(two).unwrap().output.unwrap().1);
    terminal.scroll_viewport(Scroll::Row(fold.first - 1));
    assert_eq!(
        shown(&mut terminal, &mut snapshot)[..2],
        ["$ two", "[⋯ 9800]"]
    );
    // A clear of the scrollback lets go of every fold.
    assert!(terminal.fold(one, false));
    feed(&mut terminal, "\x1b[3J");
    assert!(terminal.folds().is_empty());
    shown(&mut terminal, &mut snapshot);
    assert!(snapshot.view().is_empty());
}

#[test]
fn a_fold_goes_as_the_scrollback_s_limit_takes_its_rows() {
    let options = Options {
        scrollback: Scrollback {
            limit: 0,
            ..Scrollback::default()
        },
        ..Options::default()
    };
    let mut terminal = Terminal::new(40, 10, options);
    feed(&mut terminal, &command("one", 20));
    feed(&mut terminal, &command("two", 3));
    feed(&mut terminal, "\x1b]133;A\x07$ \x1b]133;B\x07");
    let one = terminal.commands()[0].id;
    assert!(terminal.fold(one, false));
    let mut lines = 0;
    while !terminal.folds().is_empty() {
        feed(&mut terminal, "more\r\n");
        lines += 1;
        assert!(lines < 100_000, "the scrollback keeps every row");
    }
    let mut snapshot = Snapshot::default();
    terminal.snapshot(&mut snapshot);
    assert!(snapshot.view().is_empty());
    assert!(terminal.folds.is_empty());
}

#[test]
fn a_short_session_folded_on_the_screen_ends_of_empty_rows() {
    let mut terminal = Terminal::new(40, 10, Options::default());
    feed(&mut terminal, &command("ls", 5));
    feed(&mut terminal, "\x1b]133;A\x07$ \x1b]133;B\x07");
    let mut snapshot = Snapshot::default();
    shown(&mut terminal, &mut snapshot);
    let ls = terminal.commands()[0].id;
    assert!(terminal.fold(ls, false));
    // Its prompt, its placeholder, the prompt where the user types and the
    // screen's empty rows, then rows past them, empty.
    assert_eq!(
        shown(&mut terminal, &mut snapshot),
        [
            "$ ls", "[⋯ 5]", "$", "", "", "", "[past]", "[past]", "[past]", "[past]"
        ]
    );
    // The cursor at the prompt's row, as the view shows it.
    assert_eq!(snapshot.cursor().position.y, 2);
    assert!(snapshot.cursor().visible);
    assert!(terminal.unfold(ls));
    assert_eq!(
        shown(&mut terminal, &mut snapshot)[..3],
        ["$ ls", "ls 1", "ls 2"]
    );
}

#[test]
fn a_full_reset_and_a_clear_to_the_prompt_let_go_of_every_fold() {
    let mut terminal = terminal();
    let mut snapshot = Snapshot::default();
    let [one, two, _] = ids(&terminal);
    assert!(terminal.fold(two, false));
    terminal.clear_to_prompt();
    assert!(terminal.folds().is_empty());
    shown(&mut terminal, &mut snapshot);
    assert!(snapshot.view().is_empty());
    let mut terminal = self::terminal();
    assert!(terminal.fold(one, true));
    shown(&mut terminal, &mut snapshot);
    feed(&mut terminal, "\x1bc");
    assert!(terminal.folds().is_empty());
    shown(&mut terminal, &mut snapshot);
    assert!(snapshot.view().is_empty());
}

/// The number of the row `n` from 1 of the output of `command`.
fn output_row(terminal: &Terminal, command: u64, n: i64) -> i64 {
    terminal.command(command).unwrap().output.unwrap().1 + n - 1
}

/// `terminal`, its command `two` folded, scrolled to show, from its top:
/// `one 99`, `one 100`, `$ two`, the placeholder, `$ three` and `three 1`
/// to `three 5`.
fn folded_two() -> (Terminal, Snapshot, [u64; 3]) {
    let mut terminal = terminal();
    let mut snapshot = Snapshot::default();
    let ids = ids(&terminal);
    assert!(terminal.fold(ids[1], false));
    terminal.scroll_viewport(Scroll::Up(46));
    assert_eq!(shown(&mut terminal, &mut snapshot)[3], "[⋯ 9800]");
    (terminal, snapshot, ids)
}

#[test]
fn a_selection_across_a_fold_takes_its_rows_and_shows_at_the_view_s_rows() {
    use crate::terminal::{SelectionShape, SelectionUnit};
    let (mut terminal, mut snapshot, [one, two, three]) = folded_two();
    let from = (0, output_row(&terminal, one, 99));
    let to = (6, output_row(&terminal, three, 2));
    terminal.select(from, to, SelectionShape::Cells, SelectionUnit::Cell, "");
    shown(&mut terminal, &mut snapshot);
    let selected = snapshot.selection().unwrap();
    assert_eq!((selected.start, selected.end), ((0, 0), (6, 6)));
    assert!(selected.contains(3, 3));
    // Its text, the fold's rows whole.
    let text = terminal.selection_text().unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2 + 1 + 9_800 + 1 + 2);
    assert_eq!(lines[3], "two 1");
    assert_eq!(lines[9_802], "two 9800");
    assert_eq!(lines.last(), Some(&"three 2"));
    // Its end in the fold shows at the placeholder's row.
    let inside = (3, output_row(&terminal, two, 500));
    terminal.select(from, inside, SelectionShape::Cells, SelectionUnit::Cell, "");
    shown(&mut terminal, &mut snapshot);
    assert_eq!(snapshot.selection().unwrap().end, (3, 3));
    // From above the view, of the list's rows between.
    terminal.scroll_viewport(Scroll::Down(3));
    terminal.select(from, to, SelectionShape::Cells, SelectionUnit::Cell, "");
    shown(&mut terminal, &mut snapshot);
    let selected = snapshot.selection().unwrap();
    assert_eq!((selected.start.1, selected.end), (-3, (6, 3)));
}

#[test]
fn a_current_match_in_a_fold_unfolds_it_and_others_in_folds_show_nothing() {
    let (mut terminal, mut snapshot, [_, two, three]) = folded_two();
    let pin = |terminal: &mut Terminal, at| terminal.pin_cell(at).unwrap();
    // `two 7`'s `two`, in the fold, and `three 1`'s `three`, below it.
    let in_fold = (
        (0, output_row(&terminal, two, 7)),
        (2, output_row(&terminal, two, 7)),
    );
    let below = (
        (0, output_row(&terminal, three, 1)),
        (4, output_row(&terminal, three, 1)),
    );
    let found = vec![
        (pin(&mut terminal, in_fold.0), pin(&mut terminal, in_fold.1)),
        (pin(&mut terminal, below.0), pin(&mut terminal, below.1)),
    ];
    terminal.add_matches(1, 0, found);
    // Shown, the one below the fold at its row, the one in it nowhere.
    assert!(terminal.show_matches(1, true, None));
    terminal.snapshot(&mut snapshot);
    let spans: Vec<(u16, u16, u16)> = snapshot
        .highlights()
        .iter()
        .map(|span| (span.row, span.first, span.last))
        .collect();
    assert_eq!(spans, [(5, 0, 4)]);
    // The one in the fold the current one: the fold goes.
    assert!(terminal.show_matches(1, true, Some(in_fold)));
    assert!(terminal.folds().is_empty());
    terminal.scroll_viewport(Scroll::Row(in_fold.0.1));
    terminal.snapshot(&mut snapshot);
    assert!(snapshot.view().is_empty());
    assert!(
        snapshot
            .highlights()
            .iter()
            .any(|span| span.current && span.row == 0)
    );
}

#[test]
fn a_label_after_a_fold_and_the_gutter_s_marks_show_at_their_rows() {
    use crate::terminal::{Label, Look};
    let (mut terminal, mut snapshot, [_, two, three]) = folded_two();
    let look = Look {
        underline: crate::page::Underline::None,
        underline_color: None,
        background: None,
        pointing: false,
        label: Some(Label::new("ab")),
    };
    let row = output_row(&terminal, three, 2);
    assert!(terminal.decorate(1, (0, row), (6, row), look));
    // One in the fold shows nothing.
    let hidden = output_row(&terminal, two, 9);
    assert!(terminal.decorate(2, (0, hidden), (2, hidden), look));
    terminal.snapshot(&mut snapshot);
    let spans: Vec<(u64, u16)> = snapshot
        .decorations()
        .iter()
        .map(|span| (span.id, span.row))
        .collect();
    assert_eq!(spans, [(1, 6)]);
    // The prompts' marks at their rows, `three`'s below the placeholder.
    let marks: Vec<u16> = snapshot.gutter().iter().map(|mark| mark.row).collect();
    assert_eq!(marks, [2, 4]);
    // `two`'s block ends on the placeholder's row, as its end's row is the
    // next prompt's, and `three`'s 45 rows below the view, its output's last.
    let blocks: Vec<(i32, i32)> = snapshot
        .blocks()
        .iter()
        .map(|block| (block.first, block.last))
        .collect();
    assert_eq!(blocks[1..], [(2, 3), (4, 54)]);
}

#[test]
fn a_row_below_a_fold_is_its_own_row_by_its_number_and_on_the_screen() {
    let (terminal, mut snapshot, [_, two, three]) = folded_two();
    let three_1 = output_row(&terminal, three, 1);
    assert_eq!(snapshot.shown_row(5), Some(ShownRow::Row(three_1)));
    let (first, last) = (
        output_row(&terminal, two, 1),
        output_row(&terminal, two, 9_800),
    );
    assert_eq!(
        snapshot.shown_row(3),
        Some(ShownRow::Folded {
            command: two,
            whole: false,
            first,
            last
        })
    );
    assert_eq!(snapshot.view_y(three_1), Some(5));
    assert_eq!(snapshot.view_y(first + 500), Some(3));
    assert_eq!(snapshot.shown_row(10), None);
    // Above the screen, no row of it.
    assert_eq!(snapshot.screen_row(5), None);
    // At the screen, of a short session folded on it, the rows below the
    // placeholder are the screen's rows past the fold's.
    let mut terminal = Terminal::new(40, 10, Options::default());
    feed(&mut terminal, &command("ls", 5));
    feed(&mut terminal, "\x1b]133;A\x07$ \x1b]133;B\x07");
    let ls = terminal.commands()[0].id;
    assert!(terminal.fold(ls, false));
    terminal.snapshot(&mut snapshot);
    assert_eq!(snapshot.screen_row(0), Some(0));
    assert_eq!(snapshot.screen_row(1), Some(1));
    assert_eq!(snapshot.screen_row(2), Some(6));
    assert_eq!(snapshot.screen_row(6), None);
    // The view's cells, the placeholder's empty.
    let cells = terminal.view_cells();
    assert_eq!(cells.len(), 10);
    assert_eq!(cells[0].text.trim_end(), "$ ls");
    assert_eq!(cells[1].text, "");
    assert_eq!(cells[2].text.trim_end(), "$");
}
