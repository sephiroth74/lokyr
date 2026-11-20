//! A terminal's specimen of a theme: a terminal of the host's own, fed a
//! fixed sample of every combination that a theme colors, drawn as the
//! panes draw theirs: prompts and commands that ran, failed and run, an
//! `ls` of colors and a link, the 16 colors as text and as backgrounds,
//! bold, faint, italic, underlined, curly, struck and inverse text, a
//! selection, the find's matches, a hint's label over the link, an error
//! and a warning, and a block cursor over a character on the cursor guide's
//! row; the commands' blocks as Blocks shows them, the gutter's bars along
//! them, a divider at each block's top, and a failed block's tint.

use lokyr_render_core::{CellBatch, Shape};
use lokyr_text_core::CellMetrics;
use lokyr_vt_core::page::Underline;
use lokyr_vt_core::parser::Parser;
use lokyr_vt_core::terminal::{Colors, Label, Look, Options, Snapshot, Terminal};

use crate::paint;
use crate::pane::{self, CursorLook, Glyphs, Selection, TextLook};
use crate::theme::{LabelColors, MatchColors, SelectionColors, Tokens};

/// The specimen's columns.
pub const COLUMNS: u16 = 52;

/// The specimen's rows.
pub const ROWS: u16 = 12;

/// The room inside a specimen's edges, in points, where the gutter's bars
/// show at its left.
pub const PADDING: f32 = 12.0;

/// The natural size of a specimen, in points, of a terminal's cell of
/// `cell` points: its grid's whole within its padding.
pub fn natural((width, height): (f32, f32)) -> (f32, f32) {
    (
        f32::from(COLUMNS) * width + 2.0 * PADDING,
        f32::from(ROWS) * height + 2.0 * PADDING,
    )
}

/// The row of the selected text, and its columns.
const SELECTED: (u16, std::ops::Range<u16>) = (7, 0..13);

/// The link of the `ls`, its row and its columns, which a hint labels.
const LINK: (u16, std::ops::RangeInclusive<u16>) = (1, 32..=40);

/// The find's match, and its current one, on the selected text's row.
const MATCHED: std::ops::Range<u16> = 15..20;
const CURRENT: std::ops::Range<u16> = 22..29;

/// The sample: escape sequences of every combination, the prompts marked
/// as a shell marks them.
const SAMPLE: &str = concat!(
    "\x1b]133;A\x07\x1b[34m~/lokyr\x1b[0m \x1b[35m$\x1b[0m \x1b]133;B\x07ls --color\r\n\x1b]133;C\x07",
    "\x1b[1;34msrc\x1b[0m  \x1b[1;34mdocs\x1b[0m  \x1b[32mbuild.sh\x1b[0m  README.md  ",
    "\x1b]8;;https://lokyr.dev\x07\x1b[4;34mlokyr.dev\x1b[0m\x1b]8;;\x07\r\n",
    "\x1b]133;D;0\x07\x1b]133;A\x07\x1b[34m~/lokyr\x1b[0m \x1b[35m$\x1b[0m \x1b]133;B\x07colors\r\n\x1b]133;C\x07",
    "\x1b[30mblack \x1b[31mred \x1b[32mgreen \x1b[33myellow \x1b[34mblue \x1b[35mmagenta ",
    "\x1b[36mcyan \x1b[37mwhite\x1b[0m\r\n",
    "\x1b[90mblack \x1b[91mred \x1b[92mgreen \x1b[93myellow \x1b[94mblue \x1b[95mmagenta ",
    "\x1b[96mcyan \x1b[97mwhite\x1b[0m\r\n",
    "\x1b[40m 0 \x1b[41m 1 \x1b[42m 2 \x1b[43m 3 \x1b[44m 4 \x1b[45m 5 \x1b[46m 6 \x1b[47m 7 ",
    "\x1b[100m 8 \x1b[101m 9 \x1b[102m10 \x1b[103m11 \x1b[104m12 \x1b[105m13 \x1b[106m14 \x1b[107m15 \x1b[0m\r\n",
    "\x1b[1mbold\x1b[0m \x1b[2mfaint\x1b[0m \x1b[3mitalic\x1b[0m \x1b[4munderline\x1b[0m ",
    "\x1b[4:3mcurly\x1b[0m \x1b[9mstrike\x1b[0m \x1b[7minverse\x1b[0m\r\n",
    "selected text  match  current\r\n",
    "\x1b]133;D;0\x07\x1b]133;A\x07\x1b[34m~/lokyr\x1b[0m \x1b[35m$\x1b[0m \x1b]133;B\x07make\r\n\x1b]133;C\x07",
    "\x1b[1;31merror:\x1b[0m build failed  \x1b[1;33mwarning:\x1b[0m unused import\r\n",
    "\x1b]133;D;1\x07\x1b]133;A\x07\x1b[34m~/lokyr\x1b[0m \x1b[35m$\x1b[0m \x1b]133;B\x07cargo build\r\n\x1b]133;C\x07",
    "   \x1b[1;32mCompiling\x1b[0m lokyr v1.2.0\x1b[D",
);

/// What a specimen draws of a theme: the terminal's colors, the
/// selection's and the matches', the text's look, the cursor guide's color
/// among them, and the cursor's.
#[derive(Clone, Debug, PartialEq)]
pub struct Specimen {
    /// The terminal's colors.
    pub colors: Colors,
    /// The selection's colors.
    pub selection: SelectionColors,
    /// The find's matches' colors.
    pub matches: MatchColors,
    /// The hints' labels' colors.
    pub labels: LabelColors,
    /// The text's look.
    pub text: TextLook,
    /// The cursor's look.
    pub cursor: CursorLook,
}

/// The snapshot of the specimen's terminal, of `colors`.
fn snapshot(colors: &Colors) -> Snapshot {
    let mut terminal = Terminal::new(COLUMNS, ROWS, Options::default());
    terminal.set_default_colors(colors.clone());
    Parser::new().advance(&mut terminal, SAMPLE.as_bytes());
    // A hint over the link, as the hints label it.
    let (row, columns) = LINK;
    let hint = Look {
        underline: Underline::Single,
        label: Some(Label::new("a")),
        ..Look::default()
    };
    terminal.decorate(
        1,
        (*columns.start(), i64::from(row)),
        (*columns.end(), i64::from(row)),
        hint,
    );
    let mut snapshot = Snapshot::default();
    terminal.snapshot(&mut snapshot);
    snapshot
}

/// The specimen of `specimen`'s theme: its cells, its grid's top left
/// corner at `origin`, in pixels, of `cell`, its glyphs of `glyphs`; and the
/// gutter's bars of its commands along their blocks, in the `padding` at
/// its cells' left and right, a failed block's tint and the blocks'
/// dividers across it, of `tokens`, on a display of `scale`.
pub fn cells(
    specimen: &Specimen,
    cell: &CellMetrics,
    origin: (i32, i32),
    (padding, scale): (u32, f64),
    tokens: &Tokens,
    glyphs: &mut dyn Glyphs,
) -> (CellBatch, Vec<Shape>) {
    let snapshot = snapshot(&specimen.colors);
    let contains = |column: u16, row: u16| row == SELECTED.0 && SELECTED.1.contains(&column);
    let selection = Selection {
        colors: specimen.selection,
        contains: &contains,
    };
    let mut batch = pane::cells_with(
        &snapshot,
        cell,
        origin,
        Some(&selection),
        Some(specimen.cursor),
        specimen.text,
        glyphs,
    );
    // The find's matches over their cells, as the panes lay them.
    let row = usize::from(SELECTED.0) * usize::from(batch.columns);
    for (columns, color) in [
        (MATCHED, specimen.matches.matched),
        (CURRENT, specimen.matches.current),
    ] {
        for column in columns {
            if let Some(cell) = batch.backgrounds.get_mut(row + usize::from(column)) {
                *cell = paint::over(color, *cell);
            }
        }
    }
    pane::decorate(&mut batch, &snapshot, cell);
    pane::labels(&mut batch, &snapshot, cell, specimen.labels, glyphs);
    let looks = lokyr_plugin_sdk::host::BlockLooks {
        bars: true,
        dividers: true,
        failed_tint: true,
    };
    let across = (
        origin.0 - i32::try_from(padding).unwrap_or(0),
        u32::from(COLUMNS) * cell.width + 2 * padding,
    );
    let mut shapes = pane::block_shapes(
        &snapshot,
        origin.1,
        cell.height,
        across,
        scale,
        tokens,
        looks,
    );
    shapes.extend(pane::gutter_marks(
        &snapshot,
        origin,
        (padding, cell.height),
        scale,
        tokens,
        looks.bars,
    ));
    (batch, shapes)
}

#[cfg(test)]
mod tests;
