//! Printing: the path that every character of text takes into the grid.

use super::{Cluster, Cursor, Terminal};
use crate::page::{Cell, Content, Page, RowFlag, StyleId, Width};
use crate::unicode::{Mode, Normalization, properties};

impl Terminal {
    /// Prints `c`: it joins the cluster before it, or starts one in the
    /// cells at the cursor.
    pub(super) fn print_char(&mut self, c: char) {
        let properties = properties(c);
        let width = self.widths.char_width_with(c, properties);
        let joins = match self.widths.mode {
            Mode::Unicode => !self.segmenter.starts_cluster_with(properties),
            Mode::Legacy => width == 0,
        };
        if joins || width == 0 {
            self.join_previous(c);
            return;
        }
        self.start_cluster(c, width);
    }

    /// Prints `text`, a run of printable ASCII, as [`Terminal::print_char`]
    /// would print its characters one by one, but a stretch of a row at a
    /// time: each of them takes one cell, starts a cluster and is in normal
    /// form already.
    pub(super) fn print_ascii_run(&mut self, text: &[u8]) {
        let Some((&first, rest)) = text.split_first() else {
            return;
        };
        let mut text = text;
        if self.widths.mode == Mode::Unicode && !self.segmenter.ascii_starts_cluster() {
            self.join_previous(char::from(first));
            text = rest;
        }
        while !text.is_empty() {
            let written = self.write_ascii(text);
            text = &text[written..];
        }
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
    /// end of its row, and returns how many bytes that took.
    fn write_ascii(&mut self, text: &[u8]) -> usize {
        let cols = self.cols;
        if self.cursor().pending_wrap && self.autowrap {
            self.wrap();
        }
        let Cursor { x, y, pending_wrap } = self.cursor();
        if pending_wrap {
            // Without autowrap, each character replaces the one in the last
            // column, so only the last of them stays.
            self.write_cells(x, y, &text[text.len() - 1..]);
            self.cluster = Some(Cluster { x, y });
            return text.len();
        }
        let len = text.len().min(usize::from(cols - x));
        // The row has room for `len` cells, so it fits in a u16.
        let width = len as u16;
        self.write_cells(x, y, &text[..len]);
        self.cluster = Some(Cluster {
            x: x + width - 1,
            y,
        });
        let cursor = &mut self.screen_mut().cursor;
        if x + width >= cols {
            cursor.x = cols - 1;
            cursor.pending_wrap = true;
        } else {
            cursor.x = x + width;
        }
        len
    }

    /// Writes `text`, printable ASCII that fits in the row, into the cells
    /// from column `x` of row `y`, with the pen's style, as
    /// [`Terminal::start_cluster`] writes a character.
    fn write_cells(&mut self, x: u16, y: u16, text: &[u8]) {
        let width = text.len() as u16;
        let insert = self.insert;
        self.with_pen(y, |page, row, style| {
            if insert {
                page.insert_blanks(row, x, width);
            } else {
                page.clear_halves(row, x, width);
            }
            page.write_ascii(row, x, text, style);
        });
    }

    /// Writes a new cluster, `c`, which takes `width` cells, at the cursor.
    fn start_cluster(&mut self, c: char, width: u8) {
        let cols = self.cols;
        let width = u16::from(width);
        if self.cursor().pending_wrap && self.autowrap {
            self.wrap();
        }
        // A wide character does not fit in the last column.
        if width == 2 && self.cursor().x == cols - 1 {
            if cols < 2 {
                return;
            }
            if self.autowrap {
                let y = self.cursor().y;
                self.clear_overlaps(cols - 1, y, 1);
                self.put(cols - 1, y, Cell::EMPTY.with_width(Width::SpacerHead));
                self.wrap();
            } else {
                self.screen_mut().cursor.x = cols - 2;
            }
        }

        let (x, y) = (self.cursor().x, self.cursor().y);
        let insert = self.insert;
        self.with_pen(y, |page, row, style| {
            if insert {
                page.insert_blanks(row, x, width);
            } else {
                page.clear_halves(row, x, width);
            }
            let cell = Cell::EMPTY.with_content(Content::Char(c)).with_style(style);
            if width == 2 {
                page.set_cell(row, x, cell.with_width(Width::Wide));
                let tail = Cell::EMPTY.with_width(Width::SpacerTail).with_style(style);
                page.set_cell(row, x + 1, tail);
            } else {
                page.set_cell(row, x, cell);
            }
        });
        self.cluster = Some(Cluster { x, y });
        self.normalize(Cluster { x, y });

        let cursor = &mut self.screen_mut().cursor;
        if x + width >= cols {
            cursor.x = cols - 1;
            cursor.pending_wrap = true;
        } else {
            cursor.x = x + width;
            cursor.pending_wrap = false;
        }
    }

    /// Adds `c` to the cluster at `cluster`, whose width can change.
    fn join(&mut self, cluster: Cluster, c: char) {
        let Cluster { x, y } = cluster;
        let rows = &mut self.screen_mut().rows;
        loop {
            let (page, row) = rows.screen_row_mut(y);
            match page.push_codepoint(row, x, c) {
                Ok(()) => break,
                Err(full) => rows.grow_page(y, full),
            }
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

    /// Makes the narrow cluster at `cluster` wide, if the row has room for
    /// it, and moves the cursor past it if it was right after it.
    fn widen(&mut self, cluster: Cluster) {
        let Cluster { x, y } = cluster;
        let cols = self.cols;
        if x + 1 >= cols {
            return;
        }
        let style = self.cell(x, y).style();
        self.clear_overlaps(x + 1, y, 1);
        {
            let (page, row) = self.screen_mut().rows.screen_row_mut(y);
            page.set_width(row, x, Width::Wide);
        }
        self.put(
            x + 1,
            y,
            Cell::EMPTY.with_width(Width::SpacerTail).with_style(style),
        );
        let cursor = &mut self.screen_mut().cursor;
        if cursor.y == y && cursor.x == x + 1 && !cursor.pending_wrap {
            if x + 2 >= cols {
                cursor.pending_wrap = true;
            } else {
                cursor.x = x + 2;
            }
        }
    }

    /// Makes the wide cluster at `cluster` narrow, and moves the cursor back
    /// if it was right after it.
    fn narrow(&mut self, cluster: Cluster) {
        let Cluster { x, y } = cluster;
        let cols = self.cols;
        {
            let (page, row) = self.screen_mut().rows.screen_row_mut(y);
            page.set_width(row, x, Width::Narrow);
        }
        self.put(x + 1, y, Cell::EMPTY);
        let cursor = &mut self.screen_mut().cursor;
        let right_after = if cursor.pending_wrap {
            x + 2 == cols && cursor.x == cols - 1
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
            let rows = &mut self.screen_mut().rows;
            loop {
                let (page, row) = rows.screen_row_mut(y);
                match page.set_cluster(row, x, normal[0], &normal[1..]) {
                    Ok(()) => break,
                    Err(full) => rows.grow_page(y, full),
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

    /// Goes on to the start of the next row, as text does past the right
    /// margin, and marks the rows as one line.
    fn wrap(&mut self) {
        let y = self.cursor().y;
        self.row_flag(y, RowFlag::SoftWrapped);
        self.index();
        self.screen_mut().cursor.x = 0;
        let y = self.cursor().y;
        self.row_flag(y, RowFlag::Continuation);
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

    /// Runs `write` on the page of row `y`, with the row in it and the
    /// pen's style in its table, which the pen holds a reference to while
    /// `write` runs. A full table makes the page grow first. The page is
    /// looked up once, unless it grows.
    fn with_pen(&mut self, y: u16, write: impl FnOnce(&mut Page, u16, StyleId)) {
        let pen = self.pen;
        let rows = &mut self.screen_mut().rows;
        let (mut page, mut row) = rows.screen_row_mut(y);
        let style = loop {
            match page.styles_mut().acquire(&pen) {
                Ok(style) => break style,
                Err(full) => {
                    rows.grow_page(y, full);
                    (page, row) = rows.screen_row_mut(y);
                }
            }
        };
        write(page, row, style);
        page.styles_mut().release(style);
    }
}
