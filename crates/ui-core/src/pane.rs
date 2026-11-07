//! A pane's grid of cells, as a display list draws it, from a snapshot of
//! its terminal.

use lokyr_render_core::{
    AtlasRegion, CellBatch, CellCursor, CellGlyph, CellLine, Color, CursorShape, LineStyle, Texture,
};
use lokyr_text_core::CellMetrics;
use lokyr_vt_core::page::{self, Content, Rgb, Style, Width};
use lokyr_vt_core::terminal::{Colors, Snapshot};
use lokyr_vt_core::unicode::Widths;

use crate::paint;
use crate::text::TextConfig;
use crate::theme::{self, MatchColors, SelectionColors};

/// A glyph as the atlas holds it: its pixels, and where they go from the pen
/// on the baseline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtlasGlyph {
    /// Its pixels.
    pub region: AtlasRegion,
    /// How far its left edge is from the pen, rightwards.
    pub left: i32,
    /// How far its top edge is from the baseline, upwards.
    pub top: i32,
    /// Whether its coverage was drawn of the system's font smoothing, which
    /// the system's weight is of: not a thin glyph's, nor the terminal's own
    /// drawings'.
    pub smoothed: bool,
}

/// What a cell shows, for the caller to find its glyph.
#[derive(Clone, Copy, Debug)]
pub struct CellText<'a> {
    /// The cell's grapheme cluster.
    pub text: &'a str,
    /// Its style: bold and italic choose the face.
    pub style: &'a Style,
    /// Whether it takes two columns.
    pub wide: bool,
    /// Whether its background is darker than its text.
    pub dark: bool,
}

/// A run of a row's cells of one face's style, which a font may shape
/// together, as ligatures and combining marks need.
#[derive(Clone, Copy, Debug)]
pub struct Run<'a> {
    /// The cells' grapheme clusters, one after another.
    pub text: &'a str,
    /// The cells, from the left.
    pub cells: &'a [RunCell],
    /// Their style: bold and italic choose the face.
    pub style: &'a Style,
    /// Whether their backgrounds are darker than their text.
    pub dark: bool,
}

impl<'a> Run<'a> {
    /// The grapheme cluster of the run's cell `index`.
    pub fn cluster(&self, index: usize) -> &'a str {
        let start = self.cells[index].start as usize;
        let end = self
            .cells
            .get(index + 1)
            .map_or(self.text.len(), |next| next.start as usize);
        &self.text[start..end]
    }
}

/// A cell of a [`Run`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunCell {
    /// The byte where its cluster starts, in the run's text.
    pub start: u32,
    /// Whether it takes two columns.
    pub wide: bool,
}

/// A glyph of a [`Run`], at one of its cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunGlyph {
    /// The run's cell, by its index, where the glyph's cluster starts: its
    /// pen.
    pub cell: usize,
    /// The glyph.
    pub glyph: AtlasGlyph,
    /// How far the glyph moves from the cell's pen, rightwards, in pixels.
    pub x: i32,
    /// How far it moves from the baseline, upwards, in pixels.
    pub y: i32,
}

/// What finds the glyphs of a pane's text, through a font and its atlas.
pub trait Glyphs {
    /// The glyph of one cell's text.
    fn glyph(&mut self, shown: &CellText<'_>) -> Option<AtlasGlyph>;

    /// Whether the glyphs take the weight of the system's text: not by
    /// default.
    fn system_weight(&self) -> bool {
        false
    }

    /// Appends the glyphs of `run` to `out`: by default, each cell's own.
    fn run(&mut self, run: &Run<'_>, out: &mut Vec<RunGlyph>) {
        for (index, cell) in run.cells.iter().enumerate() {
            let shown = CellText {
                text: run.cluster(index),
                style: run.style,
                wide: cell.wide,
                dark: run.dark,
            };
            if let Some(glyph) = self.glyph(&shown) {
                out.push(RunGlyph {
                    cell: index,
                    glyph,
                    x: 0,
                    y: 0,
                });
            }
        }
    }
}

/// A function that gives a cell's glyph finds the glyphs a cell at a time.
impl<F: FnMut(&CellText<'_>) -> Option<AtlasGlyph>> Glyphs for F {
    fn glyph(&mut self, shown: &CellText<'_>) -> Option<AtlasGlyph> {
        self(shown)
    }
}

/// The cells of the run being laid out in a row: their text, and the
/// column and the text's color of each.
#[derive(Default)]
struct RunBuilder {
    text: String,
    cells: Vec<RunCell>,
    columns: Vec<u16>,
    colors: Vec<Color>,
    style: Style,
    dark: bool,
    glyphs: Vec<RunGlyph>,
}

impl RunBuilder {
    /// Whether a cell of `style`, of a background darker than its text when
    /// `dark`, goes on with the run: of the same face's style, bold and
    /// italic alike, as dark, and blinking alike, so that text that blinks
    /// and text that does not take no ligature together, which the
    /// blinking would split.
    fn takes(&self, style: &Style, dark: bool) -> bool {
        self.cells.is_empty()
            || ((self.style.bold, self.style.italic, self.style.blink)
                == (style.bold, style.italic, style.blink)
                && self.dark == dark)
    }

    /// Adds a cell, at `column`, of `cluster`, of `style`, in `color`, on a
    /// background darker than it when `dark`.
    fn push(
        &mut self,
        column: u16,
        cluster: impl Iterator<Item = char>,
        wide: bool,
        style: &Style,
        dark: bool,
        color: Color,
    ) {
        if self.cells.is_empty() {
            self.style = *style;
            self.dark = dark;
        }
        self.cells.push(RunCell {
            start: u32::try_from(self.text.len()).unwrap_or(u32::MAX),
            wide,
        });
        self.text.extend(cluster);
        self.columns.push(column);
        self.colors.push(color);
    }

    /// Lays out the run's glyphs into `batch`, at `row`, on the baseline of
    /// `cell`, and starts a new run.
    fn flush(
        &mut self,
        batch: &mut CellBatch,
        row: u16,
        cell: &CellMetrics,
        glyphs: &mut dyn Glyphs,
    ) {
        if self.cells.is_empty() {
            return;
        }
        let run = Run {
            text: &self.text,
            cells: &self.cells,
            style: &self.style,
            dark: self.dark,
        };
        glyphs.run(&run, &mut self.glyphs);
        let baseline = i32::try_from(cell.baseline).unwrap_or(i32::MAX);
        // A glyph without ink, a space's, draws nothing.
        for placed in self.glyphs.drain(..) {
            if placed.glyph.region.width == 0 {
                continue;
            }
            batch.glyphs.push(CellGlyph {
                column: self.columns[placed.cell],
                row,
                region: placed.glyph.region,
                x: placed.glyph.left + placed.x,
                // The baseline's row, in the cell, less the rows of the
                // glyph above it.
                y: baseline - placed.glyph.top - placed.y,
                color: self.colors[placed.cell],
                smoothed: placed.glyph.smoothed,
            });
        }
        self.text.clear();
        self.cells.clear();
        self.columns.clear();
        self.colors.clear();
    }
}

/// How the text draws in a frame: its section of the configuration, and
/// whether the text that blinks shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextLook {
    /// The configuration's `[text]` section.
    pub config: TextConfig,
    /// Whether the text that blinks shows in this frame, which hides it half
    /// of the time.
    pub blink_shown: bool,
    /// The least contrast ratio of a cell's text to its background, from 1,
    /// none, to 21: a text of less moves toward white or black, as the
    /// theme's `minimum-contrast` asks.
    pub minimum_contrast: f32,
}

impl Default for TextLook {
    fn default() -> Self {
        Self {
            config: TextConfig::default(),
            blink_shown: true,
            minimum_contrast: 1.0,
        }
    }
}

/// How the cursor draws in a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CursorLook {
    /// Its shape.
    pub shape: CursorShape,
    /// The pixels of a bar's width and of an underline's height; a hollow
    /// block's edges take the face's underline thickness.
    pub thickness: u32,
}

/// The text that an input method composes, which a pane shows at its
/// cursor and never sends.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Preedit {
    /// The text.
    pub text: String,
    /// Where the input method's cursor is in it, in bytes.
    pub cursor: usize,
}

/// The cells of a selection, and its colors.
#[derive(Clone, Copy)]
pub struct Selection<'a> {
    /// The selection's colors.
    pub colors: SelectionColors,
    /// Whether the cell at a column and a row is selected.
    pub contains: &'a dyn Fn(u16, u16) -> bool,
}

/// The color that the grid's empty cells show: the default background, or
/// the default foreground in reverse video.
pub fn background(snapshot: &Snapshot) -> Color {
    defaults(snapshot).1
}

/// The default text's color of `snapshot`, as [`background`] is its
/// background's: of the program's colors, swapped by reverse video.
pub fn foreground(snapshot: &Snapshot) -> Color {
    defaults(snapshot).0
}

/// The grid of `snapshot`, of cells of `cell`, with its top left corner at
/// `x` and `y`: each cell's background, and the glyph of each cell that
/// shows a character, which `glyph` finds, through the fonts and the atlas,
/// or not; the cells of `selection` in its colors; and the cursor, as
/// `cursor` has it look, unless the program hid it or the blinking does.
///
/// A character that takes two columns has its glyph in the first, which
/// spans the two; the second has only its background, as has a glyph
/// without ink. A cell of the default background is clear, and shows the
/// display list's, [`background`], which can let what is behind the window
/// show through; one whose background the program colors, or that is
/// selected, is opaque. Reverse video swaps the default colors; faint text mixes
/// its color halfway to its background; hidden text shows its background
/// alone. The cursor covers both cells of a wide character, and the text
/// under a block takes its cell's background.
pub fn cells(
    snapshot: &Snapshot,
    cell: &CellMetrics,
    x: i32,
    y: i32,
    selection: Option<&Selection<'_>>,
    cursor: Option<CursorLook>,
    glyphs: &mut dyn Glyphs,
) -> CellBatch {
    cells_with(
        snapshot,
        cell,
        (x, y),
        selection,
        cursor,
        TextLook::default(),
        glyphs,
    )
}

/// Lays `snapshot`'s decorations over `batch`, its grid as [`cells_with`]
/// laid it out: each span's background in place of its cells' own, and
/// its underline under them, of its own color or the default text's.
pub fn decorate(batch: &mut CellBatch, snapshot: &Snapshot, cell: &CellMetrics) {
    let text = defaults(snapshot).0;
    for span in snapshot.decorations() {
        if span.row >= batch.rows || span.first >= batch.columns || span.first > span.last {
            continue;
        }
        let last = span.last.min(batch.columns - 1);
        if let Some(background) = span.look.background {
            let start = usize::from(span.row) * usize::from(batch.columns);
            for column in span.first..=last {
                batch.backgrounds[start + usize::from(column)] = color(background);
            }
        }
        let style = match span.look.underline {
            page::Underline::None => continue,
            page::Underline::Single => LineStyle::Single,
            page::Underline::Double => LineStyle::Double,
            page::Underline::Curly => LineStyle::Curly,
            page::Underline::Dotted => LineStyle::Dotted,
            page::Underline::Dashed => LineStyle::Dashed,
        };
        batch.lines.push(CellLine {
            column: span.first,
            row: span.row,
            columns: last - span.first + 1,
            style,
            top: cell.underline_top,
            thickness: cell.underline_thickness,
            color: span.look.underline_color.map_or(text, color),
        });
    }
}

/// Lays the highlights of `snapshot`'s searches' matches over `batch`, its
/// grid as [`cells_with`] laid it out: `colors`' match's over each
/// highlighted cell's background, and its current one's over the current
/// match's, translucent, so that a cell's own color shows through, and a
/// cell of the default background, clear, stays translucent over a
/// translucent window.
pub fn highlight(batch: &mut CellBatch, snapshot: &Snapshot, colors: MatchColors) {
    for span in snapshot.highlights() {
        if span.row >= batch.rows || span.first >= batch.columns || span.first > span.last {
            continue;
        }
        let last = span.last.min(batch.columns - 1);
        let color = if span.current {
            colors.current
        } else {
            colors.matched
        };
        let start = usize::from(span.row) * usize::from(batch.columns);
        for column in span.first..=last {
            let cell = &mut batch.backgrounds[start + usize::from(column)];
            *cell = paint::over(color, *cell);
        }
    }
}

/// Whether `snapshot` shows text that blinks, as SGR 5 and 6 make it: some
/// cell of a style that blinks.
pub fn has_blinking_text(snapshot: &Snapshot) -> bool {
    (0..snapshot.rows()).any(|row| snapshot.line(row).styles().any(|style| style.blink))
}

/// The grid of `snapshot`, as [`cells`] lays it out, its text as `text`
/// has it look: the programs' colors, or the default ones without
/// `ansi-colors`; bold text of the first eight colors in their bright ones
/// with `bold-is-bright`; and the text that blinks hidden when the frame
/// does not show it, with `blink`.
pub fn cells_with(
    snapshot: &Snapshot,
    cell: &CellMetrics,
    origin: (i32, i32),
    selection: Option<&Selection<'_>>,
    cursor: Option<CursorLook>,
    text: TextLook,
    glyphs: &mut dyn Glyphs,
) -> CellBatch {
    let shown = Shown::All;
    cells_of(
        snapshot,
        cell,
        origin,
        selection,
        cursor,
        (text, shown),
        glyphs,
    )
}

/// The text that blinks of `snapshot`, as SGR 5 and 6 make it, as a batch
/// of its own, for a layer over the grid that blinks apart from it, where
/// the grid draws that text hidden: its glyphs and lines, in the colors
/// that [`cells_with`] gives them when the blinking shows them, over the
/// rows from the first that shows them to the last, with the cells'
/// backgrounds clear, as the grid shows them; none where the composition
/// of `preedit` covers it. With the first of its rows in the grid; `None`
/// without text that blinks, or when `text`'s config keeps it steady.
pub fn blinking_cells(
    snapshot: &Snapshot,
    cell: &CellMetrics,
    selection: Option<&Selection<'_>>,
    text: TextLook,
    preedit: Option<&Preedit>,
    glyphs: &mut dyn Glyphs,
) -> Option<(u16, CellBatch)> {
    if !text.config.blink || !has_blinking_text(snapshot) {
        return None;
    }
    let look = TextLook {
        blink_shown: true,
        ..text
    };
    let shown = Shown::Blinking;
    let mut batch = cells_of(
        snapshot,
        cell,
        (0, 0),
        selection,
        None,
        (look, shown),
        glyphs,
    );
    // The composition shows in place of the text under it, and its lines.
    if let Some((row, covered)) = preedit
        .filter(|preedit| !preedit.text.is_empty())
        .and_then(|preedit| composed_span(snapshot, preedit, batch.columns))
    {
        batch
            .glyphs
            .retain(|glyph| glyph.row != row || !covered.contains(&glyph.column));
        cut_lines(&mut batch, row, covered);
    }
    let drawn = batch.glyphs.iter().map(|glyph| glyph.row);
    let drawn = drawn.chain(batch.lines.iter().map(|line| line.row));
    let (first, last) = drawn.fold(None, |rows, row| {
        Some(rows.map_or((row, row), |(first, last): (u16, u16)| {
            (first.min(row), last.max(row))
        }))
    })?;
    for glyph in &mut batch.glyphs {
        glyph.row -= first;
    }
    for line in &mut batch.lines {
        line.row -= first;
    }
    batch.rows = last - first + 1;
    let clear = background(snapshot).with_alpha(0.0);
    batch.backgrounds = vec![clear; usize::from(batch.columns) * usize::from(batch.rows)];
    Some((first, batch))
}

/// What a batch of the grid shows of its text.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shown {
    /// All of it, as its look has it.
    All,
    /// The text that blinks alone, in the rows that hold it: the others
    /// take nothing, not even their backgrounds.
    Blinking,
}

/// The grid of `snapshot`, as [`cells_with`] lays it out, of the text that
/// `shown` says.
fn cells_of(
    snapshot: &Snapshot,
    cell: &CellMetrics,
    (x, y): (i32, i32),
    selection: Option<&Selection<'_>>,
    cursor: Option<CursorLook>,
    (text, shown): (TextLook, Shown),
    glyphs: &mut dyn Glyphs,
) -> CellBatch {
    let (columns, rows) = (snapshot.cols(), snapshot.rows());
    let colors = snapshot.colors();
    let defaults = defaults(snapshot);
    // The last color of selected text that kept the text's own, by its
    // bits, and what it showed on the selection's background, which every
    // selected cell has: cells next to each other most often have the same,
    // and a contrast is dear for each cell of a large selection.
    // No color has the bits of a NaN, which start it.
    let mut readables = [([u32::MAX; 4], Color::default()); 4];
    let mut readable_next = 0;
    // The text's colors of the theme's least contrast, by the bits of each
    // pair of a text's color and the background under it: a frame has few
    // such pairs, and a contrast is dear for each cell.
    let mut contrasted = [([u32::MAX; 8], Color::default()); CONTRASTED];
    let mut contrasted_next = 0;
    let area = usize::from(columns) * usize::from(rows);
    let mut batch = CellBatch {
        x,
        y,
        columns,
        rows,
        cell_width: cell.width,
        cell_height: cell.height,
        backgrounds: Vec::with_capacity(area),
        glyphs: Vec::with_capacity(area),
        lines: Vec::new(),
        cursor: cursor.and_then(|look| place_cursor(snapshot, look, cell)),
        system_weight: glyphs.system_weight(),
    };
    let selected = |column: u16, row: u16| {
        selection
            .filter(|selection| (selection.contains)(column, row))
            .map(|selection| selection.colors)
    };
    // The cursor's cell is a run of its own, so that the text under it shows
    // its own characters, as a ligature would not, in either phase of its
    // blinking, so that the ligature does not join and split as it blinks.
    let at = cursor_cell(snapshot).map(|(column, row, _)| (column, row));
    let mut run = RunBuilder::default();
    let mut decorations = Decorations::default();
    for row in 0..rows {
        let line = snapshot.line(row);
        if shown == Shown::Blinking && !line.styles().any(|style| style.blink) {
            continue;
        }
        for (column, &cell_of) in (0..columns).zip(line.cells()) {
            if at == Some((column, row)) {
                run.flush(&mut batch, row, cell, glyphs);
            }
            let chosen = selected(column, row);
            let style = line.style(cell_of.style());
            let (mut foreground, mut background) =
                style_colors(&style, colors, defaults, text.config);
            let own_background = text.config.ansi_colors && style.bg != page::Color::Default;
            // The text of the contrast that the theme asks, over the
            // background that shows under it.
            if text.minimum_contrast > 1.0 {
                let under = if own_background || style.inverse {
                    background
                } else {
                    defaults.1
                };
                let (text_bits, under_bits) = (bits(foreground), bits(under));
                let mut key = [0; 8];
                key[..4].copy_from_slice(&text_bits);
                key[4..].copy_from_slice(&under_bits);
                foreground = match contrasted.iter().find(|(known, _)| *known == key) {
                    Some(&(_, shown)) => shown,
                    None => {
                        let shown = theme::with_contrast(foreground, under, text.minimum_contrast);
                        contrasted[contrasted_next] = (key, shown);
                        contrasted_next = (contrasted_next + 1) % CONTRASTED;
                        shown
                    }
                };
            }
            if !own_background && !style.inverse {
                background = background.with_alpha(0.0);
            }
            let character = match cell_of.content() {
                Content::Char(character) | Content::Grapheme(character) => character,
                Content::BgPalette(_) | Content::BgRgb(_) if !text.config.ansi_colors => {
                    let erased = defaults.1.with_alpha(0.0);
                    batch
                        .backgrounds
                        .push(chosen.map_or(erased, |chosen| chosen.background));
                    continue;
                }
                Content::BgPalette(index) => {
                    let erased = color(colors.palette[usize::from(index)]);
                    batch
                        .backgrounds
                        .push(chosen.map_or(erased, |chosen| chosen.background));
                    continue;
                }
                Content::BgRgb(rgb) => {
                    batch
                        .backgrounds
                        .push(chosen.map_or(color(rgb), |chosen| chosen.background));
                    continue;
                }
            };
            if let Some(chosen) = chosen {
                background = chosen.background;
                foreground = match chosen.foreground {
                    Some(own) => own,
                    None => {
                        let key = bits(foreground);
                        match readables.iter().find(|(known, _)| *known == key) {
                            Some(&(_, shown)) => shown,
                            None => {
                                let shown = readable(foreground, background, defaults);
                                readables[readable_next] = (key, shown);
                                readable_next = (readable_next + 1) % readables.len();
                                shown
                            }
                        }
                    }
                };
            }
            batch.backgrounds.push(background);
            // Text that blinks shows half of the time.
            let blinking = style.blink && text.config.blink;
            let hidden = style.invisible
                || (blinking && !text.blink_shown)
                || (shown == Shown::Blinking && !blinking);
            // The lines that decorate the text, as faint as it, and of its
            // color but for an underline's own, which spaces and a wide
            // character's second cell take too.
            let decorated =
                style.underline != page::Underline::None || style.strikethrough || style.overline;
            // A cell of the style of the one before, both selected or not,
            // goes on with its lines.
            let same = (cell_of.style(), chosen.is_some());
            if decorated && !hidden && !decorations.goes_on(row, column, same) {
                let shade = if style.faint {
                    mix(foreground, background)
                } else {
                    foreground
                };
                let own = text
                    .config
                    .ansi_colors
                    .then(|| resolved(style.underline_color, colors))
                    .flatten();
                decorations.cell(&mut batch, (column, row), &style, (shade, own), cell);
                decorations.last = Some((row, column, same));
            }
            let width = cell_of.width();
            // A wide character's second cell goes on with its run.
            if width == Width::SpacerTail {
                continue;
            }
            // A space draws nothing, and ends a run, as no ligature takes
            // one: most of a screen is spaces.
            let grapheme = line.grapheme(column);
            let blank = character == '\0' || (character == ' ' && grapheme.is_empty());
            if blank || hidden || width == Width::SpacerHead {
                run.flush(&mut batch, row, cell, glyphs);
                continue;
            }
            if style.faint {
                foreground = mix(foreground, background);
            }
            let dark = luma(background) < luma(foreground);
            if !run.takes(&style, dark) {
                run.flush(&mut batch, row, cell, glyphs);
            }
            let cluster = std::iter::once(character).chain(grapheme.iter().copied());
            run.push(
                column,
                cluster,
                width == Width::Wide,
                &style,
                dark,
                foreground,
            );
            if at == Some((column, row)) {
                run.flush(&mut batch, row, cell, glyphs);
            }
        }
        run.flush(&mut batch, row, cell, glyphs);
        decorations.flush(&mut batch);
        // A line shorter than the screen, as after a resize, has the
        // default background past its end.
        for column in u16::try_from(line.cells().len()).unwrap_or(u16::MAX)..columns {
            let chosen = selected(column, row);
            batch
                .backgrounds
                .push(chosen.map_or(defaults.1.with_alpha(0.0), |chosen| chosen.background));
        }
    }
    settle_cursor(&mut batch, snapshot);
    batch
}

/// The lines that decorate a row's text, as its cells come: each kind's,
/// underline, strikeout and overline, joined while the next cell goes on
/// with the same line, so that a wave or dashes go on across them.
#[derive(Default)]
struct Decorations {
    open: [Option<CellLine>; 3],
    /// The last cell that took lines: its row and column, and its style's
    /// id in its row's page, and whether it was selected, which its lines'
    /// colors follow.
    last: Option<(u16, u16, (page::StyleId, bool))>,
}

impl Decorations {
    /// Whether the cell at `column` of `row`, of the style and selection
    /// of `same`, goes on with the lines of the cell before it, which are
    /// its own, as the cell before has its style and selection: the open
    /// lines take it.
    fn goes_on(&mut self, row: u16, column: u16, same: (page::StyleId, bool)) -> bool {
        let follows = self.last == Some((row, column.wrapping_sub(1), same));
        if follows {
            for open in self.open.iter_mut().flatten() {
                if open.row == row && open.column + open.columns == column {
                    open.columns += 1;
                }
            }
            self.last = Some((row, column, same));
        }
        follows
    }

    /// Takes the lines of the cell at `at`, of `style`, in `color`, the
    /// text's, and `own`, an underline's own color, if it has one. Out of
    /// the loop over the cells, which most cells skip it.
    #[inline(never)]
    fn cell(
        &mut self,
        batch: &mut CellBatch,
        (column, row): (u16, u16),
        style: &Style,
        (color, own): (Color, Option<Color>),
        cell: &CellMetrics,
    ) {
        let line = |style: LineStyle, top: u32, thickness: u32, color: Color| CellLine {
            column,
            row,
            columns: 1,
            style,
            top,
            thickness,
            color,
        };
        let underline = match style.underline {
            page::Underline::None => None,
            page::Underline::Single => Some(LineStyle::Single),
            page::Underline::Double => Some(LineStyle::Double),
            page::Underline::Curly => Some(LineStyle::Curly),
            page::Underline::Dotted => Some(LineStyle::Dotted),
            page::Underline::Dashed => Some(LineStyle::Dashed),
        };
        let (top, thickness) = (cell.underline_top, cell.underline_thickness);
        if let Some(underline) = underline {
            self.push(
                batch,
                0,
                line(underline, top, thickness, own.unwrap_or(color)),
            );
        }
        if style.strikethrough {
            let (top, thickness) = (cell.strikeout_top, cell.strikeout_thickness);
            self.push(batch, 1, line(LineStyle::Single, top, thickness, color));
        }
        if style.overline {
            self.push(batch, 2, line(LineStyle::Single, 0, thickness, color));
        }
    }

    /// Takes `line`, of the kind `kind`: the open line of the kind goes on
    /// with it when it is the same line in the next cell; otherwise it
    /// goes into `batch`, and `line` opens.
    fn push(&mut self, batch: &mut CellBatch, kind: usize, line: CellLine) {
        match &mut self.open[kind] {
            Some(open)
                if open.row == line.row
                    && open.column + open.columns == line.column
                    && (open.style, open.top, open.thickness, open.color)
                        == (line.style, line.top, line.thickness, line.color) =>
            {
                open.columns += line.columns;
            }
            slot => {
                if let Some(done) = slot.replace(line) {
                    batch.lines.push(done);
                }
            }
        }
    }

    /// Puts the open lines into `batch`.
    fn flush(&mut self, batch: &mut CellBatch) {
        for slot in &mut self.open {
            if let Some(done) = slot.take() {
                batch.lines.push(done);
            }
        }
    }
}

/// The color that `which` names, of `colors`' palette; `None` for the
/// default, the text's.
fn resolved(which: page::Color, colors: &Colors) -> Option<Color> {
    match which {
        page::Color::Default => None,
        page::Color::Palette(index) => Some(color(colors.palette[usize::from(index)])),
        page::Color::Rgb(rgb) => Some(color(rgb)),
    }
}

/// Takes the cells of `columns` on `row` out of `batch`'s lines: those
/// that a composition covers, whose text it hides.
fn cut_lines(batch: &mut CellBatch, row: u16, columns: std::ops::Range<u16>) {
    let lines = std::mem::take(&mut batch.lines);
    for line in lines {
        let end = line.column + line.columns;
        if line.row != row || end <= columns.start || line.column >= columns.end {
            batch.lines.push(line);
            continue;
        }
        if line.column < columns.start {
            batch.lines.push(CellLine {
                columns: columns.start - line.column,
                ..line
            });
        }
        if end > columns.end {
            batch.lines.push(CellLine {
                column: columns.end,
                columns: end - columns.end,
                ..line
            });
        }
    }
}

/// The least contrast, as WCAG measures it, of a cursor on its cell's
/// background.
const CURSOR_CONTRAST: f64 = 1.5;

/// Gives `batch`'s cursor a color apart from its cell's background, as
/// xterm keeps the cursor's color apart from the background's: the
/// cursor's own color, or, when that is too near, as on inverse text or in
/// reverse video, the color of the cell's text, or else black or white,
/// whichever stands farther; a block's text, and the lines that cross it,
/// take the cell's background.
fn settle_cursor(batch: &mut CellBatch, snapshot: &Snapshot) {
    let Some(cursor) = &mut batch.cursor else {
        return;
    };
    let (column, row) = (cursor.column, cursor.row);
    let index = usize::from(row) * usize::from(batch.columns) + usize::from(column);
    let Some(&background) = batch.backgrounds.get(index) else {
        return;
    };
    // The cell's glyphs, a base and its marks, come together, as its run
    // laid them out; the cursor is most often low on the screen, near the
    // end of the glyphs.
    let under = |glyph: &CellGlyph| glyph.row == row && glyph.column == column;
    let end = batch
        .glyphs
        .iter()
        .rposition(under)
        .map_or(0, |last| last + 1);
    let start = batch.glyphs[..end]
        .iter()
        .rposition(|glyph| !under(glyph))
        .map_or(0, |before| before + 1);
    let glyphs = &mut batch.glyphs[start..end];
    let text = glyphs
        .first()
        .map_or(defaults(snapshot).0, |glyph| glyph.color);
    let own = color(snapshot.colors().cursor);
    cursor.color = if contrast(own, background) >= CURSOR_CONTRAST {
        own
    } else if contrast(text, background) >= CURSOR_CONTRAST {
        text
    } else if luminance(background) > 0.18 {
        Color::from_rgb8(0, 0, 0)
    } else {
        Color::from_rgb8(255, 255, 255)
    };
    if cursor.shape == CursorShape::Block {
        for glyph in glyphs {
            glyph.color = background.with_alpha(1.0);
        }
        // The lines that cross the block, as an underline, take the cell's
        // background over it too, as its text does: a piece of each.
        let columns = cursor.column..cursor.column + cursor.columns;
        let crossing: Vec<CellLine> = batch
            .lines
            .iter()
            .filter(|line| {
                line.row == row
                    && line.column < columns.end
                    && line.column + line.columns > columns.start
            })
            .copied()
            .collect();
        if crossing.is_empty() {
            return;
        }
        cut_lines(batch, row, columns.clone());
        for line in crossing {
            let start = line.column.max(columns.start);
            let end = (line.column + line.columns).min(columns.end);
            batch.lines.push(CellLine {
                column: start,
                columns: end - start,
                color: background.with_alpha(1.0),
                ..line
            });
        }
    }
}

/// The relative luminance of `color`, of sRGB's encoded components, as
/// WCAG 2 defines it.
fn luminance(color: Color) -> f64 {
    let linear = |channel: f32| {
        let value = f64::from(channel);
        if value <= 0.040_45 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(color.red) + 0.7152 * linear(color.green) + 0.0722 * linear(color.blue)
}

/// The brightness of `color`, of its encoded components as Rec. 709 weighs
/// them, which orders a cell's two colors as their luminance does, at a cost
/// that each cell of each frame can take.
fn luma(color: Color) -> f32 {
    0.2126 * color.red + 0.7152 * color.green + 0.0722 * color.blue
}

/// The least contrast, as WCAG measures it, of selected text on a selection
/// that keeps the text's colors: WCAG's AA for large text.
const SELECTED_CONTRAST: f64 = 3.0;

/// `text` when it reads on `background`; otherwise whichever reads better
/// of the default text's color and the default background's, `defaults`,
/// as inverse text, of the default background's color, does not read on a
/// selection near it.
fn readable(text: Color, background: Color, (fg, bg): (Color, Color)) -> Color {
    if contrast(text, background) >= SELECTED_CONTRAST {
        text
    } else if contrast(fg, background) >= contrast(bg, background) {
        fg
    } else {
        bg
    }
}

/// The bits of `color`'s components, which compare as one.
/// How many pairs of a text's color and its background a frame keeps the
/// colors of the least contrast of: the palette's sixteen, and the default.
const CONTRASTED: usize = 17;

fn bits(color: Color) -> [u32; 4] {
    [
        color.red.to_bits(),
        color.green.to_bits(),
        color.blue.to_bits(),
        color.alpha.to_bits(),
    ]
}

/// The contrast ratio of two colors, as WCAG 2 defines it: from 1 to 21.
fn contrast(a: Color, b: Color) -> f64 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// Draws `preedit` over `batch`, the grid of `snapshot`, from its cursor's
/// cell: each character in the cells that its width takes, on the default
/// background and in the default text's color, in place of what they show,
/// underlined, with the cursor where the input method has its own. What
/// goes past the right edge is left out, as is a character that takes no
/// cell.
pub fn preedit(
    batch: &mut CellBatch,
    snapshot: &Snapshot,
    cell: &CellMetrics,
    preedit: &Preedit,
    glyphs: &mut dyn Glyphs,
) {
    let (foreground, background) = defaults(snapshot);
    let columns = batch.columns;
    let Some((row, start)) = composition_start(snapshot, preedit, columns) else {
        return;
    };
    if row >= batch.rows {
        return;
    }
    let widths = Widths::default();
    let mut column = start;
    let mut text = String::new();
    for character in preedit.text.chars() {
        let width = u16::from(widths.char_width(character));
        if width == 0 {
            continue;
        }
        if column + width > columns {
            break;
        }
        let covered = column..column + width;
        batch
            .glyphs
            .retain(|glyph| glyph.row != row || !covered.contains(&glyph.column));
        let first = usize::from(row) * usize::from(columns) + usize::from(column);
        for drawn in &mut batch.backgrounds[first..first + usize::from(width)] {
            *drawn = background.with_alpha(0.0);
        }
        text.clear();
        text.push(character);
        let shown = CellText {
            text: &text,
            style: &Style::DEFAULT,
            wide: width == 2,
            dark: luma(background) < luma(foreground),
        };
        if let Some(found) = glyphs.glyph(&shown).filter(|found| found.region.width > 0) {
            batch.glyphs.push(CellGlyph {
                column,
                row,
                region: found.region,
                x: found.left,
                y: i32::try_from(cell.baseline).unwrap_or(i32::MAX) - found.top,
                color: foreground,
                smoothed: found.smoothed,
            });
        }
        column += width;
    }
    cut_lines(batch, row, start..column);
    batch.lines.push(CellLine {
        column: start,
        row,
        columns: column - start,
        style: LineStyle::Single,
        top: cell.underline_top,
        thickness: cell.underline_thickness,
        color: foreground,
    });
    if let (Some(drawn), Some((at, width))) = (
        &mut batch.cursor,
        composed_cursor(snapshot, preedit, columns),
    ) {
        drawn.column = at;
        drawn.columns = width;
    }
    settle_cursor(batch, snapshot);
}

/// The row and the column that the composition of `preedit` starts at, in
/// a grid of `columns` of `snapshot`: the cursor's, from a wide
/// character's first cell; or, where the composition has no room there to
/// the right edge, as far left as it takes, over the cells before, but
/// the row's first, so that a composition wider than the row loses its
/// end alone, and from a wide character's first cell there too, which it
/// covers whole; `None` off the grid.
pub fn composition_start(
    snapshot: &Snapshot,
    preedit: &Preedit,
    columns: u16,
) -> Option<(u16, u16)> {
    let position = snapshot.cursor().position;
    let (mut start, row) = (position.x, position.y);
    if row >= snapshot.rows() || start >= columns {
        return None;
    }
    let line = snapshot.line(row);
    if start > 0
        && line
            .cells()
            .get(usize::from(start))
            .is_some_and(|cell| cell.width() == Width::SpacerTail)
    {
        start -= 1;
    }
    let widths = Widths::default();
    let width: usize = preedit
        .text
        .chars()
        .map(|character| usize::from(widths.char_width(character)))
        .sum();
    let width = u16::try_from(width).unwrap_or(u16::MAX).min(columns);
    let moved = start.min(columns - width);
    let tail = |column: u16| {
        line.cells()
            .get(usize::from(column))
            .is_some_and(|cell| cell.width() == Width::SpacerTail)
    };
    Some((
        row,
        if moved > 0 && tail(moved) {
            moved - 1
        } else {
            moved
        },
    ))
}

/// The cells that the composition of `preedit` at the cursor of `snapshot`
/// covers, in a grid of `columns`, as [`preedit`] draws it: its row and
/// columns; `None` off the grid.
fn composed_span(
    snapshot: &Snapshot,
    preedit: &Preedit,
    columns: u16,
) -> Option<(u16, std::ops::Range<u16>)> {
    let (row, start) = composition_start(snapshot, preedit, columns)?;
    let widths = Widths::default();
    let mut end = start;
    for character in preedit.text.chars() {
        let width = u16::from(widths.char_width(character));
        if end + width > columns {
            break;
        }
        end += width;
    }
    Some((row, start..end))
}

/// Where the cursor goes in the composition of `preedit` at the cursor of
/// `snapshot`, in a grid of `columns`: its column and the cells that it
/// covers, as [`preedit`] draws it; `None` off the grid.
fn composed_cursor(snapshot: &Snapshot, preedit: &Preedit, columns: u16) -> Option<(u16, u16)> {
    let (row, start) = composition_start(snapshot, preedit, columns)?;
    let line = snapshot.line(row);
    let width_at = |column: u16| {
        line.cells()
            .get(usize::from(column))
            .map(|cell| cell.width())
    };
    let widths = Widths::default();
    let mut column = start;
    let mut cursor = None;
    let mut placed = Vec::new();
    for (at, character) in preedit.text.char_indices() {
        if at == preedit.cursor {
            cursor = Some(column);
        }
        let width = u16::from(widths.char_width(character));
        if width == 0 {
            continue;
        }
        if column + width > columns {
            break;
        }
        placed.push((column, width));
        column += width;
    }
    let at = cursor.unwrap_or(column).min(columns - 1);
    // A wide character under the cursor, composed or the grid's after the
    // composition, takes it whole, from its first cell, as a composition
    // cut at the right edge can leave the cursor on its second.
    let (at, width) = match placed
        .iter()
        .find(|&&(first, width)| (first..first + width).contains(&at))
    {
        Some(&(first, width)) => (first, width),
        None if width_at(at) == Some(Width::Wide) => (at, 2),
        None => (at, 1),
    };
    Some((at, width.min(columns - at)))
}

/// The cursor's cells of `batch`, a frame's grid of `snapshot` that drew no
/// cursor, as a batch of their own, for a layer over the grid that blinks
/// apart from it: the cursor as `look` has it look, at its cell, or after
/// the text that `preedit` composes; the text under a block in its cell's
/// background, as the grid's own cursor shows it; and nothing else, as the
/// grid shows the rest. With the cursor's column and row in the grid;
/// `None` while the program hides it.
pub fn cursor_cells(
    batch: &CellBatch,
    snapshot: &Snapshot,
    look: CursorLook,
    cell: &CellMetrics,
    preedit: Option<&Preedit>,
) -> Option<(u16, u16, CellBatch)> {
    let mut cursor = place_cursor(snapshot, look, cell)?;
    if let Some((at, width)) = preedit
        .filter(|preedit| !preedit.text.is_empty())
        .and_then(|preedit| composed_cursor(snapshot, preedit, batch.columns))
    {
        cursor.column = at;
        cursor.columns = width;
    }
    let (column, row) = (cursor.column, cursor.row);
    let covered = column..column + cursor.columns;
    let first = usize::from(row) * usize::from(batch.columns) + usize::from(column);
    let backgrounds = batch
        .backgrounds
        .get(first..first + usize::from(cursor.columns))?
        .to_vec();
    let glyphs = batch
        .glyphs
        .iter()
        .filter(|glyph| glyph.row == row && covered.contains(&glyph.column))
        .map(|&glyph| CellGlyph {
            column: glyph.column - column,
            row: 0,
            ..glyph
        })
        .collect();
    let lines = batch
        .lines
        .iter()
        .filter(|line| {
            line.row == row && line.column < covered.end && line.column + line.columns > column
        })
        .map(|&line| {
            let start = line.column.max(column);
            let end = (line.column + line.columns).min(covered.end);
            CellLine {
                column: start - column,
                row: 0,
                columns: end - start,
                ..line
            }
        })
        .collect();
    let mut cells = CellBatch {
        x: 0,
        y: 0,
        columns: cursor.columns,
        rows: 1,
        cell_width: batch.cell_width,
        cell_height: batch.cell_height,
        backgrounds,
        glyphs,
        lines,
        cursor: Some(CellCursor {
            column: 0,
            row: 0,
            ..cursor
        }),
        system_weight: batch.system_weight,
    };
    settle_cursor(&mut cells, snapshot);
    // The grid shows its cells' backgrounds, and their text but under a
    // block.
    for background in &mut cells.backgrounds {
        *background = background.with_alpha(0.0);
    }
    if cursor.shape != CursorShape::Block {
        cells.glyphs.clear();
        cells.lines.clear();
    }
    Some((column, row, cells))
}

/// The cell of `snapshot`'s cursor, its column and row, and whether it is
/// a wide character's, when the program shows it in the view: a wide
/// character's first, from either of its cells.
fn cursor_cell(snapshot: &Snapshot) -> Option<(u16, u16, bool)> {
    let cursor = snapshot.cursor();
    let (mut column, row) = (cursor.position.x, cursor.position.y);
    let (columns, rows) = (snapshot.cols(), snapshot.rows());
    if !cursor.visible || row >= rows || column >= columns {
        return None;
    }
    let cells = snapshot.line(row).cells();
    let width = |column: u16| cells.get(usize::from(column)).map(|cell| cell.width());
    if column > 0 && width(column) == Some(Width::SpacerTail) {
        column -= 1;
    }
    Some((column, row, width(column) == Some(Width::Wide)))
}

/// The cursor of `snapshot`, as `look` has it, where it shows: on a wide
/// character, over its two cells, from either.
fn place_cursor(snapshot: &Snapshot, look: CursorLook, cell: &CellMetrics) -> Option<CellCursor> {
    let (column, row, wide) = cursor_cell(snapshot)?;
    let columns = snapshot.cols();
    Some(CellCursor {
        column,
        row,
        columns: if wide { 2 } else { 1 }.min(columns - column),
        shape: look.shape,
        thickness: match look.shape {
            CursorShape::Bar | CursorShape::Underline => look.thickness,
            CursorShape::Block | CursorShape::Hollow => cell.underline_thickness,
        },
        color: color(snapshot.colors().cursor),
    })
}

/// The default text and background colors of `snapshot`, swapped in reverse
/// video.
fn defaults(snapshot: &Snapshot) -> (Color, Color) {
    let colors = snapshot.colors();
    let (foreground, background) = (color(colors.foreground), color(colors.background));
    if snapshot.reverse_video() {
        (background, foreground)
    } else {
        (foreground, background)
    }
}

/// The text's and the background's colors of `style`, from `colors` and the
/// `defaults`, and swapped for inverse text: the defaults alone without
/// `text`'s `ansi-colors`, and a bright color for bold text of one of the
/// first eight with its `bold-is-bright`.
fn style_colors(
    style: &Style,
    colors: &Colors,
    (fg, bg): (Color, Color),
    text: TextConfig,
) -> (Color, Color) {
    let resolve = |which: page::Color, default: Color| match which {
        _ if !text.ansi_colors => default,
        page::Color::Default => default,
        page::Color::Palette(index) => color(colors.palette[usize::from(index)]),
        page::Color::Rgb(rgb) => color(rgb),
    };
    let fg_color = match style.fg {
        page::Color::Palette(index) if text.bold_is_bright && style.bold && index < 8 => {
            page::Color::Palette(index + 8)
        }
        other => other,
    };
    let foreground = resolve(fg_color, fg);
    let background = resolve(style.bg, bg);
    if style.inverse {
        (background, foreground)
    } else {
        (foreground, background)
    }
}

/// Fades `batch`, of a pane that has not the keyboard, toward its default
/// background `toward` by `share`, from 0, not at all, to 1: its text, its
/// lines, its cursor and its cells' own backgrounds, each of its own
/// opacity, so that the cells of the default background stay clear over a
/// translucent window, as a rectangle over them would not; a glyph of its
/// own colors, as an emoji, whose color the renderer takes the opacity of
/// alone, takes the fade as an opacity over its faded cell.
pub fn fade(batch: &mut CellBatch, toward: Color, share: f32) {
    let share = share.clamp(0.0, 1.0);
    if share <= 0.0 {
        return;
    }
    let faded = |color: Color| Color {
        red: color.red + (toward.red - color.red) * share,
        green: color.green + (toward.green - color.green) * share,
        blue: color.blue + (toward.blue - color.blue) * share,
        alpha: color.alpha,
    };
    for background in &mut batch.backgrounds {
        *background = faded(*background);
    }
    for glyph in &mut batch.glyphs {
        glyph.color = faded(glyph.color);
        if glyph.region.texture == Texture::Color {
            glyph.color.alpha *= 1.0 - share;
        }
    }
    for line in &mut batch.lines {
        line.color = faded(line.color);
    }
    if let Some(cursor) = &mut batch.cursor {
        cursor.color = faded(cursor.color);
    }
}

/// The color halfway between `a` and `b`, in the encoded space, where the
/// text blends.
fn mix(a: Color, b: Color) -> Color {
    Color {
        red: (a.red + b.red) / 2.0,
        green: (a.green + b.green) / 2.0,
        blue: (a.blue + b.blue) / 2.0,
        alpha: a.alpha,
    }
}

fn color(rgb: Rgb) -> Color {
    Color::from_rgb8(rgb.r, rgb.g, rgb.b)
}

#[cfg(test)]
mod tests {
    use lokyr_render_core::Texture;
    use lokyr_text_core::{FaceMetrics, FontConfig};
    use lokyr_vt_core::parser::Parser;
    use lokyr_vt_core::terminal::{Options, Terminal};

    use super::*;

    /// The snapshot of a screen of `cols` by `rows` after `text`.
    fn snapshot(cols: u16, rows: u16, text: &str) -> Snapshot {
        let mut terminal = Terminal::new(cols, rows, Options::default());
        Parser::new().advance(&mut terminal, text.as_bytes());
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        snapshot
    }

    /// A cell of 8 by 16 pixels, its baseline at 13.
    fn cell() -> CellMetrics {
        CellMetrics::new(
            &FaceMetrics {
                ascent: 12.07,
                descent: 3.07,
                advance: 7.83,
                ..FaceMetrics::default()
            },
            &FontConfig::default(),
        )
    }

    /// A glyph of the atlas for each character, at a column of its own,
    /// that records what it was asked for; a space's has no ink, as the
    /// atlas gives it no room.
    fn atlas(
        asked: &mut Vec<(String, bool, bool)>,
    ) -> impl FnMut(&CellText<'_>) -> Option<AtlasGlyph> + '_ {
        move |shown: &CellText<'_>| {
            asked.push((shown.text.to_owned(), shown.style.bold, shown.wide));
            let first = shown.text.chars().next()?;
            let (width, height) = if first == ' ' { (0, 0) } else { (6, 10) };
            Some(AtlasGlyph {
                region: AtlasRegion {
                    texture: Texture::Coverage,
                    x: first as u32,
                    y: 0,
                    width,
                    height,
                },
                left: 1,
                top: 10,
                smoothed: true,
            })
        }
    }

    /// `color`, clear, as the cells of the default background are.
    fn clear(color: Color) -> Color {
        color.with_alpha(0.0)
    }

    fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color::from_rgb8(r, g, b)
    }

    #[test]
    fn a_snapshot_becomes_cells() {
        let snapshot = snapshot(6, 2, "AB\x1b[41;1mC\x1b[0m");
        let mut asked = Vec::new();
        let batch = cells(
            &snapshot,
            &cell(),
            4,
            30,
            None,
            None,
            &mut atlas(&mut asked),
        );
        assert_eq!((batch.x, batch.y, batch.columns, batch.rows), (4, 30, 6, 2));
        assert_eq!((batch.cell_width, batch.cell_height), (8, 16));
        assert_eq!(batch.backgrounds.len(), 12);
        let colors = Colors::default();
        // The default background's cells are clear: the list's background
        // shows through them.
        let background = clear(color(colors.background));
        assert_eq!(batch.backgrounds[0], background);
        // SGR 41 is the palette's red, opaque.
        assert_eq!(batch.backgrounds[2], color(colors.palette[1]));
        assert!(batch.backgrounds[3..].iter().all(|&bg| bg == background));
        // The three characters, and nothing for the empty cells.
        assert_eq!(
            asked,
            [
                ("A".to_owned(), false, false),
                ("B".to_owned(), false, false),
                ("C".to_owned(), true, false),
            ]
        );
        let glyphs: Vec<(u16, u16, i32, i32)> = batch
            .glyphs
            .iter()
            .map(|g| (g.column, g.row, g.x, g.y))
            .collect();
        // From the baseline at 13, a glyph whose top is 10 above it.
        assert_eq!(glyphs, [(0, 0, 1, 3), (1, 0, 1, 3), (2, 0, 1, 3)]);
        assert!(
            batch
                .glyphs
                .iter()
                .all(|g| g.color == color(colors.foreground))
        );
    }

    #[test]
    fn a_wide_character_takes_its_first_cell() {
        let snapshot = snapshot(6, 1, "a日b");
        let mut asked = Vec::new();
        let batch = cells(&snapshot, &cell(), 0, 0, None, None, &mut atlas(&mut asked));
        let columns: Vec<u16> = batch.glyphs.iter().map(|g| g.column).collect();
        assert_eq!(columns, [0, 1, 3]);
        assert_eq!(asked[1], ("日".to_owned(), false, true));
        assert_eq!(batch.backgrounds.len(), 6);
    }

    #[test]
    fn a_cluster_comes_whole() {
        let snapshot = snapshot(6, 1, "e\u{301}👍🏽");
        let mut asked = Vec::new();
        cells(&snapshot, &cell(), 0, 0, None, None, &mut atlas(&mut asked));
        let texts: Vec<&str> = asked.iter().map(|(text, ..)| text.as_str()).collect();
        assert_eq!(texts, ["e\u{301}", "👍🏽"]);
    }

    #[test]
    fn colors_resolve_from_the_palette_rgb_and_inverse() {
        let snapshot = snapshot(
            4,
            1,
            "\x1b[38;5;196ma\x1b[38;2;1;2;3;48;2;4;5;6mb\x1b[0;7mc",
        );
        let mut asked = Vec::new();
        let batch = cells(&snapshot, &cell(), 0, 0, None, None, &mut atlas(&mut asked));
        let colors = Colors::default();
        assert_eq!(batch.glyphs[0].color, color(colors.palette[196]));
        assert_eq!(batch.glyphs[1].color, rgb(1, 2, 3));
        assert_eq!(batch.backgrounds[1], rgb(4, 5, 6));
        // Inverse: the default background's color on the default
        // foreground's.
        assert_eq!(batch.glyphs[2].color, color(colors.background));
        assert_eq!(batch.backgrounds[2], color(colors.foreground));
    }

    #[test]
    fn an_erase_with_a_background_takes_its_color() {
        // Background color erase: the cleared cells keep the red.
        let snapshot = snapshot(4, 2, "\x1b[41m\x1b[2J\x1b[0m");
        let mut asked = Vec::new();
        let batch = cells(&snapshot, &cell(), 0, 0, None, None, &mut atlas(&mut asked));
        let red = color(Colors::default().palette[1]);
        assert!(batch.backgrounds.iter().all(|&bg| bg == red));
        assert!(batch.glyphs.is_empty() && asked.is_empty());
    }

    /// The runs that a layout asks for, by their text, and whether each was
    /// on a background darker than its text.
    #[derive(Default)]
    struct Runs(Vec<String>, Vec<bool>);

    impl Glyphs for Runs {
        fn glyph(&mut self, _: &CellText<'_>) -> Option<AtlasGlyph> {
            None
        }

        fn run(&mut self, run: &Run<'_>, _: &mut Vec<RunGlyph>) {
            self.0.push(run.text.to_owned());
            self.1.push(run.dark);
        }
    }

    #[test]
    fn a_row_is_laid_out_in_runs_of_a_face_s_style() {
        // Runs end at a space, an empty cell, and a change of bold or
        // italic, but not of color.
        let styled = snapshot(20, 1, "a->b \x1b[31mc=d\x1b[1mef\x1b[3mg\x1b[0m  h");
        let mut runs = Runs::default();
        cells(&styled, &cell(), 0, 0, None, None, &mut runs);
        assert_eq!(runs.0, ["a->b", "c=d", "ef", "g", "h"]);
        // The cursor's cell is a run of its own.
        let under = snapshot(20, 1, "a->b\x1b[3D");
        let look = CursorLook {
            shape: CursorShape::Block,
            thickness: 1,
        };
        let mut runs = Runs::default();
        cells(&under, &cell(), 0, 0, None, Some(look), &mut runs);
        assert_eq!(runs.0, ["a", "-", ">b"]);
        // In either phase of its blinking, as when the blinking hides it.
        let mut runs = Runs::default();
        cells(&under, &cell(), 0, 0, None, None, &mut runs);
        assert_eq!(runs.0, ["a", "-", ">b"]);
        // A cursor that the program hides breaks no run.
        let hidden = snapshot(20, 1, "a->b\x1b[3D\x1b[?25l");
        let mut runs = Runs::default();
        cells(&hidden, &cell(), 0, 0, None, Some(look), &mut runs);
        assert_eq!(runs.0, ["a->b"]);
        // Light text on the default dark background, then inverse text, dark
        // on light, each a run.
        let inverse = snapshot(20, 1, "ab\x1b[7mcd");
        let mut runs = Runs::default();
        cells(&inverse, &cell(), 0, 0, None, None, &mut runs);
        assert_eq!(
            (runs.0, runs.1),
            (vec!["ab".to_owned(), "cd".to_owned()], vec![true, false])
        );
        // A wide character is one cell of its run.
        let wide = snapshot(20, 1, "a\u{65E5}b");
        let mut runs = Runs::default();
        cells(&wide, &cell(), 0, 0, None, None, &mut runs);
        assert_eq!(runs.0, ["a\u{65E5}b"]);
        // Text that blinks takes runs of its own, shown or not.
        let blinking = snapshot(20, 1, "f\x1b[5mi\x1b[0mx");
        let mut runs = Runs::default();
        cells(&blinking, &cell(), 0, 0, None, None, &mut runs);
        assert_eq!(runs.0, ["f", "i", "x"]);
    }

    /// The grid of `text` on a screen of 8 by 1, its text as `config` and
    /// `shown`, the blinking's phase, have it look.
    fn looked(text: &str, config: TextConfig, shown: bool) -> CellBatch {
        let snapshot = snapshot(8, 1, text);
        let mut asked = Vec::new();
        let look = TextLook {
            config,
            blink_shown: shown,
            ..TextLook::default()
        };
        cells_with(
            &snapshot,
            &cell(),
            (0, 0),
            None,
            None,
            look,
            &mut atlas(&mut asked),
        )
    }

    #[test]
    fn without_ansi_colors_the_text_takes_the_default_colors() {
        let plain = TextConfig {
            ansi_colors: false,
            ..TextConfig::default()
        };
        // Red on blue, a 256-color and an RGB one, and a line erased in green.
        let batch = looked(
            "\x1b[31;44ma\x1b[38;5;200mb\x1b[38;2;1;2;3mc\x1b[42m\x1b[K",
            plain,
            true,
        );
        let colors = Colors::default();
        assert!(
            batch
                .glyphs
                .iter()
                .all(|glyph| glyph.color == color(colors.foreground))
        );
        let background = clear(color(colors.background));
        assert!(
            batch.backgrounds.iter().all(|&bg| bg == background),
            "{:?}",
            batch.backgrounds
        );
        // Inverse text still swaps the colors.
        let batch = looked("\x1b[31;7ma", plain, true);
        assert_eq!(batch.glyphs[0].color, color(colors.background));
        assert_eq!(batch.backgrounds[0], color(colors.foreground));
        // With them, as the program sets them.
        let batch = looked("\x1b[31;44ma", TextConfig::default(), true);
        assert_eq!(batch.glyphs[0].color, color(colors.palette[1]));
        assert_eq!(batch.backgrounds[0], color(colors.palette[4]));
    }

    #[test]
    fn bold_text_of_the_first_eight_colors_can_take_the_bright_ones() {
        let bright = TextConfig {
            bold_is_bright: true,
            ..TextConfig::default()
        };
        let colors = Colors::default();
        // Bold red, red, bold bright green, bold default.
        let text = "\x1b[1;31ma\x1b[0;31mb\x1b[1;92mc\x1b[0;1md";
        let found: Vec<Color> = looked(text, bright, true)
            .glyphs
            .iter()
            .map(|glyph| glyph.color)
            .collect();
        assert_eq!(
            found,
            [
                color(colors.palette[9]),
                color(colors.palette[1]),
                color(colors.palette[10]),
                color(colors.foreground)
            ]
        );
        let plain = looked(text, TextConfig::default(), true);
        assert_eq!(plain.glyphs[0].color, color(colors.palette[1]));
    }

    #[test]
    fn blinking_text_hides_when_the_frame_does_not_show_it() {
        let text = "\x1b[5mab\x1b[0mc\x1b[6md";
        let columns = |batch: CellBatch| {
            batch
                .glyphs
                .iter()
                .map(|glyph| glyph.column)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            columns(looked(text, TextConfig::default(), true)),
            [0, 1, 2, 3]
        );
        assert_eq!(columns(looked(text, TextConfig::default(), false)), [2]);
        // Its background stays.
        let batch = looked("\x1b[5;41ma", TextConfig::default(), false);
        assert_eq!(batch.backgrounds[0], color(Colors::default().palette[1]));
        // Without blinking, it shows steadily.
        let steady = TextConfig {
            blink: false,
            ..TextConfig::default()
        };
        assert_eq!(columns(looked(text, steady, false)), [0, 1, 2, 3]);
        assert!(has_blinking_text(&snapshot(8, 1, text)));
        assert!(!has_blinking_text(&snapshot(8, 1, "abc")));
    }

    #[test]
    fn the_blinking_cells_go_apart_from_the_grid() {
        // Blinking text, underlined, on the second row, and on the fourth,
        // selected, between rows that do not blink.
        let text = "top\r\na\x1b[5;4mbc\x1b[0md\r\nplain\r\n\x1b[5me\x1b[0m";
        let shown = snapshot(8, 5, text);
        let contains = |column: u16, row: u16| (column, row) == (0, 3);
        let selection = Selection {
            colors: SelectionColors {
                background: rgb(0, 0, 255),
                foreground: Some(rgb(1, 2, 3)),
            },
            contains: &contains,
        };
        let (mut first, mut second) = (Vec::new(), Vec::new());
        let look = |blink_shown| TextLook {
            config: TextConfig::default(),
            blink_shown,
            ..TextLook::default()
        };
        let draw = |look, asked: &mut Vec<_>| {
            let glyphs = &mut atlas(asked);
            cells_with(
                &shown,
                &cell(),
                (0, 0),
                Some(&selection),
                None,
                look,
                glyphs,
            )
        };
        let whole = draw(look(true), &mut first);
        let grid = draw(look(false), &mut second);
        let (row, apart) = blinking_cells(
            &shown,
            &cell(),
            Some(&selection),
            TextLook::default(),
            None,
            &mut atlas(&mut first),
        )
        .unwrap();
        // The rows from the first that blinks to the last.
        assert_eq!(row, 1);
        assert_eq!((apart.x, apart.y, apart.columns, apart.rows), (0, 0, 8, 3));
        assert!(apart.cursor.is_none());
        assert_eq!(apart.backgrounds.len(), 8 * 3);
        assert!(apart.backgrounds.iter().all(|shown| shown.alpha == 0.0));
        // Its glyphs and lines, as the whole grid has them, in their rows;
        // the grid keeps the rest.
        let moved = |glyph: &CellGlyph| CellGlyph {
            row: glyph.row + row,
            ..*glyph
        };
        let mut together: Vec<CellGlyph> = grid.glyphs.clone();
        together.extend(apart.glyphs.iter().map(moved));
        let key = |glyph: &CellGlyph| (glyph.row, glyph.column);
        together.sort_by_key(key);
        let mut expected = whole.glyphs.clone();
        expected.sort_by_key(key);
        assert_eq!(together, expected);
        let cells: Vec<(u16, u16)> = apart.glyphs.iter().map(key).collect();
        assert_eq!(cells, [(0, 1), (0, 2), (2, 0)]);
        let selected = whole.glyphs.iter().find(|glyph| key(glyph) == (3, 0));
        assert_eq!(apart.glyphs.last().map(moved).as_ref(), selected);
        assert_eq!(selected.map(|glyph| glyph.color), Some(rgb(1, 2, 3)));
        let lines: Vec<(u16, u16, u16)> = apart
            .lines
            .iter()
            .map(|line| (line.row, line.column, line.columns))
            .collect();
        assert_eq!(lines, [(0, 1, 2)]);
        assert!(grid.lines.is_empty());
        // Nothing without text that blinks, or when it shows steadily.
        let steady = TextLook {
            config: TextConfig {
                blink: false,
                ..TextConfig::default()
            },
            ..TextLook::default()
        };
        let glyphs = &mut atlas(&mut first);
        assert!(blinking_cells(&shown, &cell(), None, steady, None, glyphs).is_none());
        let plain = snapshot(8, 2, "abc");
        let default = TextLook::default();
        assert!(blinking_cells(&plain, &cell(), None, default, None, glyphs).is_none());
        // A composition at the cursor covers the text under it, and its
        // lines, a strikeout's.
        let under = snapshot(8, 1, "\x1b[5;9mabc\x1b[3D");
        let composing = Preedit {
            text: "日".to_owned(),
            cursor: 3,
        };
        let (_, apart) =
            blinking_cells(&under, &cell(), None, default, Some(&composing), glyphs).unwrap();
        let cells: Vec<u16> = apart.glyphs.iter().map(|glyph| glyph.column).collect();
        assert_eq!(cells, [2]);
        let lines: Vec<(u16, u16)> = apart
            .lines
            .iter()
            .map(|line| (line.column, line.columns))
            .collect();
        assert_eq!(lines, [(2, 1)]);
    }

    #[test]
    fn the_cursor_s_cells_go_apart_from_the_grid() {
        // The cursor on `b`, red and underlined.
        let shown = snapshot(6, 2, "a\x1b[41;4mb\x1b[0mc\x1b[2D");
        let (mut first, mut second) = (Vec::new(), Vec::new());
        let block = CursorLook {
            shape: CursorShape::Block,
            thickness: 2,
        };
        let whole = cells(
            &shown,
            &cell(),
            4,
            30,
            None,
            Some(block),
            &mut atlas(&mut first),
        );
        let grid = cells(&shown, &cell(), 4, 30, None, None, &mut atlas(&mut second));
        let (column, row, apart) = cursor_cells(&grid, &shown, block, &cell(), None).unwrap();
        assert_eq!((column, row), (1, 0));
        assert_eq!((apart.x, apart.y, apart.columns, apart.rows), (0, 0, 1, 1));
        assert_eq!((apart.cell_width, apart.cell_height), (8, 16));
        // The block, and its text, as the grid's own cursor has them.
        let own = whole.cursor.unwrap();
        assert_eq!(
            apart.cursor,
            Some(CellCursor {
                column: 0,
                row: 0,
                ..own
            })
        );
        let under = *whole
            .glyphs
            .iter()
            .find(|glyph| (glyph.column, glyph.row) == (1, 0))
            .unwrap();
        assert_eq!(
            apart.glyphs,
            [CellGlyph {
                column: 0,
                row: 0,
                ..under
            }]
        );
        let red = color(Colors::default().palette[1]);
        assert_eq!(under.color, red);
        let lines: Vec<(u16, u16, Color)> = apart
            .lines
            .iter()
            .map(|line| (line.column, line.columns, line.color))
            .collect();
        assert_eq!(lines, [(0, 1, red)]);
        // Over the grid's backgrounds, which it leaves clear; the grid keeps
        // its own colors under the cursor, and draws none.
        assert!(
            apart
                .backgrounds
                .iter()
                .all(|background| background.alpha == 0.0)
        );
        assert!(grid.cursor.is_none());
        let kept = grid.glyphs.iter().find(|glyph| glyph.column == 1).unwrap();
        assert_ne!(kept.color, red);
        // A bar is its shape alone, over the grid's text.
        let bar = CursorLook {
            shape: CursorShape::Bar,
            thickness: 2,
        };
        let (_, _, apart) = cursor_cells(&grid, &shown, bar, &cell(), None).unwrap();
        assert!(apart.glyphs.is_empty() && apart.lines.is_empty());
        assert_eq!(
            apart.cursor.map(|cursor| cursor.shape),
            Some(CursorShape::Bar)
        );
        // A wide character's two cells, and none for a hidden cursor.
        let wide = snapshot(6, 1, "日\x1b[2D");
        let grid = cells(&wide, &cell(), 0, 0, None, None, &mut atlas(&mut first));
        let (_, _, apart) = cursor_cells(&grid, &wide, block, &cell(), None).unwrap();
        assert_eq!(apart.columns, 2);
        let hidden = snapshot(6, 1, "a\x1b[?25l");
        let grid = cells(&hidden, &cell(), 0, 0, None, None, &mut atlas(&mut first));
        assert!(cursor_cells(&grid, &hidden, block, &cell(), None).is_none());
        // In a composition, at its own cursor, over its wide character.
        let typing = snapshot(6, 1, "a");
        let composing = Preedit {
            text: "日本".to_owned(),
            cursor: 3,
        };
        let mut grid = cells(&typing, &cell(), 0, 0, None, None, &mut atlas(&mut first));
        preedit(
            &mut grid,
            &typing,
            &cell(),
            &composing,
            &mut atlas(&mut second),
        );
        let (column, _, apart) =
            cursor_cells(&grid, &typing, block, &cell(), Some(&composing)).unwrap();
        assert_eq!((column, apart.columns), (3, 2));
        assert_eq!(apart.glyphs.len(), 1);
    }

    #[test]
    fn a_space_takes_no_glyph() {
        // Spaces that a program writes, as full-screen programs do, over a
        // background of their own: the font is asked for none of them.
        let snapshot = snapshot(4, 2, "\x1b[44m    \r\n a  ");
        let mut asked = Vec::new();
        let batch = cells(&snapshot, &cell(), 0, 0, None, None, &mut atlas(&mut asked));
        let texts: Vec<&str> = asked.iter().map(|(text, ..)| text.as_str()).collect();
        assert_eq!(texts, ["a"]);
        assert_eq!(batch.glyphs.len(), 1);
        assert_eq!((batch.glyphs[0].column, batch.glyphs[0].row), (1, 1));
        let blue = color(Colors::default().palette[4]);
        assert_eq!(batch.backgrounds, [blue; 8]);
    }

    #[test]
    fn a_glyph_that_the_fonts_lack_leaves_its_background() {
        let snapshot = snapshot(3, 1, "abc");
        let batch = cells(&snapshot, &cell(), 0, 0, None, None, &mut |_: &CellText<
            '_,
        >| None);
        assert!(batch.glyphs.is_empty());
        assert_eq!(batch.backgrounds.len(), 3);
    }

    /// The snapshot of a screen of `cols` by `rows` in the colors of
    /// `theme`, after `text`.
    fn themed(cols: u16, rows: u16, colors: Colors, text: &str) -> Snapshot {
        let mut terminal = Terminal::new(cols, rows, Options::default());
        terminal.set_default_colors(colors);
        Parser::new().advance(&mut terminal, text.as_bytes());
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        snapshot
    }

    #[test]
    fn a_test_pattern_takes_the_theme_s_values() {
        let theme = crate::theme::Theme::builtin();
        for variant in [&theme.light, &theme.dark] {
            let mut pattern = String::new();
            for index in 0..8 {
                pattern.push_str(&format!("\x1b[{}m#\x1b[{}m#", 30 + index, 90 + index));
            }
            pattern.push_str("\x1b[38;5;123m#\x1b[38;2;10;20;30m#\x1b[0m#");
            let snapshot = themed(40, 1, variant.colors.clone(), &pattern);
            let mut asked = Vec::new();
            let batch = cells(&snapshot, &cell(), 0, 0, None, None, &mut atlas(&mut asked));
            let palette = &variant.colors.palette;
            let mut expected: Vec<Color> = (0..8)
                .flat_map(|index| [color(palette[index]), color(palette[index + 8])])
                .collect();
            expected.extend([
                color(palette[123]),
                rgb(10, 20, 30),
                color(variant.colors.foreground),
            ]);
            let found: Vec<Color> = batch.glyphs.iter().map(|glyph| glyph.color).collect();
            assert_eq!(found, expected);
            assert!(
                batch
                    .backgrounds
                    .iter()
                    .all(|&bg| bg == clear(color(variant.colors.background)))
            );
            assert_eq!(background(&snapshot), color(variant.colors.background));
        }
    }

    #[test]
    fn a_program_s_colors_take_the_place_of_the_theme_s() {
        // OSC 4 sets the palette's red, OSC 10 and 11 the default text and
        // background.
        let snapshot = snapshot(
            4,
            1,
            "\x1b]4;1;rgb:11/22/33\x07\x1b]10;rgb:44/55/66\x07\x1b]11;#778899\x07\x1b[31ma\x1b[0mb",
        );
        let mut asked = Vec::new();
        let batch = cells(&snapshot, &cell(), 0, 0, None, None, &mut atlas(&mut asked));
        assert_eq!(batch.glyphs[0].color, rgb(0x11, 0x22, 0x33));
        assert_eq!(batch.glyphs[1].color, rgb(0x44, 0x55, 0x66));
        assert_eq!(batch.backgrounds[3], clear(rgb(0x77, 0x88, 0x99)));
        assert_eq!(background(&snapshot), rgb(0x77, 0x88, 0x99));
    }

    #[test]
    fn a_text_of_too_little_contrast_takes_the_theme_s_least() {
        // A dark gray on the dark background, and white on its own blue.
        let snapshot = snapshot(
            4,
            1,
            "\x1b]11;#1e1e20\x07\x1b]4;8;#3a3a40\x07\x1b[90ma\x1b[0m\x1b[97;44mb",
        );
        let mut asked = Vec::new();
        let draw = |minimum_contrast, asked: &mut Vec<_>| {
            let look = TextLook {
                minimum_contrast,
                ..TextLook::default()
            };
            cells_with(
                &snapshot,
                &cell(),
                (0, 0),
                None,
                None,
                look,
                &mut atlas(asked),
            )
        };
        let dark = rgb(0x1e, 0x1e, 0x20);
        let plain = draw(1.0, &mut asked);
        assert_eq!(plain.glyphs[0].color, rgb(0x3a, 0x3a, 0x40));
        let raised = draw(4.5, &mut asked);
        assert!(theme::contrast(raised.glyphs[0].color, dark) >= 4.5);
        // The text that reads on its own background stays as it is.
        assert_eq!(raised.glyphs[1].color, plain.glyphs[1].color);
    }

    /// The frame's time of 200 by 60 cells of a theme of a low contrast,
    /// at a least contrast of 1 and of 4.5, which `cargo test --release
    /// -- --ignored` measures.
    #[test]
    #[ignore = "a measure, of a release build"]
    fn the_least_contrast_costs_a_frame_little() {
        let line: String = "the quick brown fox jumps over the lazy dog "
            .chars()
            .cycle()
            .take(200)
            .collect();
        let text = format!(
            "\x1b]10;#657b83\x07\x1b]11;#fdf6e3\x07{}",
            vec![line; 60].join("\r\n")
        );
        let snapshot = snapshot(200, 60, &text);
        let mut asked = Vec::new();
        let mut time = |minimum_contrast| {
            let look = TextLook {
                minimum_contrast,
                ..TextLook::default()
            };
            let mut atlas = atlas(&mut asked);
            let mut best = std::time::Duration::MAX;
            for _ in 0..20 {
                let start = std::time::Instant::now();
                std::hint::black_box(cells_with(
                    &snapshot,
                    &cell(),
                    (0, 0),
                    None,
                    None,
                    look,
                    &mut atlas,
                ));
                best = best.min(start.elapsed());
            }
            best
        };
        let (plain, raised) = (time(1.0), time(4.5));
        eprintln!("a frame at 1: {plain:?}; at 4.5: {raised:?}");
        assert!(
            raised < plain + std::time::Duration::from_micros(500),
            "{plain:?} and {raised:?}"
        );
    }

    #[test]
    fn reverse_video_swaps_the_default_colors() {
        // DECSCNM: the default colors swap; a color that a program sets
        // stays.
        let snapshot = snapshot(4, 1, "\x1b[?5ha\x1b[31mb");
        let mut asked = Vec::new();
        let batch = cells(&snapshot, &cell(), 0, 0, None, None, &mut atlas(&mut asked));
        let colors = Colors::default();
        assert_eq!(batch.glyphs[0].color, color(colors.background));
        assert_eq!(batch.backgrounds[0], clear(color(colors.foreground)));
        assert_eq!(batch.glyphs[1].color, color(colors.palette[1]));
        assert_eq!(batch.backgrounds[3], clear(color(colors.foreground)));
        assert_eq!(background(&snapshot), color(colors.foreground));
    }

    #[test]
    fn faint_text_mixes_halfway_and_hidden_text_shows_nothing() {
        let colors = Colors {
            foreground: Rgb {
                r: 200,
                g: 100,
                b: 0,
            },
            background: Rgb { r: 0, g: 0, b: 100 },
            ..Colors::default()
        };
        let snapshot = themed(4, 1, colors, "\x1b[2ma\x1b[0;8mb\x1b[0mc");
        let mut asked = Vec::new();
        let batch = cells(&snapshot, &cell(), 0, 0, None, None, &mut atlas(&mut asked));
        let texts: Vec<&str> = asked.iter().map(|(text, ..)| text.as_str()).collect();
        assert_eq!(texts, ["a", "c"]);
        assert_eq!(batch.glyphs[0].color, Color::from_rgb8(100, 50, 50));
        // The hidden cell keeps its background, the default one, clear.
        assert_eq!(batch.backgrounds[1], clear(Color::from_rgb8(0, 0, 100)));
    }

    /// The cursor that `cells` gives `snapshot` in `shape`, and the color
    /// of each glyph by its column.
    fn cursor_of(
        snapshot: &Snapshot,
        shape: CursorShape,
    ) -> (Option<CellCursor>, Vec<(u16, Color)>) {
        let mut asked = Vec::new();
        let look = CursorLook {
            shape,
            thickness: 3,
        };
        let batch = cells(
            snapshot,
            &cell(),
            0,
            0,
            None,
            Some(look),
            &mut atlas(&mut asked),
        );
        let colors = batch
            .glyphs
            .iter()
            .map(|glyph| (glyph.column, glyph.color))
            .collect();
        (batch.cursor, colors)
    }

    #[test]
    fn the_cursor_takes_its_shape_and_the_theme_s_color() {
        let colors = Colors::default();
        let face = cell().underline_thickness;
        assert_ne!(face, 3);
        // A bar and an underline take the look's thickness, a hollow block
        // the face's.
        for (shape, thickness) in [
            (CursorShape::Block, face),
            (CursorShape::Bar, 3),
            (CursorShape::Underline, 3),
            (CursorShape::Hollow, face),
        ] {
            let (cursor, _) = cursor_of(&snapshot(4, 2, "ab"), shape);
            assert_eq!(
                cursor,
                Some(CellCursor {
                    column: 2,
                    row: 0,
                    columns: 1,
                    shape,
                    thickness,
                    color: color(colors.cursor),
                })
            );
        }
        // OSC 12 sets its color.
        let (cursor, _) = cursor_of(&snapshot(4, 2, "\x1b]12;#ff0000\x07"), CursorShape::Bar);
        assert_eq!(cursor.unwrap().color, rgb(255, 0, 0));
    }

    #[test]
    fn the_text_under_a_block_takes_its_cell_s_background() {
        // The cursor back on `b`, whose background is red.
        let text = "a\x1b[41mb\x1b[0mc\x1b[2D";
        let (cursor, glyphs) = cursor_of(&snapshot(4, 1, text), CursorShape::Block);
        assert_eq!(cursor.map(|cursor| cursor.column), Some(1));
        let colors = Colors::default();
        let (ink, red) = (color(colors.foreground), color(colors.palette[1]));
        assert_eq!(glyphs, [(0, ink), (1, red), (2, ink)]);
        // The other shapes leave the text's colors.
        let (_, glyphs) = cursor_of(&snapshot(4, 1, text), CursorShape::Hollow);
        assert_eq!(glyphs, [(0, ink), (1, ink), (2, ink)]);
    }

    /// Glyphs of a character each, as a shaping lays out a base and its
    /// marks in the base's cell.
    struct Marks;

    impl Glyphs for Marks {
        fn glyph(&mut self, _: &CellText<'_>) -> Option<AtlasGlyph> {
            None
        }

        fn run(&mut self, run: &Run<'_>, out: &mut Vec<RunGlyph>) {
            for cell in 0..run.cells.len() {
                for character in run.cluster(cell).chars() {
                    let region = AtlasRegion {
                        texture: Texture::Coverage,
                        x: character as u32,
                        y: 0,
                        width: 6,
                        height: 10,
                    };
                    out.push(RunGlyph {
                        cell,
                        glyph: AtlasGlyph {
                            region,
                            left: 0,
                            top: 10,
                            smoothed: true,
                        },
                        x: 0,
                        y: 0,
                    });
                }
            }
        }
    }

    #[test]
    fn a_block_gives_each_glyph_of_its_cell_the_cell_s_background() {
        // `e` and two marks under the cursor, between `a` and `c`.
        let snapshot = snapshot(4, 1, "ae\u{301}\u{323}c\x1b[2D");
        let look = CursorLook {
            shape: CursorShape::Block,
            thickness: 1,
        };
        let batch = cells(&snapshot, &cell(), 0, 0, None, Some(look), &mut Marks);
        assert_eq!(batch.cursor.map(|cursor| cursor.column), Some(1));
        let colors = Colors::default();
        let (ink, block) = (color(colors.foreground), color(colors.background));
        let glyphs: Vec<_> = batch
            .glyphs
            .iter()
            .map(|glyph| (glyph.column, glyph.color))
            .collect();
        assert_eq!(
            glyphs,
            [(0, ink), (1, block), (1, block), (1, block), (2, ink)]
        );
    }

    #[test]
    fn only_the_default_background_is_clear() {
        // Inverse text with the default colors takes the default text's
        // color as its background, opaque.
        let snapshot = snapshot(4, 1, "\x1b[7ma\x1b[0mb\x1b[D");
        let mut asked = Vec::new();
        let batch = cells(&snapshot, &cell(), 0, 0, None, None, &mut atlas(&mut asked));
        let colors = Colors::default();
        assert_eq!(batch.backgrounds[0], color(colors.foreground));
        assert_eq!(batch.backgrounds[1], clear(color(colors.background)));
        // A block on `b`'s clear cell gives its text the cell's color,
        // opaque.
        let (_, glyphs) = cursor_of(&snapshot, CursorShape::Block);
        let block = color(colors.background);
        assert!(glyphs.contains(&(1, block)), "{glyphs:?}");
    }

    #[test]
    fn the_cursor_covers_a_wide_character_from_either_cell() {
        // On the character, and on its second cell.
        for text in ["\u{4E2D}\x1b[2D", "\u{4E2D}\x1b[D"] {
            let (cursor, _) = cursor_of(&snapshot(4, 1, text), CursorShape::Block);
            let cursor = cursor.unwrap();
            assert_eq!((cursor.column, cursor.columns), (0, 2), "{text:?}");
        }
    }

    #[test]
    fn a_hidden_cursor_draws_nothing() {
        let (cursor, _) = cursor_of(&snapshot(4, 1, "a\x1b[?25l"), CursorShape::Block);
        assert_eq!(cursor, None);
        // Nor does one that the blinking hides.
        let mut asked = Vec::new();
        let batch = cells(
            &snapshot(4, 1, "a"),
            &cell(),
            0,
            0,
            None,
            None,
            &mut atlas(&mut asked),
        );
        assert_eq!(batch.cursor, None);
    }

    #[test]
    fn decorations_lay_their_lines_and_backgrounds_over_the_grid() {
        use lokyr_vt_core::parser::Parser;
        use lokyr_vt_core::terminal::{Look, Options, Snapshot, Terminal};

        let mut terminal = Terminal::new(8, 2, Options::default());
        Parser::new().advance(&mut terminal, b"abcdefgh\r\nxyz");
        let red = Rgb { r: 255, g: 0, b: 0 };
        let underline = Look {
            underline: page::Underline::Curly,
            ..Look::default()
        };
        let lit = Look {
            background: Some(red),
            ..Look::default()
        };
        // Across both rows, then two cells lit.
        assert!(terminal.decorate(1, (6, 0), (1, 1), underline));
        assert!(terminal.decorate(2, (2, 1), (2, 1), lit));
        let mut shown = Snapshot::default();
        terminal.snapshot(&mut shown);
        let mut asked = Vec::new();
        let mut batch = cells(&shown, &cell(), 0, 0, None, None, &mut atlas(&mut asked));
        let lines = batch.lines.len();
        decorate(&mut batch, &shown, &cell());
        let laid: Vec<(u16, u16, u16, LineStyle)> = batch.lines[lines..]
            .iter()
            .map(|line| (line.row, line.column, line.columns, line.style))
            .collect();
        assert_eq!(
            laid,
            [(0, 6, 2, LineStyle::Curly), (1, 0, 2, LineStyle::Curly)]
        );
        assert!(
            batch.lines[lines..]
                .iter()
                .all(|line| line.color == color(Colors::default().foreground))
        );
        assert_eq!(batch.backgrounds[8 + 2], color(red));
        assert_ne!(batch.backgrounds[8 + 3], color(red));
    }

    #[test]
    fn the_composed_text_shows_at_the_cursor_underlined() {
        // The cursor back on `c`, in a grid of 8 columns.
        let snapshot = snapshot(8, 2, "abcdef\x1b[4D");
        let mut asked = Vec::new();
        let look = CursorLook {
            shape: CursorShape::Bar,
            thickness: 2,
        };
        let mut batch = cells(
            &snapshot,
            &cell(),
            0,
            0,
            None,
            Some(look),
            &mut atlas(&mut asked),
        );
        let composed = Preedit {
            text: "にほ".to_owned(),
            cursor: 3,
        };
        preedit(
            &mut batch,
            &snapshot,
            &cell(),
            &composed,
            &mut atlas(&mut asked),
        );
        let row: Vec<(u16, bool)> = batch
            .glyphs
            .iter()
            .filter(|glyph| glyph.row == 0)
            .map(|glyph| {
                (
                    glyph.column,
                    glyph.region.x == u32::from('に') || glyph.region.x == u32::from('ほ'),
                )
            })
            .collect();
        // `a` and `b`, then the two wide characters over `c` to `f`.
        assert_eq!(row, [(0, false), (1, false), (2, true), (4, true)]);
        let line = batch.lines.last().unwrap();
        assert_eq!((line.column, line.row, line.columns), (2, 0, 4));
        assert_eq!(line.top, cell().underline_top);
        // The input method's cursor, after `に`.
        assert_eq!(batch.cursor.map(|cursor| cursor.column), Some(4));
        let colors = Colors::default();
        assert_eq!(batch.backgrounds[2], clear(color(colors.background)));
    }

    /// The batch of `text` on a grid of `cols` by one row, with the cursor
    /// of `shape`.
    fn with_cursor(cols: u16, text: &str, shape: CursorShape) -> CellBatch {
        let mut asked = Vec::new();
        let look = CursorLook {
            shape,
            thickness: 2,
        };
        cells(
            &snapshot(cols, 1, text),
            &cell(),
            0,
            0,
            None,
            Some(look),
            &mut atlas(&mut asked),
        )
    }

    fn glyph_color(batch: &CellBatch, column: u16) -> Option<Color> {
        batch
            .glyphs
            .iter()
            .find(|glyph| glyph.column == column)
            .map(|glyph| glyph.color)
    }

    #[test]
    fn a_block_on_inverse_text_stays_apart_from_its_cell() {
        // `b` in inverse, whose background is the cursor's color.
        let batch = with_cursor(4, "a\x1b[7mb\x1b[0mc\x1b[2D", CursorShape::Block);
        let colors = Colors::default();
        let (text, background) = (color(colors.foreground), color(colors.background));
        let cursor = batch.cursor.unwrap();
        assert_eq!((cursor.column, batch.backgrounds[1]), (1, text));
        // The block takes the cell's text color, and its text the cell's
        // background.
        assert_eq!(cursor.color, background);
        assert_eq!(glyph_color(&batch, 1), Some(text));
        // On a plain cell, the cursor's own color.
        let batch = with_cursor(4, "ab\x1b[D", CursorShape::Block);
        assert_eq!(batch.cursor.unwrap().color, color(colors.cursor));
        assert_eq!(glyph_color(&batch, 1), Some(background));
    }

    #[test]
    fn every_shape_shows_in_reverse_video() {
        let colors = Colors::default();
        let dark = color(colors.background);
        for shape in [
            CursorShape::Bar,
            CursorShape::Underline,
            CursorShape::Hollow,
        ] {
            // On a character, and past the text.
            for text in ["\x1b[?5hab\x1b[D", "\x1b[?5hab"] {
                let batch = with_cursor(4, text, shape);
                let cursor = batch.cursor.unwrap();
                let under = batch.backgrounds[usize::from(cursor.column)];
                assert_eq!(cursor.color, dark, "{shape:?} {text:?}");
                assert_ne!(cursor.color, under);
            }
        }
    }

    #[test]
    fn a_cell_of_one_color_takes_black_or_white() {
        // Text and background both of the cursor's light gray.
        let gray = "\x1b[38;2;229;229;229;48;2;229;229;229mx\x1b[D";
        let batch = with_cursor(4, gray, CursorShape::Block);
        assert_eq!(batch.cursor.unwrap().color, Color::from_rgb8(0, 0, 0));
    }

    #[test]
    fn a_block_cursor_s_lines_take_its_cell_s_background() {
        let look = CursorLook {
            shape: CursorShape::Block,
            thickness: 2,
        };
        let snapshot = snapshot(8, 1, "\x1b[4;9mabcd\x1b[2D");
        let mut asked = Vec::new();
        let batch = cells(
            &snapshot,
            &cell(),
            0,
            0,
            None,
            Some(look),
            &mut atlas(&mut asked),
        );
        let background = color(Colors::default().background);
        let text = color(Colors::default().foreground);
        // The underline and the strike, each cut around the cursor's cell,
        // `c`, whose piece takes its background.
        let mut pieces: Vec<(u16, u16, bool)> = batch
            .lines
            .iter()
            .map(|line| {
                assert!(line.color == background || line.color == text);
                (line.column, line.columns, line.color == background)
            })
            .collect();
        pieces.sort_unstable();
        assert_eq!(
            pieces,
            [
                (0, 2, false),
                (0, 2, false),
                (2, 1, true),
                (2, 1, true),
                (3, 1, false),
                (3, 1, false)
            ]
        );
        // A bar leaves them whole.
        let look = CursorLook {
            shape: CursorShape::Bar,
            ..look
        };
        let batch = cells(
            &snapshot,
            &cell(),
            0,
            0,
            None,
            Some(look),
            &mut atlas(&mut asked),
        );
        assert!(batch.lines.iter().all(|line| line.columns == 4));
    }

    #[test]
    fn a_block_under_a_composition_leaves_its_text_readable() {
        let look = CursorLook {
            shape: CursorShape::Block,
            thickness: 2,
        };
        let compose = |text: &str, composed: &str, cursor: usize| {
            let snapshot = snapshot(8, 1, text);
            let mut asked = Vec::new();
            let mut batch = cells(
                &snapshot,
                &cell(),
                0,
                0,
                None,
                Some(look),
                &mut atlas(&mut asked),
            );
            let composed = Preedit {
                text: composed.to_owned(),
                cursor,
            };
            preedit(
                &mut batch,
                &snapshot,
                &cell(),
                &composed,
                &mut atlas(&mut asked),
            );
            batch
        };
        let background = color(Colors::default().background);
        // The cursor after `x`, on the `d` that follows the composition.
        let batch = compose("abcdef\x1b[4D", "x", 1);
        let cursor = batch.cursor.unwrap();
        assert_eq!((cursor.column, cursor.columns), (3, 1));
        assert_eq!(glyph_color(&batch, 3), Some(background));
        // On a wide composed character, which it covers whole.
        let batch = compose("ab", "日本", 3);
        let cursor = batch.cursor.unwrap();
        assert_eq!((cursor.column, cursor.columns), (4, 2));
        assert_eq!(glyph_color(&batch, 4), Some(background));
        assert_eq!(
            glyph_color(&batch, 2),
            Some(color(Colors::default().foreground))
        );
    }

    #[test]
    fn a_cursor_after_a_composition_covers_a_wide_character_whole() {
        let look = CursorLook {
            shape: CursorShape::Block,
            thickness: 2,
        };
        let compose = |columns: u16, text: &str, composed: &str, cursor: usize| {
            let snapshot = snapshot(columns, 1, text);
            let mut asked = Vec::new();
            let mut batch = cells(
                &snapshot,
                &cell(),
                0,
                0,
                None,
                Some(look),
                &mut atlas(&mut asked),
            );
            let composed = Preedit {
                text: composed.to_owned(),
                cursor,
            };
            preedit(
                &mut batch,
                &snapshot,
                &cell(),
                &composed,
                &mut atlas(&mut asked),
            );
            let cursor = batch.cursor.unwrap();
            (batch, (cursor.column, cursor.columns))
        };
        let background = color(Colors::default().background);
        // `x` composed on the `b` of `ab日`: the cursor after it, on the
        // first cell of `日`, covers both, and the block's text is the
        // whole character in the background's color.
        let (batch, cursor) = compose(8, "ab\u{65E5}\x1b[3D", "x", 1);
        assert_eq!(cursor, (2, 2));
        assert_eq!(glyph_color(&batch, 2), Some(background));
        // Past a composition that covers the first cell of `日`, its second
        // cell, which no longer shows it, alone.
        let (_, cursor) = compose(8, "\u{65E5}b\x1b[3D", "x", 1);
        assert_eq!(cursor, (1, 1));
        // A composition cut at the right edge, with the cursor at its end or
        // on the character left out: on the first cell of the last wide
        // character shown.
        for at in [3, 9] {
            let (_, cursor) = compose(4, "ab", "日本語", at);
            assert_eq!(cursor, (2, 2), "the cursor at byte {at}");
        }
    }

    #[test]
    fn a_composition_from_a_wide_character_s_second_cell_starts_on_its_first() {
        // `中`, and the cursor on its second cell.
        let snapshot = snapshot(4, 1, "\u{4E2D}\x1b[2G");
        assert_eq!(snapshot.cursor().position.x, 1);
        let mut asked = Vec::new();
        let mut batch = cells(&snapshot, &cell(), 0, 0, None, None, &mut atlas(&mut asked));
        let composed = Preedit {
            text: "x".to_owned(),
            cursor: 1,
        };
        preedit(
            &mut batch,
            &snapshot,
            &cell(),
            &composed,
            &mut atlas(&mut asked),
        );
        let row: Vec<u16> = batch.glyphs.iter().map(|glyph| glyph.column).collect();
        assert_eq!(row, [0]);
        assert_eq!(batch.glyphs[0].region.x, u32::from('x'));
        let line = batch.lines.last().unwrap();
        assert_eq!((line.column, line.columns), (0, 1));
    }

    #[test]
    fn a_composition_without_room_to_the_right_edge_starts_where_it_has() {
        // The cursor in the last column of six, where `日本` takes four.
        let snapshot = snapshot(6, 1, "abcde");
        assert_eq!(snapshot.cursor().position.x, 5);
        let mut asked = Vec::new();
        let mut batch = cells(&snapshot, &cell(), 0, 0, None, None, &mut atlas(&mut asked));
        let composed = Preedit {
            text: "日本".to_owned(),
            cursor: 6,
        };
        preedit(
            &mut batch,
            &snapshot,
            &cell(),
            &composed,
            &mut atlas(&mut asked),
        );
        // Over `cde`, and the cell after them, whole.
        let shown: Vec<(u16, u32)> = batch
            .glyphs
            .iter()
            .map(|glyph| (glyph.column, glyph.region.x))
            .collect();
        let expected = [('a', 0), ('b', 1), ('日', 2), ('本', 4)];
        assert_eq!(shown, expected.map(|(text, at)| (at, u32::from(text))));
        let line = batch.lines.last().unwrap();
        assert_eq!((line.column, line.columns), (2, 4));
        assert_eq!(composed_span(&snapshot, &composed, 6), Some((0, 2..6)));
        // A narrow one stays at the cursor.
        let narrow = Preedit {
            text: "k".to_owned(),
            cursor: 1,
        };
        assert_eq!(composed_span(&snapshot, &narrow, 6), Some((0, 5..6)));
    }

    #[test]
    fn a_composition_moved_left_covers_a_wide_character_whole() {
        // `日本` from the second column of six, and the cursor after them,
        // where a composition of `日本` would start on `日`'s second cell.
        let snapshot = snapshot(6, 1, "a日本");
        assert_eq!(snapshot.cursor().position.x, 5);
        let mut asked = Vec::new();
        let mut batch = cells(&snapshot, &cell(), 0, 0, None, None, &mut atlas(&mut asked));
        let composed = Preedit {
            text: "日本".to_owned(),
            cursor: 6,
        };
        preedit(
            &mut batch,
            &snapshot,
            &cell(),
            &composed,
            &mut atlas(&mut asked),
        );
        // From `日`'s first cell, which it covers; no glyph of the grid's
        // under it.
        let shown: Vec<(u16, u32)> = batch
            .glyphs
            .iter()
            .map(|glyph| (glyph.column, glyph.region.x))
            .collect();
        let expected = [('a', 0), ('日', 1), ('本', 3)];
        assert_eq!(shown, expected.map(|(text, at)| (at, u32::from(text))));
        assert_eq!(composition_start(&snapshot, &composed, 6), Some((0, 1)));
        assert_eq!(composed_span(&snapshot, &composed, 6), Some((0, 1..5)));
    }

    #[test]
    fn composed_text_wider_than_the_row_loses_its_end() {
        let snapshot = snapshot(4, 1, "ab");
        let mut asked = Vec::new();
        let mut batch = cells(&snapshot, &cell(), 0, 0, None, None, &mut atlas(&mut asked));
        let composed = Preedit {
            text: "日本語".to_owned(),
            cursor: 9,
        };
        preedit(
            &mut batch,
            &snapshot,
            &cell(),
            &composed,
            &mut atlas(&mut asked),
        );
        // From the row's first cell: `日本`, and `語` left out.
        let line = batch.lines.last().unwrap();
        assert_eq!((line.column, line.columns), (0, 4));
        let shown: Vec<u16> = batch.glyphs.iter().map(|glyph| glyph.column).collect();
        assert_eq!(shown, [0, 2]);
    }

    /// A cell of 8 by 16 pixels whose underline, strikeout and their
    /// thicknesses differ, as a face's do.
    fn lined() -> CellMetrics {
        CellMetrics::new(
            &FaceMetrics {
                ascent: 12.07,
                descent: 3.07,
                advance: 7.83,
                underline_position: -1.5,
                underline_thickness: 1.0,
                strikeout_position: 4.0,
                strikeout_thickness: 2.0,
                ..FaceMetrics::default()
            },
            &FontConfig::default(),
        )
    }

    /// The lines of `batch`: each one's column, columns, style and top.
    fn lines_of(batch: &CellBatch) -> Vec<(u16, u16, u16, LineStyle, u32)> {
        batch
            .lines
            .iter()
            .map(|line| (line.row, line.column, line.columns, line.style, line.top))
            .collect()
    }

    #[test]
    fn the_styles_decorate_their_cells_with_lines() {
        use LineStyle::{Curly, Dashed, Dotted, Double, Single};
        // Underlines of each kind, one of its own color, then a strikeout
        // and an overline, after a space.
        let text = "\x1b[4mab\x1b[21mc\x1b[4:3md\x1b[4:4me\x1b[4:5mf\x1b[58;5;1mg\x1b[0m \x1b[9mh\x1b[29;53mi";
        let styled = snapshot(20, 2, text);
        let batch = cells(
            &styled,
            &lined(),
            0,
            0,
            None,
            None,
            &mut atlas(&mut Vec::new()),
        );
        let (under, strike) = (lined().underline_top, lined().strikeout_top);
        assert_ne!(under, strike);
        assert_eq!(
            lines_of(&batch),
            [
                (0, 0, 2, Single, under),
                (0, 2, 1, Double, under),
                (0, 3, 1, Curly, under),
                (0, 4, 1, Dotted, under),
                (0, 5, 1, Dashed, under),
                (0, 6, 1, Dashed, under),
                (0, 8, 1, Single, strike),
                (0, 9, 1, Single, 0),
            ]
        );
        let colors = Colors::default();
        let text_color = color(colors.foreground);
        assert_eq!(batch.lines[4].color, text_color);
        assert_eq!(batch.lines[5].color, color(colors.palette[1]));
        assert_eq!(
            (
                batch.lines[0].thickness,
                batch.lines[6].thickness,
                batch.lines[7].thickness
            ),
            (1, 2, 1)
        );
        // Spaces and a wide character's two cells take one line; the lines
        // of another row are its own, and a cell without one parts two.
        let wrapped = snapshot(10, 3, "\x1b[4ma b\r\n日本\x1b[0mx\x1b[4my");
        let batch = cells(
            &wrapped,
            &lined(),
            0,
            0,
            None,
            None,
            &mut atlas(&mut Vec::new()),
        );
        assert_eq!(
            lines_of(&batch),
            [
                (0, 0, 3, Single, under),
                (1, 0, 4, Single, under),
                (1, 5, 1, Single, under)
            ]
        );
    }

    #[test]
    fn hidden_text_has_no_lines_and_faint_text_faint_ones() {
        let snapshot = snapshot(10, 2, "\x1b[8;4mx\x1b[0m\x1b[2;4my\x1b[0m\x1b[5;9mz");
        let colors = Colors::default();
        let (foreground, background) = (color(colors.foreground), color(colors.background));
        let mut look = TextLook {
            blink_shown: false,
            ..TextLook::default()
        };
        let batch = cells_with(
            &snapshot,
            &cell(),
            (0, 0),
            None,
            None,
            look,
            &mut atlas(&mut Vec::new()),
        );
        // The hidden `x` and the blinking `z`, in its hidden phase, have
        // none; the faint `y`'s takes its faint color.
        assert_eq!(
            lines_of(&batch),
            [(0, 1, 1, LineStyle::Single, cell().underline_top)]
        );
        assert_eq!(
            batch.lines[0].color,
            mix(foreground, background.with_alpha(0.0))
        );
        // Shown, the blinking `z` has its strikeout.
        look.blink_shown = true;
        let batch = cells_with(
            &snapshot,
            &cell(),
            (0, 0),
            None,
            None,
            look,
            &mut atlas(&mut Vec::new()),
        );
        assert_eq!(batch.lines.len(), 2);
    }

    #[test]
    fn an_underline_s_own_color_needs_the_programs_colors() {
        let snapshot = snapshot(10, 2, "\x1b[4;58;2;10;20;30mx");
        let batch = cells(
            &snapshot,
            &cell(),
            0,
            0,
            None,
            None,
            &mut atlas(&mut Vec::new()),
        );
        assert_eq!(batch.lines[0].color, rgb(10, 20, 30));
        let mut look = TextLook::default();
        look.config.ansi_colors = false;
        let batch = cells_with(
            &snapshot,
            &cell(),
            (0, 0),
            None,
            None,
            look,
            &mut atlas(&mut Vec::new()),
        );
        assert_eq!(batch.lines[0].color, color(Colors::default().foreground));
    }

    #[test]
    fn selected_text_s_lines_take_its_color() {
        let snapshot = snapshot(10, 2, "\x1b[4mabcd\x1b[58;5;2mef");
        let colors = SelectionColors {
            background: rgb(1, 2, 3),
            foreground: Some(rgb(4, 5, 6)),
        };
        let contains = |column: u16, row: u16| row == 0 && (1..5).contains(&column);
        let selection = Selection {
            colors,
            contains: &contains,
        };
        let batch = cells(
            &snapshot,
            &cell(),
            0,
            0,
            Some(&selection),
            None,
            &mut atlas(&mut Vec::new()),
        );
        let text = color(Colors::default().foreground);
        let colored: Vec<(u16, u16, Color)> = batch
            .lines
            .iter()
            .map(|line| (line.column, line.columns, line.color))
            .collect();
        // The text's color, the selection's, and an underline's own, which
        // stays its own.
        let green = color(Colors::default().palette[2]);
        assert_eq!(colored, [(0, 1, text), (1, 3, rgb(4, 5, 6)), (4, 2, green)]);
    }

    #[test]
    fn a_composition_cuts_the_lines_under_it() {
        let snapshot = snapshot(10, 2, "\x1b[4mabcdefgh\x1b[0m\x1b[1;3H");
        let mut batch = cells(
            &snapshot,
            &cell(),
            0,
            0,
            None,
            None,
            &mut atlas(&mut Vec::new()),
        );
        let composed = Preedit {
            text: "xy".to_owned(),
            cursor: 2,
        };
        preedit(
            &mut batch,
            &snapshot,
            &cell(),
            &composed,
            &mut atlas(&mut Vec::new()),
        );
        let under = cell().underline_top;
        // The text's line before and after it, and the composition's own.
        assert_eq!(
            lines_of(&batch),
            [
                (0, 0, 2, LineStyle::Single, under),
                (0, 4, 4, LineStyle::Single, under),
                (0, 2, 2, LineStyle::Single, under),
            ]
        );
    }

    #[test]
    fn selected_text_that_would_not_read_takes_a_color_that_reads() {
        // Inverse text, of the default background's color, and plain text.
        let snapshot = snapshot(6, 2, "\x1b[7mab\x1b[0mcd");
        let colors = Colors::default();
        let (text, background) = (color(colors.foreground), color(colors.background));
        // A selection that keeps the text's colors, on a background near
        // the default one, as a dark theme's.
        let near = Color::from_rgb8(
            colors.background.r.saturating_add(40),
            colors.background.g.saturating_add(40),
            colors.background.b.saturating_add(60),
        );
        let selection = Selection {
            colors: SelectionColors {
                background: near,
                foreground: None,
            },
            contains: &|_, row| row == 0,
        };
        let batch = cells(
            &snapshot,
            &cell(),
            0,
            0,
            Some(&selection),
            None,
            &mut atlas(&mut Vec::new()),
        );
        assert!(contrast(background, near) < SELECTED_CONTRAST);
        // The inverse text takes the default text's color, which reads;
        // the plain text keeps its own.
        let colored: Vec<(u16, Color)> = batch
            .glyphs
            .iter()
            .map(|glyph| (glyph.column, glyph.color))
            .collect();
        assert_eq!(colored, [(0, text), (1, text), (2, text), (3, text)]);
        assert!(contrast(text, near) >= SELECTED_CONTRAST);
        // On a light selection, the default background reads better.
        assert_eq!(
            readable(
                Color::from_rgb8(250, 250, 250),
                Color::from_rgb8(240, 240, 240),
                (Color::from_rgb8(255, 255, 255), Color::from_rgb8(0, 0, 0))
            ),
            Color::from_rgb8(0, 0, 0)
        );
        assert_eq!(
            readable(
                Color::from_rgb8(0, 0, 0),
                Color::from_rgb8(240, 240, 240),
                (text, background)
            ),
            Color::from_rgb8(0, 0, 0)
        );
    }

    #[test]
    fn the_selection_takes_its_colors() {
        let snapshot = snapshot(6, 2, "\x1b[41mab\x1b[0mcd\r\nef");
        let colors = SelectionColors {
            background: rgb(1, 2, 3),
            foreground: Some(rgb(4, 5, 6)),
        };
        // The second and third cells of the first row, and the whole second
        // row, past its text too.
        let contains = |column: u16, row: u16| (row == 0 && (1..3).contains(&column)) || row == 1;
        let selection = Selection {
            colors,
            contains: &contains,
        };
        let mut asked = Vec::new();
        let batch = cells(
            &snapshot,
            &cell(),
            0,
            0,
            Some(&selection),
            None,
            &mut atlas(&mut asked),
        );
        let red = color(Colors::default().palette[1]);
        assert_eq!(
            batch.backgrounds[..4],
            [
                red,
                rgb(1, 2, 3),
                rgb(1, 2, 3),
                clear(color(Colors::default().background))
            ]
        );
        assert!(batch.backgrounds[6..].iter().all(|&bg| bg == rgb(1, 2, 3)));
        let colored: Vec<(u16, u16, Color)> = batch
            .glyphs
            .iter()
            .map(|glyph| (glyph.column, glyph.row, glyph.color))
            .collect();
        let text = color(Colors::default().foreground);
        assert_eq!(
            colored,
            [
                (0, 0, text),
                (1, 0, rgb(4, 5, 6)),
                (2, 0, rgb(4, 5, 6)),
                (3, 0, text),
                (0, 1, rgb(4, 5, 6)),
                (1, 1, rgb(4, 5, 6)),
            ]
        );
    }
}
