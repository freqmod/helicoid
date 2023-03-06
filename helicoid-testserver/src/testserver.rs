use anyhow::{anyhow, Result};
use async_trait::async_trait;
use helicoid_protocol::{
    caching_shaper::CachingShaper,
    gfx::{
        HelicoidToClientMessage, MetaDrawBlock, NewRenderBlock, PathVerb, PointF16,
        RemoteBoxUpdate, RenderBlockDescription, RenderBlockId, RenderBlockLocation,
        RenderBlockPath, SimpleDrawBlock, SimpleDrawElement, SimpleDrawPath, SimpleDrawPolygon,
        SimplePaint, SimpleRoundRect, SimpleSvg,
    },
    input::{
        CursorMovedEvent, HelicoidToServerMessage, ImeEvent, KeyModifierStateUpdateEvent,
        MouseButtonStateChangeEvent, SimpleKeyTappedEvent, ViewportInfo,
    },
    tcp_bridge::{
        TcpBridgeServer, TcpBridgeServerConnectionState, TcpBridgeToClientMessage,
        TcpBridgeToServerMessage,
    },
    text::ShapableString,
};
use ordered_float::OrderedFloat;
use smallvec::{smallvec, SmallVec};
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::{
    broadcast::{self, Receiver as BReceiver, Sender as BSender},
    mpsc::{self, Receiver, Sender},
    Mutex as TMutex,
};

/* Architecture:
The (Dummy)Editor object is stored in a shared Arc<TMutex<>> object, and is cloned
to all the client handles. All clients register with the editor to be notified (using a channel)
when there are changes. When the editing model has changed they will determine if the client
needs an update. */
struct DummyEditor {
    editor_state_changed_send: BSender<()>,
}
struct ServerStateData {
    editor: Arc<TMutex<DummyEditor>>,
}

struct ServerState {
    pending_message: Option<TcpBridgeToServerMessage>,
    peer_address: SocketAddr,
    channel_tx: Sender<TcpBridgeToClientMessage>,
    channel_rx: Receiver<TcpBridgeToServerMessage>,
    close_rx: BReceiver<()>,
    editor_update_rx: BReceiver<()>,
    state_data: ServerStateData,

    viewport_size: Option<ViewportInfo>,
}
pub struct HelicoidTestServer {
    editor: Arc<TMutex<DummyEditor>>,
    listen_address: String,
    bridge: Arc<TMutex<TcpBridgeServer<ServerState>>>,
}
impl HelicoidTestServer {
    pub async fn new(listen_address: String) -> Result<Self> {
        let editor = Arc::new(TMutex::new(DummyEditor::new()));
        let bridge = Arc::new(TMutex::new(TcpBridgeServer::<ServerState>::new().await?));
        //bridge.bind(&listen_address).await;
        Ok(Self {
            editor,
            bridge,
            listen_address,
        })
    }

    pub async fn event_loop(&mut self) -> Result<Self> {
        log::trace!("Helicoid test server event loop start");
        let mut state_data = ServerStateData {
            editor: self.editor.clone(),
        };
        loop {
            log::trace!("Helicoid test server event loop iterate");
            tokio::select! {
                result = TcpBridgeServer::wait_for_connection(self.bridge.clone(), &self.listen_address, state_data) =>{
                    /* Currently all event handling is done inside the state */
                    state_data =  ServerStateData { editor: self.editor.clone()};
                },
                /* Maybe add select on program close-channel here to close cleanly */
            }
        }
        log::trace!("Helicoid test server event loop completed");
    }
}
impl DummyEditor {
    pub fn new() -> Self {
        let (editor_state_changed_send, _) = broadcast::channel(1);

        Self {
            editor_state_changed_send,
        }
    }
    pub fn update_receiver(&self) -> BReceiver<()> {
        self.editor_state_changed_send.subscribe()
    }
}

impl ServerState {
    //    async fn process_event(&mut self, e: &mut DummyEditor) {}
    async fn handle_client_message(&mut self, message: TcpBridgeToServerMessage) -> Result<()> {
        match message.message {
            HelicoidToServerMessage::ViewportSizeUpdate(viewportinfo) => {
                self.viewport_size = Some(viewportinfo);
                self.sync_screen().await?;
            }
            HelicoidToServerMessage::KeyModifierStateUpdate(keymodifierstateupdateevent) => {}
            HelicoidToServerMessage::KeyPressedEvent(simplekeytappedevent) => {}
            HelicoidToServerMessage::MouseButtonStateChange(mousebuttonstatechangeevent) => {}
            HelicoidToServerMessage::CursorMoved(cursormovedevent) => {}
            HelicoidToServerMessage::CharReceived(ch) => {}
            HelicoidToServerMessage::Ime(imeevent) => {}
            HelicoidToServerMessage::ClipboardEvent(clipboard) => {}
        }
        //self.send_simple_test_shaped_string().await?;

        Ok(())
    }
    async fn sync_screen(&mut self) -> Result<()> {
        self.send_simple_test_shaped_string().await?;
        Ok(())
    }

    async fn editor_updated(&mut self) -> Result<()> {
        let editor = self.state_data.editor.lock();
        /* Assess if the update is relevant for the client represented by this server state,
        update internal shadow state and send any relevant updates to the client
        (after unlocking the editor)*/
        Ok(())
    }
    async fn send_simple_test_shaped_string(&mut self) -> Result<()> {
        //        let editor = self.state_data.editor.lock();
        let mut shaper = CachingShaper::new(1.0f32);
        shaper.set_font_key(0, String::from("Anonymous Pro"));
        //shaper.set_font_key(1, String::from("NotoSansMono-Regular"));
        shaper.set_font_key(1, String::from("FiraCodeNerdFont-Regular"));
        shaper.set_font_key(2, String::from("NotoColorEmoji"));
        shaper.set_font_key(3, String::from("MissingGlyphs"));
        shaper.set_font_key(4, String::from("LastResort-Regular"));
        let mut string_to_shape = ShapableString::from_text(
            "See IF we can shape a simple string\n ≠ <= string Some(typeface) => { 😀🙀 What about newlines?",
        );
        string_to_shape.metadata_runs.iter_mut().for_each(|i| {
            i.font_color = 0xF000A030;
            i.font_info.font_parameters.size = OrderedFloat(20.0f32);
        });
        let mut shaped = shaper.shape(&string_to_shape, &None);
        //        let mut new_render_blocks = SmallVec::with_capacity(1);
        let new_shaped_string_block = NewRenderBlock {
            id: RenderBlockId::normal(1000).unwrap(),
            contents: RenderBlockDescription::ShapedTextBlock(shaped),
        };
        //        new_render_blocks.push(new_shaped_string_block);
        //        let mut render_block_locations = SmallVec::with_capacity(1);
        let shaped_string_location = RenderBlockLocation {
            //path: RenderBlockPath::new(smallvec![1]),
            id: RenderBlockId::normal(1000).unwrap(),
            layer: 2,
            location: PointF16::new(1.0, 1.0),
        };
        let meta_string_block = NewRenderBlock {
            id: RenderBlockId::normal(1).unwrap(),
            contents: RenderBlockDescription::MetaBox(MetaDrawBlock {
                extent: PointF16::new(1500.0, 1500.0),
                sub_blocks: smallvec![RenderBlockLocation {
                    id: RenderBlockId::normal(1000).unwrap(),
                    layer: 1,
                    location: PointF16::new(0.0, 0.0)
                }],
            }),
        };
        let meta_block_location = RenderBlockLocation {
            //path: RenderBlockPath::new(smallvec![1]),
            id: RenderBlockId::normal(1).unwrap(),
            layer: 0,
            location: PointF16::new(1.0, 1.0),
        };
        //        render_block_locations.push(shaped_string_location);
        //        render_block_locations.push(meta_block_location);
        //        new_render_blocks.push(meta_string_block);

        let box_update = RemoteBoxUpdate {
            parent: RenderBlockPath::top(),
            new_render_blocks: smallvec![meta_string_block],
            remove_render_blocks: Default::default(),
            move_block_locations: smallvec![meta_block_location],
        };
        let msg = TcpBridgeToClientMessage {
            message: HelicoidToClientMessage { update: box_update },
        };
        log::trace!("Prepared message1, now sending it to the tcp bridge");
        self.channel_tx.send(msg).await?;
        let polygon = SimpleDrawPolygon {
            paint: SimplePaint::new(Some(0xFFAABBCC), Some(0xAABB55DD), Some(5.0)),
            draw_elements: smallvec![
                PointF16::new(0.0, 0.0),
                PointF16::new(150.0, 0.0),
                PointF16::new(200.7, 300.9),
                PointF16::new(150.3, 150.6),
                PointF16::new(70.1, 20.5),
            ],
            closed: true,
        };
        let rrect = SimpleRoundRect {
            paint: SimplePaint::new(Some(0xFFAABBCC), Some(0xAA3311DD), Some(5.0)),
            topleft: PointF16::new(50.0, 60.0),
            bottomright: PointF16::new(100.0, 80.0),
            roundedness: PointF16::new(5.0, 5.5),
        };
        let path = SimpleDrawPath {
            paint: SimplePaint::new(Some(0xFFAABBCC), Some(0xAABB99DD), Some(5.0)),
            draw_elements: smallvec![
                (
                    PathVerb::Move,
                    PointF16::new(250.0, 250.0),
                    Default::default(),
                    Default::default()
                ),
                (
                    PathVerb::Cubic,
                    PointF16::new(500.0, 500.0),
                    PointF16::new(100.0, 200.0),
                    PointF16::new(700.0, 800.0),
                ),
                (
                    PathVerb::Quad,
                    PointF16::new(400.0, 900.0),
                    PointF16::new(300.0, 800.0),
                    Default::default(),
                ),
                (
                    PathVerb::Line,
                    PointF16::new(100.0, 300.0),
                    Default::default(),
                    Default::default(),
                ),
                (
                    PathVerb::Close,
                    Default::default(),
                    Default::default(),
                    Default::default(),
                ),
            ],
        };
        let svg = SimpleSvg {
            paint: SimplePaint::new(Some(0xFFAABBCC), Some(0xAA3311DD), Some(5.0)),
            location: PointF16::new(90.0, 60.0),
            extent: PointF16::new(512.0, 512.0),
            resource_name: smallvec![b't', b'e', b's', b't'],
        };
        let fill_block = NewRenderBlock {
            id: RenderBlockId::normal(1001).unwrap(),
            contents: RenderBlockDescription::SimpleDraw(SimpleDrawBlock {
                extent: PointF16::new(1000f32, 1000f32),
                draw_elements: smallvec![
                    SimpleDrawElement::Polygon(polygon),
                    SimpleDrawElement::fill(SimplePaint::new(
                        Some(0xFF110022),
                        Some(0x11009255),
                        Some(0.5)
                    )),
                    SimpleDrawElement::RoundRect(rrect),
                    SimpleDrawElement::Path(path),
                    SimpleDrawElement::SvgResource(svg),
                ],
            }),
        };
        let fill_location = RenderBlockLocation {
            //path: RenderBlockPath::new(smallvec![1]),
            id: RenderBlockId::normal(1001).unwrap(),
            layer: 0,
            location: PointF16::new(10.0, 10.0),
        };

        let box_text_update = RemoteBoxUpdate {
            parent: RenderBlockPath::new(smallvec![RenderBlockId::normal(1).unwrap()]),
            new_render_blocks: smallvec![new_shaped_string_block, fill_block],
            remove_render_blocks: Default::default(),
            move_block_locations: smallvec![shaped_string_location, fill_location],
        };
        log::trace!("Prepared message2, now sending it to the tcp bridge");
        self.channel_tx
            .send(TcpBridgeToClientMessage {
                message: HelicoidToClientMessage {
                    update: box_text_update,
                },
            })
            .await?;

        Ok(())
    }
}
#[async_trait]
impl TcpBridgeServerConnectionState for ServerState {
    type StateData = ServerStateData;
    async fn new_state(
        peer_address: SocketAddr,
        channel_tx: Sender<TcpBridgeToClientMessage>,
        channel_rx: Receiver<TcpBridgeToServerMessage>,
        close_rx: BReceiver<()>,
        state_data: Self::StateData,
    ) -> Self {
        let editor_update_rx = {
            let inner_editor = state_data.editor.lock().await;
            inner_editor.update_receiver()
        };
        Self {
            pending_message: None,
            peer_address,
            channel_tx,
            channel_rx,
            close_rx,
            state_data,
            editor_update_rx,
            viewport_size: None,
        }
    }
    async fn initialize(&mut self) -> Result<()> {
        Ok(())
    }
    async fn event_loop(&mut self) -> Result<()> {
        loop {
            tokio::select! {
                client_message = self.channel_rx.recv() =>{
                    match client_message{
                        Some(message) => self.handle_client_message(message).await?,
                        None => break,
                    };
                },
                _editor_message = self.editor_update_rx.recv() =>{
                    self.editor_updated().await?
                }
                _close_message = self.close_rx.recv() =>{
                    break;
                }
            }
        }
        Ok(())
    }
}
