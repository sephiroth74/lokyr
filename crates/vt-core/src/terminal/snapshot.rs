//! Snapshots: what a renderer draws, copied from the terminal under the
//! lock, so that it can draw without the lock.

use super::folds::{FoldMark, ShownRow, ViewMap};
use super::{
    BlockMark, Colors, Cursor, CursorStyle, Decorated, GutterMark, Highlighted, InputModes,
    KeyboardFlags, Origin, Selected, Terminal, ViewRow,
};
use crate::page::{Cell, Content, ListRow, Page, RowFlag, Style, StyleId};

/// The view as a renderer draws it: its rows, the screen's or, when the
/// view is scrolled up, those of the scrollback above it, the cursor and
/// the colors.
///
/// A renderer keeps one, and brings it up to date with
/// [`Terminal::snapshot`] before each frame: only the rows that changed
/// since, or came into the view, are copied, and the others stay, moved as
/// the screen scrolled or the view moved. A renderer that falls behind gets
/// the latest state, never the ones in between.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    /// Where the rows were at the last update.
    origin: Option<Origin>,
    cols: u16,
    lines: Vec<Line>,
    /// Which rows the last update copied.
    changed: Vec<bool>,
    /// How many rows the last update moved its lines up by; down when
    /// negative.
    scrolled: i32,
    /// How many rows the view is above the screen's top.
    offset: usize,
    /// How many rows the scrollback holds, above the screen.
    scrollback: usize,
    /// Whether the last update found other colors, or reverse video.
    colors_changed: bool,
    cursor: SnapshotCursor,
    reverse_video: bool,
    smooth_scroll: bool,
    /// Whether the alternate screen is the one shown.
    alternate: bool,
    /// What the program asked of the keys and the mouse.
    input_modes: InputModes,
    keyboard_flags: KeyboardFlags,
    colors: Colors,
    /// The selection, as the view shows it.
    selection: Option<Selected>,
    /// The decorations' spans that the view shows.
    decorations: Vec<Decorated>,
    /// The gutter's marks that the view shows.
    gutter: Vec<GutterMark>,
    /// The commands' blocks that the view shows.
    blocks: Vec<BlockMark>,
    /// The spans of the searches' matches that the view shows.
    highlights: Vec<Highlighted>,
    /// Whether a search shows its matches, or its current one, wherever
    /// they are.
    matches_shown: bool,
    /// The images' placements that the view shows, the lowest first.
    images: Vec<super::Placed>,
    /// The prompt rows of the block whose rows the view's top shows,
    /// pinned over the view's first rows, as a snapshot of their own.
    sticky: Option<Box<Snapshot>>,
    /// The command of the pinned rows, the number of its prompt's row and
    /// of the first pinned row, and the epoch of the rows' numbers that they
    /// were copied of, with how many.
    sticky_of: Option<(u64, i64, i64, u64, u16)>,
    /// The view's rows, of folds: each the list's row or a fold's
    /// placeholder; none of no fold, as the view's rows are the list's from
    /// its first.
    view: Vec<ViewRow>,
    /// The rows that the folds take out of the list, all of them and those
    /// above the view's first row.
    folded: (usize, usize),
    /// Whether the lines end with the row past the view's last, as the view
    /// is scrolled a share of a row past its first.
    below: bool,
    /// The share of a row that the view is scrolled past its first row.
    fraction: f32,
}

/// The cursor, as a renderer draws it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SnapshotCursor {
    /// Where it is.
    pub position: Cursor,
    /// Whether it shows (DECTCEM).
    pub visible: bool,
    /// The shape that the program asked for, if it asked.
    pub style: Option<CursorStyle>,
    /// Whether the program asked it to blink (mode 12).
    pub blink: bool,
}

/// A row of the screen, with what its cells need from the page's tables.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Line {
    cells: Vec<Cell>,
    /// The styles that the cells use, sorted by id.
    styles: Vec<(StyleId, Style)>,
    /// The codepoints after the first of each cluster of several, one
    /// cluster after the other.
    codepoints: Vec<char>,
    /// The clusters of several codepoints: the column, where the cluster's
    /// codepoints start in `codepoints`, and how many there are.
    clusters: Vec<(u16, u32, u32)>,
    soft_wrapped: bool,
    prompt_start: bool,
}

impl Snapshot {
    /// The screen's columns.
    pub fn cols(&self) -> u16 {
        self.cols
    }

    /// The view's rows, as many as the screen's, without the row past its
    /// last that [`Snapshot::below`] gives.
    pub fn rows(&self) -> u16 {
        // The screen has at most `u16::MAX` rows.
        (self.lines.len() - usize::from(self.below)) as u16
    }

    /// The rows that a frame draws: the view's, then the row past its last
    /// while it has one, [`Snapshot::below`].
    pub fn drawn_rows(&self) -> u16 {
        // The screen has at most `u16::MAX` rows, and the row below one
        // past them at most.
        u16::try_from(self.lines.len()).unwrap_or(u16::MAX)
    }

    /// Row `y` of the view, or, at [`Snapshot::rows`], the row past its
    /// last while it has one.
    pub fn line(&self, y: u16) -> &Line {
        &self.lines[usize::from(y)]
    }

    /// The share of a row, from 0 to under 1, that the view is scrolled
    /// past its first row, as [`Terminal::viewport_fraction`] told it: the
    /// view shows its rows a share of a row up, and the row past its last,
    /// [`Snapshot::below`], at its bottom.
    pub fn fraction(&self) -> f32 {
        self.fraction
    }

    /// The row past the view's last, which the view shows at its bottom
    /// while it is scrolled a share of a row past its first: its line, at
    /// the view's row [`Snapshot::rows`], whose cursor, selection,
    /// decorations, gutter's marks, blocks, matches and images the snapshot
    /// tells as those of the view's rows, at that row.
    pub fn below(&self) -> Option<&Line> {
        self.below.then(|| &self.lines[self.lines.len() - 1])
    }

    /// How many rows the view is above the screen's top, as
    /// [`Terminal::viewport_offset`] tells: 0 when it shows the screen.
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// How many rows the active screen's scrollback held at the last
    /// update, above the screen: none on the alternate screen.
    pub fn scrollback(&self) -> usize {
        self.scrollback
    }

    /// The selection, as the view shows it, if there is one.
    pub fn selection(&self) -> Option<Selected> {
        self.selection
    }

    /// The spans of the decorations that the view shows, as the last
    /// update found them, row by row.
    pub fn decorations(&self) -> &[Decorated] {
        &self.decorations
    }

    /// The images' placements that the view shows, as the last update
    /// found them, the lowest first: those under the text before it.
    pub fn images(&self) -> &[super::Placed] {
        &self.images
    }

    /// The gutter's marks that the view shows, as the last update found
    /// them, by their rows.
    pub fn gutter(&self) -> &[GutterMark] {
        &self.gutter
    }

    /// The blocks of the commands that the view shows, oldest first, as the
    /// last update found them, of their rows and states.
    pub fn blocks(&self) -> &[BlockMark] {
        &self.blocks
    }

    /// The view's rows, of folds, as the last update found them: each the
    /// list's row, by its index, or a fold's placeholder; none of no fold,
    /// as the view's rows are then the list's from the view's first.
    pub fn view(&self) -> &[ViewRow] {
        &self.view
    }

    /// The row of the view `y`: the list's row, by its index, or a fold's
    /// placeholder, as the last update found it.
    pub fn view_row(&self, y: u16) -> ViewRow {
        match self.view.get(usize::from(y)) {
            Some(&row) => row,
            None => ViewRow::Row(
                (self.scrollback - self.offset.min(self.scrollback)).saturating_add(usize::from(y)),
            ),
        }
    }

    /// The rows that the folds take out of the list, all of them and those
    /// above the view's first row, as the last update found them: the view
    /// scrolls through the list's rows less them, each fold's placeholder
    /// one.
    pub fn folded(&self) -> (usize, usize) {
        self.folded
    }

    /// The prompt rows of the block whose rows the view's top shows, as
    /// the last update found them, while the embedder asks for them: a
    /// snapshot of those rows alone, of the view's colors, which a renderer
    /// draws over the view's first rows; none while the prompt's first row
    /// shows, on the alternate screen, or of no command.
    pub fn sticky(&self) -> Option<&Snapshot> {
        self.sticky.as_deref()
    }

    /// The command of the pinned rows and the number of its prompt's row,
    /// to which a click on them scrolls.
    pub fn sticky_prompt(&self) -> Option<(u64, i64)> {
        self.sticky.as_ref()?;
        self.sticky_of
            .map(|(command, prompt, ..)| (command, prompt))
    }

    /// The map of the view as the last update found it, none before the
    /// first.
    fn map(&self) -> Option<ViewMap<'_>> {
        let origin = self.origin?;
        let top = self.scrollback - self.offset.min(self.scrollback);
        // Fewer rows than an `i64` holds.
        let first = origin.top.saturating_sub(top as i64);
        Some(ViewMap::new(first, top, self.lines.len(), &self.view))
    }

    /// The row of the view `y`, by the numbers of the rows that it shows,
    /// as the last update found it: the list's row, or a fold's
    /// placeholder; none past the list's last or the view's, or before the
    /// first update.
    pub fn shown_row(&self, y: u16) -> Option<ShownRow> {
        self.map()?.at(y)
    }

    /// The folds that the view shows, from its top, by their rows'
    /// numbers, as the last update found them.
    pub fn shown_folds(&self) -> Vec<FoldMark> {
        self.map().map_or_else(Vec::new, |map| map.folds())
    }

    /// The row of the view that shows the row numbered `row`, as the last
    /// update found it: its own, or its fold's placeholder; none of a row
    /// that the view does not show.
    pub fn view_y(&self, row: i64) -> Option<u16> {
        self.map()?.showing(row)
    }

    /// The row of the screen that the view's row `y` shows, as the last
    /// update found it, a fold's placeholder's first row's; none of a row
    /// above the screen, past the list's last or before the first update.
    pub fn screen_row(&self, y: u16) -> Option<u16> {
        let origin = self.origin?;
        // The screen's first row, `offset` of the list's rows below the
        // view's first.
        let screen = origin.top.saturating_add(self.offset as i64);
        let row = self.shown_row(y)?.row();
        u16::try_from(row.checked_sub(screen)?)
            .ok()
            .filter(|&row| usize::from(row) < self.lines.len())
    }

    /// The spans of the searches' matches that the view shows, as the last
    /// update found them, each search's, its current match's last.
    pub fn highlights(&self) -> &[Highlighted] {
        &self.highlights
    }

    /// Whether a search showed its matches, or its current one, at the last
    /// update, wherever they are, as [`Terminal::match_marks`] then marks
    /// them.
    pub fn matches_shown(&self) -> bool {
        self.matches_shown
    }

    /// Whether the last update copied row `y`, which the renderer then has
    /// to draw again.
    pub fn changed(&self, y: u16) -> bool {
        self.changed[usize::from(y)]
    }

    /// How many rows the last update moved the lines up by, as the screen
    /// scrolled or the view moved down, and down by, as a negative number,
    /// as the view moved up: the renderer can move what it drew of them
    /// too. On the alternate screen, a scroll region that leaves fewer rows
    /// out than it holds scrolls as the whole screen does, and counts so:
    /// the rows outside it are marked changed, and drawn again.
    pub fn scrolled(&self) -> i32 {
        self.scrolled
    }

    /// Where the rows were at the last update, the drawn frame's: their
    /// epoch, and the number of the view's first row, which the lines
    /// start at; `None` before the first.
    pub fn origin(&self) -> Option<Origin> {
        self.origin
    }

    /// The cursor, in the view: below the screen's row, by the view's
    /// offset, and hidden when the view is above its row.
    pub fn cursor(&self) -> SnapshotCursor {
        self.cursor
    }

    /// Whether the screen shows in reverse video (DECSCNM).
    pub fn reverse_video(&self) -> bool {
        self.reverse_video
    }

    /// Whether the program asked for smooth scrolling (DECSCLM).
    pub fn smooth_scroll(&self) -> bool {
        self.smooth_scroll
    }

    /// Whether the alternate screen is the one shown, as a program such as
    /// vim shows its own.
    pub fn alternate_screen(&self) -> bool {
        self.alternate
    }

    /// The modes that the program set of what the keys, the mouse, the
    /// focus and a paste send.
    pub fn input_modes(&self) -> InputModes {
        self.input_modes
    }

    /// The kitty keyboard protocol's enhancements that the program asked
    /// for.
    pub fn keyboard_flags(&self) -> KeyboardFlags {
        self.keyboard_flags
    }

    /// The colors to draw with.
    pub fn colors(&self) -> &Colors {
        &self.colors
    }

    /// Whether the last update found the colors or reverse video changed
    /// since the update before: the renderer then draws every row again,
    /// those that the update did not copy too.
    pub fn colors_changed(&self) -> bool {
        self.colors_changed
    }
}

impl Line {
    /// The cells.
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    /// The styles that the line's cells use, each once.
    pub fn styles(&self) -> impl Iterator<Item = &Style> + '_ {
        self.styles.iter().map(|(_, style)| style)
    }

    /// The style that `id`, the style of one of the cells, stands for.
    pub fn style(&self, id: StyleId) -> Style {
        match self.styles.binary_search_by_key(&id, |&(id, _)| id) {
            Ok(at) => self.styles[at].1,
            Err(_) => Style::DEFAULT,
        }
    }

    /// The codepoints after the first of the cluster at column `x`: none
    /// for a cluster of one.
    pub fn grapheme(&self, x: u16) -> &[char] {
        match self.clusters.binary_search_by_key(&x, |&(x, ..)| x) {
            Ok(at) => {
                let (_, start, len) = self.clusters[at];
                &self.codepoints[start as usize..(start + len) as usize]
            }
            Err(_) => &[],
        }
    }

    /// Whether the row's text goes on to the next row, as it wrapped there.
    pub fn soft_wrapped(&self) -> bool {
        self.soft_wrapped
    }

    /// Whether a prompt starts in the row, as shell integration marks it
    /// (OSC 133), for a renderer to show.
    pub fn prompt_start(&self) -> bool {
        self.prompt_start
    }

    /// Makes the line empty, of `cols` cells, as a fold's placeholder's.
    fn clear(&mut self, cols: u16) {
        self.cells.clear();
        self.cells.resize(usize::from(cols), Cell::EMPTY);
        self.styles.clear();
        self.codepoints.clear();
        self.clusters.clear();
        self.soft_wrapped = false;
        self.prompt_start = false;
    }

    /// Makes the line a copy of row `row` of `page`.
    fn copy(&mut self, page: &Page, row: u16) {
        self.fill(page, row, page.cells(row), |x| page.grapheme(row, x));
    }

    /// Makes the line a copy of `row`, of a warm page or of a cold one,
    /// which keeps its graphemes' tables without its cells.
    fn copy_list_row(&mut self, row: ListRow<'_>) {
        let (page, cells) = row.parts();
        let y = row.row();
        self.fill(page, y, cells, |x| page.grapheme_of(y, x));
    }

    /// Makes the line row `row` of `page`, with `cells`, and empty cells
    /// after them to the page's width, and the codepoints that `grapheme`
    /// gives a cluster's column.
    fn fill<'p>(
        &mut self,
        page: &'p Page,
        row: u16,
        cells: &[Cell],
        grapheme: impl Fn(u16) -> &'p [char],
    ) {
        let metadata = page.row(row);
        self.cells.clear();
        self.cells.extend_from_slice(cells);
        self.cells.resize(usize::from(page.cols()), Cell::EMPTY);
        self.styles.clear();
        if metadata.has(RowFlag::Styles) {
            // The ids first, sorted and once each, then their styles: the
            // line's vector, kept from update to update, needs no other.
            let ids = self.cells.iter().map(|cell| cell.style());
            self.styles.extend(
                ids.filter(|&id| id != StyleId::DEFAULT)
                    .map(|id| (id, Style::DEFAULT)),
            );
            self.styles.sort_unstable_by_key(|&(id, _)| id);
            self.styles.dedup_by_key(|&mut (id, _)| id);
            for (id, style) in &mut self.styles {
                *style = *page.styles().get(*id);
            }
        }
        self.codepoints.clear();
        self.clusters.clear();
        if metadata.has(RowFlag::Graphemes) {
            for (x, cell) in self.cells.iter().enumerate() {
                // A row has `u16::MAX` cells at most.
                let x = x as u16;
                if let Content::Grapheme(_) = cell.content() {
                    let rest = grapheme(x);
                    // A row has fewer than 2^32 codepoints in its clusters.
                    let start = self.codepoints.len() as u32;
                    self.codepoints.extend_from_slice(rest);
                    self.clusters.push((x, start, rest.len() as u32));
                }
            }
        }
        self.soft_wrapped = metadata.has(RowFlag::SoftWrapped);
        self.prompt_start = metadata.has(RowFlag::PromptStart);
    }
}

/// The most rows of a block's prompt that a snapshot pins at the view's
/// top, as [`Terminal::set_sticky_prompts`] asks.
pub const MOST_STICKY_ROWS: u16 = 3;

impl Terminal {
    /// Whether each snapshot pins, over the view's first rows, the prompt
    /// rows of the block whose rows the view's top shows, the prompt's and
    /// the command's line, as a sticky header of the block, from the
    /// moment the prompt's first row leaves the view's top, on the primary
    /// screen; the block's end pushes them up, so that they cover no row of
    /// the next block.
    pub fn set_sticky_prompts(&mut self, on: bool) {
        self.sticky_prompts = on;
    }

    /// Brings `snapshot` up to date with the view, and returns how many
    /// rows it copied: those that changed since the last update, as the
    /// rows mark it, those of the screen and those that left it for the
    /// scrollback once changed, and those that scrolling, of the screen or
    /// of the view, brought in. The rows that it copies are marked as
    /// drawn, so that the next update skips them until they change again. A
    /// snapshot of another screen, or of rows laid out again by a resize,
    /// copies every row.
    pub fn snapshot(&mut self, snapshot: &mut Snapshot) -> usize {
        if let Some(view) = self.settle_folded() {
            return self.snapshot_folded(snapshot, view);
        }
        let offset = self.settle_viewport();
        let in_view = usize::from(self.cursor().y) + offset;
        let top = self.screen().rows.scrollback_rows() - offset;
        // The row past the view's last, as the view is scrolled a share of
        // a row past its first: never at the screen, which has none below.
        let fraction = self.viewport_fraction();
        let below = fraction > 0.0 && offset > 0;
        snapshot.fraction = if below { fraction } else { 0.0 };
        snapshot.below = below;
        let shown = usize::from(self.rows) + usize::from(below);
        let map = ViewMap::new(self.row_number(0), top, shown, &[]);
        self.snapshot_state(snapshot, in_view, &map);
        let origin = self.origin();
        // A view of folds before, whose lines are other rows: every line
        // is copied anew.
        if !snapshot.view.is_empty() {
            snapshot.view.clear();
            snapshot.folded = (0, 0);
            snapshot.origin = None;
        }
        let rows = &mut self.screen_mut().rows;
        let (cols, height) = (rows.cols(), rows.rows());
        let lines = usize::from(height) + usize::from(below);
        let scrollback = rows.scrollback_rows();
        // How far the view's first row moved down since the last update,
        // as the lines that the snapshot has move up; up when negative.
        let moved = match snapshot.origin {
            Some(before)
                if before.epoch == origin.epoch
                    && snapshot.cols == cols
                    && snapshot.lines.len() == lines =>
            {
                origin.top.saturating_sub(before.top)
            }
            _ => i64::try_from(lines).unwrap_or(i64::MAX),
        };
        snapshot.origin = Some(origin);
        snapshot.offset = offset;
        snapshot.scrollback = scrollback;
        snapshot.cols = cols;
        snapshot.lines.resize_with(lines, Line::default);
        snapshot.changed.clear();
        snapshot.changed.resize(lines, false);
        // At most the view's rows.
        let shift = moved.unsigned_abs().min(lines as u64) as usize;
        let down = moved < 0;
        if down {
            snapshot.lines.rotate_right(shift);
        } else {
            snapshot.lines.rotate_left(shift);
        }
        // The view has at most `u16::MAX` rows.
        snapshot.scrolled = if down { -(shift as i32) } else { shift as i32 };
        // The lines that came into the view: at its top when it moved up,
        // at its bottom otherwise.
        let fresh = |y: usize| if down { y < shift } else { y >= lines - shift };
        let mut copied = 0;
        // The rows of the scrollback in the view, at its top, and those of
        // them that changed on the screen before they left it.
        let above = offset.min(lines);
        if above > 0 {
            let first = scrollback - offset;
            let mut changed = Vec::new();
            for (y, row) in rows.rows_from(first).take(above).enumerate() {
                let dirty = row.metadata().has(RowFlag::Dirty);
                if fresh(y) || dirty {
                    snapshot.lines[y].copy_list_row(row);
                    snapshot.changed[y] = true;
                    copied += 1;
                }
                if dirty {
                    changed.push(first + y);
                }
            }
            for index in changed {
                rows.mark_drawn(index);
            }
        }
        for y in above..lines {
            // A row of the screen, `offset` rows down from the view's: the
            // row below too, as the view is above the screen.
            let (page, row) = rows.screen_row_mut((y - offset) as u16);
            if fresh(y) || page.row(row).has(RowFlag::Dirty) {
                snapshot.lines[y].copy(page, row);
                snapshot.changed[y] = true;
                page.row_mut(row).set(RowFlag::Dirty, false);
                copied += 1;
            }
        }
        self.snapshot_rest(snapshot);
        copied
    }

    /// Brings the parts of `snapshot` but its rows up to date: the cursor,
    /// at the view's row `in_view`, shown if the view has it, the row below
    /// too while there is one, or the held one, of the view `map`, the
    /// colors and the modes.
    fn snapshot_state(&self, snapshot: &mut Snapshot, in_view: usize, map: &ViewMap) {
        let cursor = self.cursor();
        let rows_shown = usize::from(self.rows) + usize::from(snapshot.below);
        snapshot.cursor = SnapshotCursor {
            position: Cursor {
                y: u16::try_from(in_view).unwrap_or(u16::MAX),
                ..cursor
            },
            visible: self.cursor_visible() && in_view < rows_shown,
            style: self.cursor_style(),
            blink: self.cursor_blink(),
        };
        // An embedder's cursor in its place, still, of the program's shape.
        if let Some(held) = self.held_cursor_in_view(map) {
            let (x, y) = held.unwrap_or((cursor.x, 0));
            snapshot.cursor = SnapshotCursor {
                position: Cursor {
                    x,
                    y,
                    pending_wrap: false,
                },
                visible: held.is_some(),
                style: self.cursor_style(),
                blink: false,
            };
        }
        let reverse_video = self.reverse_video();
        snapshot.colors_changed =
            snapshot.reverse_video != reverse_video || snapshot.colors != self.colors;
        snapshot.reverse_video = reverse_video;
        snapshot.smooth_scroll = self.smooth_scroll();
        snapshot.alternate = self.alternate_active;
        snapshot.input_modes = self.input_modes();
        snapshot.keyboard_flags = self.keyboard_flags();
        if snapshot.colors != self.colors {
            snapshot.colors.clone_from(&self.colors);
        }
    }

    /// The parts of `snapshot` past its rows, as its view shows them: the
    /// selection, the decorations, the gutter's marks and the blocks, the
    /// matches and the images.
    #[inline]
    fn snapshot_rest(&mut self, snapshot: &mut Snapshot) {
        let top = snapshot.scrollback - snapshot.offset.min(snapshot.scrollback);
        let view = std::mem::take(&mut snapshot.view);
        // The view's rows, and the row below while it has one.
        let map = ViewMap::new(self.row_number(0), top, snapshot.lines.len(), &view);
        snapshot.selection = self.selection_in_snapshot(&map, &snapshot.changed);
        snapshot.decorations = self.decorated_view(&map);
        (snapshot.gutter, snapshot.blocks) = self.gutter_view(&map);
        self.drop_rewritten_matches(&map);
        snapshot.highlights = self.highlighted_view(&map);
        snapshot.matches_shown = self.matches_shown();
        snapshot.images = self.images_view(&map);
        self.sticky_view(snapshot, &map);
        snapshot.view = view;
    }

    /// Pins in `snapshot` the prompt rows of the block whose rows the view
    /// `map`'s top shows, while the embedder asks for them, as
    /// [`Terminal::sticky_rows`] finds them; copied again only as the
    /// block, its first pinned row, its rows' epoch or their count change,
    /// as a block's prompt rows, above its output, change only as a reflow
    /// writes them anew.
    fn sticky_view(&self, snapshot: &mut Snapshot, map: &ViewMap) {
        let found = (self.sticky_prompts && !self.alternate_active)
            .then(|| self.sticky_rows(map))
            .flatten();
        let Some((command, prompt, first, rows)) = found else {
            snapshot.sticky = None;
            snapshot.sticky_of = None;
            return;
        };
        let rows_list = &self.primary.rows;
        let of = (command, prompt, first, rows_list.epoch(), rows);
        let mut pinned = snapshot.sticky.take().unwrap_or_default();
        if snapshot.sticky_of != Some(of) || pinned.cols != rows_list.cols() {
            let Some(index) = self.row_index(first) else {
                snapshot.sticky_of = None;
                return;
            };
            let count = usize::from(rows);
            pinned.cols = rows_list.cols();
            pinned.lines.resize_with(count, Line::default);
            for (line, row) in pinned
                .lines
                .iter_mut()
                .zip(rows_list.rows_from(index).take(count))
            {
                line.copy_list_row(row);
            }
            pinned.changed.clear();
            pinned.changed.resize(count, true);
            snapshot.sticky_of = Some(of);
        }
        if pinned.colors != snapshot.colors {
            pinned.colors.clone_from(&snapshot.colors);
        }
        pinned.reverse_video = snapshot.reverse_video;
        snapshot.sticky = Some(pinned);
    }

    /// The command whose block holds the view `map`'s first row, the
    /// number of its prompt's row, and the number of the first row that
    /// [`Terminal::sticky_view`] pins with their count: the block's prompt
    /// rows, from the prompt's row to the last of the command's line, the
    /// row before its output's mark, or the mark's own past its first
    /// column, three at most and fewer than the view's rows, as a sticky
    /// header that stays at the view's top once the prompt's first row
    /// leaves it, over the rows that scroll under it; and, as the block's
    /// last row comes nearer the top than they are many, their last rows
    /// alone, as the block's end pushes them up; none while the prompt's
    /// first row shows, or of no command, or of none whose output came, nor
    /// of the command that runs while the view shows the screen, where it
    /// draws.
    fn sticky_rows(&self, map: &ViewMap) -> Option<(u64, i64, i64, u16)> {
        let top = map.top();
        let command = self.command_at(top)?;
        let (_, prompt) = command.prompt?;
        let (column, output) = command.output?;
        if prompt >= top {
            return None;
        }
        // The command's line ends at the row before its output's mark, or
        // at the mark's own past its first column.
        let line_end = if column == 0 { output - 1 } else { output };
        let rows = (line_end - prompt + 1)
            .min(i64::from(MOST_STICKY_ROWS))
            .min(i64::from(self.rows) - 1);
        // The block's last row: its end's, past its first column, or the
        // row before; of a command that has not ended, the row before the
        // next command's first.
        let last = match command.end {
            Some((column, end)) => end - i64::from(column == 0),
            None => {
                let next = self
                    .commands_meeting(top, top.saturating_add(rows))
                    .into_iter()
                    .find(|next| next.id != command.id)
                    .and_then(|next| {
                        [next.prompt, next.input, next.output, next.end]
                            .into_iter()
                            .flatten()
                            .next()
                    });
                // The command that runs, of no command after it, draws on
                // the screen: nothing pinned over it while the view shows
                // the screen, as a pager or a remote shell draws at its top.
                if next.is_none() && self.viewport_offset() == 0 {
                    return None;
                }
                next.map_or(i64::MAX, |(_, row)| row - 1)
            }
        };
        let room = last.saturating_sub(top).saturating_add(1);
        let shown = rows.min(room);
        let shown = u16::try_from(shown).ok().filter(|&shown| shown > 0)?;
        Some((command.id, prompt, prompt + rows - i64::from(shown), shown))
    }

    /// Brings `snapshot` up to date with `view`, a view of folds of the
    /// primary screen, as [`Terminal::snapshot`] does: a line of a row of the
    /// list as its row changed, or as the line showed another row or a
    /// placeholder before; a placeholder's line empty. Each run of the
    /// list's rows between the placeholders is read once, so that a view of
    /// folds costs a view's rows, whatever the scrollback.
    fn snapshot_folded(
        &mut self,
        snapshot: &mut Snapshot,
        view: super::folds::FoldedView,
    ) -> usize {
        let scrollback = self.primary.rows.scrollback_rows();
        let offset = scrollback.saturating_sub(view.top);
        // The cursor, at its row's place in the view, shown if the view
        // has it.
        let index = scrollback + usize::from(self.cursor().y);
        let in_view = view
            .rows
            .iter()
            .position(|&row| row == ViewRow::Row(index))
            .unwrap_or(usize::MAX);
        // A view of folds scrolls by whole rows alone.
        snapshot.fraction = 0.0;
        snapshot.below = false;
        self.snapshot_state(snapshot, in_view, &self.view_map(view.top, &view.rows));
        let origin = super::Origin {
            epoch: self.primary.rows.epoch(),
            top: self.row_number(view.top),
        };
        let rows = &mut self.primary.rows;
        let (cols, height) = (rows.cols(), rows.rows());
        let lines = usize::from(height);
        let fresh = snapshot
            .origin
            .is_none_or(|before| before.epoch != origin.epoch)
            || snapshot.cols != cols
            || snapshot.lines.len() != lines;
        snapshot.origin = Some(origin);
        snapshot.offset = offset;
        snapshot.scrollback = scrollback;
        snapshot.cols = cols;
        snapshot.lines.resize_with(lines, Line::default);
        snapshot.changed.clear();
        snapshot.changed.resize(lines, false);
        let mut before = std::mem::take(&mut snapshot.view);
        // A view moved by rows, as a scroll moves it: its lines move with
        // their rows, and those that came in alone are read.
        let shift = (!fresh && before.len() == lines)
            .then(|| shift_of(&before, &view.rows))
            .flatten()
            .unwrap_or(0);
        if shift > 0 {
            snapshot.lines.rotate_left(shift.unsigned_abs());
            before.rotate_left(shift.unsigned_abs());
        } else if shift < 0 {
            snapshot.lines.rotate_right(shift.unsigned_abs());
            before.rotate_right(shift.unsigned_abs());
        }
        // The view has at most `u16::MAX` rows.
        snapshot.scrolled = shift as i32;
        let came_in = |y: usize| {
            if shift > 0 {
                y >= lines - shift.unsigned_abs()
            } else {
                y < shift.unsigned_abs()
            }
        };
        let moved = |y: usize| fresh || came_in(y) || before.get(y) != view.rows.get(y);
        let mut copied = 0;
        let mut drawn = Vec::new();
        let mut y = 0;
        while y < view.rows.len() {
            // A placeholder's line, or one past the list's last, empty.
            let ViewRow::Row(start) = view.rows[y] else {
                if moved(y) {
                    snapshot.lines[y].clear(cols);
                    snapshot.changed[y] = true;
                    copied += 1;
                }
                y += 1;
                continue;
            };
            let run = view.rows[y..]
                .iter()
                .enumerate()
                .take_while(|&(at, &row)| row == ViewRow::Row(start + at))
                .count();
            for (at, row) in rows.rows_from(start).take(run).enumerate() {
                let line = y + at;
                let dirty = row.metadata().has(RowFlag::Dirty);
                if dirty || moved(line) {
                    snapshot.lines[line].copy_list_row(row);
                    snapshot.changed[line] = true;
                    copied += 1;
                }
                if dirty {
                    drawn.push(start + at);
                }
            }
            y += run;
        }
        for index in drawn {
            rows.mark_drawn(index);
        }
        snapshot.view = view.rows;
        snapshot.folded = (view.folded, view.folded_above);
        self.snapshot_rest(snapshot);
        copied
    }
}

/// A snapshot of the view, copied whole, without marking anything, for
/// tests to compare an updated snapshot with.
#[cfg(test)]
pub(super) fn copy_of(terminal: &Terminal) -> Snapshot {
    let rows = terminal.screen_rows();
    let offset = terminal.viewport_offset();
    let lines = rows
        .rows_from(rows.scrollback_rows() - offset)
        .take(usize::from(rows.rows()))
        .map(|row| {
            let mut line = Line::default();
            line.copy_list_row(row);
            line
        })
        .collect::<Vec<_>>();
    let cursor = terminal.cursor();
    let y = usize::from(cursor.y) + offset;
    Snapshot {
        origin: Some(terminal.origin()),
        cols: rows.cols(),
        changed: vec![true; lines.len()],
        scrolled: 0,
        offset,
        scrollback: rows.scrollback_rows(),
        colors_changed: false,
        cursor: SnapshotCursor {
            position: Cursor {
                y: u16::try_from(y).unwrap_or(u16::MAX),
                ..cursor
            },
            visible: terminal.cursor_visible() && y < lines.len(),
            style: terminal.cursor_style(),
            blink: terminal.cursor_blink(),
        },
        lines,
        reverse_video: terminal.reverse_video(),
        smooth_scroll: terminal.smooth_scroll(),
        alternate: terminal.is_alternate_screen(),
        input_modes: terminal.input_modes(),
        keyboard_flags: terminal.keyboard_flags(),
        colors: terminal.colors().clone(),
        selection: None,
        decorations: Vec::new(),
        gutter: Vec::new(),
        blocks: Vec::new(),
        highlights: Vec::new(),
        matches_shown: terminal.matches_shown(),
        images: Vec::new(),
        sticky: None,
        sticky_of: None,
        view: Vec::new(),
        folded: (0, 0),
        below: false,
        fraction: 0.0,
    }
}

/// How many rows a view of folds moved down since `before`, to `now`, as
/// its lines move up, or up of a negative number, where its rows are the
/// same but for those that came in: none of another view, nor of a view
/// that moved by its whole height or more.
fn shift_of(before: &[ViewRow], now: &[ViewRow]) -> Option<isize> {
    let first = *now.first()?;
    if let Some(at) = before.iter().position(|&row| row == first) {
        let shift = isize::try_from(at).ok()?;
        return (before[at..] == now[..now.len() - at]).then_some(shift);
    }
    let at = now.iter().position(|&row| Some(&row) == before.first())?;
    let shift = isize::try_from(at).ok()?;
    (now[at..] == before[..before.len() - at]).then_some(-shift)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::page::{Capacity, Color, Scrollback};
    use crate::parser::Parser;
    use crate::terminal::{Options, Scroll};

    fn terminal(cols: u16, rows: u16) -> (Terminal, Parser) {
        (Terminal::new(cols, rows, Options::default()), Parser::new())
    }

    #[test]
    fn a_snapshot_tells_the_alternate_screen() {
        let (mut terminal, mut parser) = terminal(10, 3);
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        assert!(!snapshot.alternate_screen());
        feed(&mut terminal, &mut parser, "\x1b[?1049h");
        terminal.snapshot(&mut snapshot);
        assert!(snapshot.alternate_screen());
        assert!(copy_of(&terminal).alternate_screen());
        feed(&mut terminal, &mut parser, "\x1b[?1049l");
        terminal.snapshot(&mut snapshot);
        assert!(!snapshot.alternate_screen());
    }

    #[test]
    fn a_snapshot_tells_the_keys_and_the_mouse_s_modes() {
        let (mut terminal, mut parser) = terminal(10, 3);
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        assert_eq!(snapshot.input_modes(), terminal.input_modes());
        assert_eq!(snapshot.keyboard_flags(), KeyboardFlags::NONE);
        // Bracketed paste, the mouse's tracking, and the kitty protocol's
        // first flag.
        feed(&mut terminal, &mut parser, "\x1b[?2004h\x1b[?1003h\x1b[>1u");
        terminal.snapshot(&mut snapshot);
        assert!(snapshot.input_modes().bracketed_paste);
        assert_eq!(snapshot.input_modes(), terminal.input_modes());
        assert_eq!(snapshot.keyboard_flags(), terminal.keyboard_flags());
        assert_ne!(snapshot.keyboard_flags(), KeyboardFlags::NONE);
        assert_eq!(copy_of(&terminal).input_modes(), terminal.input_modes());
    }

    fn feed(terminal: &mut Terminal, parser: &mut Parser, text: &str) {
        parser.advance(terminal, text.as_bytes());
    }

    /// Whether `snapshot` shows what `copy`, a copy of the screen, shows.
    fn assert_same(snapshot: &Snapshot, copy: &Snapshot) {
        assert_eq!(
            (snapshot.cols(), snapshot.rows()),
            (copy.cols(), copy.rows())
        );
        for y in 0..copy.rows() {
            assert_eq!(snapshot.line(y), copy.line(y), "row {y}");
        }
        assert_eq!(snapshot.cursor(), copy.cursor());
        assert_eq!(snapshot.reverse_video(), copy.reverse_video());
        assert_eq!(snapshot.alternate_screen(), copy.alternate_screen());
        assert_eq!(snapshot.colors(), copy.colors());
    }

    fn changed_rows(snapshot: &Snapshot) -> Vec<u16> {
        (0..snapshot.rows())
            .filter(|&y| snapshot.changed(y))
            .collect()
    }

    #[test]
    fn an_unchanged_screen_copies_nothing() {
        let (mut terminal, mut parser) = terminal(10, 4);
        feed(&mut terminal, &mut parser, "one\r\ntwo");
        let mut snapshot = Snapshot::default();
        assert_eq!(
            terminal.snapshot(&mut snapshot),
            4,
            "the first copies every row"
        );
        assert_eq!(terminal.snapshot(&mut snapshot), 0);
        assert!(changed_rows(&snapshot).is_empty());
        // The cursor comes along without a row.
        feed(&mut terminal, &mut parser, "\x1b[3;5H\x1b[?25l");
        assert_eq!(terminal.snapshot(&mut snapshot), 0);
        let cursor = snapshot.cursor();
        assert_eq!(
            (cursor.position.x, cursor.position.y, cursor.visible),
            (4, 2, false)
        );
        assert_same(&snapshot, &copy_of(&terminal));
    }

    #[test]
    fn a_change_copies_its_row() {
        let (mut terminal, mut parser) = terminal(10, 4);
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        feed(&mut terminal, &mut parser, "\x1b[3;1Hthree");
        assert_eq!(terminal.snapshot(&mut snapshot), 1);
        assert_eq!(changed_rows(&snapshot), [2]);
        assert_eq!(snapshot.line(2).cells()[0].content(), Content::Char('t'));
    }

    #[test]
    fn scrolling_moves_the_lines_and_copies_those_that_come_in() {
        let (mut terminal, mut parser) = terminal(10, 4);
        feed(&mut terminal, &mut parser, "a\r\nb\r\nc\r\nd");
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        feed(&mut terminal, &mut parser, "\r\ne\r\nf");
        assert_eq!(terminal.snapshot(&mut snapshot), 2);
        assert_eq!(snapshot.scrolled(), 2);
        assert_eq!(changed_rows(&snapshot), [2, 3]);
        let first = |snapshot: &Snapshot, y| snapshot.line(y).cells()[0].content();
        assert_eq!(first(&snapshot, 0), Content::Char('c'));
        assert_eq!(first(&snapshot, 3), Content::Char('f'));
        assert_same(&snapshot, &copy_of(&terminal));
        // More than a screen: every line is new.
        feed(&mut terminal, &mut parser, &"\r\nx".repeat(9));
        assert_eq!(terminal.snapshot(&mut snapshot), 4);
        assert_same(&snapshot, &copy_of(&terminal));
    }

    #[test]
    fn another_screen_or_a_resize_copies_every_row() {
        let (mut terminal, mut parser) = terminal(10, 4);
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        feed(&mut terminal, &mut parser, "\x1b[?1049h");
        assert_eq!(terminal.snapshot(&mut snapshot), 4);
        feed(&mut terminal, &mut parser, "\x1b[?1049l");
        assert_eq!(terminal.snapshot(&mut snapshot), 4);
        terminal.resize(12, 5);
        assert_eq!(terminal.snapshot(&mut snapshot), 5);
        assert_eq!((snapshot.cols(), snapshot.rows()), (12, 5));
        assert_eq!(terminal.snapshot(&mut snapshot), 0);
        // A resize to the same size lays nothing out again.
        terminal.resize(12, 5);
        assert_eq!(terminal.snapshot(&mut snapshot), 0);
    }

    /// The text of row `y` of `snapshot`, without its spaces at the end.
    fn text(snapshot: &Snapshot, y: u16) -> String {
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

    /// The first character of each row of `snapshot`.
    fn firsts(snapshot: &Snapshot) -> String {
        (0..snapshot.rows())
            .map(|y| text(snapshot, y).chars().next().unwrap_or(' '))
            .collect()
    }

    /// A terminal of 10 by 4 with `a` to `t` printed, a row each: `q` to
    /// `t` on the screen, and the others in the scrollback.
    fn lettered() -> (Terminal, Parser, Snapshot) {
        let (mut terminal, mut parser) = terminal(10, 4);
        let letters: Vec<String> = ('a'..='t').map(String::from).collect();
        feed(&mut terminal, &mut parser, &letters.join("\r\n"));
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        assert_eq!(firsts(&snapshot), "qrst");
        (terminal, parser, snapshot)
    }

    #[test]
    fn the_view_scrolls_through_the_scrollback_copying_the_rows_that_come_in() {
        let (mut terminal, _, mut snapshot) = lettered();
        terminal.scroll_viewport(Scroll::Up(2));
        assert_eq!(terminal.viewport_offset(), 2);
        assert_eq!(terminal.snapshot(&mut snapshot), 2);
        assert_eq!((snapshot.scrolled(), snapshot.offset()), (-2, 2));
        assert_eq!(changed_rows(&snapshot), [0, 1]);
        assert_eq!(firsts(&snapshot), "opqr");
        assert_same(&snapshot, &copy_of(&terminal));
        // The cursor, on the screen's last row, is below the view.
        assert!(!snapshot.cursor().visible);
        terminal.scroll_viewport(Scroll::PageUp);
        assert_eq!(terminal.snapshot(&mut snapshot), 4);
        assert_eq!(firsts(&snapshot), "klmn");
        terminal.scroll_viewport(Scroll::Down(1));
        assert_eq!(terminal.snapshot(&mut snapshot), 1);
        assert_eq!((snapshot.scrolled(), changed_rows(&snapshot)), (1, vec![3]));
        assert_eq!(firsts(&snapshot), "lmno");
        // The top, past which it goes no further.
        terminal.scroll_viewport(Scroll::Top);
        assert_eq!(terminal.viewport_offset(), 16);
        terminal.scroll_viewport(Scroll::Up(1));
        assert_eq!(terminal.viewport_offset(), 16);
        terminal.snapshot(&mut snapshot);
        assert_eq!(firsts(&snapshot), "abcd");
        terminal.scroll_viewport(Scroll::PageDown);
        terminal.snapshot(&mut snapshot);
        assert_eq!(firsts(&snapshot), "efgh");
        assert_same(&snapshot, &copy_of(&terminal));
        // Back at the screen, with the cursor.
        terminal.scroll_viewport(Scroll::Bottom);
        assert_eq!(terminal.viewport_offset(), 0);
        terminal.snapshot(&mut snapshot);
        assert_eq!(firsts(&snapshot), "qrst");
        let cursor = snapshot.cursor();
        assert!(cursor.visible);
        assert_eq!((cursor.position.x, cursor.position.y), (1, 3));
        assert_same(&snapshot, &copy_of(&terminal));
        assert_eq!(terminal.screen_rows().pins(), 0, "the view's pin went");
    }

    #[test]
    fn a_view_above_the_screen_keeps_its_rows_as_output_comes() {
        let (mut terminal, mut parser, mut snapshot) = lettered();
        terminal.scroll_viewport(Scroll::Up(2));
        terminal.snapshot(&mut snapshot);
        // `q` and `r`, the screen's first rows, are in the view: a change
        // of one copies it; a change below the view copies nothing.
        feed(&mut terminal, &mut parser, "\x1b[1;1HQ");
        assert_eq!(terminal.snapshot(&mut snapshot), 1);
        assert_eq!(changed_rows(&snapshot), [2]);
        feed(&mut terminal, &mut parser, "\x1b[4;1HT");
        assert_eq!(terminal.snapshot(&mut snapshot), 0);
        assert_eq!(firsts(&snapshot), "opQr");
        // Output scrolls the screen under the view, which keeps its rows
        // and copies nothing.
        feed(&mut terminal, &mut parser, "\r\nu\r\nv\r\nw");
        assert_eq!(terminal.viewport_offset(), 5);
        assert_eq!(terminal.snapshot(&mut snapshot), 0);
        assert_eq!(firsts(&snapshot), "opQr");
        assert_same(&snapshot, &copy_of(&terminal));
        // Back at the screen, the rows that come in, the one changed
        // below the view among them.
        terminal.scroll_viewport(Scroll::Bottom);
        assert_eq!(terminal.snapshot(&mut snapshot), 4);
        assert_eq!(firsts(&snapshot), "Tuvw");
        assert_same(&snapshot, &copy_of(&terminal));
    }

    #[test]
    fn the_alternate_screen_and_a_cleared_scrollback_bring_the_view_to_the_screen() {
        let (mut terminal, mut parser, mut snapshot) = lettered();
        terminal.scroll_viewport(Scroll::Up(2));
        feed(&mut terminal, &mut parser, "\x1b[?1049h");
        assert_eq!(terminal.viewport_offset(), 0);
        // The alternate screen has no scrollback to scroll.
        terminal.scroll_viewport(Scroll::Up(2));
        assert_eq!(terminal.viewport_offset(), 0);
        feed(&mut terminal, &mut parser, "\x1b[?1049l");
        terminal.snapshot(&mut snapshot);
        assert_eq!(firsts(&snapshot), "qrst");
        terminal.scroll_viewport(Scroll::Up(3));
        feed(&mut terminal, &mut parser, "\x1b[3J");
        assert_eq!(terminal.viewport_offset(), 0);
        terminal.snapshot(&mut snapshot);
        assert_eq!(firsts(&snapshot), "qrst");
        assert_same(&snapshot, &copy_of(&terminal));
        // ED 3 and output in one feed, as `clear` and a program's output
        // after it: the view follows the screen.
        feed(&mut terminal, &mut parser, "\r\nu\r\nv\r\nw");
        terminal.scroll_viewport(Scroll::Up(2));
        assert_eq!(terminal.viewport_offset(), 2);
        feed(&mut terminal, &mut parser, "\x1b[3J\r\nx\r\ny");
        assert_eq!(terminal.viewport_offset(), 0);
        terminal.snapshot(&mut snapshot);
        assert_eq!(firsts(&snapshot), "vwxy");
        assert_eq!(terminal.screen_rows().pins(), 0);
    }

    #[test]
    fn a_resize_keeps_the_view_on_its_rows() {
        let (mut terminal, _, mut snapshot) = lettered();
        terminal.scroll_viewport(Scroll::Up(2));
        for (cols, rows) in [(12, 4), (6, 3)] {
            terminal.resize(cols, rows);
            terminal.snapshot(&mut snapshot);
            assert_eq!(text(&snapshot, 0), "o", "at {cols} by {rows}");
            assert_same(&snapshot, &copy_of(&terminal));
        }
        // A taller screen that takes the view's first row shows the screen,
        // which the view follows from then on.
        terminal.resize(6, 8);
        assert_eq!(terminal.screen_rows().pins(), 0, "let go at once");
        terminal.snapshot(&mut snapshot);
        assert_eq!(terminal.viewport_offset(), 0);
        assert_eq!(firsts(&snapshot), "mnopqrst");
        assert_eq!(terminal.screen_rows().pins(), 0);
        assert_same(&snapshot, &copy_of(&terminal));
    }

    #[test]
    fn output_after_a_resize_that_takes_the_view_leaves_it_at_the_screen() {
        let (mut terminal, mut parser, mut snapshot) = lettered();
        terminal.scroll_viewport(Scroll::Up(2));
        // A taller screen, and output before the next snapshot.
        terminal.resize(10, 8);
        feed(&mut terminal, &mut parser, "\r\nu\r\nv\r\nw\r\nx");
        assert_eq!(terminal.viewport_offset(), 0);
        terminal.snapshot(&mut snapshot);
        assert_eq!(firsts(&snapshot), "qrstuvwx");
    }

    #[test]
    fn a_row_that_changed_before_it_left_the_screen_comes_into_the_view_as_it_is() {
        let (mut terminal, mut parser, mut snapshot) = lettered();
        // `q`, changed on the screen, then scrolled up into the
        // scrollback, with no update between.
        feed(&mut terminal, &mut parser, "\x1b[1;1HQ\x1b[4;2H\r\nu");
        terminal.scroll_viewport(Scroll::Up(1));
        terminal.snapshot(&mut snapshot);
        assert_eq!(firsts(&snapshot), "Qrst");
        assert_same(&snapshot, &copy_of(&terminal));
        assert_eq!(terminal.snapshot(&mut snapshot), 0, "drawn once");
    }

    #[test]
    fn a_view_whose_rows_leave_the_scrollback_keeps_its_top() {
        let page = Capacity::standard(10);
        let options = Options {
            scrollback: Scrollback {
                limit: 2 * page.bytes(),
                budget: None,
            },
            ..Options::default()
        };
        let (mut terminal, mut parser) = (Terminal::new(10, 4, options), Parser::new());
        let lines = |from: usize, count: usize| -> String {
            (from..from + count).map(|n| format!("{n}\r\n")).collect()
        };
        let rows = usize::from(page.rows);
        feed(&mut terminal, &mut parser, &lines(0, rows));
        terminal.scroll_viewport(Scroll::Top);
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        assert_eq!(text(&snapshot, 0), "0");
        // The oldest page leaves, with the view's first row.
        feed(&mut terminal, &mut parser, &lines(rows, 2 * rows));
        let scrollback = terminal.screen_rows().scrollback_rows();
        assert_eq!(terminal.viewport_offset(), scrollback);
        terminal.snapshot(&mut snapshot);
        assert_same(&snapshot, &copy_of(&terminal));
        let top = text(&snapshot, 0);
        assert_ne!(top, "0");
        // It stays at the top as output comes.
        feed(&mut terminal, &mut parser, &lines(3 * rows, 3));
        assert_eq!(terminal.snapshot(&mut snapshot), 0);
        assert_eq!(text(&snapshot, 0), top);
        assert_eq!(terminal.viewport_offset(), scrollback + 3);
    }

    #[test]
    fn an_update_tells_when_the_colors_changed() {
        let (mut terminal, mut parser) = terminal(10, 4);
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        for change in [
            "\x1b]4;1;rgb:00/ff/00\x07",
            "\x1b[?5h",
            "\x1b]11;rgb:10/20/30\x07",
        ] {
            feed(&mut terminal, &mut parser, change);
            assert_eq!(terminal.snapshot(&mut snapshot), 0, "{change:?}: no row");
            assert!(snapshot.colors_changed(), "{change:?}");
            assert_eq!(terminal.snapshot(&mut snapshot), 0);
            assert!(!snapshot.colors_changed(), "{change:?}, once");
        }
        // The same colors again change nothing.
        feed(
            &mut terminal,
            &mut parser,
            "\x1b]4;1;rgb:00/ff/00\x07\x1b[?5h",
        );
        terminal.snapshot(&mut snapshot);
        assert!(!snapshot.colors_changed());
    }

    #[test]
    fn a_line_has_its_styles_and_clusters() {
        let (mut terminal, mut parser) = terminal(10, 2);
        feed(
            &mut terminal,
            &mut parser,
            "\x1b[31mr\x1b[1;42mg\x1b[me\u{301}",
        );
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        let line = snapshot.line(0);
        let style = |x: usize| line.style(line.cells()[x].style());
        assert_eq!(style(0).fg, Color::Palette(1));
        assert_eq!((style(1).bg, style(1).bold), (Color::Palette(2), true));
        assert_eq!(style(2), Style::DEFAULT);
        assert_eq!(line.cells()[2].content(), Content::Grapheme('e'));
        assert_eq!(line.grapheme(2), ['\u{301}']);
        assert_eq!(line.grapheme(0), [] as [char; 0]);
        assert!(!line.soft_wrapped());
        // Each style once, however many cells use it.
        feed(
            &mut terminal,
            &mut parser,
            "\r\n\x1b[31mrrr\x1b[32mggg\x1b[31mrrr",
        );
        terminal.snapshot(&mut snapshot);
        assert_eq!(snapshot.line(1).styles.len(), 2);
        // A line that wrapped.
        feed(&mut terminal, &mut parser, "\x1b[m\r\n0123456789wrapped");
        terminal.snapshot(&mut snapshot);
        assert!(snapshot.line(0).soft_wrapped());
        assert!(!snapshot.line(1).soft_wrapped());
        // A prompt's start, which marks its row to draw.
        assert!(!snapshot.line(1).prompt_start());
        feed(&mut terminal, &mut parser, "\x1b]133;A\x07");
        assert_eq!(terminal.snapshot(&mut snapshot), 1);
        assert!(snapshot.line(1).prompt_start());
    }

    /// Pieces of output that change the screen in every way they can.
    const PIECES: &[&str] = &[
        "a",
        "hello world",
        "\r\n",
        "\n",
        "\r",
        "\t",
        "\x1b[H",
        "\x1b[4;3H",
        "\x1b[2J",
        "\x1b[3J",
        "\x1b[J",
        "\x1b[K",
        "\x1b[1K",
        "\x1b[2L",
        "\x1b[2M",
        "\x1b[2S",
        "\x1b[T",
        "\x1b[2;5r",
        "\x1b[r",
        "\x1b[3@",
        "\x1b[2P",
        "\x1b[2X",
        "\x1b[31m",
        "\x1b[1;42m",
        "\x1b[m",
        "中文",
        "e\u{301}",
        "\u{231A}\u{FE0E}",
        "\u{2764}\u{FE0F}",
        "\x1b[?1049h",
        "\x1b[?1049l",
        "\x1b[?5h",
        "\x1b[?5l",
        "\x1b]4;1;rgb:12/34/56\x07",
        "\x1b]10;rgb:ff/00/00\x07",
        "\x1b7",
        "\x1b8",
        "\x1bM",
        "\x1bD",
        "\x1bE",
        "\x1b[4h",
        "\x1b[4l",
        "\x1b[?7l",
        "\x1b[?7h",
        "\x1b[?25l",
        "\x1b[?25h",
        "\x1b[2 q",
        "\x1b#8",
        "\x1b[?69h\x1b[2;6s",
        "\x1b[?69l",
        "\x1b]8;;http://x\x07link\x1b]8;;\x07",
        "\x1b]133;A\x07",
        "\x1b]11;rgb:10/20/30\x07",
        "\x1bc",
        "\x1b[3J",
        "line\r\nline\r\nline\r\n",
    ];

    #[derive(Clone, Debug)]
    enum Op {
        Feed(usize),
        Resize(u16, u16),
        Scroll(Scroll),
        Snapshot,
    }

    fn scroll() -> impl Strategy<Value = Scroll> {
        prop_oneof![
            (1_usize..6).prop_map(Scroll::Up),
            (1_usize..6).prop_map(Scroll::Down),
            Just(Scroll::PageUp),
            Just(Scroll::PageDown),
            Just(Scroll::Top),
            Just(Scroll::Bottom),
        ]
    }

    fn op() -> impl Strategy<Value = Op> {
        prop_oneof![
            8 => (0..PIECES.len()).prop_map(Op::Feed),
            1 => (2_u16..14, 1_u16..8).prop_map(|(cols, rows)| Op::Resize(cols, rows)),
            2 => scroll().prop_map(Op::Scroll),
            3 => Just(Op::Snapshot),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        /// An updated snapshot shows what a copy of the view shows, however
        /// the screen changed and the view moved between two updates.
        #[test]
        fn an_updated_snapshot_matches_a_copy_of_the_screen(ops in prop::collection::vec(op(), 1..80)) {
            let (mut terminal, mut parser) = terminal(10, 5);
            let mut snapshot = Snapshot::default();
            for op in ops {
                match op {
                    Op::Feed(piece) => feed(&mut terminal, &mut parser, PIECES[piece]),
                    Op::Resize(cols, rows) => terminal.resize(cols, rows),
                    Op::Scroll(scroll) => terminal.scroll_viewport(scroll),
                    Op::Snapshot => {
                        let before = (snapshot.colors().clone(), snapshot.reverse_video());
                        let copied = terminal.snapshot(&mut snapshot);
                        let after = (terminal.colors().clone(), terminal.reverse_video());
                        prop_assert_eq!(snapshot.colors_changed(), before != after);
                        prop_assert!(copied <= usize::from(snapshot.rows()));
                        assert_same(&snapshot, &copy_of(&terminal));
                        prop_assert_eq!(terminal.snapshot(&mut snapshot), 0, "nothing changed since");
                    }
                }
            }
            terminal.snapshot(&mut snapshot);
            assert_same(&snapshot, &copy_of(&terminal));
        }
    }
}

#[cfg(test)]
mod sticky_tests;

#[cfg(test)]
mod fraction_tests;
