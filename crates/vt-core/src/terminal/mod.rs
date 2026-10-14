//! The terminal state: the screens with their cursor, the modes, and what
//! the parser's actions do to them.

mod charsets;
mod control;
mod csi;
mod cursor;
mod edit;
mod erase;
mod escape;
mod print;

pub use charsets::{Charset, Charsets, Shift};

use crate::page::{Cell, Content, PageList, Scrollback, Style, Width};
use crate::parser::{Handler, Params};
use crate::unicode::{Normalization, Segmenter, Widths};

/// What a terminal is made with.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// The limits of the primary screen's scrollback. The alternate screen
    /// has none.
    pub scrollback: Scrollback,
    /// How the terminal counts the cells of text.
    pub widths: Widths,
    /// The normal form that printed text takes.
    pub normalization: Normalization,
}

/// Where the cursor of a screen is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cursor {
    /// The column, from 0.
    pub x: u16,
    /// The row of the screen, from 0.
    pub y: u16,
    /// Printing reached the right margin: the cursor stays on the last
    /// column, and the next character goes to the next row first.
    pub pending_wrap: bool,
}

/// The shape of the cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorShape {
    /// A block over the cell.
    Block,
    /// A line under the cell.
    Underline,
    /// A bar at the left of the cell.
    Bar,
}

/// How the cursor looks, as a program sets it with DECSCUSR.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CursorStyle {
    /// Its shape.
    pub shape: CursorShape,
    /// Whether it blinks.
    pub blinking: bool,
}

/// The margins: the rows from `top` to `bottom` and the columns from `left`
/// to `right`, all included, within which the screen scrolls and origin
/// mode places the cursor. The whole screen until a program sets them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Margins {
    top: u16,
    bottom: u16,
    left: u16,
    right: u16,
}

impl Margins {
    /// The margins of the whole of a screen of `cols` columns and `rows`
    /// rows.
    fn screen(cols: u16, rows: u16) -> Self {
        Self {
            top: 0,
            bottom: rows - 1,
            left: 0,
            right: cols - 1,
        }
    }
}

/// What DECSC saves of a screen, and DECRC brings back.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SavedCursor {
    cursor: Cursor,
    pen: Style,
    charsets: Charsets,
    origin: bool,
    protected: bool,
}

/// A screen: its rows, its cursor, and the cursor that DECSC saved, if
/// any. Each screen saves its own.
#[derive(Debug)]
struct Screen {
    rows: PageList,
    cursor: Cursor,
    saved: Option<SavedCursor>,
}

/// The cell where the last cluster was printed, which a codepoint that
/// joins the cluster goes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Cluster {
    x: u16,
    y: u16,
}

/// A terminal: what programs see, and what they change by writing to it.
///
/// It implements [`Handler`], so a [`Parser`](crate::parser::Parser) drives
/// it with the bytes that programs write.
#[derive(Debug)]
pub struct Terminal {
    cols: u16,
    rows: u16,
    primary: Screen,
    alternate: Screen,
    alternate_active: bool,
    widths: Widths,
    normalization: Normalization,
    /// The codepoints of a cluster being normalized, and its normal form,
    /// kept to be reused.
    scratch: (Vec<char>, Vec<char>),
    /// The style that printed text takes.
    pen: Style,
    /// Whether printed text is protected from the selective erases
    /// (DECSCA).
    protected: bool,
    /// Autowrap (DECAWM): printing past the right margin goes on to the
    /// next row.
    autowrap: bool,
    /// Insert mode (IRM): printing pushes the cells after it to the right.
    insert: bool,
    /// Whether each column is a tab stop.
    tab_stops: Box<[bool]>,
    margins: Margins,
    /// Origin mode (DECOM): the cursor's positions count from the top left
    /// of the margins, and stay within them.
    origin: bool,
    /// How a program asked the cursor to look; `None` leaves it to the
    /// configuration.
    cursor_style: Option<CursorStyle>,
    charsets: Charsets,
    /// Application keypad mode (DECKPAM): the keypad sends sequences
    /// rather than the characters on its keys.
    keypad_application: bool,
    /// Where the clusters of the printed text start.
    segmenter: Segmenter,
    /// The last cluster printed, until the cursor moves.
    cluster: Option<Cluster>,
    /// The last character given to print, before the character sets, which
    /// REP repeats.
    last_printed: Option<char>,
    bells: u64,
}

impl Terminal {
    /// A terminal of `cols` columns and `rows` rows, both at least 1, with
    /// empty screens and the cursor at the top left.
    pub fn new(cols: u16, rows: u16, options: Options) -> Self {
        let (cols, rows) = (cols.max(1), rows.max(1));
        let screen = |scrollback| Screen {
            rows: PageList::new(cols, rows, scrollback),
            cursor: Cursor::default(),
            saved: None,
        };
        let no_scrollback = Scrollback {
            limit: 0,
            budget: options.scrollback.budget.clone(),
        };
        Self {
            cols,
            rows,
            primary: screen(options.scrollback),
            alternate: screen(no_scrollback),
            alternate_active: false,
            widths: options.widths,
            normalization: options.normalization,
            scratch: (Vec::new(), Vec::new()),
            pen: Style::DEFAULT,
            protected: false,
            autowrap: true,
            insert: false,
            tab_stops: (0..cols).map(|x| x > 0 && x.is_multiple_of(8)).collect(),
            margins: Margins::screen(cols, rows),
            origin: false,
            cursor_style: None,
            charsets: Charsets::default(),
            keypad_application: false,
            segmenter: Segmenter::new(),
            cluster: None,
            last_printed: None,
            bells: 0,
        }
    }

    /// How many columns the screens have.
    pub fn cols(&self) -> u16 {
        self.cols
    }

    /// How many rows the screens have.
    pub fn rows(&self) -> u16 {
        self.rows
    }

    /// The cursor of the active screen.
    pub fn cursor(&self) -> Cursor {
        self.screen().cursor
    }

    /// The rows of the active screen, with the scrollback on the primary
    /// one.
    pub fn screen_rows(&self) -> &PageList {
        &self.screen().rows
    }

    /// Whether the alternate screen is the active one.
    pub fn is_alternate_screen(&self) -> bool {
        self.alternate_active
    }

    /// Makes the alternate screen, or the primary one, the active one. Each
    /// keeps its rows and its cursor.
    pub fn set_alternate_screen(&mut self, alternate: bool) {
        self.alternate_active = alternate;
        self.moved();
    }

    /// How the terminal counts the cells of text.
    pub fn widths(&self) -> Widths {
        self.widths
    }

    /// The normal form that printed text takes.
    pub fn normalization(&self) -> Normalization {
        self.normalization
    }

    /// Sets the normal form that printed text takes, from the next
    /// character on.
    pub fn set_normalization(&mut self, normalization: Normalization) {
        self.normalization = normalization;
    }

    /// The style that printed text takes.
    pub fn pen(&self) -> &Style {
        &self.pen
    }

    /// Sets the style that printed text takes.
    pub fn set_pen(&mut self, style: Style) {
        self.pen = style;
    }

    /// Whether autowrap (DECAWM) is on.
    pub fn autowrap(&self) -> bool {
        self.autowrap
    }

    /// Turns autowrap (DECAWM) on or off.
    pub fn set_autowrap(&mut self, on: bool) {
        self.autowrap = on;
    }

    /// Whether insert mode (IRM) is on.
    pub fn insert_mode(&self) -> bool {
        self.insert
    }

    /// Turns insert mode (IRM) on or off.
    pub fn set_insert_mode(&mut self, on: bool) {
        self.insert = on;
    }

    /// The character sets, and which of them is in use.
    pub fn charsets(&self) -> Charsets {
        self.charsets
    }

    /// Whether origin mode (DECOM) is on.
    pub fn origin_mode(&self) -> bool {
        self.origin
    }

    /// Turns origin mode (DECOM) on or off, which puts the cursor at the
    /// top left of the margins, or of the screen.
    pub fn set_origin_mode(&mut self, on: bool) {
        self.origin = on;
        self.goto(0, 0);
    }

    /// How a program asked the cursor to look (DECSCUSR), or `None` for the
    /// configured cursor.
    pub fn cursor_style(&self) -> Option<CursorStyle> {
        self.cursor_style
    }

    /// Whether the keypad is in application mode (DECKPAM) rather than in
    /// numeric mode (DECKPNM).
    pub fn keypad_application(&self) -> bool {
        self.keypad_application
    }

    /// How many times BEL has rung.
    pub fn bells(&self) -> u64 {
        self.bells
    }

    /// The text of row `y` of the active screen: a character per cell, a
    /// space for an empty one, with the trailing spaces taken off.
    pub fn row_text(&self, y: u16) -> String {
        let (page, row) = self.screen().rows.screen_row(y);
        let mut text = String::new();
        for (x, cell) in (0..).zip(page.cells(row)) {
            match (cell.width(), cell.content()) {
                (Width::SpacerTail | Width::SpacerHead, _) => {}
                (_, Content::Char('\0') | Content::BgPalette(_) | Content::BgRgb(_)) => {
                    text.push(' ');
                }
                (_, Content::Char(c)) => text.push(c),
                (_, Content::Grapheme(c)) => {
                    text.push(c);
                    text.extend(page.grapheme(row, x));
                }
            }
        }
        text.truncate(text.trim_end_matches(' ').len());
        text
    }

    fn screen(&self) -> &Screen {
        if self.alternate_active {
            &self.alternate
        } else {
            &self.primary
        }
    }

    fn screen_mut(&mut self) -> &mut Screen {
        if self.alternate_active {
            &mut self.alternate
        } else {
            &mut self.primary
        }
    }

    /// The cell at column `x` of row `y` of the active screen.
    fn cell(&self, x: u16, y: u16) -> Cell {
        let (page, row) = self.screen().rows.screen_row(y);
        page.cells(row)[usize::from(x)]
    }

    /// Puts `cell`, whose style must be in its page's table, at column `x`
    /// of row `y` of the active screen.
    fn put(&mut self, x: u16, y: u16, cell: Cell) {
        let (page, row) = self.screen_mut().rows.screen_row_mut(y);
        page.set_cell(row, x, cell);
    }

    /// The cursor moved, or something other than text came: the next
    /// codepoint starts a cluster of its own.
    fn moved(&mut self) {
        self.segmenter.reset();
        self.cluster = None;
    }
}

impl Handler for Terminal {
    fn print(&mut self, c: char) {
        self.print_translated(c);
    }

    fn print_ascii(&mut self, text: &[u8]) {
        self.print_ascii_run(text);
    }

    fn execute(&mut self, byte: u8) {
        self.control(byte);
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], final_byte: u8) {
        self.escape(intermediates, final_byte);
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], final_byte: u8) {
        self.csi(params, intermediates, final_byte);
    }
}

#[cfg(test)]
mod tests;
