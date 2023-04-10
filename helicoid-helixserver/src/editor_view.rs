use crate::editor::Editor as HcEditor;
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

const UNNAMED_NAME: &str = "<Not saved>";

/* Seeds for hashes: The hashes should stay consistent during program execution,
so we can compare them */
const S1: u64 = 0x1199AACCDD114773;
const S2: u64 = 0x99AACCDD11779611;
const S3: u64 = 0xAACCDD1177667199;
const S4: u64 = 0xCCDD117766A0CE7D;

const EDITOR_CHILD_CENTER: u16 = 0x10;
const EDITOR_CHILD_HEADER: u16 = 0x11;
const EDITOR_CHILD_STATUSLINE: u16 = 0x12;
const EDITOR_CHILD_LEFT: u16 = 0x13;
const EDITOR_CHILD_RIGHT: u16 = 0x14;

const STATUSLINE_CHILD_ID_LEFT: u16 = 0x10;
const STATUSLINE_CHILD_ID_CENTER: u16 = 0x11;
const STATUSLINE_CHILD_ID_RIGHT: u16 = 0x12;

const DEFAULT_TEXT_COLOR: u32 = 0xFFFFFF;
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
    pub fn editor(&self) -> &Arc<tokio::sync::Mutex<HcEditor>> {
        &self.editor
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

/* How should we organise the status line, helix view has a very string based approach
while it would be nice with a bit more semantics here to enable more fancy graphics
(e.g. for file edited state) */

/* Currently make a text based status line, to be refactored with more fancy graphics at a later
time (possibly together with helix-view). A special symbol font is used to be able to render
relatively fancy graphics using text shaping engine. */
#[derive(Hash, PartialEq, Default)]
struct StatusLineModel {
    left: ShapableString,
    center: ShapableString,
    right: ShapableString,
    cfg_hash: Option<u64>,
    src_hash: Option<u64>,
    last_frame_time: Option<u32>,
    next_frame_time: Option<u32>,
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
    extent: PointF16, // Used for tracking changes
    view_id: Option<ViewId>,
    //    main_font_metrics: Metrics,
    font_average_width: OrderedFloat<f32>,
    font_average_height: OrderedFloat<f32>,
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
        if let Some(mut header_block) = block.child_mut(RenderBlockId(EDITOR_CHILD_HEADER)) {
            let header_extent = header_block.block().extent_mut();
            *header_extent = PointF16::new(model.extent.x() as f32, header_extent.y());
        } else {
            log::info!("No header when laying out editor");
        }
        if let Some(mut statusline_block) = block.child_mut(RenderBlockId(EDITOR_CHILD_STATUSLINE))
        {
            let statusline_extent = statusline_block.block().extent_mut();
            *statusline_extent = PointF16::new(model.extent.x() as f32, statusline_extent.y());
        } else {
            log::info!("No right block when laying out editor");
        }
        if let Some(mut left_block) = block.child_mut(RenderBlockId(EDITOR_CHILD_LEFT)) {
            let left_extent = left_block.block().extent_mut();
            *left_extent = PointF16::new(
                (f32::from(model.font_average_width)
                    * (f32::from(model.scale.line_height) / f32::from(model.font_average_height)))
                    as f32,
                model.extent.y() as f32,
            );
        } else {
            log::info!("No left block when laying out editor");
        }
        if let Some(mut right_block) = block.child_mut(RenderBlockId(EDITOR_CHILD_RIGHT)) {
            let right_extent = right_block.block().extent_mut();
            *right_extent = PointF16::new(right_extent.x(), model.extent.y() as f32);
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
                    PointF16::new(horizontal_remaining.max(0f32), vertical_remaining.max(0f32));
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
            model.extent = model.extent;
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
                PointF16::default(),
                false,
                None,
                Default::default(),
           ),
        };*/
        let statusline_model = StatusLineModel::default();
        let statusline_block = ShadowMetaContainerBlock::new(
            RenderBlockId(EDITOR_CHILD_STATUSLINE),
            PointF16::default(),
            false,
            None,
            statusline_model,
        );
        block_inner.set_child(
            RenderBlockLocation {
                id: RenderBlockId(EDITOR_CHILD_STATUSLINE),
                location: PointF16::default(),
                layer: 0,
            },
            ShadowMetaBlock::WrappedContainer(Box::new(statusline_block)),
        );
        block_inner.set_child(
            RenderBlockLocation {
                id: RenderBlockId(EDITOR_CHILD_HEADER),
                location: PointF16::default(),
                layer: 0,
            },
            ShadowMetaBlock::WrappedContainer(Box::new(ShadowMetaContainerBlock::new(
                RenderBlockId(EDITOR_CHILD_HEADER),
                PointF16::default(),
                false,
                None,
                NoContainerBlockLogic::default(),
            ))),
        );
        block_inner.set_child(
            RenderBlockLocation {
                id: RenderBlockId(EDITOR_CHILD_LEFT),
                location: PointF16::default(),
                layer: 0,
            },
            ShadowMetaBlock::WrappedContainer(Box::new(ShadowMetaContainerBlock::new(
                RenderBlockId(EDITOR_CHILD_LEFT),
                PointF16::default(),
                false,
                None,
                NoContainerBlockLogic::default(),
            ))),
        );
        block_inner.set_child(
            RenderBlockLocation {
                id: RenderBlockId(EDITOR_CHILD_RIGHT),
                location: PointF16::default(),
                layer: 0,
            },
            ShadowMetaBlock::WrappedContainer(Box::new(ShadowMetaContainerBlock::new(
                RenderBlockId(EDITOR_CHILD_RIGHT),
                PointF16::default(),
                false,
                None,
                NoContainerBlockLogic::default(),
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
        tree_id: RenderBlockId,
        line_height: f32,
        font_info: Metrics,
        view_id: Option<ViewId>,
        extent: PointF16,
    ) -> Self {
        let mut editor_tree_logic = EditorModel::new(line_height, font_info);
        editor_tree_logic.view_id = view_id;
        let path = RenderBlockPath::new(smallvec::smallvec![tree_id]);
        let root = ShadowMetaContainerBlock::new(
            tree_id,
            extent,
            true,
            None,
            editor_tree_logic,
        );
        Self { root, path }
    }
    pub fn initialize(&mut self, visitor: &mut ContentVisitor) {
        self.root.initialize(visitor);
    }
    pub fn update(&mut self, visitor: &mut ContentVisitor) {
        self.root.update(visitor);
    }
    pub fn transfer_changes(&mut self, messages_vec: &mut Vec<RemoteBoxUpdate>) {
        self.root
            .inner_mut()
            .client_transfer_messages(&self.path, messages_vec);
    }
    pub fn top_container_id(&self) -> RenderBlockId{
        self.root.inner_ref().id()
    }
    pub fn resize(&mut self, extent: PointF16){
        self.root.set_extent(extent)
    }
}
impl EditorModel {
    fn new(line_height: f32, font_info: Metrics) -> Self {
        let line_scale = SizeScale {
            line_height: OrderedFloat(line_height),
        };

        Self {
            scale: line_scale.clone(),
            extent: PointF16::default(),
            view_id: None,
            font_average_width: OrderedFloat(font_info.average_width),
            font_average_height: OrderedFloat(font_info.ascent + font_info.descent),
        }
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
        self.block.set_extent(PointF16::new(
            extent.x() as f32,
            scale.line_height.0 * 1.5f32,
        ));
    }

    fn render(&mut self, context: &mut dyn RenderContext) {}
}
impl StatusLineModel {
    fn render_mode(editor: &Editor, out_string: &mut ShapableString) {
        match editor.mode {
            Mode::Normal => out_string.push_plain_str(" 󰄮 ", DEFAULT_TEXT_COLOR),
            Mode::Select => out_string.push_plain_str(" 󰒅 ", DEFAULT_TEXT_COLOR),
            Mode::Insert => out_string.push_plain_str(" 󰫙 ", DEFAULT_TEXT_COLOR),
        };
    }

    fn render_diagnostics(doc: &Document, out_string: &mut ShapableString) {
        let (warnings, errors) = doc.diagnostics().iter().fold((0, 0), |mut counts, diag| {
            use helix_core::diagnostic::Severity;
            match diag.severity {
                Some(Severity::Warning) => counts.0 += 1,
                Some(Severity::Error) | None => counts.1 += 1,
                _ => {}
            }
            counts
        });
        if warnings > 0 {
            out_string.push_plain_str(format!("{}  ", warnings).as_str(), DEFAULT_TEXT_COLOR);
        }

        if errors > 0 {
            out_string.push_plain_str(format!("{}  ", errors).as_str(), DEFAULT_TEXT_COLOR);
        }
    }
    fn render_workspace_diagnostics(editor: &Editor, out_string: &mut ShapableString) {
        let (warnings, errors) =
            editor
                .diagnostics
                .values()
                .flatten()
                .fold((0, 0), |mut counts, diag| {
                    match diag.severity {
                        Some(DiagnosticSeverity::WARNING) => counts.0 += 1,
                        Some(DiagnosticSeverity::ERROR) | None => counts.1 += 1,
                        _ => {}
                    }
                    counts
                });
        if warnings > 0 || errors > 0 {
            out_string.push_plain_str(format!("󰪏: ").as_str(), DEFAULT_TEXT_COLOR);
        }
        if warnings > 0 {
            out_string.push_plain_str(format!("{}  ", warnings).as_str(), DEFAULT_TEXT_COLOR);
        }

        if errors > 0 {
            out_string.push_plain_str(format!("{}  ", errors).as_str(), DEFAULT_TEXT_COLOR);
        }
    }
    fn render_elements(
        elements: &Vec<StatusLineElement>,
        editor: &Editor,
        doc: &Document,
        out_string: &mut ShapableString,
    ) {
        out_string.clear();
        for element in elements.iter() {
            match element {
                StatusLineElement::Mode => {
                    Self::render_mode(editor, out_string);
                }
                /* Currently no animations are implemented for the spinner */
                StatusLineElement::Spinner => out_string.push_plain_str("  ", DEFAULT_TEXT_COLOR),
                /* TODO: Currently FileName and File BaseName is not distinguished, we prbably want to do that */
                StatusLineElement::FileName | StatusLineElement::FileBaseName => {
                    if let Some(path_buf) = doc.relative_path() {
                        if let Ok(path_str) = path_buf.into_os_string().into_string() {
                            out_string.push_plain_str(&path_str, DEFAULT_TEXT_COLOR);
                        }
                    } else {
                        out_string.push_plain_str(UNNAMED_NAME, DEFAULT_TEXT_COLOR);
                    }
                }
                StatusLineElement::FileModificationIndicator => {
                    if doc.is_modified() {
                        out_string.push_plain_str("  ", DEFAULT_TEXT_COLOR);
                    } else {
                    }
                }
                StatusLineElement::Diagnostics => {
                    Self::render_diagnostics(doc, out_string);
                }
                StatusLineElement::WorkspaceDiagnostics => {
                    Self::render_workspace_diagnostics(editor, out_string);
                }
                StatusLineElement::FileEncoding => {}
                StatusLineElement::FileLineEnding => {}
                StatusLineElement::FileType => {}
                StatusLineElement::Selections => {}
                StatusLineElement::PrimarySelectionLength => {}
                StatusLineElement::Position => {}
                StatusLineElement::Separator => {}
                StatusLineElement::PositionPercentage => {}
                StatusLineElement::TotalLineNumbers => {}
                StatusLineElement::Spacer => {}
                StatusLineElement::VersionControl => {}
                StatusLineElement::WindowIdentifiers => {}
            }
        }
    }
    fn update_state(&mut self, editor: &Editor, document: &Document) {
        let status_config = &editor.config().statusline;
        Self::render_elements(&status_config.left, editor, document, &mut self.left);
        Self::render_elements(&status_config.center, editor, document, &mut self.center);
        Self::render_elements(&status_config.right, editor, document, &mut self.right);
    }
    fn hash_state(&self) -> u64 {
        let mut hasher =
            ahash::random_state::RandomState::with_seeds(S1, S2, S3, S4).build_hasher();
        self.left.hash(&mut hasher);
        self.center.hash(&mut hasher);
        self.right.hash(&mut hasher);
        hasher.finish()
    }
    fn render_string(
        string_to_shape: &ShapableString,
        target_block_id: RenderBlockId,
        block: &mut ShadowMetaContainerBlockInner<ContentVisitor>,
        context: &mut ContentVisitor,
    ) {
        let shaper = context.shaper();
        let char_width = string_to_shape
            .metadata_runs
            .first()
            .map(|meta| shaper.info(&meta.font_info).map(|i| i.1))
            .flatten()
            .unwrap_or(block.extent().y());
        let line_y = block.extent().y() / 6f32; /* Vert layout: 1/6 4/6 1/6 */
        let container_width = block.extent().x();

        let shaped = shaper.shape(string_to_shape, &None);
        let mut cblock_guard = block.child_mut(target_block_id).unwrap();
        let (cblock, cloc) = cblock_guard.destruct();
        let ctxt_block = cblock.text_mut().unwrap();
        let string_width = shaped.extent.x();
        ctxt_block.set_wire(shaped);

        match target_block_id.0 {
            STATUSLINE_CHILD_ID_LEFT => {
                cloc.location = PointF16::new(2f32 * char_width, line_y);
            }
            STATUSLINE_CHILD_ID_CENTER => {
                let block_center = container_width / 2.0;
                let string_center = string_width / 2.0;
                cloc.location = PointF16::new(block_center - string_center, line_y);
            }
            STATUSLINE_CHILD_ID_RIGHT => {
                cloc.location =
                    PointF16::new(container_width - (2f32 * char_width) - string_width, line_y);
            }
            _ => {}
        }
    }
}
impl ContainerBlockLogic for StatusLineModel {
    type UpdateContext = ContentVisitor;
    fn pre_update(
        outer_block: &mut ShadowMetaContainerBlock<Self, ContentVisitor>,
        context: &mut Self::UpdateContext,
    ) where
        Self: Sized,
    {
        let (block, model) = outer_block.destruct_mut();
        /* Update meta shadow block based on any changes to local data / model */
        // Currently the status line block consists of 3 shapable strings for
        // left (0x10), center(0x11) and right (0x12), at layer 0x10.
        let current_state = model.hash_state();
        let skip_render = if let Some(rendered_state) = model.src_hash {
            if rendered_state == current_state {
                true
            } else {
                false
            }
        } else {
            false
        };
        if !skip_render {
            {
                let current_view_id = context.active_view_id.unwrap();
                let editor = context.editor().blocking_lock();
                let view = editor.editor().tree.get(current_view_id);
                let document = editor.editor().document(view.doc).unwrap();
                Self::update_state(model, editor.editor(), document);
            }
            Self::render_string(
                &model.left,
                RenderBlockId(STATUSLINE_CHILD_ID_LEFT),
                block,
                context,
            );
            Self::render_string(
                &model.center,
                RenderBlockId(STATUSLINE_CHILD_ID_CENTER),
                block,
                context,
            );
            Self::render_string(
                &model.right,
                RenderBlockId(STATUSLINE_CHILD_ID_RIGHT),
                block,
                context,
            );
        }
        /* Who has the responsibility of syncing the shadow blocks with the server ??*/
    }

    fn post_update(
        block: &mut ShadowMetaContainerBlock<Self, ContentVisitor>,
        context: &mut Self::UpdateContext,
    ) where
        Self: Sized,
    {
    }
    fn initialize(
        outer_block: &mut ShadowMetaContainerBlock<Self, Self::UpdateContext>,
        _context: &mut Self::UpdateContext,
    ) where
        Self: Sized,
    {
        let block = outer_block.inner_mut();
        block.set_child(
            RenderBlockLocation {
                id: RenderBlockId(STATUSLINE_CHILD_ID_LEFT),
                location: PointF16::default(),
                layer: 0,
            },
            ShadowMetaBlock::Text(ShadowMetaTextBlock::new(RenderBlockId(STATUSLINE_CHILD_ID_LEFT))),
        );
        block.set_child(
            RenderBlockLocation {
                id: RenderBlockId(STATUSLINE_CHILD_ID_CENTER),
                location: PointF16::default(),
                layer: 0,
            },
            ShadowMetaBlock::Text(ShadowMetaTextBlock::new(RenderBlockId(STATUSLINE_CHILD_ID_CENTER))),
        );
        block.set_child(
            RenderBlockLocation {
                id: RenderBlockId(STATUSLINE_CHILD_ID_RIGHT),
                location: PointF16::default(),
                layer: 0,
            },
            ShadowMetaBlock::Text(ShadowMetaTextBlock::new(RenderBlockId(STATUSLINE_CHILD_ID_RIGHT))),
        );
    }
}

impl GfxComposibleBlock for LeftGutter {
    fn extent(&self) -> PointU32 {
        PointU32::default()
    }
    fn set_layout(&mut self, scale: SizeScale, extent: PointU32) {
        self.block.set_extent(PointF16::new(
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
        self.block.set_extent(PointF16::new(
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
