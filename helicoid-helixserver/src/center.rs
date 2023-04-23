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
    font_options::FontOptions,
    gfx::{
        FontPaint, HelicoidToClientMessage, MetaDrawBlock, NewRenderBlock, PathVerb, PointF16,
        PointU32, RemoteBoxUpdate, RenderBlockDescription, RenderBlockId, RenderBlockLocation,
        RenderBlockPath, RenderBlockRemoveInstruction, SimpleDrawBlock, SimpleDrawElement,
        SimpleDrawPath, SimpleDrawPolygon, SimplePaint, SimpleRoundRect, SimpleSvg,
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
        SmallFontOptions,
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

#[derive(Hash, PartialEq, Default)]
struct RenderParagraph {
    text: ShapableString,
    current_meta_font: FontParameters,
    current_meta_paint: FontPaint,
    last_substring_end: u16,
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
    paragraphs: Vec<Option<Paragraph>>,
    paragraph_temp: Vec<RenderParagraph>,
    viewport: PointU32,
    current_generation: u16,
    col_offset: u32,
    tab: String,
}
impl CenterModel {
    fn prune_old_paragraphs(&mut self, block: &mut ShadowMetaContainerBlockInner<ContentVisitor>) {
        for (par_id_offs, paragraph) in self.paragraphs.iter_mut().enumerate() {
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
        shaper: &CachingShaper,
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
        let mut paragraph = RenderParagraph::default();
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
                    self.flush_line(&mut paragraph, shaper);
                }
                last_line_pos = LinePos {
                    first_visual_line: doc_line != last_line_pos.doc_line,
                    doc_line,
                    visual_line: pos.row as u16,
                    start_char_idx: char_pos,
                };
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
        render_paragraph: &mut RenderParagraph,
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
        //        render_paragraph.text.push_str()
        /* Figure out if the metadata (draw style) has changed */
        let meta_font = FontParameters {
            size: self.scaled_font_size,
            allow_float_size: true,
            underlined: style
                .underline_style
                .map(|s| s != UnderlineStyle::Reset)
                .unwrap_or(false), // todo: Make font praameters support more underlin styles
            hinting: Default::default(),
            edging: Default::default(),
        };
        let font_paint: FontPaint = Default::default();
        if (meta_font != render_paragraph.current_meta_font
            && font_paint != render_paragraph.current_meta_paint)
        {
            if render_paragraph.text.text.is_empty() {
                render_paragraph.current_meta_font = meta_font;
                render_paragraph.current_meta_paint = font_paint;
            } else {
                Self::flush_metadata(render_paragraph, shaper);
            }
        }
        /* TODO: This is probably a bit too simple,
        and should be replaced by swash ttf-shaping
        (although a neccesary way to cache it) */
        for byte in grapheme.as_bytes() {
            render_paragraph.text.text.push(*byte)
        }
    }

    fn flush_metadata(render_paragraph: &mut RenderParagraph, shaper: &CachingShaper) {
        let substring_length = render_paragraph.text.text.len()
            - render_paragraph
                .text
                .metadata_runs
                .last()
                .map(|r| r.substring_length as usize)
                .unwrap_or(0);

        render_paragraph
            .text
            .metadata_runs
            .push(ShapedStringMetadata {
                substring_length: substring_length as u16,
                font_info: SmallFontOptions {
                    family_id: DEFAULT_FONT_ID,
                    font_parameters: render_paragraph.current_meta_font.clone(),
                },
                paint: render_paragraph.current_meta_paint.clone(),
                advance_x: Default::default(),
                advance_y: Default::default(),
                baseline_y: Default::default(),
            })
    }

    fn flush_line(&mut self, render_paragraph: &mut RenderParagraph, shaper: &CachingShaper) {
        if !render_paragraph.text.metadata_runs.is_empty() {
            if render_paragraph.last_substring_end
                + render_paragraph
                    .text
                    .metadata_runs
                    .last()
                    .map(|r| r.substring_length)
                    .unwrap_or(0)
                != render_paragraph.text.text.len() as u16
            {
                Self::flush_metadata(render_paragraph, shaper)
            }
        }
        let mut new_paragraph = RenderParagraph::default(); // TOOD: Does this need further init?
        std::mem::swap(render_paragraph, &mut new_paragraph);
        self.paragraph_temp.push(new_paragraph);
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
        let doc_container = context.current_doc().unwrap();
        let document = doc_container.document().unwrap();
        let width_chars = (block.extent().y() / avg_char_width) as u16;
        model.prune_old_paragraphs(block);
        model.render_document(
            document,
            document.text().slice(..),
            doc_container.view().offset,
            &document.text_format(width_chars, Some(&doc_container.editor().editor().theme)),
            &Default::default(),
            std::iter::empty(),
            &doc_container.editor().editor().theme,
            context.shaper_ref(),
        );
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
