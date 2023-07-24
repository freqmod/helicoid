use std::{borrow::BorrowMut, cell::RefCell};

use cosmic_text::{CacheKey, SwashCache};
use lyon::geom::euclid::num::Ceil;
use swash::{
    scale::{Render, ScaleContext, Source, StrikeWith},
    zeno::{Format, Vector},
    FontRef,
};
use wgpu::{
    CompositeAlphaMode, Extent3d, Origin2d, SamplerDescriptor, TextureFormat, TextureViewDimension,
};

use crate::texture_atlases::{self, AtlasLocation, TextureAtlas, TextureAtlases};
pub trait FontOwner {
    fn swash_font(&self) -> FontRef<'_>;
}
const POINTS_PER_SQUARE: usize = 6;
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
pub enum SubpixelBin {
    Zero,
    One,
    Two,
    Three,
}
impl From<cosmic_text::SubpixelBin> for SubpixelBin {
    fn from(bin: cosmic_text::SubpixelBin) -> Self {
        match bin {
            cosmic_text::SubpixelBin::Zero => Self::Zero,
            cosmic_text::SubpixelBin::One => Self::One,
            cosmic_text::SubpixelBin::Two => Self::Two,
            cosmic_text::SubpixelBin::Three => Self::Three,
        }
    }
}
impl SubpixelBin {
    pub fn as_float(&self) -> f32 {
        match self {
            Self::Zero => 0.0,
            Self::One => 0.25,
            Self::Two => 0.5,
            Self::Three => 0.75,
        }
    }
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
            x_bin: SubpixelBin::from(key.x_bin),
            y_bin: SubpixelBin::from(key.y_bin),
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
    /*pub fn add_atlas(
        &mut self,
        texture: wgpu::Texture,
        view: wgpu::TextureView,
        sampler: wgpu::Sampler,
    ) */
    pub fn add_atlas(&mut self, dev: &wgpu::Device, extent: Extent3d) {
        let texture = Self::create_texture(
            dev,
            extent.width,
            extent.height,
            TextureFormat::Rgba8UnormSrgb,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("Simple font cache view"),
            format: Some(TextureFormat::Rgba8UnormSrgb),
            dimension: Some(TextureViewDimension::D2),
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: Some(1),
            base_array_layer: 0,
            array_layer_count: Some(1),
        });
        let sampler = dev.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        self.cache.add_atlas(texture, view, sampler);
    }
    fn placement_for_glyph(
        font: &FontRef<'_>,
        cache_key: &SwashCacheKey,
    ) -> swash::zeno::Placement {
        /* TODO: Is it possible to get the exent of a rendered glyph without actually rendering it? */
        /* Use swash / cosmic text to runder to the texture */
        let mut context = ScaleContext::new(); // TODO: Move to class? for caching
                                               // Build the scaler
                                               /*println!(
                                                   "Font scaler size: {}",
                                                   f32::from_bits(cache_key.font_size_bits)
                                               );*/
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
        image.placement
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
            //let scaled_metrics = metrics.scale(f32::from_bits(key.font_size_bits));
            let placement = Self::placement_for_glyph(&font_ref, &key);
            /*println!(
                "Metrics: I:{} W:{} H:{}",
                key.glyph_id,
                scaled_metrics.advance_width(key.glyph_id),
                scaled_metrics.advance_height(key.glyph_id)
            );*/

            (
                key,
                Extent3d {
                    width: placement.width, //scaled_metrics.advance_width(key.glyph_id).ceil() as u32,
                    height: placement.height, //scaled_metrics.advance_height(key.glyph_id).ceil() as u32,
                    depth_or_array_layers: 0,
                },
            )
        }));
        //println!("Cache keys: {:?}", key_meta);
        self.cache.increment_generation();
        key_meta.sort_by(|a, b| texture_atlases::insert_order(&a.1, &b.1));
        for (key, extent) in key_meta.iter() {
            match self.cache.insert_single(key.clone(), extent.clone()) {
                Ok(location) => {
                    //println!("Inserted: {:?}", &key);
                    /* Render font data info the cache */
                    self.render_to_location(&key, &location);
                }
                Err(e) => {
                    println!("Insert-err: {:?} {:?}", &key, e);
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
                                    panic!("Handle no more space after eviction: {:?}", e)
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
                                               /*println!(
                                                   "Font scaler size: {}",
                                                   f32::from_bits(cache_key.font_size_bits)
                                               );*/
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
            Source::ColorBitmap(StrikeWith::ExactSize),
            // Standard scalable outline
            Source::Outline,
        ])
        // Select a subpixel format
        .format(Format::Subpixel)
        // Apply the fractional offset
        .offset(offset)
        // Render the image
        .render(&mut scaler, cache_key.glyph_id)
        .unwrap();

        let width = (image.placement.width as i32) as u32;
        let height = (image.placement.height as i32) as u32;
        // TODO: Do we need to take placement offset into account?

        println!(
            "Render to loc placement: {}, {:?} W:{} H:{}",
            cache_key.glyph_id, image.placement, width, height
        );
        let mut data_view = atlas.tile_data_mut(location);
        let bpp = 4usize;
        for y in 0..height {
            let row = data_view.row(y as u16);
            let copy_width = (bpp * width as usize).min(row.len()) as u32;
            if copy_width != (bpp as u32) * width || copy_width != (row.len()) as u32 {
                println!(
                    "Render: {}=={} {}",
                    row.len(),
                    bpp as u32 * width,
                    copy_width
                );
            }
            row[0..copy_width as usize].copy_from_slice(
                &image.data[(y * bpp as u32 * width) as usize
                    ..((y * bpp as u32 * width) + copy_width) as usize],
            );
            println!("Rendered: {:?}", &row[0..copy_width as usize]);
        }
    }

    pub fn render_run(
        &mut self,
        dev: &wgpu::Device,
        rs: &RenderSpec,
    ) -> Result<RenderedRun, RenderRunError> {
        /* Create a rendered run with lookups corresponding to all font elements */
        let mut rr = RenderedRun::default();
        rr.reset();
        match rr.fill_render_run(rs, dev, &mut self.font.swash_font(), &mut self.cache) {
            Ok(_) => Ok(rr),
            Err(e) => match e {
                RenderRunError::CharacterMissingInAtlas => {
                    /* This will result in many duplicate keys being added,
                    but duplicates are ignored, and deduping takes (alloc) resources */
                    self.update_cache(rs.elements.iter().map(|e| e.key.clone()));
                    rr.fill_render_run(rs, dev, &mut self.font.swash_font(), &mut self.cache)
                        .unwrap();
                    Ok(rr)
                }
            },
        }
    }
    pub fn owner(&self) -> &O {
        &self.font
    }
    pub fn owner_mut(&mut self) -> &mut O {
        &mut self.font
    }
    /// Creates a texture that can be used for an atlas
    fn create_texture(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: TextureFormat,
    ) -> wgpu::Texture {
        let frame_descriptor = &wgpu::TextureDescriptor {
            label: Some("Frame descriptor"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        };

        device.create_texture(frame_descriptor)
    }
    pub fn atlas(&mut self, location: &AtlasLocation) -> Option<&mut TextureAtlas<SwashCacheKey>> {
        self.cache.atlas(location)
    }
    pub fn atlas_ref(&self, location: &AtlasLocation) -> Option<&TextureAtlas<SwashCacheKey>> {
        self.cache.atlas_ref(location)
    }
}

#[derive(Debug)]
pub enum RenderRunError {
    CharacterMissingInAtlas,
}
#[repr(C)]
#[derive(Clone, Copy, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RenderPoint {
    dx: u32,
    dy: u32,
    sx: u32,
    sy: u32,
}

impl std::fmt::Debug for RenderPoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RenderPoint")
            .field("dx", &f32::from_bits(self.dx))
            .field("dy", &f32::from_bits(self.dy))
            .field("sx", &f32::from_bits(self.sx))
            .field("sy", &f32::from_bits(self.sy))
            .finish()
    }
}
unsafe impl bytemuck::Pod for RenderPoint {}
unsafe impl bytemuck::Zeroable for RenderPoint {}

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

unsafe impl bytemuck::Pod for RenderSquare {}
unsafe impl bytemuck::Zeroable for RenderSquare {}

/* NB: It is up to the caller to use the rendered run with the correct atlas.
If a wrong atlas is used the wrong characters may be displayed */
#[derive(Debug, Default)]
pub struct RenderedRun {
    first_char_generation: Option<i32>,
    last_char_generation: Option<i32>,
    pub gpu_vertices: Option<wgpu::Buffer>,
    pub gpu_indices: Option<wgpu::Buffer>,
    pub host_vertices: Vec<RenderSquare>,
    pub host_indices: Vec<u32>,
}

#[derive(Debug, Default)]
pub struct RenderSpec {
    elements: Vec<RenderSpecElement>,
}
#[derive(Debug)]
pub struct RenderSpecElement {
    pub key: SwashCacheKey,
    pub offset: Origin2d,
}

impl RenderSpec {
    pub fn unique_keys(&self) -> Vec<SwashCacheKey> {
        let mut keys = Vec::with_capacity(self.elements.len());
        keys.extend(self.elements.iter().map(|rse| rse.key.clone()));
        keys.sort();
        keys.dedup();
        keys
    }
    pub fn add_element(&mut self, element: RenderSpecElement) {
        self.elements.push(element);
    }
    pub fn len(&self) -> usize {
        self.elements.len()
    }
}
impl RenderSquare {
    pub fn from_spec_element(
        element: &RenderSpecElement,
        atlas: &AtlasLocation,
        texture_width: u32,
        texture_height: u32,
    ) -> Self {
        let tw = texture_width as f32;
        let th = texture_height as f32;
        Self {
            top_left1: RenderPoint {
                dx: (element.offset.x as f32).to_bits(),
                dy: (element.offset.y as f32).to_bits(),
                sx: (atlas.origin.x as f32 / tw).to_bits(),
                sy: (atlas.origin.y as f32 / th).to_bits(),
            },
            bottom_left1: RenderPoint {
                dx: (element.offset.x as f32).to_bits(),
                dy: ((element.offset.y + atlas.extent.height) as f32).to_bits(),
                sx: (atlas.origin.x as f32 / tw).to_bits(),
                sy: ((atlas.origin.y + atlas.extent.height) as f32 / tw).to_bits(),
            },
            top_right1: RenderPoint {
                dx: ((element.offset.x + atlas.extent.width) as f32).to_bits(),
                dy: ((element.offset.y) as f32).to_bits(),
                sx: ((atlas.origin.x + atlas.extent.width) as f32 / tw).to_bits(),
                sy: ((atlas.origin.y) as f32 / th).to_bits(),
            },
            top_right2: RenderPoint {
                dx: ((element.offset.x + atlas.extent.width) as f32).to_bits(),
                dy: ((element.offset.y) as f32).to_bits(),
                sx: ((atlas.origin.x + atlas.extent.width) as f32 / tw).to_bits(),
                sy: ((atlas.origin.y) as f32 / th).to_bits(),
            },
            bottom_right2: RenderPoint {
                dx: ((element.offset.x + atlas.extent.width) as f32).to_bits(),
                dy: ((element.offset.y + atlas.extent.height) as f32).to_bits(),
                sx: ((atlas.origin.x + atlas.extent.width) as f32 / tw).to_bits(),
                sy: ((atlas.origin.y + atlas.extent.height) as f32 / th).to_bits(),
            },
            bottom_left2: RenderPoint {
                dx: ((element.offset.x) as f32).to_bits(),
                dy: ((element.offset.y + atlas.extent.height) as f32).to_bits(),
                sx: ((atlas.origin.x) as f32 / tw).to_bits(),
                sy: ((atlas.origin.y + atlas.extent.height) as f32 / th).to_bits(),
            },
        }
    }
}
impl RenderedRun {
    pub fn reset(&mut self) {
        self.first_char_generation = None;
        self.last_char_generation = None;
        self.host_vertices.clear();
        self.host_indices.clear();
    }

    fn ensure_buffer(&mut self, dev: &wgpu::Device, spec_elements: usize) {
        let bufsize_vertices = (spec_elements * std::mem::size_of::<RenderSquare>()) as u64;
        let bufsize_indices =
            (POINTS_PER_SQUARE * spec_elements * std::mem::size_of::<u32>()) as u64;
        if let Some(buffer) = self.gpu_vertices.as_ref() {
            if buffer.size() >= bufsize_vertices {
                return;
            }
        }
        if let Some(buffer) = self.gpu_indices.as_ref() {
            if buffer.size() >= bufsize_indices {
                return;
            }
        }
        // If there is no existing big enough buffer, create another one
        self.gpu_vertices = Some(dev.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Text globals vertex buffer"),
            size: bufsize_vertices,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::VERTEX,
            mapped_at_creation: false,
        }));
        self.gpu_indices = Some(dev.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Text globals index buffer"),
            size: bufsize_indices,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::INDEX,
            mapped_at_creation: false,
        }));
    }

    pub fn fill_render_run(
        &mut self,
        spec: &RenderSpec,
        dev: &wgpu::Device,
        _font: &mut FontRef<'_>,
        atlas: &mut TextureAtlases<SwashCacheKey>,
    ) -> Result<(), RenderRunError> {
        /* Assume that all elements are in the atlas.
        If one or more elements are missing, an error is returned. */

        self.reset();
        self.ensure_buffer(dev, spec.len());
        self.host_vertices.try_reserve(spec.elements.len()).unwrap();
        self.host_indices.try_reserve(spec.elements.len()).unwrap();

        for (idx, element) in spec.elements.iter().enumerate() {
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
                    let texture = atlas.atlas_ref(&location).unwrap().texture();
                    let spec_element = RenderSquare::from_spec_element(
                        element,
                        &location,
                        texture.map(|t| t.width()).unwrap_or(1),
                        texture.map(|t| t.height()).unwrap_or(1),
                    );
                    println!("SE ({:?}): {:?}", location, spec_element);
                    self.host_vertices.push(spec_element);
                    for x in 0..POINTS_PER_SQUARE {
                        self.host_indices
                            .push(((idx * POINTS_PER_SQUARE) + x) as u32);
                    }
                }
                None => {
                    /* Caller: Populate atlas with all elements and retry */
                    println!("Missing in atlas: {:?}", element.key);
                    return Err(RenderRunError::CharacterMissingInAtlas);
                }
            }
        }
        Ok(())
    }
    pub fn queue_write_buffer(&mut self, queue: &wgpu::Queue) {
        let Some(gpu_vertices ) = self.gpu_vertices.as_mut() else {return};
        let Some(gpu_indices) = self.gpu_indices.as_mut() else {return};
        // TODO: Skip if not updated?
        queue.write_buffer(
            &gpu_vertices,
            0,
            bytemuck::cast_slice(self.host_vertices.as_slice()),
        );
        queue.write_buffer(
            &gpu_indices,
            0,
            bytemuck::cast_slice(self.host_indices.as_slice()),
        );
    }
}
#[cfg(test)]
mod tests {
    use std::{env, path::PathBuf};

    use crate::{swash_font::SwashFont, texture_map::TextureCoordinate2D};

    use super::*;

    pub fn base_asset_path() -> PathBuf {
        env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("assets")
    }
    #[test]
    fn render_string_with_font() {
        //        let
        let font_scale_f: f32 = 12.0; //2.0;
        println!(
            "FP: {:?} : Sf: {} B:{} ",
            &base_asset_path().join("fonts").join("AnonymiceNerd.ttf"),
            font_scale_f,
            font_scale_f.to_bits()
        );
        let font = SwashFont::from_path(
            &base_asset_path().join("fonts").join("AnonymiceNerd.ttf"),
            0,
        )
        .unwrap();
        let mut font_cache = FontCache::new(font);
        let mut spec = RenderSpec::default();
        font_cache
            .cache
            .add_textureless_atlas(TextureCoordinate2D { x: 1024, y: 1024 });

        let char_width = font_cache
            .owner()
            .swash_font()
            .metrics(&[])
            .scale(font_scale_f)
            .average_width;

        for x in 0..200 {
            spec.elements.push(RenderSpecElement {
                key: SwashCacheKey {
                    glyph_id: 120 + x as u16,
                    font_size_bits: font_scale_f.to_bits(),
                    x_bin: SubpixelBin::Zero,
                    y_bin: SubpixelBin::Zero,
                },
                offset: Origin2d {
                    x: char_width as u32 * x,
                    y: 0,
                },
            })
        }
        // TODO: Fill render spec with some default (statically shaped) data
        //let run = font_cache.render_run(&spec).unwrap();
        //assert_eq!(run.gpu_vertices.len(), spec.elements.len());
        //println!("Run: {:?}", run);
    }
}
