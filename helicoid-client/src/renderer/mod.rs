pub mod animation_utils;
//pub mod cursor_renderer;
pub mod fonts;
//pub mod grid_renderer;
pub mod block_renderer;
pub mod profiler;
//mod rendered_window;
//mod text_box_renderer;
//mod text_renderer;

use skia_safe::{Color, Surface};
use winit::{event::Event, event_loop::ControlFlow};

use crate::editor::editor::HeliconeEditor;

pub struct Renderer {
    editor: HeliconeEditor,
    os_scale_factor: f64,
    #[allow(dead_code)]
    user_scale_factor: f64,
}

impl Renderer {
    pub fn new(os_scale_factor: f64, editor: HeliconeEditor) -> Self {
        //      let window_settings = SETTINGS.get::<WindowSettings>();

        let user_scale_factor = 1.0; //window_settings.scale_factor.into();
        let _scale_factor = user_scale_factor * os_scale_factor;

        Renderer {
            os_scale_factor,
            user_scale_factor,
            editor,
            //window_padding,
        }
    }
    pub fn poll_events(&mut self) {
        self.editor.poll_events();
    }

    pub fn handle_event(
        &mut self,
        event: &Event<()>,
        window: &winit::window::Window,
    ) -> Option<ControlFlow> {
        self.editor.handle_event(event, window)
    }
    /* Called after a potential draw, to sync resources etc */
    pub fn is_prune_cache_data_needed(&mut self) -> bool {
        return !self.editor.is_connected();
    }
    /// Draws frame
    ///
    /// # Returns
    /// `bool` indicating whether or not font was changed during this frame.
    #[allow(clippy::needless_collect)]
    pub fn draw_frame(&mut self, root_surface: &mut Surface, dt: f32) -> bool {
        let root_canvas = root_surface.canvas();
        root_canvas.draw_color(Color::BLACK, None);
        //self.font_draw_test(root_canvas);
        /* Draw editor contents*/
        self.editor.draw_frame(root_surface, dt);

        false
    }
    pub fn handle_os_scale_factor_change(&mut self, os_scale_factor: f64) {
        self.os_scale_factor = os_scale_factor;
    }
}
