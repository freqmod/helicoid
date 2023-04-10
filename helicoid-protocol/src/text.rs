/* This file / crate is supposed to be shared between helix and helicoid, so be mindful of
the depedenencies that are introduced */

use rkyv::{Archive, Deserialize, Serialize};

use log::trace;
use std::fmt::{self, Debug};
use std::sync::Arc;

use bytecheck::CheckBytes;
use half::f16;
use num_enum::IntoPrimitive;
use ordered_float::OrderedFloat;
use smallvec::{smallvec, SmallVec};

use crate::gfx::{FontPaint, PointF16};

pub const SHAPABLE_STRING_ALLOC_LEN: usize = 128;
pub const SHAPABLE_STRING_ALLOC_RUNS: usize = 16;
/* Shaping is done in editor on "server", shaped glyphs are transfered to client
Coordinates are relative to ShapedTextBlock origin */

#[derive(
    Clone, Debug, Default, Archive, Serialize, Deserialize, Hash, PartialEq, Eq, CheckBytes,
)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct FontParameters {
    pub size: OrderedFloat<f32>,
    pub allow_float_size: bool,
    pub underlined: bool,
    pub hinting: FontHinting,
    pub edging: FontEdging,
}

#[derive(
    Clone, Hash, Default, Debug, Archive, Serialize, Deserialize, PartialEq, Eq, CheckBytes,
)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct SmallFontOptions {
    pub family_id: u8,
    pub font_parameters: FontParameters,
    //#[with(Skip)]
    //font_list_ref: Option<Arc<SmallFontRegistry>>,
}
#[derive(Clone, Hash, Archive, Serialize, Deserialize, PartialEq, Eq, CheckBytes)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct ShapedTextGlyph {
    glyph: u16,
    y: u16, /* This is an f16, but to make rkyv happy use u16 */
    x: OrderedFloat<f32>,
}

#[derive(
    Debug, Default, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes,
)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct ShapedStringMetadata {
    pub substring_length: u16, // In (UTF-8) bytes
    pub font_info: SmallFontOptions,
    pub paint: FontPaint,
    pub advance_x: u16,
    pub advance_y: u16,
    pub baseline_y: u16,
}

impl ShapedStringMetadata {
    pub fn set_advance(&mut self, x: f32, y: f32, bl_y: f32) {
        self.advance_x = half::f16::from_f32(x).to_bits();
        self.advance_y = half::f16::from_f32(y).to_bits();
        self.baseline_y = half::f16::from_f32(bl_y).to_bits();
    }
    pub fn advance_x(&self) -> f32 {
        half::f16::from_bits(self.advance_x).to_f32()
    }
    pub fn advance_y(&self) -> f32 {
        half::f16::from_bits(self.advance_y).to_f32()
    }
    pub fn baseline_y(&self) -> f32 {
        half::f16::from_bits(self.baseline_y).to_f32()
    }
}

#[derive(Default, Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
pub struct ShapableString {
    pub text: SmallVec<[u8; SHAPABLE_STRING_ALLOC_LEN]>, //text should always contain valid UTF-8?
    pub metadata_runs: SmallVec<[ShapedStringMetadata; SHAPABLE_STRING_ALLOC_RUNS]>,
}

#[derive(Default, Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct ShapedTextBlock {
    pub glyphs: SmallVec<[ShapedTextGlyph; SHAPABLE_STRING_ALLOC_LEN]>,
    pub metadata_runs: SmallVec<[ShapedStringMetadata; SHAPABLE_STRING_ALLOC_RUNS]>,
    pub extent: PointF16,
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
impl ShapableString {
    pub fn from_text(text: &str) -> Self {
        let text = SmallVec::from_slice(text.as_bytes());
        let simple_run = ShapedStringMetadata {
            substring_length: text.len() as u16,
            font_info: Default::default(),
            paint: FontPaint::default(),
            advance_x: 0,
            advance_y: 0,
            baseline_y: 0,
        };
        ShapableString {
            text,
            metadata_runs: smallvec![simple_run],
        }
    }
    pub fn push_str(&mut self, text: &str, metadata: ShapedStringMetadata) {
        debug_assert_eq!(metadata.substring_length as usize, text.as_bytes().len());
        self.text.extend_from_slice(text.as_bytes());
        self.metadata_runs.push(metadata);
    }
    pub fn push_plain_str(&mut self, text: &str, color: u32) {
        log::trace!("PPS: {}", text);
        let simple_run = ShapedStringMetadata {
            substring_length: text.as_bytes().len() as u16,
            font_info: Default::default(),
            paint: FontPaint {
                color,
                ..Default::default()
            },
            advance_x: 0,
            advance_y: 0,
            baseline_y: 0,
        };
        self.push_str(text, simple_run);
    }
    pub fn clear(&mut self) {
        self.text.clear();
        self.metadata_runs.clear();
    }
}
#[derive(Clone, Debug, Hash, PartialEq, Eq, Archive, Serialize, Deserialize, CheckBytes)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
#[derive(IntoPrimitive)]
#[repr(u8)]
pub enum FontEdging {
    AntiAlias,
    SubpixelAntiAlias,
    Alias,
}

impl FontEdging {
    pub fn parse(value: &str) -> Self {
        match value {
            "antialias" => FontEdging::AntiAlias,
            "subpixelantialias" => FontEdging::SubpixelAntiAlias,
            _ => FontEdging::Alias,
        }
    }
}

impl Default for FontEdging {
    fn default() -> Self {
        FontEdging::AntiAlias
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, Archive, Serialize, Deserialize, CheckBytes)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
#[derive(IntoPrimitive)]
#[repr(u8)]
pub enum FontHinting {
    Full,
    Normal,
    Slight,
    None,
}

impl FontHinting {
    pub fn parse(value: &str) -> Self {
        match value {
            "full" => FontHinting::Full,
            "normal" => FontHinting::Normal,
            "slight" => FontHinting::Slight,
            _ => FontHinting::None,
        }
    }
}

impl Default for FontHinting {
    fn default() -> Self {
        FontHinting::Full
    }
}
impl FontParameters {
    pub fn size(&self) -> f32 {
        f32::from(self.size)
    }
}
