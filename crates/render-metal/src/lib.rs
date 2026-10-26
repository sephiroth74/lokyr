//! Metal renderer for macOS.
//!
//! Draws a window's display list, chrome and panes alike, into the window's
//! single drawable: the [`Gpu`] is shared by every window, and each window
//! has a [`Surface`], its `CAMetalLayer`.

#![cfg(target_os = "macos")]

mod renderer;

pub use renderer::MetalRenderer;

use bolt_render_core::Color;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2_core_foundation::CGSize;
use objc2_core_graphics::{CGColorSpace, kCGColorSpaceSRGB};
use objc2_metal::{
    MTLClearColor, MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue,
    MTLCreateSystemDefaultDevice, MTLDevice, MTLLoadAction, MTLPixelFormat,
    MTLRenderPassDescriptor, MTLStoreAction,
};
use objc2_quartz_core::{CALayer, CAMetalDrawable, CAMetalLayer};
use raw_window_handle::{RawWindowHandle, WindowHandle};

/// The drawables of a window's layer: two, so that the GPU draws a frame
/// while the display shows the one before.
const DRAWABLES: usize = 2;

/// What went wrong with the GPU.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The Mac has no Metal device.
    #[error("no Metal device")]
    NoDevice,
    /// The device gave no command queue, or no command buffer.
    #[error("the Metal device gave no {0}")]
    Device(&'static str),
    /// The window is not an AppKit view backed by a `CAMetalLayer`.
    #[error("the window has no Metal layer to draw into")]
    NoLayer,
    /// The shaders did not load, or a pipeline of them did not build.
    #[error("the shaders: {0}")]
    Shaders(String),
}

/// The GPU, and its queue of commands, which every window's surface shares.
#[derive(Debug)]
pub struct Gpu {
    device: Retained<ProtocolObject<dyn MTLDevice>>,
    queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
}

impl Gpu {
    /// The system's default GPU.
    pub fn new() -> Result<Self, Error> {
        let device = MTLCreateSystemDefaultDevice().ok_or(Error::NoDevice)?;
        let queue = device
            .newCommandQueue()
            .ok_or(Error::Device("command queue"))?;
        Ok(Self { device, queue })
    }

    /// The GPU's name, such as "Apple M5 Pro".
    pub fn name(&self) -> String {
        self.device.name().to_string()
    }
}

/// A window's `CAMetalLayer`, which the surface sets up and draws into.
#[derive(Debug)]
pub struct Surface {
    layer: Retained<CAMetalLayer>,
    queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
}

impl Surface {
    /// The surface of the window that `handle` names: an AppKit view backed
    /// by a `CAMetalLayer`, which the surface retains, so that it outlives
    /// the borrow of the handle.
    pub fn new(gpu: &Gpu, handle: WindowHandle<'_>) -> Result<Self, Error> {
        let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
            return Err(Error::NoLayer);
        };
        // SAFETY: the handle holds a view that lives while it is borrowed.
        let view: &AnyObject = unsafe { appkit.ns_view.cast().as_ref() };
        // SAFETY: `NSView`'s `layer` has this signature.
        let layer: Option<Retained<CALayer>> = unsafe { msg_send![view, layer] };
        let layer = layer
            .and_then(|layer| layer.downcast::<CAMetalLayer>().ok())
            .ok_or(Error::NoLayer)?;
        Ok(Self::with_layer(gpu, layer))
    }

    /// The surface of `layer`, set up for the GPU: BGRA, 8 bits a component,
    /// in sRGB; drawables that only display, two of them.
    fn with_layer(gpu: &Gpu, layer: Retained<CAMetalLayer>) -> Self {
        layer.setDevice(Some(&gpu.device));
        layer.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
        layer.setFramebufferOnly(true);
        layer.setMaximumDrawableCount(DRAWABLES);
        // SAFETY: the name is Core Graphics' constant.
        let srgb = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }));
        layer.setColorspace(srgb.as_deref());
        Self {
            layer,
            queue: gpu.queue.clone(),
        }
    }

    /// Sizes the drawables to the window's content, `width` by `height`
    /// pixels at `scale`.
    pub fn resize(&self, width: u32, height: u32, scale: f64) {
        self.layer.setContentsScale(scale);
        self.layer
            .setDrawableSize(CGSize::new(f64::from(width), f64::from(height)));
    }

    /// Draws a frame of one color, and presents it. Returns whether it did:
    /// no drawable comes while the size is zero, as when the window is
    /// minimized, nor when every drawable is in use for a second.
    pub fn clear(&self, color: Color) -> Result<bool, Error> {
        Ok(self.encode_clear(color)?.is_some())
    }

    /// Encodes and commits a frame of one color, and returns its command
    /// buffer.
    fn encode_clear(
        &self,
        color: Color,
    ) -> Result<Option<Retained<ProtocolObject<dyn MTLCommandBuffer>>>, Error> {
        let Some(drawable) = self.layer.nextDrawable() else {
            return Ok(None);
        };
        let pass = MTLRenderPassDescriptor::new();
        // SAFETY: a render pass has a first color attachment.
        let attachment = unsafe { pass.colorAttachments().objectAtIndexedSubscript(0) };
        attachment.setTexture(Some(&drawable.texture()));
        attachment.setLoadAction(MTLLoadAction::Clear);
        attachment.setStoreAction(MTLStoreAction::Store);
        attachment.setClearColor(MTLClearColor {
            red: f64::from(color.red),
            green: f64::from(color.green),
            blue: f64::from(color.blue),
            alpha: 1.0,
        });
        let buffer = self
            .queue
            .commandBuffer()
            .ok_or(Error::Device("command buffer"))?;
        let encoder = buffer
            .renderCommandEncoderWithDescriptor(&pass)
            .ok_or(Error::Device("render command encoder"))?;
        encoder.endEncoding();
        buffer.presentDrawable(ProtocolObject::from_ref(&*drawable));
        buffer.commit();
        Ok(Some(buffer))
    }
}

#[cfg(test)]
mod tests {
    use objc2_metal::MTLCommandBufferStatus;
    use raw_window_handle::WebWindowHandle;

    use super::*;

    /// The GPU, or `None` where the machine has none, as some CI runners.
    fn gpu() -> Option<Gpu> {
        let gpu = Gpu::new().ok();
        if gpu.is_none() {
            eprintln!("no Metal device: the test checks nothing");
        }
        gpu
    }

    #[test]
    fn a_surface_takes_two_drawables_of_bgra_in_srgb() {
        let Some(gpu) = gpu() else { return };
        let layer = CAMetalLayer::new();
        let surface = Surface::with_layer(&gpu, layer.clone());
        assert_eq!(layer.pixelFormat(), MTLPixelFormat::BGRA8Unorm);
        assert!(layer.framebufferOnly());
        assert_eq!(layer.maximumDrawableCount(), DRAWABLES);
        let name = CGColorSpace::name(layer.colorspace().as_deref());
        // SAFETY: the name is Core Graphics' constant.
        assert_eq!(name.as_deref(), Some(unsafe { kCGColorSpaceSRGB }));

        surface.resize(1600, 1000, 2.0);
        assert_eq!(layer.drawableSize(), CGSize::new(1600.0, 1000.0));
        assert_eq!(layer.contentsScale(), 2.0);
    }

    #[test]
    fn a_frame_clears_and_completes() {
        let Some(gpu) = gpu() else { return };
        let surface = Surface::with_layer(&gpu, CAMetalLayer::new());
        surface.resize(64, 32, 1.0);
        let buffer = surface
            .encode_clear(Color::from_rgb8(30, 30, 32))
            .unwrap()
            .expect("a drawable comes");
        buffer.waitUntilCompleted();
        assert_eq!(buffer.status(), MTLCommandBufferStatus::Completed);
        assert!(buffer.error().is_none());
    }

    #[test]
    fn a_surface_of_no_size_draws_nothing() {
        let Some(gpu) = gpu() else { return };
        let surface = Surface::with_layer(&gpu, CAMetalLayer::new());
        surface.resize(0, 0, 2.0);
        assert!(!surface.clear(Color::default()).unwrap());
    }

    #[test]
    fn a_window_that_is_not_a_view_has_no_surface() {
        let Some(gpu) = gpu() else { return };
        let raw = RawWindowHandle::Web(WebWindowHandle::new(1));
        // SAFETY: the handle names no window, which `new` never reaches.
        let handle = unsafe { WindowHandle::borrow_raw(raw) };
        assert!(matches!(Surface::new(&gpu, handle), Err(Error::NoLayer)));
    }
}
