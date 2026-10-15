//! The page list: the pages of a screen and of its scrollback, oldest
//! first, with a pool of spare pages, within limits in bytes.

use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::pin::{Pins, Place};
use super::{Capacity, Cell, Page, PinId, Position, Row, RowFlag, TableFull, WhenFull, Width};

/// No node, at either end of the list.
const NONE: u32 = u32::MAX;

/// The bytes that the pages of a pane take by default.
pub const SCROLLBACK_BYTES: usize = 10 * 1024 * 1024;

/// The bytes that the pages of every pane together take by default.
pub const SCROLLBACK_BUDGET_BYTES: usize = 100 * 1024 * 1024;

/// Memory that the page lists of several panes share, from any thread.
///
/// Each list counts its pages in it. A list that needs a page past the
/// budget makes room by letting its own oldest pages go: a busy pane never
/// takes the scrollback of an idle one.
///
/// Only the pages that screens need can pass the budget, since a screen
/// always gets its rows. A list that adds a page while the budget is passed
/// gives back its own scrollback first, until the budget is met again.
#[derive(Debug)]
pub struct Budget {
    limit: usize,
    used: AtomicUsize,
}

impl Budget {
    /// A budget of `limit` bytes, none of them used.
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            used: AtomicUsize::new(0),
        }
    }

    /// The bytes that the lists may take together.
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// The bytes that the lists take now.
    pub fn used(&self) -> usize {
        self.used.load(Ordering::Relaxed)
    }

    /// Takes `bytes` if they fit.
    fn try_take(&self, bytes: usize) -> bool {
        self.used
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |used| {
                used.checked_add(bytes).filter(|&total| total <= self.limit)
            })
            .is_ok()
    }

    /// Takes `bytes` whether they fit or not, for the pages of a screen.
    fn take(&self, bytes: usize) {
        self.used.fetch_add(bytes, Ordering::Relaxed);
    }

    fn give(&self, bytes: usize) {
        self.used.fetch_sub(bytes, Ordering::Relaxed);
    }

    fn is_over(&self) -> bool {
        self.used() > self.limit
    }
}

/// Where a screen's new rows come from when it grows taller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grow {
    /// From the scrollback, as far as it has rows, as xterm does, then
    /// empty rows at the bottom.
    FromScrollback,
    /// Empty rows at the bottom, for a screen whose scrollback is not shown,
    /// such as the alternate screen.
    AtTheBottom,
}

/// Where a rewrap has got to in the line it writes.
struct Writer {
    cols: u16,
    /// The column that the next cell goes to, in the last row.
    x: u16,
    /// Where the line's last cell went, if it has one: its node, row and
    /// column.
    last: Option<(u32, u16, u16)>,
    /// Pins of spacer heads, which go with the next cell written.
    carried: Vec<u32>,
}

/// Whether every cell of row `row` of `page` is empty.
fn page_row_is_empty(page: &Page, row: u16) -> bool {
    page.cells(row).iter().all(|cell| cell.is_empty())
}

/// What the pages of a list may take.
#[derive(Clone, Debug)]
pub struct Scrollback {
    /// The bytes of the list's pages, those of its screen and of its pool
    /// included: [`SCROLLBACK_BYTES`] by default.
    pub limit: usize,
    /// A budget that the lists of several panes share, if any.
    pub budget: Option<Arc<Budget>>,
}

impl Default for Scrollback {
    fn default() -> Self {
        Self {
            limit: SCROLLBACK_BYTES,
            budget: None,
        }
    }
}

/// A page of a [`PageList`], stable while the page stays in the list.
///
/// When the page leaves the list, its id goes stale rather than naming
/// another page: [`PageList::page`] returns `None` for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PageId {
    index: u32,
    generation: u32,
}

/// A slot of the slab.
#[derive(Debug)]
struct Node {
    /// `None` while the slot is free.
    page: Option<Page>,
    prev: u32,
    next: u32,
    /// How many pages the slot has held, so that ids go stale.
    generation: u32,
    /// The pins in the page.
    pins: Vec<u32>,
}

/// The rows of a screen and of its scrollback, in pages of one width.
///
/// The pages form a doubly linked list of indexes into a slab, oldest
/// first. The last `rows` rows are the active screen; the rows above them
/// are the scrollback. Scrolling adds an empty row at the bottom.
///
/// The pages stay within the [`Scrollback`] limits. When a new page would
/// pass them, the oldest page leaves the list first: it is emptied and kept
/// in a pool, from which the new page then comes. A page that grew past the
/// standard size is freed instead. The screen never leaves, even when its
/// pages alone pass the limits.
///
/// Once the pool and the slab have reached their size, scrolling allocates
/// nothing.
///
/// Pins follow the cells they were put on: when rows or cells move, their
/// pins move with them, and when they leave the list, or a move drops
/// them, their pins are lost. Erasing a cell where it is leaves its pins.
#[derive(Debug)]
pub struct PageList {
    nodes: Vec<Node>,
    /// The free slots of the slab.
    free: Vec<u32>,
    head: u32,
    tail: u32,
    /// How many pages are in the list.
    pages: usize,
    /// Empty pages of the standard capacity, to use again.
    pool: Vec<Page>,
    /// The capacity of a standard page of the list's width.
    capacity: Capacity,
    /// The rows of the active screen.
    rows: u16,
    /// The rows in the list: the scrollback and the screen.
    total_rows: usize,
    scrollback: Scrollback,
    /// The bytes of the pages, in the list and in the pool.
    memory: usize,
    pins: Pins,
}

impl PageList {
    /// A list of `cols` columns, whose screen has `rows` empty rows, with
    /// pages within `scrollback`.
    pub fn new(cols: u16, rows: u16, scrollback: Scrollback) -> Self {
        let capacity = Capacity::standard(cols);
        let mut list = Self {
            nodes: Vec::new(),
            free: Vec::new(),
            head: NONE,
            tail: NONE,
            pages: 0,
            pool: Vec::new(),
            capacity,
            rows,
            total_rows: 0,
            scrollback,
            memory: 0,
            pins: Pins::default(),
        };
        for _ in 0..rows {
            list.push_row();
        }
        list
    }

    /// How many cells each row has.
    pub fn cols(&self) -> u16 {
        self.capacity.cols
    }

    /// How many rows the active screen has.
    pub fn rows(&self) -> u16 {
        self.rows
    }

    /// How many rows the list holds: the scrollback and the screen.
    pub fn total_rows(&self) -> usize {
        self.total_rows
    }

    /// How many rows are in the scrollback, above the screen.
    pub fn scrollback_rows(&self) -> usize {
        self.total_rows - usize::from(self.rows)
    }

    /// How many pages are in the list.
    pub fn pages(&self) -> usize {
        self.pages
    }

    /// How many spare pages are in the pool.
    pub fn pool_len(&self) -> usize {
        self.pool.len()
    }

    /// The limits that the pages stay within.
    pub fn scrollback(&self) -> &Scrollback {
        &self.scrollback
    }

    /// The bytes that the pages take, in the list and in the pool.
    pub fn memory(&self) -> usize {
        self.memory
    }

    /// Scrolls the screen up one row: its top row goes into the scrollback,
    /// and an empty row appears at the bottom.
    pub fn scroll_up(&mut self) {
        self.push_row();
    }

    /// The page that holds row `y` of the screen, and the row in it.
    pub fn screen_row(&self, y: u16) -> (&Page, u16) {
        let (index, row) = self.locate(y);
        (self.page_at(index), row)
    }

    /// The page that holds row `y` of the screen, to change, and the row in
    /// it.
    pub fn screen_row_mut(&mut self, y: u16) -> (&mut Page, u16) {
        let (index, row) = self.locate(y);
        (self.page_at_mut(index), row)
    }

    /// The id of the page that holds row `y` of the screen, and the row in
    /// it.
    pub fn screen_row_id(&self, y: u16) -> (PageId, u16) {
        let (index, row) = self.locate(y);
        (self.id(index), row)
    }

    /// The page with `id`, or `None` if it has left the list.
    pub fn page(&self, id: PageId) -> Option<&Page> {
        let node = self.nodes.get(id.index as usize)?;
        if node.generation == id.generation {
            node.page.as_ref()
        } else {
            None
        }
    }

    /// The page and the row in it of the row at `index` of the list, oldest
    /// first, if the list has that many.
    pub fn row_at(&self, index: usize) -> Option<(PageId, u16)> {
        let mut node = self.head;
        let mut start = 0;
        while node != NONE {
            let len = usize::from(self.page_at(node).len());
            if index < start + len {
                // Fewer rows than the page holds.
                return Some((self.id(node), (index - start) as u16));
            }
            start += len;
            node = self.nodes[node as usize].next;
        }
        None
    }

    /// The index in the list, oldest first, of row `row` of the page with
    /// `id`, if the page is in the list and has that row.
    pub fn row_index(&self, id: PageId, row: u16) -> Option<usize> {
        let page = self.page(id)?;
        if row >= page.len() {
            return None;
        }
        let mut node = self.head;
        let mut start = 0;
        while node != id.index {
            start += usize::from(self.page_at(node).len());
            node = self.nodes[node as usize].next;
        }
        Some(start + usize::from(row))
    }

    /// Pins the cell at `position`, if it is in the list.
    pub fn pin(&mut self, position: Position) -> Option<PinId> {
        let page = self.page(position.page)?;
        if position.row >= page.len() || position.col >= self.cols() {
            return None;
        }
        let node = position.page.index;
        let pin = self.pins.add(Place {
            node,
            row: position.row,
            col: position.col,
        });
        let index = self.pins.index(pin).expect("a new pin is in use");
        self.nodes[node as usize].pins.push(index);
        Some(pin)
    }

    /// Where `pin` is, or `None` if it is lost.
    pub fn pin_position(&self, pin: PinId) -> Option<Position> {
        let place = self.pins.place(pin)?;
        Some(Position {
            page: self.id(place.node),
            row: place.row,
            col: place.col,
        })
    }

    /// Where `pin` is on the screen, as a column and a row, if it is not
    /// lost and its row is on the screen.
    pub fn pin_on_screen(&self, pin: PinId) -> Option<(u16, u16)> {
        let position = self.pin_position(pin)?;
        let index = self.row_index(position.page, position.row)?;
        let y = index.checked_sub(self.scrollback_rows())?;
        // A row of the screen.
        Some((position.col, y as u16))
    }

    /// Lets go of `pin`, which goes stale.
    pub fn unpin(&mut self, pin: PinId) {
        let Some(place) = self.pins.place(pin) else {
            return;
        };
        let index = self.pins.index(pin).expect("a pin with a place is in use");
        let pins = &mut self.nodes[place.node as usize].pins;
        if let Some(at) = pins.iter().position(|&other| other == index) {
            pins.swap_remove(at);
        }
        self.pins.lose(index);
    }

    /// How many pins the list keeps track of.
    pub fn pins(&self) -> usize {
        self.pins.len()
    }

    /// Rebuilds the page that holds row `y` of the screen with room in the
    /// table that is `full`. Its rows keep their contents, and its id stays
    /// the same. If the larger page passes the limits, the oldest pages of
    /// the scrollback make room.
    ///
    /// A table that is as large as it can be does not grow: the page stays
    /// as it is, and the error says so, so that the caller writes without
    /// what the table has no room for rather than trying again.
    pub fn grow_page(&mut self, y: u16, full: TableFull) -> Result<(), TableFull> {
        let (index, _) = self.locate(y);
        self.grow_node(index, full)
    }

    /// Rebuilds the page of node `index`, as [`PageList::grow_page`] does.
    fn grow_node(&mut self, index: u32, full: TableFull) -> Result<(), TableFull> {
        let page = self.page_at(index);
        let capacity = page.capacity().grown_for(full).ok_or(full)?;
        let grown = page.grown(capacity);
        let added = grown.memory() - page.memory();
        self.nodes[index as usize].page = Some(grown);
        self.take(added);
        self.enforce();
        Ok(())
    }

    /// Moves row `from` of the screen into row `to`: its cells, with their
    /// styles, graphemes and links, and its metadata. What `to` held is
    /// dropped, and `from` is left empty.
    ///
    /// Within a page, the cells move as they are. Across pages, they are
    /// copied into the tables of the page of `to`, which grows when one of
    /// them is full; a page that cannot grow takes them without what does
    /// not fit.
    pub fn move_row(&mut self, from: u16, to: u16) {
        if from == to {
            return;
        }
        let (source, from_row) = self.locate(from);
        let (target, to_row) = self.locate(to);
        self.move_pins((source, from_row), (target, to_row), 0..self.cols());
        if source == target {
            self.page_at_mut(source).move_row(from_row, to_row);
            return;
        }
        let mut when_full = WhenFull::Fail;
        loop {
            let [source_node, target_node] = self
                .nodes
                .get_disjoint_mut([source as usize, target as usize])
                .expect("two nodes of the list");
            let source_page = source_node.page.as_mut().expect("a linked node has a page");
            let target_page = target_node.page.as_mut().expect("a linked node has a page");
            match target_page.copy_row(to_row, source_page, from_row, when_full) {
                Ok(()) => {
                    source_page.clear_row(from_row);
                    return;
                }
                Err(full) => {
                    if self.grow_page(to, full).is_err() {
                        when_full = WhenFull::Drop;
                    }
                }
            }
        }
    }

    /// Moves the cells of `columns` in row `from` of the screen into the
    /// same columns of row `to`, with their styles, graphemes and links, as
    /// the left and right margins need. What those cells of `to` held is
    /// dropped, and those of `from` are left empty; the rows' metadata
    /// stays. The wide characters that cross the edges of `columns`, in
    /// either row, are emptied whole first.
    pub fn move_cells(&mut self, from: u16, to: u16, columns: Range<u16>) {
        if from == to {
            return;
        }
        for y in [from, to] {
            let (page, row) = self.screen_row_mut(y);
            page.clear_crossing(row, columns.clone());
        }
        let (source, from_row) = self.locate(from);
        let (target, to_row) = self.locate(to);
        self.move_pins((source, from_row), (target, to_row), columns.clone());
        if source == target {
            self.page_at_mut(source)
                .move_cells(from_row, to_row, columns);
            return;
        }
        let mut when_full = WhenFull::Fail;
        loop {
            let [source_node, target_node] = self
                .nodes
                .get_disjoint_mut([source as usize, target as usize])
                .expect("two nodes of the list");
            let source_page = source_node.page.as_mut().expect("a linked node has a page");
            let target_page = target_node.page.as_mut().expect("a linked node has a page");
            match target_page.copy_cells(to_row, source_page, from_row, columns.clone(), when_full)
            {
                Ok(()) => {
                    source_page.clear_cells(from_row, columns);
                    return;
                }
                Err(full) => {
                    if self.grow_page(to, full).is_err() {
                        when_full = WhenFull::Drop;
                    }
                }
            }
        }
    }

    /// Gives the list rows of `cols` cells and a screen of `rows` rows,
    /// without rewrapping lines, and returns where the cell at `cursor`, a
    /// column and a row of the screen, goes.
    ///
    /// New columns are empty, and cut ones are dropped, with their pins: the
    /// pages are rebuilt at the new width, the old ones leaving the limits
    /// as the new ones come. A taller screen takes its new rows as `grow`
    /// says. A shorter one drops the empty rows below the cursor first, then
    /// sends rows from the top to the scrollback, as far as the cursor's
    /// row, and drops the rest from the bottom.
    pub fn resize(&mut self, cols: u16, rows: u16, cursor: (u16, u16), grow: Grow) -> (u16, u16) {
        let (mut x, mut y) = cursor;
        let (cols, rows) = (cols.max(1), rows.max(1));
        if cols != self.cols() {
            self.set_cols(cols);
            x = x.min(cols - 1);
        }
        if rows > self.rows {
            y += self.add_rows(rows - self.rows, grow);
        } else if rows < self.rows {
            y = self.remove_rows(self.rows - rows, y);
        }
        (x, y)
    }

    /// Gives the list rows of `cols` cells and a screen of `rows` rows,
    /// rewrapping lines, and returns where the cell at `cursor`, a column
    /// and a row of the screen, goes.
    ///
    /// A line, the rows that soft-wrap into each other, is written again at
    /// the new width: a wide character that the edge would split leaves a
    /// spacer head and goes on in the next row, and the empty cells that
    /// end a line are left out, but those before the cursor. Cells keep
    /// their styles, graphemes and links, and pins follow them; a pin on
    /// an empty cell left out goes after the line's text.
    ///
    /// When rewrapping adds rows, the empty rows below the cursor take
    /// them, as far as there are some, so that the text above stays on
    /// screen; the cursor always stays on screen, the rows below it going
    /// first. A taller or shorter screen then takes or gives rows as
    /// [`PageList::resize`] does with [`Grow::FromScrollback`].
    pub fn reflow(&mut self, cols: u16, rows: u16, cursor: (u16, u16)) -> (u16, u16) {
        let (cols, rows) = (cols.max(1), rows.max(1));
        let (mut x, mut y) = cursor;
        if cols != self.cols() {
            (x, y) = self.rewrap(cols, cursor);
        }
        if rows > self.rows {
            y += self.add_rows(rows - self.rows, Grow::FromScrollback);
        } else if rows < self.rows {
            y = self.remove_rows(self.rows - rows, y);
        }
        (x, y)
    }

    /// Takes the pages out of the list, oldest first, each with its pins
    /// sorted by cell, and makes the list empty, for rows of `cols` cells.
    /// The pages' ids go stale, and their bytes leave the limits now, so
    /// that the new pages have the room.
    fn take_pages(&mut self, cols: u16) -> Vec<(Page, Vec<u32>)> {
        let mut old = Vec::with_capacity(self.pages);
        let mut index = self.head;
        let mut bytes = 0;
        while index != NONE {
            let node = &mut self.nodes[index as usize];
            let page = node.page.take().expect("a linked node has a page");
            node.generation = node.generation.wrapping_add(1);
            let mut pins = std::mem::take(&mut node.pins);
            let next = node.next;
            let places = &self.pins;
            pins.sort_by_key(|&pin| places.place_of(pin).map(|place| (place.row, place.col)));
            bytes += page.memory();
            self.free.push(index);
            old.push((page, pins));
            index = next;
        }
        bytes += self.pool.drain(..).map(|page| page.memory()).sum::<usize>();
        self.give(bytes);
        (self.head, self.tail, self.pages, self.total_rows) = (NONE, NONE, 0, 0);
        self.capacity = Capacity::standard(cols);
        old
    }

    /// Writes the lines again at `cols` columns, and returns where the
    /// cursor goes, as [`PageList::reflow`] describes.
    fn rewrap(&mut self, cols: u16, cursor: (u16, u16)) -> (u16, u16) {
        let old_cols = self.cols();
        let old_total = self.total_rows;
        let cursor_row = self.scrollback_rows() + usize::from(cursor.1);
        let (page, row) = self.screen_row_id(cursor.1);
        let cursor_pin = self
            .pin(Position {
                page,
                row,
                col: cursor.0,
            })
            .expect("the cursor's cell is in the list");
        let mut writer = Writer {
            cols,
            x: 0,
            last: None,
            carried: Vec::new(),
        };
        let mut wrapped = false;
        let mut source_row = 0;
        for (page, pins) in self.take_pages(cols) {
            let mut pins = pins.into_iter().peekable();
            for row in 0..page.len() {
                let metadata = page.row(row);
                let cells = page.cells(row);
                let wraps = metadata.has(RowFlag::SoftWrapped);
                if !wrapped {
                    self.start_line(&mut writer, metadata);
                }
                let last = cells[usize::from(old_cols) - 1];
                let mut len = if wraps {
                    old_cols - u16::from(last.width() == Width::SpacerHead)
                } else {
                    // Fewer cells than a row holds.
                    cells
                        .iter()
                        .rposition(|cell| !cell.is_empty())
                        .map_or(0, |x| x as u16 + 1)
                };
                if source_row == cursor_row {
                    len = len.max(cursor.0 + 1);
                }
                let mut col = 0;
                while col < len {
                    let wide = cells[usize::from(col)].width() == Width::Wide;
                    let at = self.place(&mut writer, &page, row, col);
                    self.repin(&mut pins, row, col, at);
                    if wide && cols >= 2 {
                        self.repin(&mut pins, row, col + 1, (at.0, at.1, at.2 + 1));
                        col += 2;
                    } else {
                        col += 1;
                    }
                }
                // The pins past the text: a spacer head's go with the next
                // cell, and the others after the line's text.
                while let Some(&pin) = pins
                    .peek()
                    .filter(|&&pin| self.pins.place_of(pin).map(|p| p.row) == Some(row))
                {
                    pins.next();
                    if wraps {
                        writer.carried.push(pin);
                    } else {
                        let col = self.pins.place_of(pin).expect("a pin in use").col;
                        let at = self.after_text(&writer, col.saturating_sub(len));
                        self.move_pin(pin, at);
                    }
                }
                if !wraps {
                    for pin in std::mem::take(&mut writer.carried) {
                        let at = self.after_text(&writer, 0);
                        self.move_pin(pin, at);
                    }
                }
                wrapped = wraps;
                source_row += 1;
            }
        }
        self.settle_cursor(cursor_pin, old_total)
    }

    /// Starts a line in a row of its own, which takes the prompt mark of
    /// the line's first row.
    fn start_line(&mut self, writer: &mut Writer, metadata: Row) {
        self.push_row();
        writer.x = 0;
        writer.last = None;
        if metadata.has(RowFlag::PromptStart) {
            self.tail_row_flag(RowFlag::PromptStart);
        }
    }

    /// Goes on to a new row of the line being written.
    fn next_row(&mut self, writer: &mut Writer) {
        self.tail_row_flag(RowFlag::SoftWrapped);
        self.push_row();
        self.tail_row_flag(RowFlag::Continuation);
        writer.x = 0;
    }

    fn tail_row_flag(&mut self, flag: RowFlag) {
        let tail = self.tail;
        let page = self.page_at_mut(tail);
        let row = page.len() - 1;
        page.row_mut(row).set(flag, true);
    }

    /// Writes the cell at column `col` of row `row` of `source` where the
    /// line being written has got to, with its second half if it is wide,
    /// and returns where it went: its node, row and column.
    fn place(&mut self, writer: &mut Writer, source: &Page, row: u16, col: u16) -> (u32, u16, u16) {
        let cell = source.cells(row)[usize::from(col)];
        let wide = cell.width() == Width::Wide && writer.cols >= 2;
        if writer.x + 1 + u16::from(wide) > writer.cols {
            // A wide character that the edge would split leaves a spacer
            // head in the last column.
            if writer.x < writer.cols {
                self.set_tail_cell(writer.x, Cell::EMPTY.with_width(Width::SpacerHead));
            }
            self.next_row(writer);
        }
        let x = writer.x;
        match cell.width() {
            Width::Wide if wide => {
                self.copy_to_tail(source, row, col, x);
                let has_tail = source
                    .cells(row)
                    .get(usize::from(col) + 1)
                    .is_some_and(|cell| cell.width() == Width::SpacerTail);
                if has_tail {
                    self.copy_to_tail(source, row, col + 1, x + 1);
                } else {
                    self.set_tail_cell(x + 1, Cell::EMPTY.with_width(Width::SpacerTail));
                }
            }
            // A wide character in a column of one, and a spacer of a wide
            // character that is not there, show nothing.
            Width::Wide | Width::SpacerHead | Width::SpacerTail => {
                self.set_tail_cell(x, Cell::EMPTY);
            }
            Width::Narrow => self.copy_to_tail(source, row, col, x),
        }
        writer.x += 1 + u16::from(wide);
        let at = (self.tail, self.page_at(self.tail).len() - 1, x);
        writer.last = Some(at);
        for pin in std::mem::take(&mut writer.carried) {
            self.move_pin(pin, at);
        }
        at
    }

    /// Copies a cell of `source` into column `x` of the last row, growing
    /// its page when a table is full.
    fn copy_to_tail(&mut self, source: &Page, row: u16, col: u16, x: u16) {
        let tail = self.tail;
        let to = self.page_at(tail).len() - 1;
        let mut when_full = WhenFull::Fail;
        loop {
            match self
                .page_at_mut(tail)
                .copy_cell(to, x, source, row, col, when_full)
            {
                Ok(()) => return,
                Err(full) => {
                    if self.grow_node(tail, full).is_err() {
                        when_full = WhenFull::Drop;
                    }
                }
            }
        }
    }

    fn set_tail_cell(&mut self, x: u16, cell: Cell) {
        let tail = self.tail;
        let page = self.page_at_mut(tail);
        let row = page.len() - 1;
        page.set_cell(row, x, cell);
    }

    /// Where the cell `past` cells after the line's text goes: in its last
    /// row, as far as the row goes.
    fn after_text(&self, writer: &Writer, past: u16) -> (u32, u16, u16) {
        let (node, row, x) = writer.last.map_or_else(
            || (self.tail, self.page_at(self.tail).len() - 1, 0),
            |(node, row, x)| (node, row, x + 1),
        );
        (node, row, x.saturating_add(past).min(writer.cols - 1))
    }

    /// Moves the pins of the cell at column `col` of row `row` of the page
    /// being rewritten to `at`.
    fn repin(
        &mut self,
        pins: &mut std::iter::Peekable<std::vec::IntoIter<u32>>,
        row: u16,
        col: u16,
        at: (u32, u16, u16),
    ) {
        while let Some(&pin) = pins.peek().filter(|&&pin| {
            self.pins
                .place_of(pin)
                .is_some_and(|place| (place.row, place.col) == (row, col))
        }) {
            pins.next();
            self.move_pin(pin, at);
        }
    }

    /// Puts `pin`, which no node lists, at `at`: a node, a row and a
    /// column.
    fn move_pin(&mut self, pin: u32, at: (u32, u16, u16)) {
        let (node, row, col) = at;
        *self.pins.place_mut(pin) = Place { node, row, col };
        self.nodes[node as usize].pins.push(pin);
    }

    /// Settles the screen after rewrapping, which may have changed how many
    /// rows the list has, from `old_total`, and returns where the cursor,
    /// whose pin is `cursor_pin`, is on it.
    fn settle_cursor(&mut self, cursor_pin: PinId, old_total: usize) -> (u16, u16) {
        let index = |list: &Self| {
            let position = list.pin_position(cursor_pin)?;
            list.row_index(position.page, position.row)
        };
        // A line longer than the scrollback can take, which goes on past
        // the cursor, can push the cursor's row out of the list: the
        // cursor then starts the screen's last row.
        let Some(cursor) = index(self) else {
            while self.total_rows < usize::from(self.rows) {
                self.push_row();
            }
            return (0, self.rows - 1);
        };
        // The empty rows below the cursor take the rows that rewrapping
        // added, as far as there are some.
        let added = self.total_rows.saturating_sub(old_total);
        let below = self.total_rows - 1 - cursor;
        let empty = (0..below.min(added))
            .take_while(|&n| {
                self.row_at(self.total_rows - 1 - n)
                    .and_then(|(page, row)| Some(page_row_is_empty(self.page(page)?, row)))
                    .unwrap_or(false)
            })
            .count();
        for _ in 0..empty {
            self.pop_row();
        }
        // The cursor stays on screen: the rows below it go first.
        let excess = (self.total_rows - 1 - cursor).saturating_sub(usize::from(self.rows) - 1);
        for _ in 0..excess {
            self.pop_row();
        }
        while self.total_rows < usize::from(self.rows) {
            self.push_row();
        }
        while let Some(page) = self.pool.pop() {
            self.give(page.memory());
        }
        let col = self
            .pin_position(cursor_pin)
            .map_or(0, |position| position.col);
        let y = index(self).map_or(usize::from(self.rows) - 1, |index| {
            index - self.scrollback_rows()
        });
        self.unpin(cursor_pin);
        // Within the screen, so it fits.
        (col, y as u16)
    }

    /// Rebuilds every page at `cols` columns, each row cut or extended.
    fn set_cols(&mut self, cols: u16) {
        for (page, pins) in self.take_pages(cols) {
            let mut pins = pins.into_iter().peekable();
            for row in 0..page.len() {
                self.push_row();
                let tail = self.tail;
                let to = self.page_at(tail).len() - 1;
                let mut when_full = WhenFull::Fail;
                loop {
                    match self
                        .page_at_mut(tail)
                        .copy_row_resized(to, &page, row, when_full)
                    {
                        Ok(()) => break,
                        Err(full) => {
                            if self.grow_node(tail, full).is_err() {
                                when_full = WhenFull::Drop;
                            }
                        }
                    }
                }
                while let Some(&pin) = pins
                    .peek()
                    .filter(|&&pin| self.pins.place_of(pin).map(|p| p.row) == Some(row))
                {
                    pins.next();
                    let place = self.pins.place_mut(pin);
                    if place.col < cols {
                        *place = Place {
                            node: tail,
                            row: to,
                            col: place.col,
                        };
                        self.nodes[tail as usize].pins.push(pin);
                    } else {
                        self.pins.lose(pin);
                    }
                }
            }
        }
    }

    /// Adds `n` rows to the screen, as `grow` says, and returns how many
    /// of them came from the scrollback, above the screen's rows.
    fn add_rows(&mut self, n: u16, grow: Grow) -> u16 {
        let revealed = match grow {
            // Fewer rows than `n`.
            Grow::FromScrollback => self.scrollback_rows().min(usize::from(n)) as u16,
            Grow::AtTheBottom => 0,
        };
        self.rows += n;
        for _ in revealed..n {
            self.push_row();
        }
        revealed
    }

    /// Takes `n` rows off the screen, whose cursor is in row `y`, and
    /// returns the cursor's new row.
    fn remove_rows(&mut self, n: u16, y: u16) -> u16 {
        let blank_below = (y + 1..self.rows)
            .rev()
            .take_while(|&y| {
                let (page, row) = self.screen_row(y);
                page.cells(row).iter().all(|cell| cell.is_empty())
            })
            .count() as u16;
        let trimmed = blank_below.min(n);
        let to_scrollback = (n - trimmed).min(y);
        let dropped = n - trimmed - to_scrollback;
        for _ in 0..trimmed + dropped {
            self.pop_row();
        }
        self.rows -= n;
        // A page that emptied went; no row is about to need it.
        while let Some(page) = self.pool.pop() {
            self.give(page.memory());
        }
        y - to_scrollback
    }

    /// Drops the last row of the list, with its pins, and the page that it
    /// leaves empty.
    fn pop_row(&mut self) {
        let tail = self.tail;
        let page = self.page_at_mut(tail);
        let row = page.len() - 1;
        page.pop_row();
        self.total_rows -= 1;
        let pins = &mut self.pins;
        self.nodes[tail as usize].pins.retain(|&pin| {
            let lost = pins.place_mut(pin).row == row;
            if lost {
                pins.lose(pin);
            }
            !lost
        });
        if self.page_at(tail).is_empty() {
            self.unlink_tail();
        }
    }

    /// Unlinks the newest page, an empty one, which goes to the pool if it
    /// has the standard capacity, and is freed otherwise.
    fn unlink_tail(&mut self) {
        let index = self.tail;
        let node = &mut self.nodes[index as usize];
        let mut page = node.page.take().expect("a linked node has a page");
        node.generation = node.generation.wrapping_add(1);
        node.pins.clear();
        let prev = node.prev;
        self.tail = prev;
        if prev == NONE {
            self.head = NONE;
        } else {
            self.nodes[prev as usize].next = NONE;
        }
        self.free.push(index);
        self.pages -= 1;
        if page.capacity() == self.capacity {
            page.reset();
            self.pool.push(page);
        } else {
            self.give(page.memory());
        }
    }

    /// Inserts `n` empty cells at column `x` of row `y` of the screen, as
    /// [`Page::insert_blanks`] does: the pins of the cells that move right
    /// go along, and those of the cells that pass `end` are lost.
    pub fn insert_blanks(&mut self, y: u16, x: u16, n: u16, end: u16) {
        let (node, row) = self.locate(y);
        self.shift_pins(node, row, x..end, |col| {
            col.checked_add(n).filter(|&col| col < end)
        });
        self.page_at_mut(node).insert_blanks(row, x, n, end);
    }

    /// Deletes `n` cells at column `x` of row `y` of the screen, as
    /// [`Page::delete_cells`] does: the pins of the deleted cells are lost,
    /// and those of the cells that move left go along.
    pub fn delete_cells(&mut self, y: u16, x: u16, n: u16, end: u16) {
        let n = n.min(end - x);
        let (node, row) = self.locate(y);
        self.shift_pins(node, row, x..end, |col| (col >= x + n).then(|| col - n));
        self.page_at_mut(node).delete_cells(row, x, n, end);
    }

    /// Drops the scrollback, the rows above the screen, as ED 3 asks: the
    /// pages that hold only scrollback are freed, and the screen's rows move
    /// up in the page where it starts.
    pub fn clear_scrollback(&mut self) {
        while self.can_evict(0) {
            self.evict_head();
        }
        // No new page is about to take them from the pool, so their bytes
        // leave the limits now.
        while let Some(page) = self.pool.pop() {
            self.give(page.memory());
        }
        let extra = self.total_rows - usize::from(self.rows);
        if extra > 0 {
            let head = self.head;
            // Fewer rows than the page holds.
            let extra = extra as u16;
            self.page_at_mut(head).drop_first_rows(extra);
            self.total_rows -= usize::from(extra);
            let pins = &mut self.pins;
            self.nodes[head as usize].pins.retain(|&pin| {
                let place = pins.place_mut(pin);
                if place.row < extra {
                    pins.lose(pin);
                    return false;
                }
                place.row -= extra;
                true
            });
        }
    }

    /// Moves the pins in `columns` of row `from` of a node to the same
    /// columns of row `to` of a node, as their cells move there, and loses
    /// the pins of the cells that the move drops.
    fn move_pins(&mut self, from: (u32, u16), to: (u32, u16), columns: Range<u16>) {
        let ((source, from_row), (target, to_row)) = (from, to);
        let pins = &mut self.pins;
        let moves = |place: &Place, row| place.row == row && columns.contains(&place.col);
        self.nodes[target as usize].pins.retain(|&pin| {
            let dropped = moves(pins.place_mut(pin), to_row);
            if dropped {
                pins.lose(pin);
            }
            !dropped
        });
        if source == target {
            for &pin in &self.nodes[source as usize].pins {
                let place = pins.place_mut(pin);
                if moves(place, from_row) {
                    place.row = to_row;
                }
            }
            return;
        }
        let [source_node, target_node] = self
            .nodes
            .get_disjoint_mut([source as usize, target as usize])
            .expect("two nodes of the list");
        source_node.pins.retain(|&pin| {
            let place = pins.place_mut(pin);
            if !moves(place, from_row) {
                return true;
            }
            *place = Place {
                node: target,
                row: to_row,
                col: place.col,
            };
            target_node.pins.push(pin);
            false
        });
    }

    /// Moves the pins in `columns` of row `row` of a node to the column that
    /// `to` gives, or loses those for which it gives none.
    fn shift_pins(
        &mut self,
        node: u32,
        row: u16,
        columns: Range<u16>,
        to: impl Fn(u16) -> Option<u16>,
    ) {
        let pins = &mut self.pins;
        self.nodes[node as usize].pins.retain(|&pin| {
            let place = pins.place_mut(pin);
            if place.row != row || !columns.contains(&place.col) {
                return true;
            }
            match to(place.col) {
                Some(col) => {
                    place.col = col;
                    true
                }
                None => {
                    pins.lose(pin);
                    false
                }
            }
        });
    }

    /// The id of the page of node `index`.
    fn id(&self, index: u32) -> PageId {
        PageId {
            index,
            generation: self.nodes[index as usize].generation,
        }
    }

    /// Every row of the list, oldest first, as its page and its row in it.
    pub fn iter_rows(&self) -> impl Iterator<Item = (&Page, u16)> + '_ {
        self.iter_pages()
            .flat_map(|page| (0..page.len()).map(move |row| (page, row)))
    }

    fn iter_pages(&self) -> impl Iterator<Item = &Page> + '_ {
        let mut index = self.head;
        std::iter::from_fn(move || {
            if index == NONE {
                return None;
            }
            let node = &self.nodes[index as usize];
            index = node.next;
            node.page.as_ref()
        })
    }

    /// Adds an empty row at the bottom, in a new page if the last one is
    /// full.
    fn push_row(&mut self) {
        let needs_page = self.tail == NONE || self.page_at(self.tail).is_full();
        if needs_page {
            let page = self.new_page();
            self.link(page);
        }
        let tail = self.tail;
        self.page_at_mut(tail).push_row();
        self.total_rows += 1;
        // Another list's screen may have passed the shared budget.
        if needs_page && self.is_over() {
            self.enforce();
        }
    }

    /// A page for the end of the list: from the pool if it has one, or a
    /// new one if the limits allow it. Otherwise the oldest page leaves for
    /// the pool first, which then hands it out again.
    fn new_page(&mut self) -> Page {
        loop {
            if let Some(page) = self.pool.pop() {
                return page;
            }
            let bytes = self.capacity.bytes();
            // The new page will hold a row of the screen.
            if !self.can_evict(1) {
                // The screen needs the page, whatever the limits.
                self.take(bytes);
                return Page::new(self.capacity);
            }
            if self.memory + bytes <= self.scrollback.limit && self.try_take(bytes) {
                return Page::new(self.capacity);
            }
            self.evict_head();
        }
    }

    /// Links `page` at the end of the list.
    fn link(&mut self, page: Page) {
        let node = Node {
            page: Some(page),
            prev: self.tail,
            next: NONE,
            generation: 0,
            pins: Vec::new(),
        };
        let index = match self.free.pop() {
            Some(index) => {
                // The slot keeps the buffer of its pins.
                let slot = &mut self.nodes[index as usize];
                *slot = Node {
                    generation: slot.generation,
                    pins: std::mem::take(&mut slot.pins),
                    ..node
                };
                index
            }
            None => {
                self.nodes.push(node);
                u32::try_from(self.nodes.len() - 1).expect("fewer than 2^32 pages")
            }
        };
        if self.tail == NONE {
            self.head = index;
        } else {
            self.nodes[self.tail as usize].next = index;
        }
        self.tail = index;
        self.pages += 1;
    }

    /// Whether the oldest page can leave without taking part of the screen,
    /// with `coming` more rows about to be added.
    fn can_evict(&self, coming: usize) -> bool {
        self.head != NONE
            && self.total_rows + coming - usize::from(self.page_at(self.head).len())
                >= usize::from(self.rows)
    }

    /// Unlinks the oldest page, and keeps it in the pool if it has the
    /// standard capacity; frees it otherwise.
    fn evict_head(&mut self) {
        let index = self.head;
        let node = &mut self.nodes[index as usize];
        let mut page = node.page.take().expect("a linked node has a page");
        node.generation = node.generation.wrapping_add(1);
        for &pin in &node.pins {
            self.pins.lose(pin);
        }
        node.pins.clear();
        let next = node.next;
        self.head = next;
        if next == NONE {
            self.tail = NONE;
        } else {
            self.nodes[next as usize].prev = NONE;
        }
        self.free.push(index);
        self.pages -= 1;
        self.total_rows -= usize::from(page.len());
        if page.capacity() == self.capacity {
            page.reset();
            self.pool.push(page);
        } else {
            self.give(page.memory());
        }
    }

    /// Frees spare pages, then the oldest pages of the scrollback, until the
    /// pages are within the limits or only the screen is left.
    fn enforce(&mut self) {
        while self.is_over() {
            if let Some(page) = self.pool.pop() {
                self.give(page.memory());
            } else if self.can_evict(0) {
                self.evict_head();
            } else {
                break;
            }
        }
    }

    fn is_over(&self) -> bool {
        self.memory > self.scrollback.limit
            || self
                .scrollback
                .budget
                .as_ref()
                .is_some_and(|budget| budget.is_over())
    }

    /// Counts `bytes` more, whether the budget has room or not.
    fn take(&mut self, bytes: usize) {
        self.memory += bytes;
        if let Some(budget) = &self.scrollback.budget {
            budget.take(bytes);
        }
    }

    /// Counts `bytes` more if the budget has room for them.
    fn try_take(&mut self, bytes: usize) -> bool {
        let taken = self
            .scrollback
            .budget
            .as_ref()
            .is_none_or(|budget| budget.try_take(bytes));
        if taken {
            self.memory += bytes;
        }
        taken
    }

    /// Counts `bytes` less.
    fn give(&mut self, bytes: usize) {
        self.memory -= bytes;
        if let Some(budget) = &self.scrollback.budget {
            budget.give(bytes);
        }
    }

    /// The node and the row in its page of row `y` of the screen.
    fn locate(&self, y: u16) -> (u32, u16) {
        assert!(y < self.rows, "row {y} is past the screen");
        let row = self.total_rows - usize::from(self.rows) + usize::from(y);
        // The screen is at the end of the list, so search from the tail.
        let mut index = self.tail;
        let mut end = self.total_rows;
        loop {
            let page = self.page_at(index);
            let start = end - usize::from(page.len());
            if row >= start {
                return (index, (row - start) as u16);
            }
            end = start;
            index = self.nodes[index as usize].prev;
        }
    }

    fn page_at(&self, index: u32) -> &Page {
        self.nodes[index as usize]
            .page
            .as_ref()
            .expect("a linked node has a page")
    }

    fn page_at_mut(&mut self, index: u32) -> &mut Page {
        self.nodes[index as usize]
            .page
            .as_mut()
            .expect("a linked node has a page")
    }
}

/// The pages leave the budget with the list.
impl Drop for PageList {
    fn drop(&mut self) {
        if let Some(budget) = &self.scrollback.budget {
            budget.give(self.memory);
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::page::{Cell, Color, Content, Hyperlink, Rgb, Style};

    /// Limits that hold `pages` standard pages of `cols` columns.
    fn pages(cols: u16, pages: usize) -> Scrollback {
        Scrollback {
            limit: pages * Capacity::standard(cols).bytes(),
            budget: None,
        }
    }

    /// A cell that holds the number `n`, for checking where rows went.
    fn numbered(n: u32) -> Cell {
        let [_, r, g, b] = n.to_be_bytes();
        Cell::EMPTY.with_content(Content::BgRgb(Rgb::new(r, g, b)))
    }

    /// Scrolls `lines` times, numbering each new bottom row from `first`.
    fn scroll(list: &mut PageList, first: u32, lines: u32) {
        let bottom = list.rows() - 1;
        for n in first..first + lines {
            list.scroll_up();
            let (page, row) = list.screen_row_mut(bottom);
            page.set_cell(row, 0, numbered(n));
        }
    }

    /// The numbers of the rows of the list, oldest first; `None` for a row
    /// without one.
    fn numbers(list: &PageList) -> Vec<Option<u32>> {
        list.iter_rows()
            .map(|(page, row)| match page.cells(row)[0].content() {
                Content::BgRgb(Rgb { r, g, b }) => Some(u32::from_be_bytes([0, r, g, b])),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_new_list_is_an_empty_screen() {
        let list = PageList::new(80, 24, pages(80, 4));
        assert_eq!(
            (list.total_rows(), list.scrollback_rows(), list.pages()),
            (24, 0, 1)
        );
        for y in 0..24 {
            let (page, row) = list.screen_row(y);
            assert_eq!(row, y);
            assert!(page.cells(row).iter().all(|cell| cell.is_empty()));
        }
    }

    #[test]
    fn scrolling_moves_rows_into_the_scrollback() {
        let mut list = PageList::new(80, 24, pages(80, 4));
        scroll(&mut list, 0, 30);
        assert_eq!(list.scrollback_rows(), 30);
        let numbers = numbers(&list);
        assert_eq!(numbers[..24], [None; 24], "the first screen was empty");
        let scrolled: Vec<Option<u32>> = (0..30).map(Some).collect();
        assert_eq!(numbers[24..], scrolled);
        let (page, row) = list.screen_row(23);
        assert_eq!(page.cells(row)[0], numbered(29));
    }

    #[test]
    fn rows_go_on_into_new_pages() {
        let mut list = PageList::new(80, 24, pages(80, 4));
        let page_rows = u32::from(Capacity::standard(80).rows);
        // Twelve rows short of a page more: the screen straddles two pages.
        let lines = page_rows - 12;
        scroll(&mut list, 0, lines);
        assert_eq!(list.pages(), 2);
        assert_eq!(list.total_rows(), 24 + lines as usize);
        let (first, _) = list.screen_row_id(0);
        let (last, _) = list.screen_row_id(23);
        assert_ne!(first, last);
        let expected: Vec<Option<u32>> = (lines - 24..lines).map(Some).collect();
        let screen: Vec<Option<u32>> = numbers(&list)[list.scrollback_rows()..].to_vec();
        assert_eq!(screen, expected);
    }

    /// A list whose screen of 24 rows straddles two pages: its first 12
    /// rows end a page, and the other 12 start the next one.
    fn straddling() -> PageList {
        let mut list = PageList::new(80, 24, pages(80, 4));
        scroll(&mut list, 0, u32::from(Capacity::standard(80).rows) - 12);
        assert_ne!(list.screen_row_id(11).0, list.screen_row_id(12).0);
        list
    }

    /// The number of each row of the screen.
    fn screen_numbers(list: &PageList) -> Vec<Option<u32>> {
        numbers(list)[list.scrollback_rows()..].to_vec()
    }

    /// Writes `c` in `style` at column 1 of row `y` of the screen.
    fn styled(list: &mut PageList, y: u16, c: char, style: &Style) {
        let (page, row) = list.screen_row_mut(y);
        let id = page.styles_mut().acquire(style).unwrap();
        page.set_cell(
            row,
            1,
            Cell::EMPTY.with_content(Content::Char(c)).with_style(id),
        );
        page.styles_mut().release(id);
    }

    #[test]
    fn rows_move_within_a_page_and_across_two() {
        let mut list = straddling();
        let red = Style {
            fg: Color::Palette(1),
            ..Style::DEFAULT
        };
        for y in 0..24 {
            let (page, row) = list.screen_row_mut(y);
            page.set_cell(row, 0, numbered(1000 + u32::from(y)));
        }
        styled(&mut list, 11, 'r', &red);
        // Down across the boundary, up across it, and within a page.
        list.move_row(11, 12);
        list.move_row(13, 10);
        list.move_row(20, 23);
        let mut expected: Vec<Option<u32>> = (1000..1024).map(Some).collect();
        expected[12] = Some(1011);
        expected[11] = None;
        expected[10] = Some(1013);
        expected[13] = None;
        expected[23] = Some(1020);
        expected[20] = None;
        assert_eq!(screen_numbers(&list), expected);
        // The styled cell went into the other page, which holds its style,
        // and the first page let go of it.
        let (page, row) = list.screen_row(12);
        assert_eq!(page.styles().get(page.cells(row)[1].style()), &red);
        assert_eq!(page.styles().references(page.cells(row)[1].style()), 1);
        let (page, _) = list.screen_row(0);
        assert_eq!(page.styles().len(), 0);
    }

    #[test]
    fn a_move_into_a_page_with_a_full_table_grows_it() {
        let mut list = straddling();
        let style = |n: usize| {
            let [_, _, g, b] = u32::try_from(n).unwrap().to_be_bytes();
            Style {
                fg: Color::Rgb(Rgb::new(1, g, b)),
                ..Style::DEFAULT
            }
        };
        // Fill the style table of the lower page.
        let room = usize::from(Capacity::standard(80).styles);
        for n in 0..room {
            let (y, x) = (12 + (n / 80) as u16, (n % 80) as u16);
            let (page, row) = list.screen_row_mut(y);
            let id = page.styles_mut().acquire(&style(n)).unwrap();
            page.set_cell(row, x, Cell::EMPTY.with_style(id));
            page.styles_mut().release(id);
        }
        let (page, _) = list.screen_row(23);
        assert_eq!(page.styles().len(), room, "the table is full");
        let id = list.screen_row_id(12).0;
        let blue = Style {
            fg: Color::Palette(4),
            bold: true,
            ..Style::DEFAULT
        };
        styled(&mut list, 0, 'b', &blue);
        list.move_row(0, 23);
        let (page, row) = list.screen_row(23);
        assert_eq!(page.cells(row)[1].content(), Content::Char('b'));
        assert_eq!(page.styles().get(page.cells(row)[1].style()), &blue);
        assert_eq!(list.screen_row_id(12).0, id, "the page keeps its id");
        assert!(usize::from(page.capacity().styles) > room, "the page grew");
        assert_eq!(page.styles().len(), room + 1);
    }

    #[test]
    fn cells_move_across_pages_within_a_range() {
        let mut list = straddling();
        let red = Style {
            fg: Color::Palette(1),
            ..Style::DEFAULT
        };
        for y in [11, 12] {
            let (page, row) = list.screen_row_mut(y);
            for x in 0..4 {
                page.set_cell(row, x, numbered(u32::from(y) * 10 + u32::from(x)));
            }
        }
        styled(&mut list, 11, 'r', &red);
        list.move_cells(11, 12, 1..3);
        let cells = |list: &PageList, y| {
            let (page, row) = list.screen_row(y);
            page.cells(row)[..4].to_vec()
        };
        assert_eq!(cells(&list, 12)[0], numbered(120));
        assert_eq!(cells(&list, 12)[3], numbered(123));
        assert_eq!(cells(&list, 12)[2], numbered(112));
        assert_eq!(cells(&list, 11)[1], Cell::EMPTY);
        assert_eq!(cells(&list, 11)[0], numbered(110));
        let (page, row) = list.screen_row(12);
        assert_eq!(page.styles().get(page.cells(row)[1].style()), &red);
        let (page, _) = list.screen_row(11);
        assert_eq!(page.styles().len(), 0, "the first page let go of red");
    }

    #[test]
    fn clearing_the_scrollback_keeps_the_screen() {
        let mut list = straddling();
        for y in 0..24 {
            let (page, row) = list.screen_row_mut(y);
            page.set_cell(row, 0, numbered(1000 + u32::from(y)));
        }
        assert_eq!(list.pages(), 2);
        list.clear_scrollback();
        assert_eq!(list.scrollback_rows(), 0);
        assert_eq!(list.total_rows(), 24);
        let expected: Vec<Option<u32>> = (1000..1024).map(Some).collect();
        assert_eq!(numbers(&list), expected);
        // The list goes on as before.
        scroll(&mut list, 0, 3);
        assert_eq!(list.scrollback_rows(), 3);
        assert_eq!(screen_numbers(&list)[23], Some(2));
        list.clear_scrollback();
        assert_eq!(list.scrollback_rows(), 0);
        let mut list = PageList::new(80, 24, pages(80, 4));
        list.clear_scrollback();
        assert_eq!(list.total_rows(), 24, "nothing to clear");
    }

    #[test]
    fn the_oldest_page_leaves_for_the_pool_at_the_limit() {
        let mut list = PageList::new(80, 24, pages(80, 3));
        let page_rows = usize::from(Capacity::standard(80).rows);
        let (oldest, _) = list.screen_row_id(0);
        // Fill three pages, then start a fourth.
        scroll(&mut list, 0, (3 * page_rows - 24 + 1) as u32);
        assert_eq!(list.pages(), 3);
        assert!(
            list.page(oldest).is_none(),
            "the oldest page's id went stale"
        );
        assert_eq!(list.total_rows(), 2 * page_rows + 1);
        // The pool hands the page back out, so it does not grow.
        assert_eq!(list.pool_len(), 0);
        let numbers = numbers(&list);
        let first = numbers[0].expect("the first page's rows are gone");
        let kept: Vec<Option<u32>> = (first..first + numbers.len() as u32).map(Some).collect();
        assert_eq!(numbers, kept, "the rows kept are the latest, in order");
    }

    #[test]
    fn a_screen_taller_than_a_page_spans_several() {
        // At 4000 columns, a page holds fewer rows than the screen has.
        let mut list = PageList::new(4000, 40, pages(4000, 1));
        scroll(&mut list, 0, 100);
        let expected: Vec<Option<u32>> = (60..100).map(Some).collect();
        let screen: Vec<Option<u32>> = (0..40)
            .map(|y| {
                let (page, row) = list.screen_row(y);
                match page.cells(row)[0].content() {
                    Content::BgRgb(Rgb { r, g, b }) => Some(u32::from_be_bytes([0, r, g, b])),
                    _ => None,
                }
            })
            .collect();
        assert_eq!(screen, expected);
        assert!(list.memory() > list.scrollback().limit, "the screen stays");
    }

    #[test]
    fn a_grown_page_keeps_its_rows_and_id_and_leaves_the_pool_out() {
        let mut list = PageList::new(80, 24, pages(80, 2));
        scroll(&mut list, 0, 10);
        let (id, _) = list.screen_row_id(23);
        list.grow_page(23, TableFull::Styles).unwrap();
        assert_eq!(list.page(id).map(|page| page.capacity().styles), Some(512));
        let (page, row) = list.screen_row(23);
        assert_eq!(page.cells(row)[0], numbered(9));
        // When it leaves the list, a grown page is not pooled.
        let page_rows = usize::from(Capacity::standard(80).rows);
        scroll(&mut list, 10, (2 * page_rows) as u32);
        assert!(list.page(id).is_none());
        assert_eq!(list.pool_len(), 0);
    }

    #[test]
    fn clearing_the_scrollback_frees_its_pages() {
        let budget = Arc::new(Budget::new(8 * Capacity::standard(80).bytes()));
        let shared = Scrollback {
            budget: Some(Arc::clone(&budget)),
            ..Scrollback::default()
        };
        let mut list = PageList::new(80, 24, shared);
        let page_rows = u32::from(Capacity::standard(80).rows);
        scroll(&mut list, 0, 4 * page_rows);
        let pages = list.pages();
        list.clear_scrollback();
        assert_eq!(list.pool_len(), 0, "no spare page waits");
        assert!(list.pages() < pages);
        assert_eq!(list.memory(), counted(&list));
        assert_eq!(
            budget.used(),
            list.memory(),
            "the budget has the bytes back"
        );
    }

    #[test]
    fn a_move_into_full_grapheme_and_link_tables_grows_them() {
        let mut list = straddling();
        // Fill the lower page's grapheme table, and its table of links.
        let capacity = Capacity::standard(80);
        for n in 0..usize::from(capacity.graphemes) {
            let (y, x) = (12 + (n / 80) as u16, (n % 80) as u16);
            let (page, row) = list.screen_row_mut(y);
            page.set_cell(row, x, Cell::EMPTY.with_content(Content::Char('e')));
            page.push_codepoint(row, x, '\u{301}').unwrap();
        }
        for n in 0..usize::from(capacity.hyperlinks) {
            let (page, row) = list.screen_row_mut(20);
            let uri = format!("http://{n}");
            let id = page
                .hyperlinks_mut()
                .acquire(Hyperlink {
                    id: b"",
                    uri: uri.as_bytes(),
                })
                .unwrap();
            page.set_hyperlink(row, n as u16, Some(id)).unwrap();
            page.hyperlinks_mut().release(id);
        }
        // A row of the upper page with a grapheme and a link of its own.
        let (page, row) = list.screen_row_mut(0);
        page.set_cell(row, 0, Cell::EMPTY.with_content(Content::Char('o')));
        page.push_codepoint(row, 0, '\u{302}').unwrap();
        let id = page
            .hyperlinks_mut()
            .acquire(Hyperlink {
                id: b"",
                uri: b"http://new",
            })
            .unwrap();
        page.set_hyperlink(row, 0, Some(id)).unwrap();
        page.hyperlinks_mut().release(id);
        list.move_row(0, 23);
        let (page, row) = list.screen_row(23);
        assert_eq!(page.grapheme(row, 0), ['\u{302}']);
        let link = page.hyperlink(row, 0).unwrap();
        assert_eq!(page.hyperlinks().get(link).uri, b"http://new");
        assert!(page.capacity().graphemes > capacity.graphemes);
        assert!(page.capacity().hyperlinks > capacity.hyperlinks);
        let (page, _) = list.screen_row(0);
        assert!(
            page.hyperlinks().is_empty(),
            "the upper page let go of its link"
        );
    }

    #[test]
    fn a_taller_screen_takes_rows_from_the_scrollback() {
        let mut list = PageList::new(80, 24, pages(80, 4));
        scroll(&mut list, 0, 30);
        assert_eq!(list.resize(80, 30, (2, 23), Grow::FromScrollback), (2, 29));
        assert_eq!(list.scrollback_rows(), 24);
        let screen = screen_numbers(&list);
        assert_eq!(screen, (0..30).map(Some).collect::<Vec<_>>());
        // With less scrollback than new rows, empty ones come at the bottom.
        let mut list = PageList::new(80, 24, pages(80, 4));
        scroll(&mut list, 0, 3);
        assert_eq!(list.resize(80, 30, (0, 23), Grow::FromScrollback), (0, 26));
        assert_eq!(list.scrollback_rows(), 0);
        assert_eq!(screen_numbers(&list)[26], Some(2));
        assert_eq!(screen_numbers(&list)[27..], [None; 3]);
        assert_eq!(list.memory(), counted(&list));
    }

    #[test]
    fn a_taller_alternate_screen_adds_rows_at_the_bottom() {
        let mut list = PageList::new(80, 24, pages(80, 4));
        scroll(&mut list, 0, 30);
        assert_eq!(list.resize(80, 30, (0, 23), Grow::AtTheBottom), (0, 23));
        assert_eq!(list.scrollback_rows(), 30);
        assert_eq!(screen_numbers(&list)[23], Some(29));
        assert_eq!(screen_numbers(&list)[24..], [None; 6]);
    }

    #[test]
    fn a_shorter_screen_drops_the_empty_rows_below_the_cursor_first() {
        let numbered_screen = || {
            let mut list = PageList::new(80, 24, pages(80, 4));
            for y in 0..24 {
                let (page, row) = list.screen_row_mut(y);
                page.set_cell(row, 0, numbered(u32::from(y)));
            }
            list
        };
        // Empty rows below the cursor go, and nothing goes to the scrollback.
        let mut list = PageList::new(80, 24, pages(80, 4));
        for y in 0..=10 {
            let (page, row) = list.screen_row_mut(y);
            page.set_cell(row, 0, numbered(u32::from(y)));
        }
        assert_eq!(list.resize(80, 12, (0, 10), Grow::FromScrollback), (0, 10));
        assert_eq!(list.scrollback_rows(), 0);
        assert_eq!(screen_numbers(&list)[10], Some(10));
        // Rows from the top go to the scrollback, as far as the cursor's.
        let mut list = numbered_screen();
        assert_eq!(list.resize(80, 14, (0, 23), Grow::FromScrollback), (0, 13));
        assert_eq!(list.scrollback_rows(), 10);
        assert_eq!(screen_numbers(&list)[13], Some(23));
        // The rest is dropped from the bottom.
        let mut list = numbered_screen();
        assert_eq!(list.resize(80, 14, (0, 5), Grow::FromScrollback), (0, 0));
        assert_eq!(list.scrollback_rows(), 5);
        assert_eq!(screen_numbers(&list)[0], Some(5));
        assert_eq!(screen_numbers(&list)[13], Some(18));
        assert_eq!(list.memory(), counted(&list));
        // A page that the dropped rows empty is freed.
        let mut list = straddling();
        let (lower, _) = list.screen_row_id(12);
        let pin = pin_at(&mut list, 0, 20);
        assert_eq!(list.resize(80, 12, (0, 0), Grow::FromScrollback), (0, 0));
        assert!(list.page(lower).is_none(), "its id went stale");
        assert_eq!(list.pin_position(pin), None, "its pins were lost");
        assert_eq!((list.pages(), list.pool_len()), (1, 0));
        assert_eq!(list.memory(), counted(&list));
        assert_pins_listed(&list);
        // A new page takes the slot, but not the old page's id.
        list.resize(80, 24, (0, 0), Grow::AtTheBottom);
        assert_eq!(list.pages(), 2);
        assert!(list.page(lower).is_none());
    }

    #[test]
    fn narrower_and_wider_rows_are_cut_and_extended() {
        let budget = Arc::new(Budget::new(8 * Capacity::standard(80).bytes()));
        let shared = Scrollback {
            budget: Some(Arc::clone(&budget)),
            ..Scrollback::default()
        };
        let mut list = PageList::new(80, 24, shared);
        let page_rows = u32::from(Capacity::standard(80).rows);
        scroll(&mut list, 0, page_rows);
        let red = Style {
            fg: Color::Palette(1),
            ..Style::DEFAULT
        };
        styled(&mut list, 23, 'r', &red);
        let (page, row) = list.screen_row_mut(23);
        page.set_cell(row, 2, Cell::EMPTY.with_content(Content::Char('e')));
        page.push_codepoint(row, 2, '\u{301}').unwrap();
        page.set_cell(
            row,
            9,
            Cell::EMPTY
                .with_content(Content::Char('中'))
                .with_width(crate::page::Width::Wide),
        );
        page.set_cell(
            row,
            10,
            Cell::EMPTY.with_width(crate::page::Width::SpacerTail),
        );
        page.set_cell(row, 40, Cell::EMPTY.with_content(Content::Char('z')));
        page.set_cell(
            row,
            5,
            Cell::EMPTY.with_width(crate::page::Width::SpacerHead),
        );
        page.row_mut(row)
            .set(crate::page::RowFlag::SoftWrapped, true);
        page.row_mut(row)
            .set(crate::page::RowFlag::PromptStart, true);
        let kept = pin_at(&mut list, 2, 23);
        let cut = pin_at(&mut list, 40, 23);
        let (old_page, _) = list.screen_row_id(23);
        assert_eq!(list.resize(10, 24, (40, 23), Grow::FromScrollback), (9, 23));
        assert!(
            list.page(old_page).is_none(),
            "the old pages' ids went stale"
        );
        let (page, row) = list.screen_row(23);
        assert_eq!(page.cells(row)[0], numbered(page_rows - 1));
        assert_eq!(page.styles().get(page.cells(row)[1].style()), &red);
        assert_eq!(page.grapheme(row, 2), ['\u{301}']);
        assert_eq!(
            page.cells(row)[9],
            Cell::EMPTY,
            "the wide character lost its half"
        );
        assert!(!page.row(row).has(crate::page::RowFlag::SoftWrapped));
        assert!(page.row(row).has(crate::page::RowFlag::PromptStart));
        assert_eq!(
            page.cells(row)[5],
            Cell::EMPTY,
            "no spacer head without a wrap"
        );
        assert_eq!(pinned(&list, kept), Some(page.cells(row)[2]));
        assert_eq!(list.pin_position(cut), None);
        assert_eq!(numbers(&list).len(), list.total_rows());
        assert_eq!(numbers(&list).last(), Some(&Some(page_rows - 1)));
        assert_eq!(list.memory(), counted(&list));
        assert_eq!(budget.used(), list.memory());
        assert_pins_listed(&list);
        // Wider again: the cells stay, and the new ones are empty.
        list.resize(30, 24, (0, 23), Grow::FromScrollback);
        let (page, row) = list.screen_row(23);
        assert_eq!(page.grapheme(row, 2), ['\u{301}']);
        assert!(page.cells(row)[9..].iter().all(|cell| cell.is_empty()));
        assert_eq!(page.styles().len(), 1);
        assert_eq!(list.memory(), counted(&list));
        assert_pins_listed(&list);
    }

    #[test]
    fn a_page_at_its_largest_does_not_grow() {
        let mut list = PageList::new(80, 24, Scrollback::default());
        let mut grows = 0;
        while list.grow_page(23, TableFull::Styles).is_ok() {
            grows += 1;
        }
        // From 256 styles to 65 535, doubling.
        assert_eq!(grows, 8);
        let (page, _) = list.screen_row(23);
        assert_eq!(page.capacity().styles, u16::MAX);
        let memory = list.memory();
        assert_eq!(
            list.grow_page(23, TableFull::Styles),
            Err(TableFull::Styles)
        );
        assert_eq!(list.memory(), memory, "the page stayed as it was");
        assert_eq!(list.memory(), counted(&list));
    }

    #[test]
    #[should_panic(expected = "row 24 is past the screen")]
    fn a_row_past_the_screen_cannot_be_read() {
        PageList::new(80, 24, pages(80, 4)).screen_row(24);
    }

    /// The bytes of the pages, counted one by one.
    fn counted(list: &PageList) -> usize {
        list.iter_pages().map(Page::memory).sum::<usize>()
            + list.pool.iter().map(Page::memory).sum::<usize>()
    }

    /// Writes a row in a style of its own, `n`, growing the page when its
    /// style table is full, as the terminal will.
    fn styled_line(list: &mut PageList, n: u32) {
        let bottom = list.rows() - 1;
        list.scroll_up();
        let [_, r, g, b] = n.to_be_bytes();
        let style = Style {
            fg: Color::Rgb(Rgb::new(r, g, b)),
            ..Style::DEFAULT
        };
        let id = loop {
            let (page, _) = list.screen_row_mut(bottom);
            match page.styles_mut().acquire(&style) {
                Ok(id) => break id,
                Err(full) => list.grow_page(bottom, full).unwrap(),
            }
        };
        let (page, row) = list.screen_row_mut(bottom);
        page.set_cell(row, 0, numbered(n).with_style(id));
        page.styles_mut().release(id);
    }

    #[test]
    fn a_flood_of_output_stays_within_the_limit() {
        let limit = Scrollback::default().limit;
        let mut list = PageList::new(80, 24, Scrollback::default());
        for n in 0..100_000 {
            scroll(&mut list, n, 1);
            assert!(
                list.memory() <= limit,
                "{} bytes after {n} lines",
                list.memory()
            );
        }
        assert_eq!(list.memory(), counted(&list));
        // It holds as much as the limit allows.
        assert!(list.memory() + Capacity::standard(80).bytes() > limit);
        let numbers: Vec<u32> = numbers(&list).into_iter().map(Option::unwrap).collect();
        let first = numbers[0];
        assert_eq!(numbers, (first..100_000).collect::<Vec<u32>>());
    }

    #[test]
    fn pages_that_grow_stay_within_the_limit_too() {
        let limit = Scrollback::default().limit;
        let mut list = PageList::new(80, 24, Scrollback::default());
        for n in 0..50_000 {
            styled_line(&mut list, n);
            assert!(
                list.memory() <= limit,
                "{} bytes after {n} lines",
                list.memory()
            );
        }
        assert_eq!(list.memory(), counted(&list));
        // Every row still has its own style.
        let (page, row) = list.screen_row(23);
        let style = page.styles().get(page.cells(row)[0].style());
        assert_eq!(style.fg, Color::Rgb(Rgb::new(0, 0xC3, 0x4F)));
    }

    #[test]
    fn a_page_that_grows_past_the_limit_makes_room() {
        let mut list = PageList::new(80, 24, pages(80, 3));
        let page_rows = u32::from(Capacity::standard(80).rows);
        // Three full pages: exactly at the limit.
        scroll(&mut list, 0, 3 * page_rows - 24);
        assert_eq!(list.memory(), list.scrollback().limit);
        list.grow_page(23, TableFull::Styles).unwrap();
        assert!(list.memory() <= list.scrollback().limit);
        assert_eq!(list.pages(), 2, "the oldest page made room");
        assert_eq!(list.memory(), counted(&list));
        let (page, row) = list.screen_row(23);
        assert_eq!(page.cells(row)[0], numbered(3 * page_rows - 25));
    }

    #[test]
    fn panes_share_a_budget() {
        let budget = Arc::new(Budget::new(6 * Capacity::standard(80).bytes()));
        let shared = Scrollback {
            budget: Some(Arc::clone(&budget)),
            ..Scrollback::default()
        };
        let mut busy = PageList::new(80, 24, shared.clone());
        let mut idle = PageList::new(80, 24, shared);
        scroll(&mut idle, 0, 1000);
        let idle_memory = idle.memory();
        for n in 0..20_000 {
            scroll(&mut busy, n, 1);
            assert!(
                budget.used() <= budget.limit(),
                "{} bytes after {n} lines",
                budget.used()
            );
            assert_eq!(budget.used(), busy.memory() + idle.memory());
        }
        // The busy pane made room with its own pages, not the idle one's.
        assert_eq!(idle.memory(), idle_memory);
        assert_eq!(numbers(&idle).last(), Some(&Some(999)));
        drop(busy);
        assert_eq!(budget.used(), idle.memory());
    }

    #[test]
    fn a_pane_gives_back_its_scrollback_when_a_screen_passes_the_budget() {
        let page = Capacity::standard(80).bytes();
        let budget = Arc::new(Budget::new(3 * page));
        let shared = Scrollback {
            budget: Some(Arc::clone(&budget)),
            ..Scrollback::default()
        };
        let mut first = PageList::new(80, 24, shared.clone());
        let page_rows = u32::from(Capacity::standard(80).rows);
        scroll(&mut first, 0, 3 * page_rows - 24);
        assert_eq!(budget.used(), budget.limit());
        // A new pane's screen gets its page past the budget.
        let second = PageList::new(80, 24, shared);
        assert_eq!(budget.used(), 4 * page);
        // The first pane gives back its own scrollback when it next needs
        // a page.
        scroll(&mut first, 3 * page_rows, 1);
        assert_eq!(budget.used(), budget.limit());
        assert_eq!(budget.used(), first.memory() + second.memory());
    }

    #[test]
    fn panes_on_several_threads_share_a_budget() {
        let budget = Arc::new(Budget::new(8 * Capacity::standard(80).bytes()));
        std::thread::scope(|scope| {
            for _ in 0..4 {
                let scrollback = Scrollback {
                    budget: Some(Arc::clone(&budget)),
                    ..Scrollback::default()
                };
                let budget = &budget;
                scope.spawn(move || {
                    let mut list = PageList::new(80, 24, scrollback);
                    // A screen of 24 rows needs at most two pages, which can
                    // pass the budget while other panes hold it.
                    let screens = 4 * 2 * Capacity::standard(80).bytes();
                    for n in 0..20_000 {
                        scroll(&mut list, n, 1);
                        assert!(budget.used() <= budget.limit() + screens);
                    }
                });
            }
        });
        assert_eq!(budget.used(), 0, "every list gave its pages back");
    }

    #[test]
    fn the_screen_stays_whatever_the_limits() {
        let mut list = PageList::new(80, 24, pages(80, 0));
        scroll(&mut list, 0, 2000);
        let screen: Vec<Option<u32>> = numbers(&list)[list.scrollback_rows()..].to_vec();
        assert_eq!(screen, (1976..2000).map(Some).collect::<Vec<_>>());
        assert!(list.pages() <= 2);
    }

    /// Column `x` of row `y` of the screen, as a position in the list.
    fn at(list: &PageList, x: u16, y: u16) -> Position {
        let (page, row) = list.screen_row_id(y);
        Position { page, row, col: x }
    }

    /// Pins column `x` of row `y` of the screen.
    fn pin_at(list: &mut PageList, x: u16, y: u16) -> PinId {
        let position = at(list, x, y);
        list.pin(position).expect("a cell of the screen")
    }

    /// The cell that `pin` is on, if it is not lost.
    fn pinned(list: &PageList, pin: PinId) -> Option<Cell> {
        let position = list.pin_position(pin)?;
        let page = list
            .page(position.page)
            .expect("a pin's page is in the list");
        Some(page.cells(position.row)[usize::from(position.col)])
    }

    /// Checks that each node lists the pins in its page, and only those.
    fn assert_pins_listed(list: &PageList) {
        let mut listed = 0;
        let mut index = list.head;
        while index != NONE {
            let node = &list.nodes[index as usize];
            for &pin in &node.pins {
                let place = list.pins.place_of(pin).expect("a listed pin is in use");
                assert_eq!(place.node, index, "pin {pin} is listed in its node");
                assert!(place.row < list.page_at(index).len());
            }
            listed += node.pins.len();
            index = node.next;
        }
        assert_eq!(listed, list.pins(), "every pin in use is listed once");
    }

    /// Numbers the cells of the screen: `1000 * (y + 1) + x`.
    fn number_cells(list: &mut PageList) {
        for y in 0..list.rows() {
            for x in 0..8 {
                let (page, row) = list.screen_row_mut(y);
                page.set_cell(row, x, numbered(1000 * (u32::from(y) + 1) + u32::from(x)));
            }
        }
    }

    #[test]
    fn pins_stay_on_their_cells_as_the_screen_scrolls() {
        let mut list = PageList::new(80, 24, pages(80, 4));
        scroll(&mut list, 0, 30);
        let pin = pin_at(&mut list, 0, 23);
        let position = list.pin_position(pin).unwrap();
        assert_eq!(list.row_index(position.page, position.row), Some(53));
        scroll(&mut list, 30, 10);
        assert_eq!(list.pin_position(pin), Some(position));
        assert_eq!(pinned(&list, pin), Some(numbered(29)));
        assert_pins_listed(&list);
    }

    #[test]
    fn a_pin_into_an_evicted_page_is_lost() {
        let mut list = PageList::new(80, 24, pages(80, 3));
        let page_rows = u32::from(Capacity::standard(80).rows);
        let first = pin_at(&mut list, 5, 0);
        // The bottom of the screen is in the second page now.
        scroll(&mut list, 0, page_rows);
        let kept = pin_at(&mut list, 0, 23);
        assert_ne!(
            list.pin_position(first).unwrap().page,
            at(&list, 0, 23).page
        );
        // The first page leaves.
        scroll(&mut list, page_rows, 2 * page_rows - 23);
        assert_eq!(list.pin_position(first), None, "lost with its page");
        assert_eq!(pinned(&list, kept), Some(numbered(page_rows - 1)));
        assert_eq!(list.pins(), 1);
        assert_pins_listed(&list);
        // A new pin takes the lost one's slot, but not its id.
        let new = pin_at(&mut list, 0, 0);
        assert_ne!(new, first);
        assert_eq!(list.pin_position(first), None);
        assert!(list.pin_position(new).is_some());
    }

    #[test]
    fn pins_follow_the_rows_that_move() {
        let mut list = straddling();
        number_cells(&mut list);
        let down = pin_at(&mut list, 3, 11);
        let dropped = pin_at(&mut list, 7, 12);
        let within = pin_at(&mut list, 0, 20);
        let staying = pin_at(&mut list, 0, 5);
        // Across the pages, within one, and nowhere.
        list.move_row(11, 12);
        list.move_row(20, 23);
        list.move_row(5, 5);
        assert_eq!(list.pin_position(down), Some(at(&list, 3, 12)));
        assert_eq!(pinned(&list, down), Some(numbered(12_003)));
        assert_eq!(list.pin_position(dropped), None, "its cell was dropped");
        assert_eq!(list.pin_position(within), Some(at(&list, 0, 23)));
        assert_eq!(list.pin_position(staying), Some(at(&list, 0, 5)));
        assert_pins_listed(&list);
        // And back up across them.
        list.move_row(12, 10);
        assert_eq!(list.pin_position(down), Some(at(&list, 3, 10)));
        assert_eq!(pinned(&list, down), Some(numbered(12_003)));
        assert_eq!(list.pins(), 3);
        assert_pins_listed(&list);
    }

    #[test]
    fn pins_follow_the_cells_that_move() {
        let mut list = straddling();
        number_cells(&mut list);
        let pins = [(0, 11), (1, 11), (2, 12), (3, 12), (0, 20), (1, 22)]
            .map(|(x, y)| pin_at(&mut list, x, y));
        list.move_cells(11, 12, 1..3);
        list.move_cells(20, 22, 0..2);
        let positions = pins.map(|pin| list.pin_position(pin));
        assert_eq!(
            positions,
            [
                Some(at(&list, 0, 11)),
                Some(at(&list, 1, 12)),
                None,
                Some(at(&list, 3, 12)),
                Some(at(&list, 0, 22)),
                None,
            ],
            "the pins of the columns moved went along, or were dropped"
        );
        assert_eq!(pinned(&list, pins[1]), Some(numbered(12_001)));
        assert_eq!(pinned(&list, pins[4]), Some(numbered(21_000)));
        assert_pins_listed(&list);
    }

    #[test]
    fn clearing_the_scrollback_loses_only_its_own_pins() {
        let mut list = PageList::new(80, 24, pages(80, 4));
        let page_rows = usize::from(Capacity::standard(80).rows);
        scroll(&mut list, 0, (2 * page_rows) as u32);
        let pin_row = |list: &mut PageList, index: usize| {
            let (page, row) = list.row_at(index).unwrap();
            list.pin(Position { page, row, col: 1 }).unwrap()
        };
        let in_first = pin_row(&mut list, 3);
        let in_second = pin_row(&mut list, page_rows + 3);
        let top = pin_at(&mut list, 0, 0);
        let bottom = pin_at(&mut list, 2, 23);
        let numbers = (pinned(&list, top), pinned(&list, bottom));
        list.clear_scrollback();
        assert_eq!(list.pin_position(in_first), None);
        assert_eq!(list.pin_position(in_second), None);
        assert_eq!(list.pin_position(top), Some(at(&list, 0, 0)));
        assert_eq!(list.pin_position(bottom), Some(at(&list, 2, 23)));
        assert_eq!((pinned(&list, top), pinned(&list, bottom)), numbers);
        assert_eq!(list.pins(), 2);
        assert_pins_listed(&list);
    }

    #[test]
    fn a_page_that_grows_keeps_its_pins() {
        let mut list = straddling();
        let pin = pin_at(&mut list, 2, 23);
        let position = list.pin_position(pin);
        list.grow_page(23, TableFull::Styles).unwrap();
        assert_eq!(list.pin_position(pin), position);
        assert_pins_listed(&list);
    }

    #[test]
    fn pins_go_only_on_cells_of_the_list_and_can_be_let_go() {
        let mut list = PageList::new(80, 24, pages(80, 2));
        let position = at(&list, 0, 23);
        let past_the_rows = Position {
            row: 24,
            ..position
        };
        let past_the_columns = Position {
            col: 80,
            ..position
        };
        assert_eq!(list.pin(past_the_rows), None);
        assert_eq!(list.pin(past_the_columns), None);
        let pin = list
            .pin(Position {
                col: 79,
                ..position
            })
            .unwrap();
        let other = list.pin(position).unwrap();
        list.unpin(pin);
        assert_eq!(list.pin_position(pin), None);
        assert_eq!(list.pin_position(other), Some(position));
        list.unpin(pin);
        assert_eq!(list.pins(), 1, "letting go twice is harmless");
        assert_pins_listed(&list);
        let page_rows = u32::from(Capacity::standard(80).rows);
        scroll(&mut list, 0, 2 * page_rows);
        assert_eq!(list.pin(position), None, "its page left");
        list.unpin(other);
        assert_eq!(list.pins(), 0);
    }

    /// Something that the property does to a list.
    #[derive(Clone, Debug)]
    enum Op {
        Scroll(u16),
        MoveRow(u16, u16),
        MoveCells(u16, u16, Range<u16>),
        ClearScrollback,
        Grow(u16),
        /// Inserts empty cells in a row of the screen, as ICH does.
        Insert(u16, u16, u16, u16),
        /// Deletes cells of a row of the screen, as DCH does.
        Delete(u16, u16, u16, u16),
        /// Pins a cell of the row at an index of the list, modulo its rows.
        Pin(usize, u16),
        /// Resizes the list: 0 cuts, taking new rows from the scrollback; 1
        /// cuts, adding them at the bottom; 2 rewraps.
        Resize(u16, u16, u8),
        /// Lets go of a pin, modulo how many there are.
        Unpin(usize),
    }

    fn op() -> impl Strategy<Value = Op> {
        let y = || 0_u16..8;
        prop_oneof![
            3 => (1_u16..40).prop_map(Op::Scroll),
            3 => (y(), y()).prop_map(|(from, to)| Op::MoveRow(from, to)),
            2 => (y(), y(), 0_u16..6, 1_u16..4)
                .prop_map(|(from, to, start, len)| Op::MoveCells(from, to, start..start + len)),
            1 => Just(Op::ClearScrollback),
            1 => (y(), 0_u16..SPAN, 1_u16..4, 1_u16..=SPAN)
                .prop_map(|(y, x, n, end)| Op::Insert(y, x.min(end - 1), n, end)),
            1 => (y(), 0_u16..SPAN, 1_u16..4, 1_u16..=SPAN)
                .prop_map(|(y, x, n, end)| Op::Delete(y, x.min(end - 1), n, end)),
            1 => y().prop_map(Op::Grow),
            4 => (any::<usize>(), 0_u16..COLUMNS).prop_map(|(row, col)| Op::Pin(row, col)),
            1 => any::<usize>().prop_map(Op::Unpin),
            2 => (prop::sample::select(vec![3_u16, 6, 8, 2000]), 2_u16..10, 0_u8..3)
                .prop_map(|(cols, rows, how)| Op::Resize(cols, rows, how)),
        ]
    }

    /// The numbered columns of each row in the property.
    const COLUMNS: u16 = 5;

    /// The columns that the property looks at, wide enough for the numbered
    /// cells that insertions move right.
    const SPAN: u16 = 8;

    /// The number of the cell at `position`, if it has one.
    fn number_at(list: &PageList, position: Position) -> Option<u32> {
        let page = list.page(position.page)?;
        match page.cells(position.row)[usize::from(position.col)].content() {
            Content::BgRgb(Rgb { r, g, b }) => Some(u32::from_be_bytes([0, r, g, b])),
            _ => None,
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// Every cell has a number of its own, so a pin that is not lost is
        /// on the cell it was put on, and a lost pin's cell is nowhere.
        #[test]
        fn pins_stay_on_their_cells_until_the_cells_go(ops in prop::collection::vec(op(), 1..40)) {
            // At 2000 columns a page holds about 30 rows, so pages leave
            // after a few scrolls.
            let mut list = PageList::new(2000, 8, pages(2000, 3));
            let mut next = 0;
            let mut pins: Vec<(PinId, u32)> = Vec::new();
            for op in &ops {
                // The operations stay within the list's size as it changes.
                let (cols, rows) = (list.cols(), list.rows());
                let numbered_columns = COLUMNS.min(cols);
                match op.clone() {
                    Op::Scroll(n) => {
                        for _ in 0..n {
                            list.scroll_up();
                            let (page, row) = list.screen_row_mut(rows - 1);
                            for x in 0..numbered_columns {
                                page.set_cell(row, x, numbered(next));
                                next += 1;
                            }
                        }
                    }
                    Op::MoveRow(from, to) => list.move_row(from % rows, to % rows),
                    Op::MoveCells(from, to, columns) => {
                        let columns = columns.start.min(cols - 1)..columns.end.min(cols);
                        list.move_cells(from % rows, to % rows, columns);
                    }
                    Op::ClearScrollback => list.clear_scrollback(),
                    Op::Grow(y) => list.grow_page(y % rows, TableFull::Styles).unwrap_or(()),
                    Op::Insert(y, x, n, end) => {
                        let end = end.min(cols);
                        list.insert_blanks(y % rows, x.min(end - 1), n, end);
                    }
                    Op::Delete(y, x, n, end) => {
                        let end = end.min(cols);
                        list.delete_cells(y % rows, x.min(end - 1), n, end);
                    }
                    Op::Pin(index, col) => {
                        let (page, row) = list.row_at(index % list.total_rows()).unwrap();
                        let position = Position { page, row, col: col.min(cols - 1) };
                        if let Some(number) = number_at(&list, position) {
                            pins.push((list.pin(position).unwrap(), number));
                        }
                    }
                    Op::Unpin(k) => {
                        if !pins.is_empty() {
                            let (pin, _) = pins.swap_remove(k % pins.len());
                            list.unpin(pin);
                            prop_assert_eq!(list.pin_position(pin), None);
                        }
                    }
                    Op::Resize(cols, rows, how) => {
                        let cursor = (0, list.rows() - 1);
                        let _ = match how {
                            0 => list.resize(cols, rows, cursor, Grow::FromScrollback),
                            1 => list.resize(cols, rows, cursor, Grow::AtTheBottom),
                            _ => list.reflow(cols, rows, cursor),
                        };
                    }
                }
                assert_pins_listed(&list);
                let span = usize::from(SPAN.min(list.cols()));
                let present: std::collections::HashSet<u32> = list
                    .iter_rows()
                    .flat_map(|(page, row)| page.cells(row)[..span].iter())
                    .filter_map(|cell| match cell.content() {
                        Content::BgRgb(Rgb { r, g, b }) => Some(u32::from_be_bytes([0, r, g, b])),
                        _ => None,
                    })
                    .collect();
                for &(pin, number) in &pins {
                    match list.pin_position(pin) {
                        Some(position) => {
                            prop_assert_eq!(number_at(&list, position), Some(number), "after {:?}", op);
                        }
                        None => prop_assert!(!present.contains(&number), "{} is lost but here, after {:?}", number, op),
                    }
                }
                let live = pins.iter().filter(|(pin, _)| list.pin_position(*pin).is_some()).count();
                prop_assert_eq!(list.pins(), live);
            }
        }
    }

    #[test]
    fn rows_are_found_by_index_and_back() {
        let mut list = PageList::new(80, 24, pages(80, 4));
        let page_rows = usize::from(Capacity::standard(80).rows);
        scroll(&mut list, 0, page_rows as u32);
        let total = list.total_rows();
        for index in [0, 5, page_rows - 1, page_rows, total - 1] {
            let (page, row) = list.row_at(index).unwrap();
            assert_eq!(list.row_index(page, row), Some(index), "{index}");
        }
        assert_eq!(list.row_at(total), None);
        let (page, row) = list.row_at(total - 1).unwrap();
        assert_eq!((page, row), list.screen_row_id(23));
        assert_eq!(list.row_index(page, row + 1), None, "past the page's rows");
        let (first, _) = list.row_at(0).unwrap();
        scroll(&mut list, 0, 3 * page_rows as u32);
        assert!(list.page(first).is_none());
        assert_eq!(list.row_index(first, 0), None, "its page left");
        let (page, row) = list.row_at(0).unwrap();
        assert_eq!(list.row_index(page, row), Some(0));
    }
}
