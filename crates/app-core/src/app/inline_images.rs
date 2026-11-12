//! The inline images that the panes' programs show, drawn: each image's
//! pixels, decoded once by the platform, or a program's own pixels, cut to
//! the rectangle that shows, made smaller past [`MOST_SIDE`] pixels a side,
//! uploaded as a picture of the renderer's, kept while a placement shows
//! it, and let go [`KEPT_FOR`] after the last frame that drew it. A
//! placement draws over its cells, under the text where its z is negative,
//! over it otherwise, within its pane.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use lokyr_platform_core::Platform;
use lokyr_render_core::{Item, PictureId, Rect, Renderer};
use lokyr_vt_core::terminal::{Image, ImageFormat, Placed, Source};

/// How long a picture stays after the last frame that drew it.
const KEPT_FOR: Duration = Duration::from_secs(5);

/// The most pixels of a picture's side: a larger image is made smaller,
/// as no pane shows more.
const MOST_SIDE: u32 = 4096;

/// A picture's image: its id, its bytes' place, which tells an image from
/// another that took its id, and the rectangle of its pixels that shows.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct Key {
    id: u32,
    data: usize,
    source: Option<(u32, u32, u32, u32)>,
}

/// A picture that the renderer holds, or none of an image that does not
/// decode, which is not tried again, and when a frame last drew it.
#[derive(Debug)]
struct Kept {
    picture: Option<PictureId>,
    used: Instant,
}

/// The pictures of the inline images that the frames draw.
#[derive(Debug, Default)]
pub(crate) struct InlineImages {
    kept: HashMap<Key, Kept>,
}

/// Where a pane's grid is and what it holds: its top left corner, in
/// pixels, its cells' size, and its columns and rows.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Grid {
    pub(crate) corner: (i32, i32),
    pub(crate) cell: (u32, u32),
    pub(crate) size: (u16, u16),
}

impl InlineImages {
    /// The items that draw `placed`, a pane's placements as its view shows
    /// them, over `grid`: those under the text, and those over it, each
    /// within the grid; nothing of no placement.
    pub(crate) fn items<R: Renderer<Error: std::fmt::Display>>(
        &mut self,
        renderer: &mut R,
        platform: &dyn Platform,
        placed: &[Placed],
        grid: Grid,
        now: Instant,
    ) -> (Vec<Item>, Vec<Item>) {
        let (mut under, mut over) = (Vec::new(), Vec::new());
        for placement in placed {
            let Some(picture) = self.picture(renderer, platform, placement, now) else {
                continue;
            };
            let rect = rect(placement, grid);
            let item = Item::Picture { rect, picture };
            if placement.z < 0 {
                under.push(item);
            } else {
                over.push(item);
            }
        }
        let clip = Rect {
            x: grid.corner.0,
            y: grid.corner.1,
            width: grid.cell.0 * u32::from(grid.size.0),
            height: grid.cell.1 * u32::from(grid.size.1),
        };
        let clipped = |items: Vec<Item>| {
            if items.is_empty() {
                return items;
            }
            std::iter::once(Item::PushClip(clip))
                .chain(items)
                .chain(std::iter::once(Item::PopClip))
                .collect()
        };
        (clipped(under), clipped(over))
    }

    /// Lets go of the pictures that no frame drew for [`KEPT_FOR`].
    pub(crate) fn let_go<R: Renderer>(&mut self, renderer: &mut R, now: Instant) {
        self.kept.retain(|_, kept| {
            let keep = now.saturating_duration_since(kept.used) < KEPT_FOR;
            if !keep && let Some(picture) = kept.picture {
                renderer.drop_picture(picture);
            }
            keep
        });
    }

    /// How many pictures it holds.
    #[cfg(test)]
    pub(crate) fn held(&self) -> usize {
        self.kept
            .values()
            .filter(|kept| kept.picture.is_some())
            .count()
    }

    /// The picture of `placement`'s image and rectangle, made once.
    fn picture<R: Renderer<Error: std::fmt::Display>>(
        &mut self,
        renderer: &mut R,
        platform: &dyn Platform,
        placement: &Placed,
        now: Instant,
    ) -> Option<PictureId> {
        let key = Key {
            id: placement.image.id,
            data: Arc::as_ptr(&placement.image.data).cast::<u8>() as usize,
            source: placement
                .source
                .map(|source| (source.x, source.y, source.width, source.height)),
        };
        if let Some(kept) = self.kept.get_mut(&key) {
            kept.used = now;
            return kept.picture;
        }
        let picture = pixels(platform, &placement.image, placement.source).and_then(
            |(rgba, width, height)| match renderer.picture_of_pixels(&rgba, width, height) {
                Ok(picture) => picture,
                Err(error) => {
                    eprintln!("lokyr: an inline image does not draw: {error}");
                    None
                }
            },
        );
        self.kept.insert(key, Kept { picture, used: now });
        picture
    }
}

/// The rectangle of `placement` over `grid`: its cells', or, of an image
/// that keeps its proportions, the largest of them within its cells, from
/// their top left.
fn rect(placement: &Placed, grid: Grid) -> Rect {
    let (cell_width, cell_height) = grid.cell;
    let x = grid.corner.0 + i32::from(placement.column) * cell_width as i32;
    let y = grid.corner.1 + placement.row * cell_height as i32;
    let (width, height) = (
        u32::from(placement.columns) * cell_width,
        u32::from(placement.rows) * cell_height,
    );
    if !placement.fit {
        return Rect {
            x,
            y,
            width,
            height,
        };
    }
    let (own_width, own_height) = placement
        .source
        .map_or((placement.image.width, placement.image.height), |source| {
            (source.width, source.height)
        });
    let (own_width, own_height) = (f64::from(own_width.max(1)), f64::from(own_height.max(1)));
    let scale = (f64::from(width) / own_width).min(f64::from(height) / own_height);
    Rect {
        x,
        y,
        width: ((own_width * scale).round() as u32).clamp(1, width.max(1)),
        height: ((own_height * scale).round() as u32).clamp(1, height.max(1)),
    }
}

/// The pixels of `image`, premultiplied RGBA, cut to `source`, made
/// smaller past [`MOST_SIDE`]: its width and its height too; none of an
/// image that does not decode.
fn pixels(
    platform: &dyn Platform,
    image: &Image,
    source: Option<Source>,
) -> Option<(Vec<u8>, u32, u32)> {
    let (rgba, width, height) = match image.format {
        ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Gif => {
            let decoded = platform.decode_image(&image.data)?;
            (decoded.rgba, decoded.width, decoded.height)
        }
        ImageFormat::Rgba => {
            let mut rgba = image.data.to_vec();
            for pixel in rgba.as_chunks_mut::<4>().0 {
                let alpha = u16::from(pixel[3]);
                for channel in &mut pixel[..3] {
                    *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
                }
            }
            (rgba, image.width, image.height)
        }
        ImageFormat::Rgb => {
            let rgba = image
                .data
                .as_chunks::<3>()
                .0
                .iter()
                .flat_map(|&[red, green, blue]| [red, green, blue, 255])
                .collect();
            (rgba, image.width, image.height)
        }
    };
    if rgba.len() != width as usize * height as usize * 4 {
        return None;
    }
    let (rgba, width, height) = match source {
        Some(source) => cut(&rgba, (width, height), source)?,
        None => (rgba, width, height),
    };
    Some(smaller(rgba, width, height))
}

/// The pixels of `source` within `rgba`, of `width` by `height`, its width
/// and its height; none of a rectangle outside them.
fn cut(rgba: &[u8], (width, height): (u32, u32), source: Source) -> Option<(Vec<u8>, u32, u32)> {
    let x = source.x.min(width);
    let y = source.y.min(height);
    let cut_width = source.width.min(width - x);
    let cut_height = source.height.min(height - y);
    if cut_width == 0 || cut_height == 0 {
        return None;
    }
    let mut out = Vec::with_capacity(cut_width as usize * cut_height as usize * 4);
    for row in y..y + cut_height {
        let start = (row as usize * width as usize + x as usize) * 4;
        out.extend_from_slice(&rgba[start..start + cut_width as usize * 4]);
    }
    Some((out, cut_width, cut_height))
}

/// `rgba`, of `width` by `height`, made smaller by a whole factor until
/// neither side passes [`MOST_SIDE`], each pixel the mean of those that it
/// takes the place of, fewer at the edges; as it is otherwise.
fn smaller(rgba: Vec<u8>, width: u32, height: u32) -> (Vec<u8>, u32, u32) {
    let factor = width.max(height).div_ceil(MOST_SIDE);
    if factor <= 1 {
        return (rgba, width, height);
    }
    let (new_width, new_height) = (width.div_ceil(factor), height.div_ceil(factor));
    let mut out = Vec::with_capacity(new_width as usize * new_height as usize * 4);
    for y in 0..new_height {
        let rows = y * factor..((y + 1) * factor).min(height);
        for x in 0..new_width {
            let columns = x * factor..((x + 1) * factor).min(width);
            let mut sum = [0_u32; 4];
            for row in rows.clone() {
                for column in columns.clone() {
                    let at = (row as usize * width as usize + column as usize) * 4;
                    for (total, &value) in sum.iter_mut().zip(&rgba[at..at + 4]) {
                        *total += u32::from(value);
                    }
                }
            }
            let count = rows.len() as u32 * columns.len() as u32;
            out.extend(sum.map(|total| (total / count) as u8));
        }
    }
    (out, new_width, new_height)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An image of `format`, `width` by `height`, of `data`.
    fn image(format: ImageFormat, (width, height): (u32, u32), data: Vec<u8>) -> Arc<Image> {
        Arc::new(Image {
            id: 1,
            format,
            width,
            height,
            data: Arc::from(data),
        })
    }

    #[test]
    fn a_program_s_pixels_are_premultiplied_and_cut_and_large_ones_made_smaller() {
        let platform = lokyr_platform_core::fake::Fake::default();
        // RGBA of half an opacity, premultiplied; RGB made opaque.
        let rgba = image(ImageFormat::Rgba, (1, 1), vec![200, 100, 0, 128]);
        assert_eq!(
            pixels(&platform, &rgba, None),
            Some((vec![100, 50, 0, 128], 1, 1))
        );
        let rgb = image(ImageFormat::Rgb, (2, 1), vec![1, 2, 3, 4, 5, 6]);
        assert_eq!(
            pixels(&platform, &rgb, None),
            Some((vec![1, 2, 3, 255, 4, 5, 6, 255], 2, 1))
        );
        // The right column of two.
        let cut = Source {
            x: 1,
            y: 0,
            width: 1,
            height: 1,
        };
        assert_eq!(
            pixels(&platform, &rgb, Some(cut)),
            Some((vec![4, 5, 6, 255], 1, 1))
        );
        // An image of more pixels than its size, none; a PNG, the
        // platform's decoding.
        let wrong = image(ImageFormat::Rgb, (2, 2), vec![0; 3]);
        assert_eq!(pixels(&platform, &wrong, None), None);
        let png = image(ImageFormat::Png, (2, 2), vec![1]);
        assert_eq!(
            pixels(&platform, &png, None).map(|(_, w, h)| (w, h)),
            Some((2, 2))
        );
        // Wider than the most a side: a third of each side, of the means,
        // the edges' fewer pixels too.
        let (wide, width, height) = smaller(vec![10; 8194 * 2 * 4], 8194, 2);
        assert_eq!((width, height), (2732, 1));
        assert_eq!(wide.len(), 2732 * 4);
        assert!(wide.iter().all(|&value| value == 10));
        let (same, width, height) = smaller(vec![7; 16], 2, 2);
        assert_eq!((same, width, height), (vec![7; 16], 2, 2));
    }

    #[test]
    fn a_placement_s_rectangle_keeps_its_image_s_proportions_from_its_top_left() {
        let grid = Grid {
            corner: (100, 50),
            cell: (10, 20),
            size: (80, 24),
        };
        let placed = Placed {
            image: image(ImageFormat::Png, (200, 100), Vec::new()),
            column: 2,
            row: -1,
            columns: 10,
            rows: 10,
            z: -1,
            fit: true,
            source: None,
        };
        // A box of 100 by 200 pixels: the image's 100 by 50, at the top.
        assert_eq!(
            rect(&placed, grid),
            Rect {
                x: 120,
                y: 30,
                width: 100,
                height: 50
            }
        );
        let stretched = Placed {
            fit: false,
            ..placed
        };
        assert_eq!(rect(&stretched, grid).height, 200);
    }
}
