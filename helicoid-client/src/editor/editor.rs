use std::{sync::Arc, time::Duration};

use crate::HeliconeCommandLineArguments;
use helicoid_protocol::{
    input::ViewportInfo,
    tcp_bridge::{ClientTcpBridge, TcpBridgeToClientMessage, TcpBridgeToServerMessage},
};
use ordered_float::OrderedFloat;
use skia_safe::Canvas;
use tokio::sync::{
    mpsc::{Receiver, Sender},
    Mutex as TMutex,
};
use winit::event::{Event, WindowEvent};

struct HeliconeEditorInner {
    bridge: ClientTcpBridge,
    sender: Option<Sender<TcpBridgeToServerMessage>>,
    receiver: Option<Receiver<TcpBridgeToClientMessage>>,
}
pub struct HeliconeEditor {
    inner: Arc<TMutex<Option<HeliconeEditorInner>>>,
    sender: Option<Sender<TcpBridgeToServerMessage>>,
    receiver: Option<Receiver<TcpBridgeToClientMessage>>,
    server_address: Option<String>,
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
            sender: None,
            receiver: None,
            server_address: args.server_address.clone(),
        }
    }
    fn try_connect(inner: Arc<TMutex<Option<HeliconeEditorInner>>>, addr: String) {
        let _ = tokio::spawn(async move {
            loop {
                match ClientTcpBridge::connect(&addr).await {
                    Ok((bridge, sender, receiver)) => {
                        let mut inner_locked = inner.lock().await;
                        *inner_locked = Some(HeliconeEditorInner {
                            bridge,
                            sender: Some(sender),
                            receiver: Some(receiver),
                        });
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
            return true;
        }
        if let Some(mut inner) = self.inner.try_lock().ok() {
            if let Some(inner) = &mut *inner {
                if let Some(sender) = inner.sender.take() {
                    self.sender = Some(sender);
                }
                if let Some(receiver) = inner.receiver.take() {
                    self.receiver = Some(receiver);
                }
                if self.sender.is_some() && self.receiver.is_some() {
                    return true;
                }
            }
        }
        false
    }
    pub fn handle_event(&mut self, event: &Event<()>) {
        if !self.ensure_connected() {
            log::warn!("Try to handle event before connection is established to server");
            return;
        }
        if let Some(inner) = self.inner.try_lock().ok() {
            match event {
                Event::NewEvents(_) => {}
                Event::WindowEvent { window_id, event } => match event {
                    WindowEvent::Resized(event) => {
                        /* Convert event to helicoid protocol and send off  to server. */
                        let size = ViewportInfo {
                            physical_size: (event.width, event.height),
                            scale_factor: OrderedFloat(1.0),
                            container_physical_size: None,
                            container_scale_factor: None,
                        };
                    }
                    WindowEvent::Moved(_) => {}
                    WindowEvent::CloseRequested => {}
                    WindowEvent::Destroyed => {}
                    WindowEvent::DroppedFile(_) => {}
                    WindowEvent::HoveredFile(_) => {}
                    WindowEvent::HoveredFileCancelled => {}
                    WindowEvent::ModifiersChanged(_) => {}
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
                    } => {}
                    WindowEvent::ThemeChanged(_) => {}
                    WindowEvent::ReceivedCharacter(_) => {}
                    WindowEvent::Focused(_) => {}
                    WindowEvent::KeyboardInput {
                        device_id,
                        input,
                        is_synthetic,
                    } => {}
                    WindowEvent::Ime(_) => {}
                    WindowEvent::Occluded(_) => {}
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
                    Ok(event) => {}
                    Err(e) => match e {
                        tokio::sync::mpsc::error::TryRecvError::Empty => {
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
    pub fn draw_frame(&mut self, root_canvas: &mut Canvas, dt: f32) -> bool {
        if !self.ensure_connected() {
            log::warn!("Try to handle event before connection is established to server");
            return false;
        }
        self.peek_and_process_events();
        false
    }
}
