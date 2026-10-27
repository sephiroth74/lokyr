//! The atlas of glyphs that every window draws from: two square textures,
//! one of coverage and one of color, packed in shelves.
//!
//! The atlas keeps where each glyph is, not its pixels: what changes, the
//! pixels of a glyph to upload or a texture that grew, waits for the renderer
//! to take it, in order. A texture grows by doubling its side, which keeps
//! every glyph where it was: the renderer copies the old texture into the
//! corner of the new one. Past its largest size, it evicts the shelf least
//! recently used, but never one that the frame in progress uses, nor the
//! frame before it, which the GPU may still be drawing.

use std::hash::Hash;

use crate::backend::{Bitmap, BitmapFormat};
use crate::hash::FastMap;

/// The side, in pixels, that a texture starts at.
pub const START: u32 = 1024;
/// The largest side of the texture of coverage: 16 MB.
pub const GRAY_MAX: u32 = 4096;
/// The largest side of the texture of color: 16 MB.
pub const COLOR_MAX: u32 = 2048;

/// The pixels of blank to the right of and below each glyph, so that a
/// glyph's edge never samples its neighbour's.
const GUTTER: u32 = 1;

/// The rows that a shelf's height rounds up to a multiple of, so that
/// glyphs of near heights share shelves.
const SHELF_STEP: u32 = 4;

/// Where a glyph is in the atlas, and where it goes from the pen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    /// The texture that holds it.
    pub format: BitmapFormat,
    /// Its column in the texture.
    pub x: u32,
    /// Its row in the texture.
    pub y: u32,
    /// Its width, in pixels: none for a glyph without ink.
    pub width: u32,
    /// Its height, in pixels.
    pub height: u32,
    /// How far its left edge is from the pen, rightwards.
    pub left: i32,
    /// How far its top edge is from the baseline, upwards.
    pub top: i32,
}

/// What changed in a texture, for the renderer to do before it draws, in
/// the order that the atlas gives them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// The texture's side grew to `size`: the old texture's pixels go in its
    /// top left corner.
    Grown {
        /// The texture.
        format: BitmapFormat,
        /// Its new side, in pixels.
        size: u32,
    },
    /// Pixels to write into the texture.
    Upload {
        /// The texture.
        format: BitmapFormat,
        /// The region's column.
        x: u32,
        /// The region's row.
        y: u32,
        /// The region's width.
        width: u32,
        /// The region's height.
        height: u32,
        /// Its rows, from the top, of the texture's bytes a pixel.
        data: Vec<u8>,
    },
}

/// The atlas of glyphs, of keys `K`.
#[derive(Debug)]
pub struct Atlas<K> {
    gray: Page<K>,
    color: Page<K>,
    /// The frame in progress, which marks what it uses.
    frame: u64,
    changes: Vec<Change>,
}

impl<K: Hash + Eq + Clone> Default for Atlas<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Hash + Eq + Clone> Atlas<K> {
    /// An empty atlas of the sizes of [`START`], [`GRAY_MAX`] and
    /// [`COLOR_MAX`].
    pub fn new() -> Self {
        Self::with_sizes(START, GRAY_MAX, COLOR_MAX)
    }

    /// An empty atlas whose textures start at `start` pixels a side, and
    /// grow up to `gray_max` and `color_max`.
    pub fn with_sizes(start: u32, gray_max: u32, color_max: u32) -> Self {
        Self {
            gray: Page::new(BitmapFormat::Gray, start.min(gray_max), gray_max),
            color: Page::new(BitmapFormat::Rgba, start.min(color_max), color_max),
            frame: 1,
            changes: Vec::new(),
        }
    }

    /// Starts a frame: what it uses, it keeps from eviction.
    pub fn begin_frame(&mut self) {
        self.frame += 1;
    }

    /// Where the glyph of `key` is, if the atlas has it, which marks it as
    /// the frame's.
    pub fn get(&mut self, key: &K) -> Option<Placement> {
        let frame = self.frame;
        self.gray
            .get(key, frame)
            .or_else(|| self.color.get(key, frame))
    }

    /// Puts `bitmap` in the atlas as the glyph of `key`, and queues its
    /// pixels for the renderer. Returns where it went; `None` when it is
    /// larger than the texture's largest side, or when the frame in progress
    /// and the one before already use all of the texture.
    pub fn insert(&mut self, key: K, bitmap: &Bitmap) -> Option<Placement> {
        let frame = self.frame;
        let page = match bitmap.format {
            BitmapFormat::Gray => &mut self.gray,
            BitmapFormat::Rgba => &mut self.color,
        };
        page.insert(key, bitmap, frame, &mut self.changes)
    }

    /// The changes since the last take, in order.
    pub fn take_changes(&mut self) -> Vec<Change> {
        std::mem::take(&mut self.changes)
    }

    /// The side of the texture of `format`, in pixels.
    pub fn texture_size(&self, format: BitmapFormat) -> u32 {
        match format {
            BitmapFormat::Gray => self.gray.size,
            BitmapFormat::Rgba => self.color.size,
        }
    }

    /// The bytes that the two textures take on the GPU.
    pub fn texture_bytes(&self) -> usize {
        self.gray.bytes() + self.color.bytes()
    }

    /// How many glyphs the atlas holds.
    pub fn len(&self) -> usize {
        self.gray.entries.len() + self.color.entries.len()
    }

    /// Whether the atlas holds no glyph.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A glyph in a page.
#[derive(Clone, Copy, Debug)]
struct Entry {
    placement: Placement,
    /// Its shelf, if it takes room: a glyph without ink takes none.
    shelf: Option<usize>,
}

/// A row of glyphs of a height.
#[derive(Clone, Debug)]
struct Shelf {
    y: u32,
    height: u32,
    /// The first free column.
    next: u32,
    /// The last frame that used one of its glyphs.
    used: u64,
}

/// One texture of the atlas.
#[derive(Debug)]
struct Page<K> {
    format: BitmapFormat,
    size: u32,
    max: u32,
    shelves: Vec<Shelf>,
    /// The first row below the shelves.
    next_shelf: u32,
    entries: FastMap<K, Entry>,
}

impl<K: Hash + Eq + Clone> Page<K> {
    fn new(format: BitmapFormat, size: u32, max: u32) -> Self {
        Self {
            format,
            size,
            max,
            shelves: Vec::new(),
            next_shelf: 0,
            entries: FastMap::default(),
        }
    }

    fn bytes_a_pixel(&self) -> usize {
        match self.format {
            BitmapFormat::Gray => 1,
            BitmapFormat::Rgba => 4,
        }
    }

    fn bytes(&self) -> usize {
        self.size as usize * self.size as usize * self.bytes_a_pixel()
    }

    fn get(&mut self, key: &K, frame: u64) -> Option<Placement> {
        let entry = self.entries.get(key)?;
        if let Some(shelf) = entry.shelf {
            self.shelves[shelf].used = frame;
        }
        Some(entry.placement)
    }

    fn insert(
        &mut self,
        key: K,
        bitmap: &Bitmap,
        frame: u64,
        changes: &mut Vec<Change>,
    ) -> Option<Placement> {
        if let Some(placement) = self.get(&key, frame) {
            return Some(placement);
        }
        let mut placement = Placement {
            format: self.format,
            x: 0,
            y: 0,
            width: bitmap.width,
            height: bitmap.height,
            left: bitmap.left,
            top: bitmap.top,
        };
        if bitmap.width == 0 || bitmap.height == 0 {
            placement.width = 0;
            placement.height = 0;
            self.entries.insert(
                key,
                Entry {
                    placement,
                    shelf: None,
                },
            );
            return Some(placement);
        }
        let (width, height) = (bitmap.width + GUTTER, bitmap.height + GUTTER);
        if width > self.max || height > self.max {
            return None;
        }
        let shelf = self.room(width, height, frame, changes)?;
        let room = &mut self.shelves[shelf];
        placement.x = room.next;
        placement.y = room.y;
        room.next += width;
        room.used = frame;
        self.entries.insert(
            key,
            Entry {
                placement,
                shelf: Some(shelf),
            },
        );
        changes.push(self.upload(&placement, bitmap));
        Some(placement)
    }

    /// A shelf with room for `width` by `height`: the one that fits it with
    /// the fewest rows to spare, a new one, one in a grown texture, or one
    /// that eviction empties.
    fn room(
        &mut self,
        width: u32,
        height: u32,
        frame: u64,
        changes: &mut Vec<Change>,
    ) -> Option<usize> {
        loop {
            let fitting = self
                .shelves
                .iter()
                .enumerate()
                .filter(|(_, shelf)| {
                    shelf.height >= height && shelf.height - height < SHELF_STEP * 2 + height / 4
                })
                .filter(|(_, shelf)| self.size - shelf.next >= width)
                .min_by_key(|(_, shelf)| shelf.height);
            if let Some((index, _)) = fitting {
                return Some(index);
            }
            let rows = height.div_ceil(SHELF_STEP) * SHELF_STEP;
            if self.size - self.next_shelf >= rows {
                self.shelves.push(Shelf {
                    y: self.next_shelf,
                    height: rows,
                    next: 0,
                    used: frame,
                });
                self.next_shelf += rows;
                return Some(self.shelves.len() - 1);
            }
            if self.size < self.max {
                self.size = (self.size * 2).min(self.max);
                changes.push(Change::Grown {
                    format: self.format,
                    size: self.size,
                });
                continue;
            }
            return self.evict(height, frame);
        }
    }

    /// Empties the shelves least recently used, one after another down the
    /// texture, whose rows together take `height`, that neither the frame in
    /// progress nor the one before uses, for their rows to take new glyphs:
    /// the GPU may still draw the frame before while the renderer writes the
    /// new glyphs' pixels. The shelves join into one, of the height that the
    /// glyph needs, so that a texture of short shelves makes room for a tall
    /// glyph; the rows past it make an empty shelf of their own.
    fn evict(&mut self, height: u32, frame: u64) -> Option<usize> {
        let free = |shelf: &Shelf| shelf.used + 1 < frame;
        // The run of free shelves, from `start` to `end`, that takes the
        // height and was used least lately, and of the fewest rows.
        let mut best: Option<(usize, usize, (u64, u32))> = None;
        for start in 0..self.shelves.len() {
            let (mut rows, mut used) = (0, 0);
            for end in start..self.shelves.len() {
                let shelf = &self.shelves[end];
                if !free(shelf) {
                    break;
                }
                rows += shelf.height;
                used = used.max(shelf.used);
                if rows >= height {
                    let rank = (used, rows);
                    if best.is_none_or(|(_, _, best)| rank < best) {
                        best = Some((start, end + 1, rank));
                    }
                    break;
                }
            }
        }
        let (start, end, (_, rows)) = best?;
        let needed = height.div_ceil(SHELF_STEP) * SHELF_STEP;
        let split = rows >= needed + SHELF_STEP;
        let y = self.shelves[start].y;
        // The run's glyphs go, and the shelves after it take their new
        // places.
        let gone = (end - start) as isize - 1 - isize::from(split);
        self.entries.retain(|_, entry| {
            entry
                .shelf
                .is_none_or(|shelf| shelf < start || shelf >= end)
        });
        for entry in self.entries.values_mut() {
            if let Some(shelf) = &mut entry.shelf
                && *shelf >= end
            {
                *shelf = shelf.saturating_add_signed(-gone);
            }
        }
        let mut joined = vec![Shelf {
            y,
            height: if split { needed } else { rows },
            next: 0,
            used: frame,
        }];
        if split {
            joined.push(Shelf {
                y: y + needed,
                height: rows - needed,
                next: 0,
                used: 0,
            });
        }
        self.shelves.splice(start..end, joined);
        Some(start)
    }

    /// The upload of `bitmap` at `placement`, with its gutter cleared, so
    /// that what an evicted glyph left there goes.
    fn upload(&self, placement: &Placement, bitmap: &Bitmap) -> Change {
        let bytes = self.bytes_a_pixel();
        let (width, height) = (placement.width + GUTTER, placement.height + GUTTER);
        let row = width as usize * bytes;
        let mut data = vec![0_u8; row * height as usize];
        let source_row = bitmap.width as usize * bytes;
        for (target, source) in data.chunks_mut(row).zip(bitmap.data.chunks(source_row)) {
            target[..source_row].copy_from_slice(source);
        }
        Change::Upload {
            format: self.format,
            x: placement.x,
            y: placement.y,
            width,
            height,
            data,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bitmap(format: BitmapFormat, width: u32, height: u32, value: u8) -> Bitmap {
        let bytes = match format {
            BitmapFormat::Gray => 1,
            BitmapFormat::Rgba => 4,
        };
        Bitmap {
            format,
            width,
            height,
            left: -1,
            top: 10,
            data: vec![value; (width * height * bytes) as usize],
        }
    }

    fn gray(width: u32, height: u32) -> Bitmap {
        bitmap(BitmapFormat::Gray, width, height, 200)
    }

    /// Whether two placements share a pixel, their gutters included.
    fn overlap(a: &Placement, b: &Placement) -> bool {
        a.x < b.x + b.width + GUTTER
            && b.x < a.x + a.width + GUTTER
            && a.y < b.y + b.height + GUTTER
            && b.y < a.y + a.height + GUTTER
    }

    #[test]
    fn glyphs_pack_without_overlap() {
        let mut atlas = Atlas::with_sizes(256, 256, 256);
        let mut placed = Vec::new();
        for index in 0..200_u32 {
            let (width, height) = (3 + index * 7 % 13, 8 + index * 5 % 11);
            let placement = atlas.insert(index, &gray(width, height)).expect("room");
            assert!(placement.x + width <= 256 && placement.y + height <= 256);
            assert_eq!((placement.width, placement.height), (width, height));
            assert_eq!((placement.left, placement.top), (-1, 10));
            placed.push(placement);
        }
        for (i, a) in placed.iter().enumerate() {
            for b in &placed[i + 1..] {
                assert!(!overlap(a, b), "{a:?} {b:?}");
            }
        }
        assert_eq!(atlas.len(), 200);
    }

    #[test]
    fn a_glyph_s_pixels_go_up_once_with_their_gutter() {
        let mut atlas = Atlas::with_sizes(64, 64, 64);
        let first = atlas.insert('a', &gray(5, 7)).unwrap();
        assert_eq!(atlas.insert('a', &gray(5, 7)), Some(first));
        assert_eq!(atlas.get(&'a'), Some(first));
        let changes = atlas.take_changes();
        assert_eq!(changes.len(), 1);
        let Change::Upload {
            x,
            y,
            width,
            height,
            data,
            ..
        } = &changes[0]
        else {
            panic!("an upload: {changes:?}");
        };
        assert_eq!((*x, *y, *width, *height), (first.x, first.y, 6, 8));
        // The glyph's pixels, and the gutter's cleared right column and
        // bottom row.
        assert!(
            data.chunks(6)
                .take(7)
                .all(|row| row[..5] == [200; 5] && row[5] == 0)
        );
        assert_eq!(data[42..], [0; 6]);
        assert!(atlas.take_changes().is_empty());
    }

    #[test]
    fn a_glyph_without_ink_takes_no_room() {
        let mut atlas = Atlas::with_sizes(64, 64, 64);
        let space = atlas
            .insert(' ', &Bitmap::empty(BitmapFormat::Gray))
            .unwrap();
        assert_eq!((space.width, space.height), (0, 0));
        assert_eq!(atlas.get(&' '), Some(space));
        assert!(atlas.take_changes().is_empty());
    }

    #[test]
    fn color_goes_to_its_own_texture() {
        let mut atlas = Atlas::with_sizes(64, 64, 64);
        let emoji = atlas
            .insert(1, &bitmap(BitmapFormat::Rgba, 10, 10, 9))
            .unwrap();
        let letter = atlas.insert(2, &gray(10, 10)).unwrap();
        assert_eq!(emoji.format, BitmapFormat::Rgba);
        assert_eq!(letter.format, BitmapFormat::Gray);
        // Each texture starts at its top left corner.
        assert_eq!((emoji.x, emoji.y, letter.x, letter.y), (0, 0, 0, 0));
        let uploads: Vec<usize> = atlas
            .take_changes()
            .iter()
            .map(|change| match change {
                Change::Upload { data, .. } => data.len(),
                Change::Grown { .. } => 0,
            })
            .collect();
        assert_eq!(uploads, [11 * 11 * 4, 11 * 11]);
        assert_eq!(atlas.texture_bytes(), 64 * 64 * 5);
    }

    #[test]
    fn a_full_texture_grows_and_keeps_its_glyphs() {
        let mut atlas = Atlas::with_sizes(32, 128, 128);
        let mut placed = Vec::new();
        for key in 0..40_u32 {
            placed.push((key, atlas.insert(key, &gray(10, 12)).unwrap()));
        }
        assert_eq!(atlas.texture_size(BitmapFormat::Gray), 128);
        let grown: Vec<u32> = atlas
            .take_changes()
            .iter()
            .filter_map(|change| match change {
                Change::Grown { size, .. } => Some(*size),
                Change::Upload { .. } => None,
            })
            .collect();
        assert_eq!(grown, [64, 128]);
        // Every glyph stays where it went.
        for (key, placement) in placed {
            assert_eq!(atlas.get(&key), Some(placement));
        }
    }

    #[test]
    fn past_its_largest_size_it_evicts_the_least_recently_used() {
        let mut atlas = Atlas::with_sizes(64, 64, 64);
        // Five shelves of 12 rows, of five glyphs each, one shelf a frame.
        for shelf in 0..5_u32 {
            atlas.begin_frame();
            for glyph in 0..5 {
                atlas.insert(shelf * 10 + glyph, &gray(11, 11)).unwrap();
            }
        }
        assert_eq!(atlas.len(), 25);
        // The first shelf is used again: the second is now the oldest.
        atlas.begin_frame();
        assert!(atlas.get(&0).is_some());
        atlas.begin_frame();
        let newcomer = atlas.insert(99, &gray(11, 11)).unwrap();
        assert!(atlas.get(&10).is_none() && atlas.get(&14).is_none());
        assert!(atlas.get(&0).is_some() && atlas.get(&20).is_some());
        assert_eq!((newcomer.x, newcomer.y), (0, 12));
        assert_eq!(atlas.texture_size(BitmapFormat::Gray), 64);
    }

    #[test]
    fn an_evicted_shelf_takes_the_height_of_its_glyph() {
        let mut atlas = Atlas::with_sizes(64, 64, 64);
        // Two shelves of 32 rows, of old frames.
        atlas.begin_frame();
        atlas.insert(1, &gray(30, 30)).unwrap();
        atlas.insert(2, &gray(30, 30)).unwrap();
        atlas.insert(3, &gray(30, 30)).unwrap();
        atlas.insert(4, &gray(30, 30)).unwrap();
        atlas.begin_frame();
        atlas.begin_frame();
        // A short glyph takes the first's rows that it needs; the rows past
        // them take another short glyph, of another height.
        let short = atlas.insert(5, &gray(10, 10)).unwrap();
        let other = atlas.insert(6, &gray(10, 14)).unwrap();
        assert_eq!((short.x, short.y), (0, 0));
        assert_eq!((other.x, other.y), (0, 12));
        assert!(atlas.get(&1).is_none() && atlas.get(&2).is_none());
        assert!(atlas.get(&3).is_some() && atlas.get(&4).is_some());
        // The glyphs of the shelf after the evicted one keep their places.
        assert_eq!(atlas.get(&3).map(|placement| placement.y), Some(32));
        let third = atlas.insert(7, &gray(10, 6)).unwrap();
        assert!(third.y < 32);
    }

    #[test]
    fn a_texture_of_short_shelves_makes_room_for_a_tall_glyph() {
        let mut atlas = Atlas::with_sizes(64, 64, 64);
        // Eight shelves of 8 rows, of old frames, the texture full.
        atlas.begin_frame();
        for key in 0..64_u32 {
            atlas.insert(key, &gray(7, 7)).unwrap();
        }
        assert!(atlas.insert(100, &gray(7, 7)).is_none());
        atlas.begin_frame();
        atlas.begin_frame();
        // A glyph of 40 rows takes five of them, joined.
        let tall = atlas.insert(200, &gray(20, 39)).unwrap();
        assert_eq!(tall.y, 0);
        assert!((0..40).all(|key| atlas.get(&key).is_none()));
        // The 24 glyphs below stay, which a lookup would mark as the frame's.
        assert_eq!(atlas.len(), 25);
        // Their shelves take glyphs, as they leave.
        let below = atlas.insert(300, &gray(7, 7)).unwrap();
        assert!(below.y >= 40, "{below:?}");
    }

    #[test]
    fn the_frame_s_glyphs_are_never_evicted() {
        let mut atlas = Atlas::with_sizes(64, 64, 64);
        atlas.begin_frame();
        let mut kept = 0;
        for key in 0..100_u32 {
            if atlas.insert(key, &gray(11, 11)).is_some() {
                kept += 1;
            }
        }
        // Five shelves of five, and no more within one frame.
        assert_eq!(kept, 25);
        for key in 0..25 {
            assert!(atlas.get(&key).is_some());
        }
        // Nor can the next frame, which the GPU may draw while the frame
        // before is still on it; the one after can.
        atlas.begin_frame();
        assert!(atlas.insert(1000, &gray(11, 11)).is_none());
        atlas.begin_frame();
        assert!(atlas.insert(1000, &gray(11, 11)).is_some());
    }

    #[test]
    fn a_glyph_larger_than_the_texture_is_refused() {
        let mut atlas = Atlas::with_sizes(64, 64, 64);
        assert!(atlas.insert(1, &gray(64, 10)).is_none());
        assert!(atlas.insert(2, &gray(63, 63)).is_some());
    }

    #[test]
    fn the_textures_keep_within_their_limits() {
        let mut atlas = Atlas::with_sizes(128, 512, 256);
        for frame in 0..50_u32 {
            atlas.begin_frame();
            for glyph in 0..200 {
                let key = frame * 1000 + glyph;
                let size = 6 + glyph % 30;
                let format = if glyph % 7 == 0 {
                    BitmapFormat::Rgba
                } else {
                    BitmapFormat::Gray
                };
                atlas.insert(key, &bitmap(format, size, size + 4, 1));
            }
            atlas.take_changes();
        }
        assert_eq!(atlas.texture_size(BitmapFormat::Gray), 512);
        assert_eq!(atlas.texture_size(BitmapFormat::Rgba), 256);
        assert_eq!(atlas.texture_bytes(), 512 * 512 + 256 * 256 * 4);
        assert!(atlas.len() < 50 * 200);
    }

    #[test]
    fn the_default_sizes_take_at_most_32_megabytes() {
        let atlas: Atlas<u32> = Atlas::new();
        assert_eq!(atlas.texture_bytes(), 1024 * 1024 * 5);
        assert_eq!(
            GRAY_MAX as usize * GRAY_MAX as usize + COLOR_MAX as usize * COLOR_MAX as usize * 4,
            32 << 20
        );
    }
}
