use std::{
    convert::TryInto,
    ffi::{CStr, CString},
    num::NonZeroU32,
};

use crate::redraw_scheduler::REDRAW_SCHEDULER;
use gl::types::*;
use glutin::{
    self,
    config::{GetGlConfig, GlConfig},
    context::{AsRawContext, GlProfile, NotCurrentContext, PossiblyCurrentContext},
    display::{AsRawDisplay, Display, GetGlDisplay},
    prelude::{GlDisplay, GlSurface, NotCurrentGlContextSurfaceAccessor},
    surface::{Surface as GlutinSurface, SurfaceAttributesBuilder, WindowSurface},
};
use raw_window_handle::HasRawWindowHandle;
use skia_safe::{
    gpu::{gl::FramebufferInfo, BackendRenderTarget, DirectContext, SurfaceOrigin},
    Canvas, ColorType, Surface,
};
use winit::window::Window as WinitWindow;

//type WindowedContext = glutin::ContextWrapper<glutin::PossiblyCurrent, glutin::window::Window>;

fn create_surface(
    window: &mut WinitWindow,
    gr_context: &mut DirectContext,
    fb_info: FramebufferInfo,
    num_samples: u8,
    stencil_size: u8,
) -> Surface {
    //    let pixel_format = windowed_context.get_pixel_format();

    let size = window.inner_size();
    let size = (
        size.width.try_into().expect("Could not convert width"),
        size.height.try_into().expect("Could not convert height"),
    );
    let backend_render_target =
        BackendRenderTarget::new_gl(size, num_samples as usize, stencil_size as usize, fb_info);
    //windowed_context.resize(size.into());
    Surface::from_backend_render_target(
        gr_context,
        &backend_render_target,
        SurfaceOrigin::BottomLeft,
        ColorType::RGBA8888,
        None,
        None,
    )
    .expect("Could not create skia surface")
}
/* This must not outlive the GLContext & window it is created from.
If the GL context is lost it is suggested to reconnect to the editor
server and rentransfer any state. */
pub struct SkiaRenderer {
    gr_context: DirectContext,
    gl_context: PossiblyCurrentContext,
    fb_info: FramebufferInfo,
    gl_surface: GlutinSurface<WindowSurface>,
    surface: Surface,
    num_samples: u8,
    stencil_size: u8,
}

impl SkiaRenderer {
    pub fn new(window: &mut WinitWindow, not_current_context: NotCurrentContext) -> SkiaRenderer {
        let gl_config = not_current_context.config();
        let num_samples = gl_config.num_samples();
        let stencil_size = gl_config.stencil_size();
        let gl_display = not_current_context.display();
        /*        gl::load_with(|s| match gl_display {
            Display::Egl(ed) => ed.get_proc_address(s),
            Display::Glx(gd) => gd.get_proc_address(s),
        });*/
        gl::load_with(|s| gl_display.get_proc_address(CString::new(s).unwrap().as_c_str()));

        log::trace!("Skia make surface with size: {:?}", window.inner_size());
        let (width, height): (u32, u32) = window.inner_size().into();
        let raw_window_handle = window.raw_window_handle();
        let attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
            raw_window_handle,
            NonZeroU32::new(width).unwrap(),
            NonZeroU32::new(height).unwrap(),
        );
        let gl_surface = unsafe {
            gl_display
                .create_window_surface(&gl_config, &attrs)
                .unwrap()
        };

        let gl_context = not_current_context
            .make_current(&gl_surface)
            .expect("Could not make GL context current when setting up skia renderer");

        let interface = skia_safe::gpu::gl::Interface::new_load_with(|name| {
            if name == "eglGetCurrentDisplay" {
                return std::ptr::null();
            }
            gl_display.get_proc_address(CString::new(name).unwrap().as_c_str())
        })
        .expect("Could not create interface");

        let mut gr_context = skia_safe::gpu::DirectContext::new_gl(Some(interface), None)
            .expect("Could not create direct context");
        let fb_info = {
            let mut fboid: GLint = 0;
            unsafe { gl::GetIntegerv(gl::FRAMEBUFFER_BINDING, &mut fboid) };

            FramebufferInfo {
                fboid: fboid.try_into().expect("Could not create frame buffer id"),
                format: skia_safe::gpu::gl::Format::RGBA8.into(),
            }
        };
        let surface = create_surface(window, &mut gr_context, fb_info, num_samples, stencil_size);

        SkiaRenderer {
            gl_context,
            gr_context,
            surface,
            fb_info,
            gl_surface,
            num_samples,
            stencil_size,
        }
    }

    pub fn canvas(&mut self) -> &mut Canvas {
        self.surface.canvas()
    }
    pub fn surface(&mut self) -> &mut Surface {
        &mut self.surface
    }

    pub fn resize(&mut self, window: &mut WinitWindow) {
        let monitor_scale_factor =
        /* First resize the opengl drawable */
        if let Some(monitor) = window.current_monitor() {
//            monitor.scale_factor()
                1.0
        }else{
                1.0
            };

        log::trace!(
            "Skia make surface with new size: {:?} monitor scale:{}",
            window.inner_size(),
            monitor_scale_factor
        );
        let (width, height): (u32, u32) = window
            .inner_size()
            .to_logical::<u32>(monitor_scale_factor)
            .into();
        self.gl_surface.resize(
            &self.gl_context,
            NonZeroU32::new(width).unwrap(),
            NonZeroU32::new(height).unwrap(),
        );
        /* Then resize the skia surface */
        self.surface = create_surface(
            window,
            &mut self.gr_context,
            self.fb_info,
            self.num_samples,
            self.stencil_size,
        );

        //self.surface = create_surface(windowed_context, &mut self.gr_context, self.fb_info);
        REDRAW_SCHEDULER.queue_next_frame();
    }
    pub fn flush_and_swap_buffers(&mut self, window: &mut WinitWindow) {
        /* Tell skia to draw */
        //        window.request_redraw();
        self.gr_context.flush_and_submit();
        self.gl_surface.swap_buffers(&self.gl_context).unwrap();
    }
}
