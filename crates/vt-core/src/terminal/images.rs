//! Inline images: the pictures that programs send to show in the
//! terminal's cells, as iTerm2's OSC 1337 `File=` sends them
//! (<https://iterm2.com/documentation-images.html>).
//!
//! The terminal keeps each image's bytes as they came, encoded, with its
//! size in pixels, read from its header, within a quota of memory past
//! which the oldest go; the embedder decodes them as it draws. A placement
//! shows an image over a rectangle of cells, from a pin of its top left
//! cell, so that it keeps its text as output scrolls it into the
//! scrollback and a resize reflows it, and goes as the scrollback's limit
//! or a clearing takes its row. The snapshot carries the placements that
//! the view shows.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::Terminal;
use super::selection::CellPin;

/// The memory that a terminal's images take at most, of their bytes as
/// they came, by default: past it, the oldest go.
pub const IMAGE_MEMORY: usize = 64 * 1024 * 1024;

/// The most pixels of an image, its width by its height, so that one of a
/// small file and a huge size cannot take the memory of its decoding.
pub const MOST_PIXELS: u64 = 16_000_000;

/// The most placements that a terminal keeps, of both screens: each
/// snapshot looks at every one, which costs with their number.
pub const MOST_PLACEMENTS: usize = 1024;

/// The most rows that a placement takes.
const MOST_ROWS: u16 = 1000;

/// The size of a cell in pixels that the sizing takes where the embedder
/// told none.
const CELL: (u16, u16) = (8, 16);

/// The ids of the images that a program does not number, as iTerm2's,
/// from this one up, apart from those that a program numbers.
const ANONYMOUS: u32 = 1 << 31;

/// How an image's bytes are encoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFormat {
    /// PNG.
    Png,
    /// JPEG.
    Jpeg,
    /// GIF, its first frame.
    Gif,
    /// TIFF, its first image, as `chafa` sends iTerm2's images.
    Tiff,
    /// BMP.
    Bmp,
    /// Pixels of four bytes, red, green, blue and alpha, row by row.
    Rgba,
    /// Pixels of three bytes, red, green and blue, row by row.
    Rgb,
}

/// An image that a terminal keeps: its bytes as they came, and its size in
/// pixels.
#[derive(Debug, PartialEq, Eq)]
pub struct Image {
    /// Its id, the program's, or the terminal's of one that it did not
    /// number.
    pub id: u32,
    /// How its bytes are encoded.
    pub format: ImageFormat,
    /// Its width in pixels.
    pub width: u32,
    /// Its height in pixels.
    pub height: u32,
    /// Its bytes.
    pub data: Arc<[u8]>,
}

/// A placement of an image, as the view shows it: its rectangle of cells,
/// from the column and the row of the view of its top left cell, which can
/// be above the view, the image drawn over the rectangle, under the text
/// or over it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placed {
    /// The image.
    pub image: Arc<Image>,
    /// The column of its top left cell.
    pub column: u16,
    /// The row of the view of its top left cell, negative above it.
    pub row: i32,
    /// The columns that it takes.
    pub columns: u16,
    /// The rows that it takes.
    pub rows: u16,
    /// Under the text when negative, over it otherwise; the higher over the
    /// lower.
    pub z: i32,
    /// Whether the image keeps its proportions within the rectangle, from
    /// its top left, rather than filling it.
    pub fit: bool,
    /// The rectangle of the image's pixels that shows, of none the whole.
    pub source: Option<Source>,
}

/// A rectangle of an image's pixels: its left, its top, its width and its
/// height.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Source {
    /// Its left.
    pub x: u32,
    /// Its top.
    pub y: u32,
    /// Its width.
    pub width: u32,
    /// Its height.
    pub height: u32,
}

/// A size that a program gives an image, of a side.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Side {
    /// The image's own, or its proportions' of the other side.
    #[default]
    Auto,
    /// Cells.
    Cells(u32),
    /// Pixels.
    Pixels(u32),
    /// A share of the terminal's side, in percent.
    Percent(u32),
}

/// What a placement asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placing {
    /// Its width.
    pub width: Side,
    /// Its height.
    pub height: Side,
    /// Whether the image keeps its proportions.
    pub preserve: bool,
    /// Whether the cursor moves past the image, scrolling the screen to
    /// make room for it, rather than staying.
    pub move_cursor: bool,
    /// Under the text when negative, over it otherwise.
    pub z: i32,
    /// The rectangle of the image's pixels that shows, whose size its
    /// sides' sizing takes; of none the whole.
    pub source: Option<Source>,
    /// The placement's number, the program's, in place of its image's
    /// placement of that number; of none another.
    pub number: Option<u32>,
}

impl Default for Placing {
    fn default() -> Self {
        Self {
            width: Side::Auto,
            height: Side::Auto,
            preserve: true,
            move_cursor: true,
            z: -1,
            source: None,
            number: None,
        }
    }
}

/// A placement as a deletion looks at it: its image, its number, its
/// cells, from its top left column and row's number, and its z.
#[derive(Clone, Copy, Debug)]
pub(super) struct Seen {
    pub(super) image: u32,
    pub(super) number: Option<u32>,
    pub(super) column: u16,
    pub(super) row: i64,
    pub(super) columns: u16,
    pub(super) rows: u16,
    pub(super) z: i32,
}

impl Seen {
    /// Whether it covers the cell of `column` and the row numbered `row`;
    /// either alone of none of the other.
    pub(super) fn covers(&self, column: Option<u16>, row: Option<i64>) -> bool {
        let columns = u32::from(self.column)..u32::from(self.column) + u32::from(self.columns);
        let rows = self.row..self.row + i64::from(self.rows);
        column.is_none_or(|column| columns.contains(&u32::from(column)))
            && row.is_none_or(|row| rows.contains(&row))
    }
}

/// A placement that a terminal keeps.
#[derive(Debug)]
struct Placement {
    image: u32,
    number: Option<u32>,
    pin: CellPin,
    columns: u16,
    rows: u16,
    z: i32,
    fit: bool,
    source: Option<Source>,
}

/// An image that a terminal keeps, the order in which it came, and whether
/// it stays without a placement, as a program that numbers its images may
/// place it again.
#[derive(Debug)]
struct Kept {
    image: Arc<Image>,
    order: u64,
    lasting: bool,
    /// The number that the program gave it, which it finds it again by.
    number: Option<u32>,
}

/// A terminal's images and their placements.
#[derive(Debug)]
pub(super) struct Images {
    kept: BTreeMap<u32, Kept>,
    placements: Vec<Placement>,
    /// The bytes that the kept images take.
    bytes: usize,
    /// The bytes that they may take.
    limit: usize,
    order: u64,
    next_anonymous: u32,
}

impl Images {
    /// Images of a memory of `limit` bytes at most, none for 0.
    pub(super) fn new(limit: usize) -> Self {
        Self {
            kept: BTreeMap::new(),
            placements: Vec::new(),
            bytes: 0,
            limit,
            order: 0,
            next_anonymous: ANONYMOUS,
        }
    }

    /// The memory that they may take.
    pub(super) fn limit(&self) -> usize {
        self.limit
    }
}

/// The bytes that `text`, base64 of the standard alphabet, stands for, its
/// spaces and line breaks skipped, up to its padding; none of another byte.
pub(super) fn base64(text: &[u8]) -> Option<Vec<u8>> {
    const INVALID: u8 = 0xff;
    const TABLE: [u8; 256] = {
        let mut table = [INVALID; 256];
        let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut at = 0;
        while at < alphabet.len() {
            table[alphabet[at] as usize] = at as u8;
            at += 1;
        }
        table
    };
    let mut data = Vec::with_capacity(text.len() / 4 * 3);
    let (mut bits, mut count) = (0_u32, 0_u32);
    for &byte in text {
        if byte == b'=' {
            break;
        }
        if byte.is_ascii_whitespace() {
            continue;
        }
        let value = TABLE[usize::from(byte)];
        if value == INVALID {
            return None;
        }
        bits = (bits << 6) | u32::from(value);
        count += 6;
        if count >= 8 {
            count -= 8;
            data.push((bits >> count) as u8);
            bits &= (1 << count) - 1;
        }
    }
    Some(data)
}

/// A side of an image as OSC 1337 writes it: `auto`, cells, `px` pixels or
/// a `%` share; none of another text.
fn side(text: &[u8]) -> Option<Side> {
    let text = std::str::from_utf8(text).ok()?;
    if text == "auto" {
        return Some(Side::Auto);
    }
    let number = |digits: &str| digits.parse::<u32>().ok();
    if let Some(digits) = text.strip_suffix("px") {
        return number(digits).map(Side::Pixels);
    }
    if let Some(digits) = text.strip_suffix('%') {
        return number(digits).map(Side::Percent);
    }
    number(text).map(Side::Cells)
}

/// The format, the width and the height in pixels of an image of `data`,
/// as its header tells them: PNG, JPEG, GIF, TIFF or BMP; none of another
/// format.
pub fn dimensions(data: &[u8]) -> Option<(ImageFormat, u32, u32)> {
    if let Some(found) = tiff(data) {
        return Some(found);
    }
    if data.starts_with(b"BM") && data.len() >= 26 {
        let le32 =
            |at: usize| i32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
        let (width, height) = (le32(18), le32(22).unsigned_abs());
        return (width > 0).then(|| (ImageFormat::Bmp, width.unsigned_abs(), height));
    }
    let be16 = |at: usize| {
        Some(u32::from(u16::from_be_bytes([
            *data.get(at)?,
            *data.get(at + 1)?,
        ])))
    };
    let le16 = |at: usize| {
        Some(u32::from(u16::from_le_bytes([
            *data.get(at)?,
            *data.get(at + 1)?,
        ])))
    };
    let be32 = |at: usize| {
        Some(u32::from_be_bytes([
            *data.get(at)?,
            *data.get(at + 1)?,
            *data.get(at + 2)?,
            *data.get(at + 3)?,
        ]))
    };
    if data.starts_with(b"\x89PNG\r\n\x1a\n") && data.get(12..16) == Some(b"IHDR") {
        return Some((ImageFormat::Png, be32(16)?, be32(20)?));
    }
    if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        return Some((ImageFormat::Gif, le16(6)?, le16(8)?));
    }
    if data.starts_with(&[0xff, 0xd8]) {
        // The segments up to a frame's start, each of its length but the
        // markers of none.
        let mut at = 2;
        while at + 4 <= data.len() {
            if data[at] != 0xff {
                return None;
            }
            let marker = data[at + 1];
            if marker == 0xff {
                at += 1;
                continue;
            }
            if matches!(marker, 0x01 | 0xd0..=0xd9) {
                at += 2;
                continue;
            }
            let length = usize::try_from(be16(at + 2)?).ok()?;
            let frame = matches!(marker, 0xc0..=0xcf) && !matches!(marker, 0xc4 | 0xc8 | 0xcc);
            if frame {
                return Some((ImageFormat::Jpeg, be16(at + 7)?, be16(at + 5)?));
            }
            at += 2 + length;
        }
    }
    None
}

/// The size of a TIFF of `data`, of its first directory's width and
/// length, of either byte order; none of another format.
fn tiff(data: &[u8]) -> Option<(ImageFormat, u32, u32)> {
    let little = match data.get(..4)? {
        [b'I', b'I', 42, 0] => true,
        [b'M', b'M', 0, 42] => false,
        _ => return None,
    };
    let u16_at = |at: usize| -> Option<u32> {
        let bytes = [*data.get(at)?, *data.get(at + 1)?];
        Some(u32::from(if little {
            u16::from_le_bytes(bytes)
        } else {
            u16::from_be_bytes(bytes)
        }))
    };
    let u32_at = |at: usize| -> Option<u32> {
        let bytes = [
            *data.get(at)?,
            *data.get(at + 1)?,
            *data.get(at + 2)?,
            *data.get(at + 3)?,
        ];
        Some(if little {
            u32::from_le_bytes(bytes)
        } else {
            u32::from_be_bytes(bytes)
        })
    };
    let directory = usize::try_from(u32_at(4)?).ok()?;
    let entries = u16_at(directory)?;
    let (mut width, mut height) = (None, None);
    for entry in 0..entries as usize {
        let at = directory + 2 + entry * 12;
        let tag = u16_at(at)?;
        // A SHORT's value in the field's first two bytes, a LONG's in its
        // four.
        let value = match u16_at(at + 2)? {
            3 => u16_at(at + 8)?,
            4 => u32_at(at + 8)?,
            _ => continue,
        };
        match tag {
            256 => width = Some(value),
            257 => height = Some(value),
            _ => {}
        }
    }
    Some((ImageFormat::Tiff, width?, height?))
}

impl Terminal {
    /// OSC 1337 `File=`, as iTerm2 takes it: `File=` arguments of
    /// `key=value` between `;`, then `:` and the file in base64. Of
    /// `inline=1` alone, an image of its header's format shows at the
    /// cursor, of its `width` and `height`, `auto` by default, its
    /// proportions kept unless `preserveAspectRatio=0`, and the cursor past
    /// it unless `doNotMoveCursor=1`. A file to download, another format, or
    /// a terminal of no memory for images, shows nothing.
    pub(super) fn osc_1337(&mut self, rest: &[u8]) {
        let Some(rest) = rest.strip_prefix(b"File=") else {
            return;
        };
        let Some(colon) = rest.iter().position(|&byte| byte == b':') else {
            return;
        };
        let (arguments, payload) = (&rest[..colon], &rest[colon + 1..]);
        let mut placing = Placing::default();
        let mut inline = false;
        for argument in arguments.split(|&byte| byte == b';') {
            let Some(at) = argument.iter().position(|&byte| byte == b'=') else {
                continue;
            };
            let (key, value) = (&argument[..at], &argument[at + 1..]);
            match key {
                b"inline" => inline = value == b"1",
                b"width" => placing.width = side(value).unwrap_or_default(),
                b"height" => placing.height = side(value).unwrap_or_default(),
                b"preserveAspectRatio" => placing.preserve = value != b"0",
                b"doNotMoveCursor" => placing.move_cursor = value != b"1",
                _ => {}
            }
        }
        if !inline || self.images.limit == 0 {
            return;
        }
        let Some(data) = base64(payload) else {
            return;
        };
        let Some(found) = dimensions(&data) else {
            return;
        };
        let Some(id) = self.keep_image(None, found, data, false) else {
            return;
        };
        if !self.place_image(id, placing) {
            self.drop_image(id);
        }
    }

    /// Keeps the image of `data`, in `format`, of `width` by `height`
    /// pixels, as `id`, the program's or, of none, one of the terminal's,
    /// in place of one of that id, `lasting` without a placement or going
    /// with its last: its id, or none of an image of no size, of more
    /// pixels than [`MOST_PIXELS`], or of more bytes than the memory that
    /// images may take. The oldest images go, as many as this one needs,
    /// those of no placement first.
    pub fn keep_image(
        &mut self,
        id: Option<u32>,
        (format, width, height): (ImageFormat, u32, u32),
        data: Vec<u8>,
        lasting: bool,
    ) -> Option<u32> {
        let images = &self.images;
        if width == 0
            || height == 0
            || u64::from(width) * u64::from(height) > MOST_PIXELS
            || data.len() > images.limit
        {
            return None;
        }
        let id = match id {
            Some(id) => id,
            None => {
                let images = &mut self.images;
                let id = images.next_anonymous;
                images.next_anonymous = images.next_anonymous.checked_add(1).unwrap_or(ANONYMOUS);
                id
            }
        };
        self.drop_image(id);
        while self.images.bytes + data.len() > self.images.limit {
            if !self.drop_oldest_image() {
                return None;
            }
        }
        let images = &mut self.images;
        images.order += 1;
        images.bytes += data.len();
        let image = Arc::new(Image {
            id,
            format,
            width,
            height,
            data: Arc::from(data),
        });
        images.kept.insert(
            id,
            Kept {
                image,
                order: images.order,
                lasting,
                number: None,
            },
        );
        Some(id)
    }

    /// Lets the image `id` go, with its placements.
    pub fn drop_image(&mut self, id: u32) {
        let gone: Vec<Placement> = {
            let images = &mut self.images;
            let (gone, kept) = std::mem::take(&mut images.placements)
                .into_iter()
                .partition(|placement| placement.image == id);
            images.placements = kept;
            if let Some(kept) = images.kept.remove(&id) {
                images.bytes -= kept.image.data.len();
            }
            gone
        };
        for placement in gone {
            self.unpin_cell(&placement.pin);
        }
    }

    /// Lets the oldest image go, one of no placement before the others;
    /// whether there was one.
    fn drop_oldest_image(&mut self) -> bool {
        let images = &self.images;
        let placed = |id: u32| {
            images
                .placements
                .iter()
                .any(|placement| placement.image == id)
        };
        let oldest = images
            .kept
            .iter()
            .min_by_key(|(id, kept)| (placed(**id), kept.order))
            .map(|(&id, _)| id);
        match oldest {
            Some(id) => {
                self.drop_image(id);
                true
            }
            None => false,
        }
    }

    /// Places the kept image `id` at the cursor, as `placing` asks: over
    /// the cells that its size takes, of the cell's size in pixels, within
    /// the screen's width; the cursor past its last column on its last row,
    /// the screen scrolling to make room for its rows, as `placing` moves
    /// it; whether it placed it. The oldest placement goes past
    /// [`MOST_PLACEMENTS`].
    pub fn place_image(&mut self, id: u32, placing: Placing) -> bool {
        let Some(image) = self
            .images
            .kept
            .get(&id)
            .map(|kept| Arc::clone(&kept.image))
        else {
            return false;
        };
        let cell = self
            .cell_size
            .filter(|&(w, h)| w > 0 && h > 0)
            .unwrap_or(CELL);
        let source = placing
            .source
            .map(|source| clipped(source, (image.width, image.height)));
        if source.is_some_and(|source| source.width == 0 || source.height == 0) {
            return false;
        }
        let shown = source.map_or((image.width, image.height), |source| {
            (source.width, source.height)
        });
        let (columns, mut rows) = cells(&placing, shown, cell, (self.cols, self.rows));
        let start = self.cursor();
        let margins = self.margins;
        // A scroll of the whole screen keeps each row's number as the row
        // moves up, into the scrollback, or out of the list; a region's
        // moves the rows' text within numbers that stay.
        let whole =
            margins.top == 0 && margins.bottom == self.rows - 1 && margins.is_full_width(self.cols);
        let start_number = self.cursor_cell().1;
        let mut scrolls: i64 = 0;
        if placing.move_cursor {
            for _ in 1..rows {
                if self.cursor().y == margins.bottom && self.index_moves_on() {
                    scrolls += 1;
                }
                self.index();
            }
        }
        // The rows of the image's top that left the list, or went above
        // the region, which its source loses.
        let (number, cut) = if whole {
            // A screen of no scrollback keeps the rows above it a while, as
            // its pages go whole, but shows none of them.
            let first = if self.alternate_active || self.scrollback_limit == 0 {
                self.screen_top()
            } else {
                self.row_number(0)
            };
            (start_number.max(first), (first - start_number).max(0))
        } else {
            let within = (margins.top..=margins.bottom).contains(&start.y);
            let top = i64::from(start.y) - if within { scrolls } else { 0 };
            let floor = i64::from(margins.top);
            let index = self.screen().rows.scrollback_rows() as i64 + top.max(floor);
            (self.row_number(index as usize), (floor - top).max(0))
        };
        let mut source = source;
        if cut > 0 {
            if cut >= i64::from(rows) {
                return false;
            }
            let whole_source = source.unwrap_or(Source {
                x: 0,
                y: 0,
                width: image.width,
                height: image.height,
            });
            // Fewer rows than a `u16` holds, and pixels than a `u32`.
            let lost = (u64::from(whole_source.height) * cut as u64 / u64::from(rows)) as u32;
            source = Some(Source {
                y: whole_source.y + lost,
                height: whole_source.height - lost,
                ..whole_source
            });
            rows -= cut as u16;
        }
        let Some(pin) = self.pin_cell((start.x, number)) else {
            return false;
        };
        // A placement of the image's number goes before this one.
        if let Some(number) = placing.number {
            self.drop_placements(
                |seen| seen.image == id && seen.number == Some(number),
                false,
            );
        }
        while self.images.placements.len() >= MOST_PLACEMENTS {
            let oldest = self.images.placements.remove(0);
            self.unpin_cell(&oldest.pin);
            self.drop_unplaced(oldest.image);
        }
        self.images.placements.push(Placement {
            image: id,
            number: placing.number,
            pin,
            columns,
            rows,
            z: placing.z,
            fit: placing.preserve,
            source,
        });
        if placing.move_cursor {
            let x = u32::from(start.x) + u32::from(columns);
            let cols = self.cols;
            let cursor = &mut self.screen_mut().cursor;
            if x >= u32::from(cols) {
                cursor.x = cols - 1;
                cursor.pending_wrap = true;
            } else {
                cursor.x = x as u16;
                cursor.pending_wrap = false;
            }
        }
        true
    }

    /// Lets the image `id` go if it lasts with its placements alone and
    /// has none left.
    fn drop_unplaced(&mut self, id: u32) {
        let images = &self.images;
        let lasting = images.kept.get(&id).is_none_or(|kept| kept.lasting);
        let placed = images
            .placements
            .iter()
            .any(|placement| placement.image == id);
        if !lasting && !placed {
            self.drop_image(id);
        }
    }

    /// Lets the placements of the alternate screen go, as it is erased or
    /// left, with the images that went with them.
    pub(super) fn drop_alternate_images(&mut self) {
        let gone: Vec<Placement> = {
            let placements = &mut self.images.placements;
            let (gone, kept) = std::mem::take(placements)
                .into_iter()
                .partition(|placement| placement.pin.is_alternate());
            *placements = kept;
            gone
        };
        for placement in gone {
            self.unpin_cell(&placement.pin);
            self.drop_unplaced(placement.image);
        }
    }

    /// Gives the kept image `id` the program's `number`, which finds it
    /// again.
    pub(super) fn number_image(&mut self, id: u32, number: u32) {
        if let Some(kept) = self.images.kept.get_mut(&id) {
            kept.number = Some(number);
        }
    }

    /// The id of the newest kept image of the program's `number`.
    pub(super) fn numbered_image(&self, number: u32) -> Option<u32> {
        self.images
            .kept
            .iter()
            .filter(|(_, kept)| kept.number == Some(number))
            .max_by_key(|(_, kept)| kept.order)
            .map(|(&id, _)| id)
    }

    /// Whether the terminal keeps the image `id`.
    pub(super) fn keeps_image(&self, id: u32) -> bool {
        self.images.kept.contains_key(&id)
    }

    /// Lets the active screen's placements go that `which` takes, as their
    /// cells are now, and, `free`, the images that they leave of no
    /// placement; the placements whose text left go too.
    pub(super) fn drop_placements(&mut self, which: impl Fn(&Seen) -> bool, free: bool) {
        let seen: Vec<Option<Seen>> = self
            .images
            .placements
            .iter()
            .map(|placement| {
                let (column, row) = self.pinned_cell(&placement.pin)?;
                Some(Seen {
                    image: placement.image,
                    number: placement.number,
                    column,
                    row,
                    columns: placement.columns,
                    rows: placement.rows,
                    z: placement.z,
                })
            })
            .collect();
        let mut gone = Vec::new();
        for at in (0..seen.len()).rev() {
            let alternate = self.images.placements[at].pin.is_alternate();
            let lost = self.is_lost(&self.images.placements[at].pin);
            let taken = seen[at].as_ref().is_some_and(&which);
            if lost || (taken && alternate == self.alternate_active) {
                gone.push(self.images.placements.remove(at));
            }
        }
        for placement in gone {
            self.unpin_cell(&placement.pin);
            if free {
                let placed = self
                    .images
                    .placements
                    .iter()
                    .any(|other| other.image == placement.image);
                if !placed {
                    self.drop_image(placement.image);
                }
            } else {
                self.drop_unplaced(placement.image);
            }
        }
    }

    /// The rows of the screen, from its top, down to the last that a
    /// placement of the primary screen covers: 0 of none.
    pub(super) fn rows_under_images(&self) -> u16 {
        if self.alternate_active {
            return 0;
        }
        let top = self.screen_top();
        let last = i64::from(self.rows);
        self.images
            .placements
            .iter()
            .filter(|placement| !placement.pin.is_alternate())
            .filter_map(|placement| {
                let (_, row) = self.pinned_cell(&placement.pin)?;
                let end = row - top + i64::from(placement.rows);
                (end > 0).then(|| end.min(last) as u16)
            })
            .max()
            .unwrap_or(0)
    }

    /// Lets the placements go that start on the active screen, as an
    /// erase of it in place takes their text.
    pub(super) fn drop_screen_images(&mut self) {
        let top = self.screen_top();
        let bottom = top + i64::from(self.rows);
        self.drop_placements(|seen| (top..bottom).contains(&seen.row), false);
    }

    /// The number of the active screen's first row.
    pub(super) fn screen_top(&self) -> i64 {
        self.row_number(self.screen().rows.scrollback_rows())
    }

    /// The kept images, by their ids, and the bytes that they take.
    pub fn kept_images(&self) -> (Vec<u32>, usize) {
        (
            self.images.kept.keys().copied().collect(),
            self.images.bytes,
        )
    }

    /// The placements that the view shows of the active screen's images,
    /// the lowest first, in the order that they came of the same; the
    /// placements whose text left go, with the images that went with them.
    /// Nothing, and no cost, without placements.
    pub(super) fn images_view(&mut self) -> Vec<Placed> {
        if self.images.placements.is_empty() {
            return Vec::new();
        }
        let lost: Vec<usize> = self
            .images
            .placements
            .iter()
            .enumerate()
            .filter(|(_, placement)| self.is_lost(&placement.pin))
            .map(|(at, _)| at)
            .collect();
        for at in lost.into_iter().rev() {
            let placement = self.images.placements.remove(at);
            self.unpin_cell(&placement.pin);
            self.drop_unplaced(placement.image);
        }
        let top = self.origin().top;
        let shown = i64::from(self.rows);
        let mut placed: Vec<Placed> = self
            .images
            .placements
            .iter()
            .filter_map(|placement| {
                let (column, row) = self.pinned_cell(&placement.pin)?;
                let row = row - top;
                if row >= shown || row + i64::from(placement.rows) <= 0 {
                    return None;
                }
                let image = Arc::clone(&self.images.kept.get(&placement.image)?.image);
                Some(Placed {
                    image,
                    column,
                    row: i32::try_from(row).ok()?,
                    columns: placement.columns,
                    rows: placement.rows,
                    z: placement.z,
                    fit: placement.fit,
                    source: placement.source,
                })
            })
            .collect();
        placed.sort_by_key(|placed| placed.z);
        placed
    }
}

/// `source` within an image of `width` by `height` pixels.
fn clipped(source: Source, (width, height): (u32, u32)) -> Source {
    let x = source.x.min(width);
    let y = source.y.min(height);
    let wide = |asked: u32, room: u32| if asked == 0 { room } else { asked.min(room) };
    Source {
        x,
        y,
        width: wide(source.width, width - x),
        height: wide(source.height, height - y),
    }
}

/// The columns and the rows that an image of `pixels` takes, as `placing`
/// sizes it, of cells of `cell` pixels in a terminal of `screen` columns
/// and rows: its own size, or that of a side and its proportions' of the
/// other; never wider than the screen, as its proportions keep it, one cell
/// at least.
fn cells(
    placing: &Placing,
    (width, height): (u32, u32),
    (cell_width, cell_height): (u16, u16),
    (columns, rows): (u16, u16),
) -> (u16, u16) {
    let (cell_width, cell_height) = (f64::from(cell_width), f64::from(cell_height));
    let side = |side: Side, cell: f64, cells: u16| -> Option<f64> {
        match side {
            Side::Auto => None,
            Side::Cells(n) => Some(f64::from(n) * cell),
            Side::Pixels(n) => Some(f64::from(n)),
            Side::Percent(n) => Some(f64::from(n.min(100)) / 100.0 * f64::from(cells) * cell),
        }
    };
    let (own_width, own_height) = (f64::from(width), f64::from(height));
    let asked = (
        side(placing.width, cell_width, columns),
        side(placing.height, cell_height, rows),
    );
    let (mut w, mut h) = match asked {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) if placing.preserve => (w, w * own_height / own_width),
        (None, Some(h)) if placing.preserve => (h * own_width / own_height, h),
        (Some(w), None) => (w, own_height),
        (None, Some(h)) => (own_width, h),
        (None, None) => (own_width, own_height),
    };
    let widest = f64::from(columns) * cell_width;
    if w > widest {
        if placing.preserve && asked.1.is_none() {
            h *= widest / w;
        }
        w = widest;
    }
    let count = |pixels: f64, cell: f64, most: u16| -> u16 {
        let cells = (pixels / cell).ceil();
        if cells.is_finite() && cells >= 1.0 {
            cells.min(f64::from(most)) as u16
        } else {
            1
        }
    };
    (
        count(w, cell_width, columns),
        count(h, cell_height, MOST_ROWS),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The header of a PNG of `width` by `height` pixels.
    pub(crate) fn png(width: u32, height: u32) -> Vec<u8> {
        let mut data = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        data.extend(width.to_be_bytes());
        data.extend(height.to_be_bytes());
        data.extend([8, 6, 0, 0, 0]);
        data
    }

    use crate::parser::Parser;
    use crate::terminal::{Options, Snapshot};

    /// A terminal of `cols` by `rows` cells of 10 by 20 pixels, of
    /// `memory` bytes for images, and its parser.
    fn made(cols: u16, rows: u16, memory: usize) -> (Terminal, Parser) {
        let options = Options {
            image_memory: memory,
            ..Options::default()
        };
        let mut terminal = Terminal::new(cols, rows, options);
        terminal.set_cell_size(10, 20);
        (terminal, Parser::new())
    }

    /// OSC 1337 `File=` of `arguments` and `data`, in base64.
    fn file(arguments: &str, data: &[u8]) -> Vec<u8> {
        let mut sequence = format!("\x1b]1337;File={arguments}:").into_bytes();
        sequence.extend(super::super::clipboard::tests_encode(data).bytes());
        sequence.push(0x07);
        sequence
    }

    /// The placements that the view of `terminal` shows: each image's id,
    /// its column, its row, its columns and its rows.
    fn shown(terminal: &mut Terminal) -> Vec<(u32, u16, i32, u16, u16)> {
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        snapshot
            .images()
            .iter()
            .map(|placed| {
                (
                    placed.image.id,
                    placed.column,
                    placed.row,
                    placed.columns,
                    placed.rows,
                )
            })
            .collect()
    }

    #[test]
    fn an_inline_file_shows_at_the_cursor_and_moves_it_past_the_image() {
        let (mut terminal, mut parser) = made(20, 6, IMAGE_MEMORY);
        parser.advance(&mut terminal, b"ab");
        // 50 by 40 pixels: 5 columns by 2 rows of cells of 10 by 20.
        parser.advance(&mut terminal, &file("inline=1", &png(50, 40)));
        let placed = shown(&mut terminal);
        assert_eq!(placed.len(), 1);
        let (_, column, row, columns, rows) = placed[0];
        assert_eq!((column, row, columns, rows), (2, 0, 5, 2));
        // The cursor on the image's last row, past its last column.
        assert_eq!((terminal.cursor().x, terminal.cursor().y), (7, 1));
        let (ids, bytes) = terminal.kept_images();
        assert_eq!((ids.len(), bytes), (1, png(50, 40).len()));
        // A file to download, another format, or base64 that is not, shows
        // nothing.
        parser.advance(&mut terminal, &file("name=eA==", &png(10, 10)));
        parser.advance(&mut terminal, &file("inline=1", b"BM not an image"));
        parser.advance(&mut terminal, b"\x1b]1337;File=inline=1:!!!\x07");
        assert_eq!(terminal.kept_images().0.len(), 1);
    }

    #[test]
    fn an_image_scrolls_with_its_rows_and_goes_as_they_leave() {
        let (mut terminal, mut parser) = made(20, 4, IMAGE_MEMORY);
        // At the bottom, its rows scroll the screen to make room.
        parser.advance(&mut terminal, b"\r\n\r\n\r\n");
        parser.advance(
            &mut terminal,
            &file("inline=1;width=3;height=2", &png(30, 40)),
        );
        assert_eq!(shown(&mut terminal)[0].2, 2);
        // Output scrolls it up, then out of the view; the view scrolled
        // back shows it again.
        parser.advance(&mut terminal, b"\r\n");
        assert_eq!(shown(&mut terminal)[0].2, 1);
        parser.advance(&mut terminal, b"\r\n\r\n\r\n\r\n");
        assert!(shown(&mut terminal).is_empty());
        assert_eq!(terminal.kept_images().0.len(), 1);
        // A clearing of the scrollback takes its rows, and it goes.
        parser.advance(&mut terminal, b"\x1b[3J");
        shown(&mut terminal);
        assert_eq!(terminal.kept_images(), (Vec::new(), 0));
    }

    /// The rectangle of the image's pixels of the view's placements.
    fn sources(terminal: &mut Terminal) -> Vec<Option<Source>> {
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        snapshot
            .images()
            .iter()
            .map(|placed| placed.source)
            .collect()
    }

    #[test]
    fn an_image_taller_than_the_screen_starts_up_in_the_scrollback() {
        let (mut terminal, mut parser) = made(20, 6, IMAGE_MEMORY);
        parser.advance(&mut terminal, b"$ imgcat\r\n");
        // 30 by 200 pixels: 3 columns by 10 rows, of a screen of 6.
        parser.advance(&mut terminal, &file("inline=1", &png(30, 200)));
        let placed = shown(&mut terminal);
        assert_eq!(placed.len(), 1);
        let (_, column, row, columns, rows) = placed[0];
        assert_eq!((column, row, columns, rows), (0, -4, 3, 10));
        assert_eq!(sources(&mut terminal), [None]);
        // The cursor on the image's last row, the screen's.
        assert_eq!((terminal.cursor().x, terminal.cursor().y), (3, 5));
    }

    #[test]
    fn an_image_taller_than_the_alternate_screen_loses_its_top() {
        let (mut terminal, mut parser) = made(20, 6, IMAGE_MEMORY);
        parser.advance(&mut terminal, b"\x1b[?1049h\r\n");
        parser.advance(&mut terminal, &file("inline=1", &png(30, 200)));
        let placed = shown(&mut terminal);
        assert_eq!(placed.len(), 1);
        let (_, column, row, columns, rows) = placed[0];
        // Four of its ten rows scrolled off a screen of no scrollback.
        assert_eq!((column, row, columns, rows), (0, 0, 3, 6));
        assert_eq!(
            sources(&mut terminal),
            [Some(Source {
                x: 0,
                y: 80,
                width: 30,
                height: 120,
            })]
        );
    }

    #[test]
    fn an_image_in_a_scroll_region_scrolls_the_region_alone() {
        // Below the region, the last row stops the cursor: the image starts
        // at the cursor, over no row above it.
        let (mut terminal, mut parser) = made(20, 10, IMAGE_MEMORY);
        parser.advance(&mut terminal, b"\x1b[1;4r\x1b[10;1H");
        parser.advance(&mut terminal, &file("inline=1", &png(30, 60)));
        assert_eq!(shown(&mut terminal)[0].2, 9);
        // At the region's bottom, the region scrolls under the image.
        let (mut terminal, mut parser) = made(20, 10, IMAGE_MEMORY);
        parser.advance(&mut terminal, b"\x1b[1;4r\x1b[4;1H");
        parser.advance(&mut terminal, &file("inline=1", &png(30, 60)));
        assert_eq!(shown(&mut terminal)[0].2, 1);
        assert_eq!(terminal.cursor().y, 3);
        // Taller than the region, its top lost above the region's.
        let (mut terminal, mut parser) = made(20, 10, IMAGE_MEMORY);
        parser.advance(&mut terminal, b"\x1b[4;7r\x1b[7;1H");
        parser.advance(&mut terminal, &file("inline=1", &png(30, 120)));
        let (_, _, row, _, rows) = shown(&mut terminal)[0];
        assert_eq!((row, rows), (3, 4));
        assert_eq!(
            sources(&mut terminal),
            [Some(Source {
                x: 0,
                y: 40,
                width: 30,
                height: 80,
            })]
        );
    }

    #[test]
    fn a_clear_takes_the_screen_s_images_with_its_rows() {
        let (mut terminal, mut parser) = made(20, 6, IMAGE_MEMORY);
        // `clear; imgcat` in a loop: each image, of no text after it, goes
        // to the scrollback with the screen.
        for _ in 0..3 {
            parser.advance(&mut terminal, b"\x1b[H\x1b[2J");
            parser.advance(&mut terminal, &file("inline=1", &png(30, 40)));
            parser.advance(&mut terminal, b"\n");
        }
        let placed = shown(&mut terminal);
        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].2, 0);
        // Of ED 3 too, the others go with the scrollback.
        parser.advance(&mut terminal, b"\x1b[H\x1b[2J\x1b[3J");
        assert!(shown(&mut terminal).is_empty());
        // A selective erase, in place, takes the screen's images.
        parser.advance(&mut terminal, &file("inline=1", &png(30, 40)));
        assert_eq!(shown(&mut terminal).len(), 1);
        parser.advance(&mut terminal, b"\x1b[?2J");
        assert!(shown(&mut terminal).is_empty());
    }

    #[test]
    fn the_alternate_screen_s_images_go_as_it_is_left() {
        let (mut terminal, mut parser) = made(20, 6, IMAGE_MEMORY);
        parser.advance(&mut terminal, b"\x1b[?1049h");
        parser.advance(&mut terminal, &file("inline=1", &png(20, 20)));
        assert_eq!(shown(&mut terminal).len(), 1);
        parser.advance(&mut terminal, b"\x1b[?1049l");
        assert!(shown(&mut terminal).is_empty());
        assert_eq!(terminal.kept_images().0.len(), 0);
        // The primary's stay on the screen that it shows again.
        parser.advance(&mut terminal, &file("inline=1", &png(20, 20)));
        parser.advance(&mut terminal, b"\x1b[?1049h");
        assert!(shown(&mut terminal).is_empty());
        parser.advance(&mut terminal, b"\x1b[?1049l");
        assert_eq!(shown(&mut terminal).len(), 1);
    }

    #[test]
    fn the_images_stay_within_their_memory_and_their_pixels() {
        let image = png(10, 10);
        // Room for two: the third takes the first's place.
        let (mut terminal, mut parser) = made(20, 6, image.len() * 2);
        for _ in 0..3 {
            parser.advance(&mut terminal, &file("inline=1", &image));
        }
        let (ids, bytes) = terminal.kept_images();
        assert_eq!((ids.len(), bytes), (2, image.len() * 2));
        assert_eq!(shown(&mut terminal).len(), 2);
        // An image of too many pixels, and a terminal of no memory for
        // images, keep nothing.
        let (mut terminal, mut parser) = made(20, 6, IMAGE_MEMORY);
        parser.advance(&mut terminal, &file("inline=1", &png(10_000, 10_000)));
        assert_eq!(terminal.kept_images().0.len(), 0);
        let (mut terminal, mut parser) = made(20, 6, 0);
        parser.advance(&mut terminal, &file("inline=1", &png(10, 10)));
        assert_eq!(terminal.kept_images().0.len(), 0);
    }

    #[test]
    fn base64_skips_line_breaks_and_refuses_other_bytes() {
        assert_eq!(base64(b"Zm9v\nYmFy"), Some(b"foobar".to_vec()));
        assert_eq!(base64(b"Zm8="), Some(b"fo".to_vec()));
        assert_eq!(base64(b"Zm9v!"), None);
    }

    #[test]
    fn an_image_s_header_tells_its_format_and_its_size() {
        assert_eq!(
            dimensions(&png(640, 480)),
            Some((ImageFormat::Png, 640, 480))
        );
        let mut gif = b"GIF89a".to_vec();
        gif.extend(320_u16.to_le_bytes());
        gif.extend(200_u16.to_le_bytes());
        assert_eq!(dimensions(&gif), Some((ImageFormat::Gif, 320, 200)));
        // A JPEG's frame after an APP0 segment.
        let mut jpeg = vec![0xff, 0xd8, 0xff, 0xe0, 0x00, 0x04, 0x00, 0x00];
        jpeg.extend([0xff, 0xc0, 0x00, 0x11, 0x08]);
        jpeg.extend(600_u16.to_be_bytes());
        jpeg.extend(800_u16.to_be_bytes());
        assert_eq!(dimensions(&jpeg), Some((ImageFormat::Jpeg, 800, 600)));
        // A TIFF of either byte order, as `chafa` sends one: a directory of
        // a SHORT width and a LONG length.
        let mut tiff = b"II*\0\x08\0\0\0\x02\0".to_vec();
        tiff.extend([0x00, 0x01, 3, 0, 1, 0, 0, 0, 0x40, 0x01, 0, 0]);
        tiff.extend([0x01, 0x01, 4, 0, 1, 0, 0, 0, 0xc8, 0, 0, 0]);
        assert_eq!(dimensions(&tiff), Some((ImageFormat::Tiff, 320, 200)));
        let mut big = b"MM\0*\0\0\0\x08\0\x02".to_vec();
        big.extend([0x01, 0x00, 0, 3, 0, 0, 0, 1, 0x01, 0x40, 0, 0]);
        big.extend([0x01, 0x01, 0, 4, 0, 0, 0, 1, 0, 0, 0, 0xc8]);
        assert_eq!(dimensions(&big), Some((ImageFormat::Tiff, 320, 200)));
        // A BMP, top-down of a negative height.
        let mut bmp = b"BM".to_vec();
        bmp.extend([0; 16]);
        bmp.extend(40_i32.to_le_bytes());
        bmp.extend((-30_i32).to_le_bytes());
        assert_eq!(dimensions(&bmp), Some((ImageFormat::Bmp, 40, 30)));
        // Another format, a header cut short, and nothing.
        assert_eq!(dimensions(b"BM\0\0"), None);
        assert_eq!(dimensions(b"II*\0\xff\xff\0\0"), None);
        assert_eq!(dimensions(&png(1, 1)[..18]), None);
        assert_eq!(dimensions(&[0xff, 0xd8, 0xff]), None);
        assert_eq!(dimensions(b""), None);
    }

    #[test]
    fn an_image_takes_the_cells_of_its_size_or_of_one_side_and_its_proportions() {
        let screen = (80, 24);
        let cell = (10, 20);
        let of = |width, height, preserve| Placing {
            width,
            height,
            preserve,
            ..Placing::default()
        };
        // Its own: 200 by 100 pixels, 20 columns by 5 rows.
        assert_eq!(
            cells(&of(Side::Auto, Side::Auto, true), (200, 100), cell, screen),
            (20, 5)
        );
        // A width of cells, its height of its proportions.
        assert_eq!(
            cells(
                &of(Side::Cells(10), Side::Auto, true),
                (200, 100),
                cell,
                screen
            ),
            (10, 3)
        );
        // Pixels, a share of the screen, and both sides of no proportions.
        assert_eq!(
            cells(
                &of(Side::Pixels(55), Side::Pixels(41), false),
                (200, 100),
                cell,
                screen
            ),
            (6, 3)
        );
        assert_eq!(
            cells(
                &of(Side::Percent(50), Side::Auto, true),
                (200, 100),
                cell,
                screen
            ),
            (40, 10)
        );
        // Wider than the screen, the screen's width, its proportions kept.
        assert_eq!(
            cells(&of(Side::Auto, Side::Auto, true), (1600, 400), cell, screen),
            (80, 10)
        );
        // One cell at least.
        assert_eq!(
            cells(&of(Side::Auto, Side::Auto, true), (1, 1), cell, screen),
            (1, 1)
        );
    }
}
