use crate::{
    constants::{DEFAULT_TEXT_COLOR, S1, S2, S3, S4},
    editor::Editor as HcEditor,
    editor_view::ContentVisitor,
};
use hashbrown::HashMap;
use helicoid_protocol::{
    caching_shaper::CachingShaper,
    gfx::{
        FontPaint, HelicoidToClientMessage, MetaDrawBlock, NewRenderBlock, PathVerb, PointF16,
        PointF32, PointU32, RemoteBoxUpdate, RenderBlockDescription, RenderBlockId,
        RenderBlockLocation, RenderBlockPath, SimpleDrawBlock, SimpleDrawElement, SimpleDrawPath,
        SimpleDrawPolygon, SimplePaint, SimpleRoundRect, SimpleSvg,
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

/* How should we organise the status line, helix view has a very string based approach
while it would be nice with a bit more semantics here to enable more fancy graphics
(e.g. for file edited state) */

/* Currently make a text based status line, to be refactored with more fancy graphics at a later
time (possibly together with helix-view). A special symbol font is used to be able to render
relatively fancy graphics using text shaping engine. */
#[derive(Hash, PartialEq, Default)]
pub struct StatusLineModel {
    left: ShapableString,
    center: ShapableString,
    right: ShapableString,
    pub(crate) scaled_font_size: OrderedFloat<f32>,
    cfg_hash: Option<u64>,
    src_hash: Option<u64>,
    last_frame_time: Option<u32>,
    next_frame_time: Option<u32>,
}
impl StatusLineModel {
    fn render_mode(font_size: f32, editor: &Editor, out_string: &mut ShapableString) {
        match editor.mode {
            Mode::Normal => out_string.push_plain_str(" 󰄮 ", DEFAULT_TEXT_COLOR, font_size),
            Mode::Select => out_string.push_plain_str(" 󰒅 ", DEFAULT_TEXT_COLOR, font_size),
            Mode::Insert => out_string.push_plain_str(" 󰫙 ", DEFAULT_TEXT_COLOR, font_size),
        };
    }

    fn render_diagnostics(font_size: f32, doc: &Document, out_string: &mut ShapableString) {
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
            out_string.push_plain_str(
                format!("{} W ", warnings).as_str(),
                DEFAULT_TEXT_COLOR,
                font_size,
            );
        }

        if errors > 0 {
            out_string.push_plain_str(
                format!("{} E ", errors).as_str(),
                DEFAULT_TEXT_COLOR,
                font_size,
            );
        }
    }
    fn render_workspace_diagnostics(
        font_size: f32,
        editor: &Editor,
        out_string: &mut ShapableString,
    ) {
        let (warnings, errors) =
            editor
                .diagnostics
                .values()
                .flatten()
                .fold((0, 0), |mut counts, (diag, num)| {
                    match diag.severity {
                        Some(DiagnosticSeverity::WARNING) => counts.0 += 1,
                        Some(DiagnosticSeverity::ERROR) | None => counts.1 += 1,
                        _ => {}
                    }
                    counts
                });
        if warnings > 0 || errors > 0 {
            out_string.push_plain_str(format!("󰪏: ").as_str(), DEFAULT_TEXT_COLOR, font_size);
        }
        if warnings > 0 {
            out_string.push_plain_str(
                format!("{}  ", warnings).as_str(),
                DEFAULT_TEXT_COLOR,
                font_size,
            );
        }

        if errors > 0 {
            out_string.push_plain_str(
                format!("{}  ", errors).as_str(),
                DEFAULT_TEXT_COLOR,
                font_size,
            );
        }
    }
    fn render_elements(
        font_size: f32,
        elements: &Vec<StatusLineElement>,
        editor: &Editor,
        doc: &Document,
        out_string: &mut ShapableString,
    ) {
        out_string.clear();
        for element in elements.iter() {
            match element {
                StatusLineElement::Mode => {
                    Self::render_mode(font_size, editor, out_string);
                }
                /* Currently no animations are implemented for the spinner */
                StatusLineElement::Spinner => {
                    out_string.push_plain_str(" L ", DEFAULT_TEXT_COLOR, font_size)
                }
                /* TODO: Currently FileName and File BaseName is not distinguished, we prbably want to do that */
                StatusLineElement::FileName | StatusLineElement::FileBaseName => {
                    if let Some(path_buf) = doc.relative_path() {
                        if let Ok(path_str) = path_buf.into_os_string().into_string() {
                            out_string.push_plain_str(&path_str, DEFAULT_TEXT_COLOR, font_size);
                        }
                    } else {
                        out_string.push_plain_str(UNNAMED_NAME, DEFAULT_TEXT_COLOR, font_size);
                    }
                }
                StatusLineElement::FileModificationIndicator => {
                    if doc.is_modified() {
                        out_string.push_plain_str("  ", DEFAULT_TEXT_COLOR, font_size);
                    } else {
                    }
                }
                StatusLineElement::Diagnostics => {
                    Self::render_diagnostics(font_size, doc, out_string);
                }
                StatusLineElement::WorkspaceDiagnostics => {
                    Self::render_workspace_diagnostics(font_size, editor, out_string);
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
        let font_size = f32::from(self.scaled_font_size);
        Self::render_elements(
            font_size,
            &status_config.left,
            editor,
            document,
            &mut self.left,
        );
        Self::render_elements(
            font_size,
            &status_config.center,
            editor,
            document,
            &mut self.center,
        );
        Self::render_elements(
            font_size,
            &status_config.right,
            editor,
            document,
            &mut self.right,
        );
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
        log::trace!("Char width: {} Y: {}", char_width, line_y);
        let container_width = block.extent().x();

        let shaped = shaper.shape(string_to_shape, &None);
        let mut cblock_guard = block.child_mut(target_block_id).unwrap();
        let (cblock, cloc) = cblock_guard.destruct();
        let ctxt_block = cblock.text_mut().unwrap();
        let string_width = shaped.extent.x();
        ctxt_block.set_wire(shaped);

        match target_block_id.0 {
            STATUSLINE_CHILD_ID_LEFT => {
                cloc.location = PointF32::new(2f32 * char_width, line_y);
            }
            STATUSLINE_CHILD_ID_CENTER => {
                let block_center = container_width / 2.0;
                let string_center = string_width / 2.0;
                cloc.location = PointF32::new(block_center - string_center, line_y);
            }
            STATUSLINE_CHILD_ID_RIGHT => {
                cloc.location =
                    PointF32::new(container_width - (2f32 * char_width) - string_width, line_y);
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
                let doc_container = context.current_doc().unwrap();
                Self::update_state(
                    model,
                    doc_container.editor().editor(),
                    doc_container.document().unwrap(),
                );
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
                location: PointF32::default(),
                layer: 0,
            },
            ShadowMetaBlock::Text(ShadowMetaTextBlock::new(RenderBlockId(
                STATUSLINE_CHILD_ID_LEFT,
            ))),
        );
        block.set_child(
            RenderBlockLocation {
                id: RenderBlockId(STATUSLINE_CHILD_ID_CENTER),
                location: PointF32::default(),
                layer: 0,
            },
            ShadowMetaBlock::Text(ShadowMetaTextBlock::new(RenderBlockId(
                STATUSLINE_CHILD_ID_CENTER,
            ))),
        );
        block.set_child(
            RenderBlockLocation {
                id: RenderBlockId(STATUSLINE_CHILD_ID_RIGHT),
                location: PointF32::default(),
                layer: 0,
            },
            ShadowMetaBlock::Text(ShadowMetaTextBlock::new(RenderBlockId(
                STATUSLINE_CHILD_ID_RIGHT,
            ))),
        );
    }
}
