//! An input fed to both cores a byte at a time, with the screens compared
//! after each, as the differential fuzz target feeds it.

use std::fmt;

use bolt_vt_core::page::Width;
use bolt_vt_core::parser::{Handler, Params, StringEnd};
use bolt_vt_core::terminal::{Charset, Shift, Terminal};

#[cfg(test)]
use crate::Screen;
use crate::cause::{self, Cause, Seen};
use crate::{Difference, Pair};

/// What a byte made the VT core do, as its parser dispatched it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Nothing that ends: a byte within a sequence, a string or a
    /// character.
    Nothing,
    /// A character printed.
    Print(char),
    /// A C0 control executed.
    Execute(u8),
    /// An escape sequence.
    Esc {
        /// Its intermediate bytes.
        intermediates: Vec<u8>,
        /// Its final byte.
        final_byte: u8,
    },
    /// A control sequence.
    Csi {
        /// The first value of each parameter, 0 for an empty one.
        params: Vec<u16>,
        /// How many values it has, its sub-parameters' included.
        values: usize,
        /// Its private marker and intermediate bytes.
        intermediates: Vec<u8>,
        /// Its final byte.
        final_byte: u8,
    },
    /// The end of a control string.
    String,
}

/// The VT core's state before a step, which a cause can depend on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Before {
    /// The cursor's column, row and pending wrap.
    pub cursor: (usize, usize, bool),
    /// The screen's columns.
    pub cols: usize,
    /// The screen's rows.
    pub rows: usize,
    /// The scroll region's top and bottom rows.
    pub region: (usize, usize),
    /// Whether origin mode is on.
    pub origin: bool,
    /// Whether insert mode is on.
    pub insert: bool,
    /// Whether autowrap is on.
    pub autowrap: bool,
    /// The sets that G0 to G3 hold.
    pub charsets: [Charset; 4],
    /// The set that is shifted in, as SO and SI choose.
    pub shift: Shift,
    /// The width of each cell of the cursor's row.
    pub widths: Vec<Width>,
    /// The width of each cell of the row below the cursor's, if any.
    pub below: Vec<Width>,
    /// Whether each row below the cursor's, the nearest first, holds a wide
    /// character, which text inserted can push past the row's end: in insert
    /// mode alone, as reading every row at each byte would cost.
    pub wide_below: Vec<bool>,
    /// Whether each column has a tab stop.
    pub tab_stops: Vec<bool>,
    /// The character that REP repeats, and its width, if any.
    pub last_printed: Option<(char, u8)>,
    /// Whether DEL came since the last character printed, which vte prints
    /// as one.
    pub del: bool,
    /// Whether RIS came since the last character printed, which vte's
    /// parser keeps for REP.
    pub reset: bool,
    /// Whether `alacritty_terminal` still has its tab stop at the first
    /// column, which the VT core has not, and which TBC clears.
    pub first_stop: bool,
}

/// What the comparison keeps track of besides the VT core's state: what
/// came since the last character printed, and `alacritty_terminal`'s tab
/// stop at the first column.
#[derive(Clone, Copy, Debug)]
struct Track {
    del: bool,
    reset: bool,
    first_stop: bool,
}

impl Default for Track {
    fn default() -> Self {
        Self {
            del: false,
            reset: false,
            first_stop: true,
        }
    }
}

impl Track {
    /// Notes what `byte` made the VT core do, `step`, with the cursor
    /// `before` it, and whether the parser was in the ground state before
    /// it, where vte prints DEL as a character, and outside of which DEL is
    /// none for vte either.
    fn note(&mut self, byte: u8, ground: bool, step: &Step, before: &Before) {
        if byte == 0x7F && ground {
            self.del = true;
        }
        match step {
            Step::Print(_) => (self.del, self.reset) = (false, false),
            Step::Esc {
                intermediates,
                final_byte: b'c',
            } if intermediates.is_empty() => (self.reset, self.first_stop) = (true, true),
            // HTS at the first column.
            Step::Esc {
                intermediates,
                final_byte: b'H',
            } if intermediates.is_empty() && before.cursor.0 == 0 => self.first_stop = true,
            // TBC of every stop, or of the one at the first column.
            Step::Csi {
                params,
                intermediates,
                final_byte: b'g',
                ..
            } if intermediates.is_empty() => match params.first().copied().unwrap_or(0) {
                3 => self.first_stop = false,
                0 if before.cursor.0 == 0 => self.first_stop = false,
                _ => {}
            },
            _ => {}
        }
    }
}

impl Before {
    fn of(terminal: &Terminal, track: Track) -> Self {
        let cursor = terminal.cursor();
        Self {
            cursor: (
                usize::from(cursor.x),
                usize::from(cursor.y),
                cursor.pending_wrap,
            ),
            cols: usize::from(terminal.cols()),
            rows: usize::from(terminal.rows()),
            region: {
                let (top, bottom) = terminal.scroll_region();
                (usize::from(top), usize::from(bottom))
            },
            origin: terminal.origin_mode(),
            insert: terminal.insert_mode(),
            autowrap: terminal.autowrap(),
            charsets: cause::sets(terminal.charsets()),
            shift: terminal.charsets().shift(),
            widths: widths(terminal, cursor.y),
            below: if cursor.y + 1 < terminal.rows() {
                widths(terminal, cursor.y + 1)
            } else {
                Vec::new()
            },
            wide_below: if terminal.insert_mode() {
                (cursor.y + 1..terminal.rows())
                    .map(|y| widths(terminal, y).contains(&Width::SpacerTail))
                    .collect()
            } else {
                Vec::new()
            },
            tab_stops: (0..terminal.cols()).map(|x| terminal.tab_stop(x)).collect(),
            last_printed: terminal
                .last_printed()
                .map(|c| (c, terminal.widths().char_width(c))),
            del: track.del,
            reset: track.reset,
            first_stop: track.first_stop,
        }
    }
}

/// The width of each cell of row `y` of the VT core's screen.
fn widths(terminal: &Terminal, y: u16) -> Vec<Width> {
    let (page, row) = terminal.screen_rows().screen_row(y);
    page.cells(row).iter().map(|cell| cell.width()).collect()
}

/// The first point where the screens part for no known cause.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Divergence {
    /// How many bytes were fed, the one that the screens parted at
    /// included.
    pub at: usize,
    /// What that byte made the VT core do.
    pub step: Step,
    /// The VT core's state before it.
    pub before: Before,
    /// Where the screens differ.
    pub difference: Difference,
}

impl fmt::Display for Divergence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the screens part at byte {}, after {:?} from {:?}: {}; {}",
            self.at, self.step, self.before, self.difference.summary, self.difference.context
        )
    }
}

/// The input that the differential fuzz target feeds for `data`: a byte
/// from 0x80 stands for a character that both cores print alike, a wide one
/// or a narrow one, so that the input is control sequences and text, as
/// ill-formed UTF-8, the widths and the clusters, where the cores differ for
/// their reasons, are compared over Kuhn's file and the parser's corpus; and
/// OSC strings become PM strings, which both cores pass over, as they change
/// no cell and vte's parser of colors overflows on some.
pub fn fuzz_input(data: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(data.len() * 3);
    // Whether an escape sequence started, which C0 controls, DEL and the
    // bytes from 0x80, which both parsers pass over there, do not end, but
    // CAN and SUB.
    let mut escape = false;
    for &byte in data {
        let after_escape = escape;
        match byte {
            0x1B => escape = true,
            0x18 | 0x1A | 0x20..=0x7E => escape = false,
            _ => {}
        }
        match byte {
            b']' if after_escape => bytes.push(b'^'),
            0x00..=0x7F => bytes.push(byte),
            _ if byte % 2 == 0 => bytes.extend_from_slice("\u{4E2D}".as_bytes()),
            _ => bytes.extend_from_slice("\u{E9}".as_bytes()),
        }
    }
    bytes
}

/// Feeds `bytes` to a pair of `cols` by `rows`, a byte at a time, and
/// compares the screens after each. `None` if they never part, the cause if
/// they first part for a known one, or meet a step whose effect can show
/// later, after which they are compared no more, as they then say nothing;
/// the divergence, if they first part for none.
pub fn explore(cols: u16, rows: u16, bytes: &[u8]) -> Result<Option<Cause>, Box<Divergence>> {
    explore_after(cols, rows, &[], bytes)
}

/// Feeds `prefix` to a pair of `cols` by `rows`, without comparing the
/// screens, then `bytes` as [`explore`] does, the bytes counted from the
/// first of `bytes`: to find the cause of a stretch of a recording, from
/// the chunk that it starts at.
pub fn explore_after(
    cols: u16,
    rows: u16,
    prefix: &[u8],
    bytes: &[u8],
) -> Result<Option<Cause>, Box<Divergence>> {
    let mut pair = Pair::new(cols, rows);
    let mut track = Track::default();
    let mut before = Before::of(pair.terminal(), track);
    for &byte in prefix {
        let ground = pair.parser.in_ground();
        let step = pair.step(byte);
        track.note(byte, ground, &step, &before);
        before = Before::of(pair.terminal(), track);
    }
    for (at, &byte) in (1..).zip(bytes) {
        let ground = pair.parser.in_ground();
        let step = pair.step(byte);
        track.note(byte, ground, &step, &before);
        let after = Before::of(pair.terminal(), track);
        let screens = (!pair.same()).then(|| (pair.bolt(), pair.alacritty()));
        let wide = match step {
            Step::Print(c) => pair.terminal().widths().char_width(c) == 2,
            _ => false,
        };
        let seen = Seen {
            step: &step,
            byte,
            before: &before,
            after: &after,
            screens: screens.as_ref().map(|(bolt, alacritty)| (bolt, alacritty)),
            wide,
        };
        if let Some(cause) = cause::ending(&seen).or_else(|| cause::cause(&seen)) {
            return Ok(Some(cause));
        }
        if screens.is_some() {
            let difference = pair.difference().expect("the screens differ");
            return Err(Box::new(Divergence {
                at,
                step,
                before,
                difference,
            }));
        }
        before = after;
    }
    Ok(None)
}

impl Pair {
    /// Feeds `byte` to both, and returns what it made the VT core do.
    fn step(&mut self, byte: u8) -> Step {
        let mut probe = Probe {
            terminal: &mut self.bolt,
            step: Step::Nothing,
        };
        self.parser.advance(&mut probe, &[byte]);
        self.processor.advance(&mut self.alacritty, &[byte]);
        probe.step
    }
}

/// The VT core's terminal, which notes what the parser dispatches to it.
struct Probe<'a> {
    terminal: &'a mut Terminal,
    step: Step,
}

impl Handler for Probe<'_> {
    fn print(&mut self, c: char) {
        self.step = Step::Print(c);
        self.terminal.print(c);
    }

    fn print_ascii(&mut self, text: &[u8]) {
        if let Some(&last) = text.last() {
            self.step = Step::Print(char::from(last));
        }
        self.terminal.print_ascii(text);
    }

    fn execute(&mut self, byte: u8) {
        self.step = Step::Execute(byte);
        self.terminal.execute(byte);
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], final_byte: u8) {
        self.step = Step::Esc {
            intermediates: intermediates.to_vec(),
            final_byte,
        };
        self.terminal.esc_dispatch(intermediates, final_byte);
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], final_byte: u8) {
        self.step = Step::Csi {
            params: params
                .iter()
                .map(|group| group.first().copied().unwrap_or(0))
                .collect(),
            values: params.iter().map(<[u16]>::len).sum(),
            intermediates: intermediates.to_vec(),
            final_byte,
        };
        self.terminal
            .csi_dispatch(params, intermediates, final_byte);
    }

    fn osc_start(&mut self) {
        self.terminal.osc_start();
    }

    fn osc_put(&mut self, data: &[u8]) {
        self.terminal.osc_put(data);
    }

    fn osc_end(&mut self, end: StringEnd) {
        self.step = Step::String;
        self.terminal.osc_end(end);
    }

    fn dcs_start(&mut self, params: &Params, intermediates: &[u8], final_byte: u8) {
        self.terminal.dcs_start(params, intermediates, final_byte);
    }

    fn dcs_put(&mut self, data: &[u8]) {
        self.terminal.dcs_put(data);
    }

    fn dcs_end(&mut self, end: StringEnd) {
        self.step = Step::String;
        self.terminal.dcs_end(end);
    }

    fn apc_start(&mut self) {
        self.terminal.apc_start();
    }

    fn apc_put(&mut self, data: &[u8]) {
        self.terminal.apc_put(data);
    }

    fn apc_end(&mut self, end: StringEnd) {
        self.step = Step::String;
        self.terminal.apc_end(end);
    }
}

/// What the comparison saw at the byte where the screens first part.
#[cfg(test)]
pub(crate) struct Parted {
    pub(crate) step: Step,
    pub(crate) byte: u8,
    pub(crate) before: Before,
    pub(crate) after: Before,
    pub(crate) bolt: Screen,
    pub(crate) alacritty: Screen,
    pub(crate) wide: bool,
}

/// Feeds `bytes` to a pair of `cols` by `rows` a byte at a time, as
/// [`explore`] does, and tells what it saw at the first byte where the
/// screens part, if they do.
#[cfg(test)]
pub(crate) fn parted(cols: u16, rows: u16, bytes: &[u8]) -> Option<Parted> {
    let mut pair = Pair::new(cols, rows);
    let mut track = Track::default();
    let mut before = Before::of(pair.terminal(), track);
    for &byte in bytes {
        let ground = pair.parser.in_ground();
        let step = pair.step(byte);
        track.note(byte, ground, &step, &before);
        let after = Before::of(pair.terminal(), track);
        if !pair.same() {
            let wide = match step {
                Step::Print(c) => pair.terminal().widths().char_width(c) == 2,
                _ => false,
            };
            return Some(Parted {
                step,
                byte,
                before,
                after,
                bolt: pair.bolt(),
                alacritty: pair.alacritty(),
                wide,
            });
        }
        before = after;
    }
    None
}
