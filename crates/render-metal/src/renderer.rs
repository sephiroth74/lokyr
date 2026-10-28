//! The renderer of Metal: the atlas's textures, the pipelines of the shaders,
//! and a frame drawn from a display list.

use std::collections::VecDeque;
use std::ffi::c_void;
use std::ptr::NonNull;

use dispatch2::DispatchData;
use lokyr_render_core::{
    AtlasRegion, CellBatch, Color, CursorShape, DisplayList, Item, LineStyle, Rect, Renderer,
    Texture, TextureUpdate,
};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSString;
use objc2_metal::{
    MTLBlendFactor, MTLBlitCommandEncoder, MTLBuffer, MTLClearColor, MTLCommandBuffer,
    MTLCommandBufferStatus, MTLCommandEncoder, MTLCommandQueue, MTLDevice, MTLLibrary,
    MTLLoadAction, MTLOrigin, MTLPixelFormat, MTLPrimitiveType, MTLRenderCommandEncoder,
    MTLRenderPassDescriptor, MTLRenderPipelineDescriptor, MTLRenderPipelineState,
    MTLResourceOptions, MTLScissorRect, MTLSize, MTLStorageMode, MTLStoreAction, MTLTexture,
    MTLTextureDescriptor, MTLTextureUsage,
};
use objc2_quartz_core::CAMetalDrawable;

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
    _padding: [u32; 2],
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
    coverage_glyphs: Pipeline,
    color_glyphs: Pipeline,
    coverage: MetalTexture,
    color: MetalTexture,
    /// The commands of the frames that the GPU may still draw, by their
    /// lists' frames, from the oldest.
    drawing: VecDeque<(u64, Retained<ProtocolObject<dyn MTLCommandBuffer>>)>,
    /// The commands of the last uploads, which the tests wait for.
    #[cfg(test)]
    uploaded: Option<Retained<ProtocolObject<dyn MTLCommandBuffer>>>,
}

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
            coverage_glyphs: pipeline("glyph_vertex", "coverage_fragment")?,
            color_glyphs: pipeline("glyph_vertex", "color_fragment")?,
            coverage: make_texture(&device, Texture::Coverage, coverage)?,
            color: make_texture(&device, Texture::Color, color)?,
            queue: gpu.queue.clone(),
            device,
            drawing: VecDeque::new(),
            #[cfg(test)]
            uploaded: None,
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
        let Some(drawable) = target.layer.nextDrawable() else {
            return Ok(false);
        };
        let buffer = self.encode(&drawable.texture(), list)?;
        buffer.presentDrawable(ProtocolObject::from_ref(&*drawable));
        buffer.commit();
        self.drawing.push_back((list.frame, buffer));
        self.drop_finished();
        Ok(true)
    }

    fn drawing_from(&mut self) -> Option<u64> {
        self.drop_finished();
        self.drawing.front().map(|&(frame, _)| frame)
    }
}

impl MetalRenderer {
    /// The commands that draw `list` into `output`, to commit.
    fn encode(
        &self,
        output: &ProtocolObject<dyn MTLTexture>,
        list: &DisplayList,
    ) -> Result<Retained<ProtocolObject<dyn MTLCommandBuffer>>, Error> {
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
        };
        for item in &list.items {
            frame.item(item);
        }
        encoder.endEncoding();
        Ok(buffer)
    }

    /// Forgets the frames that the GPU finished, from the oldest: a queue's
    /// commands finish in their order.
    fn drop_finished(&mut self) {
        while self.drawing.front().is_some_and(|(_, buffer)| {
            matches!(
                buffer.status(),
                MTLCommandBufferStatus::Completed | MTLCommandBufferStatus::Error
            )
        }) {
            self.drawing.pop_front();
        }
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
                let instances: Vec<(Texture, GlyphInstance)> = glyphs
                    .iter()
                    .map(|glyph| {
                        let position = [glyph.x as f32, glyph.y as f32];
                        (
                            glyph.region.texture,
                            glyph_instance(position, &glyph.region, glyph.color),
                        )
                    })
                    .collect();
                self.glyphs(&instances);
            }
            Item::Cells(batch) => self.cells(batch),
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
        let Some(buffer) = self.renderer.buffer(rects) else {
            return;
        };
        self.encoder.setRenderPipelineState(&self.renderer.solid);
        // SAFETY: the buffers and bytes that the rectangles' shaders take, at
        // their indices.
        unsafe {
            self.encoder
                .setVertexBuffer_offset_atIndex(Some(&buffer), 0, 0);
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

    fn lines(&self, lines: &[LineInstance]) {
        let Some(buffer) = self.renderer.buffer(lines) else {
            return;
        };
        self.encoder.setRenderPipelineState(&self.renderer.lines);
        // SAFETY: the buffers and bytes that the lines' shaders take, at
        // their indices.
        unsafe {
            self.encoder
                .setVertexBuffer_offset_atIndex(Some(&buffer), 0, 0);
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
        if let Some(buffer) = self.renderer.buffer(&colors) {
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
                    .setVertexBuffer_offset_atIndex(Some(&buffer), 0, 0);
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
        let glyphs: Vec<(Texture, GlyphInstance)> = batch
            .glyphs
            .iter()
            .map(|glyph| {
                let (x, y) = cell_origin(glyph.column, glyph.row);
                let position = [(x + glyph.x) as f32, (y + glyph.y) as f32];
                (
                    glyph.region.texture,
                    glyph_instance(position, &glyph.region, glyph.color),
                )
            })
            .collect();
        self.glyphs(&glyphs);
        self.lines(&lines);
        if !block {
            self.rects(&cursor);
        }
    }

    /// Draws `glyphs`, those of each texture with its pipeline.
    fn glyphs(&self, glyphs: &[(Texture, GlyphInstance)]) {
        for (texture, pipeline) in [
            (Texture::Coverage, &self.renderer.coverage_glyphs),
            (Texture::Color, &self.renderer.color_glyphs),
        ] {
            let instances: Vec<GlyphInstance> = glyphs
                .iter()
                .filter(|(of, _)| *of == texture)
                .map(|&(_, instance)| instance)
                .collect();
            let Some(buffer) = self.renderer.buffer(&instances) else {
                continue;
            };
            self.encoder.setRenderPipelineState(pipeline);
            // SAFETY: the buffers, bytes and texture that the glyphs' shaders
            // take, at their indices.
            unsafe {
                self.encoder
                    .setVertexBuffer_offset_atIndex(Some(&buffer), 0, 0);
                self.set_viewport();
                self.encoder
                    .setFragmentTexture_atIndex(Some(self.renderer.texture(texture)), 0);
                self.encoder
                    .drawPrimitives_vertexStart_vertexCount_instanceCount(
                        MTLPrimitiveType::TriangleStrip,
                        0,
                        QUAD,
                        instances.len(),
                    );
            }
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

fn glyph_instance(position: [f32; 2], region: &AtlasRegion, color: Color) -> GlyphInstance {
    GlyphInstance {
        position,
        size: [region.width as f32, region.height as f32],
        atlas: [region.x, region.y],
        _padding: [0; 2],
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
    use lokyr_render_core::{CellCursor, CellGlyph, CellLine, LineStyle, PlacedGlyph};
    use objc2_metal::MTLRegion;
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
                        },
                        CellGlyph {
                            column: 2,
                            row: 0,
                            region: region(Texture::Color, 0, 0, 14, 14),
                            x: 1,
                            y: 1,
                            color: white,
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
        let (_, last) = renderer
            .drawing
            .back()
            .cloned()
            .expect("a frame on the GPU");
        last.waitUntilCompleted();
        assert_eq!(renderer.drawing_from(), None);
        assert!(renderer.drawing.is_empty());
    }

    /// `list` drawn into a texture of `width` by `height` pixels, read
    /// back, a row of pixels at a time, four bytes a pixel: blue, green,
    /// red and opacity.
    fn drawn(
        renderer: &MetalRenderer,
        list: &DisplayList,
        width: usize,
        height: usize,
    ) -> Vec<Vec<u8>> {
        // SAFETY: a two-dimensional texture of a valid format and size.
        let descriptor = unsafe {
            MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
                MTLPixelFormat::BGRA8Unorm,
                width,
                height,
                false,
            )
        };
        descriptor.setUsage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
        descriptor.setStorageMode(MTLStorageMode::Shared);
        let output = renderer
            .device
            .newTextureWithDescriptor(&descriptor)
            .unwrap();
        let buffer = renderer.encode(&output, list).unwrap();
        buffer.commit();
        buffer.waitUntilCompleted();
        (0..height).map(|y| read(&output, 0, y, width, 4)).collect()
    }

    /// The decorations of a row each, white on black, in cells of `width`
    /// by `height` pixels, four columns wide, whose underline and
    /// strikeout are at `underline` and `strikeout`, of `thickness`: a
    /// single, a double, a curly, a dotted and a dashed underline, and a
    /// strikeout with an overline. Returns each pixel's red, by row.
    fn decorations(
        renderer: &MetalRenderer,
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
            })],
            frame: 1,
        };
        let pixels = drawn(renderer, &list, 4 * width as usize, 6 * height as usize);
        pixels
            .into_iter()
            .map(|row| row.chunks(4).map(|pixel| pixel[2]).collect())
            .collect()
    }

    #[test]
    fn every_decoration_draws_its_pattern_at_two_sizes() {
        let Some((_gpu, renderer)) = renderer() else {
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
            let red = decorations(&renderer, cell, lines);
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
