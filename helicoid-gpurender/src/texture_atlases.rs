use crate::texture_map::{self, PackedTextureCache, TextureCoordinate2D, TextureCoordinateInt};
use std::{cmp::Ordering, collections::HashMap, hash::Hash, ops::Range};
use wgpu::{Extent3d, ImageDataLayout, Origin2d, Sampler, Texture, TextureViewDescriptor};

const RGBA_BPP: usize = 4;
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct AtlasLocation {
    pub atlas: u8,
    /* Keeps track of how much of the area of the location that is used for padding between atlas entries */
    pub padding: u8,
    pub origin: Origin2d, // Includes padding
    pub extent: Extent3d, // Includes padding
    pub generation: i32,
}

#[derive(Debug)]
pub struct TextureInfo {
    pub texture: Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
}

//https://sotrh.github.io/learn-wgpu/beginner/tutorial5-textures/#loading-an-image-from-a-file
#[derive(Debug)]
pub struct BackedUpTexture {
    host_data: Vec<u8>,
    gpu: Option<TextureInfo>,
    gpu_outdated: bool,
    layout: ImageDataLayout,
    extent: wgpu::Extent3d,
    format: wgpu::TextureFormat,
    label: Option<String>,
}

impl Hash for BackedUpTexture {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.host_data.hash(state);
        self.gpu_outdated.hash(state);
        self.extent.hash(state);
        self.format.hash(state);
        self.label.hash(state);
    }
}
impl PartialEq for BackedUpTexture {
    fn eq(&self, other: &Self) -> bool {
        self.host_data.eq(&other.host_data)
            && self.gpu_outdated.eq(&other.gpu_outdated)
            && self.extent.eq(&other.extent)
            && self.format.eq(&other.format)
            && self.label.eq(&other.label)
    }
}

impl AtlasLocation {
    pub fn atlas_only(atlas: u8) -> Self {
        Self {
            atlas,
            padding: 0,
            origin: Origin2d::ZERO,
            extent: Default::default(),
            generation: 0,
        }
    }
    /* This function converts the location to a location without any padding.
    Origin and extent is transformed so it refers to the non padded area */
    pub fn apply_padding(&self) -> Self {
        Self {
            atlas: self.atlas,
            padding: 0,
            origin: Origin2d {
                x: self.origin.x + self.padding as u32,
                y: self.origin.y + self.padding as u32,
            },
            extent: Extent3d {
                width: self.extent.width - self.padding as u32 * 2,
                height: self.extent.height - self.padding as u32 * 2,
                depth_or_array_layers: self.extent.depth_or_array_layers,
            },
            generation: self.generation,
        }
    }
}
#[derive(Debug, PartialEq)]
pub struct TextureAtlas<K>
where
    K: PartialEq + Eq + Hash,
{
    manager: PackedTextureCache<K>,
    backed_up_texture: BackedUpTexture,
}

pub struct TextureAtlases<K>
where
    K: PartialEq + Eq + Hash,
{
    atlases: Vec<TextureAtlas<K>>,
    contents: HashMap<K, AtlasLocation>,
    current_generation: i32,
    last_eviction_generation: i32,
}

#[derive(Debug)]
pub enum InsertResult {
    AlreadyPresent,
    NoMoreSpace,
}

impl<K> Default for TextureAtlases<K>
where
    K: PartialEq + Eq + Hash + Clone,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K> TextureAtlases<K>
where
    K: PartialEq + Eq + Hash + Clone,
{
    pub fn new() -> Self {
        Self {
            atlases: Vec::new(),
            contents: HashMap::new(),
            current_generation: 0,
            last_eviction_generation: 0,
        }
    }
    pub fn add_atlas(
        &mut self,
        texture: wgpu::Texture,
        view: wgpu::TextureView,
        sampler: wgpu::Sampler,
    ) {
        let texture_info = TextureInfo {
            texture,
            view,
            sampler,
        };
        self.atlases
            .push(TextureAtlas::with_texture_info(texture_info));
    }
    pub(crate) fn add_textureless_atlas(&mut self, extent: TextureCoordinate2D) {
        self.atlases.push(TextureAtlas::new(extent));
    }
    pub fn increment_generation(&mut self) {
        self.current_generation = self.current_generation.wrapping_add(1);
    }

    pub fn valid_generation_range(&self) -> Option<Range<i32>> {
        if self.last_eviction_generation < self.current_generation {
            Some(Range {
                start: self.last_eviction_generation,
                end: self.current_generation,
            })
        } else {
            None
        }
    }

    pub fn valid_generations_raw(&self) -> (i32, i32) {
        (self.last_eviction_generation, self.current_generation)
    }

    pub fn evict_outdated<R, D>(&mut self, data: &mut D, redraw: R)
    where
        R: Fn(&mut D, &K, &AtlasLocation, &mut TextureAtlas<K>),
    {
        let mut current_generations = Vec::with_capacity(self.contents.len());
        let mut atlases_dirty = Vec::with_capacity(self.atlases.len());
        atlases_dirty.resize(self.atlases.len(), false);
        for (key, location) in self.contents.iter() {
            current_generations.push((key.clone(), location.clone()));
        }
        // sort generations, evict the oldest
        current_generations.sort_by_key(|e| e.1.generation);
        /* Evict the first 20% of the generations (this heuristics can be improved) */
        let evict_limit = (current_generations.len() * 8) / 100;
        for (key, location) in current_generations.iter().take(evict_limit) {
            self.contents.remove(key);
            atlases_dirty[location.atlas as usize] = true;
        }
        /* Reset the dirty atlases */
        for (idx, dirty) in atlases_dirty.iter().enumerate() {
            if *dirty {
                self.atlases[idx].manager.reset();
            }
        }

        /* Add the non evicted elements for the modified atlases back */
        let num_re_add: usize = self
            .contents
            .iter()
            .map(|(_, loc)| {
                if atlases_dirty[loc.atlas as usize] {
                    1
                } else {
                    0
                }
            })
            .sum();
        let mut contents_to_add = Vec::with_capacity(num_re_add);

        for (key, location) in self.contents.iter() {
            if atlases_dirty[location.atlas as usize] {
                contents_to_add.push((key.clone(), location.clone()));
            }
        }
        contents_to_add.sort_by(|a, b| insert_order(&a.1.extent, &b.1.extent));
        /* Fill in items that are in atlases that are dirty again */
        let mut current_free_index = 0;
        'addloop: for (key, loc) in contents_to_add {
            while current_free_index < self.atlases.len() {
                match self.atlases[current_free_index].insert_single(
                    key.clone(),
                    loc.extent,
                    loc.padding,
                    self.current_generation,
                ) {
                    Ok(loc) => {
                        self.clear_location(&loc);
                        redraw(
                            data,
                            &key,
                            &loc.apply_padding(),
                            &mut self.atlases[current_free_index],
                        );
                        *self.contents.get_mut(&key).unwrap() = loc;
                        continue 'addloop;
                    }
                    Err(e) => match e {
                        InsertResult::AlreadyPresent => {
                            panic!("This should not happen")
                        }
                        InsertResult::NoMoreSpace => {
                            current_free_index += 1;
                            println!("CFI: {}", current_free_index);
                        }
                    },
                }
            }
            unimplemented!("If no atlases have space make a new one and insert the element there?")
        }
        /* Do / how do we ensure that all elements that are cached in the rebuilt
        atlases use the new location.  */
    }

    pub fn look_up(&mut self, key: &K) -> Option<AtlasLocation> {
        if let Some(value) = self.contents.get_mut(key) {
            value.generation = self.current_generation;
            Some(value.apply_padding())
        } else {
            None
        }
    }

    pub fn peek(&self, key: &K) -> Option<AtlasLocation> {
        self.contents.get(key).map(|l| l.apply_padding())
    }
    pub fn peek_unpadded(&self, key: &K) -> Option<&AtlasLocation> {
        self.contents.get(key)
    }
    pub fn insert_single(
        &mut self,
        key: K,
        extent: wgpu::Extent3d,
        padding: u8,
    ) -> Result<AtlasLocation, InsertResult> {
        if self.contents.contains_key(&key) {
            return Err(InsertResult::AlreadyPresent);
        }
        for (idx, atlas) in self.atlases.iter_mut().enumerate() {
            match atlas.insert_single(key.clone(), extent, padding, self.current_generation) {
                Ok(mut location) => {
                    location.atlas = idx as u8;
                    let old = self.contents.insert(key, location.clone());
                    debug_assert!(old.is_none());
                    return Ok(location.apply_padding());
                }
                Err(e) => { /* Do nothing, wait for next interation in for */ }
            }
        }
        /* TODO: If this code is reached it was not space in the existing atlases, so make another one */

        Err(InsertResult::NoMoreSpace)
    }
    pub fn clear_location(&mut self, location: &AtlasLocation) -> bool {
        if let Some(atlas) = self.atlases.get_mut(location.atlas as usize) {
            let mut data_mut = atlas.tile_data_mut(location);
            for row in 0..data_mut.rows() {
                data_mut.row(row as u16).fill(0);
            }
            true
        } else {
            false
        }
    }
    pub fn tile_data_mut<'a>(
        &'a mut self,
        location: &AtlasLocation,
    ) -> Option<TextureAtlasTileView<'a, K>> {
        if let Some(atlas) = self.atlases.get_mut(location.atlas as usize) {
            Some(atlas.tile_data_mut(location))
        } else {
            None
        }
    }
    pub fn atlas(&mut self, location: &AtlasLocation) -> Option<&mut TextureAtlas<K>> {
        self.atlases.get_mut(location.atlas as usize)
    }
    pub fn atlas_ref(&self, location: &AtlasLocation) -> Option<&TextureAtlas<K>> {
        self.atlases.get(location.atlas as usize)
    }
}

fn to_texture_coordinate(extent: &wgpu::Extent3d) -> TextureCoordinate2D {
    debug_assert_eq!(extent.depth_or_array_layers, 0);
    TextureCoordinate2D {
        x: extent.width as u16,
        y: extent.height as u16,
    }
}

fn extent_from_texture_coordinate(extent: &TextureCoordinate2D) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width: extent.x as u32,
        height: extent.y as u32,
        depth_or_array_layers: 0,
    }
}

fn origin_from_texture_coordinate(extent: &TextureCoordinate2D) -> wgpu::Origin2d {
    wgpu::Origin2d {
        x: extent.x as u32,
        y: extent.y as u32,
    }
}

pub fn insert_order(a: &wgpu::Extent3d, b: &wgpu::Extent3d) -> Ordering {
    texture_map::insert_order(&to_texture_coordinate(a), &to_texture_coordinate(b))
}

impl<K> TextureAtlas<K>
where
    K: PartialEq + Eq + Hash,
{
    pub fn new(extent: TextureCoordinate2D) -> Self {
        TextureAtlas {
            manager: PackedTextureCache::new(extent),
            backed_up_texture: BackedUpTexture::with_extent(&extent),
        }
    }
    pub fn with_texture_info(texture: TextureInfo) -> Self {
        TextureAtlas {
            manager: PackedTextureCache::new(TextureCoordinate2D {
                x: texture.texture.width() as u16,
                y: texture.texture.height() as u16,
            }),
            backed_up_texture: BackedUpTexture::with_texture(texture),
        }
    }
    /* Padding is applied internally to avoid any neighbour related artifacts */
    pub fn insert_single(
        &mut self,
        key: K,
        extent: wgpu::Extent3d,
        padding: u8,
        current_generation: i32,
    ) -> Result<AtlasLocation, InsertResult> {
        if let Some(packed_texture) = self.manager.insert(
            key,
            to_texture_coordinate(&extent).padded(
                2 * padding as TextureCoordinateInt,
                2 * padding as TextureCoordinateInt,
            ),
        ) {
            let location = AtlasLocation {
                atlas: 0,
                padding,
                origin: origin_from_texture_coordinate(&packed_texture.origin),
                extent: extent_from_texture_coordinate(&packed_texture.extent),
                generation: current_generation,
            };
            Ok(location)
        } else {
            Err(InsertResult::NoMoreSpace)
        }
    }
    /*
    TODO: Not adapted for padding, consider writing a function that uses
    tile_data_mut internally instead.

        pub fn copy_data(&mut self, location: &AtlasLocation, data: &[u8]) {
            let out_layout = self.backed_up_texture.data_layout();
            let stride_out = out_layout.bytes_per_row.unwrap();
            let bytes_per_pixel = stride_out / self.backed_up_texture.extent().width;
            let offset_out = out_layout.offset as u32
                + (location.origin.y * stride_out)
                + location.origin.x * bytes_per_pixel;
            let stride_in = location.extent.width * bytes_per_pixel;
            let rows = location.extent.height;

            let out_host_data = self.backed_up_texture.host_data_mut();
            for row in 0..rows {
                out_host_data[(offset_out + row * stride_out) as usize
                    ..(offset_out + row * stride_out + stride_in) as usize]
                    .copy_from_slice(
                        &data[(row * stride_in) as usize..((row + 1) * stride_in) as usize],
                    );
            }
        }
    */
    pub fn tile_data_mut<'a>(
        &'a mut self,
        location: &AtlasLocation,
    ) -> TextureAtlasTileView<'a, K> {
        let out_layout = self.backed_up_texture.data_layout();
        let stride_out = out_layout.bytes_per_row.unwrap();
        let bytes_per_pixel = stride_out / self.backed_up_texture.extent().width;
        let offset_out = out_layout.offset as u32
            + (location.origin.y * stride_out)
            + location.origin.x * bytes_per_pixel;
        TextureAtlasTileView {
            atlas: self,
            stride_out,
            stride_in: location.extent.width * bytes_per_pixel,
            bytes_per_pixel,
            offset_out,
            rows: location.extent.height,
            padding: location.padding,
        }
    }
    pub fn texture(&self) -> Option<&Texture> {
        self.backed_up_texture.gpu.as_ref().map(|ti| &ti.texture)
    }
    pub fn sampler(&self) -> Option<&Sampler> {
        self.backed_up_texture.gpu.as_ref().map(|ti| &ti.sampler)
    }
    pub fn update_texture(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.backed_up_texture.update_texture(device, queue);
    }
    pub fn backed_up_texture(&self) -> &BackedUpTexture {
        &self.backed_up_texture
    }
}

pub struct TextureAtlasTileView<'a, K>
where
    K: PartialEq + Eq + Hash,
{
    atlas: &'a mut TextureAtlas<K>,
    stride_out: u32,
    stride_in: u32,
    bytes_per_pixel: u32,
    offset_out: u32,
    rows: u32,
    padding: u8,
}

impl<'a, K> TextureAtlasTileView<'a, K>
where
    K: PartialEq + Eq + Hash,
{
    pub fn row(&mut self, row: u16) -> &mut [u8] {
        let paded_row = row + self.padding as u16;
        let fixed_offset = self.offset_out + self.padding as u32;
        let out_host_data = self.atlas.backed_up_texture.host_data_mut();
        /*println!(
            "O: {} R: {} So: {} Si: {}",
            self.offset_out, row, self.stride_out, self.stride_in
        );*/
        &mut out_host_data[(fixed_offset + (paded_row as u32 * self.stride_out)) as usize
            ..(fixed_offset + (paded_row as u32 * self.stride_out) + self.stride_in) as usize]
    }
    pub fn rows(&self) -> u32 {
        self.rows
    }
    pub fn columns(&self) -> u32 {
        self.stride_out / self.bytes_per_pixel
    }
    pub fn bytes_per_pixel(&self) -> u32 {
        self.bytes_per_pixel
    }
}

impl BackedUpTexture {
    pub fn new() -> Self {
        Self {
            host_data: Vec::new(),
            gpu: None,
            gpu_outdated: true,
            layout: ImageDataLayout::default(),
            label: None,
            extent: wgpu::Extent3d {
                width: 0,
                height: 0,
                depth_or_array_layers: 0,
            },
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
        }
    }
    pub fn with_extent(extent: &TextureCoordinate2D) -> Self {
        let mut host_data = Vec::with_capacity(extent.x as usize * extent.y as usize * RGBA_BPP);
        host_data.resize(extent.x as usize * extent.y as usize * RGBA_BPP, 0);
        Self {
            host_data,
            gpu: None,
            gpu_outdated: true,
            layout: ImageDataLayout::default(),
            label: None,
            extent: extent_from_texture_coordinate(extent),
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
        }
    }
    pub fn with_texture(texture_info: TextureInfo) -> Self {
        let mut host_data = Vec::with_capacity(
            texture_info.texture.width() as usize
                * texture_info.texture.width() as usize
                * RGBA_BPP,
        );
        host_data.resize(
            texture_info.texture.width() as usize
                * texture_info.texture.width() as usize
                * RGBA_BPP,
            0,
        );
        /*        let view = texture_info
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());*/
        let width = texture_info.texture.width();
        let height = texture_info.texture.height();
        Self {
            host_data,
            gpu: Some(texture_info),
            gpu_outdated: true,
            layout: ImageDataLayout::default(),
            label: None,
            extent: Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
        }
    }
    fn ensure_texture_parameters(&mut self, device: &wgpu::Device) {
        /* Check if a texture already exists with the requested parameters */
        if let Some(texture_info) = self.gpu.as_ref() {
            if texture_info.texture.size() == self.extent
                && texture_info.texture.format() == self.format
            {
                return;
            }
        }
        /* If the texture doesn't exist, or has different parameters drop the old and make a new one */
        self.gpu = None;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: self.label.as_ref().map(|s| s.as_str()),
            size: self.extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let texture_info = TextureInfo {
            texture,
            view,
            sampler,
        };
        self.gpu = Some(texture_info);
    }

    pub fn data_layout(&self) -> ImageDataLayout {
        let dl = match self.format {
            wgpu::TextureFormat::R8Unorm
            | wgpu::TextureFormat::R8Snorm
            | wgpu::TextureFormat::R8Uint
            | wgpu::TextureFormat::R8Sint
            | wgpu::TextureFormat::Stencil8 => wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(self.extent.width),
                rows_per_image: Some(self.extent.height),
            },
            wgpu::TextureFormat::Rg8Unorm
            | wgpu::TextureFormat::Rg8Snorm
            | wgpu::TextureFormat::Rg8Uint
            | wgpu::TextureFormat::Rg8Sint => wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(2 * self.extent.width),
                rows_per_image: Some(self.extent.height),
            },
            wgpu::TextureFormat::Rgba32Uint
            | wgpu::TextureFormat::Rgba32Sint
            | wgpu::TextureFormat::Rgba32Float
            | wgpu::TextureFormat::Rgba8Unorm
            | wgpu::TextureFormat::Rgba8UnormSrgb
            | wgpu::TextureFormat::Rgba8Snorm
            | wgpu::TextureFormat::Rgba8Uint
            | wgpu::TextureFormat::Rgba8Sint
            | wgpu::TextureFormat::Bgra8Unorm
            | wgpu::TextureFormat::Bgra8UnormSrgb
            | wgpu::TextureFormat::Depth16Unorm => wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * self.extent.width),
                rows_per_image: Some(self.extent.height),
            },
            _ => {
                panic!("Trying to create backed up texture with unsupported format")
            }
        };
        dl
    }
    pub fn extent(&self) -> &wgpu::Extent3d {
        &self.extent
    }
    pub fn update_texture(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        /*        if !self.gpu_outdated {
            debug_assert!(self.gpu.is_some());
            return;
        }*/
        self.ensure_texture_parameters(device);
        //println!("Writing texture");
        queue.write_texture(
            wgpu::ImageCopyTexture {
                aspect: wgpu::TextureAspect::All,
                texture: &self.gpu.as_ref().unwrap().texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
            },
            &self.host_data,
            self.data_layout(),
            self.extent,
        );
        self.gpu_outdated = false;
    }
    pub fn texture(&self) -> &Option<TextureInfo> {
        if self.gpu_outdated {
            debug_assert!(self.gpu.is_some());
            &None
        } else {
            &self.gpu
        }
    }
    pub fn host_data_mut(&mut self) -> &mut [u8] {
        self.gpu_outdated = true;
        self.host_data.as_mut_slice()
    }
    pub fn host_data(&self) -> &[u8] {
        self.host_data.as_slice()
    }
}
