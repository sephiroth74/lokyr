//! Printing: the path that every character of text takes into the grid.

use super::{Cluster, Cursor, Terminal};
use crate::page::{Cell, Content, Hyperlink, Page, RowFlag, StyleId, Width};
use crate::unicode::{GraphemeBreak, Mode, Normalization, properties};

/// The most codepoints that a cluster keeps; those that would join it past
/// them are dropped. Unicode's stream-safe text format allows thirty
/// combining marks in a row (<https://www.unicode.org/reports/tr15/#Stream_Safe_Text_Format>),
/// and the longest emoji sequences have about ten codepoints. The bound
/// keeps a cluster's work, normalization included, small.
const MAX_CLUSTER: usize = 32;

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
            let written = self.write_ascii(text);
            text = &text[written..];
        }
        self.last_printed = Some(char::from(last));
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
        self.with_pen(y, |page, row, style| {
            if !insert {
                page.clear_halves(row, x, width);
            }
            let template = Cell::EMPTY
                .with_style(style)
                .with_protected(protected)
                .with_semantic(zone);
            page.write_ascii(row, x, text, template);
        });
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
        if width == 2 && x == end - 1 {
            if self.autowrap {
                self.clear_overlaps(x, y, 1);
                self.put(x, y, Cell::EMPTY.with_width(Width::SpacerHead));
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
        self.with_pen(y, |page, row, style| {
            if !insert {
                page.clear_halves(row, x, width);
            }
            let blank = Cell::EMPTY
                .with_style(style)
                .with_protected(protected)
                .with_semantic(zone);
            let cell = blank.with_content(Content::Char(c));
            if width == 2 {
                page.set_cell(row, x, cell.with_width(Width::Wide));
                page.set_cell(row, x + 1, blank.with_width(Width::SpacerTail));
            } else {
                page.set_cell(row, x, cell);
            }
        });
        if self.link.is_open() {
            self.link_cells(x, y, width);
        }
        self.cluster = Some(Cluster { x, y });
        self.normalize(Cluster { x, y });

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
        let rows = &mut self.screen_mut().rows;
        let joined = loop {
            let (page, row) = rows.screen_row_mut(y);
            if page.grapheme(row, x).len() + 1 >= MAX_CLUSTER {
                break false;
            }
            match page.push_codepoint(row, x, c) {
                Ok(()) => break true,
                Err(full) => {
                    if rows.grow_page(y, full).is_err() {
                        break false;
                    }
                }
            }
        };
        if !joined {
            self.cluster = Some(cluster);
            return;
        }
        self.normalize(cluster);
        if self.widths.mode == Mode::Unicode {
            let (page, row) = self.screen().rows.screen_row(y);
            let cell = page.cells(row)[usize::from(x)];
            let (Content::Char(first) | Content::Grapheme(first)) = cell.content() else {
                return;
            };
            let width = self.widths.cluster_width(first, page.grapheme(row, x));
            match (cell.width(), width) {
                (Width::Narrow, 2) => self.widen(cluster),
                (Width::Wide, 1) => self.narrow(cluster),
                _ => {}
            }
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
        let head = self.cell(x, y);
        // In insert mode, the second half pushes the rest of the line
        // right, as the cluster's first half did.
        if self.insert {
            self.screen_mut().rows.insert_blanks(y, x + 1, 1, end);
        }
        self.clear_overlaps(x + 1, y, 1);
        {
            let (page, row) = self.screen_mut().rows.screen_row_mut(y);
            page.set_width(row, x, Width::Wide);
        }
        // The second half has the style, the protection, the zone and the
        // link of the first.
        let tail = Cell::EMPTY
            .with_width(Width::SpacerTail)
            .with_style(head.style())
            .with_protected(head.is_protected())
            .with_semantic(head.semantic());
        self.put(x + 1, y, tail);
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
    fn wrap(&mut self) {
        self.cluster = None;
        let Cursor { x, y, .. } = self.cursor();
        // Only a wrap from the last column of the screen to the first one
        // of another row makes the two rows one line, for reflow. Below the
        // scroll region, or right of the right margin, the text goes on in
        // the same row.
        let joins_rows = x == self.cols - 1 && self.margins.left == 0 && self.index_moves_on();
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

    fn row_flag(&mut self, y: u16, flag: RowFlag) {
        let (page, row) = self.screen_mut().rows.screen_row_mut(y);
        page.row_mut(row).set(flag, true);
    }

    /// Empties the halves of wide characters that writing `width` cells at
    /// column `x` of row `y` would leave without their other half.
    fn clear_overlaps(&mut self, x: u16, y: u16, width: u16) {
        let (page, row) = self.screen_mut().rows.screen_row_mut(y);
        page.clear_halves(row, x, width);
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
    fn with_pen(&mut self, y: u16, write: impl FnOnce(&mut Page, u16, StyleId)) {
        let pen = self.pen;
        let rows = &mut self.screen_mut().rows;
        let (mut page, mut row) = rows.screen_row_mut(y);
        let style = loop {
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
        };
        write(page, row, style);
        page.styles_mut().release(style);
    }
}
