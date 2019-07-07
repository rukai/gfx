//! Window creation using glutin for gfx.
//!
//! # Examples
//!
//! The following code creates a `gfx::Surface` using glutin.
//!
//! ```no_run
//! extern crate glutin;
//! extern crate gfx_backend_gl;
//!
//! fn main() {
//!     use gfx_backend_gl::Surface;
//!     use glutin::{ContextBuilder, WindowedContext};
//!     use glutin::window::WindowBuilder;
//!     use glutin::event_loop::EventLoop;
//!
//!     // First create a window using glutin.
//!     let mut events_loop = EventLoop::new();
//!     let wb = WindowBuilder::new();
//!     let glutin_window = ContextBuilder::new().with_vsync(true).build_windowed(wb, &events_loop).unwrap();
//!     let glutin_window = unsafe { glutin_window.make_current() }.expect("Failed to make the context current");
//!
//!     // Then use the glutin window to create a gfx surface.
//!     let surface = Surface::from_window(glutin_window);
//! }
//! ```
//!
//! Headless initialization without a window.
//!
//! ```no_run
//! extern crate glutin;
//! extern crate gfx_backend_gl;
//! extern crate gfx_hal;
//!
//! use gfx_hal::Instance;
//! use gfx_backend_gl::Headless;
//! use glutin::{Context, ContextBuilder};
//! use glutin::event_loop::EventLoop;
//!
//! fn main() {
//!     let events_loop = EventLoop::new();
//!     let context = ContextBuilder::new().build_headless(&events_loop, glutin::dpi::PhysicalSize::new(0.0, 0.0))
//!         .expect("Failed to build headless context");
//!     let context = unsafe { context.make_current() }.expect("Failed to make the context current");
//!     let headless = Headless::from_context(context);
//!     let _adapters = headless.enumerate_adapters();
//! }
//! ```

use crate::hal::window::Extent2D;
use crate::hal::{self, format as f, image, memory, CompositeAlpha};
use crate::{native, Backend as B, Device, GlContainer, PhysicalDevice, QueueFamily, Starc};

use glow::Context;

use glutin;

fn get_window_extent(window: &glutin::window::Window) -> image::Extent {
    let px = window
        .inner_size()
        .to_physical(window.hidpi_factor());
    image::Extent {
        width: px.width as image::Size,
        height: px.height as image::Size,
        depth: 1,
    }
}

#[derive(Debug)]
pub struct Swapchain {
    // Underlying window, required for presentation
    pub(crate) context: Starc<glutin::WindowedContext<glutin::PossiblyCurrent>>,
    // Extent because the window lies
    pub(crate) extent: Extent2D,
    ///
    pub(crate) fbos: Vec<native::FrameBuffer>,
}

impl hal::Swapchain<B> for Swapchain {
    unsafe fn acquire_image(
        &mut self,
        _timeout_ns: u64,
        _semaphore: Option<&native::Semaphore>,
        _fence: Option<&native::Fence>,
    ) -> Result<(hal::SwapImageIndex, Option<hal::window::Suboptimal>), hal::AcquireError> {
        // TODO: sync
        Ok((0, None))
    }
}

//TODO: if we make `Surface` a `WindowBuilder` instead of `WindowedContext`,
// we could spawn window + GL context when a swapchain is requested
// and actually respect the swapchain configuration provided by the user.
#[derive(Debug)]
pub struct Surface {
    pub(crate) context: Starc<glutin::WindowedContext<glutin::PossiblyCurrent>>,
}

impl Surface {
    pub fn from_window(context: glutin::WindowedContext<glutin::PossiblyCurrent>) -> Self {
        Surface {
            context: Starc::new(context),
        }
    }

    pub fn get_context(&self) -> &glutin::WindowedContext<glutin::PossiblyCurrent> {
        &*self.context
    }

    pub fn context(&self) -> &glutin::WindowedContext<glutin::PossiblyCurrent> {
        &self.context
    }

    fn swapchain_formats(&self) -> Vec<f::Format> {
        let pixel_format = self.context.get_pixel_format();
        let color_bits = pixel_format.color_bits;
        let alpha_bits = pixel_format.alpha_bits;
        let srgb = pixel_format.srgb;

        // TODO: expose more formats
        match (color_bits, alpha_bits, srgb) {
            (24, 8, true) => vec![f::Format::Rgba8Srgb, f::Format::Bgra8Srgb],
            (24, 8, false) => vec![f::Format::Rgba8Unorm, f::Format::Bgra8Unorm],
            _ => vec![],
        }
    }
}

impl hal::Surface<B> for Surface {
    fn kind(&self) -> hal::image::Kind {
        let ex = get_window_extent(&self.context.window());
        let samples = self.context.get_pixel_format().multisampling.unwrap_or(1);
        hal::image::Kind::D2(ex.width, ex.height, 1, samples as _)
    }

    fn compatibility(
        &self,
        _: &PhysicalDevice,
    ) -> (
        hal::SurfaceCapabilities,
        Option<Vec<f::Format>>,
        Vec<hal::PresentMode>,
    ) {
        let ex = get_window_extent(&self.context.window());
        let extent = hal::window::Extent2D::from(ex);

        let caps = hal::SurfaceCapabilities {
            image_count: if self.context.get_pixel_format().double_buffer {
                2 .. 3
            } else {
                1 .. 2
            },
            current_extent: Some(extent),
            extents: extent .. hal::window::Extent2D {
                width: ex.width + 1,
                height: ex.height + 1,
            },
            max_image_layers: 1,
            usage: image::Usage::COLOR_ATTACHMENT | image::Usage::TRANSFER_SRC,
            composite_alpha: CompositeAlpha::OPAQUE, //TODO
        };
        let present_modes = vec![
            hal::PresentMode::Fifo, //TODO
        ];

        (caps, Some(self.swapchain_formats()), present_modes)
    }

    fn supports_queue_family(&self, _: &QueueFamily) -> bool {
        true
    }
}

impl hal::Instance for Surface {
    type Backend = B;
    fn enumerate_adapters(&self) -> Vec<hal::Adapter<B>> {
        let adapter = PhysicalDevice::new_adapter(
            (),
            GlContainer::from_fn_proc(|s| self.context.get_proc_address(s) as *const _),
        );
        vec![adapter]
    }
}

pub fn config_context<C>(
    builder: glutin::ContextBuilder<C>,
    color_format: f::Format,
    ds_format: Option<f::Format>,
) -> glutin::ContextBuilder<C>
where
    C: glutin::ContextCurrentState,
{
    let color_base = color_format.base_format();
    let color_bits = color_base.0.describe_bits();
    let depth_bits = match ds_format {
        Some(fm) => fm.base_format().0.describe_bits(),
        None => f::BITS_ZERO,
    };
    builder
        .with_depth_buffer(depth_bits.depth)
        .with_stencil_buffer(depth_bits.stencil)
        .with_pixel_format(color_bits.color, color_bits.alpha)
        .with_srgb(color_base.1 == f::ChannelType::Srgb)
}

#[derive(Debug)]
pub struct Headless(pub Starc<glutin::Context<glutin::PossiblyCurrent>>);

impl Headless {
    pub fn from_context(context: glutin::Context<glutin::PossiblyCurrent>) -> Headless {
        Headless(Starc::new(context))
    }
}

impl hal::Instance for Headless {
    type Backend = B;
    fn enumerate_adapters(&self) -> Vec<hal::Adapter<B>> {
        let adapter = PhysicalDevice::new_adapter(
            (),
            GlContainer::from_fn_proc(|s| self.0.get_proc_address(s) as *const _),
        );
        vec![adapter]
    }
}
