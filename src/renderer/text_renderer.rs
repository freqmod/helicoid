/* This file / crate is supposed to be shared between helix and helicoid, so be mindful of
the depedenencies that are introduced */

use rkyv::{Archive, Deserialize, Serialize};

use glutin::dpi::PhysicalSize;
use log::trace;
use std::fmt::{self, Debug};
use std::sync::Arc;

use half::f16;
use ordered_float::OrderedFloat;
use smallvec::{smallvec, SmallVec};

use crate::{
    editor::{Colors, Style, UnderlineStyle},
    renderer::fonts::font_options::{FontOptions, SmallFontOptions},
    //dimensions::Dimensions,
    renderer::CachingShaper,
};

pub const SHAPABLE_STRING_ALLOC_LEN : usize = 128;
pub const SHAPABLE_STRING_ALLOC_RUNS : usize = 16;
/* Shaping is done in editor on "server", shaped glyphs are transfered to client
Coordinates are relative to ShapedTextBlock origin */
#[derive(Default, Clone, Copy, Hash, Eq, PartialEq, Archive, Serialize, Deserialize)]
pub struct ShapedTextGlyph {
    glyph: u16,
    y: u16, /* This is an f16, but to make rkyv happy use u16 */
    x: OrderedFloat<f32>,
}

#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
pub struct ShapedStringMetadata {
    pub substring_length: u16,
    pub font_info: SmallFontOptions,
    pub font_color: u32, /* ARGB32 */
    pub advance_x: u16,
    pub advance_y: u16,
}

impl ShapedStringMetadata {
    pub fn set_advance(&mut self, x: f32, y: f32) {
        self.advance_x = half::f16::from_f32(x).to_bits();
        self.advance_y = half::f16::from_f32(y).to_bits();
    }
    pub fn advance_x(&self) -> f32 {
        half::f16::from_bits(self.advance_x).to_f32()
    }
    pub fn advance_y(&self) -> f32 {
        half::f16::from_bits(self.advance_y).to_f32()
    }
}

#[derive(Default, Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
pub struct ShapableString {
    pub text: SmallVec<[u8; SHAPABLE_STRING_ALLOC_LEN]>, //text should always contain valid UTF-8?
    pub metadata_runs: SmallVec<[ShapedStringMetadata; SHAPABLE_STRING_ALLOC_RUNS]>,
}

#[derive(Default, Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
pub struct ShapedTextBlock {
    pub glyphs: SmallVec<[ShapedTextGlyph; SHAPABLE_STRING_ALLOC_LEN]>,
    pub metadata_runs: SmallVec<[ShapedStringMetadata; SHAPABLE_STRING_ALLOC_RUNS]>,
}

impl ShapedTextGlyph {
    pub fn x(&self) -> f32 {
        *self.x
    }
    pub fn y(&self) -> f32 {
        half::f16::from_bits(self.y).to_f32()
    }
    pub fn glyph(&self) -> u16 {
        self.glyph
    }
    pub fn new(glyph: u16, x: f32, y: f32) -> Self {
        Self {
            glyph,
            x: OrderedFloat(x),
            y: half::f16::from_f32(y).to_bits(),
        }
    }
}
impl Debug for ShapedTextGlyph {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ShapedTextGlyph")
            .field("glyph", &self.glyph)
            .field("x", &self.x())
            .field("y", &self.y())
            .finish()
    }
}
pub struct TextRenderer {
    pub shaper: CachingShaper,
    //pub paint: Paint,
    pub default_style: Arc<Style>,
    pub em_size: f32,
    //pub font_dimensions: Dimensions,
    pub scale_factor: f64,
    pub is_ready: bool,
}

impl ShapableString {
    pub fn from_text(text: &str) -> Self {
        let text = SmallVec::from_slice(text.as_bytes());
        let simple_run = ShapedStringMetadata {
            substring_length: text.len() as u16,
            font_info: Default::default(),
            font_color: 0,
            advance_x: 0,
            advance_y: 0,
        };
        ShapableString {
            text,
            metadata_runs: smallvec![simple_run],
        }
    }
}
