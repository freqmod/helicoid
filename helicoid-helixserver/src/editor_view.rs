use hashbrown::HashMap;
use helicoid_protocol::{
    caching_shaper::CachingShaper,
    dataflow::{ShadowMetaBlock, ShadowMetaContainerBlock, ShadowMetaTextBlock},
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
use helix_view::{document::Mode, editor::StatusLineElement, Document, DocumentId, Editor};
use ordered_float::OrderedFloat;
use std::hash::{BuildHasher, Hash, Hasher};
use swash::Metrics;

/* Seeds for hashes: The hashes should stay consistent so we can compare them */
const S1: u64 = 0x1199AACCDD114773;
const S2: u64 = 0x99AACCDD11779611;
const S3: u64 = 0xAACCDD1177667199;
const S4: u64 = 0xCCDD117766A0CE7D;

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
/* Top at the moment is not in use */
#[derive(Hash, PartialEq)]
struct EditorTop {
    block: ShadowMetaContainerBlock,
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
struct Statusline {
    block: ShadowMetaContainerBlock,
    scale: SizeScale,

    model: StatusLineModel,
}
#[derive(Hash, PartialEq)]
struct LeftGutter {
    block: ShadowMetaContainerBlock,
    scale: SizeScale,
}
#[derive(Hash, PartialEq)]
struct RightGutter {
    block: ShadowMetaContainerBlock,
    scale: SizeScale,
}
#[derive(Hash, PartialEq)]
struct TopOverlay {
    block: ShadowMetaContainerBlock,
    scale: SizeScale,
}
#[derive(Hash, PartialEq)]
struct BottomOverlay {
    block: ShadowMetaContainerBlock,
    scale: SizeScale,
}
#[derive(Hash, PartialEq)]
struct LeftOverlay {
    block: ShadowMetaContainerBlock,
    scale: SizeScale,
}
#[derive(Hash, PartialEq)]
struct RightOverlay {
    block: ShadowMetaContainerBlock,
    scale: SizeScale,
}
#[derive(Hash, PartialEq)]
struct TopRightOverlay {
    block: ShadowMetaContainerBlock,
    scale: SizeScale,
}

#[derive(Default, Hash, PartialEq)]
struct EditorTextArea {
    extent: PointU32,
}

struct EditorModel {
    scale: SizeScale, // Size of a line, in native pixels
    extent: PointU32, // In native pixels, whatever that is
    document_id: Option<DocumentId>,
    main_font_metrics: Metrics,
}
pub struct EditorContainer {
    top: EditorTop,
    bottom: Statusline,
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

impl EditorContainer {
    pub fn new(line_height: f32, font_info: Metrics) -> Self {
        let line_scale = SizeScale {
            line_height: OrderedFloat(line_height),
        };

        Self {
            model: EditorModel {
                scale: line_scale.clone(),
                extent: PointU32::default(),
                document_id: None,
                main_font_metrics: font_info,
            },
            top: EditorTop {
                scale: line_scale.clone(),
                block: ShadowMetaContainerBlock::new(
                    RenderBlockId::normal(10).unwrap(),
                    PointF16::default(),
                    false,
                    None,
                ),
            },
            bottom: Statusline::new(line_scale.clone()),
            left: LeftGutter {
                scale: line_scale.clone(),
                block: ShadowMetaContainerBlock::new(
                    RenderBlockId::normal(12).unwrap(),
                    PointF16::default(),
                    false,
                    None,
                ),
            },
            right: RightGutter {
                scale: line_scale.clone(),
                block: ShadowMetaContainerBlock::new(
                    RenderBlockId::normal(13).unwrap(),
                    PointF16::default(),
                    false,
                    None,
                ),
            },
            /*            top_overlay: TopOverlay {},
            bottom_overlay: BottomOverlay {},
            left_overlay: LeftOverlay {},
            right_overlay: RightOverlay {},-
            topright_overlay: TopRightOverlay {},*/
            center_text: EditorTextArea::default(),
        }
    }

    pub fn set_size(&mut self, extent: PointU32) {
        self.model.extent = extent;
        self.lay_out();
    }

    pub fn lay_out(&mut self) {
        let metrics = self.model.main_font_metrics;
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
                (metrics.average_width
                    * (f32::from(self.model.scale.line_height)
                        / (metrics.ascent + metrics.descent))) as u32,
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

const STATUSLINE_CHILD_ID_LEFT: u16 = 0x10;
const STATUSLINE_CHILD_ID_CENTER: u16 = 0x11;
const STATUSLINE_CHILD_ID_RIGHT: u16 = 0x12;
const UNNAMED_NAME: &str = "<Not saved>";
impl Statusline {
    fn new(line_scale: SizeScale) -> Self {
        let mut sl = Self {
            scale: line_scale,
            block: ShadowMetaContainerBlock::new(
                RenderBlockId::normal(11).unwrap(),
                PointF16::default(),
                false,
                None,
            ),
            model: StatusLineModel::default(),
        };
        sl.init_layout();
        sl
    }
    fn init_layout(&mut self) {
        self.block.set_child(
            RenderBlockLocation {
                id: RenderBlockId(STATUSLINE_CHILD_ID_LEFT),
                location: PointF16::default(),
                layer: 0,
            },
            ShadowMetaBlock::Text(ShadowMetaTextBlock::new()),
        );
        self.block.set_child(
            RenderBlockLocation {
                id: RenderBlockId(STATUSLINE_CHILD_ID_CENTER),
                location: PointF16::default(),
                layer: 0,
            },
            ShadowMetaBlock::Text(ShadowMetaTextBlock::new()),
        );
        self.block.set_child(
            RenderBlockLocation {
                id: RenderBlockId(STATUSLINE_CHILD_ID_RIGHT),
                location: PointF16::default(),
                layer: 0,
            },
            ShadowMetaBlock::Text(ShadowMetaTextBlock::new()),
        );
    }
    fn render_mode(editor: &Editor, out_string: &mut ShapableString) {
        match editor.mode {
            Mode::Normal => out_string.push_plain_str(" 󰄮 "),
            Mode::Select => out_string.push_plain_str(" 󰒅 "),
            Mode::Insert => out_string.push_plain_str(" 󰫙 "),
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
            out_string.push_plain_str(format!("{}  ", warnings).as_str());
        }

        if errors > 0 {
            out_string.push_plain_str(format!("{}  ", errors).as_str());
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
            out_string.push_plain_str(format!("󰪏: ").as_str());
        }
        if warnings > 0 {
            out_string.push_plain_str(format!("{}  ", warnings).as_str());
        }

        if errors > 0 {
            out_string.push_plain_str(format!("{}  ", errors).as_str());
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
                StatusLineElement::Spinner => out_string.push_plain_str("  "),
                /* TODO: Currently FileName and File BaseName is not distinguished, we prbably want to do that */
                StatusLineElement::FileName | StatusLineElement::FileBaseName => {
                    if let Some(path_buf) = doc.relative_path() {
                        if let Ok(path_str) = path_buf.into_os_string().into_string() {
                            out_string.push_plain_str(&path_str);
                        }
                    } else {
                        out_string.push_plain_str(UNNAMED_NAME);
                    }
                }
                StatusLineElement::FileModificationIndicator => {
                    if doc.is_modified() {
                        out_string.push_plain_str("  ");
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
        Self::render_elements(&status_config.left, editor, document, &mut self.model.left);
        Self::render_elements(
            &status_config.center,
            editor,
            document,
            &mut self.model.center,
        );
        Self::render_elements(
            &status_config.right,
            editor,
            document,
            &mut self.model.right,
        );
    }
    fn hash_state(&self) -> u64 {
        let mut hasher =
            ahash::random_state::RandomState::with_seeds(S1, S2, S3, S4).build_hasher();
        self.model.left.hash(&mut hasher);
        self.model.center.hash(&mut hasher);
        self.model.right.hash(&mut hasher);
        hasher.finish()
    }
}
impl GfxComposibleBlock for Statusline {
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

    fn render(&mut self, context: &mut dyn RenderContext) {
        /* Update meta shadow block based on any changes to local data / model */
        // Currently the status line block consists of 3 shapable strings for
        // left (0x10), center(0x11) and right (0x12), at layer 0x10.
        let current_state = self.hash_state();
        let skip_render = if let Some(rendered_state) = self.model.src_hash {
            if rendered_state == current_state {
                true
            } else {
                false
            }
        } else {
            false
        };
        if !skip_render {
            let string_to_shape = &self.model.left;
            let shaper = context.shaper();
            // TODO: The shaper should be shared, maybe some kind of render context?
            let mut shaped = shaper.shape(&string_to_shape, &None);
            //        let mut new_render_blocks = SmallVec::with_capacity(1);
            let new_shaped_string_block = NewRenderBlock {
                id: RenderBlockId::normal(1000).unwrap(),
                contents: RenderBlockDescription::ShapedTextBlock(shaped),
            };
        }
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
