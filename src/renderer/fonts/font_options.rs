use itertools::Itertools;

use ordered_float::OrderedFloat;
use rkyv::{with::Skip, Archive, Deserialize, Serialize};
//use serde::{Serialize, Deserialize};
use smallvec::{smallvec, SmallVec};
use std::sync::Arc;
const DEFAULT_FONT_SIZE: f32 = 14.0;

#[derive(Clone, Debug, Archive, Serialize, Deserialize, Hash, Eq)]
pub struct FontParameters {
    pub size: OrderedFloat<f32>,
    pub bold: bool,
    pub italic: bool,
    pub allow_float_size: bool,
    pub emoji: bool,
    pub hinting: FontHinting,
    pub edging: FontEdging,
}

#[derive(Debug, PartialEq)]
pub struct SmallFontRegistry {
    pub font_list: Vec<String>,
}

/* Font options for sending over the wire. Refer to a list of font names to save space */
#[derive(Clone, Hash, Debug, Archive, Serialize, Deserialize, PartialEq, Eq)]
pub struct SmallFontOptions {
    pub family_id: u8,
    pub font_parameters: FontParameters,
    //#[with(Skip)]
    //font_list_ref: Option<Arc<SmallFontRegistry>>,
}
#[derive(Clone, Debug, Archive, Serialize, Deserialize)]
pub struct FontOptions {
    pub font_list: SmallVec<[SmallVec<[u8; 32]>; 8]>,
    pub font_parameters: FontParameters,
}

impl Default for SmallFontOptions {
    fn default() -> Self {
        SmallFontOptions {
            family_id: 0,
            font_parameters: FontParameters {
                size: OrderedFloat(DEFAULT_FONT_SIZE),
                bold: false,
                italic: false,
                emoji: false,
                allow_float_size: true,
                hinting: FontHinting::Normal,
                edging: FontEdging::AntiAlias,
            },
        }
    }
}

impl FontParameters {
    pub fn size(&self) -> f32 {
        f32::from(self.size)
    }
}
impl FontOptions {
    pub fn parse(guifont_setting: &str) -> FontOptions {
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
                bold,
                italic,
                allow_float_size,
                hinting,
                edging,
                emoji: false,
                size: OrderedFloat(points_to_pixels(size)),
            },
        }
    }

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
                bold: false,
                italic: false,
                allow_float_size: false,
                emoji: false,
                size: OrderedFloat(points_to_pixels(DEFAULT_FONT_SIZE)),
                hinting: FontHinting::default(),
                edging: FontEdging::default(),
            },
        }
    }
}
impl PartialEq for FontParameters {
    fn eq(&self, other: &Self) -> bool {
        (self.size - other.size).abs() < std::f32::EPSILON
            && self.bold == other.bold
            && self.italic == other.italic
            && self.edging == other.edging
            && self.hinting == other.hinting
    }
}
impl PartialEq for FontOptions {
    fn eq(&self, other: &Self) -> bool {
        self.font_list == other.font_list && self.font_parameters == other.font_parameters
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

#[derive(Clone, Debug, Hash, PartialEq, Eq, Archive, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Hash, PartialEq, Eq, Archive, Serialize, Deserialize)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_one_font_from_guifont_setting() {
        let guifont_setting = "Fira Code Mono";
        let font_options = FontOptions::parse(guifont_setting);

        assert_eq!(
            font_options.font_list.len(),
            1,
            "font list length should equal {}, but {}",
            font_options.font_list.len(),
            1
        );
    }

    #[test]
    fn test_parse_many_fonts_from_guifont_setting() {
        let guifont_setting = "Fira Code Mono,Console";
        let font_options = FontOptions::parse(guifont_setting);

        assert_eq!(
            font_options.font_list.len(),
            2,
            "font list length should equal {}, but {}",
            font_options.font_list.len(),
            1
        );
    }

    #[test]
    fn test_parse_edging_from_guifont_setting() {
        let guifont_setting = "Fira Code Mono:#e-subpixelantialias";
        let font_options = FontOptions::parse(guifont_setting);

        assert_eq!(
            font_options.font_parameters.edging,
            FontEdging::SubpixelAntiAlias,
            "font edging should equal {:?}, but {:?}",
            font_options.font_parameters.edging,
            FontEdging::SubpixelAntiAlias,
        );
    }

    #[test]
    fn test_parse_hinting_from_guifont_setting() {
        let guifont_setting = "Fira Code Mono:#h-slight";
        let font_options = FontOptions::parse(guifont_setting);

        assert_eq!(
            font_options.font_parameters.hinting,
            FontHinting::Slight,
            "font hinting should equal {:?}, but {:?}",
            font_options.font_parameters.hinting,
            FontHinting::Slight,
        );
    }
    #[test]
    fn test_parse_font_size_float_from_guifont_setting() {
        let guifont_setting = "Fira Code Mono:h15.5";
        let font_options = FontOptions::parse(guifont_setting);

        let font_size_pixels = points_to_pixels(15.5);
        assert_eq!(
            font_options.font_parameters.size, font_size_pixels,
            "font size should equal {}, but {}",
            font_size_pixels, font_options.font_parameters.size,
        );
    }

    #[test]
    #[allow(clippy::bool_assert_comparison)]
    fn test_parse_all_params_together_from_guifont_setting() {
        let guifont_setting = "Fira Code Mono:h15:b:i:#h-slight:#e-alias";
        let font_options = FontOptions::parse(guifont_setting);

        let font_size_pixels = points_to_pixels(15.0);
        assert_eq!(
            font_options.font_parameters.size, font_size_pixels,
            "font size should equal {}, but {}",
            font_size_pixels, font_options.font_parameters.size,
        );

        assert_eq!(
            font_options.font_parameters.bold, true,
            "bold should equal {}, but {}",
            font_options.font_parameters.bold, true,
        );

        assert_eq!(
            font_options.font_parameters.italic, true,
            "italic should equal {}, but {}",
            font_options.font_parameters.italic, true,
        );

        assert_eq!(
            font_options.font_parameters.edging,
            FontEdging::Alias,
            "font hinting should equal {:?}, but {:?}",
            font_options.font_parameters.hinting,
            FontEdging::Alias,
        );

        assert_eq!(
            font_options.font_parameters.hinting,
            FontHinting::Slight,
            "font hinting should equal {:?}, but {:?}",
            font_options.font_parameters.hinting,
            FontHinting::Slight,
        );
    }

    #[test]
    fn test_parse_font_name_with_escapes() {
        let without_escapes_or_specials_chars = parse_font_name("Fira Code Mono");
        let without_escapes = parse_font_name("Fira_Code_Mono");
        let with_escapes = parse_font_name(r"Fira\_Code\_Mono");
        let with_too_many_escapes = parse_font_name(r"Fira\\_Code\\_Mono");
        let ignored_escape_at_the_end = parse_font_name(r"Fira_Code_Mono\");

        assert_eq!(
            without_escapes_or_specials_chars, "Fira Code Mono",
            "font name should equal {}, but {}",
            without_escapes_or_specials_chars, "Fira Code Mono"
        );

        assert_eq!(
            without_escapes, "Fira Code Mono",
            "font name should equal {}, but {}",
            without_escapes, "Fira Code Mono"
        );

        assert_eq!(
            with_escapes, "Fira_Code_Mono",
            "font name should equal {}, but {}",
            with_escapes, "Fira_Code_Mono"
        );

        assert_eq!(
            with_too_many_escapes, "Fira\\ Code\\ Mono",
            "font name should equal {}, but {}",
            with_too_many_escapes, "Fira\\ Code\\ Mono"
        );

        assert_eq!(
            ignored_escape_at_the_end, "Fira Code Mono",
            "font name should equal {}, but {}",
            ignored_escape_at_the_end, "Fira Code Mono"
        )
    }
}
