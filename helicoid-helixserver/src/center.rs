use crate::{
    constants::{DEFAULT_TEXT_COLOR, S1, S2, S3, S4},
    editor::Editor as HcEditor,
    editor_view::ContentVisitor,
};
use hashbrown::HashMap;
use helicoid_protocol::{
    caching_shaper::CachingShaper,
    dataflow::{
        ContainerBlockLogic, NoContainerBlockLogic, ShadowMetaBlock, ShadowMetaContainerBlock,
        ShadowMetaContainerBlockInner, ShadowMetaTextBlock, VisitingContext,
    },
    gfx::{
        FontPaint, HelicoidToClientMessage, MetaDrawBlock, NewRenderBlock, PathVerb, PointF16,
        PointU32, RemoteBoxUpdate, RenderBlockDescription, RenderBlockId, RenderBlockLocation,
        RenderBlockPath, SimpleDrawBlock, SimpleDrawElement, SimpleDrawPath, SimpleDrawPolygon,
        SimplePaint, SimpleRoundRect, SimpleSvg,
    },
    input::{
        CursorMovedEvent, HelicoidToServerMessage, ImeEvent, KeyModifierStateUpdateEvent,
        MouseButtonStateChangeEvent, SimpleKeyTappedEvent, ViewportInfo, VirtualKeycode,
    },
    tcp_bridge::{
        TcpBridgeServer, TcpBridgeServerConnectionState, TcpBridgeToClientMessage,
        TcpBridgeToServerMessage,
    },
    text::{FontEdging, FontHinting, ShapableString},
};
use helix_lsp::lsp::DiagnosticSeverity;
use helix_view::{document::Mode, editor::StatusLineElement, Document, DocumentId, Editor, ViewId};
use ordered_float::OrderedFloat;
use std::{
    hash::{BuildHasher, Hash, Hasher},
    sync::Arc,
};
use swash::Metrics;

const STATUSLINE_CHILD_ID_LEFT: u16 = 0x10;
const STATUSLINE_CHILD_ID_CENTER: u16 = 0x11;
const STATUSLINE_CHILD_ID_RIGHT: u16 = 0x12;

const UNNAMED_NAME: &str = "<Not saved != a-> >";

pub type ParagraphId = u16;
#[derive(Hash, PartialEq, Default)]
pub struct ParagraphModel {}

/* How should we organise the status line, helix view has a very string based approach
while it would be nice with a bit more semantics here to enable more fancy graphics
(e.g. for file edited state) */

/* Currently make a text based status line, to be refactored with more fancy graphics at a later
time (possibly together with helix-view). A special symbol font is used to be able to render
relatively fancy graphics using text shaping engine. */
#[derive(Hash, PartialEq, Default)]
pub struct CenterModel {
    pub(crate) scaled_font_size: OrderedFloat<f32>,
    cfg_hash: Option<u64>,
    src_hash: Option<u64>,
    last_frame_time: Option<u32>,
    next_frame_time: Option<u32>,
    paragraphs: Vec<ParagraphId>,
}
impl ContainerBlockLogic for CenterModel {
    type UpdateContext = ContentVisitor;
    fn pre_update(
        outer_block: &mut ShadowMetaContainerBlock<Self, ContentVisitor>,
        context: &mut Self::UpdateContext,
    ) where
        Self: Sized,
    {
    }

    fn initialize(
        block: &mut ShadowMetaContainerBlock<Self, Self::UpdateContext>,
        context: &mut Self::UpdateContext,
    ) where
        Self: Sized,
    {
    }

    fn post_update(
        block: &mut ShadowMetaContainerBlock<Self, Self::UpdateContext>,
        context: &mut Self::UpdateContext,
    ) where
        Self: Sized,
    {
    }
}
