//! The screen of a terminal as text, for people and for golden files.

use std::fmt::Write as _;

use saetta_vt_core::page::{Color, Content, Rgb, Style, Underline};
use saetta_vt_core::terminal::Terminal;

/// Every row of the screen, a line each, trailing spaces taken off.
pub fn text(terminal: &Terminal) -> String {
    let mut out = String::new();
    for y in 0..terminal.rows() {
        out.push_str(&terminal.row_text(y));
        out.push('\n');
    }
    out
}

/// The rows of the screen, each followed by the attributes of its cells, if
/// any, then the cursor.
///
/// An attribute line lists runs of cells, by their first and last column,
/// as in `  0-4 bold fg=1`.
pub fn with_attributes(terminal: &Terminal) -> String {
    let mut out = String::new();
    for y in 0..terminal.rows() {
        out.push_str(&terminal.row_text(y));
        out.push('\n');
        let runs = runs(terminal, y);
        if !runs.is_empty() {
            let _ = writeln!(out, "  {}", runs.join("; "));
        }
    }
    let cursor = terminal.cursor();
    let _ = write!(out, "cursor {},{}", cursor.x, cursor.y);
    if cursor.pending_wrap {
        out.push_str(" pending wrap");
    }
    out.push('\n');
    out
}

/// The runs of cells of row `y` that share attributes other than the
/// default.
fn runs(terminal: &Terminal, y: u16) -> Vec<String> {
    let (page, row) = terminal.screen_rows().screen_row(y);
    let mut runs: Vec<(u16, u16, String)> = Vec::new();
    for (x, cell) in (0..).zip(page.cells(row)) {
        let mut attributes = describe(page.styles().get(cell.style()));
        match cell.content() {
            Content::BgPalette(index) => attributes.push(format!("bg={index}")),
            Content::BgRgb(rgb) => attributes.push(format!("bg={}", hex(rgb))),
            Content::Char(_) | Content::Grapheme(_) => {}
        }
        let attributes = attributes.join(" ");
        // The second half of a wide character has the style of the first, so
        // it joins its run.
        match runs.last_mut() {
            Some((_, last, previous)) if *last + 1 == x && *previous == attributes => *last = x,
            _ if attributes.is_empty() => {}
            _ => runs.push((x, x, attributes)),
        }
    }
    runs.into_iter()
        .map(|(first, last, attributes)| format!("{first}-{last} {attributes}"))
        .collect()
}

/// The attributes of `style`, the default ones aside.
fn describe(style: &Style) -> Vec<String> {
    let mut attributes = Vec::new();
    let flags = [
        (style.bold, "bold"),
        (style.faint, "faint"),
        (style.italic, "italic"),
        (style.blink, "blink"),
        (style.inverse, "inverse"),
        (style.invisible, "invisible"),
        (style.strikethrough, "strike"),
        (style.overline, "overline"),
    ];
    attributes.extend(
        flags
            .iter()
            .filter(|(on, _)| *on)
            .map(|(_, name)| (*name).to_owned()),
    );
    let underline = match style.underline {
        Underline::None => None,
        Underline::Single => Some("single"),
        Underline::Double => Some("double"),
        Underline::Curly => Some("curly"),
        Underline::Dotted => Some("dotted"),
        Underline::Dashed => Some("dashed"),
    };
    if let Some(underline) = underline {
        attributes.push(format!("underline={underline}"));
    }
    for (name, color) in [
        ("fg", style.fg),
        ("bg", style.bg),
        ("ul", style.underline_color),
    ] {
        match color {
            Color::Default => {}
            Color::Palette(index) => attributes.push(format!("{name}={index}")),
            Color::Rgb(rgb) => attributes.push(format!("{name}={}", hex(rgb))),
        }
    }
    attributes
}

fn hex(Rgb { r, g, b }: Rgb) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

#[cfg(test)]
mod tests {
    use saetta_vt_core::parser::Parser;
    use saetta_vt_core::terminal::Options;

    use super::*;

    fn terminal(cols: u16, rows: u16, steps: &[(Style, &str)]) -> Terminal {
        let mut terminal = Terminal::new(cols, rows, Options::default());
        let mut parser = Parser::new();
        for (style, text) in steps {
            terminal.set_pen(*style);
            parser.advance(&mut terminal, text.as_bytes());
        }
        terminal
    }

    #[test]
    fn the_text_has_a_line_per_row() {
        let terminal = terminal(10, 3, &[(Style::DEFAULT, "hello\r\nwor中ld")]);
        assert_eq!(text(&terminal), "hello\nwor中ld\n\n");
    }

    #[test]
    fn attributes_come_in_runs_under_their_row() {
        let bold_red = Style {
            bold: true,
            fg: Color::Palette(1),
            ..Style::DEFAULT
        };
        let curly = Style {
            underline: Underline::Curly,
            underline_color: Color::Rgb(Rgb::new(0xff, 0x80, 0)),
            ..Style::DEFAULT
        };
        let terminal = terminal(
            12,
            2,
            &[
                (Style::DEFAULT, "ab"),
                (bold_red, "cd中"),
                (Style::DEFAULT, " "),
                (curly, "x"),
            ],
        );
        assert_eq!(
            with_attributes(&terminal),
            "abcd中 x\n  2-5 bold fg=1; 7-7 underline=curly ul=#ff8000\n\ncursor 8,0\n"
        );
    }

    #[test]
    fn the_cursor_says_when_the_wrap_is_pending() {
        let terminal = terminal(3, 1, &[(Style::DEFAULT, "abc")]);
        assert_eq!(with_attributes(&terminal), "abc\ncursor 2,0 pending wrap\n");
    }
}
