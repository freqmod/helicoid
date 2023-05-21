use crate::{center::CenterModel, editor::Editor as HcEditor, statusline::StatusLineModel};
use hashbrown::HashMap;
use helicoid_protocol::{
    caching_shaper::CachingShaper,
    gfx::{
        FontPaint, HelicoidToClientMessage, MetaDrawBlock, NewRenderBlock, PathVerb, PointF16,
        PointF32, PointU32, RemoteBoxUpdate, RemoteSingleChange, RenderBlockDescription,
        RenderBlockId, RenderBlockLocation, RenderBlockPath, SimpleDrawBlock, SimpleDrawElement,
        SimpleDrawPath, SimpleDrawPolygon, SimplePaint, SimpleRoundRect, SimpleSvg,
    },
    input::{
        CursorMovedEvent, HelicoidToServerMessage, ImeEvent, KeyModifierStateUpdateEvent,
        MouseButtonStateChangeEvent, SimpleKeyTappedEvent, ViewportInfo, VirtualKeycode,
    },
    shadowblocks::{
        ContainerBlockLogic, NoContainerBlockLogic, ShadowMetaBlock, ShadowMetaContainerBlock,
        ShadowMetaContainerBlockInner, ShadowMetaTextBlock, VisitingContext,
    },
    tcp_bridge::{
        TcpBridgeServer, TcpBridgeServerConnectionState, TcpBridgeToClientMessage,
        TcpBridgeToServerMessage,
    },
    text::{FontEdging, FontHinting, ShapableString},
    transferbuffer::TransferBuffer,
};
use helix_lsp::lsp::DiagnosticSeverity;
use helix_view::{
    document::Mode, editor::StatusLineElement, Document, DocumentId, Editor, View, ViewId,
};
use ordered_float::OrderedFloat;
use std::{
    hash::{BuildHasher, Hash, Hasher},
    sync::Arc,
};
use swash::Metrics;
use tokio::sync::MutexGuard;

const EDITOR_CHILD_CENTER: u16 = 0x10;
const EDITOR_CHILD_HEADER: u16 = 0x11;
const EDITOR_CHILD_STATUSLINE: u16 = 0x12;
const EDITOR_CHILD_LEFT: u16 = 0x13;
const EDITOR_CHILD_RIGHT: u16 = 0x14;

trait RenderContext {
    fn shaper(&mut self) -> &mut CachingShaper;
}

trait GfxComposibleBlock: Hash + PartialEq {
    fn extent(&self) -> PointU32;
    fn set_layout(&mut self, scale: SizeScale, extent: PointU32);
    fn render(&mut self, context: &mut dyn RenderContext);
}
#[derive(Hash, PartialEq, Clone)]
struct SizeScale {
    line_height: OrderedFloat<f32>,
}
/*#[derive(Clone, Copy, Default)]
pub struct ActiveIds{
    pub document: DocumentId,
    pub view: ViewId,
}*/
pub struct ContentDocContainer<'a> {
    editor: MutexGuard<'a, HcEditor>,
    view_id: ViewId,
}
impl ContentDocContainer<'_> {
    pub fn editor(&self) -> &HcEditor {
        &self.editor
    }
    pub fn editor_mut(&mut self) -> &mut HcEditor {
        &mut self.editor
    }
    pub fn view_id(&self) -> ViewId {
        self.view_id
    }
    pub fn view(&self) -> &View {
        self.editor.editor().tree.get(self.view_id)
    }
    pub fn view_mut(&mut self) -> &mut View {
        self.editor.editor_mut().tree.get_mut(self.view_id)
    }
    pub fn document(&self) -> Option<&Document> {
        let view = self.editor.editor().tree.get(self.view_id);
        self.editor.editor().document(view.doc)
    }
    pub fn destruct(&self) -> (&HcEditor, &View, Option<&Document>) {
        let view = self.editor.editor().tree.get(self.view_id);
        let document = self.editor.editor().document(view.doc);
        (&self.editor, view, document)
    }
    pub fn document_mut(&mut self) -> Option<&mut Document> {
        let view = self.editor.editor().tree.get(self.view_id);
        let doc_id = view.doc.clone();
        self.editor.editor_mut().document_mut(doc_id)
    }
    pub fn destruct_mut(&mut self) -> (&mut View, Option<&mut Document>) {
        let hxeditor = self.editor.editor_mut();
        let view = hxeditor.tree.get_mut(self.view_id);
        let document = hxeditor.documents.get_mut(&view.doc);
        (view, document)
    }
}
pub struct ContentVisitor {
    shaper: CachingShaper,
    scale: SizeScale,
    editor: Arc<tokio::sync::Mutex<HcEditor>>,
    active_view_id: Option<ViewId>,
}
impl ContentVisitor {
    pub fn new(
        line_height: f32,
        shaper: CachingShaper,
        editor: Arc<tokio::sync::Mutex<HcEditor>>,
    ) -> Self {
        Self {
            editor,
            shaper,
            scale: SizeScale {
                line_height: OrderedFloat(line_height),
            },
            active_view_id: None,
        }
    }
    pub fn shaper(&mut self) -> &mut CachingShaper {
        &mut self.shaper
    }
    pub fn shaper_ref(&self) -> &CachingShaper {
        &self.shaper
    }
    pub fn editor(&self) -> &Arc<tokio::sync::Mutex<HcEditor>> {
        &self.editor
    }
    pub fn current_doc(&self) -> Option<ContentDocContainer<'_>> {
        if let Some(current_view_id) = self.active_view_id {
            Some(ContentDocContainer {
                editor: self.editor().blocking_lock(),
                view_id: current_view_id,
            })
        } else {
            None
        }
    }
    /* Get both current document and shaper at once.
    Needed as a hackish way of satisfying the borrow checker */
    pub fn doc_and_shaper(&mut self) -> (Option<ContentDocContainer<'_>>, &mut CachingShaper) {
        (
            if let Some(current_view_id) = self.active_view_id {
                Some(ContentDocContainer {
                    editor: self.editor.blocking_lock(),
                    view_id: current_view_id,
                })
            } else {
                None
            },
            &mut self.shaper,
        )
    }
    pub fn active_view_id(&self) -> Option<ViewId> {
        self.active_view_id
    }

    #[cfg(test)]
    pub fn set_active_view_id(&mut self, view_id: Option<ViewId>) {
        self.active_view_id = view_id
    }
}

impl VisitingContext for ContentVisitor {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
/* Top at the moment is not in use */
#[derive(Hash, PartialEq)]
struct EditorTop {
    block: ShadowMetaContainerBlock<NoContainerBlockLogic<ContentVisitor>, ContentVisitor>,
    scale: SizeScale,
}

#[derive(Hash, PartialEq)]
struct LeftGutter {
    block: ShadowMetaContainerBlock<NoContainerBlockLogic<ContentVisitor>, ContentVisitor>,
    scale: SizeScale,
}
#[derive(Hash, PartialEq)]
struct RightGutter {
    block: ShadowMetaContainerBlock<NoContainerBlockLogic<ContentVisitor>, ContentVisitor>,
    scale: SizeScale,
}
#[derive(Hash, PartialEq)]
struct TopOverlay {
    block: ShadowMetaContainerBlock<NoContainerBlockLogic<ContentVisitor>, ContentVisitor>,
    scale: SizeScale,
}
#[derive(Hash, PartialEq)]
struct BottomOverlay {
    block: ShadowMetaContainerBlock<NoContainerBlockLogic<ContentVisitor>, ContentVisitor>,
    scale: SizeScale,
}
#[derive(Hash, PartialEq)]
struct LeftOverlay {
    block: ShadowMetaContainerBlock<NoContainerBlockLogic<ContentVisitor>, ContentVisitor>,
    scale: SizeScale,
}
#[derive(Hash, PartialEq)]
struct RightOverlay {
    block: ShadowMetaContainerBlock<NoContainerBlockLogic<ContentVisitor>, ContentVisitor>,
    scale: SizeScale,
}
#[derive(Hash, PartialEq)]
struct TopRightOverlay {
    block: ShadowMetaContainerBlock<NoContainerBlockLogic<ContentVisitor>, ContentVisitor>,
    scale: SizeScale,
}

#[derive(Default, Hash, PartialEq)]
struct EditorTextArea {
    extent: PointU32,
}

/* TODO: Is this the right name for this class, it is mostly concerned with
layout of its subparts */
#[derive(Hash, PartialEq)]
struct EditorModel {
    /* The following will be mot up to date in the process visitor,
    but keep the last values here to detect lazy evaluations */
    scale: SizeScale, // Used for tracking changes
    extent: PointF32, // Used for tracking changes
    view_id: Option<ViewId>,
    //    main_font_metrics: Metrics,
    unscaled_font_average_width: OrderedFloat<f32>,
    unscaled_font_average_height: OrderedFloat<f32>,
    scaled_font_size: OrderedFloat<f32>,
    scale_factor: OrderedFloat<f32>,
}

pub struct EditorContainer {
    top: EditorTop,
    left: LeftGutter,
    right: RightGutter, // Scrollbar, minimap etc.
    /*    top_overlay: TopOverlay,
    bottom_overlay: BottomOverlay,
    left_overlay: LeftOverlay,
    right_overlay: RightOverlay,
    topright_overlay: TopRightOverlay,*/
    center_text: EditorTextArea,
    model: EditorModel,
}

pub struct EditorTree {
    root: ShadowMetaContainerBlock<EditorModel, ContentVisitor>,
    path: RenderBlockPath,
}

impl EditorModel {
    /* Updates layout sizes of the different elements, should only be
    called if the scale or external extent of the editor has changed */
    fn layout(
        outer_block: &mut ShadowMetaContainerBlock<Self, ContentVisitor>,
        context: &mut ContentVisitor,
    ) {
        let (block, model) = outer_block.destruct_mut();
        log::info!("Editor top level layout with extent: {:?}", model.extent);
        if let Some(mut header_block) = block.child_mut(RenderBlockId(EDITOR_CHILD_HEADER)) {
            let header_extent = header_block.block().extent_mut();
            *header_extent = PointF32::new(model.extent.x() as f32, header_extent.y());
        } else {
            log::info!("No header when laying out editor");
        }
        if let Some(mut statusline_block) = block.child_mut(RenderBlockId(EDITOR_CHILD_STATUSLINE))
        {
            let statusline_extent = statusline_block.block().extent_mut();
            *statusline_extent = PointF32::new(
                model.extent.x() as f32,
                f32::from(model.scaled_font_size) * 1.5f32,
            );
            statusline_block.location().location =
                PointF32::new(0f32, model.extent.y() - statusline_extent.y());
            let sl = statusline_block
                .block()
                .container_mut()
                .unwrap()
                .as_any_mut()
                .downcast_mut::<ShadowMetaContainerBlock<StatusLineModel, ContentVisitor>>()
                .unwrap()
                .logic_mut();
            sl.scaled_font_size = model.scaled_font_size;
        } else {
            log::info!("No right block when laying out editor");
        }
        if let Some(mut left_block) = block.child_mut(RenderBlockId(EDITOR_CHILD_LEFT)) {
            let left_extent = left_block.block().extent_mut();
            *left_extent = PointF32::new(
                (f32::from(model.font_average_width())
                    * (f32::from(model.font_line_height())
                        / f32::from(model.font_average_height()))) as f32,
                model.extent.y() as f32,
            );
        } else {
            log::info!("No left block when laying out editor");
        }
        if let Some(mut right_block) = block.child_mut(RenderBlockId(EDITOR_CHILD_RIGHT)) {
            let right_extent = right_block.block().extent_mut();
            *right_extent = PointF32::new(right_extent.x(), model.extent.y() as f32);
        } else {
            log::info!("No right block when laying out editor");
        }
        {
            let horizontal_remaining = (model.extent.y() as f32)
                - (block
                    .child(RenderBlockId(EDITOR_CHILD_HEADER))
                    .map(|b| b.0.extent().y())
                    .unwrap_or(0f32))
                - (block
                    .child(RenderBlockId(EDITOR_CHILD_STATUSLINE))
                    .map(|b| b.0.extent().y())
                    .unwrap_or(0f32));
            let vertical_remaining = (model.extent.x() as f32)
                - (block
                    .child(RenderBlockId(EDITOR_CHILD_RIGHT))
                    .map(|b| b.0.extent().x())
                    .unwrap_or(0f32))
                - (block
                    .child(RenderBlockId(EDITOR_CHILD_LEFT))
                    .map(|b| b.0.extent().x())
                    .unwrap_or(0f32));
            if let Some(mut center_block) = block.child_mut(RenderBlockId(EDITOR_CHILD_CENTER)) {
                let center_extent = center_block.block().extent_mut();
                *center_extent =
                    PointF32::new(vertical_remaining.max(0f32), horizontal_remaining.max(0f32));
            } else {
                log::info!("No center block when laying out editor");
            }
        }
    }
}
impl ContainerBlockLogic for EditorModel {
    type UpdateContext = ContentVisitor;
    fn pre_update(
        outer_block: &mut ShadowMetaContainerBlock<Self, ContentVisitor>,
        context: &mut Self::UpdateContext,
    ) where
        Self: Sized,
    {
        let (block, model) = outer_block.destruct_mut();
        debug_assert!(context.active_view_id.is_none());
        context.active_view_id = Some(model.view_id.unwrap());
        if block.extent() != model.extent || model.scale != context.scale {
            model.extent = block.extent();
            model.scale = context.scale.clone();
            Self::layout(outer_block, context);
        }
    }

    fn post_update(
        block: &mut ShadowMetaContainerBlock<Self, ContentVisitor>,
        context: &mut Self::UpdateContext,
    ) where
        Self: Sized,
    {
        context.active_view_id = None;
    }
    fn initialize(
        block: &mut ShadowMetaContainerBlock<Self, Self::UpdateContext>,
        context: &mut Self::UpdateContext,
    ) where
        Self: Sized,
    {
        let (block_inner, logic) = block.destruct_mut();
        /*        let top_model =
        EditorTop {
            scale: logic.scale.clone(),
            block: ShadowMetaContainerBlock::<
                NoContainerBlockLogic<ContentVisitor>,
                ContentVisitor,
            >::new(
                RenderBlockId::normal(10).unwrap(),
                PointF32::default(),
                false,
                None,
                Default::default(),
           ),
        };*/
        let mut statusline_model = StatusLineModel::default();
        statusline_model.scaled_font_size = logic.scaled_font_size;
        let statusline_block = ShadowMetaContainerBlock::new(
            RenderBlockId(EDITOR_CHILD_STATUSLINE),
            PointF32::default(),
            true,
            None,
            statusline_model,
        );
        block_inner.set_child(
            RenderBlockLocation {
                id: RenderBlockId(EDITOR_CHILD_STATUSLINE),
                location: PointF32::default(),
                layer: 0,
            },
            ShadowMetaBlock::WrappedContainer(Box::new(statusline_block)),
        );
        block_inner.set_child(
            RenderBlockLocation {
                id: RenderBlockId(EDITOR_CHILD_HEADER),
                location: PointF32::default(),
                layer: 0,
            },
            ShadowMetaBlock::WrappedContainer(Box::new(ShadowMetaContainerBlock::new(
                RenderBlockId(EDITOR_CHILD_HEADER),
                PointF32::default(),
                false,
                None,
                NoContainerBlockLogic::default(),
            ))),
        );
        block_inner.set_child(
            RenderBlockLocation {
                id: RenderBlockId(EDITOR_CHILD_LEFT),
                location: PointF32::default(),
                layer: 0,
            },
            ShadowMetaBlock::WrappedContainer(Box::new(ShadowMetaContainerBlock::new(
                RenderBlockId(EDITOR_CHILD_LEFT),
                PointF32::default(),
                false,
                None,
                NoContainerBlockLogic::default(),
            ))),
        );
        block_inner.set_child(
            RenderBlockLocation {
                id: RenderBlockId(EDITOR_CHILD_RIGHT),
                location: PointF32::default(),
                layer: 0,
            },
            ShadowMetaBlock::WrappedContainer(Box::new(ShadowMetaContainerBlock::new(
                RenderBlockId(EDITOR_CHILD_RIGHT),
                PointF32::default(),
                false,
                None,
                NoContainerBlockLogic::default(),
            ))),
        );

        let mut center_model = CenterModel::default();
        block_inner.set_child(
            RenderBlockLocation {
                id: RenderBlockId(EDITOR_CHILD_CENTER),
                location: PointF32::default(),
                layer: 0,
            },
            ShadowMetaBlock::WrappedContainer(Box::new(ShadowMetaContainerBlock::new(
                RenderBlockId(EDITOR_CHILD_CENTER),
                PointF32::default(),
                false,
                None,
                center_model,
            ))),
        );
    }
}

impl From<SizeScale> for f32 {
    fn from(value: SizeScale) -> Self {
        f32::from(value.line_height)
    }
}
impl From<SizeScale> for u32 {
    fn from(value: SizeScale) -> Self {
        f32::from(value.line_height) as u32
    }
}

impl SizeScale {
    fn round_up(&self) -> u32 {
        self.line_height.ceil() as u32
    }
    fn round_down(&self) -> u32 {
        self.line_height.floor() as u32
    }
}

impl EditorTree {
    pub fn new(
        parent_path: RenderBlockPath,
        tree_id: RenderBlockId,
        line_height: f32,
        scale_factor: f32,
        font_info: Metrics,
        view_id: Option<ViewId>,
        extent: PointF32,
    ) -> Self {
        let mut editor_tree_logic = EditorModel::new(line_height, scale_factor, font_info);
        editor_tree_logic.view_id = view_id;
        let root = ShadowMetaContainerBlock::new(tree_id, extent, true, None, editor_tree_logic);
        Self {
            root,
            path: parent_path,
        }
    }
    pub fn initialize(&mut self, visitor: &mut ContentVisitor) {
        self.root.initialize(visitor);
    }
    pub fn update(&mut self, visitor: &mut ContentVisitor) {
        self.root.update(visitor);
    }
    pub fn transfer_changes(
        &mut self,
        parent_path: &RenderBlockPath,
        location: &mut RenderBlockLocation,
        transfer_buffer: &mut TransferBuffer,
    ) {
        self.root
            .inner_mut()
            .client_transfer_messages(&parent_path, location, transfer_buffer);
    }
    pub fn top_container_id(&self) -> RenderBlockId {
        self.root.inner_ref().id()
    }
    pub fn resize(&mut self, extent: PointF32, scale_factor: OrderedFloat<f32>) {
        self.root.logic_mut().set_scale_factor(scale_factor);
        self.root.set_extent(extent)
    }
    pub fn current_view_id(&self) -> Option<ViewId> {
        self.root.logic_ref().view_id
    }
}
impl EditorModel {
    fn new(line_height: f32, scale_factor: f32, font_info: Metrics) -> Self {
        let line_scale = SizeScale {
            line_height: OrderedFloat(line_height),
        };

        Self {
            scale: line_scale.clone(),
            extent: PointF32::default(),
            view_id: None,
            unscaled_font_average_width: OrderedFloat(font_info.average_width),
            unscaled_font_average_height: OrderedFloat(font_info.ascent + font_info.descent),
            scaled_font_size: OrderedFloat(line_height * scale_factor),
            scale_factor: OrderedFloat(scale_factor),
        }
    }
    fn set_scale_factor(&mut self, new_scale_factor: OrderedFloat<f32>) {
        self.scaled_font_size = self.scale.line_height * f32::from(new_scale_factor);
        self.scale_factor = new_scale_factor;
    }
    /* TODO: Consider if the proper font is required to recalculate metrics on scale */
    fn font_average_width(&self) -> OrderedFloat<f32> {
        self.unscaled_font_average_width * self.scale_factor
    }
    fn font_average_height(&self) -> OrderedFloat<f32> {
        self.unscaled_font_average_height * self.scale_factor
    }
    fn font_line_height(&self) -> OrderedFloat<f32> {
        self.scale.line_height * self.scale_factor
    }
    pub fn current_view_id(&self) -> Option<ViewId> {
        self.view_id
    }
}

/* This is obsolete, and will be migrated into the editor tree */
/*
impl EditorContainer {

    pub fn set_size(&mut self, extent: PointU32) {
        self.model.extent = extent;
        self.lay_out();
    }

    pub fn lay_out(&mut self) {
        //        let metrics = self.model.main_font_metrics;
        /* Updates layout sizes of the different elements */
        self.top.set_layout(
            self.model.scale.clone(),
            PointU32::new(self.model.extent.x(), 0),
        );
        self.bottom.set_layout(
            self.model.scale.clone(),
            PointU32::new(self.model.extent.x(), 0),
        );
        self.left.set_layout(
            self.model.scale.clone(),
            PointU32::new(
                (f32::from(self.model.font_average_width)
                    * (f32::from(self.model.scale.line_height)
                        / f32::from(self.model.font_average_height))) as u32,
                self.model.extent.y(),
            ),
        );
        self.right.set_layout(
            self.model.scale.clone(),
            PointU32::new(0, self.model.extent.y()),
        );
        let horizontal_left = self
            .model
            .extent
            .y()
            .saturating_sub(self.top.extent().y())
            .saturating_sub(self.bottom.extent().y());
        let vertical_left = self
            .model
            .extent
            .x()
            .saturating_sub(self.right.extent().x())
            .saturating_sub(self.left.extent().x());
        self.center_text.set_layout(
            self.model.scale.clone(),
            PointU32::new(horizontal_left, vertical_left),
        );
    }
    /* The helix editor logic works by calling update in a loop. This is
    a bit different than the lazy update logic of helicone. Update any
    intermediate structures every time this function is called, recalculate the
    appropriate hashes, and recompute (and transmit) the lazy loaded data if the
    hashes are different */
    pub fn update_state(&mut self, editor: &mut Editor) {
        if let Some(doc_id) = self.model.document_id {
            let doc = editor.document(doc_id);
            if let Some(doc) = doc {
                self.bottom.update_state(editor, doc);
            } else {
                /* If there are no document that matches the editor something
                should probably be done */
                panic!("This is not expected to happen");
            }
        }
    }
}
*/
impl GfxComposibleBlock for EditorTop {
    fn extent(&self) -> PointU32 {
        PointU32::default()
    }
    fn set_layout(&mut self, scale: SizeScale, extent: PointU32) {
        /* Only the external width is used, for height we use line height * 1.5
        to have space for aline and some decoration. */
        self.block.set_extent(PointF32::new(
            extent.x() as f32,
            scale.line_height.0 * 1.5f32,
        ));
    }

    fn render(&mut self, context: &mut dyn RenderContext) {}
}
impl GfxComposibleBlock for LeftGutter {
    fn extent(&self) -> PointU32 {
        PointU32::default()
    }
    fn set_layout(&mut self, scale: SizeScale, extent: PointU32) {
        self.block.set_extent(PointF32::new(
            extent.x() as f32 * 6f32, // Occupy 6 average width letters
            scale.line_height.0 * 1.5f32,
        ));
    }

    fn render(&mut self, context: &mut dyn RenderContext) {}
}
impl GfxComposibleBlock for RightGutter {
    fn extent(&self) -> PointU32 {
        PointU32::default()
    }
    fn set_layout(&mut self, scale: SizeScale, extent: PointU32) {
        self.block.set_extent(PointF32::new(
            extent.x() as f32 * 2.0f32, // Occupy 2 average width letters
            scale.line_height.0 * 1.5f32,
        ));
    }

    fn render(&mut self, context: &mut dyn RenderContext) {}
}
/*
impl GfxComposibleBlock for TopOverlay {
    fn extent(&self) -> PointU32 {
        PointU32::default()
    }
    fn set_layout(&mut self, scale: SizeScale, extent: PointU32) {}
}
impl GfxComposibleBlock for BottomOverlay {
    fn extent(&self) -> PointU32 {
        PointU32::default()
    }
    fn set_layout(&mut self, scale: SizeScale, extent: PointU32) {}
}
impl GfxComposibleBlock for LeftOverlay {
    fn extent(&self) -> PointU32 {
        PointU32::default()
    }
    fn set_layout(&mut self, scale: SizeScale, extent: PointU32) {}
}
impl GfxComposibleBlock for RightOverlay {
    fn extent(&self) -> PointU32 {
        PointU32::default()
    }
    fn set_layout(&mut self, scale: SizeScale, extent: PointU32) {}
}
impl GfxComposibleBlock for TopRightOverlay {
    fn extent(&self) -> PointU32 {
        PointU32::default()
    }
    fn set_layout(&mut self, scale: SizeScale, extent: PointU32) {}
}*/
impl GfxComposibleBlock for EditorTextArea {
    fn extent(&self) -> PointU32 {
        self.extent
    }
    fn set_layout(&mut self, scale: SizeScale, extent: PointU32) {
        self.extent = extent;
    }

    fn render(&mut self, context: &mut dyn RenderContext) {}
}
