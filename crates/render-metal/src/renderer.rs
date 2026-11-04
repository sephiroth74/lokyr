//! The renderer of Metal: the atlas's textures, the pipelines of the shaders,
//! and a frame drawn from a display list.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::time::{Duration, Instant};

use dispatch2::DispatchData;
use lokyr_render_core::{
    AtlasRegion, CellBatch, Color, CursorShape, DisplayList, GpuTimes, Item, LineStyle, Rect,
    Renderer, Shape, Texture, TextureUpdate,
};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSString;
use objc2_metal::{
    MTLBlendFactor, MTLBlitCommandEncoder, MTLBuffer, MTLClearColor, MTLCommandBuffer,
    MTLCommandBufferStatus, MTLCommandEncoder, MTLCommandQueue, MTLDevice, MTLDrawable, MTLLibrary,
    MTLLoadAction, MTLOrigin, MTLPixelFormat, MTLPrimitiveType, MTLRegion, MTLRenderCommandEncoder,
    MTLRenderPassDescriptor, MTLRenderPipelineDescriptor, MTLRenderPipelineState,
    MTLResourceOptions, MTLScissorRect, MTLSize, MTLStorageMode, MTLStoreAction, MTLTexture,
    MTLTextureDescriptor, MTLTextureUsage,
};
use objc2_quartz_core::{CACurrentMediaTime, CAMetalDrawable};

use crate::{Error, Gpu, Surface};

/// The shaders, compiled when the crate builds.
static LIBRARY: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/cells.metallib"));

/// The vertices of a quad's strip.
const QUAD: usize = 4;

/// A rectangle, as the shaders take it.
#[repr(C)]
#[derive(Clone, Copy)]
struct RectInstance {
    position: [f32; 2],
    size: [f32; 2],
    color: [f32; 4],
}

/// A glyph, as the shaders take it.
#[repr(C)]
#[derive(Clone, Copy)]
struct GlyphInstance {
    position: [f32; 2],
    size: [f32; 2],
    atlas: [u32; 2],
    /// The atlas: 0 for coverage's, 1 for colors'.
    texture: u32,
    /// Whether its coverage takes the weight of the system's text: 1, or 0.
    weight: u32,
    color: [f32; 4],
}

/// A line across cells, as the shaders take it: its band, its color, its
/// style, the thickness of its strokes, and its pattern's period and
/// origin.
#[repr(C)]
#[derive(Clone, Copy)]
struct LineInstance {
    position: [f32; 2],
    size: [f32; 2],
    color: [f32; 4],
    style: u32,
    thickness: f32,
    period: f32,
    origin: f32,
}

/// A shape, or its shadow, as the shaders take it: the quad that it covers,
/// its outline's centre and half its size, its corners' radius, its
/// border's width, its blur's deviation, negative for the shape itself, what
/// fills it, and its border's color.
#[repr(C)]
#[derive(Clone, Copy)]
struct ShapeInstance {
    position: [f32; 2],
    size: [f32; 2],
    center: [f32; 2],
    extent: [f32; 2],
    radius: f32,
    border: f32,
    sigma: f32,
    _padding: f32,
    fill: [f32; 4],
    border_color: [f32; 4],
    /// An arc's start and span, in turns, its thickness, and 1, or 0 for a
    /// shape of no arc.
    arc: [f32; 4],
}

/// A grid, as the shaders take it.
#[repr(C)]
#[derive(Clone, Copy)]
struct GridUniform {
    origin: [f32; 2],
    cell: [f32; 2],
    columns: u32,
    _padding: u32,
}

type Pipeline = Retained<ProtocolObject<dyn MTLRenderPipelineState>>;
type MetalTexture = Retained<ProtocolObject<dyn MTLTexture>>;

/// The renderer of Metal, which every window's surface shares.
#[derive(Debug)]
pub struct MetalRenderer {
    device: Retained<ProtocolObject<dyn MTLDevice>>,
    queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    solid: Pipeline,
    background: Pipeline,
    lines: Pipeline,
    glyphs: Pipeline,
    shapes: Pipeline,
    coverage: MetalTexture,
    color: MetalTexture,
    /// The commands of the frames that the GPU may still draw, by their
    /// lists' frames, from the oldest.
    drawing: VecDeque<Drawing>,
    /// Buffers that the frames that the GPU finished let go of, for the
    /// next frames' instances.
    spare: Vec<Buffer>,
    /// The bytes of the last frame's instances, which the next frame's
    /// first buffer takes at least.
    last_bytes: usize,
    /// How many buffers of instances the renderer made, which the tests
    /// count.
    #[cfg(test)]
    made: std::cell::Cell<usize>,
    /// The GPU's times of the frames that it finished, until they are taken.
    gpu_times: GpuTimes,
    /// How long the last draw waited for its drawable.
    drawable_wait: Duration,
    /// The commands of the last uploads, which the tests wait for.
    #[cfg(test)]
    uploaded: Option<Retained<ProtocolObject<dyn MTLCommandBuffer>>>,
    /// What the next drawable that shows calls, once it shows.
    on_presented: Option<OnPresented>,
}

/// What the next drawable calls as it shows.
struct OnPresented(Presented);

impl std::fmt::Debug for OnPresented {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OnPresented")
    }
}

/// What a drawable calls as it shows on the display, from the display's
/// thread, with how long ago it showed; or `None`, for a frame that the
/// display skipped.
pub type Presented = Box<dyn FnOnce(Option<Duration>) + Send>;

impl MetalRenderer {
    /// A renderer of `gpu`, whose atlas's textures start at `coverage` and
    /// `color` pixels a side.
    pub fn new(gpu: &Gpu, coverage: u32, color: u32) -> Result<Self, Error> {
        let device = gpu.device.clone();
        let data = DispatchData::from_static_bytes(LIBRARY);
        let library = device
            .newLibraryWithData_error(&data)
            .map_err(|error| Error::Shaders(error.to_string()))?;
        let pipeline =
            |vertex: &str, fragment: &str| make_pipeline(&device, &library, vertex, fragment);
        Ok(Self {
            solid: pipeline("rect_vertex", "solid_fragment")?,
            background: pipeline("background_vertex", "solid_fragment")?,
            lines: pipeline("line_vertex", "line_fragment")?,
            glyphs: pipeline("glyph_vertex", "glyph_fragment")?,
            shapes: pipeline("shape_vertex", "shape_fragment")?,
            coverage: make_texture(&device, Texture::Coverage, coverage)?,
            color: make_texture(&device, Texture::Color, color)?,
            queue: gpu.queue.clone(),
            device,
            drawing: VecDeque::new(),
            spare: Vec::new(),
            last_bytes: 0,
            #[cfg(test)]
            made: std::cell::Cell::new(0),
            gpu_times: GpuTimes::default(),
            drawable_wait: Duration::ZERO,
            #[cfg(test)]
            uploaded: None,
            on_presented: None,
        })
    }

    fn texture(&self, texture: Texture) -> &MetalTexture {
        match texture {
            Texture::Coverage => &self.coverage,
            Texture::Color => &self.color,
        }
    }

    /// Grows `texture` to `size`, copying its pixels into the new one's
    /// corner, on the queue, after the uploads before it and before those
    /// after it.
    fn grow(&mut self, texture: Texture, size: u32) -> Result<(), Error> {
        let old = self.texture(texture).clone();
        let new = make_texture(&self.device, texture, size)?;
        let buffer = self
            .queue
            .commandBuffer()
            .ok_or(Error::Device("command buffer"))?;
        let blit = buffer
            .blitCommandEncoder()
            .ok_or(Error::Device("blit command encoder"))?;
        let side = old.width();
        // SAFETY: the old texture's whole first level, into the new one's,
        // which is larger. The command buffer keeps both alive.
        unsafe {
            blit.copyFromTexture_sourceSlice_sourceLevel_sourceOrigin_sourceSize_toTexture_destinationSlice_destinationLevel_destinationOrigin(
                &old,
                0,
                0,
                MTLOrigin { x: 0, y: 0, z: 0 },
                MTLSize {
                    width: side,
                    height: side,
                    depth: 1,
                },
                &new,
                0,
                0,
                MTLOrigin { x: 0, y: 0, z: 0 },
            );
        }
        blit.endEncoding();
        buffer.commit();
        match texture {
            Texture::Coverage => self.coverage = new,
            Texture::Color => self.color = new,
        }
        Ok(())
    }

    /// Writes each of `uploads`' bytes into its region of its texture,
    /// through the GPU: the bytes into one buffer, and a copy of each into
    /// its texture, on the queue that draws the frames. Each lands after the
    /// frames before it, which may still read what it covers, and before
    /// those after it; and on every GPU, where a write of the CPU into a
    /// texture that the GPU wrote may not reach it, as a virtual machine's
    /// GPU loses it. An upload outside its texture, or whose bytes do not
    /// fill its region, is dropped.
    fn upload(&mut self, uploads: &[(&AtlasRegion, &[u8])]) -> Result<(), Error> {
        let mut bytes = Vec::new();
        let mut copies = Vec::new();
        for &(region, data) in uploads {
            let texture = self.texture(region.texture);
            let pixel = match region.texture {
                Texture::Coverage => 1,
                Texture::Color => 4,
            };
            let (x, y) = (region.x as usize, region.y as usize);
            let (width, height) = (region.width as usize, region.height as usize);
            let length = width * height * pixel;
            if length == 0
                || data.len() < length
                || x + width > texture.width()
                || y + height > texture.height()
            {
                continue;
            }
            // A copy's bytes start at a multiple of its pixel's size.
            bytes.resize(bytes.len().next_multiple_of(4), 0);
            copies.push((region.texture, bytes.len(), pixel, x, y, width, height));
            bytes.extend_from_slice(&data[..length]);
        }
        if copies.is_empty() {
            return Ok(());
        }
        let source = self.buffer(&bytes).ok_or(Error::Device("buffer"))?;
        let buffer = self
            .queue
            .commandBuffer()
            .ok_or(Error::Device("command buffer"))?;
        let blit = buffer
            .blitCommandEncoder()
            .ok_or(Error::Device("blit command encoder"))?;
        for (texture, offset, pixel, x, y, width, height) in copies {
            // SAFETY: the copy's rows, `width * pixel` bytes each, lie within
            // the buffer from `offset`, and its region within the texture,
            // as the checks above keep them. The command buffer keeps both
            // alive.
            unsafe {
                blit.copyFromBuffer_sourceOffset_sourceBytesPerRow_sourceBytesPerImage_sourceSize_toTexture_destinationSlice_destinationLevel_destinationOrigin(
                    &source,
                    offset,
                    width * pixel,
                    width * height * pixel,
                    MTLSize {
                        width,
                        height,
                        depth: 1,
                    },
                    self.texture(texture),
                    0,
                    0,
                    MTLOrigin { x, y, z: 0 },
                );
            }
        }
        blit.endEncoding();
        buffer.commit();
        #[cfg(test)]
        {
            self.uploaded = Some(buffer);
        }
        Ok(())
    }

    /// A buffer of `items`, for the shaders; `None` for none.
    fn buffer<T: Copy>(&self, items: &[T]) -> Option<Retained<ProtocolObject<dyn MTLBuffer>>> {
        if items.is_empty() {
            return None;
        }
        let length = size_of_val(items);
        // SAFETY: the bytes of `items`, which the device copies.
        unsafe {
            self.device.newBufferWithBytes_length_options(
                NonNull::new_unchecked(items.as_ptr().cast_mut().cast::<c_void>()),
                length,
                MTLResourceOptions::StorageModeShared,
            )
        }
    }
}

impl Renderer for MetalRenderer {
    type Target = Surface;
    type Error = Error;

    fn update_textures(&mut self, updates: &[TextureUpdate]) -> Result<(), Error> {
        let mut uploads = Vec::new();
        for update in updates {
            match update {
                // The uploads before a growth go to the texture that it
                // copies.
                TextureUpdate::Grown { texture, size } => {
                    self.upload(&uploads)?;
                    uploads.clear();
                    self.grow(*texture, *size)?;
                }
                TextureUpdate::Upload { region, data } => uploads.push((region, data.as_slice())),
            }
        }
        self.upload(&uploads)
    }

    fn draw(&mut self, target: &Surface, list: &DisplayList) -> Result<bool, Error> {
        self.present(target, list, true)
    }

    fn drawing_from(&mut self) -> Option<u64> {
        self.drop_finished();
        self.drawing.front().map(|drawing| drawing.frame)
    }

    fn take_gpu_times(&mut self) -> GpuTimes {
        self.drop_finished();
        std::mem::take(&mut self.gpu_times)
    }

    fn drawable_wait(&self) -> Duration {
        self.drawable_wait
    }

    fn draw_untimed(&mut self, target: &Surface, list: &DisplayList) -> Result<bool, Error> {
        self.present(target, list, false)
    }

    /// Calls `presented` from the display's thread, as the next drawable
    /// that the renderer draws, of any surface, shows.
    fn on_next_presented(&mut self, presented: impl FnOnce(Option<Duration>) + Send + 'static) {
        self.on_presented = Some(OnPresented(Box::new(presented)));
    }

    fn forget_next_presented(&mut self) {
        self.on_presented = None;
    }
}

impl MetalRenderer {
    /// Draws `list` into the next drawable of `target`, and presents it,
    /// its GPU's time counted when `timed`. Whether it drew: no drawable
    /// comes from a layer that was never sized, nor when every drawable is
    /// in use for a second.
    fn present(
        &mut self,
        target: &Surface,
        list: &DisplayList,
        timed: bool,
    ) -> Result<bool, Error> {
        let asked = Instant::now();
        let drawable = target.layer.nextDrawable();
        self.drawable_wait = asked.elapsed();
        let Some(drawable) = drawable else {
            return Ok(false);
        };
        let (commands, buffers) = self.encode(&drawable.texture(), list)?;
        if let Some(OnPresented(presented)) = self.on_presented.take() {
            tell_presented(&drawable, presented);
        }
        // A layer that presents with the transaction shows the drawable as
        // Core Animation commits it, once the GPU has the commands.
        if target.layer.presentsWithTransaction() {
            commands.commit();
            commands.waitUntilScheduled();
            drawable.present();
        } else {
            commands.presentDrawable(ProtocolObject::from_ref(&*drawable));
            commands.commit();
        }
        self.drawing.push_back(Drawing {
            frame: list.frame,
            commands,
            buffers,
            timed,
        });
        self.drop_finished();
        Ok(true)
    }

    /// Draws `list` into a texture of its own, `width` by `height` pixels,
    /// as a window's surface would show it, waits for the GPU, and reads
    /// its pixels back: RGBA, 8 bits a component in sRGB, a row after the
    /// other from the top, as tests compare them with images. Its time on
    /// the GPU counts among [`Renderer::take_gpu_times`]'s.
    pub fn draw_offscreen(
        &mut self,
        list: &DisplayList,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>, Error> {
        let (width, height) = (width.max(1) as usize, height.max(1) as usize);
        // SAFETY: a two-dimensional texture of a valid format and size.
        let descriptor = unsafe {
            MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
                MTLPixelFormat::BGRA8Unorm,
                width,
                height,
                false,
            )
        };
        descriptor.setUsage(MTLTextureUsage::RenderTarget);
        descriptor.setStorageMode(MTLStorageMode::Shared);
        let texture = self
            .device
            .newTextureWithDescriptor(&descriptor)
            .ok_or(Error::Device("texture"))?;
        let (commands, buffers) = self.encode(&texture, list)?;
        commands.commit();
        commands.waitUntilCompleted();
        let time = commands.GPUEndTime() - commands.GPUStartTime();
        if commands.status() == MTLCommandBufferStatus::Completed && time > 0.0 {
            self.gpu_times.add(Duration::from_secs_f64(time));
        }
        self.take_back(buffers);
        let mut pixels = vec![0_u8; width * height * 4];
        let region = MTLRegion {
            origin: MTLOrigin { x: 0, y: 0, z: 0 },
            size: MTLSize {
                width,
                height,
                depth: 1,
            },
        };
        // SAFETY: room for the whole texture, of four bytes a pixel.
        unsafe {
            texture.getBytes_bytesPerRow_fromRegion_mipmapLevel(
                NonNull::new_unchecked(pixels.as_mut_ptr().cast()),
                width * 4,
                region,
                0,
            );
        }
        // BGRA to RGBA.
        for pixel in pixels.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        Ok(pixels)
    }

    /// The commands that draw `list` into `output`, to commit, and the
    /// buffers of its instances, which the GPU reads until it finishes.
    fn encode(
        &mut self,
        output: &ProtocolObject<dyn MTLTexture>,
        list: &DisplayList,
    ) -> Result<(Commands, Vec<Buffer>), Error> {
        let arena = RefCell::new(Arena {
            device: self.device.clone(),
            spare: std::mem::take(&mut self.spare),
            used: Vec::new(),
            offset: 0,
            first: self.last_bytes,
            bytes: 0,
            #[cfg(test)]
            made: 0,
        });
        let viewport = [output.width() as f32, output.height() as f32];
        let pass = MTLRenderPassDescriptor::new();
        // SAFETY: a render pass has a first color attachment.
        let attachment = unsafe { pass.colorAttachments().objectAtIndexedSubscript(0) };
        attachment.setTexture(Some(output));
        attachment.setLoadAction(MTLLoadAction::Clear);
        attachment.setStoreAction(MTLStoreAction::Store);
        attachment.setClearColor(clear_color(list.background));
        let buffer = self
            .queue
            .commandBuffer()
            .ok_or(Error::Device("command buffer"))?;
        let encoder = buffer
            .renderCommandEncoderWithDescriptor(&pass)
            .ok_or(Error::Device("render command encoder"))?;
        let mut frame = Frame {
            renderer: self,
            encoder: &encoder,
            viewport,
            clips: Vec::new(),
            arena: &arena,
        };
        for item in &list.items {
            frame.item(item);
        }
        encoder.endEncoding();
        let arena = arena.into_inner();
        self.spare = arena.spare;
        self.last_bytes = arena.bytes;
        #[cfg(test)]
        self.made.set(self.made.get() + arena.made);
        Ok((buffer, arena.used))
    }

    /// Takes back `buffers`, of a frame that the GPU finished, for the
    /// next frames: [`SPARE`] at most, the largest.
    fn take_back(&mut self, buffers: Vec<Buffer>) {
        self.spare.extend(buffers);
        if self.spare.len() > SPARE {
            self.spare
                .sort_by_key(|buffer| std::cmp::Reverse(buffer.length()));
            self.spare.truncate(SPARE);
        }
    }

    /// Lets the commands of the frames that the GPU finished go, from the
    /// oldest, as a queue's commands finish in their order, with their
    /// times, and takes their buffers back.
    fn drop_finished(&mut self) {
        while self.drawing.front().is_some_and(|drawing| {
            matches!(
                drawing.commands.status(),
                MTLCommandBufferStatus::Completed | MTLCommandBufferStatus::Error
            )
        }) {
            if let Some(drawing) = self.drawing.pop_front() {
                let commands = &drawing.commands;
                let time = commands.GPUEndTime() - commands.GPUStartTime();
                if drawing.timed
                    && commands.status() == MTLCommandBufferStatus::Completed
                    && time > 0.0
                {
                    self.gpu_times.add(Duration::from_secs_f64(time));
                }
                self.take_back(drawing.buffers);
            }
        }
    }
}

/// A buffer of the GPU's.
type Buffer = Retained<ProtocolObject<dyn MTLBuffer>>;

/// A frame's commands.
type Commands = Retained<ProtocolObject<dyn MTLCommandBuffer>>;

/// The spare buffers that the renderer keeps for the frames' instances, the
/// largest; each frame most often takes one.
const SPARE: usize = 6;

/// The frames that a test draws to find each buffer taken back.
#[cfg(test)]
const DRAWABLES_IN_FLIGHT: usize = 3;

/// The least that a buffer of instances takes, in bytes.
const SMALLEST: usize = 64 * 1024;

/// Where a draw's instances start in their buffer: a multiple of this, as
/// a vertex buffer's offset is on every GPU of Metal's.
const ALIGNMENT: usize = 256;

/// Has `drawable` call `presented` as it shows, with how long ago it
/// showed, as the display's clock tells, or `None` for a frame that the
/// display skipped.
fn tell_presented(drawable: &ProtocolObject<dyn CAMetalDrawable>, presented: Presented) {
    let presented = std::sync::Mutex::new(Some(presented));
    let handler = block2::RcBlock::new(
        move |drawable: NonNull<ProtocolObject<dyn objc2_metal::MTLDrawable>>| {
            // SAFETY: the drawable that Metal hands its handler, which lives
            // through the call.
            let shown = unsafe { drawable.as_ref() }.presentedTime();
            // Core Animation's clock is the presented time's.
            let ago = (shown > 0.0)
                .then(|| Duration::from_secs_f64((CACurrentMediaTime() - shown).max(0.0)));
            let taken = presented
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            if let Some(presented) = taken {
                presented(ago);
            }
        },
    );
    // SAFETY: a block of the handler's signature, which Metal copies.
    unsafe { drawable.addPresentedHandler(block2::RcBlock::as_ptr(&handler)) };
}

/// A frame that the GPU may still be drawing: its list's `frame`, its
/// commands, and the buffers of its instances, which the GPU reads until it
/// finishes.
#[derive(Debug)]
struct Drawing {
    frame: u64,
    commands: Commands,
    buffers: Vec<Buffer>,
    /// Whether its GPU's time counts among those that
    /// [`Renderer::take_gpu_times`] gives.
    timed: bool,
}

/// The buffers of a frame's instances: each draw's bytes go after the last
/// one's, in the buffer that the frame took last, or in the next, a spare
/// one of the frames that the GPU finished, or a new one, so that a frame
/// of the same size as the last makes none.
struct Arena {
    device: Retained<ProtocolObject<dyn MTLDevice>>,
    spare: Vec<Buffer>,
    /// The buffers that the frame took, the last one the one being filled,
    /// up to `offset`.
    used: Vec<Buffer>,
    offset: usize,
    /// The bytes that the first buffer takes at least: the last frame's.
    first: usize,
    /// The bytes that the frame's instances took.
    bytes: usize,
    /// How many buffers it made, which the tests count.
    #[cfg(test)]
    made: usize,
}

impl Arena {
    /// The buffer that holds `items`' bytes, written into it, and where
    /// they start in it; `None` for no items.
    fn put<T: Copy>(&mut self, items: &[T]) -> Option<(Buffer, usize)> {
        if items.is_empty() {
            return None;
        }
        let length = size_of_val(items);
        let mut start = self.offset.next_multiple_of(ALIGNMENT);
        if self
            .used
            .last()
            .is_none_or(|buffer| start + length > buffer.length())
        {
            let wanted = if self.used.is_empty() {
                length.max(self.first)
            } else {
                length
            };
            let buffer = self.take(wanted)?;
            self.used.push(buffer);
            start = 0;
        }
        let buffer = self.used.last()?;
        // SAFETY: the buffer's bytes from `start` hold `length` more, as the
        // check above keeps them; no frame that the GPU draws reads it, as
        // the frames give their buffers back once the GPU finished them, and
        // this frame's draws read the bytes before `start` alone.
        unsafe {
            std::ptr::copy_nonoverlapping(
                items.as_ptr().cast::<u8>(),
                buffer.contents().as_ptr().cast::<u8>().add(start),
                length,
            );
        }
        self.offset = start + length;
        self.bytes += length;
        Some((buffer.clone(), start))
    }

    /// A buffer of `length` bytes at least: the smallest spare one that
    /// holds them, or a new one, of their next power of two.
    fn take(&mut self, length: usize) -> Option<Buffer> {
        let fitting = self
            .spare
            .iter()
            .enumerate()
            .filter(|(_, buffer)| buffer.length() >= length)
            .min_by_key(|(_, buffer)| buffer.length())
            .map(|(index, _)| index);
        if let Some(index) = fitting {
            return Some(self.spare.swap_remove(index));
        }
        #[cfg(test)]
        {
            self.made += 1;
        }
        self.device.newBufferWithLength_options(
            length.next_power_of_two().max(SMALLEST),
            MTLResourceOptions::StorageModeShared,
        )
    }
}

/// A frame being encoded.
struct Frame<'a> {
    renderer: &'a MetalRenderer,
    encoder: &'a ProtocolObject<dyn MTLRenderCommandEncoder>,
    viewport: [f32; 2],
    /// The clips in force, each within those before it; `None` for one that
    /// leaves nothing to draw.
    clips: Vec<Option<MTLScissorRect>>,
    /// Where the frame's instances go.
    arena: &'a RefCell<Arena>,
}

impl Frame<'_> {
    fn item(&mut self, item: &Item) {
        match item {
            Item::PushClip(rect) => {
                let outer = self
                    .clips
                    .last()
                    .copied()
                    .unwrap_or_else(|| Some(self.full()));
                let clip = outer.and_then(|outer| intersect(outer, *rect));
                self.clips.push(clip);
                if let Some(clip) = clip {
                    self.encoder.setScissorRect(clip);
                }
            }
            Item::PopClip => {
                self.clips.pop();
                let clip = self
                    .clips
                    .last()
                    .copied()
                    .unwrap_or_else(|| Some(self.full()));
                if let Some(clip) = clip {
                    self.encoder.setScissorRect(clip);
                }
            }
            _ if matches!(self.clips.last(), Some(None)) => {}
            Item::Rect { rect, color } => self.rects(&[rect_instance(rect, *color)]),
            Item::Glyphs(glyphs) => {
                let instances: Vec<GlyphInstance> = glyphs
                    .iter()
                    .map(|glyph| {
                        let position = [glyph.x as f32, glyph.y as f32];
                        glyph_instance(position, &glyph.region, glyph.color, glyph.weight)
                    })
                    .collect();
                self.glyphs(&instances);
            }
            Item::Cells(batch) => self.cells(batch),
            Item::Shapes(shapes) => {
                let instances: Vec<ShapeInstance> =
                    shapes.iter().flat_map(shape_instances).collect();
                self.shapes(&instances);
            }
        }
    }

    /// The whole drawable, as a clip.
    fn full(&self) -> MTLScissorRect {
        MTLScissorRect {
            x: 0,
            y: 0,
            width: self.viewport[0] as usize,
            height: self.viewport[1] as usize,
        }
    }

    fn rects(&self, rects: &[RectInstance]) {
        let Some((buffer, offset)) = self.arena.borrow_mut().put(rects) else {
            return;
        };
        self.encoder.setRenderPipelineState(&self.renderer.solid);
        // SAFETY: the buffers and bytes that the rectangles' shaders take, at
        // their indices.
        unsafe {
            self.encoder
                .setVertexBuffer_offset_atIndex(Some(&buffer), offset, 0);
            self.set_viewport();
            self.encoder
                .drawPrimitives_vertexStart_vertexCount_instanceCount(
                    MTLPrimitiveType::TriangleStrip,
                    0,
                    QUAD,
                    rects.len(),
                );
        }
    }

    fn shapes(&self, shapes: &[ShapeInstance]) {
        let Some((buffer, offset)) = self.arena.borrow_mut().put(shapes) else {
            return;
        };
        self.encoder.setRenderPipelineState(&self.renderer.shapes);
        // SAFETY: the buffers and bytes that the shapes' shaders take, at
        // their indices.
        unsafe {
            self.encoder
                .setVertexBuffer_offset_atIndex(Some(&buffer), offset, 0);
            self.set_viewport();
            self.encoder
                .drawPrimitives_vertexStart_vertexCount_instanceCount(
                    MTLPrimitiveType::TriangleStrip,
                    0,
                    QUAD,
                    shapes.len(),
                );
        }
    }

    fn lines(&self, lines: &[LineInstance]) {
        let Some((buffer, offset)) = self.arena.borrow_mut().put(lines) else {
            return;
        };
        self.encoder.setRenderPipelineState(&self.renderer.lines);
        // SAFETY: the buffers and bytes that the lines' shaders take, at
        // their indices.
        unsafe {
            self.encoder
                .setVertexBuffer_offset_atIndex(Some(&buffer), offset, 0);
            self.set_viewport();
            self.encoder
                .drawPrimitives_vertexStart_vertexCount_instanceCount(
                    MTLPrimitiveType::TriangleStrip,
                    0,
                    QUAD,
                    lines.len(),
                );
        }
    }

    fn cells(&self, batch: &CellBatch) {
        let colors: Vec<[f32; 4]> = batch.backgrounds.iter().map(|&color| rgba(color)).collect();
        if let Some((buffer, offset)) = self.arena.borrow_mut().put(&colors) {
            let grid = GridUniform {
                origin: [batch.x as f32, batch.y as f32],
                cell: [batch.cell_width as f32, batch.cell_height as f32],
                columns: u32::from(batch.columns.max(1)),
                _padding: 0,
            };
            self.encoder
                .setRenderPipelineState(&self.renderer.background);
            // SAFETY: the buffers and bytes that the backgrounds' shaders
            // take, at their indices.
            unsafe {
                self.encoder
                    .setVertexBuffer_offset_atIndex(Some(&buffer), offset, 0);
                self.set_viewport();
                self.encoder.setVertexBytes_length_atIndex(
                    NonNull::from(&grid).cast(),
                    size_of::<GridUniform>(),
                    2,
                );
                self.encoder
                    .drawPrimitives_vertexStart_vertexCount_instanceCount(
                        MTLPrimitiveType::TriangleStrip,
                        0,
                        QUAD,
                        colors.len(),
                    );
            }
        }

        let cursor: Vec<RectInstance> = batch
            .cursor
            .iter()
            .flat_map(|cursor| {
                cursor
                    .rects(batch.x, batch.y, batch.cell_width, batch.cell_height)
                    .into_iter()
                    .map(|rect| rect_instance(&rect, cursor.color))
            })
            .collect();
        let block = batch
            .cursor
            .is_some_and(|cursor| cursor.shape == CursorShape::Block);
        if block {
            self.rects(&cursor);
        }

        let (cell_width, cell_height) = (batch.cell_width as i32, batch.cell_height as i32);
        let cell_origin = |column: u16, row: u16| {
            (
                batch.x + i32::from(column) * cell_width,
                batch.y + i32::from(row) * cell_height,
            )
        };
        let lines: Vec<LineInstance> = batch
            .lines
            .iter()
            .map(|line| {
                let (x, y) = cell_origin(line.column, line.row);
                let (top, rows) = line.band(batch.cell_height);
                LineInstance {
                    position: [x as f32, (y + top as i32) as f32],
                    size: [
                        (u32::from(line.columns) * batch.cell_width) as f32,
                        rows as f32,
                    ],
                    color: rgba(line.color),
                    style: match line.style {
                        LineStyle::Single => 0,
                        LineStyle::Double => 1,
                        LineStyle::Curly => 2,
                        LineStyle::Dotted => 3,
                        LineStyle::Dashed => 4,
                    },
                    thickness: line.thickness.max(1) as f32,
                    period: batch.cell_width.max(1) as f32,
                    origin: batch.x as f32,
                }
            })
            .collect();
        let glyphs: Vec<GlyphInstance> = batch
            .glyphs
            .iter()
            .map(|glyph| {
                let (x, y) = cell_origin(glyph.column, glyph.row);
                let position = [(x + glyph.x) as f32, (y + glyph.y) as f32];
                let weight = batch.system_weight && glyph.smoothed;
                glyph_instance(position, &glyph.region, glyph.color, weight)
            })
            .collect();
        self.glyphs(&glyphs);
        self.lines(&lines);
        if !block {
            self.rects(&cursor);
        }
    }

    /// Draws `glyphs`, of either atlas, in their order, each over those
    /// before it.
    fn glyphs(&self, glyphs: &[GlyphInstance]) {
        let Some((buffer, offset)) = self.arena.borrow_mut().put(glyphs) else {
            return;
        };
        self.encoder.setRenderPipelineState(&self.renderer.glyphs);
        // SAFETY: the buffers, bytes and textures that the glyphs' shaders
        // take, at their indices.
        unsafe {
            self.encoder
                .setVertexBuffer_offset_atIndex(Some(&buffer), offset, 0);
            self.set_viewport();
            self.encoder
                .setFragmentTexture_atIndex(Some(self.renderer.texture(Texture::Coverage)), 0);
            self.encoder
                .setFragmentTexture_atIndex(Some(self.renderer.texture(Texture::Color)), 1);
            self.encoder
                .drawPrimitives_vertexStart_vertexCount_instanceCount(
                    MTLPrimitiveType::TriangleStrip,
                    0,
                    QUAD,
                    glyphs.len(),
                );
        }
    }

    /// Gives the shaders the drawable's size.
    ///
    /// # Safety
    ///
    /// The shader that runs takes it at index 1.
    unsafe fn set_viewport(&self) {
        // SAFETY: the bytes of two floats, which the encoder copies.
        unsafe {
            self.encoder.setVertexBytes_length_atIndex(
                NonNull::from(&self.viewport).cast(),
                size_of::<[f32; 2]>(),
                1,
            );
        }
    }
}

/// `color`'s components, premultiplied by its opacity, as the drawable
/// and the window server take them.
fn rgba(color: Color) -> [f32; 4] {
    let alpha = color.alpha;
    [
        color.red * alpha,
        color.green * alpha,
        color.blue * alpha,
        alpha,
    ]
}

/// The color that a drawable clears to: `color`, premultiplied.
pub(crate) fn clear_color(color: Color) -> MTLClearColor {
    let [red, green, blue, alpha] = rgba(color);
    MTLClearColor {
        red: f64::from(red),
        green: f64::from(green),
        blue: f64::from(blue),
        alpha: f64::from(alpha),
    }
}

fn rect_instance(rect: &Rect, color: Color) -> RectInstance {
    RectInstance {
        position: [rect.x as f32, rect.y as f32],
        size: [rect.width as f32, rect.height as f32],
        color: rgba(color),
    }
}

/// The instances of `shape`: its shadow's, if it casts one, under its own.
fn shape_instances(shape: &Shape) -> impl Iterator<Item = ShapeInstance> {
    let rect = shape.rect;
    // A shape of no width or height draws nothing, nor casts a shadow.
    let drawn = rect.width > 0 && rect.height > 0;
    let size = [rect.width as f32, rect.height as f32];
    let extent = [size[0] / 2.0, size[1] / 2.0];
    let center = [rect.x as f32 + extent[0], rect.y as f32 + extent[1]];
    let shadow = shape.shadow.map(|shadow| {
        let sigma = shadow.blur.max(0.0) / 2.0;
        // Three deviations cover all but a thousandth of the blur.
        let reach = (3.0 * sigma).ceil() + 1.0;
        let position = [
            rect.x as f32 + shadow.x - reach,
            rect.y as f32 + shadow.y - reach,
        ];
        ShapeInstance {
            position,
            size: [size[0] + 2.0 * reach, size[1] + 2.0 * reach],
            center: [center[0] + shadow.x, center[1] + shadow.y],
            extent,
            radius: shape.radius,
            border: 0.0,
            sigma,
            _padding: 0.0,
            fill: rgba(shadow.color),
            border_color: [0.0; 4],
            arc: [0.0; 4],
        }
    });
    let own = ShapeInstance {
        position: [rect.x as f32, rect.y as f32],
        size,
        center,
        extent,
        radius: shape.radius.max(0.0),
        border: shape.border.max(0.0),
        sigma: -1.0,
        _padding: 0.0,
        fill: rgba(shape.fill),
        border_color: rgba(shape.border_color),
        arc: shape.arc.map_or([0.0; 4], |arc| {
            [
                arc.start.rem_euclid(1.0),
                arc.span.clamp(0.0, 1.0),
                arc.thickness.max(0.0),
                1.0,
            ]
        }),
    };
    // An arc casts no shadow.
    let shadow = shadow.filter(|_| shape.arc.is_none());
    shadow
        .into_iter()
        .chain(std::iter::once(own))
        .filter(move |_| drawn)
}

fn glyph_instance(
    position: [f32; 2],
    region: &AtlasRegion,
    color: Color,
    system_weight: bool,
) -> GlyphInstance {
    GlyphInstance {
        position,
        size: [region.width as f32, region.height as f32],
        atlas: [region.x, region.y],
        texture: match region.texture {
            Texture::Coverage => 0,
            Texture::Color => 1,
        },
        weight: u32::from(system_weight),
        color: rgba(color),
    }
}

/// The part of `clip` that `rect` covers, if any.
fn intersect(clip: MTLScissorRect, rect: Rect) -> Option<MTLScissorRect> {
    let left = i64::from(rect.x).max(clip.x as i64);
    let top = i64::from(rect.y).max(clip.y as i64);
    let right = (i64::from(rect.x) + i64::from(rect.width)).min((clip.x + clip.width) as i64);
    let bottom = (i64::from(rect.y) + i64::from(rect.height)).min((clip.y + clip.height) as i64);
    (right > left && bottom > top).then(|| MTLScissorRect {
        x: left as usize,
        y: top as usize,
        width: (right - left) as usize,
        height: (bottom - top) as usize,
    })
}

fn make_pipeline(
    device: &ProtocolObject<dyn MTLDevice>,
    library: &ProtocolObject<dyn MTLLibrary>,
    vertex: &str,
    fragment: &str,
) -> Result<Pipeline, Error> {
    let function = |name: &str| {
        library
            .newFunctionWithName(&NSString::from_str(name))
            .ok_or_else(|| Error::Shaders(format!("the library has no function `{name}`")))
    };
    let descriptor = MTLRenderPipelineDescriptor::new();
    let (vertex, fragment) = (function(vertex)?, function(fragment)?);
    descriptor.setVertexFunction(Some(&vertex));
    descriptor.setFragmentFunction(Some(&fragment));
    // SAFETY: a pipeline has a first color attachment.
    let attachment = unsafe { descriptor.colorAttachments().objectAtIndexedSubscript(0) };
    attachment.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
    // Every fragment is premultiplied, and goes over what is under it: a
    // clear one leaves it as it is.
    attachment.setBlendingEnabled(true);
    attachment.setSourceRGBBlendFactor(MTLBlendFactor::One);
    attachment.setSourceAlphaBlendFactor(MTLBlendFactor::One);
    attachment.setDestinationRGBBlendFactor(MTLBlendFactor::OneMinusSourceAlpha);
    attachment.setDestinationAlphaBlendFactor(MTLBlendFactor::OneMinusSourceAlpha);
    device
        .newRenderPipelineStateWithDescriptor_error(&descriptor)
        .map_err(|error| Error::Shaders(error.to_string()))
}

fn make_texture(
    device: &ProtocolObject<dyn MTLDevice>,
    texture: Texture,
    size: u32,
) -> Result<MetalTexture, Error> {
    let format = match texture {
        Texture::Coverage => MTLPixelFormat::R8Unorm,
        Texture::Color => MTLPixelFormat::RGBA8Unorm,
    };
    let side = size.max(1) as usize;
    // SAFETY: a two-dimensional texture of a valid format and size.
    let descriptor = unsafe {
        MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
            format, side, side, false,
        )
    };
    descriptor.setUsage(MTLTextureUsage::ShaderRead);
    descriptor.setStorageMode(MTLStorageMode::Shared);
    device
        .newTextureWithDescriptor(&descriptor)
        .ok_or(Error::Device("texture"))
}

#[cfg(test)]
mod tests {
    use lokyr_render_core::Surface as _;
    use lokyr_render_core::{Arc, CellCursor, CellGlyph, CellLine, LineStyle, PlacedGlyph, Shadow};
    use objc2_quartz_core::CAMetalLayer;

    use super::*;

    fn renderer() -> Option<(Gpu, MetalRenderer)> {
        let Ok(gpu) = Gpu::new() else {
            eprintln!("no Metal device: the test checks nothing");
            return None;
        };
        let renderer = MetalRenderer::new(&gpu, 64, 32).expect("the shaders load");
        Some((gpu, renderer))
    }

    /// Waits for the GPU to write the last uploads.
    fn finish(renderer: &MetalRenderer) {
        if let Some(uploaded) = &renderer.uploaded {
            uploaded.waitUntilCompleted();
        }
    }

    /// The bytes of `region` of the texture, read back.
    fn read(
        texture: &ProtocolObject<dyn MTLTexture>,
        x: usize,
        y: usize,
        width: usize,
        bytes: usize,
    ) -> Vec<u8> {
        let mut data = vec![0_u8; width * bytes];
        let region = MTLRegion {
            origin: MTLOrigin { x, y, z: 0 },
            size: MTLSize {
                width,
                height: 1,
                depth: 1,
            },
        };
        // SAFETY: room for one row of the region, of `bytes` a pixel.
        unsafe {
            texture.getBytes_bytesPerRow_fromRegion_mipmapLevel(
                NonNull::new_unchecked(data.as_mut_ptr().cast()),
                width * bytes,
                region,
                0,
            );
        }
        data
    }

    fn region(texture: Texture, x: u32, y: u32, width: u32, height: u32) -> AtlasRegion {
        AtlasRegion {
            texture,
            x,
            y,
            width,
            height,
        }
    }

    /// The GPU's time of a frame of a screen full of text, `width` by
    /// `height` pixels, of cells of `cell` pixels and glyphs of `glyph`, of
    /// light text on a dark background, its coverage as drawn and of the
    /// system's weight, in turns, by the median of rounds of frames.
    fn time_screen(width: u32, height: u32, cell: (u32, u32), glyph: (u32, u32)) {
        let Ok(gpu) = Gpu::new() else {
            return;
        };
        let mut renderer = MetalRenderer::new(&gpu, 1024, 32).expect("the shaders load");
        // 95 glyphs of a coverage that has the mid-tones of antialiased
        // text: a third of their pixels inked, of every value.
        let mut seed = 7_u32;
        let mut next = || {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            seed >> 16
        };
        let (glyph_width, glyph_height) = glyph;
        let per_row = 1024 / glyph_width;
        let regions: Vec<AtlasRegion> = (0..95)
            .map(|index| {
                region(
                    Texture::Coverage,
                    (index % per_row) * glyph_width,
                    (index / per_row) * glyph_height,
                    glyph_width,
                    glyph_height,
                )
            })
            .collect();
        let uploads: Vec<TextureUpdate> = regions
            .iter()
            .map(|region| TextureUpdate::Upload {
                region: *region,
                data: (0..region.width * region.height)
                    .map(|_| {
                        if next() % 3 == 0 {
                            (next() % 256) as u8
                        } else {
                            0
                        }
                    })
                    .collect(),
            })
            .collect();
        renderer.update_textures(&uploads).unwrap();
        finish(&renderer);
        let (columns, rows) = ((width / cell.0) as u16, (height / cell.1) as u16);
        let light = Color::from_rgb8(0xe6, 0xe6, 0xe6);
        let list = |system_weight: bool| DisplayList {
            frame: 1,
            background: Color::from_rgb8(0x1e, 0x1e, 0x20),
            items: vec![Item::Cells(CellBatch {
                x: 0,
                y: 0,
                columns,
                rows,
                cell_width: cell.0,
                cell_height: cell.1,
                backgrounds: vec![
                    Color::from_rgb8(0x1e, 0x1e, 0x20);
                    usize::from(columns) * usize::from(rows)
                ],
                glyphs: (0..rows)
                    .flat_map(|row| (0..columns).map(move |column| (column, row)))
                    .enumerate()
                    .map(|(index, (column, row))| CellGlyph {
                        column,
                        row,
                        region: regions[index % regions.len()],
                        x: 0,
                        y: 1,
                        color: light,
                        smoothed: true,
                    })
                    .collect(),
                lines: Vec::new(),
                cursor: None,
                system_weight,
            })],
        };
        let (plain, system) = (list(false), list(true));
        let mut frame = |list: &DisplayList, frames: u32| {
            renderer.take_gpu_times();
            for _ in 0..frames {
                renderer.draw_offscreen(list, width, height).unwrap();
            }
            let times = renderer.take_gpu_times();
            times.total.as_secs_f64() * 1e6 / f64::from(times.frames.max(1))
        };
        frame(&plain, 30);
        frame(&system, 30);
        // Pairs of rounds, each of the two first in turn, compared within
        // the pair, as the GPU's clock drifts between them.
        let (mut plains, mut ratios) = (Vec::new(), Vec::new());
        for round in 0..40 {
            let (first, second) = if round % 2 == 0 {
                let plain = frame(&plain, 10);
                (plain, frame(&system, 10))
            } else {
                let system = frame(&system, 10);
                (frame(&plain, 10), system)
            };
            plains.push(first);
            ratios.push(second / first);
        }
        let quartiles = |values: &mut Vec<f64>| {
            values.sort_by(f64::total_cmp);
            let at = |share: f64| values[((values.len() - 1) as f64 * share) as usize];
            (at(0.25), at(0.5), at(0.75))
        };
        let (_, plain, _) = quartiles(&mut plains);
        let (low, ratio, high) = quartiles(&mut ratios);
        let percent = |ratio: f64| (ratio - 1.0) * 100.0;
        println!(
            "{width}x{height}, {columns}x{rows} cells: as drawn {plain:.0} µs a frame; of the system's weight {:+.1}% (quartiles {:+.1}% to {:+.1}%)",
            percent(ratio),
            percent(low),
            percent(high)
        );
    }

    #[test]
    #[ignore = "a measure of the GPU's time, which `-- --ignored --nocapture` prints"]
    fn a_screen_of_text_takes_the_gpu_s_time_of_either_weight() {
        time_screen(3440, 1440, (8, 17), (8, 14));
        time_screen(3456, 2234, (16, 34), (16, 28));
    }

    #[test]
    fn uploads_land_and_survive_a_texture_s_growth() {
        let Some((_, mut renderer)) = renderer() else {
            return;
        };
        assert_eq!(renderer.coverage.width(), 64);
        assert_eq!(renderer.color.width(), 32);
        renderer
            .update_textures(&[
                TextureUpdate::Upload {
                    region: region(Texture::Coverage, 2, 3, 4, 1),
                    data: vec![10, 20, 30, 40],
                },
                TextureUpdate::Grown {
                    texture: Texture::Coverage,
                    size: 128,
                },
                TextureUpdate::Upload {
                    region: region(Texture::Coverage, 100, 90, 2, 1),
                    data: vec![7, 8],
                },
                TextureUpdate::Upload {
                    region: region(Texture::Color, 1, 1, 1, 1),
                    data: vec![1, 2, 3, 4],
                },
            ])
            .unwrap();
        assert_eq!(renderer.coverage.width(), 128);
        finish(&renderer);
        assert_eq!(read(&renderer.coverage, 2, 3, 4, 1), [10, 20, 30, 40]);
        assert_eq!(read(&renderer.coverage, 100, 90, 2, 1), [7, 8]);
        assert_eq!(read(&renderer.color, 1, 1, 1, 4), [1, 2, 3, 4]);
        // Uploads land in their order, after the growth's copy too.
        renderer
            .update_textures(&[
                TextureUpdate::Upload {
                    region: region(Texture::Coverage, 3, 3, 2, 1),
                    data: vec![1, 2],
                },
                TextureUpdate::Grown {
                    texture: Texture::Coverage,
                    size: 256,
                },
                TextureUpdate::Upload {
                    region: region(Texture::Coverage, 4, 3, 1, 1),
                    data: vec![3],
                },
            ])
            .unwrap();
        finish(&renderer);
        assert_eq!(read(&renderer.coverage, 2, 3, 4, 1), [10, 1, 3, 40]);
        assert_eq!(read(&renderer.coverage, 100, 90, 2, 1), [7, 8]);
    }

    #[test]
    fn an_upload_outside_its_texture_is_dropped() {
        let Some((_, mut renderer)) = renderer() else {
            return;
        };
        renderer
            .update_textures(&[TextureUpdate::Upload {
                region: region(Texture::Coverage, 62, 0, 4, 1),
                data: vec![9; 4],
            }])
            .unwrap();
        assert!(renderer.uploaded.is_none());
        assert_eq!(read(&renderer.coverage, 60, 0, 4, 1), [0; 4]);
    }

    fn list() -> DisplayList {
        let white = Color::from_rgb8(255, 255, 255);
        DisplayList {
            frame: 1,
            background: Color::from_rgb8(30, 30, 32),
            items: vec![
                Item::Rect {
                    rect: Rect {
                        x: 0,
                        y: 0,
                        width: 64,
                        height: 4,
                    },
                    color: white,
                },
                Item::PushClip(Rect {
                    x: 0,
                    y: 4,
                    width: 64,
                    height: 28,
                }),
                Item::Cells(CellBatch {
                    x: 2,
                    y: 4,
                    columns: 4,
                    rows: 1,
                    cell_width: 8,
                    cell_height: 16,
                    backgrounds: vec![Color::default(); 4],
                    glyphs: vec![
                        CellGlyph {
                            column: 0,
                            row: 0,
                            region: region(Texture::Coverage, 0, 0, 6, 10),
                            x: 1,
                            y: 3,
                            color: white,
                            smoothed: false,
                        },
                        CellGlyph {
                            column: 2,
                            row: 0,
                            region: region(Texture::Color, 0, 0, 14, 14),
                            x: 1,
                            y: 1,
                            color: white,
                            smoothed: false,
                        },
                    ],
                    lines: vec![CellLine {
                        column: 0,
                        row: 0,
                        columns: 4,
                        style: LineStyle::Single,
                        top: 14,
                        thickness: 1,
                        color: white,
                    }],
                    cursor: Some(CellCursor {
                        column: 1,
                        row: 0,
                        columns: 1,
                        shape: CursorShape::Hollow,
                        thickness: 1,
                        color: white,
                    }),
                    system_weight: false,
                }),
                // A clip outside the drawable leaves nothing to draw.
                Item::PushClip(Rect {
                    x: 100,
                    y: 100,
                    width: 10,
                    height: 10,
                }),
                Item::Glyphs(vec![PlacedGlyph {
                    region: region(Texture::Coverage, 0, 0, 6, 10),
                    x: 50,
                    y: 10,
                    color: white,
                    weight: false,
                }]),
                Item::PopClip,
                Item::PopClip,
            ],
        }
    }

    #[test]
    fn colors_go_premultiplied() {
        let half = Color::from_rgb8(255, 0, 51).with_alpha(0.5);
        assert_eq!(rgba(half), [0.5, 0.0, 0.1, 0.5]);
        assert_eq!(rgba(Color::from_rgb8(255, 255, 255)), [1.0; 4]);
        let clear = clear_color(Color::from_rgb8(255, 255, 255).with_alpha(0.0));
        assert_eq!(
            (clear.red, clear.green, clear.blue, clear.alpha),
            (0.0, 0.0, 0.0, 0.0)
        );
    }

    #[test]
    fn a_frame_draws_a_display_list() {
        let Some((gpu, mut renderer)) = renderer() else {
            return;
        };
        let surface = Surface::with_layer(&gpu, CAMetalLayer::new());
        surface.resize(64, 32, 1.0);
        assert!(renderer.draw(&surface, &list()).unwrap());
    }

    #[test]
    fn a_frame_of_the_same_size_takes_back_the_buffers_of_one_before() {
        let Some((gpu, mut renderer)) = renderer() else {
            return;
        };
        let surface = Surface::with_layer(&gpu, CAMetalLayer::new());
        surface.resize(64, 32, 1.0);
        let finish = |renderer: &mut MetalRenderer| {
            for drawing in &renderer.drawing {
                drawing.commands.waitUntilCompleted();
            }
            renderer.drawing_from();
        };
        assert!(renderer.draw(&surface, &list()).unwrap());
        let made = renderer.made.get();
        assert!(made >= 1);
        // The frames that come once the GPU finished take its buffers back,
        // one frame of each count of the drawables.
        for _ in 0..DRAWABLES_IN_FLIGHT {
            finish(&mut renderer);
            assert!(renderer.draw(&surface, &list()).unwrap());
        }
        assert_eq!(renderer.made.get(), made);
        finish(&mut renderer);
        assert!(renderer.spare.len() <= SPARE);
        // A frame larger than the buffers takes a new one.
        let mut large = list();
        large.items.push(Item::Glyphs(vec![
            PlacedGlyph {
                region: region(Texture::Coverage, 0, 0, 1, 1),
                x: 0,
                y: 0,
                color: Color::from_rgb8(255, 255, 255),
                weight: false,
            };
            SMALLEST
        ]));
        assert!(renderer.draw(&surface, &large).unwrap());
        assert!(renderer.made.get() > made);
    }

    #[test]
    fn the_next_drawable_tells_as_it_shows_once() {
        let Some((gpu, mut renderer)) = renderer() else {
            return;
        };
        let surface = Surface::with_layer(&gpu, CAMetalLayer::new());
        surface.resize(64, 32, 1.0);
        let (sender, told) = std::sync::mpsc::channel();
        renderer.on_next_presented(move |ago| {
            let _ = sender.send(ago);
        });
        assert!(renderer.draw(&surface, &list()).unwrap());
        assert!(renderer.draw(&surface, &list()).unwrap());
        // A layer in no window shows nothing: the display skips its frame.
        let ago = told.recv_timeout(Duration::from_secs(5));
        assert!(ago.is_ok(), "{ago:?}");
        // The second frame tells nothing.
        assert!(told.recv_timeout(Duration::from_millis(500)).is_err());
    }

    #[test]
    fn a_layer_that_presents_with_the_transaction_has_its_frame_scheduled() {
        let Some((gpu, mut renderer)) = renderer() else {
            return;
        };
        let surface = Surface::with_layer(&gpu, CAMetalLayer::new());
        surface.resize(64, 32, 1.0);
        surface.set_presents_with_transaction(true);
        assert!(renderer.draw(&surface, &list()).unwrap());
        // The GPU has the frame's commands as the draw returns, so that the
        // drawable shows as the transaction commits: a frame that it
        // finished already went from the queue.
        let status = renderer
            .drawing
            .back()
            .map_or(MTLCommandBufferStatus::Completed, |drawing| {
                drawing.commands.status()
            });
        assert!(
            matches!(
                status,
                MTLCommandBufferStatus::Scheduled | MTLCommandBufferStatus::Completed
            ),
            "{status:?}"
        );
        surface.set_presents_with_transaction(false);
        assert!(renderer.draw(&surface, &list()).unwrap());
    }

    #[test]
    fn the_gpu_s_times_of_finished_frames_are_taken_once() {
        let Some((gpu, mut renderer)) = renderer() else {
            return;
        };
        let surface = Surface::with_layer(&gpu, CAMetalLayer::new());
        surface.resize(64, 32, 1.0);
        for _ in 0..2 {
            assert!(renderer.draw(&surface, &list()).unwrap());
        }
        // A layer's frame, which is not timed.
        assert!(renderer.draw_untimed(&surface, &list()).unwrap());
        for drawing in &renderer.drawing {
            drawing.commands.waitUntilCompleted();
        }
        let times = renderer.take_gpu_times();
        assert_eq!(times.frames, 2);
        assert!(times.longest > Duration::ZERO && times.total >= times.longest);
        assert_eq!(renderer.take_gpu_times(), GpuTimes::default());
    }

    #[test]
    fn glyphs_of_either_atlas_draw_in_their_order() {
        let Some((_, mut renderer)) = renderer() else {
            return;
        };
        // A pixel of full coverage, and a green one of its own color.
        renderer
            .update_textures(&[
                TextureUpdate::Upload {
                    region: region(Texture::Coverage, 0, 0, 1, 1),
                    data: vec![255],
                },
                TextureUpdate::Upload {
                    region: region(Texture::Color, 0, 0, 1, 1),
                    data: vec![0, 255, 0, 255],
                },
            ])
            .unwrap();
        let red = Color::from_rgb8(255, 0, 0);
        let glyph = |texture| PlacedGlyph {
            region: region(texture, 0, 0, 1, 1),
            x: 0,
            y: 0,
            color: red,
            weight: false,
        };
        let drawn = |renderer: &mut MetalRenderer, glyphs: Vec<PlacedGlyph>| {
            let list = DisplayList {
                frame: 1,
                background: Color::from_rgb8(0, 0, 255),
                items: vec![Item::Glyphs(glyphs)],
            };
            renderer.draw_offscreen(&list, 1, 1).unwrap()
        };
        let color_then_coverage = drawn(
            &mut renderer,
            vec![glyph(Texture::Color), glyph(Texture::Coverage)],
        );
        assert_eq!(color_then_coverage, [255, 0, 0, 255]);
        let coverage_then_color = drawn(
            &mut renderer,
            vec![glyph(Texture::Coverage), glyph(Texture::Color)],
        );
        assert_eq!(coverage_then_color, [0, 255, 0, 255]);
        // A glyph of its own colors takes its color's opacity, as a fade
        // gives it: none, the blue alone; half, the green over the blue,
        // each of half.
        let faded = |opacity| PlacedGlyph {
            color: Color::WHITE.with_alpha(opacity),
            ..glyph(Texture::Color)
        };
        assert_eq!(drawn(&mut renderer, vec![faded(0.0)]), [0, 0, 255, 255]);
        let half = drawn(&mut renderer, vec![faded(0.5)]);
        assert_eq!(half[0], 0);
        assert!(
            half[1].abs_diff(half[2]) <= 2 && (100..=200).contains(&half[1]),
            "{half:?}"
        );
    }

    #[test]
    fn a_list_drawn_offscreen_reads_back_as_rgba() {
        let Some((_, mut renderer)) = renderer() else {
            return;
        };
        let red = Color::from_rgb8(255, 0, 0);
        let list = DisplayList {
            frame: 1,
            background: Color::from_rgb8(0, 0, 255),
            items: vec![Item::Rect {
                rect: Rect {
                    x: 1,
                    y: 2,
                    width: 2,
                    height: 1,
                },
                color: red,
            }],
        };
        let pixels = renderer.draw_offscreen(&list, 4, 3).unwrap();
        let pixel = |x: usize, y: usize| &pixels[(y * 4 + x) * 4..][..4];
        assert_eq!(pixels.len(), 4 * 3 * 4);
        assert_eq!(pixel(0, 0), [0, 0, 255, 255]);
        assert_eq!(pixel(1, 2), [255, 0, 0, 255]);
        assert_eq!(pixel(2, 2), [255, 0, 0, 255]);
        assert_eq!(pixel(3, 2), [0, 0, 255, 255]);
        // A size of nothing is a pixel.
        assert_eq!(renderer.draw_offscreen(&list, 0, 0).unwrap().len(), 4);
    }

    fn rect(x: i32, y: i32, width: u32, height: u32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn a_shape_fills_within_its_border_and_its_rounded_corners() {
        let Some((_, mut renderer)) = renderer() else {
            return;
        };
        let (white, red, blue) = (
            Color::from_rgb8(255, 255, 255),
            Color::from_rgb8(255, 0, 0),
            Color::from_rgb8(0, 0, 255),
        );
        let square = Shape {
            border: 2.0,
            border_color: blue,
            ..Shape::filled(rect(2, 2, 16, 12), red)
        };
        let round = Shape {
            radius: 8.0,
            ..Shape::filled(rect(20, 2, 16, 16), red)
        };
        let list = DisplayList {
            frame: 1,
            background: white,
            items: vec![Item::Shapes(vec![square, round])],
        };
        let rows = drawn(&mut renderer, &list, 40, 20);
        let pixel = |x: usize, y: usize| rows[y][x * 4..][..3].to_vec();
        // The square: its border's two pixels, then its fill; nothing past
        // its edges.
        assert_eq!(pixel(1, 5), [255, 255, 255]);
        assert_eq!(pixel(2, 5), [0, 0, 255]);
        assert_eq!(pixel(3, 5), [0, 0, 255]);
        assert_eq!(pixel(4, 5), [255, 0, 0]);
        assert_eq!(pixel(17, 13), [0, 0, 255]);
        assert_eq!(pixel(15, 11), [255, 0, 0]);
        assert_eq!(pixel(18, 13), [255, 255, 255]);
        // The circle: its corners left out, its middle and its edges'
        // middles filled, its outline's pixels between.
        assert_eq!(pixel(20, 2), [255, 255, 255]);
        assert_eq!(pixel(35, 17), [255, 255, 255]);
        assert_eq!(pixel(28, 10), [255, 0, 0]);
        assert_eq!(pixel(21, 10), [255, 0, 0]);
        assert_eq!(pixel(28, 3), [255, 0, 0]);
        let edge = pixel(22, 4);
        assert!(edge[1] > 0 && edge[1] < 255, "{edge:?}");
    }

    #[test]
    fn an_arc_draws_its_part_of_the_ring_inside_its_shape() {
        let Some((_, mut renderer)) = renderer() else {
            return;
        };
        let (white, red) = (Color::from_rgb8(255, 255, 255), Color::from_rgb8(255, 0, 0));
        let arc = |x, start, span| Shape {
            arc: Some(Arc {
                start,
                span,
                thickness: 4.0,
            }),
            // An arc takes neither a border nor a shadow.
            border: 3.0,
            border_color: Color::from_rgb8(0, 0, 255),
            shadow: Some(Shadow {
                x: 0.0,
                y: 0.0,
                blur: 0.0,
                color: Color::BLACK,
            }),
            ..Shape::filled(rect(x, 0, 20, 20), red)
        };
        // A quarter from the top, round to the right; a whole ring; a
        // quarter from three quarters, round from the left to the top.
        let list = DisplayList {
            frame: 1,
            background: white,
            items: vec![Item::Shapes(vec![
                arc(0, 0.0, 0.25),
                arc(24, 0.0, 1.0),
                arc(48, 0.75, 0.25),
            ])],
        };
        let rows = drawn(&mut renderer, &list, 72, 20);
        let pixel = |x: usize, y: usize| rows[y][x * 4..][..3].to_vec();
        let red = [255, 0, 0];
        let white = [255, 255, 255];
        // The first: its line 4 pixels in, at the top right; not at the
        // left, the bottom, its middle or its corner.
        assert_eq!(pixel(14, 2), red);
        assert_eq!(pixel(17, 6), red);
        assert_eq!(pixel(5, 2), white);
        assert_eq!(pixel(2, 10), white);
        assert_eq!(pixel(10, 17), white);
        assert_eq!(pixel(10, 10), white);
        assert_eq!(pixel(0, 0), white);
        // The ring: all round, its middle open.
        for (x, y) in [(34, 1), (42, 10), (34, 18), (25, 10)] {
            assert_eq!(pixel(x, y), red, "{x}, {y}");
        }
        assert_eq!(pixel(34, 10), white);
        assert_eq!(pixel(24, 0), white);
        // The last: at the top left, not at the right.
        assert_eq!(pixel(53, 2), red);
        assert_eq!(pixel(62, 2), white);
        // Its ends cut across the line: the first's start between the top's
        // two middle pixels.
        assert!(pixel(9, 1)[1] > 240, "{:?}", pixel(9, 1));
        assert!(pixel(10, 1)[1] < 15, "{:?}", pixel(10, 1));
    }

    #[test]
    fn a_border_not_opaque_shows_the_fill_under_it_and_a_shape_of_no_size_draws_nothing() {
        let Some((_, mut renderer)) = renderer() else {
            return;
        };
        let (dark, panel) = (
            Color::from_rgb8(30, 30, 32),
            Color::from_rgb8(246, 246, 246),
        );
        let (red, blue, white) = (
            Color::from_rgb8(255, 0, 0),
            Color::from_rgb8(0, 0, 255),
            Color::from_rgb8(255, 255, 255),
        );
        // A light panel of a border of black at 0.14, over a dark
        // background; a red circle of a blue border; a shadowed shape of no
        // width.
        let bordered = Shape {
            border: 2.0,
            border_color: Color::BLACK.with_alpha(0.14),
            ..Shape::filled(rect(2, 2, 16, 12), panel)
        };
        let round = Shape {
            radius: 8.0,
            border: 2.0,
            border_color: blue,
            ..Shape::filled(rect(20, 2, 16, 16), red)
        };
        let empty = Shape {
            shadow: Some(Shadow {
                x: 0.0,
                y: 0.0,
                blur: 8.0,
                color: Color::BLACK.with_alpha(0.5),
            }),
            ..Shape::filled(rect(4, 18, 0, 0), panel)
        };
        let list = |background| DisplayList {
            frame: 1,
            background,
            items: vec![Item::Shapes(vec![bordered, round, empty])],
        };
        let rows = drawn(&mut renderer, &list(dark), 40, 24);
        let pixel = |rows: &[Vec<u8>], x: usize, y: usize| rows[y][x * 4..][..3].to_vec();
        // The border over the panel's fill, 246 of 0.86, not over the dark
        // background behind it, 30 of 0.86; the fill inside it.
        let ring = pixel(&rows, 2, 6);
        assert!(
            ring.iter().all(|&value| value.abs_diff(212) <= 2),
            "{ring:?}"
        );
        assert_eq!(pixel(&rows, 8, 8), [246, 246, 246]);
        // Nothing of the shape of no width, nor of its shadow.
        assert_eq!(pixel(&rows, 4, 18), [30, 30, 32]);
        assert_eq!(pixel(&rows, 6, 20), [30, 30, 32]);
        // At the circle's edge, on white, its border's blue over the white
        // alone, none of the red fill under it.
        let rows = drawn(&mut renderer, &list(white), 40, 24);
        let edge = pixel(&rows, 22, 4);
        assert!(edge[0] < 250 && edge[0].abs_diff(edge[1]) <= 1, "{edge:?}");
    }

    #[test]
    fn a_shadow_darkens_under_its_shape_alone_and_fades() {
        let Some((_, mut renderer)) = renderer() else {
            return;
        };
        let white = Color::from_rgb8(255, 255, 255);
        let shadowed = |blur: f32| DisplayList {
            frame: 1,
            background: white,
            items: vec![Item::Shapes(vec![Shape {
                shadow: Some(Shadow {
                    x: 0.0,
                    y: 4.0,
                    blur,
                    color: Color::BLACK.with_alpha(0.5),
                }),
                ..Shape::filled(rect(10, 10, 20, 20), white)
            }])],
        };
        let rows = drawn(&mut renderer, &shadowed(0.0), 40, 44);
        let gray = |rows: &[Vec<u8>], x: usize, y: usize| rows[y][x * 4];
        // Sharp: half black under the shape's bottom, moved 4 down, and
        // nothing above it or past it.
        assert_eq!(gray(&rows, 20, 20), 255);
        assert!(
            gray(&rows, 20, 31).abs_diff(128) <= 1,
            "{}",
            gray(&rows, 20, 31)
        );
        assert!(gray(&rows, 20, 33).abs_diff(128) <= 1);
        assert_eq!(gray(&rows, 20, 34), 255);
        assert_eq!(gray(&rows, 20, 8), 255);
        // Blurred: darkest at the edge, fading below it, half as dark at
        // the moved edge, and nothing a blur's three deviations past it.
        let rows = drawn(&mut renderer, &shadowed(8.0), 40, 50);
        let column: Vec<u8> = (30..50).map(|y| gray(&rows, 20, y)).collect();
        assert!(
            column.windows(2).all(|pair| pair[0] <= pair[1]),
            "{column:?}"
        );
        assert!(gray(&rows, 20, 33).abs_diff(185) <= 2, "{column:?}");
        assert!(gray(&rows, 20, 43) >= 253, "{column:?}");
        assert_eq!(gray(&rows, 20, 48), 255);
        assert_eq!(gray(&rows, 20, 20), 255);
    }

    #[test]
    fn the_frames_that_the_gpu_draws_are_told_until_it_finishes() {
        let Some((gpu, mut renderer)) = renderer() else {
            return;
        };
        assert_eq!(renderer.drawing_from(), None);
        let surface = Surface::with_layer(&gpu, CAMetalLayer::new());
        surface.resize(64, 32, 1.0);
        for frame in [7, 8] {
            let list = DisplayList { frame, ..list() };
            assert!(renderer.draw(&surface, &list).unwrap());
        }
        // The oldest that the GPU has not finished, if it has not.
        assert!(matches!(renderer.drawing_from(), None | Some(7 | 8)));
        let last = renderer
            .drawing
            .back()
            .map(|drawing| drawing.commands.clone())
            .expect("a frame on the GPU");
        last.waitUntilCompleted();
        assert_eq!(renderer.drawing_from(), None);
        assert!(renderer.drawing.is_empty());
    }

    #[test]
    fn a_glyph_of_the_system_s_weight_thins_dark_text_alone() {
        let Some((_, mut renderer)) = renderer() else {
            return;
        };
        // A glyph of four pixels, of no, a quarter, half and whole coverage.
        let coverage = region(Texture::Coverage, 0, 0, 4, 1);
        renderer
            .update_textures(&[TextureUpdate::Upload {
                region: coverage,
                data: vec![0, 64, 128, 255],
            }])
            .unwrap();
        finish(&renderer);
        let (black, white) = (Color::from_rgb8(0, 0, 0), Color::from_rgb8(255, 255, 255));
        let row = |renderer: &mut MetalRenderer, item: Item, background: Color| {
            let list = DisplayList {
                frame: 1,
                background,
                items: vec![item],
            };
            let pixels = drawn(renderer, &list, 4, 1);
            pixels[0]
                .chunks(4)
                .map(|pixel| pixel[1])
                .collect::<Vec<u8>>()
        };
        let cells = |system_weight, color| {
            Item::Cells(CellBatch {
                columns: 1,
                rows: 1,
                cell_width: 4,
                cell_height: 1,
                backgrounds: vec![Color::CLEAR],
                glyphs: vec![CellGlyph {
                    column: 0,
                    row: 0,
                    region: coverage,
                    x: 0,
                    y: 0,
                    color,
                    smoothed: true,
                }],
                system_weight,
                ..CellBatch::default()
            })
        };
        let placed = |color| {
            Item::Glyphs(vec![PlacedGlyph {
                region: coverage,
                x: 0,
                y: 0,
                color,
                weight: true,
            }])
        };
        let near = |row: Vec<u8>, expected: [u8; 4]| {
            assert!(
                row.iter()
                    .zip(expected)
                    .all(|(&got, want)| got.abs_diff(want) <= 2),
                "{row:?} against {expected:?}"
            );
        };
        // Black on white: the coverage as drawn, then squared, as black's
        // luminance is 0.
        near(
            row(&mut renderer, cells(false, black), white),
            [255, 191, 127, 0],
        );
        near(
            row(&mut renderer, cells(true, black), white),
            [255, 239, 191, 0],
        );
        near(row(&mut renderer, placed(black), white), [255, 239, 191, 0]);
        // A glyph that was drawn without the font smoothing keeps its
        // coverage, of the batch's weight or the glyph's.
        let unsmoothed = |item: Item| match item {
            Item::Cells(mut batch) => {
                batch.glyphs[0].smoothed = false;
                Item::Cells(batch)
            }
            Item::Glyphs(mut glyphs) => {
                glyphs[0].weight = false;
                Item::Glyphs(glyphs)
            }
            other => other,
        };
        near(
            row(&mut renderer, unsmoothed(cells(true, black)), white),
            [255, 191, 127, 0],
        );
        near(
            row(&mut renderer, unsmoothed(placed(black)), white),
            [255, 191, 127, 0],
        );
        // White on black, of luminance 1: as drawn either way.
        near(
            row(&mut renderer, cells(true, white), black),
            [0, 64, 128, 255],
        );
        near(row(&mut renderer, placed(white), black), [0, 64, 128, 255]);
    }

    /// `list` drawn into a texture of `width` by `height` pixels, read
    /// back, a row of pixels at a time, four bytes a pixel: red, green,
    /// blue and opacity.
    fn drawn(
        renderer: &mut MetalRenderer,
        list: &DisplayList,
        width: usize,
        height: usize,
    ) -> Vec<Vec<u8>> {
        let pixels = renderer
            .draw_offscreen(list, width as u32, height as u32)
            .unwrap();
        assert_eq!(pixels.len(), width * height * 4);
        pixels.chunks(width * 4).map(<[u8]>::to_vec).collect()
    }

    /// The decorations of a row each, white on black, in cells of `width`
    /// by `height` pixels, four columns wide, whose underline and
    /// strikeout are at `underline` and `strikeout`, of `thickness`: a
    /// single, a double, a curly, a dotted and a dashed underline, and a
    /// strikeout with an overline. Returns each pixel's red, by row.
    fn decorations(
        renderer: &mut MetalRenderer,
        (width, height): (u32, u32),
        (underline, strikeout, thickness): (u32, u32, u32),
    ) -> Vec<Vec<u8>> {
        let white = Color::from_rgb8(255, 255, 255);
        let line = |row: u16, style: LineStyle, top: u32| CellLine {
            column: 0,
            row,
            columns: 4,
            style,
            top,
            thickness,
            color: white,
        };
        let lines = vec![
            line(0, LineStyle::Single, underline),
            line(1, LineStyle::Double, underline),
            line(2, LineStyle::Curly, underline),
            line(3, LineStyle::Dotted, underline),
            line(4, LineStyle::Dashed, underline),
            line(5, LineStyle::Single, strikeout),
            line(5, LineStyle::Single, 0),
        ];
        let list = DisplayList {
            background: Color::from_rgb8(0, 0, 0),
            items: vec![Item::Cells(CellBatch {
                x: 0,
                y: 0,
                columns: 4,
                rows: 6,
                cell_width: width,
                cell_height: height,
                backgrounds: vec![Color::default(); 24],
                glyphs: Vec::new(),
                lines,
                cursor: None,
                system_weight: false,
            })],
            frame: 1,
        };
        let pixels = drawn(renderer, &list, 4 * width as usize, 6 * height as usize);
        pixels
            .into_iter()
            .map(|row| row.chunks(4).map(|pixel| pixel[0]).collect())
            .collect()
    }

    #[test]
    fn every_decoration_draws_its_pattern_at_two_sizes() {
        let Some((_gpu, mut renderer)) = renderer() else {
            return;
        };
        // Cells of a scale of 1 and of 2, with their underline and
        // strikeout where a face of their size puts them.
        for (cell, lines) in [
            ((8_u32, 16_u32), (13_u32, 8_u32, 1_u32)),
            ((16, 34), (28, 17, 2)),
        ] {
            let (width, height) = cell;
            let (underline, strikeout, t) = lines;
            let red = decorations(&mut renderer, cell, lines);
            let lit = |row: u32, y: u32, x: u32| red[(row * height + y) as usize][x as usize] > 127;
            let any = |row: u32, y: u32, x: u32| red[(row * height + y) as usize][x as usize] > 0;
            let columns = 4 * width;
            // The rows of a cell that a row's pattern lights, at any column.
            let rows_lit = |row: u32| -> Vec<u32> {
                (0..height)
                    .filter(|&y| (0..columns).any(|x| any(row, y, x)))
                    .collect()
            };
            // Single: the underline's rows, across every column.
            assert_eq!(
                rows_lit(0),
                (underline..underline + t).collect::<Vec<_>>(),
                "{cell:?}"
            );
            assert!((0..columns).all(|x| lit(0, underline, x)));
            // Double: two strokes, and the gap of a stroke between.
            let double: Vec<u32> = (underline..underline + t)
                .chain(underline + 2 * t..underline + 3 * t)
                .collect();
            assert_eq!(rows_lit(1), double, "{cell:?}");
            assert!((0..columns).all(|x| lit(1, underline + 2 * t, x)));
            // Curly: every column lit, high at a quarter of each cell and
            // low at three quarters, within its band.
            let (top, rows) = CellLine {
                style: LineStyle::Curly,
                top: underline,
                thickness: t,
                column: 0,
                row: 0,
                columns: 4,
                color: Color::default(),
            }
            .band(height);
            assert!(
                rows_lit(2).iter().all(|y| (top..top + rows).contains(y)),
                "{cell:?}"
            );
            let brightest = |x: u32| {
                (0..height)
                    .max_by_key(|&y| red[(2 * height + y) as usize][x as usize])
                    .unwrap()
            };
            for cell_start in (0..columns).step_by(width as usize) {
                assert!(
                    (cell_start..cell_start + width).all(|x| (0..height).any(|y| any(2, y, x)))
                );
                let (high, low) = (cell_start + width / 4, cell_start + 3 * width / 4);
                assert!(brightest(high) < brightest(low), "{cell:?} at {cell_start}");
            }
            // Dotted: dots of the thickness, as far apart.
            for x in 0..columns {
                assert_eq!(lit(3, underline, x), (x / t) % 2 == 0, "{cell:?} at {x}");
            }
            assert_eq!(rows_lit(3), (underline..underline + t).collect::<Vec<_>>());
            // Dashed: a dash in the middle of each cell, of three fifths.
            let dash = (width as f32 * 0.6).round() as u32;
            let start = (width - dash) / 2;
            for x in 0..columns {
                let within = x % width;
                assert_eq!(
                    lit(4, underline, x),
                    within >= start && within < start + dash,
                    "{cell:?} at {x}"
                );
            }
            // The strikeout and the overline.
            let mut both: Vec<u32> = (0..t).chain(strikeout..strikeout + t).collect();
            both.dedup();
            assert_eq!(rows_lit(5), both, "{cell:?}");
        }
    }

    #[test]
    fn a_surface_of_no_size_draws_nothing() {
        let Some((gpu, mut renderer)) = renderer() else {
            return;
        };
        let surface = Surface::with_layer(&gpu, CAMetalLayer::new());
        surface.resize(0, 0, 1.0);
        assert!(!renderer.draw(&surface, &list()).unwrap());
    }

    #[test]
    fn clips_meet_or_leave_nothing() {
        let full = MTLScissorRect {
            x: 0,
            y: 0,
            width: 100,
            height: 50,
        };
        let inner = intersect(
            full,
            Rect {
                x: -10,
                y: 40,
                width: 30,
                height: 30,
            },
        );
        assert_eq!(
            inner.map(|clip| (clip.x, clip.y, clip.width, clip.height)),
            Some((0, 40, 20, 10))
        );
        let outside = Rect {
            x: 100,
            y: 0,
            width: 5,
            height: 5,
        };
        assert!(intersect(full, outside).is_none());
    }
}
