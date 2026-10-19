//! An input fed to both cores a byte at a time, with the screens compared
//! after each, as the differential fuzz target feeds it.

use std::fmt;

use bolt_vt_core::page::Width;
use bolt_vt_core::parser::{Handler, Params, StringEnd};
use bolt_vt_core::terminal::{Charset, Shift, Terminal};

use crate::cause::{self, Cause};
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
    /// Whether each column has a tab stop.
    pub tab_stops: Vec<bool>,
    /// Whether DEL came since the last character printed, which vte prints
    /// as one.
    pub del: bool,
    /// Whether RIS came since the last character printed, which vte's
    /// parser keeps for REP.
    pub reset: bool,
}

impl Before {
    fn of(terminal: &Terminal, (del, reset): (bool, bool)) -> Self {
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
            tab_stops: (0..terminal.cols()).map(|x| terminal.tab_stop(x)).collect(),
            del,
            reset,
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
    // Whether an escape sequence started, which C0 controls and DEL do not
    // end, but CAN and SUB.
    let mut escape = false;
    for &byte in data {
        let after_escape = escape;
        match byte {
            0x1B => escape = true,
            0x18 | 0x1A | 0x20..=0x7E | 0x80.. => escape = false,
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
    let mut pair = Pair::new(cols, rows);
    // Whether DEL and RIS came since the last character printed.
    let (mut del, mut reset) = (false, false);
    for (at, &byte) in (1..).zip(bytes) {
        let before = Before::of(pair.terminal(), (del, reset));
        let step = pair.step(byte);
        match &step {
            Step::Print(_) => (del, reset) = (false, false),
            Step::Esc {
                intermediates,
                final_byte: b'c',
            } if intermediates.is_empty() => reset = true,
            _ if byte == 0x7F => del = true,
            _ => {}
        }
        let shift = pair.terminal().charsets().shift();
        if let Some(cause) = cause::ending(&step, &before, shift) {
            return Ok(Some(cause));
        }
        if pair.same() {
            continue;
        }
        if let Some(cause) = cause::cause(&step, byte, &before) {
            return Ok(Some(cause));
        }
        let difference = pair.difference().expect("the screens differ");
        return Err(Box::new(Divergence {
            at,
            step,
            before,
            difference,
        }));
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
