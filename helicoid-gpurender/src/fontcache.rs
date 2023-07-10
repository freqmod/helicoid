use std::{borrow::BorrowMut, cell::RefCell};

use cosmic_text::{CacheKey, SubpixelBin, SwashCache};
use swash::{
    scale::{Render, ScaleContext, Source, StrikeWith},
    zeno::{Format, Vector},
    FontRef,
};
use wgpu::{Extent3d, Origin2d};

use crate::texture_atlases::{self, AtlasLocation, TextureAtlas, TextureAtlases};
pub trait FontOwner {
    fn swash_font(&self) -> FontRef<'_>;
}

thread_local! {
    static RENDER_LIST_HOST: RefCell<Vec<RenderSquare>> = RefCell::new(Vec::new());
}
pub struct FontCache<O>
where
    O: FontOwner,
{
    font: O,
    cache: TextureAtlases<SwashCacheKey>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SwashCacheKey {
    pub glyph_id: u16,
    pub font_size_bits: u32,
    pub x_bin: SubpixelBin,
    pub y_bin: SubpixelBin,
}

impl From<CacheKey> for SwashCacheKey {
    fn from(key: CacheKey) -> Self {
        SwashCacheKey {
            glyph_id: key.glyph_id,
            font_size_bits: key.font_size_bits,
            x_bin: key.x_bin,
            y_bin: key.y_bin,
        }
    }
}

impl<O> FontCache<O>
where
    O: FontOwner,
{
    pub fn new(font: O) -> Self {
        Self {
            font,
            cache: TextureAtlases::default(),
        }
    }
    pub fn update_cache<K, I>(&mut self, keys: I)
    where
        K: Into<SwashCacheKey> + Clone,
        I: ExactSizeIterator<Item = K>,
    {
        let mut key_meta = Vec::with_capacity(keys.len());
        let font_ref = self.font.swash_font();
        let metrics = font_ref.glyph_metrics(&[]);
        key_meta.extend(keys.map(|k| {
            let key: SwashCacheKey = Into::into(k.clone());
            (
                key,
                Extent3d {
                    width: metrics.advance_width(key.glyph_id).ceil() as u32, // These sizes are likely too big
                    height: metrics.advance_height(key.glyph_id).ceil() as u32,
                    depth_or_array_layers: 0,
                },
            )
        }));
        self.cache.increment_generation();
        key_meta.sort_by(|a, b| texture_atlases::insert_order(&a.1, &b.1));
        for (key, extent) in key_meta.iter() {
            match self.cache.insert_single(key.clone(), extent.clone()) {
                Ok(location) => {
                    /* Render font data info the cache */
                    self.render_to_location(&key, &location);
                }
                Err(e) => {
                    match e {
                        texture_atlases::InsertResult::NoMoreSpace => {
                            self.cache
                                .evict_outdated(&mut self.font.swash_font(), &Self::redraw);
                            match self.cache.insert_single(key.clone(), extent.clone()) {
                                Ok(location) => {
                                    /* Render font data info the cache */
                                    self.render_to_location(&key, &location);
                                }
                                Err(e) => {
                                    panic!("Handle no more space after eviction")
                                }
                            }
                        }
                        texture_atlases::InsertResult::AlreadyPresent => { /*_Assume the already present value is ok, and continue */
                        }
                    }
                }
            }
        }
    }

    fn redraw(
        font: &mut FontRef<'_>,
        key: &SwashCacheKey,
        loc: &AtlasLocation,
        atlas: &mut TextureAtlas<SwashCacheKey>,
    ) {
        Self::do_render_to_location(font, key, loc, atlas);
    }

    fn render_to_location(&mut self, key: &SwashCacheKey, location: &AtlasLocation) {
        Self::do_render_to_location(
            &self.font.swash_font(),
            key,
            location,
            self.cache.atlas(location).unwrap(),
        );
    }

    fn do_render_to_location(
        font: &FontRef<'_>,
        cache_key: &SwashCacheKey,
        location: &AtlasLocation,
        atlas: &mut TextureAtlas<SwashCacheKey>,
    ) {
        /* Use swash / cosmic text to runder to the texture */
        let mut context = ScaleContext::new(); // TODO: Move to class? for caching
                                               // Build the scaler
        let mut scaler = context
            .builder(*font)
            .size(f32::from_bits(cache_key.font_size_bits))
            .hint(true)
            .build();

        // Compute the fractional offset-- you'll likely want to quantize this
        // in a real renderer
        let offset = Vector::new(cache_key.x_bin.as_float(), cache_key.y_bin.as_float());

        // Select our source order
        let image = Render::new(&[
            // Color outline with the first palette
            Source::ColorOutline(0),
            // Color bitmap with best fit selection mode
            Source::ColorBitmap(StrikeWith::BestFit),
            // Standard scalable outline
            Source::Outline,
        ])
        // Select a subpixel format
        .format(Format::Alpha)
        // Apply the fractional offset
        .offset(offset)
        // Render the image
        .render(&mut scaler, cache_key.glyph_id)
        .unwrap();

        let width = image.placement.width;
        let height = image.placement.height;
        // TODO: Do we need to take placement offset into account?

        let mut data_view = atlas.tile_data_mut(location);
        for y in 0..height {
            data_view
                .row(y as u16)
                .copy_from_slice(&image.data[(y * width) as usize..((y + 1) * width) as usize]);
        }
    }

    pub fn render_run(&mut self, rs: &RenderSpec) -> Result<RenderedRun, RenderRunError> {
        /* Create a rendered run with lookups corresponding to all font elements */
        let mut rr = RenderedRun::default();
        rr.reset();
        RENDER_LIST_HOST.with(|host_vertices_tmp_cell| {
            let mut host_vertices_tmp = host_vertices_tmp_cell.borrow_mut();
            match rr.fill_render_run(
                rs,
                &mut self.font.swash_font(),
                &mut (*host_vertices_tmp),
                &mut self.cache,
            ) {
                Ok(_) => Ok(rr),
                Err(e) => match e {
                    RenderRunError::CharacterMissingInAtlas => {
                        /* This will result in many duplicate keys being added,
                        but duplicates are ignored, and deduping takes resources */
                        self.update_cache(rs.elements.iter().map(|e| e.key.clone()));
                        rr.fill_render_run(
                            rs,
                            &mut self.font.swash_font(),
                            &mut (*host_vertices_tmp),
                            &mut self.cache,
                        )
                        .unwrap();
                        Ok(rr)
                    }
                },
            }
        })
    }
}

#[derive(Debug)]
pub enum RenderRunError {
    CharacterMissingInAtlas,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RenderPoint {
    dx: u32,
    dy: u32,
    sx: u32,
    sy: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RenderSquare {
    // First triangle
    top_left1: RenderPoint,
    bottom_left1: RenderPoint,
    top_right1: RenderPoint,

    // Second Triangle
    top_right2: RenderPoint,
    bottom_right2: RenderPoint,
    bottom_left2: RenderPoint,
}

/* NB: It is up to the caller to use the rendered run with the correct atlas.
If a wrong atlas is used the wrong characters may be displayed */
#[derive(Debug, Default)]
pub struct RenderedRun {
    first_char_generation: Option<i32>,
    last_char_generation: Option<i32>,
    gpu_vertices: Option<wgpu::Buffer>,
}

pub struct RenderSpec {
    elements: Vec<RenderSpecElement>,
}
pub struct RenderSpecElement {
    key: SwashCacheKey,
    offset: Origin2d,
}

impl RenderSpec {
    pub fn unique_keys(&self) -> Vec<SwashCacheKey> {
        let mut keys = Vec::with_capacity(self.elements.len());
        keys.extend(self.elements.iter().map(|rse| rse.key.clone()));
        keys.sort();
        keys.dedup();
        keys
    }
}
impl RenderSquare {
    pub fn from_spec_element(element: &RenderSpecElement, atlas: &AtlasLocation) -> Self {
        Self {
            top_left1: RenderPoint {
                dx: element.offset.x,
                dy: element.offset.y,
                sx: atlas.origin.x,
                sy: atlas.origin.y,
            },
            bottom_left1: RenderPoint {
                dx: element.offset.x,
                dy: element.offset.y + atlas.extent.height,
                sx: atlas.origin.x,
                sy: atlas.origin.y + atlas.extent.height,
            },
            top_right1: RenderPoint {
                dx: element.offset.x + atlas.extent.width,
                dy: element.offset.y,
                sx: atlas.origin.x + atlas.extent.width,
                sy: atlas.origin.y,
            },
            top_right2: RenderPoint {
                dx: element.offset.x + atlas.extent.width,
                dy: element.offset.y,
                sx: atlas.origin.x + atlas.extent.width,
                sy: atlas.origin.y,
            },
            bottom_right2: RenderPoint {
                dx: element.offset.x + atlas.extent.width,
                dy: element.offset.y + atlas.extent.height,
                sx: atlas.origin.x + atlas.extent.width,
                sy: atlas.origin.y + atlas.extent.height,
            },
            bottom_left2: RenderPoint {
                dx: element.offset.x,
                dy: element.offset.y + atlas.extent.height,
                sx: atlas.origin.x,
                sy: atlas.origin.y + atlas.extent.height,
            },
        }
    }
}
impl RenderedRun {
    pub fn reset(&mut self) {
        self.first_char_generation = None;
        self.last_char_generation = None;
        self.gpu_vertices = None; // TODO: Consider if we can reuse the buffer allocation
    }

    pub fn fill_render_run(
        &mut self,
        spec: &RenderSpec,
        font: &mut FontRef<'_>,
        host_vertices_tmp: &mut Vec<RenderSquare>, // Temp memory used for transferring to GPU
        atlas: &mut TextureAtlases<SwashCacheKey>,
    ) -> Result<(), RenderRunError> {
        /* Assume that all elements are in the atlas.
        If one or more elements are missing, an error is returned. */

        self.reset();
        host_vertices_tmp.clear();
        host_vertices_tmp.try_reserve(spec.elements.len()).unwrap();

        for element in spec.elements.iter() {
            match atlas.look_up(&element.key) {
                Some(location) => {
                    if let Some(first_char_generation) = self.first_char_generation.as_mut() {
                        *first_char_generation = (*first_char_generation).min(location.generation);
                    } else {
                        self.first_char_generation = Some(location.generation);
                    }
                    if let Some(last_char_generation) = self.last_char_generation.as_mut() {
                        *last_char_generation = (*last_char_generation).max(location.generation);
                    } else {
                        self.last_char_generation = Some(location.generation);
                    }
                    host_vertices_tmp.push(RenderSquare::from_spec_element(element, location));
                }
                None => {
                    /* Caller: Populate atlas with all elements and retry */
                    return Err(RenderRunError::CharacterMissingInAtlas);
                }
            }
        }
        Ok(())
    }
}
