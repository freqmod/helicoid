use crate::fontcache::{PackedTextureCache, TextureCoordinate2D};
use std::{collections::HashMap, hash::Hash};
use wgpu::{Extent3d, ImageDataLayout, Origin2d, Texture};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AtlasLocation {
    pub atlas: usize,
    pub origin: Origin2d,
    pub extent: Extent3d,
}

pub struct TextureInfo {
    pub texture: Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
}

//https://sotrh.github.io/learn-wgpu/beginner/tutorial5-textures/#loading-an-image-from-a-file
pub struct BackedUpTexture {
    host_data: Vec<u8>,
    gpu: Option<TextureInfo>,
    gpu_outdated: bool,
    layout: ImageDataLayout,
    extent: wgpu::Extent3d,
    format: wgpu::TextureFormat,
    label: Option<String>,
}

struct TextureAtlas<K>
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
}

pub enum InsertResult {
    AlreadyPresent,
    NoMoreSpace,
}
impl<K> TextureAtlases<K>
where
    K: PartialEq + Eq + Hash + Clone,
{
    pub fn new() -> Self {
        Self {
            atlases: Vec::new(),
            contents: HashMap::new(),
        }
    }
    pub fn look_up(&self, key: &K) -> Option<AtlasLocation> {
        None
    }
    pub fn insert_single(
        &mut self,
        key: K,
        data: &[u8],
        extent: wgpu::Extent3d,
    ) -> Result<AtlasLocation, InsertResult> {
        if self.contents.contains_key(&key) {
            return Err(InsertResult::AlreadyPresent);
        }
        for (idx, atlas) in self.atlases.iter_mut().enumerate() {
            match atlas.insert_single(key.clone(), data, extent) {
                Ok(mut location) => {
                    location.atlas = idx;
                    return Ok(location);
                }
                Err(e) => { /* Do nothing, wait for next interation in for */ }
            }
        }
        /* TODO: If this code is reached it was not space in the existing atlases, so make another one */

        Err(InsertResult::NoMoreSpace)
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

impl<K> TextureAtlas<K>
where
    K: PartialEq + Eq + Hash,
{
    pub fn insert_single(
        &mut self,
        key: K,
        data: &[u8],
        extent: wgpu::Extent3d,
    ) -> Result<AtlasLocation, InsertResult> {
        if let Some(packed_texture) = self.manager.insert(key, to_texture_coordinate(&extent)) {
            let location = AtlasLocation {
                atlas: 0,
                origin: origin_from_texture_coordinate(&packed_texture.origin),
                extent: extent_from_texture_coordinate(&packed_texture.extent),
            };
            self.copy_data(&location.origin, &location.extent, data);
            Ok(location)
        } else {
            Err(InsertResult::NoMoreSpace)
        }
    }

    fn copy_data(&mut self, origin: &wgpu::Origin2d, extent_in: &wgpu::Extent3d, data: &[u8]) {
        let out_layout = self.backed_up_texture.data_layout();
        let stride_out = out_layout.bytes_per_row.unwrap();
        let bytes_per_pixel = stride_out / self.backed_up_texture.extent().width;
        let offset_out =
            out_layout.offset as u32 + (origin.y * stride_out) + origin.x * bytes_per_pixel;
        let stride_in = extent_in.width * bytes_per_pixel;
        let rows = extent_in.height;

        let out_host_data = self.backed_up_texture.host_data_mut();
        for row in 0..rows {
            out_host_data[(offset_out + row * stride_out) as usize
                ..(offset_out + row * stride_out + stride_in) as usize]
                .copy_from_slice(
                    &data[(row * stride_in) as usize..((row + 1) * stride_in) as usize],
                );
        }
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
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
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
            mag_filter: wgpu::FilterMode::Linear,
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
        match self.format {
            wgpu::TextureFormat::R8Unorm
            | wgpu::TextureFormat::R8Snorm
            | wgpu::TextureFormat::R8Uint
            | wgpu::TextureFormat::R8Sint
            | wgpu::TextureFormat::Rg8Unorm
            | wgpu::TextureFormat::Rg8Snorm
            | wgpu::TextureFormat::Rg8Uint
            | wgpu::TextureFormat::Rg8Sint
            | wgpu::TextureFormat::Rgba8Unorm
            | wgpu::TextureFormat::Rgba8UnormSrgb
            | wgpu::TextureFormat::Rgba8Snorm
            | wgpu::TextureFormat::Rgba8Uint
            | wgpu::TextureFormat::Rgba8Sint
            | wgpu::TextureFormat::Bgra8Unorm
            | wgpu::TextureFormat::Bgra8UnormSrgb
            | wgpu::TextureFormat::Stencil8 => wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(self.extent.width),
                rows_per_image: Some(self.extent.height),
            },
            wgpu::TextureFormat::Rgba32Uint
            | wgpu::TextureFormat::Rgba32Sint
            | wgpu::TextureFormat::Rgba32Float
            | wgpu::TextureFormat::Depth16Unorm => wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * self.extent.width),
                rows_per_image: Some(self.extent.height),
            },
            _ => {
                panic!("Trying to create backed up texture with unsupported format")
            }
        }
    }
    pub fn extent(&self) -> &wgpu::Extent3d {
        &self.extent
    }
    pub fn update_texture(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if !self.gpu_outdated {
            debug_assert!(self.gpu.is_some());
            return;
        }
        self.ensure_texture_parameters(device);
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
