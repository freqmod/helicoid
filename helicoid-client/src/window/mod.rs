/* Based on neovide, Copyright Neovide contributors under BSD license */

//mod keyboard_manager;
//mod mouse_manager;
mod renderer;
mod settings;

//#[cfg(target_os = "macos")]
//mod draw_background;

use std::time::{Duration, Instant};

use glutin::{
    self,
    config::{GetGlConfig, GlConfig},
    context::NotCurrentContext,
    display::GetGlDisplay,
    prelude::GlDisplay,
};
use winit::{
    self,
    dpi::PhysicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop, EventLoopWindowTarget},
    window::{self, Icon},
};

use winit::window::{Window, WindowBuilder};

use glutin::config::ConfigTemplateBuilder;
use glutin::context::{ContextApi, ContextAttributesBuilder};

use glutin_winit::{self, DisplayBuilder};

use raw_window_handle::HasRawWindowHandle;

use image::{load_from_memory, GenericImageView, Pixel};
use renderer::SkiaRenderer;

use crate::{
    //bridge::{ParallelCommand, UiCommand},
    //cmd_line::CmdLineSettings,
    editor::editor::HeliconeEditor,
    redraw_scheduler::REDRAW_SCHEDULER,
    renderer::Renderer,
    HeliconeCommandLineArguments,
    //    running_tracker::*,
    /*    settings::{
        load_last_window_settings, save_window_geometry, PersistentWindowSettings, SETTINGS,
    },*/
};
pub use settings::{KeyboardSettings, WindowSettings};

static ICON: &[u8] = include_bytes!("../../../assets/icon.ico");

/*
#[derive(Clone, Debug)]
pub enum WindowCommand {
    TitleChanged(String),
    SetMouseEnabled(bool),
    ListAvailableFonts,
}*/
struct GlutinRunning {
    //    skia_renderer: ManuallyDrop<SkiaRenderer>,
    skia_renderer: SkiaRenderer,
    //config: GlutinConfig,
}
struct GlutinPaused {
    context: NotCurrentContext,
    //config: GlutinConfig,
}
enum GlutinWindowGl {
    Uninitialized,
    Paused(GlutinPaused),
    Running(GlutinRunning),
}

pub struct GlutinWindowWrapper {
    //windowed_context: WindowedContext<glutin::PossiblyCurrent>,
    //gl_window: GlWindow,
    //    surface: Surface<WindowSurface>,
    renderer: Renderer,
    //keyboard_manager: KeyboardManager,
    //mouse_manager: MouseManager,
    //title: String,
    //fullscreen: bool,
    font_changed_last_frame: bool,
    saved_inner_size: PhysicalSize<u32>,
    /*saved_grid_size: Option<Dimensions>,*/
    //size_at_startup: PhysicalSize<u32>,
    //maximized_at_startup: bool,
    //window_command_receiver: UnboundedReceiver<WindowCommand>,
    /* NB: Observer drop (i.e declaration) order */
    glutin_context: GlutinWindowGl,
    window: Window,
}

impl GlutinWindowWrapper {
    /*pub fn toggle_fullscreen(&mut self) {
            let window = &self.window;
            if self.fullscreen {
                window.set_fullscreen(None);
            } else {
                let handle = window.current_monitor();
                window.set_fullscreen(Some(Fullscreen::Borderless(handle)));
            }

            self.fullscreen = !self.fullscreen;
        }
    */
    pub fn synchronize_settings(&mut self) {
        /*let fullscreen = { SETTINGS.get::<WindowSettings>().fullscreen };

        if self.fullscreen != fullscreen {
            self.toggle_fullscreen();
        }*/
    }

    /*
        #[allow(clippy::needless_collect)]
        pub fn handle_window_commands(&mut self) {
            while let Ok(window_command) = self.window_command_receiver.try_recv() {
                match window_command {
                    WindowCommand::TitleChanged(new_title) => self.handle_title_changed(new_title),
                    WindowCommand::SetMouseEnabled(mouse_enabled) => {
                        //self.mouse_manager.enabled = mouse_enabled
                    }
                    WindowCommand::ListAvailableFonts => {} //self.send_font_names(),
                }
            }
        }
    */
    /*pub fn handle_title_changed(&mut self, new_title: String) {
        self.title = new_title;
        self.window.set_title(&self.title);
    }*/

    /*pub fn send_font_names(&self) {
        let font_names = self.renderer.font_names();
        EVENT_AGGREGATOR.send(UiCommand::Parallel(ParallelCommand::DisplayAvailableFonts(
            font_names,
        )));
    }*/

    /*pub fn handle_quit(&mut self) {
        if SETTINGS.get::<CmdLineSettings>().remote_tcp.is_none() {
            EVENT_AGGREGATOR.send(UiCommand::Parallel(ParallelCommand::Quit));
        } else {
            RUNNING_TRACKER.quit("window closed");
        }
    }*/

    /*pub fn handle_focus_lost(&mut self) {
        EVENT_AGGREGATOR.send(UiCommand::Parallel(ParallelCommand::FocusLost));
    }*/

    /*pub fn handle_focus_gained(&mut self) {
        EVENT_AGGREGATOR.send(UiCommand::Parallel(ParallelCommand::FocusGained));
        REDRAW_SCHEDULER.queue_next_frame();
    }*/

    fn finish_gl_initialization(&mut self, window_target: &EventLoopWindowTarget<()>) {
        log::trace!("Start finishing gl initialization");
        let mut local_glutin_context = GlutinWindowGl::Uninitialized;
        std::mem::swap(&mut self.glutin_context, &mut local_glutin_context);
        if let GlutinWindowGl::Paused(paused_context) = local_glutin_context {
            let not_current_context = paused_context.context;
            let gl_config = not_current_context.config();
            //            let window = self.window.take().unwrap_or_else(|| {
            let window_builder = WindowBuilder::new().with_transparent(true);
            glutin_winit::finalize_window(window_target, window_builder, &gl_config).unwrap();
            //          });

            //let gl_window = GlWindow::new(window, &gl_config);
            let skia_renderer = SkiaRenderer::new(&mut self.window, not_current_context);
            self.glutin_context = GlutinWindowGl::Running(GlutinRunning { skia_renderer });
        } else {
            /* Swap context back if nothing was changed */
            std::mem::swap(&mut self.glutin_context, &mut local_glutin_context);
        }
        log::trace!("End finishing gl initialization");
    }
    pub fn handle_event(
        &mut self,
        event: Event<()>,
        wt: &EventLoopWindowTarget<()>,
    ) -> Option<ControlFlow> {
        //log::info!("Got event: {:?}", event);
        /*self.keyboard_manager.handle_event(&event);
        self.mouse_manager.handle_event(
            &event,
            &self.keyboard_manager,
            &self.renderer,
            //&self.windowed_context,
        );*/
        if let Some(control_flow) = self.renderer.handle_event(&event, &self.window) {
            match control_flow {
                ControlFlow::ExitWithCode(_) => {
                    /* TODO: Something should probably be done before exit,
                    like notifying the server */
                    //self.handle_quit();
                    log::debug!("Handle quit")
                }
                _ => {}
            }
            return Some(control_flow);
        }
        match event {
            Event::LoopDestroyed => {
                //self.handle_quit();
            }
            Event::Resumed => {
                //EVENT_AGGREGATOR.send(EditorCommand::RedrawScreen);
                self.finish_gl_initialization(wt);
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                //self.handle_quit();
            }
            Event::WindowEvent {
                event: WindowEvent::ScaleFactorChanged { scale_factor, .. },
                ..
            } => {
                self.handle_scale_factor_update(scale_factor);
            }
            Event::WindowEvent {
                event: WindowEvent::DroppedFile(path),
                ..
            } => {
                let _file_path = path.into_os_string().into_string().unwrap();
                //EVENT_AGGREGATOR.send(UiCommand::Parallel(ParallelCommand::FileDrop(file_path)));
            }
            Event::WindowEvent {
                event: WindowEvent::Focused(_focus),
                ..
            } => {
                /*
                if focus {
                    self.handle_focus_gained();
                } else {
                    self.handle_focus_lost();
                }*/
            }
            Event::RedrawRequested(..) => {
                log::trace!("Window redraw requested");
                REDRAW_SCHEDULER.queue_next_frame()
            }
            _ => {}
        }
        return None;
    }

    pub fn draw_frame(&mut self, dt: f32) {
        let window = &self.window;
        let new_size = window.inner_size();

        //self.skia_renderer.render(dt);
        /*let window_settings = SETTINGS.get::<WindowSettings>();
                let window_padding = WindowPadding {
                    top: window_settings.padding_top,
                    left: window_settings.padding_left,
                    right: window_settings.padding_right,
                    bottom: window_settings.padding_bottom,
                };

                let padding_changed = window_padding != self.renderer.window_padding;
                if padding_changed {
                    self.renderer.window_padding = window_padding;
                }
        */
        if self.saved_inner_size != new_size {
            //|| self.font_changed_last_frame || padding_changed {
            //self.font_changed_last_frame = false;
            self.saved_inner_size = new_size;

            //self.handle_new_grid_size(new_size);
            if let GlutinWindowGl::Running(gl_run) = &mut self.glutin_context {
                gl_run.skia_renderer.resize(&mut self.window);
            }
        }

        self.renderer.poll_events();
        if REDRAW_SCHEDULER.should_draw() {
            //|| SETTINGS.get::<WindowSettings>().no_idle {
            if let GlutinWindowGl::Running(gl_run) = &mut self.glutin_context {
                self.font_changed_last_frame =
                    self.renderer.draw_frame(gl_run.skia_renderer.surface(), dt);
                gl_run
                    .skia_renderer
                    .flush_and_swap_buffers(&mut self.window);
            }
        }
        if self.renderer.is_prune_cache_data_needed() {
            if let GlutinWindowGl::Running(gl_run) = &mut self.glutin_context {
                gl_run.skia_renderer.prune_cache_data();
            }
        }

        /*
        // Wait until fonts are loaded, so we can set proper window size.
        if !self.renderer.grid_renderer.is_ready {
            return;
        }

        let settings = SETTINGS.get::<CmdLineSettings>();
        // Resize at startup happens when window is maximized or when using tiling WM
        // which already resized window.
        let resized_at_startup = self.maximized_at_startup || self.has_been_resized();

        log::trace!(
            "Settings geometry {:?}",
            PhysicalSize::from(settings.geometry)
        );
        log::trace!("Inner size: {:?}", new_size);

        if self.saved_grid_size.is_none() && !resized_at_startup {
            let window = self.windowed_context.window();
            window.set_inner_size(
                self.renderer
                    .grid_renderer
                    .convert_grid_to_physical(settings.geometry),
            );
            self.saved_grid_size = Some(settings.geometry);
            // Font change at startup is ignored, so grid size (and startup screen) could be preserved.
            // But only when not resized yet. With maximized or resized window we should redraw grid.
            self.font_changed_last_frame = false;
        }
        */
    }
    /*
        fn handle_new_grid_size(&mut self, new_size: PhysicalSize<u32>) {
            let window_padding = self.renderer.window_padding;
            let window_padding_width = window_padding.left + window_padding.right;
            let window_padding_height = window_padding.top + window_padding.bottom;

            let content_size = PhysicalSize {
                width: new_size.width - window_padding_width,
                height: new_size.height - window_padding_height,
            };

            let grid_size = self
                .renderer
                .grid_renderer
                .convert_physical_to_grid(content_size);

            // Have a minimum size
            if grid_size.width < MIN_WINDOW_WIDTH || grid_size.height < MIN_WINDOW_HEIGHT {
                return;
            }

            if self.saved_grid_size == Some(grid_size) {
                trace!("Grid matched saved size, skip update.");
                return;
            }
            self.saved_grid_size = Some(grid_size);
            EVENT_AGGREGATOR.send(UiCommand::Parallel(ParallelCommand::Resize {
                width: grid_size.width,
                height: grid_size.height,
            }));
        }
    */
    fn handle_scale_factor_update(&mut self, scale_factor: f64) {
        self.renderer.handle_os_scale_factor_change(scale_factor);
        //EVENT_AGGREGATOR.send(EditorCommand::RedrawScreen);
    }

    /*fn has_been_resized(&self) -> bool {
        true
        //self.windowed_context.window().inner_size() != self.size_at_startup
    }*/
}

/*
Create a window with a gl context for rendering on it. This function is to
separate winit & glutin details from create window function */
fn create_window_with_gl_context(
    event_loop: &EventLoop<()>,
    icon: Icon,
    maximized: bool,
) -> (Window, NotCurrentContext) {
    //let mut previous_position = None;
    /* TODO: Android does not support using a window builder, so as long as a
    window builder is made by default android is not supported */
    let winit_window_builder = window::WindowBuilder::new()
        .with_title("Helicoid")
        .with_window_icon(Some(icon))
        .with_maximized(maximized)
        .with_transparent(true);

    let _frame_decoration = true; //cmd_line_settings.frame;

    // There is only two options for windows & linux, no need to match more options.
    #[cfg(not(target_os = "macos"))]
    let mut winit_window_builder = winit_window_builder.with_decorations(frame_decoration); // == Frame::Full);

    let template = ConfigTemplateBuilder::new()
        .with_alpha_size(8)
        .with_transparency(true);

    let display_builder = DisplayBuilder::new().with_window_builder(Some(winit_window_builder));
    let (window, gl_config) = display_builder
        .build(event_loop, template, |configs| {
            // Find the config with the maximum number of samples, so our triangle will
            // be smooth.
            configs
                .reduce(|accum, config| {
                    let transparency_check = config.supports_transparency().unwrap_or(false)
                        & !accum.supports_transparency().unwrap_or(false);

                    if transparency_check || config.num_samples() > accum.num_samples() {
                        config
                    } else {
                        accum
                    }
                })
                .unwrap()
        })
        .unwrap();
    log::trace!("Picked a config with {} samples", gl_config.num_samples());

    let raw_window_handle = window.as_ref().map(|window| window.raw_window_handle());

    // XXX The display could be obtained from the any object created by it, so we
    // can query it from the config.
    let gl_display = gl_config.display();

    // The context creation part. It can be created before surface and that's how
    // it's expected in multithreaded + multiwindow operation mode, since you
    // can send NotCurrentContext, but not Surface.
    let context_attributes = ContextAttributesBuilder::new().build(raw_window_handle);

    // Since glutin by default tries to create OpenGL core context, which may not be
    // present we should try gles.
    let fallback_context_attributes = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::Gles(None))
        .build(raw_window_handle);
    let not_current_gl_context = unsafe {
        gl_display
            .create_context(&gl_config, &context_attributes)
            .unwrap_or_else(|_| {
                gl_display
                    .create_context(&gl_config, &fallback_context_attributes)
                    .expect("failed to create context")
            })
    };

    (window.unwrap(), not_current_gl_context)
}
pub fn create_window(args: &HeliconeCommandLineArguments) {
    let icon = {
        let icon = load_from_memory(ICON).expect("Failed to parse icon data");
        let (width, height) = icon.dimensions();
        let mut rgba = Vec::with_capacity((width * height) as usize * 4);
        for (_, _, pixel) in icon.pixels() {
            rgba.extend_from_slice(&pixel.to_rgba().0);
        }
        Icon::from_rgba(rgba, width, height).expect("Failed to create icon object")
    };

    let event_loop = EventLoop::new();

    /*    let cmd_line_settings = SETTINGS.get::<CmdLineSettings>();

        let mut maximized = cmd_line_settings.maximized;
        let mut previous_position = None;
        if let Ok(last_window_settings) = load_last_window_settings() {
            match last_window_settings {
                PersistentWindowSettings::Maximized => {
                    maximized = true;
                }
                PersistentWindowSettings::Windowed { position, .. } => {
                    previous_position = Some(position);
                }
            }
        }
    */
    let maximized = false;

    /*
        #[cfg(target_os = "macos")]
        let mut winit_window_builder = match frame_decoration {
            Frame::Full => winit_window_builder,
            Frame::None => winit_window_builder.with_decorations(false),
            Frame::Buttonless => winit_window_builder
                .with_transparent(true)
                .with_title_hidden(true)
                .with_titlebar_buttons_hidden(true)
                .with_titlebar_transparent(true)
                .with_fullsize_content_view(true),
            Frame::Transparent => winit_window_builder
                .with_title_hidden(true)
                .with_titlebar_transparent(true)
                .with_fullsize_content_view(true),
        };
    */

    /*if let Some(previous_position) = previous_position {
        if !maximized {
            winit_window_builder = winit_window_builder.with_position(previous_position);
        }
    }*/

    //    #[cfg(target_os = "linux")]
    //    let winit_window_builder = winit_window_builder;
    /*        .with_app_id(cmd_line_settings.wayland_app_id)
    .with_class(
        cmd_line_settings.x11_wm_class_instance,
        cmd_line_settings.x11_wm_class,
    );*/

    //    #[cfg(target_os = "macos")]
    //    let winit_window_builder = winit_window_builder.with_accepts_first_mouse(false);

    /*
        let builder = ContextBuilder::new()
            .with_pixel_format(24, 8)
            .with_stencil_buffer(8)
            .with_gl_profile(GlProfile::Core);
        //.with_srgb(cmd_line_settings.srgb)
        //.with_vsync(cmd_line_settings.vsync);

        let windowed_context = match builder
            .clone()
            .build_windowed(winit_window_builder.clone(), &event_loop)
        {
            Ok(ctx) => ctx,
            Err(err) => {
                // haven't found any sane way to actually match on the pattern rabbithole CreationError
                // provides, so here goes nothing
                if err.to_string().contains("vsync") {
                    builder
                        .with_vsync(false)
                        .build_windowed(winit_window_builder, &event_loop)
                        .unwrap()
                } else {
                    panic!("{}", err);
                }
            }
        };
        let windowed_context = unsafe { windowed_context.make_current().unwrap() };

        let window = windowed_context.window();
    */
    let (window, gl_context) = create_window_with_gl_context(&event_loop, icon, maximized);
    //let initial_size = window.inner_size();

    let gl_paused = GlutinPaused {
        context: gl_context,
    };
    //let raw_window_handle = window.raw_window_handle();

    // Check that window is visible in some monitor, and reposition it if not.
    let did_reposition = window
        .current_monitor()
        .and_then(|current_monitor| {
            let monitor_position = current_monitor.position();
            let monitor_size = current_monitor.size();
            let monitor_width = monitor_size.width as i32;
            let monitor_height = monitor_size.height as i32;

            let window_position = window.outer_position().ok()?;
            let window_size = window.outer_size();
            let window_width = window_size.width as i32;
            let window_height = window_size.height as i32;

            if window_position.x + window_width < monitor_position.x
                || window_position.y + window_height < monitor_position.y
                || window_position.x > monitor_position.x + monitor_width
                || window_position.y > monitor_position.y + monitor_height
            {
                window.set_outer_position(monitor_position);
            }

            Some(())
        })
        .is_some();

    log::trace!("repositioned window: {}", did_reposition);

    let editor = HeliconeEditor::new(args);
    let scale_factor = window.scale_factor();
    let renderer = Renderer::new(scale_factor, editor);
    let saved_inner_size = window.inner_size();

    //let skia_renderer = SkiaRenderer::new(&windowed_context);

    //let window_command_receiver = EVENT_AGGREGATOR.register_event::<WindowCommand>();

    log::info!(
        "window created (scale_factor: {:.4}, font_dimensions: {:?})",
        scale_factor,
        "",
        //        renderer.grid_renderer.font_dimensions,
    );

    let mut window_wrapper = GlutinWindowWrapper {
        //        windowed_context,
        window,
        renderer,
        //title: String::from("Helicoid"),
        //fullscreen: false,
        font_changed_last_frame: false,
        //size_at_startup: initial_size,
        //maximized_at_startup: maximized,
        saved_inner_size,
        //        saved_grid_size: None,
        //window_command_receiver,
        glutin_context: GlutinWindowGl::Paused(gl_paused),
    };

    let mut previous_frame_start = Instant::now();

    enum FocusedState {
        Focused,
        UnfocusedNotDrawn,
        Unfocused,
    }
    let mut focused = FocusedState::Focused;

    event_loop.run(move |e, window_target, control_flow| {
        // Window focus changed
        match e {
            Event::WindowEvent {
                event: WindowEvent::Focused(focused_event),
                ..
            } => {
                focused = if focused_event {
                    FocusedState::Focused
                } else {
                    FocusedState::UnfocusedNotDrawn
                };
            }
            /*            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
                return;
            }*/
            _ => {}
        }
        /*
                if !RUNNING_TRACKER.is_running() {
                    let window = window_wrapper.windowed_context.window();
                    save_window_geometry(
                        window.is_maximized(),
                        window_wrapper.saved_grid_size,
                        window.outer_position().ok(),
                    );

                    std::process::exit(RUNNING_TRACKER.exit_code());
                }
        */
        let frame_start = Instant::now();

        //window_wrapper.handle_window_commands();
        window_wrapper.synchronize_settings();
        let ctrl: Option<ControlFlow> = window_wrapper.handle_event(e, window_target);
        if let Some(ctrl) = ctrl {
            match ctrl {
                ControlFlow::Exit | ControlFlow::ExitWithCode(_) => {
                    *control_flow = ctrl;
                    return;
                }
                _ => {}
            }
        }

        let refresh_rate = match focused {
            FocusedState::Focused | FocusedState::UnfocusedNotDrawn => {
                60f32 //SETTINGS.get::<WindowSettings>().refresh_rate as f32
            }
            FocusedState::Unfocused => 10f32, //SETTINGS.get::<WindowSettings>().refresh_rate_idle as f32,
        }
        .max(1.0);

        let expected_frame_length_seconds = 1.0 / refresh_rate;
        let frame_duration = Duration::from_secs_f32(expected_frame_length_seconds);

        if frame_start - previous_frame_start > frame_duration {
            //            log::trace!("Evtloop refresh");
            let dt = previous_frame_start.elapsed().as_secs_f32();
            window_wrapper.draw_frame(dt);
            if let FocusedState::UnfocusedNotDrawn = focused {
                focused = FocusedState::Unfocused;
            }
            previous_frame_start = frame_start;
            //            #[cfg(target_os = "macos")]
            //            draw_background(&window_wrapper.windowed_context);
        }

        *control_flow = ControlFlow::WaitUntil(previous_frame_start + frame_duration)
    });
}
