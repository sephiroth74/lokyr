//! The terminal state: the screens with their cursor, the modes, and what
//! the parser's actions do to them.

mod charsets;
mod clipboard;
mod colors;
mod control;
mod csi;
mod cursor;
mod edit;
mod erase;
mod escape;
mod modes;
mod osc;
mod print;
mod reports;
mod sgr;

pub use charsets::{Charset, Charsets, Shift};
pub use clipboard::{ClipboardRequest, ClipboardTarget};
pub use colors::Colors;
pub use osc::{Event, Progress, WorkingDirectory};

use std::ops::Range;

use crate::page::{Cell, Content, PageList, Rgb, Scrollback, Style, Width};
use crate::parser::{Handler, Params, StringEnd};
use crate::unicode::{Normalization, Segmenter, Widths};

/// What a terminal is made with.
#[derive(Clone, Debug)]
pub struct Options {
    /// The limits of the primary screen's scrollback. The alternate screen
    /// has none.
    pub scrollback: Scrollback,
    /// How the terminal counts the cells of text.
    pub widths: Widths,
    /// The normal form that printed text takes.
    pub normalization: Normalization,
    /// The `TERM` that programs see, which XTGETTCAP reports as `TN`:
    /// `xterm-256color` by default.
    pub term: String,
    /// Whether programs may read the title back (CSI 20 t and 21 t), the
    /// classic way of injecting text into a shell: off by default.
    pub title_reports: bool,
    /// Whether programs may ask to read the clipboard (OSC 52 with `?`),
    /// which the embedder still confirms: off by default.
    pub clipboard_reads: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            scrollback: Scrollback::default(),
            widths: Widths::default(),
            normalization: Normalization::default(),
            term: "xterm-256color".to_owned(),
            title_reports: false,
            clipboard_reads: false,
        }
    }
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

/// Which mouse events a program asks for (modes 9, 1000, 1002 and 1003).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MouseTracking {
    /// None.
    #[default]
    None,
    /// Presses only, as X10 sent them (9).
    X10,
    /// Presses and releases (1000).
    Normal,
    /// Presses, releases, and motion with a button down (1002).
    ButtonEvent,
    /// Presses, releases, and any motion (1003).
    AnyEvent,
}

/// How mouse events are encoded (modes 1006 and 1016).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MouseEncoding {
    /// Bytes after `CSI M`, as X10 encoded them.
    #[default]
    X10,
    /// `CSI <` with decimal numbers, in cells (1006).
    Sgr,
    /// The same, in pixels (1016).
    SgrPixels,
}

/// The modes that change what keys, the mouse and pastes send to the
/// program.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputModes {
    /// The cursor keys send application sequences (DECCKM, mode 1).
    pub cursor_keys_application: bool,
    /// The keypad sends application sequences (DECKPAM, rather than
    /// DECKPNM).
    pub keypad_application: bool,
    /// Return sends CR LF rather than CR (LNM).
    pub newline: bool,
    /// The mouse events the program asks for.
    pub mouse_tracking: MouseTracking,
    /// How they are encoded.
    pub mouse_encoding: MouseEncoding,
    /// Focus changes send `CSI I` and `CSI O` (mode 1004).
    pub focus_events: bool,
    /// On the alternate screen, the wheel sends cursor keys (mode 1007).
    pub alternate_scroll: bool,
    /// Meta, or Option, sends ESC before the key (mode 1036).
    pub meta_sends_escape: bool,
    /// Pastes come between `CSI 200 ~` and `CSI 201 ~` (mode 2004).
    pub bracketed_paste: bool,
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

    /// The columns from the left margin to the right one.
    fn columns(self) -> Range<u16> {
        self.left..self.right + 1
    }

    /// Whether column `x` is between the left and right margins.
    fn has_column(self, x: u16) -> bool {
        (self.left..=self.right).contains(&x)
    }

    /// Whether the left and right margins are the edges of a screen of
    /// `cols` columns.
    fn is_full_width(self, cols: u16) -> bool {
        self.left == 0 && self.right == cols - 1
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
    /// Left and right margin mode (DECLRMM): DECSLRM can set the left and
    /// right margins, which are otherwise the edges of the screen.
    left_right_margins: bool,
    /// Origin mode (DECOM): the cursor's positions count from the top left
    /// of the margins, and stay within them.
    origin: bool,
    /// How a program asked the cursor to look; `None` leaves it to the
    /// configuration.
    cursor_style: Option<CursorStyle>,
    charsets: Charsets,
    input: InputModes,
    /// Reverse video (DECSCNM, mode 5): the screen's colors swap.
    reverse_video: bool,
    /// The cursor shows (DECTCEM, mode 25).
    cursor_visible: bool,
    /// The cursor blinks (mode 12).
    cursor_blink: bool,
    /// Reverse wraparound (mode 45): with autowrap, moving left past the
    /// left margin goes on at the end of the row above, when that row wraps
    /// into this one.
    reverse_wrap: bool,
    /// Synchronized output (mode 2026): the screen should not be drawn
    /// until the program ends the update.
    synchronized_output: bool,
    /// The widths the terminal was made with, which RIS brings back after
    /// mode 2027 changed them.
    configured_widths: Widths,
    /// What the terminal sends back to the program, such as replies to
    /// queries, for the embedder to write to the PTY.
    replies: Vec<u8>,
    /// The `TERM` that programs see.
    term: String,
    /// Whether programs may read the title back.
    title_reports: bool,
    /// Whether programs may ask to read the clipboard.
    clipboard_reads: bool,
    /// The theme's colors, which the resets bring back, and the colors
    /// now, which programs can change.
    default_colors: Colors,
    colors: Colors,
    /// The size of a cell in pixels, width then height, once the embedder
    /// knows it.
    cell_size: Option<(u16, u16)>,
    /// The window's title (OSC 2) and the icon's (OSC 1).
    title: String,
    icon_title: String,
    /// The titles that CSI 22 t pushed, the latest last.
    title_stack: Vec<(String, String)>,
    /// The OSC string being read.
    osc: Vec<u8>,
    /// What the embedder has to act on, oldest first.
    events: Vec<Event>,
    /// The working directory that OSC 7 gave last.
    working_directory: Option<WorkingDirectory>,
    /// The tab's color, from OSC 6;1.
    tab_color: Option<Rgb>,
    /// The DCS query being read, DECRQSS's or XTGETTCAP's, and its bytes.
    query: Option<reports::Query>,
    query_bytes: Vec<u8>,
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
            left_right_margins: false,
            origin: false,
            cursor_style: None,
            charsets: Charsets::default(),
            input: InputModes::default(),
            reverse_video: false,
            cursor_visible: true,
            cursor_blink: false,
            reverse_wrap: false,
            synchronized_output: false,
            configured_widths: options.widths,
            replies: Vec::new(),
            term: options.term,
            title_reports: options.title_reports,
            clipboard_reads: options.clipboard_reads,
            default_colors: Colors::default(),
            colors: Colors::default(),
            cell_size: None,
            title: String::new(),
            icon_title: String::new(),
            title_stack: Vec::new(),
            query: None,
            query_bytes: Vec::new(),
            osc: Vec::new(),
            events: Vec::new(),
            working_directory: None,
            tab_color: None,
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

    /// Whether left and right margin mode (DECLRMM) is on.
    pub fn left_right_margin_mode(&self) -> bool {
        self.left_right_margins
    }

    /// Turns left and right margin mode (DECLRMM) on or off. Off, the left
    /// and right margins go back to the edges of the screen.
    pub fn set_left_right_margin_mode(&mut self, on: bool) {
        self.left_right_margins = on;
        if !on {
            self.margins.left = 0;
            self.margins.right = self.cols - 1;
        }
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
        self.input.keypad_application
    }

    /// The modes that change what keys, the mouse and pastes send.
    pub fn input_modes(&self) -> InputModes {
        self.input
    }

    /// Whether reverse video (DECSCNM) is on.
    pub fn reverse_video(&self) -> bool {
        self.reverse_video
    }

    /// Whether the cursor shows (DECTCEM).
    pub fn cursor_visible(&self) -> bool {
        self.cursor_visible
    }

    /// Whether the program asked the cursor to blink (mode 12).
    pub fn cursor_blink(&self) -> bool {
        self.cursor_blink
    }

    /// Whether reverse wraparound (mode 45) is on.
    pub fn reverse_wrap(&self) -> bool {
        self.reverse_wrap
    }

    /// Whether a synchronized update (mode 2026) is under way.
    pub fn synchronized_output(&self) -> bool {
        self.synchronized_output
    }

    /// The window's title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// The icon's title.
    pub fn icon_title(&self) -> &str {
        &self.icon_title
    }

    /// The working directory that OSC 7 gave last.
    pub fn working_directory(&self) -> Option<&WorkingDirectory> {
        self.working_directory.as_ref()
    }

    /// The tab's color, from OSC 6;1.
    pub fn tab_color(&self) -> Option<Rgb> {
        self.tab_color
    }

    /// The colors to draw with: the theme's, as programs changed them.
    pub fn colors(&self) -> &Colors {
        &self.colors
    }

    /// Sets the theme's colors, which become the colors to draw with, and
    /// those that the resets bring back.
    pub fn set_default_colors(&mut self, colors: Colors) {
        self.colors.clone_from(&colors);
        self.default_colors = colors;
    }

    /// Takes what the embedder has to act on, oldest first.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// Sets the size of a cell in pixels, which CSI 14 t and 16 t report;
    /// until then, they get no reply.
    pub fn set_cell_size(&mut self, width: u16, height: u16) {
        self.cell_size = Some((width, height));
    }

    /// Takes what the terminal has to send back to the program, such as
    /// replies to queries, oldest first, for the embedder to write to the
    /// PTY.
    pub fn take_replies(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.replies)
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

    fn osc_start(&mut self) {
        self.start_osc();
    }

    fn osc_put(&mut self, data: &[u8]) {
        self.read_osc(data);
    }

    fn osc_end(&mut self, end: StringEnd) {
        self.end_osc(end);
    }

    fn dcs_start(&mut self, _params: &Params, intermediates: &[u8], final_byte: u8) {
        self.start_query(intermediates, final_byte);
    }

    fn dcs_put(&mut self, data: &[u8]) {
        self.read_query(data);
    }

    fn dcs_end(&mut self, end: StringEnd) {
        self.end_query(end);
    }
}

#[cfg(test)]
mod tests;
