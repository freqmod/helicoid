pub mod animation_utils;
//pub mod cursor_renderer;
pub mod fonts;
//pub mod grid_renderer;
pub mod block_renderer;
pub mod profiler;
//mod rendered_window;
//mod text_box_renderer;
//mod text_renderer;

use helicoid_protocol::{caching_shaper::CachingShaper, gfx::FontPaint, text::ShapableString};

use ordered_float::OrderedFloat;
use skia_safe::{BlendMode, Canvas, Color, Paint, Point, Rect, Surface};
use winit::{event::Event, event_loop::ControlFlow};

use crate::{
    //bridge::EditorMode,
    editor::editor::HeliconeEditor,
    renderer::{block_renderer::font_paint_to_sk_paint, fonts::blob_builder::ShapedBlobBuilder},
};

//pub use grid_renderer::GridRenderer;
/*pub use rendered_window::{
    LineFragment, RenderedWindow, WindowDrawCommand, WindowDrawDetails, WindowPadding,
};*/
/*
#[derive(Clone, Debug)]
pub enum DrawCommand {
    CloseWindow(u64),
    Window {
        grid_id: u64,
        command: WindowDrawCommand,
    },
    FontChanged(String),
    DefaultStyleChanged(Style),
    //    ModeChanged(EditorMode),
}
*/
pub struct Renderer {
    editor: HeliconeEditor,
    //    text_box: RemoteBoxRenderer,
    //pub grid_renderer: GridRenderer,
    //    current_mode: EditorMode,

    //pub batched_draw_command_receiver: UnboundedReceiver<Vec<DrawCommand>>,
    //profiler: profiler::Profiler,
    os_scale_factor: f64,
    user_scale_factor: f64,
    //pub window_padding: WindowPadding,
}

impl Renderer {
    pub fn new(os_scale_factor: f64, editor: HeliconeEditor) -> Self {
        //      let window_settings = SETTINGS.get::<WindowSettings>();

        let user_scale_factor = 1.0; //window_settings.scale_factor.into();
        let _scale_factor = user_scale_factor * os_scale_factor;
        //let grid_renderer = GridRenderer::new(scale_factor);
        //let current_mode = EditorMode::Unknown(String::from(""));

        //        let batched_draw_command_receiver = EVENT_AGGREGATOR.register_event::<Vec<DrawCommand>>();
        //let profiler = profiler::Profiler::new(12.0);

        /*let window_padding = WindowPadding {
            top: window_settings.padding_top,
            left: window_settings.padding_left,
            right: window_settings.padding_right,
            bottom: window_settings.padding_bottom,
        };*/

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
    /*
        pub fn font_names(&self) -> Vec<String> {
            self.grid_renderer.font_names()
        }
    */
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

        /*
        let mut draw_commands = Vec::new();
        while let Ok(draw_command) = self.batched_draw_command_receiver.try_recv() {
            draw_commands.extend(draw_command);
        }

        let mut font_changed = false;

        for draw_command in draw_commands.into_iter() {
            if let DrawCommand::FontChanged(_) = draw_command {
                font_changed = true;
            }
            self.handle_draw_command(root_canvas, draw_command);
        }

        let default_background = self.grid_renderer.get_default_background();
        let font_dimensions = self.grid_renderer.font_dimensions;

        let transparency = { SETTINGS.get::<WindowSettings>().transparency };
        root_canvas.clear(default_background.with_a((255.0 * transparency) as u8));
        root_canvas.save();
        root_canvas.reset_matrix();

        let user_scale_factor = SETTINGS.get::<WindowSettings>().scale_factor.into();
        if user_scale_factor != self.user_scale_factor {
            self.user_scale_factor = user_scale_factor;
            self.grid_renderer
                .handle_scale_factor_update(self.os_scale_factor * self.user_scale_factor);
            font_changed = true;
        }

        if let Some(root_window) = self.rendered_windows.get(&1) {
            let clip_rect = root_window.pixel_region(font_dimensions);
            root_canvas.clip_rect(&clip_rect, None, Some(false));
        }

        let windows: Vec<&mut RenderedWindow> = {
            let (mut root_windows, mut floating_windows): (
                Vec<&mut RenderedWindow>,
                Vec<&mut RenderedWindow>,
            ) = self
                .rendered_windows
                .values_mut()
                .filter(|window| !window.hidden)
                .partition(|window| window.floating_order.is_none());

            root_windows
                .sort_by(|window_a, window_b| window_a.id.partial_cmp(&window_b.id).unwrap());

            floating_windows.sort_by(floating_sort);

            root_windows
                .into_iter()
                .chain(floating_windows.into_iter())
                .collect()
        };

        //let settings = SETTINGS.get::<RendererSettings>();
        self.window_regions = windows
            .into_iter()
            .map(|window| {
                if window.padding != self.window_padding {
                    window.padding = self.window_padding;
                }

                window.draw(
                    root_canvas,
                    //&settings,
                    default_background.with_a((255.0 * transparency) as u8),
                    font_dimensions,
                    dt,
                )
            })
            .collect();

        let windows = &self.rendered_windows;
        self.cursor_renderer
            .update_cursor_destination(font_dimensions.into(), windows);

        self.cursor_renderer
            .draw(&mut self.grid_renderer, &self.current_mode, dt);

        self.profiler.draw(root_canvas, dt);

        root_canvas.restore();

        font_changed

        */
        false
    }
    pub fn font_draw_test(&mut self, root_canvas: &mut Canvas) {
        let mut shaper = CachingShaper::new(1.0f32, 12.0f32);
        let mut blob_builder = ShapedBlobBuilder::new();
        //shaper.set_font_key(0, String::from("Anonymous Pro"));
        //shaper.set_font_key(1, String::from("NotoSansMono-Regular"));
        shaper.set_font_key(0, String::from("FiraCodeNerdFont-Regular"));
        shaper.set_font_key(2, String::from("NotoColorEmoji"));
        shaper.set_font_key(3, String::from("MissingGlyphs"));
        shaper.set_font_key(4, String::from("LastResort-Regular"));
        //blob_builder.set_font_key(0, String::from("Anonymous Pro"));
        //blob_builder.set_font_key(1, String::from("NotoSansMono-Regular"));
        blob_builder.set_font_key(0, String::from("FiraCodeNerdFont-Regular"));
        blob_builder.set_font_key(2, String::from("NotoColorEmoji"));
        blob_builder.set_font_key(3, String::from("MissingGlyphs"));
        blob_builder.set_font_key(4, String::from("LastResort-Regular"));
        let mut string_to_shape = ShapableString::from_text(
            "See if we can shape a simple local string ≠ <= string Some(tf) => { 😀🙀 What?",
        );
        let font_paint = FontPaint {
            color: 0xF000A030,
            blend: helicoid_protocol::gfx::SimpleBlendMode::SrcOver,
        };
        string_to_shape.metadata_runs.iter_mut().for_each(|i| {
            i.paint = font_paint.clone();
            i.font_info.font_parameters.size = OrderedFloat(80.0f32);
        });
        //shaper.cache_fonts(&string_to_shape, &None);
        let mut shaped = shaper.shape(&string_to_shape, &None);
        log::trace!("Shaped: {:?}", shaped);
        let blobs = blob_builder.bulid_blobs(&shaped);
        let mut x = 0f32;
        /* Move the line a bit down to not conflict with the same line from the server */
        let y = 200.0f32;

        let mut paint = Paint::default();
        //paint.set_blend_mode(BlendMode::SrcIn);
        paint.set_blend_mode(BlendMode::SrcATop);
        paint.set_anti_alias(true);

        log::trace!("Draw text: {:?}", blobs);
        for (blob, metadata_run) in blobs.iter().zip(shaped.metadata_runs.iter()) {
            let paint = font_paint_to_sk_paint(&metadata_run.paint);
            root_canvas.draw_text_blob(blob, (x as f32, y as f32), &paint);
        }
        let mut rect_paint = Paint::default();
        rect_paint.set_stroke_width(1.0);
        rect_paint.set_style(skia_safe::PaintStyle::Stroke);
        shaped
            .metadata_runs
            .get_mut(1)
            .map(|r| r.font_info.font_parameters.underlined = true);
        for metadata_run in shaped.metadata_runs.iter() {
            if metadata_run.font_info.font_parameters.underlined {
                root_canvas.draw_line(
                    Point {
                        x: x as f32,
                        y: y + metadata_run.baseline_y(),
                    },
                    Point {
                        x: x as f32 + metadata_run.advance_x(),
                        y: y + metadata_run.baseline_y(),
                    },
                    &rect_paint,
                );
            }
            root_canvas.draw_rect(
                Rect {
                    left: x,
                    top: y,
                    right: x + metadata_run.advance_x(),
                    bottom: y + metadata_run.advance_y(),
                },
                &rect_paint,
            );
            x += metadata_run.advance_x();
        }
    }
    pub fn handle_os_scale_factor_change(&mut self, os_scale_factor: f64) {
        self.os_scale_factor = os_scale_factor;
        //self.grid_renderer
        //    .handle_scale_factor_update(self.os_scale_factor * self.user_scale_factor);
    }
}
