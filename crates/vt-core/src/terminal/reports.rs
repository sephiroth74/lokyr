//! Reports: the replies to queries about the terminal, its cursor and its
//! settings, and the title stack.
//!
//! Replies carry only what the terminal makes of its own state, never text
//! that a program chose, so that a program cannot type into another
//! through them; the one exception, the title, is reported only when the
//! configuration allows it.

use std::fmt::Write as _;

use super::osc::hex_byte;
use super::terminfo::{self, Capability};
use super::{CursorShape, Event, Terminal};
use crate::page::{Color, Style, Underline};
use crate::parser::{Params, StringEnd};

/// The bytes of the replies to programs' queries that wait to be taken, at
/// most: past them, a reply is dropped whole, as some queries make replies
/// far longer than they are, and a program that sends them without
/// reading would grow memory without end.
pub(super) const REPLY_BYTES: usize = 1 << 20;

/// The longest DCS query that the terminal reads.
const QUERY_BYTES: usize = 1024;

/// How many titles CSI 22 t keeps, as in xterm.
const TITLE_STACK: usize = 10;

/// The DCS queries that the terminal answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Query {
    /// DECRQSS, `DCS $ q`: a setting, as the sequence that sets it.
    Setting,
    /// XTGETTCAP, `DCS + q`: terminfo capabilities, by name.
    Capabilities,
}

impl Terminal {
    /// Queues `bytes`, a reply to a program's query, unless the replies
    /// that wait would take more than [`REPLY_BYTES`].
    pub(super) fn reply(&mut self, bytes: &[u8]) {
        if self.reply_bytes + bytes.len() <= REPLY_BYTES {
            self.reply_bytes += bytes.len();
            self.replies.extend_from_slice(bytes);
        }
    }

    /// Queues `bytes`, the embedder's answer to a request that the user
    /// allowed, whatever its length.
    pub(super) fn answer(&mut self, bytes: &[u8]) {
        self.replies.extend_from_slice(bytes);
    }

    /// DSR: 5 asks whether the terminal is well, 6 where the cursor is
    /// (CPR), from the top left of the margins in origin mode. DECRC can
    /// bring the cursor back above or left of margins set since, which
    /// reports as their top left.
    pub(super) fn device_status(&mut self, which: u16) {
        match which {
            5 => self.reply(b"\x1b[0n"),
            6 => {
                let cursor = self.cursor();
                let (row, column) = if self.origin {
                    (
                        cursor.y.saturating_sub(self.margins.top),
                        cursor.x.saturating_sub(self.margins.left),
                    )
                } else {
                    (cursor.y, cursor.x)
                };
                let reply = format!("\x1b[{};{}R", row + 1, column + 1);
                self.reply(reply.as_bytes());
            }
            _ => {}
        }
    }

    /// XTVERSION: the terminal's name and version.
    pub(super) fn report_version(&mut self) {
        let reply = format!("\x1bP>|bolt {}\x1b\\", env!("CARGO_PKG_VERSION"));
        self.reply(reply.as_bytes());
    }

    /// XTWINOPS: the sizes (14, 16 and 18), the title (21) if the
    /// configuration allows it, and the title stack (22 and 23). The others,
    /// which move and resize the window, do nothing.
    pub(super) fn window_ops(&mut self, params: &Params) {
        let (cols, rows) = (u32::from(self.cols), u32::from(self.rows));
        match params.value(0) {
            14 => {
                if let Some((width, height)) = self.cell_size {
                    let (width, height) = (u32::from(width) * cols, u32::from(height) * rows);
                    self.reply(format!("\x1b[4;{height};{width}t").as_bytes());
                }
            }
            16 => {
                if let Some((width, height)) = self.cell_size {
                    self.reply(format!("\x1b[6;{height};{width}t").as_bytes());
                }
            }
            18 => self.reply(format!("\x1b[8;{rows};{cols}t").as_bytes()),
            20 if self.title_reports => {
                let reply = format!("\x1b]L{}\x1b\\", printable(&self.icon_title));
                self.reply(reply.as_bytes());
            }
            21 if self.title_reports => {
                let reply = format!("\x1b]l{}\x1b\\", printable(&self.title));
                self.reply(reply.as_bytes());
            }
            22 => self.push_title(params.value(1)),
            23 => self.pop_title(params.value(1)),
            _ => {}
        }
    }

    /// CSI 22 t: pushes both titles with 0, the icon's with 1 and the
    /// window's with 2, as xterm does, dropping the oldest entry when the
    /// stack is full.
    fn push_title(&mut self, which: u16) {
        if self.title_stack.len() == TITLE_STACK {
            self.title_stack.remove(0);
        }
        let (title, icon) = titles(which);
        self.title_stack.push((
            title.then(|| self.title.clone()),
            icon.then(|| self.icon_title.clone()),
        ));
    }

    /// CSI 23 t: pops an entry, and brings back both titles with 0, the
    /// icon's with 1 and the window's with 2, of those that it holds.
    fn pop_title(&mut self, which: u16) {
        let Some((title, icon_title)) = self.title_stack.pop() else {
            return;
        };
        let (window, icon) = titles(which);
        let mut changed = false;
        if let Some(title) = title.filter(|_| window) {
            self.title = title;
            changed = true;
        }
        if let Some(icon_title) = icon_title.filter(|_| icon) {
            self.icon_title = icon_title;
            changed = true;
        }
        if changed {
            self.events.push(Event::TitleChanged);
        }
    }

    /// Starts reading a DCS string, if it is a query the terminal answers.
    pub(super) fn start_query(&mut self, intermediates: &[u8], final_byte: u8) {
        self.query = match (intermediates, final_byte) {
            ([b'$'], b'q') => Some(Query::Setting),
            ([b'+'], b'q') => Some(Query::Capabilities),
            _ => None,
        };
        self.query_bytes.clear();
    }

    /// Reads the next chunk of a DCS query; one too long is dropped.
    pub(super) fn read_query(&mut self, data: &[u8]) {
        if self.query.is_none() {
            return;
        }
        if self.query_bytes.len() + data.len() > QUERY_BYTES {
            self.query = None;
        } else {
            self.query_bytes.extend_from_slice(data);
        }
    }

    /// Answers the DCS query, if it ended with ST.
    pub(super) fn end_query(&mut self, end: StringEnd) {
        let Some(query) = self.query.take() else {
            return;
        };
        if end != StringEnd::St {
            return;
        }
        let bytes = std::mem::take(&mut self.query_bytes);
        match query {
            Query::Setting => self.report_setting(&bytes),
            Query::Capabilities => self.report_capabilities(&bytes),
        }
        self.query_bytes = bytes;
    }

    /// DECRQSS: `DCS 1 $ r` and the sequence that sets the setting named by
    /// `name`, as xterm replies, or `DCS 0 $ r` for a setting the terminal
    /// does not report, without the name.
    fn report_setting(&mut self, name: &[u8]) {
        let margins = self.margins;
        let setting = match name {
            b"m" => Some(format!("{}m", sgr(&self.pen))),
            b"r" => Some(format!("{};{}r", margins.top + 1, margins.bottom + 1)),
            b"s" => Some(format!("{};{}s", margins.left + 1, margins.right + 1)),
            b" q" => Some(format!("{} q", self.cursor_style_value())),
            b"\"q" => Some(format!("{}\"q", u8::from(self.protected))),
            _ => None,
        };
        let reply = match setting {
            Some(setting) => format!("\x1bP1$r{setting}\x1b\\"),
            None => "\x1bP0$r\x1b\\".to_owned(),
        };
        self.reply(reply.as_bytes());
    }

    /// The value of DECSCUSR that gives the cursor its style.
    fn cursor_style_value(&self) -> u8 {
        let Some(style) = self.cursor_style else {
            return 0;
        };
        let base = match style.shape {
            CursorShape::Block => 1,
            CursorShape::Underline => 3,
            CursorShape::Bar => 5,
        };
        if style.blinking { base } else { base + 1 }
    }

    /// XTGETTCAP: for each name in `names`, hexadecimal and separated by
    /// `;`, `DCS 1 + r` and the name with its value, or `DCS 0 + r` and the
    /// name for one the terminal does not know. A name that is not
    /// hexadecimal ends the reply.
    fn report_capabilities(&mut self, names: &[u8]) {
        for hex in names.split(|&byte| byte == b';') {
            let Some(name) = unhex(hex) else {
                return;
            };
            let reply = match self.capability(&name) {
                Some(Capability::Flag) => format!("\x1bP1+r{}\x1b\\", to_hex(&name)),
                Some(Capability::Value(value)) => {
                    format!("\x1bP1+r{}={}\x1b\\", to_hex(&name), to_hex(&value))
                }
                None => format!("\x1bP0+r{}\x1b\\", to_hex(&name)),
            };
            self.reply(reply.as_bytes());
        }
    }

    /// The capability `name`: the terminal's terminfo entry's, by its
    /// terminfo name, and those that xterm answers too: `TN` and `name`,
    /// the `TERM` that programs see, `Co`, the colors' termcap name, and
    /// `RGB`, the bits of each channel of a direct color.
    fn capability(&self, name: &[u8]) -> Option<Capability> {
        match name {
            b"TN" | b"name" => Some(Capability::Value(self.term.clone().into_bytes())),
            b"Co" => terminfo::capability(b"colors").cloned(),
            b"RGB" => Some(Capability::Value(b"8/8/8".to_vec())),
            _ => terminfo::capability(name).cloned(),
        }
    }
}

/// The SGR parameters that give a pen its style, from a reset.
fn sgr(style: &Style) -> String {
    let mut sgr = String::from("0");
    let mut add = |parameter: &str| {
        sgr.push(';');
        sgr.push_str(parameter);
    };
    for (on, parameter) in [
        (style.bold, "1"),
        (style.faint, "2"),
        (style.italic, "3"),
        (style.blink, "5"),
        (style.inverse, "7"),
        (style.invisible, "8"),
        (style.strikethrough, "9"),
        (style.overline, "53"),
    ] {
        if on {
            add(parameter);
        }
    }
    match style.underline {
        Underline::None => {}
        Underline::Single => add("4"),
        Underline::Double => add("4:2"),
        Underline::Curly => add("4:3"),
        Underline::Dotted => add("4:4"),
        Underline::Dashed => add("4:5"),
    }
    if let Some(fg) = color(style.fg, 30, 90, 38) {
        add(&fg);
    }
    if let Some(bg) = color(style.bg, 40, 100, 48) {
        add(&bg);
    }
    match style.underline_color {
        Color::Default => {}
        Color::Palette(n) => add(&format!("58:5:{n}")),
        Color::Rgb(rgb) => add(&format!("58:2::{}:{}:{}", rgb.r, rgb.g, rgb.b)),
    }
    sgr
}

/// The SGR parameter of `color`: from `base` for the first 8 colors of the
/// palette, from `bright` for the next 8, and through `extended` for the
/// others; none for the default.
fn color(color: Color, base: u16, bright: u16, extended: u16) -> Option<String> {
    Some(match color {
        Color::Default => return None,
        Color::Palette(n @ 0..8) => (base + u16::from(n)).to_string(),
        Color::Palette(n @ 8..16) => (bright + u16::from(n) - 8).to_string(),
        Color::Palette(n) => format!("{extended};5;{n}"),
        Color::Rgb(rgb) => format!("{extended};2;{};{};{}", rgb.r, rgb.g, rgb.b),
    })
}

/// The text of `title` without its control characters, for a reply.
fn printable(title: &str) -> String {
    title.chars().filter(|c| !c.is_control()).collect()
}

/// Which titles a title stack operation with `which` concerns, the
/// window's and the icon's: 0 for both, 1 for the icon's, 2 for the
/// window's, and none for the other values.
fn titles(which: u16) -> (bool, bool) {
    match which {
        0 => (true, true),
        1 => (false, true),
        2 => (true, false),
        _ => (false, false),
    }
}

/// The bytes that `hex`, pairs of hexadecimal digits, stand for.
fn unhex(hex: &[u8]) -> Option<Vec<u8>> {
    if hex.is_empty() || !hex.len().is_multiple_of(2) {
        return None;
    }
    hex.chunks(2).map(hex_byte).collect()
}

/// `bytes` in uppercase hexadecimal.
fn to_hex(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(2 * bytes.len());
    for byte in bytes {
        write!(hex, "{byte:02X}").expect("writing to a String");
    }
    hex
}
