use std::{sync::Arc, time::Duration};

use crate::HeliconeCommandLineArguments;
use glutin::event::Event;
use helicoid_protocol::tcp_bridge::{
    ClientTcpBridge, TcpBridgeToClientMessage, TcpBridgeToServerMessage,
};
use skia_safe::Canvas;
use tokio::sync::{
    mpsc::{Receiver, Sender},
    Mutex as TMutex,
};

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
        if let Some(inner) = self.inner.try_lock().ok() {}
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
