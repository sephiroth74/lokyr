//! Printing: the path that every character of text takes into the grid.

use super::{Cluster, Terminal};
use crate::page::{Cell, Content, RowFlag, StyleId, Width};
use crate::unicode::{Mode, Normalization};

impl Terminal {
    /// Prints `c`: it joins the cluster before it, or starts one in the
    /// cells at the cursor.
    pub(super) fn print_char(&mut self, c: char) {
        let joins = match self.widths.mode {
            Mode::Unicode => !self.segmenter.starts_cluster(c),
            Mode::Legacy => self.widths.char_width(c) == 0,
        };
        let width = self.widths.char_width(c);
        if joins || width == 0 {
            // A codepoint of no width that starts a cluster, such as a mark
            // after the cursor moved, joins the character left of the
            // cursor; with none, it is dropped.
            if let Some(cluster) = self.cluster.or_else(|| self.character_left_of_cursor()) {
                self.join(cluster, c);
            }
            return;
        }
        self.start_cluster(c, width);
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
        if self.insert {
            self.insert_blanks(x, y, width);
        } else {
            self.clear_overlaps(x, y, width);
        }
        let style = self.pen_style(y);
        let cell = Cell::EMPTY.with_content(Content::Char(c)).with_style(style);
        if width == 2 {
            self.put(x, y, cell.with_width(Width::Wide));
            self.put(
                x + 1,
                y,
                Cell::EMPTY.with_width(Width::SpacerTail).with_style(style),
            );
        } else {
            self.put(x, y, cell);
        }
        self.release_pen_style(y, style);
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
        if self.cell(x, y).width() == Width::SpacerTail && x > 0 {
            self.put(x - 1, y, Cell::EMPTY);
        }
        let last = x + width - 1;
        if self.cell(last, y).width() == Width::Wide && last + 1 < self.cols {
            self.put(last + 1, y, Cell::EMPTY);
        }
    }

    /// Makes room for `width` cells at column `x` of row `y`, as insert mode
    /// does, without splitting wide characters.
    fn insert_blanks(&mut self, x: u16, y: u16, width: u16) {
        let cols = self.cols;
        if self.cell(x, y).width() == Width::SpacerTail {
            self.put(x - 1, y, Cell::EMPTY);
            self.put(x, y, Cell::EMPTY);
        }
        {
            let (page, row) = self.screen_mut().rows.screen_row_mut(y);
            page.insert_blanks(row, x, width);
        }
        // A wide character whose second half fell off the row.
        if self.cell(cols - 1, y).width() == Width::Wide {
            self.put(cols - 1, y, Cell::EMPTY);
        }
    }

    /// The pen's style in the table of the page of row `y`, which the pen
    /// holds a reference to until [`Terminal::release_pen_style`]. A full
    /// table makes the page grow.
    fn pen_style(&mut self, y: u16) -> StyleId {
        let pen = self.pen;
        let rows = &mut self.screen_mut().rows;
        loop {
            let (page, _) = rows.screen_row_mut(y);
            match page.styles_mut().acquire(&pen) {
                Ok(id) => return id,
                Err(full) => rows.grow_page(y, full),
            }
        }
    }

    fn release_pen_style(&mut self, y: u16, style: StyleId) {
        let (page, _) = self.screen_mut().rows.screen_row_mut(y);
        page.styles_mut().release(style);
    }
}
