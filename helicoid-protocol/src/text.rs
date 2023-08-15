/* This file / crate is supposed to be shared between helix and helicoid, so be mindful of
the depedenencies that are introduced */

use rkyv::{Archive, Deserialize, Serialize};

use std::fmt::{self, Debug};

use bytecheck::CheckBytes;

use num_enum::IntoPrimitive;
use ordered_float::OrderedFloat;
use smallvec::{smallvec, SmallVec};

use crate::gfx::{FontPaint, PointF32};

pub const SHAPABLE_STRING_ALLOC_LEN: usize = 128;

pub const SHAPABLE_STRING_ALLOC_RUNS: usize = 8;
pub const SHAPABLE_STRING_ALLOC_RUN_SPANS: usize = 16;
pub const SHAPABLE_STRING_ALLOC_COORDINATES: usize = 4;
pub const SHAPABLE_STRING_COORDINATES_ID_SHAPED: u8 = 0xFF;
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
    y: OrderedFloat<f32>,
    x: OrderedFloat<f32>,
}
#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct ShapedStringMetadataSpan {
    pub substring_length: u16, // In (UTF-8) bytes
    pub metadata_info: u8,
    pub span_coordinates: u8, /* 0xFF means coordinates from shaping */
}
#[derive(
    Debug, Default, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes,
)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct ShapedStringMetadataCoordinates {
    pub baseline_x: OrderedFloat<f32>,
    pub baseline_y: OrderedFloat<f32>,
    pub fixed_advance_x: OrderedFloat<f32>, // advance per (monospace glyph)
    pub fixed_advance_y: OrderedFloat<f32>, // advance per line
}

#[derive(
    Debug, Default, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes,
)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct ShapedStringMetadata {
    pub font_info: SmallFontOptions,
    pub paint: FontPaint,
}
impl Default for ShapedStringMetadataSpan {
    fn default() -> Self {
        Self::simple(0)
    }
}
impl ShapedStringMetadataSpan {
    pub fn simple(substring_length: u16) -> Self {
        Self {
            substring_length,
            metadata_info: 0,
            span_coordinates: 0xFF,
        }
    }
}
impl ShapedStringMetadataCoordinates {
    pub fn set_baseline(&mut self, x: f32, y: f32) {
        self.baseline_x = OrderedFloat(x);
        self.baseline_y = OrderedFloat(y);
    }
    pub fn baseline_x(&self) -> f32 {
        f32::from(self.baseline_x)
    }
    pub fn baseline_y(&self) -> f32 {
        f32::from(self.baseline_y)
    }
}

#[derive(
    Debug, Default, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes,
)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct ShapableMetadata {
    pub runs: SmallVec<[ShapedStringMetadata; SHAPABLE_STRING_ALLOC_RUNS]>,
    pub spans: SmallVec<[ShapedStringMetadataSpan; SHAPABLE_STRING_ALLOC_RUN_SPANS]>,
    pub span_coordinates:
        SmallVec<[ShapedStringMetadataCoordinates; SHAPABLE_STRING_ALLOC_COORDINATES]>,
}
#[derive(Debug, Default, Hash, Eq, Clone, PartialEq)]
pub struct ShapableString {
    // TODO: Is there some kind of embedded string type we could use instead?
    pub text: SmallVec<[u8; SHAPABLE_STRING_ALLOC_LEN]>, //text should always contain valid UTF-8?
    pub metadata: ShapableMetadata,
}

#[derive(Default, Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct ShapedTextBlock {
    pub glyphs: SmallVec<[ShapedTextGlyph; SHAPABLE_STRING_ALLOC_LEN]>,
    pub metadata: ShapableMetadata,
    pub extent: PointF32,
}

impl ShapableMetadata {
    pub fn push(
        &mut self,
        substring_length: u16,
        span_metadata: ShapedStringMetadata,
        coordinates: Option<ShapedStringMetadataCoordinates>,
    ) {
        //debug_assert_eq!(metadata.substring_length as usize, text.as_bytes().len());
        let run_id = (if let Some(run_id) = self.runs.iter().position(|mr| *mr == span_metadata) {
            run_id
        } else {
            self.runs.push(span_metadata);
            self.runs.len() - 1
        }) as u8;

        let coord_id = if let Some(coordinates) = coordinates {
            (if let Some(coord_id) = self
                .span_coordinates
                .iter()
                .position(|sc| *sc == coordinates)
            {
                coord_id
            } else {
                self.span_coordinates.push(coordinates);
                self.span_coordinates.len() - 1
            }) as u8
        } else {
            SHAPABLE_STRING_COORDINATES_ID_SHAPED
        };
        let span = ShapedStringMetadataSpan {
            substring_length,
            metadata_info: run_id,
            span_coordinates: coord_id,
        };
        self.spans.push(span);
    }

    pub fn clear(&mut self) {
        self.runs.clear();
        self.spans.clear();
        self.span_coordinates.clear();
    }
}
impl ShapedTextGlyph {
    pub fn x(&self) -> f32 {
        *self.x
    }
    pub fn y(&self) -> f32 {
        f32::from(self.y)
    }
    pub fn glyph(&self) -> u16 {
        self.glyph
    }
    pub fn new(glyph: u16, x: f32, y: f32) -> Self {
        Self {
            glyph,
            x: OrderedFloat(x),
            y: OrderedFloat(y),
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
            font_info: Default::default(),
            paint: FontPaint::default(),
        };
        let substring_length = text.len() as u16;
        ShapableString {
            text,
            metadata: ShapableMetadata {
                runs: smallvec![simple_run],
                spans: smallvec![ShapedStringMetadataSpan {
                    substring_length,
                    metadata_info: 0,
                    span_coordinates: 0xFF,
                }],
                span_coordinates: smallvec![],
            },
        }
    }
    pub fn push_str(
        &mut self,
        text: &str,
        metadata: ShapedStringMetadata,
        mut span: ShapedStringMetadataSpan,
        coordinates: Option<ShapedStringMetadataCoordinates>,
    ) {
        self.text.extend_from_slice(text.as_bytes());
        span.substring_length = text.len() as u16;
        self.push_span(metadata, span, coordinates);
    }
    pub fn push_span(
        &mut self,
        metadata: ShapedStringMetadata,
        mut span: ShapedStringMetadataSpan,
        coordinates: Option<ShapedStringMetadataCoordinates>,
    ) {
        //debug_assert_eq!(metadata.substring_length as usize, text.as_bytes().len());
        let run_id = if let Some(run_id) = self.metadata.runs.iter().position(|mr| *mr == metadata)
        {
            run_id
        } else {
            self.metadata.runs.push(metadata);
            self.metadata.runs.len() - 1
        };

        let coord_id = if let Some(coordinates) = coordinates {
            (if let Some(coord_id) = self
                .metadata
                .span_coordinates
                .iter()
                .position(|sc| *sc == coordinates)
            {
                coord_id
            } else {
                self.metadata.span_coordinates.push(coordinates);
                self.metadata.span_coordinates.len() - 1
            }) as u8
        } else {
            SHAPABLE_STRING_COORDINATES_ID_SHAPED
        };
        span.metadata_info = run_id as u8;
        span.span_coordinates = coord_id;
        self.metadata.spans.push(span);
    }

    pub fn push_plain_str(&mut self, text: &str, color: u32, scaled_font_size: f32) {
        log::trace!("PPS: {}", text);
        let simple_run = ShapedStringMetadata {
            font_info: SmallFontOptions {
                family_id: 0,
                font_parameters: FontParameters {
                    size: OrderedFloat(scaled_font_size),
                    ..Default::default()
                },
            },
            paint: FontPaint {
                color,
                ..Default::default()
            },
        };
        let simple_span = ShapedStringMetadataSpan::simple(text.as_bytes().len() as u16);
        self.push_str(text, simple_run, simple_span, None);
    }
    pub fn clear(&mut self) {
        self.text.clear();
        self.metadata.clear();
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
