use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use crate::{
    redraw_scheduler::REDRAW_SCHEDULER,
    renderer::block_renderer::{SkiaClientRenderBlock, SkiaClientRenderTarget, SkiaGfxManager},
    HeliconeCommandLineArguments,
};
use helicoid_protocol::{
    block_manager::Manager,
    gfx::{PointF16, RenderBlockDescription, RenderBlockId, RenderBlockLocation},
    input::{
        ComplexKeyEvent, HelicoidToServerMessage, KeyModifierStateUpdateEvent, ViewportInfo,
        VirtualKeycode,
    },
    tcp_bridge::{ClientTcpBridge, TcpBridgeToClientMessage, TcpBridgeToServerMessage},
};
use ordered_float::OrderedFloat;
use skia_safe::{Canvas, Image, Surface};
use tokio::sync::{
    mpsc::{Receiver, Sender},
    Mutex as TMutex,
};
use winit::{
    event::{Event, WindowEvent},
    event_loop::ControlFlow,
};

struct HeliconeEditorInner {
    //bridge: ClientTcpBridge,
    sender: Option<Sender<TcpBridgeToServerMessage>>,
    receiver: Option<Receiver<TcpBridgeToClientMessage>>,
    scale_factor: f64,
    time_ref_base: Instant,
}
pub struct HeliconeEditor {
    inner: Arc<TMutex<Option<HeliconeEditorInner>>>,
    sender: Option<Sender<TcpBridgeToServerMessage>>,
    receiver: Option<Receiver<TcpBridgeToClientMessage>>,
    server_address: Option<String>,
    current_viewport_info: Option<ViewportInfo>,
    renderer: Manager<SkiaClientRenderBlock>,
    graphics_manager: SkiaGfxManager,
}
impl HeliconeEditor {
    pub fn new(args: &HeliconeCommandLineArguments) -> Self {
        let inner: Arc<TMutex<Option<HeliconeEditorInner>>> = Arc::new(TMutex::new(None));
        if let Some(server_address) = &args.server_address {
            //            let bridge = ClientTcpBridge::
            let inner_async = inner.clone();
            let addr_async = server_address.clone();
            Self::try_connect(inner_async, addr_async);
        } else {
            panic!("Integrated helicone editor is not supported (yet)");
        }
        Self {
            inner,
            renderer: Manager::new(),
            graphics_manager: SkiaGfxManager::new(),
            sender: None,
            receiver: None,
            server_address: args.server_address.clone(),
            current_viewport_info: None,
        }
    }
    fn try_connect(inner: Arc<TMutex<Option<HeliconeEditorInner>>>, addr: String) {
        let _ = tokio::spawn(async move {
            loop {
                match ClientTcpBridge::connect(&addr).await {
                    Ok((mut bridge, sender, receiver)) => {
                        {
                            let mut inner_locked = inner.lock().await;
                            *inner_locked = Some(HeliconeEditorInner {
                                sender: Some(sender),
                                receiver: Some(receiver),
                                scale_factor: 1.0,
                                time_ref_base: Instant::now(),
                            });
                        }
                        match bridge.process_rxtx().await {
                            Ok(_) => {}
                            Err(e) => {
                                log::warn!("Error during client bridge processing: {:?}", e);
                            }
                        }
                        break;
                    }
                    Err(e) => {
                        /* Try to (re)connect to the sever every 10'th second if it fails */
                        log::warn!(
                            "Error while connecting to editor-server at: {:?} {:?}",
                            addr,
                            e
                        );
                        tokio::time::sleep(Duration::from_secs(10)).await;
                        continue;
                    }
                }
            }
        });
    }
    fn ensure_connected(&mut self) -> bool {
        if self.sender.is_some() && self.receiver.is_some() {
            if self.sender.as_ref().unwrap().is_closed() {
                self.sender = None;
                self.receiver = None;
            } else {
                return true;
            }
        }
        let result;
        if let Some(mut inner_opt) = self.inner.try_lock().ok() {
            if let Some(inner) = &mut *inner_opt {
                if inner.sender.is_none() && inner.receiver.is_none() {
                    /* An empty shell inner means that the contents have been moved out before,
                    if this if still is entered it is because the connection has been lost, and
                    a reconnection should be initiated. The inner struct is set to None to signal
                    that connection establishement is in progress (and avoid multiple concurrent
                    connection establishment functions). */
                    log::trace!("Initalize reconnect");
                    let _ = inner_opt.take();
                    Self::try_connect(self.inner.clone(), self.server_address.clone().unwrap());
                    return false;
                }
                log::trace!("Extract connection channels");
                if let Some(sender) = inner.sender.take() {
                    self.sender = Some(sender);
                }
                if let Some(receiver) = inner.receiver.take() {
                    self.receiver = Some(receiver);
                }
                if self.sender.is_some() && self.receiver.is_some() {
                    result = true;
                } else {
                    result = false;
                }
            } else {
                result = false;
            }
        } else {
            result = false;
        }
        if result {
            self.post_connect();
        }
        result
    }
    pub fn post_connect(&mut self) {
        log::trace!("Post connect: WP: {}", self.current_viewport_info.is_some());
        /* Send information about size to the server */
        if let Some(viewport_info) = self.current_viewport_info.as_ref() {
            let size_msg = TcpBridgeToServerMessage {
                message: HelicoidToServerMessage::ViewportSizeUpdate(viewport_info.clone()),
            };
            let _ = self
                .sender
                .as_mut()
                .unwrap()
                .blocking_send(size_msg)
                .map_err(|e| {
                    log::warn!(
                        "Error while sending intitial viewport update to server: {:?}",
                        e
                    )
                });
            log::trace!("Sent viewport info");
        }
    }
    fn now_timestamp(inner: &HeliconeEditorInner) -> u32 {
        (Instant::now()
            .saturating_duration_since(inner.time_ref_base)
            .as_millis()
            % (u32::MAX as u128)) as u32
    }
    pub fn handle_event_disconnected(&mut self, event: &Event<()>) -> Option<ControlFlow> {
        match event {
            Event::WindowEvent { window_id, event } => match event {
                WindowEvent::CloseRequested => {
                    return Some(ControlFlow::Exit);
                }
                WindowEvent::Resized(event) => {
                    let scale_factor = if let Some(inner) = self.inner.try_lock().ok() {
                        inner.as_ref().map(|i| i.scale_factor).unwrap_or(1.0)
                    } else {
                        1.0
                    };
                    let size = ViewportInfo {
                        physical_size: (event.width, event.height),
                        scale_factor: OrderedFloat(scale_factor as f32),
                        container_physical_size: None,
                        container_scale_factor: None,
                    };
                    self.current_viewport_info = Some(size.clone());
                }
                _ => {}
            },

            _ => {}
        }
        None
    }
    pub fn poll_events(&mut self) {
        if !self.ensure_connected() {
            return;
        }
        self.peek_and_process_events();
    }

    fn forward_keyboard_modifier_changed(
        &self,
        inner: &mut HeliconeEditorInner,
        modifier_state: &winit::event::ModifiersState,
    ) {
        let key_modifier_update_event = KeyModifierStateUpdateEvent {
            timestamp: Self::now_timestamp(inner),
            lshift_pressed: modifier_state.shift(),
            lctrl_pressed: modifier_state.ctrl(),
            lalt_pressed: modifier_state.alt(),
            llogo_pressed: modifier_state.logo(),
            caps_pressed: modifier_state.caps(),
            rshift_pressed: false,
            rctrl_pressed: false,
            ralt_pressed: false,
            rlogo_pressed: false,
            bits: modifier_state.bits(),
        };
        let size_msg = TcpBridgeToServerMessage {
            message: HelicoidToServerMessage::KeyModifierStateUpdate(key_modifier_update_event),
        };
        let _ = self
            .sender
            .as_ref()
            .unwrap()
            .blocking_send(size_msg)
            .map_err(|e| log::warn!("Error while sending key code update to server: {:?}", e));
    }

    fn forward_keyboard_input(
        &self,
        inner: &mut HeliconeEditorInner,
        _device_id: &winit::event::DeviceId,
        input: &winit::event::KeyboardInput,
        is_synthetic: &bool,
    ) {
        let complex_key_event = ComplexKeyEvent {
            key_code: input.scancode,
            timestamp: Self::now_timestamp(inner),
            virtual_keycode: convert_virtual_keycodes(input.virtual_keycode),
            pressed: input.state == winit::event::ElementState::Pressed,
            synthetic: *is_synthetic,
        };
        let size_msg = TcpBridgeToServerMessage {
            message: HelicoidToServerMessage::KeyInputEvent(complex_key_event),
        };
        let _ = self
            .sender
            .as_ref()
            .unwrap()
            .blocking_send(size_msg)
            .map_err(|e| log::warn!("Error while sending key code update to server: {:?}", e));
    }

    fn send_size_info(
        sender: &mut Sender<TcpBridgeToServerMessage>,
        current_viewport_info_out: &mut Option<ViewportInfo>,
        physical_size: (u32, u32),
        scale_factor: f64,
    ) {
        /* Convert event to helicoid protocol and send off  to server. */
        let size = ViewportInfo {
            physical_size,
            scale_factor: OrderedFloat(scale_factor as f32),
            container_physical_size: None,
            container_scale_factor: None,
        };
        *current_viewport_info_out = Some(size.clone());
        let size_msg = TcpBridgeToServerMessage {
            message: HelicoidToServerMessage::ViewportSizeUpdate(size),
        };
        let _ = sender.blocking_send(size_msg).map_err(|e| {
            log::warn!(
                "Error while sending intitial viewport update to server: {:?}",
                e
            )
        });
        log::trace!("Resize sent viewport info");
    }
    pub fn handle_event(
        &mut self,
        event: &Event<()>,
        window: &winit::window::Window,
    ) -> Option<ControlFlow> {
        if !self.ensure_connected() {
            /*log::trace!(
                "Try to handle event before connection is established to server: {:?}",
                event
            );*/
            return self.handle_event_disconnected(event);
        }
        if let Some(mut inner) = self.inner.try_lock().ok() {
            match event {
                Event::MainEventsCleared
                | Event::RedrawEventsCleared
                | Event::NewEvents(winit::event::StartCause::ResumeTimeReached { .. })
                | Event::NewEvents(winit::event::StartCause::WaitCancelled { .. })
                | Event::DeviceEvent {
                    device_id: _,
                    event: winit::event::DeviceEvent::Motion { .. },
                }
                | Event::DeviceEvent {
                    device_id: _,
                    event: winit::event::DeviceEvent::MouseMotion { .. },
                } => {}
                _ => {
                    log::trace!("Got winit event: {:?}", event);
                }
            }
            match event {
                Event::NewEvents(_) => {}
                Event::WindowEvent { window_id, event } => match event {
                    WindowEvent::Resized(event) => {
                        //let scale_factor = inner.as_ref().map(|i| i.scale_factor).unwrap_or(1.0);
                        let scale_factor = if let Some(monitor) = window.current_monitor() {
                            monitor.scale_factor()
                        } else {
                            1.0
                        };
                        let logical_size = event.to_logical::<u32>(scale_factor);
                        log::trace!(
                            "Window resize: {:?} Logical size: {:?} ({})",
                            event,
                            logical_size,
                            scale_factor,
                        );
                        let physical_size = (event.width, event.height);
                        Self::send_size_info(
                            self.sender.as_mut().unwrap(),
                            &mut self.current_viewport_info,
                            physical_size,
                            scale_factor,
                        );
                    }
                    WindowEvent::Moved(_) => {}
                    WindowEvent::CloseRequested => {
                        return Some(ControlFlow::Exit);
                    }
                    WindowEvent::Destroyed => {}
                    WindowEvent::DroppedFile(_) => {}
                    WindowEvent::HoveredFile(_) => {}
                    WindowEvent::HoveredFileCancelled => {}
                    WindowEvent::ModifiersChanged(modifiers_changed) => {
                        self.forward_keyboard_modifier_changed(
                            inner.as_mut().unwrap(),
                            modifiers_changed,
                        );
                    }
                    WindowEvent::CursorMoved {
                        device_id,
                        position,
                        modifiers,
                    } => {}
                    WindowEvent::CursorEntered { device_id } => {}
                    WindowEvent::CursorLeft { device_id } => {}
                    WindowEvent::MouseWheel {
                        device_id,
                        delta,
                        phase,
                        modifiers,
                    } => {}
                    WindowEvent::MouseInput {
                        device_id,
                        state,
                        button,
                        modifiers,
                    } => {}
                    WindowEvent::TouchpadPressure {
                        device_id,
                        pressure,
                        stage,
                    } => {}
                    WindowEvent::AxisMotion {
                        device_id,
                        axis,
                        value,
                    } => {}
                    WindowEvent::Touch(_) => {}
                    WindowEvent::ScaleFactorChanged {
                        scale_factor,
                        new_inner_size,
                    } => {
                        log::trace!("Scale factor change: {} {:?}", scale_factor, new_inner_size);
                        if let Some(inner) = inner.as_mut() {
                            inner.scale_factor = *scale_factor;
                        }
                        let physical_size = (new_inner_size.width, new_inner_size.height);
                        Self::send_size_info(
                            self.sender.as_mut().unwrap(),
                            &mut self.current_viewport_info,
                            physical_size,
                            *scale_factor,
                        );
                    }
                    WindowEvent::ThemeChanged(_) => {}
                    WindowEvent::ReceivedCharacter(_) => {}
                    WindowEvent::Focused(_) => {}
                    WindowEvent::KeyboardInput {
                        device_id,
                        input,
                        is_synthetic,
                    } => {
                        self.forward_keyboard_input(
                            inner.as_mut().unwrap(),
                            device_id,
                            input,
                            is_synthetic,
                        );
                    }
                    WindowEvent::Ime(_) => {}
                    WindowEvent::Occluded(_) => {}
                    WindowEvent::TouchpadMagnify {
                        device_id,
                        delta,
                        phase,
                    } => {}
                    WindowEvent::SmartMagnify { device_id } => todo!(),
                    WindowEvent::TouchpadRotate {
                        device_id,
                        delta,
                        phase,
                    } => {}
                },
                Event::DeviceEvent { device_id, event } => {}
                Event::UserEvent(_) => {}
                Event::Suspended => {}
                Event::Resumed => {}
                Event::MainEventsCleared => {}
                Event::RedrawRequested(_) => {}
                Event::RedrawEventsCleared => {}
                Event::LoopDestroyed => {}
            }
        }
        return None;
    }
    fn reconnect_bridge(&mut self) {
        /* Set editor in disconnected state (i.e. render appropriate graphics) and try to reconnect regularly */
        self.receiver = None;
        /* Send message to bridge (via sender) */
        self.sender = None;
        unimplemented!();
        /* Spawn an async closure that disconncts the old bridge and sets up a new one to (try to) connect */
    }
    /// Check the receiver channel if any events are received from the editor server.
    fn peek_and_process_events(&mut self) {
        let mut disconnected = false;
        if let Some(receiver) = self.receiver.as_mut() {
            loop {
                match receiver.try_recv() {
                    Ok(event) => {
                        log::trace!("Got event from server: {:?}", event);
                        let update = &event.message.update;
                        //todo!("Instantiate GFX manager, and call handle block update")
                        self.renderer.handle_block_update(
                            RenderBlockId::normal(0).unwrap(),
                            update,
                            &mut self.graphics_manager,
                        );
                        REDRAW_SCHEDULER.queue_next_frame();
                    }
                    Err(e) => match e {
                        tokio::sync::mpsc::error::TryRecvError::Empty => {
                            //log::trace!("POPevt empty");
                            break;
                        }
                        tokio::sync::mpsc::error::TryRecvError::Disconnected => {
                            disconnected = true;
                            break;
                        }
                    },
                }
            }
        }
        if disconnected {
            self.reconnect_bridge()
        }
    }
    pub fn draw_frame(&mut self, root_surface: &mut Surface, dt: f32) -> bool {
        if !self.ensure_connected() {
            log::warn!("Try to draw frame before connection is established to server");
            return false;
        }
        log::trace!("Editor: got request to draw frame");
        self.peek_and_process_events();
        let location = RenderBlockLocation {
            id: RenderBlockId::normal(0).unwrap(),
            location: PointF16::new(0.0, 0.0),
            layer: 0,
        };
        let mut target = SkiaClientRenderTarget {
            location: &location,
            target_surface: root_surface,
        };
        self.renderer
            .process_blocks_for_client(RenderBlockId::normal(0).unwrap(), &mut target);
        // render(root_surface);
        false
    }
}

fn convert_virtual_keycodes(winit_code: Option<winit::event::VirtualKeyCode>) -> VirtualKeycode {
    let Some(got_winit_code) = winit_code else { return VirtualKeycode::None };
    match got_winit_code {
        winit::event::VirtualKeyCode::Key1 => VirtualKeycode::Key1,
        winit::event::VirtualKeyCode::Key2 => VirtualKeycode::Key2,
        winit::event::VirtualKeyCode::Key3 => VirtualKeycode::Key3,
        winit::event::VirtualKeyCode::Key4 => VirtualKeycode::Key4,
        winit::event::VirtualKeyCode::Key5 => VirtualKeycode::Key5,
        winit::event::VirtualKeyCode::Key6 => VirtualKeycode::Key6,
        winit::event::VirtualKeyCode::Key7 => VirtualKeycode::Key7,
        winit::event::VirtualKeyCode::Key8 => VirtualKeycode::Key8,
        winit::event::VirtualKeyCode::Key9 => VirtualKeycode::Key9,
        winit::event::VirtualKeyCode::Key0 => VirtualKeycode::Key0,
        winit::event::VirtualKeyCode::A => VirtualKeycode::A,
        winit::event::VirtualKeyCode::B => VirtualKeycode::B,
        winit::event::VirtualKeyCode::C => VirtualKeycode::C,
        winit::event::VirtualKeyCode::D => VirtualKeycode::D,
        winit::event::VirtualKeyCode::E => VirtualKeycode::E,
        winit::event::VirtualKeyCode::F => VirtualKeycode::F,
        winit::event::VirtualKeyCode::G => VirtualKeycode::G,
        winit::event::VirtualKeyCode::H => VirtualKeycode::H,
        winit::event::VirtualKeyCode::I => VirtualKeycode::I,
        winit::event::VirtualKeyCode::J => VirtualKeycode::J,
        winit::event::VirtualKeyCode::K => VirtualKeycode::K,
        winit::event::VirtualKeyCode::L => VirtualKeycode::L,
        winit::event::VirtualKeyCode::M => VirtualKeycode::M,
        winit::event::VirtualKeyCode::N => VirtualKeycode::N,
        winit::event::VirtualKeyCode::O => VirtualKeycode::O,
        winit::event::VirtualKeyCode::P => VirtualKeycode::P,
        winit::event::VirtualKeyCode::Q => VirtualKeycode::Q,
        winit::event::VirtualKeyCode::R => VirtualKeycode::R,
        winit::event::VirtualKeyCode::S => VirtualKeycode::S,
        winit::event::VirtualKeyCode::T => VirtualKeycode::T,
        winit::event::VirtualKeyCode::U => VirtualKeycode::U,
        winit::event::VirtualKeyCode::V => VirtualKeycode::V,
        winit::event::VirtualKeyCode::W => VirtualKeycode::W,
        winit::event::VirtualKeyCode::X => VirtualKeycode::X,
        winit::event::VirtualKeyCode::Y => VirtualKeycode::Y,
        winit::event::VirtualKeyCode::Z => VirtualKeycode::Z,
        winit::event::VirtualKeyCode::Escape => VirtualKeycode::Escape,
        winit::event::VirtualKeyCode::F1 => VirtualKeycode::F1,
        winit::event::VirtualKeyCode::F2 => VirtualKeycode::F2,
        winit::event::VirtualKeyCode::F3 => VirtualKeycode::F3,
        winit::event::VirtualKeyCode::F4 => VirtualKeycode::F4,
        winit::event::VirtualKeyCode::F5 => VirtualKeycode::F5,
        winit::event::VirtualKeyCode::F6 => VirtualKeycode::F6,
        winit::event::VirtualKeyCode::F7 => VirtualKeycode::F7,
        winit::event::VirtualKeyCode::F8 => VirtualKeycode::F8,
        winit::event::VirtualKeyCode::F9 => VirtualKeycode::F9,
        winit::event::VirtualKeyCode::F10 => VirtualKeycode::F10,
        winit::event::VirtualKeyCode::F11 => VirtualKeycode::F11,
        winit::event::VirtualKeyCode::F12 => VirtualKeycode::F12,
        winit::event::VirtualKeyCode::F13 => VirtualKeycode::F13,
        winit::event::VirtualKeyCode::F14 => VirtualKeycode::F14,
        winit::event::VirtualKeyCode::F15 => VirtualKeycode::F15,
        winit::event::VirtualKeyCode::F16 => VirtualKeycode::F16,
        winit::event::VirtualKeyCode::F17 => VirtualKeycode::F17,
        winit::event::VirtualKeyCode::F18 => VirtualKeycode::F18,
        winit::event::VirtualKeyCode::F19 => VirtualKeycode::F19,
        winit::event::VirtualKeyCode::F20 => VirtualKeycode::F20,
        winit::event::VirtualKeyCode::F21 => VirtualKeycode::F21,
        winit::event::VirtualKeyCode::F22 => VirtualKeycode::F22,
        winit::event::VirtualKeyCode::F23 => VirtualKeycode::F23,
        winit::event::VirtualKeyCode::F24 => VirtualKeycode::F24,
        winit::event::VirtualKeyCode::Snapshot => VirtualKeycode::Snapshot,
        winit::event::VirtualKeyCode::Scroll => VirtualKeycode::Scroll,
        winit::event::VirtualKeyCode::Pause => VirtualKeycode::Pause,
        winit::event::VirtualKeyCode::Insert => VirtualKeycode::Insert,
        winit::event::VirtualKeyCode::Home => VirtualKeycode::Home,
        winit::event::VirtualKeyCode::Delete => VirtualKeycode::Delete,
        winit::event::VirtualKeyCode::End => VirtualKeycode::End,
        winit::event::VirtualKeyCode::PageDown => VirtualKeycode::PageDown,
        winit::event::VirtualKeyCode::PageUp => VirtualKeycode::PageUp,
        winit::event::VirtualKeyCode::Left => VirtualKeycode::Left,
        winit::event::VirtualKeyCode::Up => VirtualKeycode::Up,
        winit::event::VirtualKeyCode::Right => VirtualKeycode::Right,
        winit::event::VirtualKeyCode::Down => VirtualKeycode::Down,
        winit::event::VirtualKeyCode::Back => VirtualKeycode::Backspace,
        winit::event::VirtualKeyCode::Return => VirtualKeycode::Return,
        winit::event::VirtualKeyCode::Space => VirtualKeycode::Space,
        winit::event::VirtualKeyCode::Compose => VirtualKeycode::Compose,
        winit::event::VirtualKeyCode::Caret => VirtualKeycode::Caret,
        winit::event::VirtualKeyCode::Numlock => VirtualKeycode::Numlock,
        winit::event::VirtualKeyCode::Numpad0 => VirtualKeycode::Numpad0,
        winit::event::VirtualKeyCode::Numpad1 => VirtualKeycode::Numpad1,
        winit::event::VirtualKeyCode::Numpad2 => VirtualKeycode::Numpad2,
        winit::event::VirtualKeyCode::Numpad3 => VirtualKeycode::Numpad3,
        winit::event::VirtualKeyCode::Numpad4 => VirtualKeycode::Numpad4,
        winit::event::VirtualKeyCode::Numpad5 => VirtualKeycode::Numpad5,
        winit::event::VirtualKeyCode::Numpad6 => VirtualKeycode::Numpad6,
        winit::event::VirtualKeyCode::Numpad7 => VirtualKeycode::Numpad7,
        winit::event::VirtualKeyCode::Numpad8 => VirtualKeycode::Numpad8,
        winit::event::VirtualKeyCode::Numpad9 => VirtualKeycode::Numpad9,
        winit::event::VirtualKeyCode::NumpadAdd => VirtualKeycode::NumpadAdd,
        winit::event::VirtualKeyCode::NumpadDivide => VirtualKeycode::NumpadDivide,
        winit::event::VirtualKeyCode::NumpadDecimal => VirtualKeycode::NumpadDecimal,
        winit::event::VirtualKeyCode::NumpadComma => VirtualKeycode::NumpadComma,
        winit::event::VirtualKeyCode::NumpadEnter => VirtualKeycode::NumpadEnter,
        winit::event::VirtualKeyCode::NumpadEquals => VirtualKeycode::NumpadEquals,
        winit::event::VirtualKeyCode::NumpadMultiply => VirtualKeycode::NumpadMultiply,
        winit::event::VirtualKeyCode::NumpadSubtract => VirtualKeycode::NumpadSubtract,
        winit::event::VirtualKeyCode::AbntC1 => VirtualKeycode::AbntC1,
        winit::event::VirtualKeyCode::AbntC2 => VirtualKeycode::AbntC2,
        winit::event::VirtualKeyCode::Apostrophe => VirtualKeycode::Apostrophe,
        winit::event::VirtualKeyCode::Apps => VirtualKeycode::Apps,
        winit::event::VirtualKeyCode::Asterisk => VirtualKeycode::Asterisk,
        winit::event::VirtualKeyCode::At => VirtualKeycode::At,
        winit::event::VirtualKeyCode::Ax => VirtualKeycode::Ax,
        winit::event::VirtualKeyCode::Backslash => VirtualKeycode::Backslash,
        winit::event::VirtualKeyCode::Calculator => VirtualKeycode::Calculator,
        winit::event::VirtualKeyCode::Capital => VirtualKeycode::Capital,
        winit::event::VirtualKeyCode::Colon => VirtualKeycode::Colon,
        winit::event::VirtualKeyCode::Comma => VirtualKeycode::Comma,
        winit::event::VirtualKeyCode::Convert => VirtualKeycode::Convert,
        winit::event::VirtualKeyCode::Equals => VirtualKeycode::Equals,
        winit::event::VirtualKeyCode::Grave => VirtualKeycode::Grave,
        winit::event::VirtualKeyCode::Kana => VirtualKeycode::Kana,
        winit::event::VirtualKeyCode::Kanji => VirtualKeycode::Kanji,
        winit::event::VirtualKeyCode::LAlt => VirtualKeycode::LAlt,
        winit::event::VirtualKeyCode::LBracket => VirtualKeycode::LBracket,
        winit::event::VirtualKeyCode::LControl => VirtualKeycode::LControl,
        winit::event::VirtualKeyCode::LShift => VirtualKeycode::LShift,
        winit::event::VirtualKeyCode::LWin => VirtualKeycode::LWin,
        winit::event::VirtualKeyCode::Mail => VirtualKeycode::Mail,
        winit::event::VirtualKeyCode::MediaSelect => VirtualKeycode::MediaSelect,
        winit::event::VirtualKeyCode::MediaStop => VirtualKeycode::MediaStop,
        winit::event::VirtualKeyCode::Minus => VirtualKeycode::Minus,
        winit::event::VirtualKeyCode::Mute => VirtualKeycode::Mute,
        winit::event::VirtualKeyCode::MyComputer => VirtualKeycode::MyComputer,
        winit::event::VirtualKeyCode::NavigateForward => VirtualKeycode::NavigateForward,
        winit::event::VirtualKeyCode::NavigateBackward => VirtualKeycode::NavigateBackward,
        winit::event::VirtualKeyCode::NextTrack => VirtualKeycode::NextTrack,
        winit::event::VirtualKeyCode::NoConvert => VirtualKeycode::NoConvert,
        winit::event::VirtualKeyCode::OEM102 => VirtualKeycode::OEM102,
        winit::event::VirtualKeyCode::Period => VirtualKeycode::Period,
        winit::event::VirtualKeyCode::PlayPause => VirtualKeycode::PlayPause,
        winit::event::VirtualKeyCode::Plus => VirtualKeycode::Plus,
        winit::event::VirtualKeyCode::Power => VirtualKeycode::Power,
        winit::event::VirtualKeyCode::PrevTrack => VirtualKeycode::PrevTrack,
        winit::event::VirtualKeyCode::RAlt => VirtualKeycode::RAlt,
        winit::event::VirtualKeyCode::RBracket => VirtualKeycode::RBracket,
        winit::event::VirtualKeyCode::RControl => VirtualKeycode::RControl,
        winit::event::VirtualKeyCode::RShift => VirtualKeycode::RShift,
        winit::event::VirtualKeyCode::RWin => VirtualKeycode::RWin,
        winit::event::VirtualKeyCode::Semicolon => VirtualKeycode::Semicolon,
        winit::event::VirtualKeyCode::Slash => VirtualKeycode::Slash,
        winit::event::VirtualKeyCode::Sleep => VirtualKeycode::Sleep,
        winit::event::VirtualKeyCode::Stop => VirtualKeycode::Stop,
        winit::event::VirtualKeyCode::Sysrq => VirtualKeycode::Sysrq,
        winit::event::VirtualKeyCode::Tab => VirtualKeycode::Tab,
        winit::event::VirtualKeyCode::Underline => VirtualKeycode::Underline,
        winit::event::VirtualKeyCode::Unlabeled => VirtualKeycode::Unlabeled,
        winit::event::VirtualKeyCode::VolumeDown => VirtualKeycode::VolumeDown,
        winit::event::VirtualKeyCode::VolumeUp => VirtualKeycode::VolumeUp,
        winit::event::VirtualKeyCode::Wake => VirtualKeycode::Wake,
        winit::event::VirtualKeyCode::WebBack => VirtualKeycode::WebBack,
        winit::event::VirtualKeyCode::WebFavorites => VirtualKeycode::WebFavorites,
        winit::event::VirtualKeyCode::WebForward => VirtualKeycode::WebForward,
        winit::event::VirtualKeyCode::WebHome => VirtualKeycode::WebHome,
        winit::event::VirtualKeyCode::WebRefresh => VirtualKeycode::WebRefresh,
        winit::event::VirtualKeyCode::WebSearch => VirtualKeycode::WebSearch,
        winit::event::VirtualKeyCode::WebStop => VirtualKeycode::WebStop,
        winit::event::VirtualKeyCode::Yen => VirtualKeycode::Yen,
        winit::event::VirtualKeyCode::Copy => VirtualKeycode::Copy,
        winit::event::VirtualKeyCode::Paste => VirtualKeycode::Paste,
        winit::event::VirtualKeyCode::Cut => VirtualKeycode::Cut,
    }
}
