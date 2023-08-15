//pub mod animation_utils;
//pub mod cursor_renderer;
//pub mod fonts;
//pub mod grid_renderer;
pub mod block_renderer;
pub mod fontconverter;
//pub mod profiler;
//mod rendered_window;
//mod text_box_renderer;
//mod text_renderer;

use std::any::Any;

use hashbrown::HashMap;
use helicoid_protocol::{gfx::PointU32, input::ViewportInfo};
use wgpu::{CommandEncoder, RenderPass, Surface};
use winit::{event_loop::ControlFlow, window::WindowId as winitWindowId};

type HelicoidWinitEvent<'a> = winit::event::Event<'a, ()>;

#[derive(Debug, Hash, Eq, Clone, PartialEq)]
struct ViewManagerId(usize);
#[derive(Debug, Hash, Eq, Clone, PartialEq)]
struct ManagedViewId(usize);
#[derive(Debug, Hash, Eq, Clone, PartialEq)]
struct WindowAreaId(usize);

struct WindowArea {
    location: PointU32,
    extent: PointU32,
}

/* Connected program, local program etc. */
pub struct ViewManager {
    views: Vec<ManagedViewId>,
    deligate: Box<dyn ManagerDelegate>,
}

pub struct RemoteMessage {}
trait ManagerDelegate: Any + Send {
    //    fn poll(&mut self);
    fn event_received(&mut self, event: &HelicoidWinitEvent);
}

trait RemoteManagedDelegate: Any + Send {
    fn message_received(&mut self, server_message: &RemoteMessage);
    fn connection_lost(&mut self);
}
/* Represents a connected client program */
pub struct ManagedView {
    view_info: ViewportInfo,
    view_manager: ViewManagerId,
}

/* Represents a window as shown by the window manager / OS */
pub struct Window {
    view_info: ViewportInfo,
    native_handle: winitWindowId,
}

/* Context with handles to the structs required to do actual rendering, like
device, window etc */
pub struct RenderContext<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
}

/* TODO: The actual contents, and lifetimes of this struct must be adjusted
when the struct is actually used so the requirements are determined */
pub struct RenderTargetContext<'a> {
    pub backend: RenderContext<'a>,
    pub encoder: &'a CommandEncoder,
    pub pass: &'a RenderPass<'a>,
    pub target: &'a Surface,
}
#[derive(Default)]
pub struct Renderer {
    #[allow(dead_code)]
    user_scale_factor: f64,
    manager: HashMap<ViewManagerId, ViewManager>,
    managed_views: HashMap<ManagedViewId, ManagedView>,
    window_areas: HashMap<WindowAreaId, WindowArea>,
}

impl Renderer {
    pub fn new() -> Self {
        let user_scale_factor = 1.0; //window_settings.scale_factor.into();
        Renderer {
            user_scale_factor,
            ..Default::default()
        }
    }

    pub fn handle_event(
        &mut self,
        event: &HelicoidWinitEvent,
        window: &winit::window::Window,
    ) -> Option<ControlFlow> {
        //self.editor.handle_event(event, window)
        Some(ControlFlow::Poll)
    }
    /* Called after a potential draw, to sync resources etc */
    pub fn is_prune_cache_data_needed(&mut self) -> bool {
        false
        // !self.editor.is_connected();
    }

    /// Draws frame
    ///
    /// # Returns
    /// `bool` indicating whether or not font was changed during this frame.
    /*
    #[allow(clippy::needless_collect)]
    pub fn draw_frame(&mut self, root_surface: &mut Surface, dt: f32) -> bool {
        let root_canvas = root_surface.canvas();
        root_canvas.draw_color(Color::BLACK, None);
        //self.font_draw_test(root_canvas);
        /* Draw editor contents*/
        self.editor.draw_frame(root_surface, dt);

        false
    }*/
    pub fn handle_os_scale_factor_change(&mut self, os_scale_factor: f64) {
        //sClientelf.os_scale_factor = os_scale_factor;
    }
}
