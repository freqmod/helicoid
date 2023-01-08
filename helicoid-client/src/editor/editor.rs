use std::{sync::Arc, time::Duration};

use crate::HeliconeCommandLineArguments;
use helicoid_protocol::{
    input::{HelicoidToServerMessage, ViewportInfo},
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
    //bridge: ClientTcpBridge,
    sender: Option<Sender<TcpBridgeToServerMessage>>,
    receiver: Option<Receiver<TcpBridgeToClientMessage>>,
}
pub struct HeliconeEditor {
    inner: Arc<TMutex<Option<HeliconeEditorInner>>>,
    sender: Option<Sender<TcpBridgeToServerMessage>>,
    receiver: Option<Receiver<TcpBridgeToClientMessage>>,
    server_address: Option<String>,
    current_viewport_info: Option<ViewportInfo>,
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
    pub fn handle_event_disconnected(&mut self, event: &Event<()>) {
        match event {
            Event::WindowEvent { window_id, event } => match event {
                WindowEvent::Resized(event) => {
                    let size = ViewportInfo {
                        physical_size: (event.width, event.height),
                        scale_factor: OrderedFloat(1.0),
                        container_physical_size: None,
                        container_scale_factor: None,
                    };
                    self.current_viewport_info = Some(size.clone());
                }
                _ => {}
            },

            _ => {}
        }
    }
    pub fn poll_events(&mut self) {
        if !self.ensure_connected() {
            return;
        }
        self.peek_and_process_events();
    }

    pub fn handle_event(&mut self, event: &Event<()>) {
        if !self.ensure_connected() {
            log::warn!("Try to handle event before connection is established to server");
            self.handle_event_disconnected(event);
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
                        self.current_viewport_info = Some(size.clone());
                        let size_msg = TcpBridgeToServerMessage {
                            message: HelicoidToServerMessage::ViewportSizeUpdate(size),
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
                        log::trace!("Resize sent viewport info");
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
                    Ok(event) => {
                        log::trace!("Got event from server: {:?}", event);
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
    pub fn draw_frame(&mut self, root_canvas: &mut Canvas, dt: f32) -> bool {
        if !self.ensure_connected() {
            log::warn!("Try to draw frame before connection is established to server");
            return false;
        }
        log::trace!("Editor: got request to draw frame");
        self.peek_and_process_events();
        false
    }
}
