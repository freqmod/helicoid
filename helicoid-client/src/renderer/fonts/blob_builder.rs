use half::f16;
use helicoid_protocol::caching_shaper::base_asset_path;
use helicoid_protocol::font_options::FontOptions;
use helicoid_protocol::text::{
    FontEdging, FontHinting, FontParameters, ShapedTextBlock, SmallFontOptions,
};
use ordered_float::OrderedFloat;
use smallvec::SmallVec;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::{collections::HashMap, path::PathBuf};

use log::{debug, trace, warn};
use lru::LruCache;
use skia_safe::{
    font::Edging as SkiaEdging,
    graphics::{font_cache_limit, font_cache_used, set_font_cache_limit},
    Data, Font, FontHinting as SkiaHinting, FontMgr, FontStyle, TextBlob, TextBlobBuilder,
    Typeface,
};
use swash::{
    shape::ShapeContext,
    text::{
        cluster::{CharCluster, Parser, Status, Token},
        Script,
    },
    Metrics,
};
use unicode_segmentation::UnicodeSegmentation;

//use {caching_shaper::base_asset_path, font_loader::*, font_options::*};

static DEFAULT_FONT: &[u8] =
    include_bytes!("../../../../assets/fonts/FiraCodeNerdFont-Regular.ttf");
static LAST_RESORT_FONT: &[u8] = include_bytes!("../../../../assets/fonts/LastResort-Regular.ttf");
static DEFAULT_FONT_SIZE: f32 = 12.0f32;

pub struct ShapedBlobBuilder {
    options: FontOptions,
    //skia: FontLoader,
    blob_cache: LruCache<ShapedTextBlock, Vec<TextBlob>>,
    shape_context: ShapeContext,
    //scale_factor: f32,
    font_cache: HashMap<SmallFontOptions, KeyedFont>,
    font_names: Vec<Option<String>>,
    default_font: KeyedFont,
    font_manager: FontMgr,
    //    fudge_factor: f32,
}

impl ShapedBlobBuilder {
    pub fn new() -> ShapedBlobBuilder {
        let mut font_manager = FontMgr::new();
        let options = FontOptions::default();
        let font_size = options.font_parameters.size;
        let default_font = KeyedFont::load_keyed(
            &mut font_manager,
            &base_asset_path(),
            Default::default(),
            DEFAULT_FONT_SIZE,
        )
        .unwrap();
        let mut shaper = ShapedBlobBuilder {
            options,
            //font_loader: FontLoader::new(font_size),
            blob_cache: LruCache::new(NonZeroUsize::new(10000).unwrap()),
            shape_context: ShapeContext::new(),
            font_cache: Default::default(),
            font_names: Vec::new(),
            default_font,
            font_manager,
            //fudge_factor: 1.0,
        };
        shaper.reset_font_loader();
        shaper
    }

    pub fn current_nsize(&self) -> f32 {
        f32::from(self.options.font_parameters.size)
    }

    fn reset_font_loader(&mut self) {
        self.font_names.clear();
        self.blob_cache.clear();
        // clear font manager?
    }

    pub fn font_names(&self) -> Vec<String> {
        self.font_names
            .iter()
            .filter_map(|s| s.as_ref().map(|s| s.clone()))
            .collect()
    }

    pub fn set_font_key(&mut self, font_id: u8, font_name: String) {
        if font_id as usize >= self.font_names.len() {
            self.font_names.resize(font_id as usize + 1, None);
        }
        self.font_names[font_id as usize] = Some(font_name);
    }
    pub fn adjust_font_cache_size(&self) {
        let current_font_cache_size = font_cache_limit() as f32;
        let percent_font_cache_used = font_cache_used() as f32 / current_font_cache_size;
        if percent_font_cache_used > 0.9 {
            warn!(
                "Font cache is {}% full, increasing cache size",
                percent_font_cache_used * 100.0
            );
            set_font_cache_limit((percent_font_cache_used * 1.5) as usize);
        }
    }

    pub fn bulid_blobs(&mut self, text: &ShapedTextBlock) -> Vec<TextBlob> {
        let mut resulting_blobs = Vec::new();

        trace!("Shaping text: {:?}", text);

        let mut current_run_start = 0;
        for shaped_string in text.metadata_runs.iter() {
            if shaped_string.substring_length == 0 {
                continue;
            }
            let current_run_end = current_run_start + shaped_string.substring_length as usize;
            let subglyphs = &text.glyphs[current_run_start..current_run_end];

            //let resolved_fonts =  SmallVec::<[u8;8]>::new();// self.fonts.get()
            let font: &KeyedFont = if let Some(font) = self.font_cache.get(&shaped_string.font_info)
            {
                font
            } else {
                //let loaded_font = self.font_loader.load(shaped_string.font_info);
                if let Some(font_name) = self
                    .font_names
                    .get(shaped_string.font_info.family_id as usize)
                {
                    if let Some(font_name) = font_name {
                        let loaded = KeyedFont::load_keyed(
                            &mut self.font_manager,
                            &base_asset_path(),
                            FontKey::from_parameters(
                                shaped_string.font_info.font_parameters.clone(),
                                Some(font_name.clone()),
                            ),
                            *shaped_string.font_info.font_parameters.size,
                        );
                        if let Some(loaded) = loaded {
                            self.font_cache
                                .insert(shaped_string.font_info.clone(), loaded);
                        } else {
                            log::trace!("Failed loading font with name: {}", font_name);
                        }
                    }
                }
                self.font_cache
                    .get(&shaped_string.font_info)
                    .unwrap_or(&self.default_font)
            };

            let mut blob_builder = TextBlobBuilder::new();
            let (glyphs, positions) = blob_builder.alloc_run_pos(
                &font.skia_font(),
                shaped_string.substring_length as usize,
                None,
            );
            for (i, shaped_glyph) in subglyphs.iter().enumerate() {
                glyphs[i] = shaped_glyph.glyph();
                positions[i].x = shaped_glyph.x();
                positions[i].y = shaped_glyph.y();
            }

            let blob = blob_builder.make();
            resulting_blobs.push(blob.expect("Could not create textblob"));

            current_run_start = current_run_end;
        }

        self.adjust_font_cache_size();

        resulting_blobs
    }
    /*
    pub fn shape_cached(&mut self, text: String, bold: bool, italic: bool) -> &Vec<TextBlob> {
        let key = ShapeKey::new(text.clone(), bold, italic);

        if !self.blob_cache.contains(&key) {
            let blobs = self.bulid_blobs(text, bold, italic);
            self.blob_cache.put(key.clone(), blobs);
        }

        self.blob_cache.get(&key).unwrap()
    }*/
}

#[derive(Debug, Default, Hash, PartialEq, Eq, Clone)]
pub struct FontKey {
    // TODO(smolck): Could make these private and add constructor method(s)?
    // Would theoretically make things safer I guess, but not sure . . .
    pub bold: bool,
    pub italic: bool,
    pub family_name: Option<String>,
    pub hinting: FontHinting,
    pub edging: FontEdging,
    pub size: OrderedFloat<f32>,
}

impl FontKey {
    pub fn from_parameters(parameters: FontParameters, family_name: Option<String>) -> Self {
        FontKey {
            bold: false,
            italic: false,
            family_name,
            hinting: parameters.hinting,
            edging: parameters.edging,
            size: parameters.size,
        }
    }
    pub fn size(&self) -> f32 {
        f32::from(self.size)
    }
}
#[derive(Debug, Default)]
pub struct FontSet {
    fonts: Vec<KeyedFont>,
}
#[derive(Debug)]
pub struct KeyedFont {
    pub key: FontKey,
    pub skia_font: Font,
}

impl KeyedFont {
    fn new(key: FontKey, mut skia_font: Font) -> Option<Self> {
        skia_font.set_subpixel(true);
        skia_font.set_hinting(font_hinting(&key.hinting));
        skia_font.set_edging(font_edging(&key.edging));

        let typeface = skia_font.typeface().unwrap();

        Some(Self { key, skia_font })
    }
    fn load_keyed(
        font_manager: &mut FontMgr,
        base_directory: &PathBuf,
        font_key: FontKey,
        font_size: f32,
    ) -> Option<Self> {
        let font_style = font_style(font_key.bold, font_key.italic);

        if let Some(family_name) = &font_key.family_name {
            trace!("Loading font {:?}", font_key);
            match font_manager.match_family_style(family_name, font_style) {
                Some(typeface) => {
                    /* Load typeface from system fonts */
                    KeyedFont::new(font_key, Font::from_typeface(typeface, font_size))
                }
                None => {
                    /* See if there is a local ttf file in assets we can load */
                    let font_file_path = base_directory
                        .join("fonts")
                        .join(format!("{}.ttf", family_name));
                    let font_data_vec = std::fs::read(font_file_path).ok()?;
                    let font_data = Data::new_copy(&font_data_vec.as_slice());
                    let typeface = Typeface::from_data(font_data, 0).unwrap();
                    KeyedFont::new(font_key, Font::from_typeface(typeface, font_size))
                }
            }
        } else {
            trace!("Loading default font {:?}", font_key);
            let data = Data::new_copy(DEFAULT_FONT);
            let typeface = Typeface::from_data(data, 0).unwrap();
            KeyedFont::new(font_key, Font::from_typeface(typeface, font_size))
        }
    }
    fn skia_font(&self) -> &Font {
        &self.skia_font
    }
    fn as_skia_font(self) -> Font {
        self.skia_font
    }
}

fn font_style(bold: bool, italic: bool) -> FontStyle {
    match (bold, italic) {
        (true, true) => FontStyle::bold_italic(),
        (false, true) => FontStyle::italic(),
        (true, false) => FontStyle::bold(),
        (false, false) => FontStyle::normal(),
    }
}

fn font_hinting(hinting: &FontHinting) -> SkiaHinting {
    match hinting {
        FontHinting::Full => SkiaHinting::Full,
        FontHinting::Slight => SkiaHinting::Slight,
        FontHinting::Normal => SkiaHinting::Normal,
        FontHinting::None => SkiaHinting::None,
    }
}

fn font_edging(edging: &FontEdging) -> SkiaEdging {
    match edging {
        FontEdging::AntiAlias => SkiaEdging::AntiAlias,
        FontEdging::Alias => SkiaEdging::Alias,
        FontEdging::SubpixelAntiAlias => SkiaEdging::SubpixelAntiAlias,
    }
}
