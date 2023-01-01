/* This file / crate is supposed to be shared between helix and helicoid, so be mindful of
the depedenencies that are introduced */

use rkyv::{Archive, Deserialize, Serialize};

use std::sync::Arc;

use glutin::dpi::PhysicalSize;
use log::trace;

use half::f16;
use ordered_float::OrderedFloat;
use smallvec::SmallVec;

use crate::{
    editor::{Colors, Style, UnderlineStyle},
    renderer::fonts::font_options::{FontOptions, SmallFontOptions},
    //dimensions::Dimensions,
    renderer::CachingShaper,
};

/* Shaping is done in editor on "server", shaped glyphs are transfered to client
Coordinates are relative to ShapedTextBlock origin */
#[derive(Default, Debug, Clone, Copy, Hash, Eq, PartialEq, Archive, Serialize, Deserialize)]
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
}

#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
pub struct ShapableString {
    pub text: SmallVec<[u8; 128]>, //text should always contain valid UTF-8?
    pub metadata_runs: SmallVec<[ShapedStringMetadata; 8]>,
}

#[derive(Default, Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
pub struct ShapedTextBlock {
    pub glyphs: SmallVec<[ShapedTextGlyph; 128]>,
    pub metadata_runs: SmallVec<[ShapedStringMetadata; 8]>,
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

pub struct TextRenderer {
    pub shaper: CachingShaper,
    //pub paint: Paint,
    pub default_style: Arc<Style>,
    pub em_size: f32,
    //pub font_dimensions: Dimensions,
    pub scale_factor: f64,
    pub is_ready: bool,
}
