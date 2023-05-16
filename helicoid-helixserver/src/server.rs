use ahash::AHasher;
use anyhow::{anyhow, Result};
use arc_swap::{access::Map, ArcSwap};
use async_trait::async_trait;
use hashbrown::HashMap;

use helicoid_protocol::{
    block_manager::RenderBlockFullId,
    caching_shaper::CachingShaper,
    gfx::{
        FontPaint, HelicoidToClientMessage, MetaDrawBlock, NewRenderBlock, PathVerb, PointF16,
        PointF32, PointU16, PointU32, RemoteBoxUpdate, RenderBlockDescription, RenderBlockId,
        RenderBlockLocation, RenderBlockPath, SimpleDrawBlock, SimpleDrawElement, SimpleDrawPath,
        SimpleDrawPolygon, SimplePaint, SimpleRoundRect, SimpleSvg,
    },
    input::{
        CursorMovedEvent, HelicoidToServerMessage, ImeEvent, KeyModifierStateUpdateEvent,
        MouseButtonStateChangeEvent, SimpleKeyTappedEvent, ViewportInfo, VirtualKeycode,
    },
    tcp_bridge::{
        TcpBridgeServer, TcpBridgeServerConnectionState, TcpBridgeToClientMessage,
        TcpBridgeToServerMessage,
    },
    text::{FontEdging, FontHinting, ShapableString, SmallFontOptions},
};
use helix_core::{
    config::user_syntax_loader,
    movement::{move_horizontally, move_vertically, Direction},
    syntax,
};
use helix_view::{
    editor::{Action, Config},
    graphics::Rect,
    theme, Editor,
};
use ordered_float::OrderedFloat;
use smallvec::{smallvec, SmallVec};
use std::{
    hash::{Hash, Hasher},
    net::SocketAddr,
    sync::Arc,
};
use tokio::sync::{
    broadcast::{self, Receiver as BReceiver, Sender as BSender},
    mpsc::{self, Receiver, Sender},
    Mutex as TMutex,
};

use crate::editor::Editor as HcEditor;
use crate::editor_view::{ContentVisitor, EditorTree};

const CONTAINER_IDS_BASE: u16 = 0x100;
const ENCLOSURE_ID: u16 = 0x0;

const UNSCALED_FONT_SIZE: f32 = 12f32;
struct Compositor {
    containers: HashMap<RenderBlockId, EditorTree>,
    content_visitor: ContentVisitor,
    client_messages_scratch: Vec<RemoteBoxUpdate>,
}

#[derive(Debug)]
struct EditorEnclosure {
    enclosure_location: RenderBlockLocation,
    enclosure_meta: MetaDrawBlock,
}
/* This struct stores a pointer to the common editor, as well as all client specific
information */
struct ServerStateData {
    compositor: Option<Box<Compositor>>,
    enclosure: Option<EditorEnclosure>,
    enclosure_hash: Option<u64>,
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
pub struct HelicoidServer {
    editor: Arc<TMutex<HcEditor>>,
    listen_address: String,
    bridge: Arc<TMutex<TcpBridgeServer<ServerState>>>,
}
impl HelicoidServer {
    pub async fn new(listen_address: String) -> Result<Self> {
        let editor = Arc::new(TMutex::new(HcEditor::new()));
        let bridge = Arc::new(TMutex::new(TcpBridgeServer::<ServerState>::new().await?));
        //bridge.bind(&listen_address).await;
        Ok(Self {
            editor,
            bridge,
            listen_address,
        })
    }

    pub fn make_content_visitor(
        scale_factor: f32,
        editor: Arc<TMutex<HcEditor>>,
    ) -> ContentVisitor {
        let unscaled_font_size = UNSCALED_FONT_SIZE;
        let mut shaper = CachingShaper::new(scale_factor, unscaled_font_size);
        //shaper.set_font_key(0, String::from("AnonymiceNerd"));
        shaper.set_font_key(0, String::from("FiraCodeNerdFont-Regular"));
        /*        shaper.set_font_key(1, String::from("FiraCodeNerdFont-Regular"));
        shaper.set_font_key(2, String::from("NotoColorEmoji"));
        shaper.set_font_key(3, String::from("MissingGlyphs"));
        shaper.set_font_key(4, String::from("LastResort-Regular"));*/

        let mut line_height = 0f32;
        for id in 0..5 {
            let mut options = SmallFontOptions {
                family_id: id,
                font_parameters: shaper.default_parameters(),
            };
            options.font_parameters.size = OrderedFloat(unscaled_font_size);
            if let Some((metrics, _advance)) = shaper.info(&options) {
                line_height = line_height.max(metrics.ascent + metrics.descent);
            }
        }
        ContentVisitor::new(line_height, shaper, editor)
    }
    pub async fn event_loop(&mut self) -> Result<Self> {
        log::trace!("Helicoid test server event loop start");
        loop {
            let mut visitor = Self::make_content_visitor(1.0f32, self.editor.clone());

            let shaper = visitor.shaper();
            let mut font_options = SmallFontOptions {
                family_id: 0,
                font_parameters: shaper.default_parameters(),
            };
            font_options.font_parameters.size = OrderedFloat(UNSCALED_FONT_SIZE);
            let font_metrics = shaper.info(&font_options).unwrap().0;

            let mut state_data = ServerStateData {
                enclosure: None,
                enclosure_hash: None,
                compositor: Some(Box::new(Compositor {
                    containers: HashMap::default(),
                    content_visitor: visitor,
                    client_messages_scratch: Default::default(),
                })),
            };
            let view_id = {
                let mut editor = self.editor.lock().await;
                let heditor = editor.editor_mut();
                //                let doc_id = Some(heditor.new_file(Action::VerticalSplit));
                let doc_id = heditor.open(
                    &std::env::current_dir().unwrap().join("src/center.rs"),
                    helix_view::editor::Action::VerticalSplit,
                );
                let view_id = heditor.tree.focus;
                assert_eq!(heditor.tree.get(view_id).doc, doc_id.unwrap());
                Some(view_id)
            };
            let mut initial_container = EditorTree::new(
                RenderBlockPath::new(smallvec![RenderBlockId(ENCLOSURE_ID)]),
                RenderBlockId(CONTAINER_IDS_BASE),
                UNSCALED_FONT_SIZE,
                1.0f32, /* Scale factor is determined when a resize event occurs */
                font_metrics,
                view_id,
                PointF32::default(),
            );
            let initial_container = {
                let mut compositor = state_data.compositor.take();
                let (initial_container, compositor) = tokio::task::spawn_blocking(move || {
                    initial_container.initialize(&mut compositor.as_mut().unwrap().content_visitor);
                    (initial_container, compositor)
                })
                .await
                .unwrap();
                state_data.compositor = compositor;
                initial_container
            };
            state_data
                .compositor
                .as_mut()
                .unwrap()
                .containers
                .insert(RenderBlockId(CONTAINER_IDS_BASE), initial_container);

            log::trace!("Helicoid test server event loop iterate");
            tokio::select! {
            result = TcpBridgeServer::wait_for_connection(self.bridge.clone(), &self.listen_address, state_data) =>{
                    /* Currently all event handling is done inside the state */

                },
                /* Maybe add select on program close-channel here to close cleanly */
            }
        }
        //log::trace!("Helicoid test server event loop completed");
    }
}

impl ServerState {
    //    async fn process_event(&mut self, e: &mut DummyEditor) {}
    async fn handle_client_message(&mut self, message: TcpBridgeToServerMessage) -> Result<()> {
        log::trace!("Handle client message: {:?}", message.message);
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
            HelicoidToServerMessage::KeyInputEvent(event) => {
                if event.pressed {
                    let text = match event.virtual_keycode {
                        VirtualKeycode::A => Some('A'),
                        VirtualKeycode::B => Some('B'),
                        VirtualKeycode::C => Some('C'),
                        VirtualKeycode::D => Some('D'),
                        VirtualKeycode::E => Some('E'),
                        VirtualKeycode::F => Some('F'),
                        VirtualKeycode::G => Some('G'),
                        VirtualKeycode::H => Some('H'),
                        VirtualKeycode::I => Some('I'),
                        VirtualKeycode::J => Some('J'),
                        VirtualKeycode::K => Some('K'),
                        VirtualKeycode::L => Some('L'),
                        VirtualKeycode::M => Some('M'),
                        VirtualKeycode::N => Some('N'),
                        VirtualKeycode::O => Some('O'),
                        VirtualKeycode::P => Some('P'),
                        VirtualKeycode::Q => Some('Q'),
                        VirtualKeycode::R => Some('R'),
                        VirtualKeycode::S => Some('S'),
                        VirtualKeycode::T => Some('T'),
                        VirtualKeycode::U => Some('U'),
                        VirtualKeycode::V => Some('V'),
                        VirtualKeycode::W => Some('W'),
                        VirtualKeycode::X => Some('X'),
                        VirtualKeycode::Y => Some('Y'),
                        VirtualKeycode::Z => Some('Z'),
                        VirtualKeycode::Space => Some(' '),
                        VirtualKeycode::Up => {
                            self.move_document(None, Some(Direction::Backward)).await;
                            self.sync_screen().await?;
                            None
                        }
                        VirtualKeycode::Down => {
                            self.move_document(None, Some(Direction::Forward)).await;
                            self.sync_screen().await?;
                            None
                        }
                        VirtualKeycode::Left => {
                            self.move_document(Some(Direction::Backward), None).await;
                            self.sync_screen().await?;
                            None
                        }
                        VirtualKeycode::Right => {
                            self.move_document(Some(Direction::Forward), None).await;
                            self.sync_screen().await?;
                            None
                        }
                        _ => None,
                    };
                    if let Some(text) = text {
                        let mut editor = self
                            .state_data
                            .compositor
                            .as_mut()
                            .unwrap()
                            .content_visitor
                            .editor()
                            .lock()
                            .await;
                        //editor.text += &text.to_string();
                    }
                    if let VirtualKeycode::Backspace = event.virtual_keycode {
                        let mut editor = self
                            .state_data
                            .compositor
                            .as_mut()
                            .unwrap()
                            .content_visitor
                            .editor()
                            .lock()
                            .await;
                        //let textlen = editor.text.len().saturating_sub(1);
                        //editor.text.truncate(textlen);
                    }
                }
            }
        }
        //self.send_simple_test_shaped_string().await?;

        Ok(())
    }

    async fn move_document(&mut self, dx: Option<Direction>, dy: Option<Direction>) {
        let view_id = self
            .state_data
            .compositor
            .as_ref()
            .unwrap()
            .containers()
            .get(&RenderBlockId(CONTAINER_IDS_BASE))
            .unwrap()
            .current_view_id()
            .unwrap();
        let context = &mut self.state_data.compositor.as_mut().unwrap().content_visitor;
        let editor_locked = context.editor();
        let mut editor = editor_locked.lock().await;
        //        let mut doc = context.current_doc().unwrap();
        //        let (view, doc) = doc.destruct_mut();
        let view = editor.editor().tree.get(view_id);
        let doc_id = view.doc.clone();
        let doc = editor.editor().documents.get(&doc_id);
        let doc = doc.unwrap();
        let viewport_width = view.inner_area(doc).width;
        let count = 1;
        let text = doc.text().slice(..);
        let text_fmt = doc.text_format(viewport_width, None);
        let mut annotations = view.text_annotations(doc, None);

        let mut selection = doc.selection(view.id).clone();
        let old_selection = selection.clone();
        if let Some(dx) = dx {
            selection = selection.transform(|range| {
                move_horizontally(
                    text,
                    range,
                    dx,
                    count,
                    helix_core::movement::Movement::Move,
                    &text_fmt,
                    &mut annotations,
                )
            });
        }
        if let Some(dy) = dy {
            selection = selection.transform(|range| {
                move_vertically(
                    text,
                    range,
                    dy,
                    count,
                    helix_core::movement::Movement::Move,
                    &text_fmt,
                    &mut annotations,
                )
            });
        }
        let doc_mut = editor.editor_mut().documents.get_mut(&doc_id).unwrap();
        log::debug!(
            "Moving section: x: {:?} y: {:?} {:?} -> {:?}",
            dx,
            dy,
            old_selection,
            selection
        );
        doc_mut.set_selection(view_id, selection);
    }

    async fn maintain_enclosure(&mut self) -> Result<()> {
        let view_size = self.viewport_size.as_ref().unwrap().physical_size;
        let scale_factor = self.viewport_size.as_ref().unwrap().scale_factor;
        let enclosure_extent = PointF32::new(view_size.0 as f32, view_size.1 as f32);
        let mut content_list_hasher = AHasher::default();
        enclosure_extent.hash(&mut content_list_hasher);
        {
            for (_id, container) in &self.state_data.compositor.as_ref().unwrap().containers {
                //                let MetaDrawBlock { extent, buffered, alpha, sub_blocks }
                content_list_hasher.write_u16(container.top_container_id().0);
            }
        }
        let enclosure_content_hash = content_list_hasher.finish();

        if self.state_data.enclosure.is_none()
            || self
                .state_data
                .enclosure_hash
                .map(|h| h != enclosure_content_hash)
                .unwrap_or(false)
        {
            /* Make sure visitor scale factor is up to date */
            self.state_data
                .compositor
                .as_mut()
                .unwrap()
                .content_visitor
                .shaper()
                .change_scale_factor(f32::from(scale_factor));

            let containers = &mut self.state_data.compositor.as_mut().unwrap().containers;
            let mut sub_blocks = SmallVec::with_capacity(containers.len());
            for (_id, container) in containers.iter_mut() {
                container.resize(enclosure_extent, scale_factor);
                //                let MetaDrawBlock { extent, buffered, alpha, sub_blocks }
                let block_loc = RenderBlockLocation {
                    id: container.top_container_id(),
                    location: PointF32::default(),
                    layer: 0x40,
                };
                sub_blocks.push(block_loc);
            }

            self.state_data.enclosure = Some(EditorEnclosure {
                enclosure_location: RenderBlockLocation {
                    id: RenderBlockId(ENCLOSURE_ID),
                    location: PointF32::default(),
                    layer: 0x10,
                },
                enclosure_meta: MetaDrawBlock {
                    extent: enclosure_extent,
                    buffered: false,
                    alpha: None,
                    sub_blocks,
                },
            });
            self.state_data
                .enclosure
                .as_mut()
                .unwrap()
                .send_message(&mut self.channel_tx)
                .await?;
            self.state_data.enclosure_hash = Some(enclosure_content_hash);
            log::trace!(
                "Sent editor enclosure for view dimensions {:?}",
                enclosure_extent,
                //self.state_data.enclosure,
            );
        }

        Ok(())
    }
    async fn sync_screen(&mut self) -> Result<()> {
        self.maintain_enclosure().await?;
        //        self.send_simple_test_shaped_string().await?;
        let mut compositor = self.state_data.compositor.take();
        let compositor = tokio::task::spawn_blocking(move || {
            compositor.as_mut().unwrap().sync_screen().unwrap();
            compositor
        })
        .await
        .unwrap();
        self.state_data.compositor = compositor;
        self.state_data
            .compositor
            .as_mut()
            .unwrap()
            .transfer_messages_to_client(&mut self.channel_tx)
            .await?;
        self.maintain_enclosure().await?;
        Ok(())
    }

    async fn editor_updated(&mut self) -> Result<()> {
        let mut editor = self
            .state_data
            .compositor
            .as_mut()
            .unwrap()
            .content_visitor
            .editor()
            .lock();
        /* Assess if the update is relevant for the client represented by this server state,
        update internal shadow state and send any relevant updates to the client
        (after unlocking the editor)*/
        Ok(())
    }
}
impl EditorEnclosure {
    pub async fn send_message(
        &mut self,
        channel_tx: &mut Sender<TcpBridgeToClientMessage>,
    ) -> Result<()> {
        let newblock = NewRenderBlock {
            id: self.enclosure_location.id,
            contents: RenderBlockDescription::MetaBox(self.enclosure_meta.clone()),
            update: true,
        };
        let send_msg = TcpBridgeToClientMessage {
            message: HelicoidToClientMessage {
                update: RemoteBoxUpdate {
                    parent: RenderBlockPath::top(),
                    new_render_blocks: smallvec![newblock],
                    remove_render_blocks: Default::default(),
                    move_block_locations: smallvec![self.enclosure_location.clone()],
                },
            },
        };
        log::trace!("Enclosure msg: {:?}", send_msg);
        channel_tx.send(send_msg).await?;
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
            let inner_editor_locked = state_data
                .compositor
                .as_ref()
                .unwrap()
                .content_visitor
                .editor()
                .clone();

            let inner_editor = inner_editor_locked.lock().await;
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

impl Compositor {
    /* This is running synchronously, and should not depend on any non cpu/gpu resources*/
    fn sync_screen(&mut self) -> anyhow::Result<()> {
        for (id, tree) in self.containers.iter_mut() {
            tree.update(&mut self.content_visitor);
            /*TODO: Retrieve the proper location and id from the enclosure, this is needed to be done
            if multiple enclosures in the same client / changing the location live is required */
            let mut loc = RenderBlockLocation {
                id: tree.top_container_id(),
                location: PointF32::default(),
                layer: 0,
            };
            tree.transfer_changes(
                &RenderBlockPath::new(smallvec![RenderBlockId(ENCLOSURE_ID)]),
                &mut loc,
                &mut self.client_messages_scratch,
            );
        }
        Ok(())
    }
    async fn transfer_messages_to_client(
        &mut self,
        channel_tx: &mut Sender<TcpBridgeToClientMessage>,
    ) -> anyhow::Result<()> {
        let scratch_len = self.client_messages_scratch.len();
        /* TOTO: Make channel accept more messages in one go to improve performance */
        for message in self.client_messages_scratch.drain(..) {
            log::trace!("Send message to client: {:?}", message);
            channel_tx
                .send(TcpBridgeToClientMessage {
                    message: HelicoidToClientMessage { update: message },
                })
                .await?;
        }
        /* Avoid having to incrementally grow the scratch vector again */
        self.client_messages_scratch.reserve(scratch_len);
        Ok(())
    }
    pub fn containers(&self) -> &HashMap<RenderBlockId, EditorTree> {
        &self.containers
    }
}
