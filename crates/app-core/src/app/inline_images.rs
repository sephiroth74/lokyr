//! The inline images that the panes' programs show, drawn: each image's
//! pixels, decoded by the platform on a thread of its own, or a program's
//! own pixels, cut to the rectangle that shows and made as small as they
//! draw, uploaded as a picture of the renderer's. A picture lives while its
//! image does, the cache holding a weak reference to it, which no other
//! image's place can take; an image that no snapshot shows lets it go
//! [`KEPT_FOR`] after the last frame that drew it; and the pictures' pixels
//! stay within [`MOST_HELD`], the least recently drawn going first. A
//! placement draws over its cells, under the text where its z is negative,
//! over it otherwise, within its pane; nothing until its picture is made,
//! the decoder's thread waking the app for a frame then.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::time::{Duration, Instant};

use lokyr_platform_core::{ImageDecoder, Platform, Waker};
use lokyr_render_core::{Item, PictureId, Rect, Renderer};
use lokyr_vt_core::terminal::{Image, ImageFormat, Placed, Source};

/// How long a picture stays after the last frame that drew it, once no
/// snapshot shows its image.
const KEPT_FOR: Duration = Duration::from_secs(30);

/// The most pixels of a picture's side: a larger image is made smaller,
/// as no pane shows more.
const MOST_SIDE: u32 = 4096;

/// The most pixels of the pictures held, past which the least recently
/// drawn go: some 170 MB of the graphics' memory, of their mipmaps.
const MOST_HELD: u64 = 32_000_000;

/// A picture's image, by the place of its `Arc`, which the cache's weak
/// reference keeps from another image, and the rectangle of its pixels
/// that shows.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct Key {
    image: usize,
    source: Option<(u32, u32, u32, u32)>,
}

/// A picture that the renderer holds, of its size, its pixels made for a
/// size of the placement's in a frame, as the decoder makes it.
#[derive(Debug)]
struct Kept {
    image: Weak<Image>,
    /// The picture and its size.
    shown: Option<(PictureId, (u32, u32))>,
    /// A size asked of the decoder, under way.
    asked: Option<(u32, u32)>,
    /// Whether the image does not decode, which is not tried again.
    failed: bool,
    /// When a frame last drew it.
    used: Instant,
}

/// What the decoder makes: a picture's key, the size asked, and its
/// pixels, premultiplied RGBA, of their width and height, or none of an
/// image that does not decode.
type Made = (Key, (u32, u32), Option<(Vec<u8>, u32, u32)>);

/// A picture asked of the decoder: its key, its image, the rectangle of its
/// pixels, and the size that it draws at.
struct Job {
    key: Key,
    image: Arc<Image>,
    source: Option<Source>,
    size: (u32, u32),
}

/// The decoder's thread: its jobs, and what it made, which it wakes the
/// main thread for.
struct Worker {
    jobs: Sender<Job>,
    made: Arc<Mutex<Vec<Made>>>,
}

impl std::fmt::Debug for Worker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Worker").finish_non_exhaustive()
    }
}

impl Worker {
    /// A thread that decodes of `decode` and wakes the main thread of
    /// `waker` for each picture made.
    fn start(decode: ImageDecoder, waker: Waker) -> Self {
        let (jobs, received) = mpsc::channel::<Job>();
        let made = Arc::new(Mutex::new(Vec::new()));
        let told = Arc::clone(&made);
        std::thread::Builder::new()
            .name("lokyr-images".to_owned())
            .spawn(move || decoding(&received, &*decode, &told, &waker))
            .ok();
        Self { jobs, made }
    }
}

/// The decoder's loop: each job's pixels, the last image's decoded pixels
/// kept for the next jobs of its other rectangles; a job whose image the
/// app no longer holds is skipped.
fn decoding(
    jobs: &Receiver<Job>,
    decode: &dyn Fn(&[u8]) -> Option<lokyr_platform_core::Pixels>,
    made: &Mutex<Vec<Made>>,
    waker: &Waker,
) {
    let mut last: Option<(Arc<Image>, Option<(Vec<u8>, u32, u32)>)> = None;
    while let Ok(job) = jobs.recv() {
        if Arc::strong_count(&job.image) == 1 {
            continue;
        }
        if !last
            .as_ref()
            .is_some_and(|(image, _)| Arc::ptr_eq(image, &job.image))
        {
            // The last image's pixels go before the next one's decode.
            drop(last.take());
            let whole = whole_pixels(&job.image, |data| decode(data));
            last = Some((Arc::clone(&job.image), whole));
        }
        let pixels = last
            .as_ref()
            .and_then(|(_, whole)| whole.as_ref())
            .and_then(|whole| shown_pixels(whole, job.source, job.size));
        made.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((job.key, job.size, pixels));
        waker.wake();
    }
}

/// The pictures of the inline images that the frames draw.
#[derive(Debug)]
pub(crate) struct InlineImages {
    kept: HashMap<Key, Kept>,
    /// The pixels of the pictures held, and the most that they may be.
    held: u64,
    budget: u64,
    /// The decoder's thread, made as the first image needs it, of a
    /// platform that decodes off the main thread.
    worker: Option<Worker>,
}

impl Default for InlineImages {
    fn default() -> Self {
        Self {
            kept: HashMap::new(),
            held: 0,
            budget: MOST_HELD,
            worker: None,
        }
    }
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
    /// within the grid; nothing of no placement, nor of a picture still
    /// being made.
    pub(crate) fn items<R: Renderer<Error: std::fmt::Display>>(
        &mut self,
        renderer: &mut R,
        platform: &dyn Platform,
        placed: &[Placed],
        grid: Grid,
        now: Instant,
    ) -> (Vec<Item>, Vec<Item>) {
        self.take_made(renderer, now);
        let (mut under, mut over) = (Vec::new(), Vec::new());
        for placement in placed {
            let rect = rect(placement, grid);
            let Some(picture) = self.picture(renderer, platform, placement, rect, now) else {
                continue;
            };
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

    /// Whether the decoder made pictures that the next frame takes.
    pub(crate) fn has_made(&self) -> bool {
        self.worker.as_ref().is_some_and(|worker| {
            !worker
                .made
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .is_empty()
        })
    }

    /// Lets go of the pictures whose images went, and of those that no
    /// frame drew for [`KEPT_FOR`] whose images no snapshot shows, as the
    /// terminal alone holds them.
    pub(crate) fn let_go<R: Renderer>(&mut self, renderer: &mut R, now: Instant) {
        let mut freed = 0;
        self.kept.retain(|_, kept| {
            let holders = kept.image.strong_count();
            let stale = now.saturating_duration_since(kept.used) >= KEPT_FOR;
            let keep = holders > 1 || (holders == 1 && !stale);
            if !keep && let Some((picture, (width, height))) = kept.shown {
                renderer.drop_picture(picture);
                freed += u64::from(width) * u64::from(height);
            }
            keep
        });
        self.held -= freed;
    }

    /// How many pictures it holds.
    #[cfg(test)]
    pub(crate) fn held(&self) -> usize {
        self.kept
            .values()
            .filter(|kept| kept.shown.is_some())
            .count()
    }

    /// The picture of `placement`'s image and rectangle, drawn over `rect`,
    /// if it is made: asked of the decoder, or made at once of a platform
    /// that decodes on the main thread alone, the first time, and again
    /// where it draws larger than it was made.
    fn picture<R: Renderer<Error: std::fmt::Display>>(
        &mut self,
        renderer: &mut R,
        platform: &dyn Platform,
        placement: &Placed,
        rect: Rect,
        now: Instant,
    ) -> Option<PictureId> {
        let key = Key {
            image: Arc::as_ptr(&placement.image) as usize,
            source: placement
                .source
                .map(|source| (source.x, source.y, source.width, source.height)),
        };
        let (own_width, own_height) = placement
            .source
            .map_or((placement.image.width, placement.image.height), |source| {
                (source.width, source.height)
            });
        let size = (
            rect.width.clamp(1, own_width.clamp(1, MOST_SIDE)),
            rect.height.clamp(1, own_height.clamp(1, MOST_SIDE)),
        );
        let kept = self.kept.entry(key).or_insert_with(|| Kept {
            image: Arc::downgrade(&placement.image),
            shown: None,
            asked: None,
            failed: false,
            used: now,
        });
        kept.used = now;
        if kept.failed {
            return None;
        }
        let smaller = kept
            .shown
            .is_none_or(|(_, made)| made.0 < size.0 || made.1 < size.1);
        let asked_enough = kept
            .asked
            .is_some_and(|asked| asked.0 >= size.0 && asked.1 >= size.1);
        let shown = kept.shown.map(|(picture, _)| picture);
        if smaller && !asked_enough {
            kept.asked = Some(size);
            if self.worker.is_none()
                && let Some(decode) = platform.image_decoder()
            {
                self.worker = Some(Worker::start(decode, platform.waker()));
            }
            match &self.worker {
                Some(worker) => {
                    let job = Job {
                        key,
                        image: Arc::clone(&placement.image),
                        source: placement.source,
                        size,
                    };
                    if worker.jobs.send(job).is_err() {
                        self.worker = None;
                    }
                }
                None => {
                    let pixels = whole_pixels(&placement.image, |data| platform.decode_image(data))
                        .and_then(|whole| shown_pixels(&whole, placement.source, size));
                    self.take(renderer, (key, size, pixels), now);
                    return self.kept.get(&key)?.shown.map(|(picture, _)| picture);
                }
            }
        }
        shown
    }

    /// Uploads the pictures that the decoder made.
    fn take_made<R: Renderer<Error: std::fmt::Display>>(&mut self, renderer: &mut R, now: Instant) {
        let Some(worker) = &self.worker else {
            return;
        };
        let made = std::mem::take(&mut *worker.made.lock().unwrap_or_else(PoisonError::into_inner));
        for made in made {
            self.take(renderer, made, now);
        }
    }

    /// Takes a picture made: uploaded in place of the one shown, of the
    /// placement that still asks for it; the least recently drawn let go
    /// past [`MOST_HELD`].
    fn take<R: Renderer<Error: std::fmt::Display>>(
        &mut self,
        renderer: &mut R,
        (key, size, pixels): Made,
        now: Instant,
    ) {
        let Some(kept) = self.kept.get_mut(&key) else {
            return;
        };
        if kept.asked == Some(size) {
            kept.asked = None;
        }
        let Some((rgba, width, height)) = pixels else {
            if kept.shown.is_none() {
                kept.failed = true;
            }
            return;
        };
        let picture = match renderer.picture_of_pixels(&rgba, width, height) {
            Ok(Some(picture)) => picture,
            Ok(None) => return,
            Err(error) => {
                eprintln!("lokyr: an inline image does not draw: {error}");
                return;
            }
        };
        if let Some((old, (old_width, old_height))) = kept.shown.replace((picture, (width, height)))
        {
            renderer.drop_picture(old);
            self.held -= u64::from(old_width) * u64::from(old_height);
        }
        self.held += u64::from(width) * u64::from(height);
        kept.used = now;
        self.within_budget(renderer, key);
    }

    /// Lets go of the least recently drawn pictures but `keep`'s, as long
    /// as their pixels pass the budget.
    fn within_budget<R: Renderer>(&mut self, renderer: &mut R, keep: Key) {
        while self.held > self.budget {
            let oldest = self
                .kept
                .iter()
                .filter(|(key, kept)| **key != keep && kept.shown.is_some())
                .min_by_key(|(_, kept)| kept.used)
                .map(|(&key, _)| key);
            let Some(key) = oldest else {
                return;
            };
            if let Some(Kept {
                shown: Some((picture, (width, height))),
                ..
            }) = self.kept.remove(&key)
            {
                renderer.drop_picture(picture);
                self.held -= u64::from(width) * u64::from(height);
            }
        }
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

/// The pixels of `image`, premultiplied RGBA, its width and its height:
/// decoded by `decode`, or of the program's own; none of an image that does
/// not decode, or of pixels that its size does not take.
fn whole_pixels(
    image: &Image,
    decode: impl FnOnce(&[u8]) -> Option<lokyr_platform_core::Pixels>,
) -> Option<(Vec<u8>, u32, u32)> {
    let (rgba, width, height) = match image.format {
        ImageFormat::Png
        | ImageFormat::Jpeg
        | ImageFormat::Gif
        | ImageFormat::Tiff
        | ImageFormat::Bmp => {
            let decoded = decode(&image.data)?;
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
    (rgba.len() == width as usize * height as usize * 4).then_some((rgba, width, height))
}

/// The pixels of `whole`, of its width and height, that a placement shows:
/// cut to `source`, made no larger than `size` on either side; their width
/// and height too; none of a rectangle outside them.
fn shown_pixels(
    (rgba, width, height): &(Vec<u8>, u32, u32),
    source: Option<Source>,
    size: (u32, u32),
) -> Option<(Vec<u8>, u32, u32)> {
    match source {
        Some(source) => {
            let (cut, width, height) = cut(rgba, (*width, *height), source)?;
            Some(scaled(&cut, (width, height), size))
        }
        None => Some(scaled(rgba, (*width, *height), size)),
    }
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

/// `rgba`, of `width` by `height`, made no larger than `most` on either
/// side, each pixel the mean of those whose place it takes, of a side at a
/// time; as it is where it fits.
fn scaled(rgba: &[u8], (width, height): (u32, u32), most: (u32, u32)) -> (Vec<u8>, u32, u32) {
    let (new_width, new_height) = (width.min(most.0.max(1)), height.min(most.1.max(1)));
    if (new_width, new_height) == (width, height) {
        return (rgba.to_vec(), width, height);
    }
    // The pixels of the old side that each of the new takes: from
    // `start` to `end`, one at least.
    let spans = |old: u32, new: u32| -> Vec<(usize, usize)> {
        (0..u64::from(new))
            .map(|at| {
                let start = (at * u64::from(old) / u64::from(new)) as usize;
                let end = ((at + 1) * u64::from(old)).div_ceil(u64::from(new)) as usize;
                (start, end.max(start + 1).min(old as usize))
            })
            .collect()
    };
    let (columns, rows) = (spans(width, new_width), spans(height, new_height));
    let mut out = Vec::with_capacity(new_width as usize * new_height as usize * 4);
    for &(top, bottom) in &rows {
        for &(left, right) in &columns {
            let mut sum = [0_u32; 4];
            for row in top..bottom {
                let line =
                    &rgba[(row * width as usize + left) * 4..(row * width as usize + right) * 4];
                for pixel in line.as_chunks::<4>().0 {
                    for (total, &value) in sum.iter_mut().zip(pixel) {
                        *total += u32::from(value);
                    }
                }
            }
            let count = ((bottom - top) * (right - left)) as u32;
            out.extend(sum.map(|total| ((total + count / 2) / count) as u8));
        }
    }
    (out, new_width, new_height)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::fakes::Renderer as FakeRenderer;

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

    /// A placement of `image` over 10 by 10 cells at the top left, of
    /// `source`.
    fn placed(image: &Arc<Image>, source: Option<Source>) -> Placed {
        Placed {
            image: Arc::clone(image),
            column: 0,
            row: 0,
            columns: 10,
            rows: 10,
            z: -1,
            fit: false,
            source,
        }
    }

    /// A grid of 80 by 24 cells of `cell` pixels.
    fn grid(cell: (u32, u32)) -> Grid {
        Grid {
            corner: (0, 0),
            cell,
            size: (80, 24),
        }
    }

    /// The pictures that a frame of `placed` draws.
    fn drawn(
        images: &mut InlineImages,
        renderer: &mut FakeRenderer,
        placed: &[Placed],
        now: Instant,
    ) -> Vec<PictureId> {
        let platform = lokyr_platform_core::fake::Fake::default();
        let (under, over) = images.items(renderer, &platform, placed, grid((10, 20)), now);
        under
            .into_iter()
            .chain(over)
            .filter_map(|item| match item {
                Item::Picture { picture, .. } => Some(picture),
                _ => None,
            })
            .collect()
    }

    /// Waits up to 5 seconds for the decoder's pictures.
    fn wait_for_made(images: &InlineImages) {
        let started = Instant::now();
        while !images.has_made() {
            assert!(started.elapsed() < Duration::from_secs(5), "nothing made");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_program_s_pixels_are_premultiplied_and_cut_and_large_ones_made_smaller() {
        let none = |_: &[u8]| None;
        // RGBA of half an opacity, premultiplied; RGB made opaque.
        let rgba = image(ImageFormat::Rgba, (1, 1), vec![200, 100, 0, 128]);
        assert_eq!(
            whole_pixels(&rgba, none),
            Some((vec![100, 50, 0, 128], 1, 1))
        );
        let rgb = image(ImageFormat::Rgb, (2, 1), vec![1, 2, 3, 4, 5, 6]);
        let whole = whole_pixels(&rgb, none).unwrap();
        assert_eq!(whole, (vec![1, 2, 3, 255, 4, 5, 6, 255], 2, 1));
        // The right column of two.
        let cut = Source {
            x: 1,
            y: 0,
            width: 1,
            height: 1,
        };
        assert_eq!(
            shown_pixels(&whole, Some(cut), (10, 10)),
            Some((vec![4, 5, 6, 255], 1, 1))
        );
        // An image of more pixels than its size, none; a PNG, the
        // platform's decoding.
        let wrong = image(ImageFormat::Rgb, (2, 2), vec![0; 3]);
        assert_eq!(whole_pixels(&wrong, none), None);
        let platform = lokyr_platform_core::fake::Fake::default();
        let png = image(ImageFormat::Png, (2, 2), vec![1]);
        assert_eq!(
            whole_pixels(&png, |data| platform.decode_image(data)).map(|(_, w, h)| (w, h)),
            Some((2, 2))
        );
        // Made as small as it draws, of the means of the pixels that each
        // takes the place of, the edges' fewer too.
        let (small, width, height) =
            scaled(&[0, 0, 0, 0, 200, 100, 50, 255, 0, 0, 0, 0], (3, 1), (2, 1));
        assert_eq!((width, height), (2, 1));
        assert_eq!(small, vec![100, 50, 25, 128, 100, 50, 25, 128]);
        let (same, width, height) = scaled(&[7; 16], (2, 2), (4, 4));
        assert_eq!((same, width, height), (vec![7; 16], 2, 2));
    }

    #[test]
    fn a_picture_lives_as_its_image_does_and_another_image_takes_its_own() {
        let mut images = InlineImages::default();
        let mut renderer = FakeRenderer::default();
        let now = Instant::now();
        let first = image(ImageFormat::Rgb, (1, 1), vec![1, 2, 3]);
        assert_eq!(
            drawn(&mut images, &mut renderer, &[placed(&first, None)], now),
            [PictureId(1)]
        );
        // The image replaced by another of its size: the other's picture,
        // the first's let go as it went.
        drop(first);
        let second = image(ImageFormat::Rgb, (1, 1), vec![4, 5, 6]);
        assert_eq!(
            drawn(&mut images, &mut renderer, &[placed(&second, None)], now),
            [PictureId(2)]
        );
        images.let_go(&mut renderer, now);
        assert_eq!(renderer.dropped, [PictureId(1)]);
        assert_eq!(images.held(), 1);
        // An image that the terminal alone holds stays a while after its
        // last frame; one that a snapshot shows stays as long as it does.
        let shown = placed(&second, None);
        images.let_go(&mut renderer, now + KEPT_FOR * 2);
        assert_eq!(images.held(), 1);
        drop(shown);
        images.let_go(&mut renderer, now + KEPT_FOR / 2);
        assert_eq!(images.held(), 1);
        images.let_go(&mut renderer, now + KEPT_FOR);
        assert_eq!(images.held(), 0);
        assert_eq!(renderer.dropped, [PictureId(1), PictureId(2)]);
    }

    #[test]
    fn a_picture_is_made_as_small_as_it_draws_and_again_as_it_draws_larger() {
        let platform = lokyr_platform_core::fake::Fake::default();
        let mut images = InlineImages::default();
        let mut renderer = FakeRenderer::default();
        let now = Instant::now();
        let large = image(ImageFormat::Rgb, (400, 400), vec![9; 400 * 400 * 3]);
        let placements = [placed(&large, None)];
        // 10 by 10 cells of 10 by 20 pixels: 100 by 200.
        images.items(&mut renderer, &platform, &placements, grid((10, 20)), now);
        assert_eq!(images.held, 100 * 200);
        // Of larger cells, made again, the first let go.
        images.items(&mut renderer, &platform, &placements, grid((20, 40)), now);
        assert_eq!(images.held, 200 * 400);
        assert_eq!(renderer.dropped, [PictureId(1)]);
        // Of smaller ones, the larger kept.
        images.items(&mut renderer, &platform, &placements, grid((10, 20)), now);
        assert_eq!(renderer.pictures, 2);
    }

    #[test]
    fn the_pictures_stay_within_their_budget_the_least_recently_drawn_going() {
        let mut images = InlineImages {
            budget: 250,
            ..InlineImages::default()
        };
        let mut renderer = FakeRenderer::default();
        let now = Instant::now();
        let all: Vec<Arc<Image>> = (0..3)
            .map(|_| image(ImageFormat::Rgb, (10, 10), vec![0; 300]))
            .collect();
        for (at, image) in all.iter().enumerate() {
            let later = now + Duration::from_secs(at as u64);
            drawn(&mut images, &mut renderer, &[placed(image, None)], later);
        }
        assert_eq!(images.held(), 2);
        assert_eq!(images.held, 200);
        assert_eq!(renderer.dropped, [PictureId(1)]);
    }

    #[test]
    fn the_thread_decodes_an_image_once_for_its_rectangles_then_wakes_the_app() {
        let decodes = Arc::new(AtomicUsize::new(0));
        let wakes = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&decodes);
        let decode: ImageDecoder = Arc::new(move |_: &[u8]| {
            counted.fetch_add(1, Ordering::Relaxed);
            Some(lokyr_platform_core::Pixels {
                width: 100,
                height: 100,
                rgba: vec![255; 100 * 100 * 4],
            })
        });
        let woken = Arc::clone(&wakes);
        let mut images = InlineImages {
            worker: Some(Worker::start(
                decode,
                Waker::new(move || {
                    woken.fetch_add(1, Ordering::Relaxed);
                }),
            )),
            ..InlineImages::default()
        };
        let mut renderer = FakeRenderer::default();
        let now = Instant::now();
        let png = image(ImageFormat::Png, (100, 100), vec![1]);
        // Fifty slices of the image, as a program tiles it.
        let slices: Vec<Placed> = (0..50)
            .map(|at| {
                placed(
                    &png,
                    Some(Source {
                        x: at * 2,
                        y: 0,
                        width: 2,
                        height: 100,
                    }),
                )
            })
            .collect();
        // Nothing until the thread made them.
        assert!(drawn(&mut images, &mut renderer, &slices, now).is_empty());
        let started = Instant::now();
        while wakes.load(Ordering::Relaxed) < 50 {
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "not every slice made"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        wait_for_made(&images);
        assert_eq!(drawn(&mut images, &mut renderer, &slices, now).len(), 50);
        assert_eq!(decodes.load(Ordering::Relaxed), 1);
        assert!(!images.has_made());
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
