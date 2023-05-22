use crate::font_options::FontOptions;
use crate::gfx::{FontPaint, PointF32};
use crate::swash_font::SwashFont;
use crate::text::{
    FontParameters, ShapableString, ShapedStringMetadata, ShapedTextBlock, ShapedTextGlyph,
    SmallFontOptions, SHAPABLE_STRING_ALLOC_LEN, SHAPABLE_STRING_ALLOC_RUNS,
};
use smallvec::SmallVec;
use std::env;

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use parking_lot::{RwLock, RwLockUpgradableReadGuard};

use log::trace;

use ordered_float::OrderedFloat;
/* use skia_safe::{
    graphics::{font_cache_limit, font_cache_used, set_font_cache_limit},
    TextBlob, TextBlobBuilder,
};*/
use swash::{
    shape::ShapeContext,
    text::{
        cluster::{CharCluster, Parser, Status, Token},
        Script,
    },
    Metrics,
};
use unicode_segmentation::UnicodeSegmentation;

/* Make a shaper per font (variant), that contains cached shaped info etc.
then make an algorithm that works on multiple shapers to shape multifont text */
static DEFAULT_FONT: &[u8] = include_bytes!("../../assets/fonts/FiraCodeNerdFont-Regular.ttf");
pub const DEFAULT_FONT_NAME_LENGTH: usize = 32;

/* TODO: This should ideally point to the configuration / shared directory for the helix/helicoid editor
however currently we just use the current executable path as a base. */
pub fn base_asset_path() -> PathBuf {
    env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("assets")
}
#[derive(new, Clone, Hash, PartialEq, Eq, Debug)]
struct ShapeKey {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
}

#[derive(Debug)]
pub struct KeyedSwashFont {
    pub key: Option<SmallVec<[u8; DEFAULT_FONT_NAME_LENGTH]>>,
    pub swash_font: SwashFont,
}
#[derive(new, Clone, Hash, PartialEq, Eq, Debug)]
pub struct BackupClusterKey {
    text: SmallVec<[u8; 8]>,
    font_info: SmallFontOptions,
}
struct CachingShaperInner {
    options: FontOptions,
    font_cache: HashMap<SmallFontOptions, KeyedSwashFont>,
    font_names: Vec<Option<String>>,
    default_font: KeyedSwashFont,
    scale_factor: f32,
}
pub struct CachingShaper {
    inner: Arc<RwLock<CachingShaperInner>>,
    shape_context: ShapeContext,
}

impl CachingShaper {
    pub fn new(scale_factor: f32, unscaled_font_size: f32) -> CachingShaper {
        let mut options = FontOptions::default();
        let scaled_font_size = unscaled_font_size * scale_factor;
        options.font_parameters.size = OrderedFloat(scaled_font_size);
        let default_font =
            KeyedSwashFont::load_keyed(&base_asset_path(), Default::default(), scaled_font_size)
                .unwrap();
        let shaper = CachingShaper {
            inner: Arc::new(RwLock::new(CachingShaperInner {
                options,
                font_cache: Default::default(),
                font_names: Vec::new(),
                default_font,
                scale_factor,
            })),
            shape_context: ShapeContext::new(),
        };
        shaper.cache_fonts(&ShapableString::default(), &None);
        //        shaper.reset_font_loader();
        shaper
    }
    /*
        fn current_font_pair(&mut self) -> Arc<FontPair> {
            self.font_loader
                .get_or_load(&FontKey {
                    italic: false,
                    bold: false,
                    family_name: self.options.primary_font(),
                    hinting: self.options.font_parameters.hinting.clone(),
                    edging: self.options.font_parameters.edging.clone(),
                })
                .unwrap_or_else(|| {
                    self.font_loader
                        .get_or_load(&FontKey::default())
                        .expect("Could not load default font")
                })
        }
    */
    pub fn change_scale_factor(&self, scale_factor: f32) {
        let mut inner = self.inner.write();
        inner.scale_factor = scale_factor;
    }

    pub fn current_size(&self) -> f32 {
        let inner = self.inner.read();
        inner.options.font_parameters.size() * inner.scale_factor
    }
    pub fn current_scale_factor(&self) -> f32 {
        let inner = self.inner.read();
        inner.scale_factor
    }

    fn _reset_font_loader(&mut self) {
        let mut inner = self.inner.write();
        inner.font_names.clear();
    }

    pub fn font_names(&self) -> Vec<String> {
        let inner = self.inner.read();
        inner
            .font_names
            .iter()
            .filter_map(|s| s.as_ref().map(|s| s.clone()))
            .collect()
    }
    pub fn set_font_key(&mut self, font_id: u8, font_name: String) {
        let mut inner = self.inner.write();
        if font_id as usize >= inner.font_names.len() {
            inner.font_names.resize(font_id as usize + 1, None);
        }
        inner.font_names[font_id as usize] = Some(font_name);
    }

    fn cache_font_for_index(&self, options: &SmallFontOptions) -> bool {
        //        let font_key =
        let inner = self.inner.upgradable_read();
        if !inner.font_cache.contains_key(options) {
            if let Some(font_family_name) = inner.font_names.get(options.family_id as usize) {
                if let Some(font_family_name) = font_family_name {
                    let font = KeyedSwashFont::load_keyed(
                        &base_asset_path(),
                        Some(font_family_name.clone()),
                        options.font_parameters.size(),
                    );
                    if let Some(font) = font {
                        let mut inner_write = RwLockUpgradableReadGuard::upgrade(inner);
                        assert!(inner_write
                            .font_cache
                            .insert(options.clone(), font)
                            .is_none());
                        return true;
                    }
                }
            }
        } else {
            return true;
        }
        false
    }

    pub fn info(&mut self, font_options: &SmallFontOptions) -> Option<(Metrics, f32)> {
        //let font_pair = self.current_font_pair();
        /* Ensure font is loaded (if possible) */
        let _ = self.cache_font_for_index(font_options);
        //        let current_size = self.current_size();
        let Self {
            inner,
            shape_context,
            ..
        } = self;
        let inner_read = inner.read();
        inner_read.font_cache.get(font_options).map(|font| {
            let mut shaper = shape_context
                .builder(font.swash_font.as_ref())
                .size(f32::from(font_options.font_parameters.size))
                .build();
            shaper.add_str("M");
            let metrics = shaper.metrics();
            let mut advance = metrics.average_width;
            shaper.shape_with(|cluster| {
                advance = cluster
                    .glyphs
                    .first()
                    .map_or(metrics.average_width, |g| g.advance);
            });
            (metrics, advance)
        })
    }

    fn _metrics(&mut self, font_options: &SmallFontOptions) -> Option<Metrics> {
        self.info(font_options).map(|i| i.0)
    }
    pub fn default_parameters(&self) -> FontParameters {
        let inner = self.inner.read();
        let mut fp = inner.options.font_parameters.clone();
        fp.size = OrderedFloat(self.current_size());
        fp
    }
    /*
        pub fn font_base_dimensions(&mut self) -> (u64, u64) {
            let (metrics, glyph_advance) = self.info();
            let font_height = (metrics.ascent + metrics.descent + metrics.leading).ceil() as u64;
            let font_width = (glyph_advance + 0.5).floor() as u64;

            (font_width, font_height)
        }

        pub fn underline_position(&mut self) -> u64 {
            self.metrics().underline_offset as u64
        }

        pub fn y_adjustment(&mut self) -> u64 {
            let metrics = self.metrics();
            (metrics.ascent + metrics.leading).ceil() as u64
        }
    */
    /* Make sure that all fonts that may be needed for building clusters are cached */
    fn cache_fonts(&self, text: &ShapableString, backup_font_families: &Option<SmallVec<[u8; 8]>>) {
        for meta_run in text.metadata_runs.iter() {
            let _ = self.cache_font_for_index(&meta_run.font_info);
            //let Some(specified_font) = self.font_cache.get(&meta_run.font_info) else {return (SmallVec::new(), SmallVec::new())};
            if let Some(font_list) = backup_font_families.as_ref() {
                for font_id in font_list {
                    let mut modified_font_options = meta_run.font_info.clone();
                    modified_font_options.family_id = *font_id;
                    let _ = self.cache_font_for_index(&modified_font_options);
                }
            } else {
                let font_names_len = {
                    /* Make sure to relase inner before calling get_load_font_for_index to avoid deadlock */
                    let inner = self.inner.read();
                    inner.font_names.len()
                };
                for font_id in 0..font_names_len {
                    let mut modified_font_options = meta_run.font_info.clone();
                    modified_font_options.family_id = font_id as u8;
                    let _ = self.cache_font_for_index(&modified_font_options);
                }
            }
        }
    }
    fn build_clusters(
        inner: &CachingShaperInner,
        text: &ShapableString,
        meta_run_index: usize,
        meta_run_start: usize,
        backup_font_families: &Option<SmallVec<[u8; 8]>>,
    ) -> (
        SmallVec<[CharCluster; SHAPABLE_STRING_ALLOC_LEN]>,
        SmallVec<[ShapedStringMetadata; SHAPABLE_STRING_ALLOC_RUNS]>,
    ) {
        let _cluster = CharCluster::new();
        let meta_run = &text.metadata_runs[meta_run_index];
        let text_str_data =
            &text.text[meta_run_start..(meta_run_start + meta_run.substring_length as usize)];
        let text_str = std::str::from_utf8(text_str_data).unwrap();
        // Enumerate the characters storing the glyph index in the user data so that we can position
        // glyphs according to Neovim's grid rules
        let mut character_index = 0;
        let mut default_parser = Parser::new(
            Script::Latin,
            [Token {
                ch: '#',
                ..Default::default()
            }]
            .into_iter(),
        );
        let mut default_cluster = CharCluster::new();
        default_parser.next(&mut default_cluster);

        let mut parser = Parser::new(
            Script::Latin,
            text_str
                .graphemes(true)
                .enumerate()
                .flat_map(|(glyph_index, unicode_segment)| {
                    unicode_segment.chars().map(move |character| {
                        let token = Token {
                            ch: character,
                            offset: character_index as u32,
                            len: character.len_utf8() as u8,
                            info: character.into(),
                            data: glyph_index as u32,
                        };
                        character_index += 1;
                        token
                    })
                }),
        );

        let mut results: SmallVec<[(CharCluster, SmallFontOptions); SHAPABLE_STRING_ALLOC_LEN]> =
            SmallVec::new();
        let specified_font = inner
            .font_cache
            .get(&meta_run.font_info)
            .unwrap_or(&inner.default_font);
        let mut cluster = CharCluster::new();
        'cluster: while parser.next(&mut cluster) {
            // TODO: Don't redo this work for every cluster. Save it some how
            // Create font fallback list
            /*            if font_fallback_keys.is_none() {
                font_fallback_keys = Some(make_fallback_list(self.options));
            }*/
            //let mut font_fallback_keys = Vec::new();

            // The simplest case is if the cluster is available in the specified font, then no more work is needed
            let mut best = None;
            let charmap = specified_font.swash_font.as_ref().charmap();
            match cluster.map(|ch| charmap.map(ch)) {
                Status::Complete => {
                    results.push((cluster.to_owned(), meta_run.font_info.clone()));
                    continue 'cluster;
                }
                Status::Keep => best = Some(meta_run.font_info.clone()),
                Status::Discard => {}
            }

            if let Some(font_list) = &backup_font_families {
                for font_id in font_list {
                    let mut modified_font_options = meta_run.font_info.clone();
                    modified_font_options.family_id = *font_id;
                    if let Some(list_font) = inner.font_cache.get(&modified_font_options) {
                        let charmap = list_font.swash_font.as_ref().charmap();
                        match cluster.map(|ch| charmap.map(ch)) {
                            Status::Complete => {
                                results.push((cluster.to_owned(), modified_font_options));
                                continue 'cluster;
                            }
                            Status::Keep => best = Some(modified_font_options),
                            Status::Discard => {}
                        }
                    }
                }
            } else {
                /* If no backup font families are specified, just try all available fonts */
                for font_id in 0..inner.font_names.len() {
                    let mut modified_font_options = meta_run.font_info.clone();
                    modified_font_options.family_id = font_id as u8;
                    if let Some(list_font) = inner.font_cache.get(&modified_font_options) {
                        let charmap = list_font.swash_font.as_ref().charmap();
                        match cluster.map(|ch| charmap.map(ch)) {
                            Status::Complete => {
                                results.push((cluster.to_owned(), modified_font_options));
                                continue 'cluster;
                            }
                            Status::Keep => best = Some(modified_font_options),
                            Status::Discard => {}
                        }
                    }
                }
            }

            if let Some(best) = best {
                results.push((cluster.to_owned(), best.clone()));
            } else {
                /*
                let fallback_character = cluster.chars()[0].ch;
                if let Some(fallback_font) =
                    self.font_loader
                        .load_font_for_character(bold, italic, fallback_character)
                {
                    results.push((cluster.to_owned(), fallback_font));
                } else {
                    // Last Resort covers all of the unicode space so we will always have a fallback
                    results.push((
                        cluster.to_owned(),
                        self.font_loader.get_or_load_last_resort(),
                    ));
                }*/
                //                CharCluster()
                results.push((default_cluster.to_owned(), meta_run.font_info.clone()));
                log::warn!(
                    "Could not shape character: {}, using dummy",
                    cluster.chars()[0].ch
                );
            }
        }
        let mut result_metadatas: SmallVec<[ShapedStringMetadata; SHAPABLE_STRING_ALLOC_RUNS]> =
            SmallVec::new();
        let mut last_result_metadata: Option<SmallFontOptions> = None;
        let mut last_result_metadata_start = 0;

        // Now we have to group clusters by the font used so that the shaper can actually form
        // ligatures across clusters
        /*        let mut grouped_results = Vec::new();
        let mut current_group = Vec::new();
        let mut current_font_option = None;
        if last_result_metadata.is_none(){
            last_result_metadata = results
        }*/
        //        let mut current_offset = 0;

        if let Some((_, font_info)) = results.first() {
            last_result_metadata = Some(font_info.clone());
            last_result_metadata_start = 0;
        }

        for (cluster_index, (_, font_info)) in results.iter().enumerate() {
            if last_result_metadata.as_ref() != Some(font_info) {
                result_metadatas.push(ShapedStringMetadata {
                    substring_length: (cluster_index - last_result_metadata_start) as u16,
                    font_info: last_result_metadata.take().unwrap(),
                    paint: FontPaint::default(),
                    advance_x: 0,
                    advance_y: 0,
                    baseline_y: 0,
                });
                last_result_metadata = Some(font_info.clone());
                last_result_metadata_start = cluster_index;
            }
        }

        if last_result_metadata.is_some() {
            result_metadatas.push(ShapedStringMetadata {
                substring_length: (results.len() - last_result_metadata_start) as u16,
                font_info: last_result_metadata.take().unwrap(),
                paint: FontPaint::default(),
                advance_x: 0,
                advance_y: 0,
                baseline_y: 0,
            });
        }

        let mut result_clusters: SmallVec<[CharCluster; SHAPABLE_STRING_ALLOC_LEN]> =
            SmallVec::new();
        result_clusters.reserve(results.len());
        result_clusters.extend(results.drain(..).map(|(c, _)| c));

        (result_clusters, result_metadatas)
    }

    /*    pub fn adjust_font_cache_size(&self) {
        let current_font_cache_size = font_cache_limit() as f32;
        let percent_font_cache_used = font_cache_used() as f32 / current_font_cache_size;
        if percent_font_cache_used > 0.9 {
            warn!(
                "Font cache is {}% full, increasing cache size",
                percent_font_cache_used * 100.0
            );
            set_font_cache_limit((percent_font_cache_used * 1.5) as usize);
        }
    }*/

    pub fn shape(
        &mut self,
        text: &ShapableString,
        backup_font_families: &Option<SmallVec<[u8; 8]>>,
    ) -> ShapedTextBlock {
        let _current_size = self.current_size();
        //let (glyph_width, ..) = self.font_base_dimensions();

        //        let mut resulting_blobs = Vec::new();
        let mut resulting_block: ShapedTextBlock = Default::default();

        trace!("Shaping text: {:?}", text);
        self.cache_fonts(text, &backup_font_families);
        let mut current_text_offset = 0;
        let inner = self.inner.read();
        let mut current_pixel_offset = 0f32;
        let mut max_y_advance = 0f32;

        for (run_index, run) in text.metadata_runs.iter().enumerate() {
            let (mut cluster_list, shaped_string_list) = Self::build_clusters(
                &inner,
                text,
                run_index,
                current_text_offset,
                backup_font_families,
            );
            let mut current_cluster_offset = 0;
            for shaped_string_run in shaped_string_list {
                let font_options = &shaped_string_run.font_info;
                /* If this font is not valid it should not be returned by build clusters */
                let font = inner
                    .font_cache
                    .get(font_options)
                    .unwrap_or(&inner.default_font);
                let mut shaper = self
                    .shape_context
                    .builder(font.swash_font.as_ref())
                    .size(font_options.font_parameters.size())
                    //.normalized_coords(&ncoords)
                    .build();
                //let y_offset = font.swash_font.as_ref().metrics(shaper.normalized_coords()).ascent;
                let metrics = &shaper.metrics();
                let y_offset = metrics.ascent;
                let y_advance = metrics.ascent + metrics.descent + metrics.leading;
                let charmap = font.swash_font.as_ref().charmap();
                let cluster_list_slice = &mut cluster_list[current_cluster_offset
                    ..(current_cluster_offset + shaped_string_run.substring_length as usize)];
                max_y_advance = max_y_advance.max(y_advance);

                for char_cluster in cluster_list_slice.iter_mut() {
                    char_cluster.map(|ch| charmap.map(ch));
                    shaper.add_cluster(&char_cluster);
                }

                let start_pixel_offset = current_pixel_offset;
                let glyphs_start_offset = resulting_block.glyphs.len();
                shaper.shape_with(|glyph_cluster| {
                    for glyph in glyph_cluster.glyphs {
                        // TODO: Consider implementing word wrapping
                        // It could be interesting to look at info (word/line boundary etc.)
                        // and components for ligatures here
                        resulting_block.glyphs.push(ShapedTextGlyph::new(
                            glyph.id as u16,
                            glyph.x + current_pixel_offset,
                            glyph.y + y_offset,
                        ));
                        current_pixel_offset += glyph.advance;
                    }
                });
                /* Should we store some more metadata here that may be useful for drawing decorations
                related to the text, but not neccesarily transmitted over the wire to the drawing
                client? Like the total with of the text box (to know the size of the charaters)
                */

                let mut metadata = shaped_string_run.clone();
                metadata.substring_length =
                    (resulting_block.glyphs.len() - glyphs_start_offset) as u16;
                metadata.paint = run.paint.clone();
                metadata.set_advance(
                    current_pixel_offset - start_pixel_offset,
                    y_advance,
                    y_offset,
                );
                resulting_block.metadata_runs.push(metadata);
                current_cluster_offset += shaped_string_run.substring_length as usize;
            }
            current_text_offset += run.substring_length as usize;
        }
        resulting_block.extent = PointF32::new(current_pixel_offset, max_y_advance);
        trace!("Shaped text: {:?}", resulting_block);

        resulting_block
    }
    /*
            pub fn shape_cached(&mut self, text: String, bold: bool, italic: bool) -> &Vec<TextBlob> {
                let key = ShapeKey::new(text.clone(), bold, italic);

                if !self.blob_cache.contains(&key) {
                    let blobs = self.shape(text, bold, italic);
                    self.blob_cache.put(key.clone(), blobs);
                }

                self.blob_cache.get(&key).unwrap()
            }

    }*/
}
impl Clone for CachingShaper {
    fn clone(&self) -> Self {
        /* TODO: See if it is possible to optimize this for many clones
        and drops by some kind of pool */
        Self {
            inner: self.inner.clone(),
            shape_context: ShapeContext::new(),
        }
    }
}
impl KeyedSwashFont {
    fn _new(key: Option<&str>, swash_font: SwashFont) -> Self {
        Self {
            key: key.map(|s| SmallVec::from_slice(s.as_bytes())),
            swash_font,
        }
    }
    fn new_string(key: Option<String>, swash_font: SwashFont) -> Self {
        Self {
            key: key.map(|s| SmallVec::from_slice(s.as_bytes())),
            swash_font,
        }
    }
    fn load_keyed(base_directory: &PathBuf, name: Option<String>, _font_size: f32) -> Option<Self> {
        //        let font_style = font_style(font_key.bold, font_key.italic);
        if let Some(family_name) = &name {
            trace!("KSFLoading font {:?}", name);
            let font_file_path = base_directory
                .join("fonts")
                .join(format!("{}.ttf", family_name));
            //            let typeface = font_manager.match_family_style(family_name, font_style)?;
            let res = SwashFont::from_path(&font_file_path, 0)
                .map(|font| KeyedSwashFont::new_string(name.clone(), font));
            if res.is_none() {
                trace!("KSFLoading font failed: {:?}", font_file_path);
                let res_def = SwashFont::from_data(DEFAULT_FONT.to_vec(), 0)
                    .map(|font| KeyedSwashFont::new_string(name, font));
                trace!("Loaded default instead: {:?}", res_def);
                res_def
            } else {
                trace!("KSFLoading font succeeded: {:?}", font_file_path);
                res
            }
        } else {
            trace!("KSFLoading default font {:?}", name);
            SwashFont::from_data(DEFAULT_FONT.to_vec(), 0)
                .map(|font| KeyedSwashFont::new_string(name, font))
        }
    }
}
/*
fn make_fallback_list(
    font_name: &str,
    options: FontOptions,
    meta_run: &ShapedStringMetadata,
) -> Vec<FontKey> {
    // Create font fallback list
    let mut font_fallback_keys = Vec::new();

    // Add parsed fonts from guifont
    font_fallback_keys.extend(options.font_list.iter().map(|font_name| FontKey {
        size: OrderedFloat(DEFAULT_FONT_SIZE),
        italic: options.font_parameters.italic || meta_run.font_info.font_parameters.italic,
        bold: options.font_parameters.bold || meta_run.font_info.font_parameters.bold,
        family_name: Some(font_name.as_slice()),
        hinting: options.font_parameters.hinting.clone(),
        edging: options.font_parameters.edging.clone(),
    }));

    // Add default font
    font_fallback_keys.push(FontKey {
        size: OrderedFloat(DEFAULT_FONT_SIZE),
        italic: options.font_parameters.italic || meta_run.font_info.font_parameters.italic,
        bold: options.font_parameters.bold || meta_run.font_info.font_parameters.bold,
        family_name: None,
        hinting: options.font_parameters.hinting.clone(),
        edging: options.font_parameters.edging.clone(),
    });
    font_fallback_keys
}
*/
