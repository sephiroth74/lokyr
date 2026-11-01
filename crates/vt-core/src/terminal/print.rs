//! Printing: the path that every character of text takes into the grid.

use super::{Cluster, Cursor, Terminal};
use crate::page::{Cell, Content, Hyperlink, Page, RowFlag, Style, StyleId, Width};
use crate::unicode::{GraphemeBreak, Mode, Normalization, Widths, properties};

/// The most codepoints that a cluster keeps; those that would join it past
/// them are dropped. Unicode's stream-safe text format allows thirty
/// combining marks in a row (<https://www.unicode.org/reports/tr15/#Stream_Safe_Text_Format>),
/// and the longest emoji sequences have about ten codepoints. The bound
/// keeps a cluster's work, normalization included, small.
const MAX_CLUSTER: usize = 32;

/// The most characters that [`Terminal::print_char_run`] writes at once.
const STRETCH: usize = 64;

/// The most codepoints that join the clusters of a stretch as it is
/// written.
const JOINS: usize = 64;

/// The buffers of a stretch that [`Terminal::print_char_run`] writes.
#[derive(Debug, Default)]
pub(super) struct Stretch {
    /// Its characters, each with the cells that it takes.
    chars: Vec<(char, u8)>,
    /// The codepoints that join its clusters, each with its cluster's
    /// column from the stretch's start.
    joins: Vec<(u16, char)>,
}

/// Whether writing `width` cells from column `x` of a row whose cells are
/// `cells` goes over a wide character in the two columns from `start`,
/// where wraps land: one at `start`, or its second half, for which the row
/// above may have left a spacer head, or one that starts at `start + 1`,
/// which `alacritty_terminal` takes for one too.
fn over_wide_start(cells: &[Cell], x: u16, width: u16, start: u16) -> bool {
    x <= start + 1
        && x + width > start
        && cells[usize::from(x.max(start))..usize::from((x + width).min(start + 2))]
            .iter()
            .any(|cell| matches!(cell.width(), Width::Wide | Width::SpacerTail))
}

/// The width that the cluster at column `x` of row `row` of `page` takes,
/// as `widths` give it, if its cell has another: wide for a narrow one that
/// takes two cells now, narrow for a wide one that takes one.
fn new_width(page: &Page, row: u16, x: u16, widths: Widths) -> Option<Width> {
    let cell = page.cells(row)[usize::from(x)];
    let (Content::Char(first) | Content::Grapheme(first)) = cell.content() else {
        return None;
    };
    match (
        cell.width(),
        widths.cluster_width(first, page.grapheme(row, x)),
    ) {
        (Width::Narrow, 2) => Some(Width::Wide),
        (Width::Wide, 1) => Some(Width::Narrow),
        _ => None,
    }
}

/// The width that `c`, which just joined the cluster at column `x` of row
/// `row` of `page`, gives it, if its cell has another, as
/// [`Widths::joined_cells`] counts it: the codepoint alone, and the
/// cluster's base for a variation selector.
fn joined_width(page: &Page, row: u16, x: u16, widths: Widths, c: char) -> Option<Width> {
    let cell = page.cells(row)[usize::from(x)];
    let (Content::Char(first) | Content::Grapheme(first)) = cell.content() else {
        return None;
    };
    let cells = match cell.width() {
        Width::Narrow => 1,
        Width::Wide => 2,
        _ => return None,
    };
    // The base, the first codepoint that has a width.
    let base = || {
        std::iter::once(first)
            .chain(page.grapheme(row, x).iter().copied())
            .find(|&c| widths.char_width(c) > 0)
            .unwrap_or(first)
    };
    match (cells, widths.joined_cells(cells, c, properties(c), base)) {
        (1, 2) => Some(Width::Wide),
        (2, 1) => Some(Width::Narrow),
        _ => None,
    }
}

impl Terminal {
    /// Prints `c`: it joins the cluster before it, or starts one in the
    /// cells at the cursor. A codepoint of no width that starts a cluster
    /// joins the character before it instead, unless it is a prepended
    /// character, such as U+0600, whose cluster the next character joins.
    pub(super) fn print_char(&mut self, c: char) {
        let properties = properties(c);
        let width = self.widths.char_width_with(c, properties);
        let joins = match self.widths.mode {
            Mode::Unicode => !self.segmenter.starts_cluster_with(properties),
            Mode::Legacy => width == 0,
        };
        let prepended = properties.grapheme_break == GraphemeBreak::Prepend;
        if joins || (width == 0 && !prepended) {
            self.join_previous(c);
            return;
        }
        // A prepended character of no width takes a cell until the character
        // after it gives the cluster its width.
        self.start_cluster(c, width.max(1));
    }

    /// Prints `c`, as the character it stands for in the character set in
    /// use.
    pub(super) fn print_translated(&mut self, c: char) {
        self.last_printed = Some(c);
        let c = self.charsets.translate(c);
        self.print_char(c);
    }

    /// Prints `text`, a run of printable ASCII, as
    /// [`Terminal::print_translated`] would print its characters one by
    /// one, but a stretch of a row at a time where it can: each character
    /// of ASCII takes one cell, starts a cluster and is in normal form
    /// already.
    pub(super) fn print_ascii_run(&mut self, text: &[u8]) {
        let Some(&last) = text.last() else {
            return;
        };
        let mut text = text;
        // Another character set, or a single shift, gives the characters
        // other meanings, and an open link takes the cells into its page's
        // table, so they go one at a time.
        while !self.charsets.is_plain() || self.link.is_open() {
            let Some((&first, rest)) = text.split_first() else {
                return;
            };
            self.print_translated(char::from(first));
            text = rest;
        }
        let Some((&first, rest)) = text.split_first() else {
            return;
        };
        if self.widths.mode == Mode::Unicode && !self.segmenter.ascii_starts_cluster() {
            self.join_previous(char::from(first));
            text = rest;
        }
        while !text.is_empty() {
            let written = match self.write_ascii_plainly(text) {
                Some(written) => written,
                None => self.write_ascii(text),
            };
            text = &text[written..];
        }
        self.last_printed = Some(char::from(last));
    }

    /// Writes the start of `text` as [`Terminal::write_ascii`] does, in the
    /// commonest case: without a pending wrap or insert mode, and with room
    /// for the pen's style in the page. It finds the screen, the cursor and
    /// the row's page once, and returns `None`, having written nothing, in
    /// the other cases.
    fn write_ascii_plainly(&mut self, text: &[u8]) -> Option<usize> {
        if self.insert {
            return None;
        }
        debug_assert_eq!(self.pen_default, self.pen == Style::DEFAULT);
        let (margins, cols, protected, zone) = (self.margins, self.cols, self.protected, self.zone);
        let (pen, last, default) = (self.pen, self.pen_id, self.pen_default);
        let screen = if self.alternate_active {
            &mut self.alternate
        } else {
            &mut self.primary
        };
        let Cursor { x, y, pending_wrap } = screen.cursor;
        if pending_wrap {
            return None;
        }
        // The line's end, as `line_end` tells it, and the column where its
        // wraps land, which a spacer head above stands for.
        let end = if x > margins.right {
            cols
        } else {
            margins.right + 1
        };
        let start = margins.left;
        let len = text.len().min(usize::from(end - x));
        // The line has room for `len` cells, so it fits in a u16.
        let width = len as u16;
        let (page, row) = screen.rows.screen_row_mut(y);
        let style = if default {
            StyleId::DEFAULT
        } else if page.styles().holds(last, &pen) {
            page.styles_mut().retain(last);
            last
        } else {
            // A full table makes the page grow, which the path of every
            // case does.
            page.styles_mut().acquire(&pen).ok()?
        };
        let template = Cell::EMPTY
            .with_style(style)
            .with_protected(protected)
            .with_semantic(zone);
        let head = (y > 0 && x <= start + 1).then_some(start);
        let over_wide = page.overwrite_ascii(row, x, &text[..len], template, head);
        page.styles_mut().release(style);
        screen.cursor = if x + width >= end {
            Cursor {
                x: end - 1,
                y,
                pending_wrap: true,
            }
        } else {
            Cursor {
                x: x + width,
                y,
                pending_wrap: false,
            }
        };
        self.pen_id = style;
        self.cluster = Some(Cluster {
            x: x + width - 1,
            y,
        });
        if over_wide {
            self.clear_spacer_head_above(y, None);
        }
        Some(len)
    }

    /// Prints `chars`, as [`Terminal::print_translated`] would print them
    /// one by one, but each stretch of characters that start clusters of
    /// their own, one or two cells wide, and fit in the line, at once, with
    /// the row's page found once for the stretch and once for its joins,
    /// and with the codepoints that join those clusters and leave them as
    /// wide. The others take the path of each
    /// character: those that join a cluster otherwise, or have no width, or
    /// do not fit, or come with another character set, an open link, insert
    /// mode or a normal form to put them in.
    pub(super) fn print_char_run(&mut self, chars: &[char]) {
        let plain = self.charsets.is_plain()
            && !self.link.is_open()
            && !self.insert
            && self.normalization == Normalization::None;
        if !plain {
            for &c in chars {
                self.print_translated(c);
            }
            return;
        }
        // The stretch's characters, from the terminal's buffer; the
        // codepoints that join its clusters, which few runs have, wait in
        // the terminal's other buffer, out of the loop's way.
        let mut stretch = std::mem::take(&mut self.stretch.chars);
        stretch.clear();
        self.stretch.joins.clear();
        let mut cells = 0_u16;
        let (mut x, mut end) = (0, 0);
        let mut rest = chars.iter();
        while let Some(&c) = rest.next() {
            let properties = properties(c);
            let width = self.widths.char_width_with(c, properties);
            let mut segmenter = self.segmenter;
            let starts = match self.widths.mode {
                Mode::Unicode => segmenter.starts_cluster_with(properties),
                Mode::Legacy => width > 0,
            };
            if stretch.is_empty() {
                let cursor = self.cursor();
                (x, end) = (cursor.x, self.line_end(cursor.x));
                if cursor.pending_wrap {
                    // The path of each character wraps first.
                    end = x;
                }
            }
            // `x + cells` never passes `end`, and the sum with the width
            // could pass `u16::MAX` at the last column of the widest screen.
            let fits = u16::from(width) <= end - (x + cells);
            if !starts || !matches!(width, 1 | 2) || !fits || stretch.len() == STRETCH {
                // A codepoint that joins the stretch's last cluster and
                // leaves it as wide, as clusters keep their widths in the
                // legacy mode, joins it as the stretch is written; one that
                // widens a narrow one, as a vowel sign that spaces, makes it
                // wide in the stretch, as the path of each character would:
                // its line has room for the second cell, as a stretch that
                // reaches the line's end is written at once.
                if !starts
                    && let Some(&(base, last)) = stretch.last()
                    && self.stretch.joins.len() < JOINS
                {
                    let after = match self.widths.mode {
                        Mode::Legacy => last,
                        Mode::Unicode => {
                            self.widths
                                .joined_cells_of(last, c, properties, width, || base)
                        }
                    };
                    let widens = after == 2 && last == 1;
                    if after == last || widens {
                        if widens && let Some(entry) = stretch.last_mut() {
                            debug_assert!(x + cells < end, "a narrow cluster's second cell");
                            entry.1 = 2;
                            cells += 1;
                        }
                        self.segmenter = segmenter;
                        self.stretch.joins.push((cells - u16::from(after), c));
                        continue;
                    }
                }
                self.write_stretch(x, &stretch, cells, end);
                cells = 0;
                stretch.clear();
                if starts {
                    self.print_translated(c);
                } else {
                    // It joins the cluster before it, as the path of each
                    // character finds, with the state that it leaves, and so
                    // may the codepoints after it.
                    self.segmenter = segmenter;
                    let after = rest.as_slice();
                    let more = self.join_previous_run(c, after);
                    rest = after[more..].iter();
                }
                continue;
            }
            self.segmenter = segmenter;
            stretch.push((c, width));
            cells += u16::from(width);
            if x + cells == end {
                self.write_stretch(x, &stretch, cells, end);
                cells = 0;
                stretch.clear();
            }
        }
        self.write_stretch(x, &stretch, cells, end);
        self.stretch.chars = stretch;
        if let Some(&last) = chars.last() {
            self.last_printed = Some(last);
        }
    }

    /// Writes `stretch`, characters that take `cells` cells, from column
    /// `x` of the cursor's row, which is where the cursor is, in a line that
    /// ends before column `end`, as [`Terminal::start_cluster`] would write
    /// them one by one; then the codepoints that wait to join its clusters,
    /// at their columns from `x`, which leave them as wide, as
    /// [`Terminal::join`] would add them.
    fn write_stretch(&mut self, x: u16, stretch: &[(char, u8)], cells: u16, end: u16) {
        let Some(&(_, last_width)) = stretch.last() else {
            return;
        };
        let y = self.cursor().y;
        let (protected, zone) = (self.protected, self.zone);
        let start = self.margins.left;
        let head_above = y > 0 && x <= start + 1;
        let mut over_wide = false;
        self.with_pen(y, |page, row, style| {
            over_wide = head_above && over_wide_start(page.cells(row), x, cells, start);
            page.clear_halves(row, x, cells);
            let template = Cell::EMPTY
                .with_style(style)
                .with_protected(protected)
                .with_semantic(zone);
            page.write_chars(row, x, stretch, template);
        });
        if over_wide {
            self.clear_spacer_head_above(y, None);
        }
        self.cluster = Some(Cluster {
            x: x + cells - u16::from(last_width),
            y,
        });
        let cursor = &mut self.screen_mut().cursor;
        if x + cells >= end {
            cursor.x = end - 1;
            cursor.pending_wrap = true;
        } else {
            cursor.x = x + cells;
            cursor.pending_wrap = false;
        }
        if !self.stretch.joins.is_empty() {
            self.write_joins(x, y);
        }
    }

    /// Adds the codepoints that wait to join the clusters of the stretch
    /// written from column `x` of row `y`, at their columns from `x`, which
    /// leave them as wide, as [`Terminal::join`] would add them.
    #[inline(never)]
    fn write_joins(&mut self, x: u16, y: u16) {
        let joins = std::mem::take(&mut self.stretch.joins);
        let rows = &mut self.screen_mut().rows;
        let (mut page, mut row) = rows.screen_row_mut(y);
        // The codepoints of each cluster go in together, those of one column
        // following one another.
        for cluster in joins.chunk_by(|a, b| a.0 == b.0) {
            let at = x + cluster[0].0;
            let mut codepoints = [char::default(); MAX_CLUSTER];
            for (slot, &(_, c)) in codepoints.iter_mut().zip(cluster) {
                *slot = c;
            }
            let codepoints = &codepoints[..cluster.len().min(MAX_CLUSTER)];
            // Those past the most that a cluster keeps are dropped.
            let room = (MAX_CLUSTER - 1).saturating_sub(page.grapheme(row, at).len());
            let added = &codepoints[..codepoints.len().min(room)];
            let mut together = !added.is_empty();
            while together {
                match page.push_codepoints(row, at, added) {
                    Ok(()) => together = false,
                    Err(full) => {
                        let grown = rows.grow_page(y, full);
                        (page, row) = rows.screen_row_mut(y);
                        if grown.is_err() {
                            break;
                        }
                    }
                }
            }
            if !together {
                continue;
            }
            // A page that has no room for them all and cannot grow takes
            // them one by one, as long as it has room.
            for &c in added {
                while page.grapheme(row, at).len() + 1 < MAX_CLUSTER {
                    match page.push_codepoint(row, at, c) {
                        Ok(()) => break,
                        Err(full) => {
                            let grown = rows.grow_page(y, full);
                            (page, row) = rows.screen_row_mut(y);
                            if grown.is_err() {
                                break;
                            }
                        }
                    }
                }
            }
        }
        let mut joins = joins;
        joins.clear();
        self.stretch.joins = joins;
    }

    /// Adds `c`, a codepoint that joins the cluster before it, whose state
    /// the segmenter has taken, and the codepoints at the start of `rest`
    /// that join that cluster too, to it, as [`Terminal::join_previous`]
    /// would add them one by one; and returns how many of `rest` it added.
    /// Those that leave the cluster's cells as many, whether it takes one or
    /// two, go into the page at once, and change no width: combining marks,
    /// as most clusters of several codepoints have.
    #[inline(never)]
    fn join_previous_run(&mut self, c: char, rest: &[char]) -> usize {
        let widths = self.widths;
        let keeps = |c: char, properties| {
            let width = widths.char_width_with(c, properties);
            widths.keeps_cells_of(1, c, properties, width)
                && widths.keeps_cells_of(2, c, properties, width)
        };
        if rest.is_empty() || !keeps(c, properties(c)) {
            self.join_previous(c);
            return 0;
        }
        let mut segmenter = self.segmenter;
        let mut more = 0;
        for &next in rest {
            let properties = properties(next);
            let mut after = segmenter;
            let joins = match widths.mode {
                Mode::Unicode => !after.starts_cluster_with(properties),
                Mode::Legacy => widths.char_width_with(next, properties) == 0,
            };
            if !joins || !keeps(next, properties) {
                break;
            }
            segmenter = after;
            more += 1;
        }
        if more == 0 {
            self.join_previous(c);
            return 0;
        }
        self.segmenter = segmenter;
        if let Some(cluster) = self.cluster.or_else(|| self.character_left_of_cursor()) {
            // Those past the most that a cluster keeps would be dropped.
            let mut codepoints = [c; MAX_CLUSTER];
            let count = (1 + more).min(MAX_CLUSTER);
            codepoints[1..count].copy_from_slice(&rest[..count - 1]);
            self.join_all(cluster, &codepoints[..count]);
        }
        more
    }

    /// Adds `codepoints` to the cluster at `cluster`, as [`Terminal::join`]
    /// would add them one by one, at once: codepoints that leave its cells
    /// as many, whether it takes one or two, without a normal form to put it
    /// in. A cluster whose first codepoint has no width, as a prepended
    /// character, or a page that has no room for them all, takes them one
    /// by one.
    fn join_all(&mut self, cluster: Cluster, codepoints: &[char]) {
        let Cluster { x, y } = cluster;
        let widths = self.widths;
        let unicode = widths.mode == Mode::Unicode;
        let rows = &mut self.screen_mut().rows;
        let width = loop {
            let (page, row) = rows.screen_row_mut(y);
            if unicode {
                let (Content::Char(first) | Content::Grapheme(first)) =
                    page.cells(row)[usize::from(x)].content()
                else {
                    break Err(());
                };
                if widths.char_width(first) == 0 {
                    break Err(());
                }
            }
            // Those past the most that a cluster keeps are dropped.
            let room = (MAX_CLUSTER - 1).saturating_sub(page.grapheme(row, x).len());
            let added = &codepoints[..codepoints.len().min(room)];
            if added.is_empty() {
                break Ok(None);
            }
            match page.push_codepoints(row, x, added) {
                // Codepoints that leave the cluster's cells as many change
                // no width, as they would one by one.
                Ok(()) => break Ok(None),
                Err(full) => {
                    if rows.grow_page(y, full).is_err() {
                        break Err(());
                    }
                }
            }
        };
        match width {
            Ok(Some(Width::Wide)) => self.widen(cluster),
            Ok(Some(_)) => self.narrow(cluster),
            Ok(None) => {}
            Err(()) => {
                for &c in codepoints {
                    self.join(cluster, c);
                }
            }
        }
        self.cluster = Some(cluster);
    }

    /// Adds `c` to the cluster that the last character printed started, or,
    /// after the cursor moved, to the character left of the cursor. With
    /// neither, `c` is dropped.
    fn join_previous(&mut self, c: char) {
        if let Some(cluster) = self.cluster.or_else(|| self.character_left_of_cursor()) {
            self.join(cluster, c);
        }
    }

    /// Writes the start of `text`, printable ASCII, from the cursor to the
    /// end of its line, and returns how many bytes that took.
    fn write_ascii(&mut self, text: &[u8]) -> usize {
        if self.cursor().pending_wrap && self.autowrap {
            self.wrap();
        }
        let Cursor { x, y, pending_wrap } = self.cursor();
        let end = self.line_end(x);
        // A wrap can stay pending after the margins moved; only at the last
        // column of the line does it hold the cursor there.
        if pending_wrap && x == end - 1 {
            // Without autowrap, each character replaces the one in the last
            // column, so only the last of them stays.
            self.write_cells(x, y, &text[text.len() - 1..], end);
            self.cluster = Some(Cluster { x, y });
            return text.len();
        }
        let len = text.len().min(usize::from(end - x));
        // The line has room for `len` cells, so it fits in a u16.
        let width = len as u16;
        self.write_cells(x, y, &text[..len], end);
        self.cluster = Some(Cluster {
            x: x + width - 1,
            y,
        });
        let cursor = &mut self.screen_mut().cursor;
        if x + width >= end {
            cursor.x = end - 1;
            cursor.pending_wrap = true;
        } else {
            cursor.x = x + width;
            cursor.pending_wrap = false;
        }
        len
    }

    /// Writes `text`, printable ASCII that fits in the line, which ends
    /// before column `end`, into the cells from column `x` of row `y`, with
    /// the pen's style, as [`Terminal::start_cluster`] writes a character.
    fn write_cells(&mut self, x: u16, y: u16, text: &[u8], end: u16) {
        let width = text.len() as u16;
        let (insert, protected, zone) = (self.insert, self.protected, self.zone);
        if insert {
            self.screen_mut().rows.insert_blanks(y, x, width, end);
        }
        // The row's page is found once, for the spacer head as well.
        let start = self.margins.left;
        let head_above = !insert && y > 0 && x <= start + 1;
        let mut over_wide = false;
        self.with_pen(y, |page, row, style| {
            let template = Cell::EMPTY
                .with_style(style)
                .with_protected(protected)
                .with_semantic(zone);
            if insert {
                page.write_ascii(row, x, text, template);
            } else {
                over_wide =
                    page.overwrite_ascii(row, x, text, template, head_above.then_some(start));
            }
        });
        if over_wide {
            self.clear_spacer_head_above(y, None);
        }
    }

    /// Writes a new cluster, `c`, which takes `width` cells, at the cursor.
    fn start_cluster(&mut self, c: char, width: u8) {
        let width = u16::from(width);
        if self.cursor().pending_wrap && self.autowrap {
            self.wrap();
        }
        // A wide character does not fit in the last column of the line.
        // Lines have two columns at least, as the screen and the margins
        // do, so it fits in the next row, or one column back.
        let Cursor { x, y, .. } = self.cursor();
        let end = self.line_end(x);
        // The column of the head that this character leaves in the row above
        // it, which the check of that row passes over: when the wrap takes
        // it to another row, down or by scrolling, rather than back to the
        // start of the same one, as at the last row below the scroll region.
        let mut own_head = None;
        if width == 2 && x == end - 1 {
            if self.autowrap {
                self.clear_overlaps(x, y, 1);
                self.put(x, y, Cell::EMPTY.with_width(Width::SpacerHead));
                // A head at the screen's last column whose wrap does not join
                // the rows stands for text that goes on elsewhere: the row's
                // soft wrap, which an earlier wrap marked, ends.
                if x == self.cols - 1 && !self.wrap_joins_rows() {
                    self.end_soft_wrap(y);
                }
                own_head = self.index_moves_on().then_some(x);
                self.wrap();
            } else {
                self.screen_mut().cursor.x = x - 1;
            }
        }

        let (x, y) = (self.cursor().x, self.cursor().y);
        let end = self.line_end(x);
        let (insert, protected, zone) = (self.insert, self.protected, self.zone);
        if insert {
            self.screen_mut().rows.insert_blanks(y, x, width, end);
        }
        // The row's page is found once, for the spacer heads above as well.
        let start = self.margins.left;
        let head_above = !insert && y > 0 && x <= start + 1;
        let mut over_wide = false;
        self.with_pen(y, |page, row, style| {
            if !insert {
                over_wide = head_above && over_wide_start(page.cells(row), x, width, start);
                page.clear_halves(row, x, width);
            }
            let blank = Cell::EMPTY
                .with_style(style)
                .with_protected(protected)
                .with_semantic(zone);
            let cell = blank.with_content(Content::Char(c));
            if width == 2 {
                page.set_cells(
                    row,
                    x,
                    &[
                        cell.with_width(Width::Wide),
                        blank.with_width(Width::SpacerTail),
                    ],
                );
            } else {
                page.set_cells(row, x, &[cell]);
            }
        });
        if over_wide {
            self.clear_spacer_head_above(y, own_head);
        }
        if self.link.is_open() {
            self.link_cells(x, y, width);
        }
        self.cluster = Some(Cluster { x, y });
        if self.normalization != Normalization::None {
            self.normalize(Cluster { x, y });
        }

        let cursor = &mut self.screen_mut().cursor;
        if x + width >= end {
            cursor.x = end - 1;
            cursor.pending_wrap = true;
        } else {
            cursor.x = x + width;
            cursor.pending_wrap = false;
        }
    }

    /// Where the line of text at column `x` ends, excluded: at the right
    /// margin, unless `x` is past it, and then at the edge of the screen.
    fn line_end(&self, x: u16) -> u16 {
        if x > self.margins.right {
            self.cols
        } else {
            self.margins.right + 1
        }
    }

    /// Adds `c` to the cluster at `cluster`, whose width can change. A
    /// cluster that has [`MAX_CLUSTER`] codepoints, or a page that has no
    /// room for another and cannot grow, drops `c`.
    fn join(&mut self, cluster: Cluster, c: char) {
        let Cluster { x, y } = cluster;
        let unicode = self.widths.mode == Mode::Unicode;
        // Without a normal form to put the cluster in, its width is read
        // from the page that the codepoint went into.
        let widths = (unicode && self.normalization == Normalization::None).then_some(self.widths);
        let rows = &mut self.screen_mut().rows;
        let joined = loop {
            let (page, row) = rows.screen_row_mut(y);
            if page.grapheme(row, x).len() + 1 >= MAX_CLUSTER {
                break None;
            }
            match page.push_codepoint(row, x, c) {
                Ok(()) => break Some(widths.map(|widths| joined_width(page, row, x, widths, c))),
                Err(full) => {
                    if rows.grow_page(y, full).is_err() {
                        break None;
                    }
                }
            }
        };
        let width = match joined {
            None => None,
            Some(Some(width)) => width,
            Some(None) => {
                self.normalize(cluster);
                if unicode {
                    let (page, row) = self.screen().rows.screen_row(y);
                    new_width(page, row, x, self.widths)
                } else {
                    None
                }
            }
        };
        match width {
            Some(Width::Wide) => self.widen(cluster),
            Some(_) => self.narrow(cluster),
            None => {}
        }
        self.cluster = Some(cluster);
    }

    /// Makes the narrow cluster at `cluster` wide, if its line has room for
    /// it, and moves the cursor past it if it was right after it.
    fn widen(&mut self, cluster: Cluster) {
        let Cluster { x, y } = cluster;
        let end = self.line_end(x);
        if x + 1 >= end {
            return;
        }
        // In insert mode, the second half pushes the rest of the line
        // right, as the cluster's first half did.
        if self.insert {
            self.screen_mut().rows.insert_blanks(y, x + 1, 1, end);
        }
        let start = self.margins.left;
        let (page, row) = self.screen_mut().rows.screen_row_mut(y);
        let head = page.cells(row)[usize::from(x)];
        // The second half goes over what was there, as a write would.
        let over_wide = y > 0 && over_wide_start(page.cells(row), x + 1, 1, start);
        page.clear_halves(row, x + 1, 1);
        page.set_width(row, x, Width::Wide);
        // The second half has the style, the protection, the zone and the
        // link of the first.
        let tail = Cell::EMPTY
            .with_width(Width::SpacerTail)
            .with_style(head.style())
            .with_protected(head.is_protected())
            .with_semantic(head.semantic());
        page.set_cell(row, x + 1, tail);
        if over_wide {
            self.clear_spacer_head_above(y, None);
        }
        if head.has_hyperlink() {
            self.share_link(x, y);
        }
        let cursor = &mut self.screen_mut().cursor;
        if cursor.y == y && cursor.x == x + 1 && !cursor.pending_wrap {
            if x + 2 >= end {
                cursor.pending_wrap = true;
            } else {
                cursor.x = x + 2;
            }
        }
    }

    /// Makes the wide cluster at `cluster` narrow, and moves the cursor back
    /// if it was right after it. The second half becomes a blank with the
    /// cluster's style, protection and zone; in insert mode, it goes, and
    /// the rest of the row moves back left, as widening moved it right.
    fn narrow(&mut self, cluster: Cluster) {
        let Cluster { x, y } = cluster;
        let end = self.line_end(x);
        let head = self.cell(x, y);
        {
            let (page, row) = self.screen_mut().rows.screen_row_mut(y);
            page.set_width(row, x, Width::Narrow);
        }
        let blank = Cell::EMPTY
            .with_style(head.style())
            .with_protected(head.is_protected())
            .with_semantic(head.semantic());
        self.put(x + 1, y, blank);
        // Past the line's end, as when a margin moved over the cluster's
        // second half, the line has nothing to move back.
        if self.insert && x + 1 < end {
            self.screen_mut().rows.delete_cells(y, x + 1, 1, end);
        }
        let cursor = &mut self.screen_mut().cursor;
        // A wrap pends at the end of the cursor's line, which can be past a
        // margin that the cluster's line ends at: on the freed half, the
        // cursor is right after the cluster.
        let right_after = if cursor.pending_wrap {
            cursor.x == x + 1
        } else {
            cursor.x == x + 2
        };
        if cursor.y == y && right_after {
            cursor.x = x + 1;
            cursor.pending_wrap = false;
        }
    }

    /// Puts the cluster at `cluster` in the terminal's normal form.
    fn normalize(&mut self, cluster: Cluster) {
        if self.normalization == Normalization::None {
            return;
        }
        let Cluster { x, y } = cluster;
        let (mut codepoints, mut normal) = std::mem::take(&mut self.scratch);
        {
            let (page, row) = self.screen().rows.screen_row(y);
            let (Content::Char(first) | Content::Grapheme(first)) =
                page.cells(row)[usize::from(x)].content()
            else {
                self.scratch = (codepoints, normal);
                return;
            };
            codepoints.clear();
            codepoints.push(first);
            codepoints.extend_from_slice(page.grapheme(row, x));
        }
        if self.normalization.normalize(&codepoints, &mut normal) {
            // A decomposition can make the cluster longer than it keeps.
            normal.truncate(MAX_CLUSTER);
            let rows = &mut self.screen_mut().rows;
            loop {
                let (page, row) = rows.screen_row_mut(y);
                match page.set_cluster(row, x, normal[0], &normal[1..]) {
                    Ok(()) => break,
                    // The cluster keeps the codepoints that fit.
                    Err(full) => {
                        if rows.grow_page(y, full).is_err() {
                            break;
                        }
                    }
                }
            }
        }
        self.scratch = (codepoints, normal);
    }

    /// The cell left of the cursor, or under it while the wrap is pending,
    /// if it holds a character.
    fn character_left_of_cursor(&self) -> Option<Cluster> {
        let cursor = self.cursor();
        let mut x = if cursor.pending_wrap {
            cursor.x
        } else {
            cursor.x.checked_sub(1)?
        };
        if self.cell(x, cursor.y).width() == Width::SpacerTail {
            x = x.checked_sub(1)?;
        }
        match self.cell(x, cursor.y).content() {
            Content::Char('\0') => None,
            Content::Char(_) | Content::Grapheme(_) => Some(Cluster { x, y: cursor.y }),
            Content::BgPalette(_) | Content::BgRgb(_) => None,
        }
    }

    /// Goes on to the left margin of the next row, as text does past the
    /// end of its line, and marks the rows as one line when they make one.
    /// The cluster printed last ends: a new one is about to start, and
    /// scrolling may have moved the row it was in.
    pub(super) fn wrap(&mut self) {
        self.cluster = None;
        let y = self.cursor().y;
        let joins_rows = self.wrap_joins_rows();
        // Marked before the index, which may scroll the row away.
        if joins_rows {
            self.row_flag(y, RowFlag::SoftWrapped);
        }
        self.index();
        self.screen_mut().cursor.x = self.margins.left;
        if joins_rows {
            let y = self.cursor().y;
            self.row_flag(y, RowFlag::Continuation);
        }
    }

    /// Whether a wrap from the cursor makes its row and the next one line,
    /// for reflow: only one from the last column of the screen to the first
    /// one of another row. Where the index does not move, at the screen's
    /// last row below the scroll region or at the bottom margin right of
    /// the right margin, the text goes on in the same row.
    fn wrap_joins_rows(&self) -> bool {
        self.cursor().x == self.cols - 1 && self.margins.left == 0 && self.index_moves_on()
    }

    fn row_flag(&mut self, y: u16, flag: RowFlag) {
        let (page, row) = self.screen_mut().rows.screen_row_mut(y);
        page.row_mut(row).set(flag, true);
    }

    /// Ends row `y`'s soft wrap: it no longer goes on into the next row,
    /// which is no longer its continuation.
    pub(super) fn end_soft_wrap(&mut self, y: u16) {
        let rows = &mut self.screen_mut().rows;
        let (page, row) = rows.screen_row_mut(y);
        page.row_mut(row).set(RowFlag::SoftWrapped, false);
        if y + 1 < rows.rows() {
            let (page, row) = rows.screen_row_mut(y + 1);
            page.row_mut(row).set(RowFlag::Continuation, false);
        }
    }

    /// Empties the spacer heads at the end of the line on the row above `y`
    /// that the wrap there left for a wide character at the left margin of
    /// row `y`, where wraps land: for a write that goes over one in the two
    /// columns from the left margin, as [`over_wide_start`] tells, and as
    /// `alacritty_terminal` does. The head at the right margin, and the one
    /// at the screen's edge when the row above soft-wraps into row `y` and
    /// there is no left margin, as only a wrap from the screen's last column
    /// to the first marks the rows, and a head that a wrap from there leaves
    /// without joining them ends the mark; but not the one at `own`, which
    /// the character written has just left. A head that a wrap left at
    /// another right margin stays. A head in the scrollback stays, where
    /// `alacritty_terminal` clears it: the scrollback's rows are not written
    /// to, as their pages can be kept cold.
    fn clear_spacer_head_above(&mut self, y: u16, own: Option<u16>) {
        let (margin, edge) = (self.margins.right, self.cols - 1);
        let first_column = self.margins.left == 0;
        let rows = &mut self.screen_mut().rows;
        let continues = {
            let (page, row) = rows.screen_row(y);
            page.row(row).has(RowFlag::Continuation)
        };
        let (page, row) = rows.screen_row_mut(y - 1);
        let wraps = first_column && continues && page.row(row).has(RowFlag::SoftWrapped);
        for last in [margin, edge] {
            let looked_at = last == margin || wraps;
            if looked_at
                && own != Some(last)
                && page.cells(row)[usize::from(last)].width() == Width::SpacerHead
            {
                page.set_cell(row, last, Cell::EMPTY);
            }
        }
    }

    /// Empties the halves of wide characters that writing `width` cells at
    /// column `x` of row `y` would leave without their other half, and the
    /// spacer head above a wide character at the left margin that they go
    /// over, as the other writes do: a spacer head on a line of two
    /// columns, or a cluster that widens over the left margin.
    fn clear_overlaps(&mut self, x: u16, y: u16, width: u16) {
        let start = self.margins.left;
        let (page, row) = self.screen_mut().rows.screen_row_mut(y);
        let over_wide = y > 0 && over_wide_start(page.cells(row), x, width, start);
        page.clear_halves(row, x, width);
        if over_wide {
            self.clear_spacer_head_above(y, None);
        }
    }

    /// Makes the `width` cells from column `x` of row `y` part of the open
    /// link, in their page's table, which grows when it is full. The cells
    /// of a page that cannot grow stay out of the link.
    fn link_cells(&mut self, x: u16, y: u16, width: u16) {
        let link = std::mem::take(&mut self.link);
        let target = Hyperlink {
            id: &link.id,
            uri: &link.uri,
        };
        let rows = &mut self.screen_mut().rows;
        loop {
            let (page, row) = rows.screen_row_mut(y);
            let id = match page.hyperlinks_mut().acquire(target) {
                Ok(id) => id,
                Err(full) => {
                    if rows.grow_page(y, full).is_err() {
                        break;
                    }
                    continue;
                }
            };
            let linked = (x..x + width).try_for_each(|x| page.set_hyperlink(row, x, Some(id)));
            page.hyperlinks_mut().release(id);
            match linked {
                Ok(()) => break,
                Err(full) => {
                    if rows.grow_page(y, full).is_err() {
                        break;
                    }
                }
            }
        }
        self.link = link;
    }

    /// Makes the second half of the wide cluster at column `x` of row `y`
    /// part of the first half's link.
    fn share_link(&mut self, x: u16, y: u16) {
        let rows = &mut self.screen_mut().rows;
        loop {
            let (page, row) = rows.screen_row_mut(y);
            let Some(id) = page.hyperlink(row, x) else {
                return;
            };
            match page.set_hyperlink(row, x + 1, Some(id)) {
                Ok(()) => return,
                Err(full) => {
                    if rows.grow_page(y, full).is_err() {
                        return;
                    }
                }
            }
        }
    }

    /// Runs `write` on the page of row `y`, with the row in it and the
    /// pen's style in its table, which the pen holds a reference to while
    /// `write` runs. A full table makes the page grow first, and a page
    /// that cannot grow writes in the default style. The page is looked up
    /// once, unless it grows.
    pub(super) fn with_pen(&mut self, y: u16, write: impl FnOnce(&mut Page, u16, StyleId)) {
        debug_assert_eq!(self.pen_default, self.pen == Style::DEFAULT);
        let (pen, last, default) = (self.pen, self.pen_id, self.pen_default);
        let rows = &mut self.screen_mut().rows;
        let (mut page, mut row) = rows.screen_row_mut(y);
        let style = if default {
            StyleId::DEFAULT
        } else if page.styles().holds(last, &pen) {
            page.styles_mut().retain(last);
            last
        } else {
            loop {
                match page.styles_mut().acquire(&pen) {
                    Ok(style) => break style,
                    Err(full) => {
                        let grown = rows.grow_page(y, full);
                        (page, row) = rows.screen_row_mut(y);
                        if grown.is_err() {
                            break StyleId::DEFAULT;
                        }
                    }
                }
            }
        };
        write(page, row, style);
        page.styles_mut().release(style);
        self.pen_id = style;
    }
}
