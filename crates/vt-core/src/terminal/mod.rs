//! The terminal state: the screens with their cursor, the modes, and what
//! the parser's actions do to them.

mod charsets;
mod clipboard;
mod colors;
mod control;
mod csi;
mod cursor;
mod decorations;
mod edit;
mod erase;
mod escape;
mod events;
mod gutter;
mod keyboard;
mod marks;
mod matches;
mod modes;
mod osc;
mod print;
mod rectangles;
mod reports;
mod selection;
mod sgr;
mod snapshot;
mod terminfo;
mod viewport;
mod window;

pub use charsets::{Charset, Charsets, Shift};
pub use clipboard::{ClipboardRequest, ClipboardTarget};
pub use colors::{ColorScheme, Colors, SPECIAL_COLORS};
pub use decorations::{Decorated, Look, MOST_DECORATIONS};
pub use gutter::{GutterKind, GutterMark, MOST_GUTTER_MARKS};
pub use keyboard::KeyboardFlags;
pub use marks::{CommandMarks, MOST_COMMANDS};
pub use matches::{Highlighted, MatchCells};
pub use osc::{Event, Progress, WorkingDirectory};
pub use selection::{
    CellPin, RowCells, RowText, Selected, SelectionShape, SelectionUnit, WORD_SEPARATORS,
};
pub use snapshot::{Line, Snapshot, SnapshotCursor};
pub use viewport::{Origin, Scroll};
pub use window::Window;

use std::ops::Range;

use crate::page::{
    Cell, Content, Grow, PageList, PinId, Position, Rgb, RowFlag, Scrollback, Semantic, Style,
    StyleId, Width,
};
use crate::parser::{Handler, Params, StringEnd};
use crate::unicode::{Normalization, Segmenter, Tables, Widths};

/// The reply to DA: a VT220 with ANSI colors.
const DA1: &[u8] = b"\x1b[?62;22c";

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
    /// The version that XTVERSION reports after the terminal's name: the
    /// crate's by default.
    pub version: String,
    /// Whether programs may read the title back (CSI 20 t and 21 t), the
    /// classic way of injecting text into a shell: off by default.
    pub title_reports: bool,
    /// Whether programs may ask to read the clipboard (OSC 52 with `?`),
    /// which the embedder still confirms: off by default.
    pub clipboard_reads: bool,
    /// Whether programs may ask for the checksum of a rectangle of the
    /// screen (DECRQCRA), through which a program can read the screen, cell
    /// by cell, what others wrote there too, as test suites do: off by
    /// default.
    pub rectangle_checksums: bool,
    /// The schemes that links (OSC 8) may have, in any case: `http`,
    /// `https`, `mailto` and `file` by default.
    pub link_schemes: Vec<String>,
    /// Whether programs may operate on the window, as xterm's
    /// `allowWindowOps` lets them: change the terminal's size (XTWINOPS 4
    /// t and 8 t, DECSLPP and DECSNLS), which the embedder then gives the
    /// window, iconify, move, maximize the window or make it fill the
    /// display, and ask where it is and how large the display is: off by
    /// default.
    pub window_ops: bool,
    /// Whether output brings the view back to the screen from the
    /// scrollback, where input always does: off by default, so that the
    /// view stays where it was scrolled to.
    pub scroll_on_output: bool,
    /// Whether alternate scroll (mode 1007) is on as the terminal starts,
    /// and as a full reset makes it again: off by default, as xterm's
    /// `alternateScroll` is.
    pub alternate_scroll: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            scrollback: Scrollback::default(),
            widths: Widths::default(),
            normalization: Normalization::default(),
            term: "xterm-256color".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            title_reports: false,
            clipboard_reads: false,
            rectangle_checksums: false,
            link_schemes: ["http", "https", "mailto", "file"]
                .map(str::to_owned)
                .to_vec(),
            window_ops: false,
            scroll_on_output: false,
            alternate_scroll: false,
        }
    }
}

/// The most columns, or rows, that a program can ask the terminal to take.
pub const ASKED_SIZE: u16 = 1000;

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

/// What a shell draws again of its prompt once the terminal's size
/// changes, as `OSC 133 ; A`'s `redraw` option tells it: kitty's `0` and
/// `1`, and Ghostty's `last`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Redraw {
    /// The whole prompt, as zsh does: `redraw=1`, and the default.
    #[default]
    Whole,
    /// Its last line alone, as bash does: `redraw=last`.
    Last,
    /// Nothing: `redraw=0`.
    Nothing,
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
    /// Backspace sends BS, and Control with it DEL, rather than the other
    /// way round (DECBKM, mode 67).
    pub backspace_sends_bs: bool,
    /// xterm's `modifyOtherKeys`, which `CSI > 4 ; n m` sets: 0, keys as
    /// usual; 1, the keys whose usual sequence would lose a modifier as
    /// `CSI 27 ; modifiers ; code ~`; 2, every modified key so.
    pub modify_other_keys: u8,
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

/// A screen: its rows, its cursor, the cursor that DECSC saved, if any,
/// and the flags of the kitty keyboard protocol. Each screen keeps its own.
#[derive(Debug)]
struct Screen {
    rows: PageList,
    cursor: Cursor,
    saved: Option<SavedCursor>,
    keyboard: keyboard::KeyboardStack,
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
    /// The buffers of a stretch being printed, kept to be reused.
    stretch: print::Stretch,
    /// The style that printed text takes.
    pen: Style,
    /// Whether the pen is the default style, which printed text then takes
    /// without the page's table: [`Terminal::pen_changed`] keeps it.
    pen_default: bool,
    /// The id that the pen's style last had in a page, which the next page
    /// written to is likely to give it too.
    pen_id: StyleId,
    /// Whether printed text is protected from the erases (DECSCA, or SPA
    /// and EPA).
    protected: bool,
    /// Whether the protection is ISO's, which SPA set, and which ED, EL and
    /// ECH respect, rather than DEC's, which DECSCA set, and which the
    /// selective erases respect.
    iso_protection: bool,
    /// Autowrap (DECAWM): printing past the right margin goes on to the
    /// next row.
    autowrap: bool,
    /// xterm's fix for a bug of curses (mode 41): a tab at a pending wrap
    /// wraps first.
    tab_wraps: bool,
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
    /// Smooth scrolling (DECSCLM, mode 4), which a program asks for and the
    /// screen keeps for whoever draws it; it scrolls at once.
    smooth_scroll: bool,
    /// The conformance level that DECSCL set, 61 or 62: 62 at first, the
    /// VT220 that DA tells of.
    conformance: u16,
    /// The extent of the attribute changes in rectangles that DECSACE set:
    /// 0 or 1 the stream of characters, 2 the rectangle.
    attribute_extent: u16,
    /// Whether DECCOLM may switch between 80 and 132 columns (mode 40),
    /// whether it did to 132 (mode 3), and whether the switch keeps the
    /// screen rather than erase it (DECNCSM, mode 95).
    column_switch: bool,
    columns_132: bool,
    keep_screen: bool,
    /// The DEC private modes that XTSAVE saved, with their states, for
    /// XTRESTORE.
    saved_modes: Vec<(u16, bool)>,
    /// Reverse wraparound (mode 45): with autowrap, moving left past the
    /// left margin goes on at the end of the row above, when that row wraps
    /// into this one.
    reverse_wrap: bool,
    /// Extended reverse wraparound (mode 1045), xterm's mode 45 as it was:
    /// with autowrap, moving left past the left margin goes on at the end
    /// of the row above, whether it wraps or not, and past the top margin
    /// at the end of the bottom one.
    reverse_wrap_extended: bool,
    /// Synchronized output (mode 2026): the screen should not be drawn
    /// until the program ends the update.
    synchronized_output: bool,
    /// The reports of the color scheme (mode 2031): as the theme's colors
    /// change, the program is told their scheme, as `CSI ? 996 n` answers.
    color_scheme_reports: bool,
    /// How many synchronized updates programs started.
    synchronized_updates: u64,
    /// The widths the terminal was made with, which RIS brings back after
    /// mode 2027 changed them.
    configured_widths: Widths,
    /// Whether alternate scroll is on as the terminal starts.
    configured_alternate_scroll: bool,
    /// What the terminal sends back to the program, such as replies to
    /// queries, for the embedder to write to the PTY.
    replies: Vec<u8>,
    /// The bytes of the replies to queries among them, which the limit of
    /// the replies counts.
    reply_bytes: usize,
    /// The `TERM` that programs see.
    term: String,
    /// The version that XTVERSION reports.
    version: String,
    /// Whether programs may read the title back.
    title_reports: bool,
    /// Whether programs may ask to read the clipboard.
    clipboard_reads: bool,
    /// Whether programs may ask for the checksum of a rectangle.
    rectangle_checksums: bool,
    /// The schemes that links may have.
    link_schemes: Vec<String>,
    /// Whether programs may operate on the window.
    window_ops: bool,
    /// The width that the embedder gave last, and the scrollback's limit as
    /// it was configured, which the sizes that programs set start from.
    given_cols: u16,
    scrollback_limit: usize,
    /// The link that the text printed next is part of (OSC 8).
    link: osc::Link,
    /// The zone of the text printed next (OSC 133).
    zone: Semantic,
    /// What the shell draws again of its prompt on a resize (OSC 133).
    redraw: Redraw,
    /// The status of the last command that ended, as OSC 133's `D` tells
    /// it.
    command_status: Option<i32>,
    /// The commands that the shell marked, of the primary screen.
    commands: marks::Commands,
    /// The theme's colors, which the resets bring back, and the colors
    /// now, which programs can change.
    default_colors: Colors,
    colors: Colors,
    /// The size of a cell in pixels, width then height, once the embedder
    /// knows it.
    cell_size: Option<(u16, u16)>,
    /// What the terminal knows of its window, the display's size in pixels
    /// once the embedder tells it, and the size before the window was
    /// maximized or filled the display, to bring back.
    window: Window,
    display_size: Option<(u16, u16)>,
    restored_size: Option<(u16, u16)>,
    /// The window's title (OSC 2) and the icon's (OSC 1).
    title: String,
    icon_title: String,
    /// The bytes of the window's title and the icon's as CSI 21 t and 20 t
    /// report them, which [`Terminal::titles_changed`] keeps, so that a
    /// report that the replies have no room for goes without being made.
    reported_titles: (usize, usize),
    /// The titles that CSI 22 t pushed, the latest last.
    title_stack: Vec<(Option<String>, Option<String>)>,
    /// xterm's title modes 0 and 1: titles are set, and reported, in
    /// hexadecimal.
    hex_titles: bool,
    hex_title_reports: bool,
    /// The OSC string being read.
    osc: Vec<u8>,
    /// What the embedder has to act on, oldest first.
    events: events::Events,
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
    /// The first row of the primary screen's view, pinned, while the view
    /// is above the screen; `None` while it shows the screen.
    viewport: Option<PinId>,
    /// Whether output brings the view back to the screen.
    scroll_on_output: bool,
    /// The selection, if there is one.
    selection: Option<selection::Selection>,
    /// The cell that a selection by the mouse grows from, if one was
    /// anchored.
    selection_anchor: Option<selection::CellPin>,
    /// The cell that the embedder marked, as the one that a menu opened
    /// on, if one was.
    cell_mark: Option<selection::CellPin>,
    /// A cursor of the embedder's, which takes the place of the terminal's
    /// in the snapshots, as [`Terminal::hold_cursor`] pins it.
    held_cursor: Option<selection::CellPin>,
    /// The decorations that the embedder laid over the text.
    decorations: Vec<decorations::Decoration>,
    /// The gutter's marks that the embedder laid.
    gutter: Vec<gutter::Laid>,
    /// The matches of the searches that the embedder keeps, few at once.
    matches: Vec<matches::Matches>,
}

impl Terminal {
    /// A terminal of `cols` columns and `rows` rows, at least two columns,
    /// so that a wide character always fits, and one row, with empty
    /// screens and the cursor at the top left.
    pub fn new(cols: u16, rows: u16, options: Options) -> Self {
        let (cols, rows) = (cols.max(2), rows.max(1));
        let screen = |scrollback| Screen {
            rows: PageList::new(cols, rows, scrollback),
            cursor: Cursor::default(),
            saved: None,
            keyboard: keyboard::KeyboardStack::default(),
        };
        let no_scrollback = Scrollback {
            limit: 0,
            budget: options.scrollback.budget.clone(),
        };
        let scrollback_limit = options.scrollback.limit;
        Self {
            cols,
            rows,
            primary: screen(options.scrollback),
            alternate: screen(no_scrollback),
            alternate_active: false,
            widths: options.widths,
            normalization: options.normalization,
            scratch: (Vec::new(), Vec::new()),
            stretch: print::Stretch::default(),
            pen: Style::DEFAULT,
            pen_default: true,
            pen_id: StyleId::DEFAULT,
            protected: false,
            iso_protection: false,
            autowrap: true,
            tab_wraps: false,
            insert: false,
            tab_stops: (0..cols).map(|x| x > 0 && x.is_multiple_of(8)).collect(),
            margins: Margins::screen(cols, rows),
            left_right_margins: false,
            origin: false,
            cursor_style: None,
            charsets: Charsets::default(),
            input: InputModes {
                alternate_scroll: options.alternate_scroll,
                ..InputModes::default()
            },
            reverse_video: false,
            cursor_visible: true,
            cursor_blink: false,
            smooth_scroll: false,
            conformance: 62,
            attribute_extent: 0,
            column_switch: false,
            columns_132: false,
            keep_screen: false,
            saved_modes: Vec::new(),
            reverse_wrap: false,
            reverse_wrap_extended: false,
            synchronized_output: false,
            color_scheme_reports: false,
            synchronized_updates: 0,
            configured_widths: options.widths,
            configured_alternate_scroll: options.alternate_scroll,
            replies: Vec::new(),
            reply_bytes: 0,
            term: options.term,
            version: options.version,
            title_reports: options.title_reports,
            clipboard_reads: options.clipboard_reads,
            rectangle_checksums: options.rectangle_checksums,
            link_schemes: options.link_schemes,
            window_ops: options.window_ops,
            given_cols: cols,
            scrollback_limit,
            link: osc::Link::default(),
            zone: Semantic::Output,
            redraw: Redraw::Whole,
            command_status: None,
            commands: marks::Commands::default(),
            default_colors: Colors::default(),
            colors: Colors::default(),
            cell_size: None,
            window: Window::default(),
            display_size: None,
            restored_size: None,
            title: String::new(),
            icon_title: String::new(),
            reported_titles: (0, 0),
            title_stack: Vec::new(),
            hex_titles: false,
            hex_title_reports: false,
            query: None,
            query_bytes: Vec::new(),
            osc: Vec::new(),
            events: events::Events::default(),
            working_directory: None,
            tab_color: None,
            segmenter: Segmenter::new(),
            cluster: None,
            last_printed: None,
            bells: 0,
            viewport: None,
            scroll_on_output: options.scroll_on_output,
            selection: None,
            selection_anchor: None,
            cell_mark: None,
            held_cursor: None,
            decorations: Vec::new(),
            gutter: Vec::new(),
            matches: Vec::new(),
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

    /// The rows of the active screen, to pin cells in, as search and
    /// selection do. Laying the rows out, by scrolling,
    /// resizing or reflowing them, is the terminal's: doing it here would
    /// leave its cursor and margins behind.
    pub fn screen_rows_mut(&mut self) -> &mut PageList {
        &mut self.screen_mut().rows
    }

    /// The rows of the screen that is not active: the primary one, with
    /// its scrollback, while the alternate one is active, and the other way
    /// around.
    pub fn inactive_screen_rows(&self) -> &PageList {
        if self.alternate_active {
            &self.primary.rows
        } else {
            &self.alternate.rows
        }
    }

    /// Whether the alternate screen is the active one.
    pub fn is_alternate_screen(&self) -> bool {
        self.alternate_active
    }

    /// The cursor that DECSC saved on the primary screen, if any, which
    /// leaving the alternate screen with 1049 brings back.
    pub fn primary_saved_cursor(&self) -> Option<Cursor> {
        self.primary.saved.map(|saved| saved.cursor)
    }

    /// Makes the alternate screen, or the primary one, the active one. Each
    /// keeps its rows and its cursor.
    pub fn set_alternate_screen(&mut self, alternate: bool) {
        // The alternate screen shows whole, and the view comes back to the
        // primary one's screen with it.
        if alternate {
            self.set_viewport_offset(0);
        }
        // The selection is the other screen's.
        if alternate != self.alternate_active {
            self.clear_selection();
        }
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

    /// Sets the width tables that the terminal counts text by, and whether
    /// East Asian Ambiguous characters take two cells, from the next
    /// character on, and as a full reset brings the widths back; the mode
    /// that a program set, by clusters or by codepoints, stays.
    pub fn set_width_tables(&mut self, tables: Tables, ambiguous_wide: bool) {
        for widths in [&mut self.widths, &mut self.configured_widths] {
            widths.tables = tables;
            widths.ambiguous_wide = ambiguous_wide;
        }
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
        self.pen_changed();
    }

    /// Notes whether the pen, which has just changed, is the default style.
    fn pen_changed(&mut self) {
        self.pen_default = self.pen == Style::DEFAULT;
    }

    /// Gives the terminal `cols` columns and `rows` rows, at least two
    /// columns and one row, as [`Terminal::new`] has them. The primary
    /// screen rewraps its lines, as [`PageList::reflow`]
    /// does; the alternate screen's are cut or extended, as
    /// [`PageList::resize`] does, its new rows at the bottom. Each cursor
    /// follows its cell, and the cursors that DECSC saved follow theirs,
    /// coming back within the screen when their text has gone; a wrap stays
    /// pending only at the last column, and otherwise goes on in the cell
    /// after. The margins become the whole screen, the tab stops go on
    /// every eight columns past the old width, and the cluster being
    /// printed ends.
    ///
    /// The width becomes the embedder's, which the sizes that programs set
    /// are measured against, and the scrollback's limit the configured one.
    /// A resize to the size that the terminal has changes nothing, so that
    /// the embedder, giving the window and the PTY a size that a program
    /// set, leaves the program's width a program's, and the scrollback the
    /// rows that it kept there.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        if (cols.max(2), rows.max(1)) == (self.cols, self.rows) {
            return;
        }
        self.clear_prompt(cols);
        self.resize_screens(cols, rows);
        self.given_cols = self.cols;
        let limit = self.scrollback_limit;
        self.primary.rows.set_scrollback_limit(limit);
        // A taller screen that took the view's first row brings the view to
        // the screen, which it follows from now on.
        self.settle_viewport();
        self.resized_selection();
    }

    /// Clears the prompt that the cursor is in, as semantic prompts mark it,
    /// from the row where it starts to the bottom of the screen, before a
    /// resize: the shell draws it again once it learns the new size, from
    /// as many rows above the cursor as it took when the shell drew it,
    /// which a reflow of the prompt would change, leaving the first rows of
    /// a prompt that it made taller above the one drawn again. A shell that
    /// draws the prompt's last line alone has the cursor's line cleared, from
    /// the screen's first row when the line starts above it, and one that
    /// draws nothing keeps its prompt, which reflows. Only the primary
    /// screen's prompt clears, and of a whole prompt only one that starts on
    /// the screen.
    ///
    /// The cleared rows take the default background, whatever the pen's;
    /// the row above no longer goes on into them, as output without a final
    /// newline does under a shell's mark of its end, so that a wider screen
    /// does not join that output to them, which the shell's drawing would
    /// erase; and the cursor comes within the `cols` columns that the screen
    /// takes, so that the reflow does not wrap its empty row down.
    fn clear_prompt(&mut self, cols: u16) {
        if self.alternate_active || self.zone == Semantic::Output {
            return;
        }
        let start = {
            let y = self.cursor().y;
            let rows = &self.screen().rows;
            let has = |y: u16, flag: RowFlag| {
                let (page, row) = rows.screen_row(y);
                page.row(row).has(flag)
            };
            match self.redraw {
                Redraw::Whole => (0..=y).rev().find(|&y| has(y, RowFlag::PromptStart)),
                // The first row of the cursor's line.
                Redraw::Last => Some(
                    (0..=y)
                        .rev()
                        .find(|&y| !has(y, RowFlag::Continuation))
                        .unwrap_or(0),
                ),
                Redraw::Nothing => None,
            }
        };
        let Some(start) = start else {
            return;
        };
        if start > 0 {
            self.end_soft_wrap(start - 1);
        }
        for y in start..self.rows {
            let width = self.cols;
            let (page, row) = self.screen_mut().rows.screen_row_mut(y);
            page.erase(row, 0..width, Cell::EMPTY, false);
            self.end_soft_wrap(y);
        }
        let last = cols.max(2) - 1;
        let cursor = &mut self.screen_mut().cursor;
        if cursor.x > last {
            cursor.x = last;
            cursor.pending_wrap = false;
        }
    }

    /// Clears the scrollback, and the screen but the prompt that the cursor
    /// is in, which goes to the top: from the row where the shell's
    /// semantic prompt starts, while no command runs, or else from the
    /// first row of the cursor's line, to the last row of that line, as
    /// it wraps on. The program is told nothing: a shell that draws its
    /// line from the cursor goes on where it is. The alternate screen,
    /// which has no scrollback, keeps what it shows.
    pub fn clear_to_prompt(&mut self) {
        if self.alternate_active {
            return;
        }
        let y = self.cursor().y;
        let (start, end) = {
            let rows = &self.screen().rows;
            let has = |y: u16, flag: RowFlag| {
                let (page, row) = rows.screen_row(y);
                page.row(row).has(flag)
            };
            let prompt = (self.zone != Semantic::Output)
                .then(|| (0..=y).rev().find(|&y| has(y, RowFlag::PromptStart)))
                .flatten();
            let line = (0..=y)
                .rev()
                .find(|&y| !has(y, RowFlag::Continuation))
                .unwrap_or(0);
            let end = (y..self.rows)
                .find(|&y| !has(y, RowFlag::SoftWrapped))
                .unwrap_or(self.rows - 1);
            (prompt.unwrap_or(line), end)
        };
        for y in end + 1..self.rows {
            let width = self.cols;
            let (page, row) = self.screen_mut().rows.screen_row_mut(y);
            page.erase(row, 0..width, Cell::EMPTY, false);
            // An empty row that neither goes on from another, nor to the
            // next, nor starts a prompt, as an erase leaves it.
            let flags = page.row_mut(row);
            for flag in [
                RowFlag::SoftWrapped,
                RowFlag::Continuation,
                RowFlag::PromptStart,
            ] {
                flags.set(flag, false);
            }
        }
        if start > 0 {
            // The row above no longer goes on into the prompt, as output
            // without a final newline does.
            self.end_soft_wrap(start - 1);
            let bottom = self.rows - 1;
            self.screen_mut().rows.shift_rows_up(0, bottom, start);
        }
        self.clear_scrollback();
        self.screen_mut().cursor.y -= start;
        // The cluster in progress was the cursor's old row's.
        self.moved();
    }

    /// Sets the bytes that the primary screen's scrollback may take, as
    /// [`Scrollback::limit`] says, and drops the oldest rows that pass
    /// them. Past the width that the embedder gave, a program's, the
    /// scrollback keeps the rows that it has, within twice the limit.
    pub fn set_scrollback_limit(&mut self, limit: usize) {
        let (before, now) = (self.scrollback_limit, self.primary.rows.scrollback().limit);
        self.scrollback_limit = limit;
        let mut measured = self.primary_limit();
        // A limit that does not shrink keeps what a program's width grew,
        // which pages that compacting made smaller would measure less.
        if self.cols > self.given_cols && limit >= before {
            measured = measured.max(now).min(limit.saturating_mul(2));
        }
        self.primary.rows.set_scrollback_limit(measured);
    }

    /// The primary screen's limit at its width: the configured one, or past
    /// the embedder's width the bytes that its rows take there, within the
    /// configured limit and twice it.
    fn primary_limit(&self) -> usize {
        let configured = self.scrollback_limit;
        if self.cols > self.given_cols {
            self.primary
                .rows
                .memory()
                .clamp(configured, configured.saturating_mul(2))
        } else {
            configured
        }
    }

    /// Resizes the screens as [`Terminal::resize`] does, without taking the
    /// size for the embedder's.
    fn resize_screens(&mut self, cols: u16, rows: u16) {
        let (cols, rows) = (cols.max(2), rows.max(1));
        if (cols, rows) == (self.cols, self.rows) {
            return;
        }
        self.moved();
        resize_screen(&mut self.primary, cols, rows, true);
        resize_screen(&mut self.alternate, cols, rows, false);
        let stops = &self.tab_stops;
        self.tab_stops = (0..cols)
            .map(|x| {
                stops
                    .get(usize::from(x))
                    .copied()
                    .unwrap_or(x.is_multiple_of(8))
            })
            .collect();
        (self.cols, self.rows) = (cols, rows);
        self.margins = Margins::screen(cols, rows);
    }

    /// Takes the size that a program asked for, `cols` by `rows`, at once,
    /// so that the program's next query reads it, and tells the embedder,
    /// which gives it to the window and the PTY. A side that changes is
    /// [`ASKED_SIZE`] at most. Past the width that the embedder gave, the
    /// primary screen's rows rewrap with room for twice the scrollback's
    /// limit, which becomes the bytes that they take there, so that the
    /// scrollback keeps its rows, as it does when the program goes back.
    fn take_size(&mut self, cols: u16, rows: u16) {
        let asked = |asked: u16, now: u16| {
            if asked == now {
                asked
            } else {
                asked.min(ASKED_SIZE)
            }
        };
        let (cols, rows) = (asked(cols, self.cols), asked(rows, self.rows));
        let before = (self.cols, self.rows);
        // The rows rewrap within the larger limit, and the limit shrinks
        // to the new width's once they have.
        let configured = self.scrollback_limit;
        let room = if cols.max(2) > self.given_cols {
            configured.saturating_mul(2)
        } else {
            configured
        };
        let now = self.primary.rows.scrollback().limit;
        self.primary.rows.set_scrollback_limit(now.max(room));
        self.resize_screens(cols, rows);
        let limit = self.primary_limit();
        self.primary.rows.set_scrollback_limit(limit);
        if (self.cols, self.rows) != before {
            self.events.push(Event::SizeChanged);
        }
    }

    /// Keeps small the primary screen's pages that are three screens or
    /// more above its last row, as [`PageList::compact`] does, and returns
    /// the bytes that this gave back. For an idle terminal: compacting
    /// allocates, and output brings the pages back as it needs them.
    pub fn compact_scrollback(&mut self) -> usize {
        let keep = 3 * usize::from(self.rows);
        self.primary.rows.compact(keep)
    }

    /// Whether autowrap (DECAWM) is on.
    pub fn autowrap(&self) -> bool {
        self.autowrap
    }

    /// Turns autowrap (DECAWM) on or off.
    pub fn set_autowrap(&mut self, on: bool) {
        self.autowrap = on;
    }

    /// The last character given to print, before the character sets,
    /// which REP repeats, if any.
    pub fn last_printed(&self) -> Option<char> {
        self.last_printed
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

    /// The top and bottom margins (DECSTBM), the rows from 0 that the
    /// screen scrolls between, both included.
    pub fn scroll_region(&self) -> (u16, u16) {
        (self.margins.top, self.margins.bottom)
    }

    /// Whether column `x`, from 0, has a tab stop.
    pub fn tab_stop(&self, x: u16) -> bool {
        self.tab_stops.get(usize::from(x)).copied().unwrap_or(false)
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

    /// The status of the last command that ended, as OSC 133's `D ;
    /// status` tells it: the last command's of a shell that marks its
    /// commands, or a shell's own, as the wrapper between `login` and the
    /// login shell tells it on macOS.
    pub fn command_status(&self) -> Option<i32> {
        self.command_status
    }

    /// Whether the program asked the cursor to blink (mode 12).
    pub fn cursor_blink(&self) -> bool {
        self.cursor_blink
    }

    /// Whether a program asked for smooth scrolling (DECSCLM, mode 4), for
    /// whoever draws the screen to honor.
    pub fn smooth_scroll(&self) -> bool {
        self.smooth_scroll
    }

    /// Whether reverse wraparound (mode 45) is on.
    pub fn reverse_wrap(&self) -> bool {
        self.reverse_wrap
    }

    /// Whether a synchronized update (mode 2026) is under way.
    pub fn synchronized_output(&self) -> bool {
        self.synchronized_output
    }

    /// How many synchronized updates programs have started, one for each
    /// `CSI ? 2026 h` while none is under way, so that whoever draws the
    /// screen can tell a new update from the one it waits for, and give
    /// each its own deadline, which a start during the update, going on
    /// with it, does not put off.
    pub fn synchronized_updates(&self) -> u64 {
        self.synchronized_updates
    }

    /// Ends the synchronized update under way, as whoever draws the screen
    /// does when the update takes too long.
    pub fn end_synchronized_output(&mut self) {
        self.synchronized_output = false;
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

    /// The URI of the link that the text printed next is part of.
    pub fn open_link(&self) -> Option<&[u8]> {
        self.link.is_open().then_some(&self.link.uri[..])
    }

    /// The zone of the text printed next, as semantic prompts mark it.
    pub fn semantic_zone(&self) -> Semantic {
        self.zone
    }

    /// The colors to draw with: the theme's, as programs changed them.
    pub fn colors(&self) -> &Colors {
        &self.colors
    }

    /// Sets the theme's colors, which become the colors to draw with, and
    /// those that the resets bring back: of other colors than the theme's
    /// before, a program that set mode 2031 is told their scheme.
    pub fn set_default_colors(&mut self, colors: Colors) {
        let changed = colors != self.default_colors;
        self.colors.clone_from(&colors);
        self.default_colors = colors;
        if changed && self.color_scheme_reports {
            self.report_color_scheme();
        }
    }

    /// The scheme of the theme's colors, dark or light, as programs learn
    /// of it, whatever colors they set.
    pub fn color_scheme(&self) -> ColorScheme {
        self.default_colors.scheme()
    }

    /// Takes what the embedder has to act on, oldest first.
    pub fn take_events(&mut self) -> Vec<Event> {
        self.events.take()
    }

    /// Whether an event waits for the embedder to take it.
    pub fn has_events(&self) -> bool {
        !self.events.is_empty()
    }

    /// Sets the size of a cell in pixels, which CSI 14 t and 16 t report;
    /// until then, they get no reply.
    pub fn set_cell_size(&mut self, width: u16, height: u16) {
        self.cell_size = Some((width, height));
    }

    /// The size of a cell in pixels, width and height, once it is set.
    pub fn cell_size(&self) -> Option<(u16, u16)> {
        self.cell_size
    }

    /// Takes what the terminal has to send back to the program, such as
    /// replies to queries, oldest first, for the embedder to write to the
    /// PTY. The replies to queries that wait take a MiB at most: past it, a
    /// reply is dropped whole until the next take, as a program can ask for
    /// replies far longer than its queries. An answer to the clipboard,
    /// which the user allowed, goes whole.
    pub fn take_replies(&mut self) -> Vec<u8> {
        self.reply_bytes = 0;
        std::mem::take(&mut self.replies)
    }

    /// How many times BEL has rung.
    pub fn bells(&self) -> u64 {
        self.bells
    }

    /// The text of row `y` of the active screen: a character per cell, a
    /// space for an empty one, with the trailing spaces taken off. A spacer
    /// head reads as a space, so that the text keeps its columns where a
    /// wrap within left and right margins leaves one inside the row; one at
    /// the row's end goes with the trailing spaces.
    pub fn row_text(&self, y: u16) -> String {
        let (page, row) = self.screen().rows.screen_row(y);
        let mut text = String::new();
        for (x, cell) in page.cells(row).iter().enumerate() {
            // A row has `u16::MAX` cells at most.
            let x = x as u16;
            match (cell.width(), cell.content()) {
                (Width::SpacerHead, _) => text.push(' '),
                (Width::SpacerTail, _) => {}
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

    fn print_chars(&mut self, chars: &[char]) {
        self.print_char_run(chars);
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

/// Resizes `screen` to `cols` columns and `rows` rows, rewrapping its lines
/// with `reflow` and cutting them without, and moves its cursors with
/// their cells.
fn resize_screen(screen: &mut Screen, cols: u16, rows: u16, reflow: bool) {
    let cursor = screen.cursor;
    // The saved cursor's cell is pinned, so that it follows its text; when
    // columns are cut, the cell in the last column that stays.
    let saved = screen.saved.map(|saved| {
        let (page, row) = screen.rows.screen_row_id(saved.cursor.y);
        let col = if reflow {
            saved.cursor.x
        } else {
            saved.cursor.x.min(cols - 1)
        };
        screen.rows.pin(Position { page, row, col })
    });
    let (x, y) = if reflow {
        screen.rows.reflow(cols, rows, (cursor.x, cursor.y))
    } else {
        screen
            .rows
            .resize(cols, rows, (cursor.x, cursor.y), Grow::AtTheBottom)
    };
    screen.cursor = after_resize(cursor.pending_wrap, x, y, cols);
    if let (Some(saved), Some(Some(pin))) = (&mut screen.saved, saved) {
        let old = saved.cursor;
        let (x, y) = match screen.rows.pin_on_screen(pin) {
            Some(point) => point,
            // Its text went to the scrollback.
            None if screen.rows.pin_position(pin).is_some() => (old.x.min(cols - 1), 0),
            // Its row was dropped.
            None => (old.x.min(cols - 1), old.y.min(rows - 1)),
        };
        saved.cursor = after_resize(old.pending_wrap, x, y, cols);
        screen.rows.unpin(pin);
    }
}

/// A cursor on the cell at column `x` of row `y` of a screen `cols` wide
/// after a resize, whose wrap was pending or not before it: the wrap stays
/// pending at the last column, and goes on in the cell after the text
/// otherwise.
fn after_resize(pending_wrap: bool, x: u16, y: u16, cols: u16) -> Cursor {
    if pending_wrap && x + 1 < cols {
        Cursor {
            x: x + 1,
            y,
            pending_wrap: false,
        }
    } else {
        Cursor { x, y, pending_wrap }
    }
}

#[cfg(test)]
mod tests;
