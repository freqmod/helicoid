use crate::{
    constants::{DEFAULT_TEXT_COLOR, S1, S2, S3, S4},
    editor::Editor as HcEditor,
    editor_view::ContentVisitor,
};
use ahash::AHasher;
use hashbrown::HashMap;
use helicoid_protocol::{
    caching_shaper::CachingShaper,
    dataflow::{
        ContainerBlockLogic, NoContainerBlockLogic, ShadowMetaBlock, ShadowMetaContainerBlock,
        ShadowMetaContainerBlockInner, ShadowMetaTextBlock, VisitingContext,
    },
    font_options::FontOptions,
    gfx::{
        FontPaint, HelicoidToClientMessage, MetaDrawBlock, NewRenderBlock, PathVerb, PointF16,
        PointU16, PointU32, RemoteBoxUpdate, RenderBlockDescription, RenderBlockId,
        RenderBlockLocation, RenderBlockPath, RenderBlockRemoveInstruction, SimpleDrawBlock,
        SimpleDrawElement, SimpleDrawPath, SimpleDrawPolygon, SimplePaint, SimpleRoundRect,
        SimpleSvg,
    },
    input::{
        CursorMovedEvent, HelicoidToServerMessage, ImeEvent, KeyModifierStateUpdateEvent,
        MouseButtonStateChangeEvent, SimpleKeyTappedEvent, ViewportInfo, VirtualKeycode,
    },
    tcp_bridge::{
        TcpBridgeServer, TcpBridgeServerConnectionState, TcpBridgeToClientMessage,
        TcpBridgeToServerMessage,
    },
    text::{
        FontEdging, FontHinting, FontParameters, ShapableString, ShapedStringMetadata,
        ShapedTextBlock, SmallFontOptions, SHAPABLE_STRING_ALLOC_LEN, SHAPABLE_STRING_ALLOC_RUNS,
    },
};
use helix_core::{
    doc_formatter::{DocumentFormatter, GraphemeSource, TextFormat},
    graphemes::Grapheme,
    str_utils::char_to_byte_idx,
    syntax::{Highlight, HighlightEvent},
    text_annotations::TextAnnotations,
    visual_offset_from_block, Position, RopeSlice,
};
use helix_lsp::lsp::DiagnosticSeverity;
use helix_view::{
    document::Mode, editor::StatusLineElement, graphics::UnderlineStyle, theme::Style,
    view::ViewPosition, Document, DocumentId, Editor, Theme, ViewId,
};
use ordered_float::OrderedFloat;
use rayon::prelude::*;
use smallvec::SmallVec;
use std::{
    hash::{BuildHasher, Hash, Hasher},
    sync::Arc,
};
use swash::Metrics;

const CENTER_PARAGRAPH_BASE: u16 = 0x1000;
const MAX_AGE: i16 = 10;
const DEFAULT_FONT_ID: u8 = 0;

pub type ParagraphId = u16;
#[derive(Hash, PartialEq, Default)]
pub struct Paragraph {
    data_hash: u64, /* Of the latest changed value, it is up to the model to make it synced with the client */
    last_modified: u16, /* Age counter when this paragraph was last changed, for cache eviction */
}
/* How should we organise the status line, helix view has a very string based approach
while it would be nice with a bit more semantics here to enable more fancy graphics
(e.g. for file edited state) */

#[derive(Hash, PartialEq, Clone)]
enum MaybeRenderedParagraph {
    Source(RenderParagraphSource),
    Rendered(ShapedTextBlock),
}
#[derive(Hash, PartialEq)]
struct RenderedParagraph {
    rendered_block: ShapedTextBlock,
}
#[derive(Hash, PartialEq, Clone)]
struct RenderParagraphSource {
    text: ShapableString,
    location: PointU16,
}

#[derive(Hash, PartialEq, Clone)]
struct RenderParagraph {
    contents: MaybeRenderedParagraph,
    location: RenderBlockLocation,
    //id: RenderBlockId,
    data_hash: u64, /* Of the latest changed value, it is up to the model to make it synced with the client */
    last_modified: u16, /* Age counter when this paragraph was last changed, for cache eviction */
}

/* Formatting information per font run */
#[derive(Hash, PartialEq, Default)]
struct LayoutStringMetadata {
    section_length: u16,
    style: Style,
}
#[derive(Hash, PartialEq, Default)]
struct LayoutParagraph {
    text: SmallVec<[u8; SHAPABLE_STRING_ALLOC_LEN]>, //text should always contain valid UTF-8?
    metadata_runs: SmallVec<[LayoutStringMetadata; SHAPABLE_STRING_ALLOC_RUNS]>,
    current_style: Style,
    substring_end: u16,
}
#[derive(Hash, PartialEq, Default)]
struct LayoutParagraphEntry {
    layout: LayoutParagraph,
    location: PointF16,
    client_location: Option<PointF16>,
    client_hash: Option<u64>,
    layout_hash: u64,
    rendered_id: Option<RenderBlockId>,
}

#[derive(Hash, PartialEq)]
enum RenderingParagraph {
    Source(RenderParagraph),
    Dest((ShapedTextBlock, RenderBlockLocation)),
}
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
    client_layout: Vec<LayoutParagraphEntry>,
    offline_layout: Vec<LayoutParagraphEntry>,
    rendered_paragraphs: Vec<Option<RenderParagraph>>,
    viewport: PointU32,
    current_generation: u16,
    col_offset: u32,
    tab: String,
}
impl RenderingParagraph {
    pub fn destination(&self) -> Option<&(ShapedTextBlock, RenderBlockLocation)> {
        if let Self::Dest(tuple) = self {
            Some(tuple)
        } else {
            None
        }
    }
    pub fn source(&self) -> Option<&RenderParagraph> {
        if let Self::Source(src) = self {
            Some(src)
        } else {
            None
        }
    }
}
impl MaybeRenderedParagraph {
    pub fn rendered(&self) -> Option<&ShapedTextBlock> {
        if let Self::Rendered(rendered) = self {
            Some(rendered)
        } else {
            None
        }
    }
    pub fn source(&self) -> Option<&RenderParagraphSource> {
        if let Self::Source(src) = self {
            Some(src)
        } else {
            None
        }
    }
}
impl LayoutParagraphEntry {
    /* Reuse a client entry from earlier, removing the context from the old entry */
    fn reuse(&mut self, other: &mut Self) {
        debug_assert!(self.layout_hash == other.layout_hash);
        self.rendered_id = other.rendered_id.take();
        self.client_hash = other.client_hash.take();
    }
    /* Returns a render block location to send to the client, unless the block
    is at the right location at the client already */
    fn new_location(&mut self) -> Option<RenderBlockLocation> {
        if self
            .client_location
            .map(|loc| loc != self.location)
            .unwrap_or(true)
        {
            Some(RenderBlockLocation {
                id: self.rendered_id.unwrap(),
                location: self.location,
                layer: 0,
            })
        } else {
            None
        }
    }
    fn render(&mut self, scaled_font_size: OrderedFloat<f32>) -> Result<RenderParagraphSource, ()> {
        let text = ShapableString {
            text: self.layout.text.clone(),
            metadata_runs: SmallVec::from_iter(self.layout.metadata_runs.iter().map(|run| {
                let font_info = SmallFontOptions {
                    family_id: 0,
                    font_parameters: FontParameters {
                        size: scaled_font_size,
                        allow_float_size: true,
                        underlined: run
                            .style
                            .underline_style
                            .map(|s| s != UnderlineStyle::Reset)
                            .unwrap_or(false), // todo: Make font praameters support more underlin styles
                        hinting: Default::default(),
                        edging: Default::default(),
                    },
                };
                let paint: FontPaint = Default::default();

                ShapedStringMetadata {
                    substring_length: run.section_length,
                    font_info,
                    paint,
                    ..Default::default()
                }
            })),
        };
        /*        let rendered = RenderParagraph {
            location: todo!(),
            data_hash: 0,
            last_modified: current_generation,
            contents: MaybeRenderedParagraph::Source(RenderParagraphSource { text }),
        };*/
        Ok(RenderParagraphSource {
            text,
            location: Default::default(),
        })
    }
    /** @brief Assign render id, unless it is assigned already
     * returns false if an id is already assigned and the supplied id is unused
     */
    fn assign_id(&mut self, block_id: RenderBlockId) -> Result<(), ()> {
        if self.rendered_id.is_none() {
            self.rendered_id = Some(block_id);
            Ok(())
        } else {
            Err(())
        }
    }
}
impl RenderParagraph {
    fn render_to_wire(&self, shaper: &mut CachingShaper) -> Option<ShapedTextBlock> {
        /* TODO: Implement */
        let MaybeRenderedParagraph::Source(ref source) = self.contents else { return None; };
        let shaped = shaper.shape(&source.text, &None);
        Some(shaped)
    }
    fn hash_contents(&self) -> u64 {
        let mut hasher = AHasher::default();
        self.contents.hash(&mut hasher);
        self.location.hash(&mut hasher);
        hasher.finish()
    }
    fn ensure_rendered(&mut self, shaper: &mut CachingShaper) {
        //        if let MaybeRenderedParagraph::Source(ref mut source) = self.contents {
        if let Some(rendered) = self.render_to_wire(shaper) {
            self.contents = MaybeRenderedParagraph::Rendered(rendered);
            self.data_hash = self.hash_contents();
        }
    }
}
impl CenterModel {
    fn prune_old_render_paragraphs(
        &mut self,
        block: &mut ShadowMetaContainerBlockInner<ContentVisitor>,
    ) {
        for (par_id_offs, paragraph) in self.rendered_paragraphs.iter_mut().enumerate() {
            let par_id = RenderBlockId(CENTER_PARAGRAPH_BASE + (par_id_offs as u16));
            if let Some(paragraph_val) = paragraph {
                let age =
                    wrapping_age(paragraph_val.last_modified, self.current_generation).unwrap();
                debug_assert!(age >= 0);
                if age > MAX_AGE {
                    /*  Anything unused for more than max age iterations gets pruned */
                    //                        removed_paragraphs.push_back(paragraph_val.id);
                    block.remove_child(par_id);
                    *paragraph = None;
                }
            }
        }
    }
    /* This code is adopted from the corresponding functionality in helix-term/document */
    pub fn render_document<'t>(
        &mut self,
        doc: &Document,
        text: RopeSlice<'t>,
        offset: ViewPosition,
        text_fmt: &TextFormat,
        text_annotations: &TextAnnotations,
        highlight_iter: impl Iterator<Item = HighlightEvent>,
        theme: &Theme,
        shaper: &mut CachingShaper,
        //        line_decorations: &mut [Box<dyn LineDecoration + '_>],
        //        translated_positions: &mut [TranslatedPosition],
    ) {
        /* This function updates the center model to match the document,
        changing blocks if neccesary */
        if doc.tab_width() != self.tab.len() {
            self.tab = " ".repeat(doc.tab_width());
        }
        let (
            Position {
                row: mut row_off, ..
            },
            mut char_pos,
        ) = visual_offset_from_block(
            text,
            offset.anchor,
            offset.anchor,
            text_fmt,
            text_annotations,
        );
        let shaper_font_options = SmallFontOptions {
            font_parameters: shaper.default_parameters(),
            family_id: 0,
        };

        self.offline_layout.clear();
        let mut paragraph = LayoutParagraph::default();
        row_off += offset.vertical_offset;
        let (mut formatter, mut first_visible_char_idx) = DocumentFormatter::new_at_prev_checkpoint(
            text,
            text_fmt,
            text_annotations,
            offset.anchor,
        );
        let mut styles = StyleIter {
            text_style: Style::default(), // TODO: Reintroduce custom styles
            active_highlights: Vec::with_capacity(64),
            highlight_iter,
            theme,
        };

        let mut last_line_pos = LinePos {
            first_visual_line: false,
            doc_line: usize::MAX,
            visual_line: u16::MAX,
            start_char_idx: usize::MAX,
        };
        let mut line_y = 0f32;
        let mut is_in_indent_area = true;
        let mut last_line_indent_level = 0;
        let mut style_span = styles
            .next()
            .unwrap_or_else(|| (Style::default(), usize::MAX));

        loop {
            // formattter.line_pos returns to line index of the next grapheme
            // so it must be called before formatter.next
            let doc_line = formatter.line_pos();
            let Some((grapheme, mut pos)) = formatter.next() else {
                let mut last_pos = formatter.visual_pos();
                if last_pos.row >= row_off {
                    last_pos.col -= 1;
                    last_pos.row -= row_off;
                    /* TODO */
                    // check if any positions translated on the fly (like cursor) are at the EOF
                    /*translate_positions(
                        char_pos + 1,
                        first_visible_char_idx,
                        translated_positions,
                        text_fmt,
                        renderer,
                        last_pos,
                    );*/
                }
                break;
            };

            // skip any graphemes on visual lines before the block start
            if pos.row < row_off {
                if char_pos >= style_span.1 {
                    style_span = if let Some(style_span) = styles.next() {
                        style_span
                    } else {
                        break;
                    }
                }
                char_pos += grapheme.doc_chars();
                first_visible_char_idx = char_pos + 1;
                continue;
            }
            pos.row -= row_off;

            // if the end of the viewport is reached stop rendering
            if pos.row as u32 >= self.viewport.y() {
                break;
            }

            // apply decorations before rendering a new line
            if pos.row as u16 != last_line_pos.visual_line {
                if pos.row > 0 {
                    //renderer.draw_indent_guides(last_line_indent_level, last_line_pos.visual_line);
                    is_in_indent_area = true;
                    /*for line_decoration in &mut *line_decorations {
                        line_decoration.render_foreground(renderer, last_line_pos, char_pos);
                    }*/
                    /* Flush current line, and prepare a new empty one */
                }
                //        let location = PointF16::new(line_pos.start_char_idx as f32, line_pos.visual_line as f32);
                /* TODO: It should probably be width of space, and not avg char here? */
                let (font_metrics, avg_char_width) = shaper.info(&shaper_font_options).unwrap();
                //                last_line_pos.start_char_idx * shaper.current_size()

                let line_loc =
                    PointF16::new(avg_char_width * last_line_pos.start_char_idx as f32, line_y);
                self.flush_line(&mut paragraph, shaper, line_loc);
                last_line_pos = LinePos {
                    first_visual_line: doc_line != last_line_pos.doc_line,
                    doc_line,
                    visual_line: pos.row as u16,
                    start_char_idx: char_pos,
                };
                line_y += font_metrics.ascent + font_metrics.descent + font_metrics.leading;
                /*for line_decoration in &mut *line_decorations {
                    line_decoration.render_background(renderer, last_line_pos);
                }*/
            }

            // acquire the correct grapheme style
            if char_pos >= style_span.1 {
                style_span = styles.next().unwrap_or((default_text_style(), usize::MAX));
            }
            char_pos += grapheme.doc_chars();

            // TODO: check if any positions translated on the fly (like cursor) has been reached
            /*translate_positions(
                char_pos,
                first_visible_char_idx,
                translated_positions,
                text_fmt,
                renderer,
                pos,
            );*/

            let grapheme_style = if let GraphemeSource::VirtualText { highlight } = grapheme.source
            {
                let style = default_text_style();
                if let Some(highlight) = highlight {
                    style.patch(theme.highlight(highlight.0))
                } else {
                    style
                }
            } else {
                style_span.0
            };
            /* TODO: Currently this is using helix core for deciding when
            to cut of the line. Consider bringing in more swash (font specific)
            shaping knowlegde into this */
            let virt = grapheme.is_virtual();
            self.draw_grapheme(
                &mut paragraph,
                grapheme.grapheme,
                grapheme_style,
                virt,
                &mut last_line_indent_level,
                &mut is_in_indent_area,
                pos,
                shaper,
            );
        }

        /*renderer.draw_indent_guides(last_line_indent_level, last_line_pos.visual_line);
        for line_decoration in &mut *line_decorations {
            line_decoration.render_foreground(renderer, last_line_pos, char_pos);
        }*/
    }
    /* TODO: Add a separate parameter with pragraph struct data to this function ? */
    fn draw_grapheme(
        &mut self,
        layout_paragraph: &mut LayoutParagraph,
        grapheme: Grapheme,
        mut style: Style,
        is_virtual: bool,
        last_indent_level: &mut usize,
        is_in_indent_area: &mut bool,
        position: Position,
        shaper: &CachingShaper,
    ) {
        /* Quick and dirty solution, to have sometheing to add at a later time */
        let width = grapheme.width();
        /* TODO: Support virtual / printed whitespace */
        let grapheme = match grapheme {
            Grapheme::Tab { width } => {
                let grapheme_tab_width = char_to_byte_idx(&self.tab, width);
                &self.tab[..grapheme_tab_width]
            }
            // TODO special rendering for other whitespaces?
            Grapheme::Other { ref g } => g,
            Grapheme::Newline => "",
        };

        let in_bounds = self.col_offset <= position.col as u32
            && position.col < self.viewport.x() as usize + self.col_offset as usize;
        if !in_bounds {
            return;
        }
        //        layout_paragraph.text.push_str()
        /* Figure out if the metadata (draw style) has changed */
        /*        let meta_font = FontParameters {
            size: self.scaled_font_size,
            allow_float_size: true,
            underlined: style
                .underline_style
                .map(|s| s != UnderlineStyle::Reset)
                .unwrap_or(false), // todo: Make font praameters support more underlin styles
            hinting: Default::default(),
            edging: Default::default(),
        };*/
        if style != layout_paragraph.current_style {
            if layout_paragraph.text.is_empty() {
                layout_paragraph.current_style = style;
            } else {
                Self::flush_metadata(layout_paragraph, shaper);
            }
        }
        /* TODO: This is probably a bit too simple,
        and should be replaced by swash ttf-shaping
        (although a neccesary way to cache it) */
        for byte in grapheme.as_bytes() {
            layout_paragraph.text.push(*byte)
        }
    }
    fn flush_metadata(layout_paragraph: &mut LayoutParagraph, shaper: &CachingShaper) {
        let substring_length = layout_paragraph.text.len()
            - layout_paragraph
                .metadata_runs
                .last()
                .map(|r| r.section_length as usize)
                .unwrap_or(0);
        /* Only flush non empty metadata blocks */
        if substring_length != 0 {
            layout_paragraph.substring_end += substring_length as u16;
            layout_paragraph.metadata_runs.push(LayoutStringMetadata {
                section_length: substring_length as u16,
                style: layout_paragraph.current_style,
            })
        }
    }

    fn flush_line(
        &mut self,
        layout_paragraph: &mut LayoutParagraph,
        shaper: &CachingShaper,
        location: PointF16,
    ) {
        if !layout_paragraph.text.is_empty() {
            if layout_paragraph.substring_end
                + layout_paragraph
                    .metadata_runs
                    .last()
                    .map(|r| r.section_length)
                    .unwrap_or(0)
                != layout_paragraph.text.len() as u16
            {
                Self::flush_metadata(layout_paragraph, shaper)
            }
        }
        let mut new_paragraph = LayoutParagraph::default(); // TODO: Does this need further init?
        std::mem::swap(layout_paragraph, &mut new_paragraph);
        let mut hasher = AHasher::default();
        self.hash(&mut hasher);
        let layout_hash = hasher.finish();
        let new_paragraph_entry = LayoutParagraphEntry {
            layout: new_paragraph,
            location,
            layout_hash,
            rendered_id: None,
            client_hash: None,
            client_location: None,
        };
        self.offline_layout.push(new_paragraph_entry);
    }

    /* @brief Retrieve an id that can be used for a new block.
     *
     * first see if an old id can be reused, otherwise extend the id range.
     * A render paragraph for the id should be set before this function is called
     * again to avoid multiple users for id's
     */
    fn new_id(rendered_paragraphs: &mut Vec<Option<RenderParagraph>>) -> RenderBlockId {
        for (idx, paragraph) in rendered_paragraphs.iter().enumerate() {
            if paragraph.is_none() {
                return RenderBlockId(CENTER_PARAGRAPH_BASE + idx as u16);
            }
        }
        /* If the loop above have not returned, make a new id */
        rendered_paragraphs.push(None);
        return RenderBlockId(CENTER_PARAGRAPH_BASE + rendered_paragraphs.len() as u16 - 1);
    }

    fn sync_client_view(
        &mut self,
        block: &mut ShadowMetaContainerBlockInner<ContentVisitor>,
        shaper: &mut CachingShaper,
    ) {
        log::trace!("Sync center client");
        //        let mut removed_entry_ids = SmallVec::<[RenderBlockId; 128]>::new();
        let mut updated_contents = SmallVec::<[RenderBlockId; 128]>::new();
        let mut updated_locations = SmallVec::<[RenderBlockLocation; 128]>::new();
        /* Try to make search faster by improving cache coherency of hashes */
        let mut old_locations = SmallVec::<[u64; 128]>::with_capacity(self.client_layout.len());
        old_locations.extend(self.client_layout.iter().map(|entry| entry.layout_hash));
        /* Figure out which entries that can be reused, that only needs moving and that needs complete rerender */
        for entry in self.offline_layout.iter_mut() {
            let entry_hash = entry.layout_hash;
            let client_entry = old_locations
                .iter()
                .enumerate()
                .find(|(_, h)| **h == entry_hash);
            if let Some((client_idx, _)) = client_entry {
                /* If this entry is found, it is up to date, so there is no reason to update the contents */
                old_locations.swap_remove(client_idx);
                let mut retrieved_entry = self.client_layout.swap_remove(client_idx);
                if let Some(rendered_id) = retrieved_entry.rendered_id {
                    block.remove_child(rendered_id);
                    entry.reuse(&mut retrieved_entry);
                }
            } else {
                /* No entry to reuse, so a new entry has to be made */
                let block_id = Self::new_id(&mut self.rendered_paragraphs);
                assert!(entry.assign_id(block_id).is_ok());
                let rendered = entry.render(self.scaled_font_size).unwrap();
                let rendered_slot =
                    &mut self.rendered_paragraphs[(block_id.0 - CENTER_PARAGRAPH_BASE) as usize];
                debug_assert!(rendered_slot.is_none());
                *rendered_slot = Some(RenderParagraph {
                    contents: MaybeRenderedParagraph::Source(rendered),
                    location: RenderBlockLocation {
                        id: block_id,
                        location: PointF16::default(),
                        layer: 0,
                    },
                    data_hash: 0, // TODO: Is hash needed here, or just set it as 0 and fill it further down this function
                    last_modified: self.current_generation,
                });
                updated_contents.push(block_id);
            }
            /* Check if entry needs moving */
            if let Some(location) = entry.new_location() {
                // TODO: Where do we change statae for the entry, to track wthe client location?
                updated_locations.push(location);
            }
        }
        /* All entries left in client layout are unused. Drain and clean them up.
        TODO: Consider leaving them in here to be aged out to avoid having to resend them if scrolling short distances */
        for entry in self.client_layout.drain(..) {
            if let Some(rendered_id) = entry.rendered_id {
                block.remove_child(rendered_id);
            }
        }
        if updated_contents.len() > 8 {
            /* Parallelise shaping */
            self.rendered_paragraphs
                .par_iter_mut()
                .enumerate()
                .for_each(|(idx, paragraph)| {
                    if let Some(ref mut paragraph) = paragraph {
                        paragraph.ensure_rendered(&mut shaper.clone());
                    }
                });
        } else {
            /* If few lines are updated it is probably faster to not try to paralelise the shaping_*/
            updated_contents.iter().for_each(|id| {
                self.rendered_paragraphs[(id.0 - CENTER_PARAGRAPH_BASE) as usize]
                    .as_mut()
                    .unwrap()
                    .ensure_rendered(shaper);
            });
        }
        // TODO: Make sure rendered paragraphs are added to the parent block
        for block_id in updated_contents.drain(..) {
            //
            let paragraph_block = self
                .rendered_paragraphs
                .get((block_id.0 - CENTER_PARAGRAPH_BASE) as usize)
                .unwrap()
                .as_ref()
                .unwrap()
                .clone();
            let mut text_block = ShadowMetaTextBlock::new(block_id);
            text_block.set_wire(paragraph_block.contents.rendered().unwrap().clone());
            block.set_child(
                RenderBlockLocation {
                    id: block_id,
                    location: PointF16::default(),
                    layer: 0,
                },
                ShadowMetaBlock::Text(text_block),
            );
        }
        /* TODO: Act on the updated locations */
        for location in updated_locations.drain(..) {
            let id = location.id;
            *(block.child_mut(id).unwrap().location()) = location;
        }
    }
}
impl ContainerBlockLogic for CenterModel {
    type UpdateContext = ContentVisitor;
    fn pre_update(
        outer_block: &mut ShadowMetaContainerBlock<Self, ContentVisitor>,
        context: &mut Self::UpdateContext,
    ) where
        Self: Sized,
    {
        let (block, model) = outer_block.destruct_mut();
        let options = SmallFontOptions {
            font_parameters: context.shaper_ref().default_parameters(),
            family_id: 0,
        };
        let avg_char_width = context.shaper().info(&options).unwrap().1;
        let (doc_container, shaper) = context.doc_and_shaper();
        let doc_container = doc_container.unwrap();
        let document = doc_container.document().unwrap();
        let width_chars = (block.extent().y() / avg_char_width) as u16;
        model.prune_old_render_paragraphs(block);
        /* This will update offline layout according to the current document */
        model.render_document(
            document,
            document.text().slice(..),
            doc_container.view().offset,
            &document.text_format(width_chars, Some(&doc_container.editor().editor().theme)),
            &Default::default(),
            std::iter::empty(),
            &doc_container.editor().editor().theme,
            shaper,
        );
        /* Figure out what differences there are between offline layout and client layout and make
        instructions for the client to sync */
        model.sync_client_view(block, shaper);
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

impl Paragraph {}
pub fn hash_line<'t>(
    text: RopeSlice<'t>,
    offset: ViewPosition,
    text_fmt: &TextFormat,
    text_annotations: &TextAnnotations,
    highlight_iter: impl Iterator<Item = HighlightEvent>,
    theme: &Theme,
) {
}

/* From helix-term */
struct StyleIter<'a, H: Iterator<Item = HighlightEvent>> {
    text_style: Style,
    active_highlights: Vec<Highlight>,
    highlight_iter: H,
    theme: &'a Theme,
}

impl<H: Iterator<Item = HighlightEvent>> Iterator for StyleIter<'_, H> {
    type Item = (Style, usize);
    fn next(&mut self) -> Option<(Style, usize)> {
        while let Some(event) = self.highlight_iter.next() {
            match event {
                HighlightEvent::HighlightStart(highlights) => {
                    self.active_highlights.push(highlights)
                }
                HighlightEvent::HighlightEnd => {
                    self.active_highlights.pop();
                }
                HighlightEvent::Source { start, end } => {
                    if start == end {
                        continue;
                    }
                    let style = self
                        .active_highlights
                        .iter()
                        .fold(self.text_style, |acc, span| {
                            acc.patch(self.theme.highlight(span.0))
                        });
                    return Some((style, end));
                }
            }
        }
        None
    }
}

/* From helix-term */
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub struct LinePos {
    /// Indicates whether the given visual line
    /// is the first visual line of the given document line
    pub first_visual_line: bool,
    /// The line index of the document line that contains the given visual line
    pub doc_line: usize,
    /// Vertical offset from the top of the inner view area
    pub visual_line: u16,
    /// The first char index of this visual line.
    /// Note that if the visual line is entirely filled by
    /// a very long inline virtual text then this index will point
    /// at the next (non-virtual) char after this visual line
    pub start_char_idx: usize,
}

/* It would have been best to use a const expression here, but because default
is part of a trait, and not const settle for a function instead */
fn default_text_style() -> Style {
    Style::default()
}

/* Determine the age of an individual, based on a wrapping generation number */
pub fn wrapping_age(individual: u16, generation: u16) -> Option<i16> {
    wrapping_subtract_u16(generation, individual)
}

pub fn wrapping_subtract_u16(a: u16, b: u16) -> Option<i16> {
    const U14: u16 = u16::MAX >> 2;
    let aq = a >> 14;
    let bq = b >> 14;
    if aq == bq || aq == (bq + 1) % 4 || (aq + 1) % 4 == bq {
        Some(if aq == 0 || bq == 0 || aq == 3 || bq == 3 {
            (a.wrapping_add(U14) as i16) - (b.wrapping_add(U14) as i16)
        } else {
            (a as i16) - (b as i16)
        })
    } else {
        None
    }
}
