use crate::text::{FontEdging, FontHinting, FontParameters};
//use itertools::Itertools;

use ordered_float::OrderedFloat;
use rkyv::{Archive, Deserialize, Serialize};
//use serde::{Serialize, Deserialize};
use itertools::Itertools;
use smallvec::{SmallVec};

use unicode_segmentation::UnicodeSegmentation;
const DEFAULT_FONT_SIZE: f32 = 14.0;
/*
#[derive(Clone, Debug, Archive, Serialize, Deserialize, Hash, Eq)]
pub struct FontParameters {
    pub size: OrderedFloat<f32>,
    pub allow_float_size: bool,
    pub underlined: bool,
    pub hinting: FontHinting,
    pub edging: FontEdging,
}
*/
#[derive(Debug, PartialEq)]
pub struct SmallFontRegistry {
    pub font_list: Vec<String>,
}

#[derive(Clone, Debug, Archive, Serialize, Deserialize)]
pub struct FontOptions {
    pub font_list: SmallVec<[SmallVec<[u8; 32]>; 8]>,
    pub font_parameters: FontParameters,
}

impl FontOptions {
    /*    pub fn parse(guifont_setting: &str) -> FontOptions {
            let mut font_list = SmallVec::new();
            let mut size = DEFAULT_FONT_SIZE;
            let mut bold = false;
            let mut italic = false;
            let mut allow_float_size = false;
            let mut hinting = FontHinting::default();
            let mut edging = FontEdging::default();

            let mut parts = guifont_setting.split(':').filter(|part| !part.is_empty());

            if let Some(parts) = parts.next() {
                let parsed_font_list: SmallVec<[SmallVec<[u8; 32]>; 8]> = parts
                    .split(',')
                    .filter_map(|fallback| {
                        if !fallback.is_empty() {
                            Some(parse_font_name(fallback))
                        } else {
                            None
                        }
                    })
                    .collect();

                if !parsed_font_list.is_empty() {
                    font_list = parsed_font_list;
                }
            }

            for part in parts {
                if let Some(hinting_string) = part.strip_prefix("#h-") {
                    hinting = FontHinting::parse(hinting_string);
                } else if let Some(edging_string) = part.strip_prefix("#e-") {
                    edging = FontEdging::parse(edging_string);
                } else if part.starts_with('h') && part.len() > 1 {
                    if part.contains('.') {
                        allow_float_size = true;
                    }
                    if let Ok(parsed_size) = part[1..].parse::<f32>() {
                        size = parsed_size
                    }
                } else if part == "b" {
                    bold = true;
                } else if part == "i" {
                    italic = true;
                }
            }

            FontOptions {
                font_list,
                font_parameters: FontParameters {
                    allow_float_size,
                    hinting,
                    edging,
                    underlined: false,
                    size: OrderedFloat(points_to_pixels(size)),
                },
            }
        }
    */
    pub fn primary_font(&self) -> Option<String> {
        self.font_list
            .first()
            .cloned()
            .map(|s| std::str::from_utf8(s.as_slice()).unwrap().to_string())
    }
}

impl Default for FontOptions {
    fn default() -> Self {
        FontOptions {
            font_list: SmallVec::new(),
            font_parameters: FontParameters {
                allow_float_size: false,
                underlined: false,
                size: OrderedFloat(points_to_pixels(DEFAULT_FONT_SIZE)),
                hinting: FontHinting::default(),
                edging: FontEdging::default(),
            },
        }
    }
}

fn parse_font_name(font_name: impl AsRef<str>) -> SmallVec<[u8; 32]> {
    let mut parsed_font_name_bytes = SmallVec::new();
    font_name
        .as_ref()
        .chars()
        .batching(|iter| {
            let ch = iter.next();
            match ch? {
                '\\' => iter.next(),
                '_' => Some(' '),
                _ => ch,
            }
        })
        .for_each(|ch| {
            let mut tmp_enc = [0u8; 4];
            ch.encode_utf8(&mut tmp_enc);
            parsed_font_name_bytes.extend_from_slice(&tmp_enc[0..ch.len_utf8()]);
        });
    parsed_font_name_bytes
}

fn points_to_pixels(value: f32) -> f32 {
    // Fonts in neovim are using points, not pixels.
    //
    // Skia docs is incorrectly stating it uses points, but uses pixels:
    // https://api.skia.org/classSkFont.html#a7e28a156a517d01bc608c14c761346bf
    // https://github.com/mono/SkiaSharp/issues/1147#issuecomment-587421201
    //
    // So, we need to convert points to pixels.
    //
    // In reality, this depends on DPI/PPI of monitor, but here we only care about converting
    // from points to pixels, so this is standard constant values.
    if cfg!(target_os = "macos") {
        // On macos points == pixels
        value
    } else {
        let pixels_per_inch = 96.0;
        let points_per_inch = 72.0;
        value * (pixels_per_inch / points_per_inch)
    }
}
