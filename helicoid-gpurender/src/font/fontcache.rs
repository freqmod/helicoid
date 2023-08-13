use core::panic;
use std::{borrow::BorrowMut, cell::RefCell};

use bytemuck::offset_of;
use cosmic_text::{CacheKey, SwashCache};
use hashbrown::HashMap;
use helicoid_protocol::text::SHAPABLE_STRING_ALLOC_LEN;
use lyon::geom::euclid::num::Ceil;
use num_enum::{FromPrimitive, IntoPrimitive};
use smallvec::SmallVec;
use swash::{
    scale::{Render, ScaleContext, Source, StrikeWith},
    zeno::{Format, Vector},
    FontRef,
};
use wgpu::{
    BindGroup, BindGroupLayout, BlendComponent, CompositeAlphaMode, Device, Extent3d,
    MultisampleState, Origin2d, PipelineLayout, RenderPass, RenderPipeline, SamplerDescriptor,
    ShaderModule, TextureFormat, TextureViewDescriptor, TextureViewDimension,
};

use crate::font::texture_atlases::{self, AtlasLocation, TextureAtlas, TextureAtlases};
pub trait FontOwner {
    fn swash_font(&self) -> FontRef<'_>;
}
const POINTS_PER_SQUARE: usize = 6;
pub type FontId = u8;

thread_local! {
    static RENDER_LIST_HOST: RefCell<Vec<RenderSquare>> = RefCell::new(Vec::new());
}

pub struct FontPalette {
    host: Vec<u32>,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
}

pub type RenderTargetId = u32;

pub struct FontCacheRenderer {
    pipeline: Option<RenderPipeline>,
    // the bind_group is not created before the texture etc. that it binds to are created
    bind_group: Option<BindGroup>,
    globals_ubo: Option<wgpu::Buffer>,
    globals: FontCacheGlobals,
    multisample: MultisampleState,
}

pub struct FontCache<O>
where
    O: FontOwner,
{
    context: ScaleContext,
    font: O,
    cache: TextureAtlases<SwashCacheKey>, // Consider using the etagere crate instead
    color: bool,                          // use RGB subpixel rendering
    wgpu_resources: Option<WGpuResources>,
    renderers: HashMap<RenderTargetId, FontCacheRenderer>,
}

/* Todo: Split bind group, and globals into a separate struct so different
setups can be used for different surfaces and windows */
pub struct WGpuResources {
    text_vs_shader: ShaderModule,
    text_fs_shader: ShaderModule,
    text_pipeline_layout: PipelineLayout,
    bind_group_layout: BindGroupLayout,
    palette: FontPalette,
}

impl FontCacheRenderer {
    pub fn pipeline(&self) -> Option<&RenderPipeline> {
        self.pipeline.as_ref()
    }
    pub fn bind_group(&self) -> Option<&BindGroup> {
        self.bind_group.as_ref()
    }
}
#[repr(C)]
#[derive(Copy, Clone, Debug, Default)]
struct FontCacheGlobals {
    resolution: [f32; 2],
    offset: [f32; 2],
}

unsafe impl bytemuck::Pod for FontCacheGlobals {}
unsafe impl bytemuck::Zeroable for FontCacheGlobals {}

#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, IntoPrimitive, FromPrimitive, Default,
)]
#[repr(u8)]
pub enum SubpixelBin {
    #[default]
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

struct RedrawData<'a, 'b> {
    font: &'a mut FontRef<'b>,
    bpp: u8,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Default)]
pub struct PackedSubpixels(u8);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SwashCacheKey {
    pub glyph_id: u16,
    pub font_size_bits: u32,
    pub bins: PackedSubpixels,
}

impl From<CacheKey> for SwashCacheKey {
    fn from(key: CacheKey) -> Self {
        SwashCacheKey {
            glyph_id: key.glyph_id,
            font_size_bits: key.font_size_bits,
            bins: PackedSubpixels::new(
                SubpixelBin::from(key.x_bin),
                SubpixelBin::from(key.y_bin),
                0,
            ),
        }
    }
}

impl SwashCacheKey {
    pub fn x_bin(&self) -> SubpixelBin {
        self.bins.x_bin()
    }
    pub fn y_bin(&self) -> SubpixelBin {
        self.bins.y_bin()
    }
}
impl PackedSubpixels {
    pub fn new(x_bin: SubpixelBin, y_bin: SubpixelBin, font_id: FontId) -> Self {
        debug_assert!(font_id < 16);
        Self((font_id << 4) & ((x_bin as u8) << 2) & (y_bin as u8))
    }
    pub fn x_bin(&self) -> SubpixelBin {
        SubpixelBin::from((self.0 >> 2) & 3)
    }
    pub fn y_bin(&self) -> SubpixelBin {
        SubpixelBin::from(self.0 & 3)
    }
    pub fn font_id(&self) -> FontId {
        (self.0 >> 4) & 0xF
    }
    pub fn cleared_offset(&self) -> Self {
        Self(self.0 & 0xF) // clear subpixel offset, but keep fontid
    }
}

impl FontPalette {
    /* TODO: Add support for resizing (extending palette)*/
    pub fn new(device: &wgpu::Device) -> Self {
        let initial_size = 128 as usize;
        let mut host = Vec::<u32>::with_capacity(initial_size);
        host.resize(initial_size, 0xFFFFFFFF);
        let texture_descriptor = &wgpu::TextureDescriptor {
            label: Some("Frame descriptor"),
            size: wgpu::Extent3d {
                width: initial_size as u32,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        };

        let texture = device.create_texture(texture_descriptor);
        let view = texture.create_view(&TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        Self {
            host,
            texture,
            view,
            sampler,
        }
    }
    pub fn set_entry(&mut self, idx: usize, value: u32) {
        self.host[idx] = value;
    }
    pub fn copy_to_gpu(&mut self, queue: &wgpu::Queue) {
        queue.write_texture(
            wgpu::ImageCopyTexture {
                aspect: wgpu::TextureAspect::All,
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
            },
            bytemuck::cast_slice(self.host.as_slice()),
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * self.texture.width()),
                rows_per_image: Some(1),
            },
            Extent3d {
                width: self.texture.width(),
                height: 1,
                depth_or_array_layers: 1,
            },
        );
    }
}

impl FontCacheRenderer {
    fn new(globals: FontCacheGlobals, multisample: wgpu::MultisampleState) -> Self {
        FontCacheRenderer {
            pipeline: None,
            bind_group: None,
            globals_ubo: None,
            globals,
            multisample,
        }
    }
    pub fn resolution_changed(&mut self, queue: &wgpu::Queue, resolution: (u32, u32)) {
        self.globals.resolution = [resolution.0 as f32, resolution.1 as f32];
        queue.write_buffer(
            &self.globals_ubo.as_ref().unwrap(),
            0,
            bytemuck::cast_slice(&[self.globals]),
        );
    }
    /* This should ideally only run on init for the surface */
    fn setup_resources(
        &mut self,
        device: &wgpu::Device,
        color: bool,
        wgpu_resources: &mut WGpuResources,
        atlas: &TextureAtlas<SwashCacheKey>,
    ) {
        self.globals_ubo = Some(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Globals ubo"),
            size: std::mem::size_of::<FontCacheGlobals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
        let resources = wgpu_resources;
        self.update_pipeline(&resources, color, atlas, device);
        self.update_bind_group(resources, atlas, device);
    }
    pub fn setup_pipeline<'a: 'p, 'p>(
        &'a mut self,
        //        device: &wgpu::Device,
        pass: &mut RenderPass<'p>,
    ) {
        pass.set_pipeline(self.pipeline.as_ref().unwrap());
        pass.set_bind_group(0, &self.bind_group.as_ref().unwrap(), &[])
    }
    fn update_bind_group(
        &mut self,
        owner_resources: &WGpuResources,
        atlas: &TextureAtlas<SwashCacheKey>,
        device: &wgpu::Device,
    ) {
        if self.bind_group.is_none() {
            self.bind_group = Some(
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Text Bind group"),
                    layout: &owner_resources.bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Buffer(
                                self.globals_ubo
                                    .as_ref()
                                    .unwrap()
                                    .as_entire_buffer_binding(),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(atlas.sampler().unwrap()),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(
                                &atlas
                                    .texture()
                                    .unwrap()
                                    .create_view(&TextureViewDescriptor::default()),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::Sampler(
                                &owner_resources.palette.sampler,
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: wgpu::BindingResource::TextureView(
                                &owner_resources.palette.view,
                            ),
                        },
                    ],
                }),
            );
        }
    }
    fn update_pipeline(
        &mut self,
        owner_resources: &WGpuResources,
        color: bool,
        atlas: &TextureAtlas<SwashCacheKey>,
        device: &wgpu::Device,
    ) {
        if self.pipeline.is_none() {
            self.pipeline = Some(
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: None,
                    layout: Some(&owner_resources.text_pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &owner_resources.text_vs_shader,
                        entry_point: "main",
                        buffers: &[wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<RenderPoint>() as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &[
                                wgpu::VertexAttribute {
                                    offset: offset_of!(RenderPoint, dx) as u64,
                                    format: wgpu::VertexFormat::Float32x2,
                                    shader_location: 0,
                                },
                                wgpu::VertexAttribute {
                                    offset: offset_of!(RenderPoint, sx) as u64,
                                    format: wgpu::VertexFormat::Float32x2,
                                    shader_location: 1,
                                },
                                wgpu::VertexAttribute {
                                    offset: offset_of!(RenderPoint, color_idx) as u64,
                                    format: wgpu::VertexFormat::Float32,
                                    shader_location: 2,
                                },
                            ],
                        }],
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &owner_resources.text_fs_shader,
                        entry_point: "main",
                        targets: &[Some(wgpu::ColorTargetState {
                            format: wgpu::TextureFormat::Bgra8UnormSrgb,
                            blend: Some(wgpu::BlendState {
                                color: wgpu::BlendComponent {
                                    src_factor: if color {
                                        wgpu::BlendFactor::Src1
                                    } else {
                                        wgpu::BlendFactor::SrcAlpha
                                    },
                                    dst_factor: if color {
                                        wgpu::BlendFactor::OneMinusSrc1
                                    } else {
                                        wgpu::BlendFactor::OneMinusSrcAlpha
                                    },
                                    operation: wgpu::BlendOperation::Add,
                                },
                                alpha: BlendComponent::REPLACE,
                            }),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        polygon_mode: wgpu::PolygonMode::Fill,
                        front_face: wgpu::FrontFace::Ccw,
                        strip_index_format: None,
                        cull_mode: None,
                        unclipped_depth: false,
                        conservative: false,
                    },
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: wgpu::TextureFormat::Depth32Float,
                        depth_write_enabled: true,
                        depth_compare: wgpu::CompareFunction::Greater,
                        stencil: wgpu::StencilState {
                            front: wgpu::StencilFaceState::IGNORE,
                            back: wgpu::StencilFaceState::IGNORE,
                            read_mask: 0,
                            write_mask: 0,
                        },
                        bias: wgpu::DepthBiasState::default(),
                    }),
                    multisample: self.multisample,
                    multiview: None,
                }),
            );
        }
    }
}
impl<O> FontCache<O>
where
    O: FontOwner,
{
    pub fn new(font: O, color: bool, dev: Option<&Device>) -> Self {
        let resources = dev.map(|dev| Self::create_resources(dev, color));
        Self {
            context: ScaleContext::new(),
            font,
            cache: TextureAtlases::default(),
            color,
            wgpu_resources: resources,
            renderers: Default::default(),
        }
    }
    pub fn bytes_per_pixel(&self) -> usize {
        if self.color {
            4
        } else {
            1
        }
    }
    pub fn color(&self) -> bool {
        self.color
    }
    pub fn texture_format(&self) -> wgpu::TextureFormat {
        if self.color {
            TextureFormat::Bgra8UnormSrgb
        } else {
            TextureFormat::R8Unorm
        }
    }
    /*pub fn add_atlas(
        &mut self,
        texture: wgpu::Texture,
        view: wgpu::TextureView,
        sampler: wgpu::Sampler,
    ) */
    pub fn add_atlas(&mut self, dev: &wgpu::Device, extent: Extent3d) {
        let texture = Self::create_texture(dev, extent.width, extent.height, self.texture_format());
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("Simple font cache view"),
            format: Some(self.texture_format()),
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
        context: &mut ScaleContext,
        font: &FontRef<'_>,
        cache_key: &SwashCacheKey,
        bpp: usize,
    ) -> swash::zeno::Placement {
        /* TODO: Is it possible to get the exent of a rendered glyph without actually rendering it? */
        /* Use swash / cosmic text to runder to the texture */
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

        // Select our source order
        let image = if bpp == 4 {
            // Compute the fractional offset-- you'll likely want to quantize this
            // in a real renderer
            let offset = Vector::new(cache_key.x_bin().as_float(), cache_key.y_bin().as_float());
            Render::new(&[
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
            .unwrap()
        } else {
            Render::new(&[
                Source::Outline,
                // Bitmap with best fit selection mode
                Source::Bitmap(StrikeWith::ExactSize),
                // Standard scalable outline
                Source::Outline,
            ])
            .render(&mut scaler, cache_key.glyph_id)
            .unwrap()
        };

        image.placement
    }
    pub fn update_cache<'a, I>(&mut self, elements: I)
    where
        I: ExactSizeIterator<Item = &'a RenderSpecElement>,
    {
        let mut key_meta = Vec::with_capacity(elements.len());
        let font_ref = self.font.swash_font();
        let context = &mut self.context;
        key_meta.extend(elements.map(|elm| {
            //let scaled_metrics = metrics.scale(f32::from_bits(key.font_size_bits));
            /*println!(
                "Metrics: I:{} W:{} H:{}",
                key.glyph_id,
                scaled_metrics.advance_width(key.glyph_id),
                scaled_metrics.advance_height(key.glyph_id)
            );*/

            (
                elm.key(),
                Extent3d {
                    width: elm.extent_o2d().x, //scaled_metrics.advance_width(key.glyph_id).ceil() as u32,
                    height: elm.extent_o2d().y, //scaled_metrics.advance_height(key.glyph_id).ceil() as u32,
                    depth_or_array_layers: 0,
                },
            )
        }));
        //println!("Cache keys: {:?}", key_meta);
        self.cache.increment_generation();
        key_meta.sort_by(|a, b| texture_atlases::insert_order(&a.1, &b.1));
        for (key, extent) in key_meta.iter() {
            match self.cache.insert_single(
                key.clone(),
                extent.clone(),
                self.bytes_per_pixel() as u8,
            ) {
                Ok(location) => {
                    //println!("Inserted: {:?}", &key);
                    /* Render font data info the cache */
                    self.render_to_location(&key, &location);
                }
                Err(e) => {
                    log::trace!("Insert-err: {:?} {:?}", &key, e);
                    match e {
                        texture_atlases::InsertResult::NoMoreSpace => {
                            let mut evict_data = RedrawData {
                                font: &mut self.font.swash_font(),
                                bpp: self.bytes_per_pixel() as u8,
                            };
                            self.cache.evict_outdated(&mut evict_data, &Self::redraw);
                            match self.cache.insert_single(
                                key.clone(),
                                extent.clone(),
                                self.bytes_per_pixel() as u8,
                            ) {
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

    /* TODO: Improve palette management of both colors and copying only when changed */
    pub fn update_palette(&mut self, queue: &wgpu::Queue) {
        if let Some(wgpu_resources) = self.wgpu_resources.as_mut() {
            wgpu_resources.palette.copy_to_gpu(queue);
        } else {
            panic!()
        }
    }
    pub fn set_palette_entry(&mut self, idx: usize, value: u32) {
        if let Some(wgpu_resources) = self.wgpu_resources.as_mut() {
            wgpu_resources.palette.set_entry(idx, value);
        }
    }
    fn redraw(
        redraw_data: &mut RedrawData<'_, '_>,
        key: &SwashCacheKey,
        loc: &AtlasLocation,
        atlas: &mut TextureAtlas<SwashCacheKey>,
    ) {
        Self::do_render_to_location(
            &mut redraw_data.font,
            key,
            loc,
            atlas,
            redraw_data.bpp as usize,
        );
    }

    fn render_to_location(&mut self, key: &SwashCacheKey, location: &AtlasLocation) {
        let bpp = self.bytes_per_pixel();
        Self::do_render_to_location(
            &self.font.swash_font(),
            key,
            location,
            self.cache.atlas(location).unwrap(),
            bpp,
        );
    }

    fn do_render_to_location(
        font: &FontRef<'_>,
        cache_key: &SwashCacheKey,
        location: &AtlasLocation,
        atlas: &mut TextureAtlas<SwashCacheKey>,
        bpp: usize,
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
        let offset = Vector::new(cache_key.x_bin().as_float(), cache_key.y_bin().as_float());

        // Select our source order
        let image = if bpp == 4 {
            Render::new(&[
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
            .unwrap()
        } else {
            Render::new(&[
                Source::Outline,
                // Bitmap with best fit selection mode
                Source::Bitmap(StrikeWith::ExactSize),
                // Standard scalable outline
                Source::Outline,
            ])
            .render(&mut scaler, cache_key.glyph_id)
            .unwrap()
        };

        let width = (image.placement.width as i32) as u32;
        let height = (image.placement.height as i32) as u32;
        // TODO: Do we need to take placement offset into account?

        log::trace!(
            "Render to loc placement: {}, {:?} W:{} H:{}",
            cache_key.glyph_id,
            image.placement,
            width,
            height
        );
        let mut data_view = atlas.tile_data_mut(location);
        for y in 0..height {
            let row = data_view.row(y as u16);
            let copy_width = (bpp * width as usize) as u32;
            /*if copy_width != (bpp as u32) * width || copy_width != (row.len()) as u32 {
                println!(
                    "Render: {}=={} {}",
                    row.len(),
                    bpp as u32 * width,
                    copy_width
                );
            }*/
            row[0..copy_width as usize].copy_from_slice(
                &image.data[(y * bpp as u32 * width) as usize
                    ..((y * bpp as u32 * width) + copy_width) as usize],
            );
            //println!("Rendered: {:?}", &row[0..copy_width as usize]);
        }
    }
    pub fn offset_glyphs(&mut self, rs: &mut RenderSpec) {
        let font_ref = &mut self.font.swash_font();
        for elm in rs.elements.iter_mut() {
            if !self.color() {
                elm.key_bins = elm.key_bins.cleared_offset();
            }
            let bpp = self.bytes_per_pixel();
            let placement =
                Self::placement_for_glyph(&mut self.context, &font_ref, &elm.key(), bpp);
            // Are placement scaled differently trough the graphics pipeline than the pixels in the texture?

            elm.offset.x = (elm.offset.x as i32 + placement.left) as u32;
            elm.offset.y = (elm.offset.y as i32 - placement.top) as u32;
            elm.extent.0 = placement.width as u8;
            elm.extent.1 = placement.height as u8;
            log::trace!(
                "Applied offset: {:?}: Placement: {:?} Offs: {:?}",
                elm,
                placement,
                elm.offset
            );
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
                    self.update_cache(rs.elements.iter());
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
    fn create_resources(device: &wgpu::Device, color: bool) -> WGpuResources {
        let text_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("FCText Bind group layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<
                                FontCacheGlobals,
                            >()
                                as u64),
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            });
        let text_vs_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Text vs"),
            source: wgpu::ShaderSource::Wgsl(include_str!("./../../shaders/text.vs.wgsl").into()),
        });
        let text_fs_module = (if color {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Text fs"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("./../../shaders/text.fs.wgsl").into(),
                ),
            })
        } else {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Text fs"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("./../../shaders/text_mono.fs.wgsl").into(),
                ),
            })
        });

        let text_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            bind_group_layouts: &[&text_bind_group_layout],
            push_constant_ranges: &[],
            label: None,
        });
        let palette = FontPalette::new(device);
        WGpuResources {
            text_vs_shader: text_vs_module,
            text_fs_shader: text_fs_module,
            text_pipeline_layout,
            bind_group_layout: text_bind_group_layout,
            palette,
        }
    }

    pub fn wgpu_resources(&self) -> Option<&WGpuResources> {
        self.wgpu_resources.as_ref()
    }
    pub fn renderer(&mut self, target_id: &RenderTargetId) -> Option<&mut FontCacheRenderer> {
        self.renderers.get_mut(target_id)
    }
    /* This should ideally only run on init for the surface */
    pub fn renderer_setup_resources(&mut self, target_id: &RenderTargetId, device: &wgpu::Device) {
        if let Some(renderer) = self.renderers.get_mut(target_id) {
            let atlas = self.cache.atlas_ref(&AtlasLocation::atlas_only(0)).unwrap();
            renderer.setup_resources(
                device,
                self.color,
                self.wgpu_resources.as_mut().unwrap(),
                atlas,
            );
        }
    }
    /*    pub fn renderer_setup_pipeline<'a: 'p, 'p>(
        &'a mut self,
        target_id: &RenderTargetId,
        device: &wgpu::Device,
        pass: &mut RenderPass<'p>,
    ) {
        if let Some(renderer) = self.renderers.get_mut(target_id) {
            renderer.setup_pipeline::<O>(device, pass);
        }
    }*/
    pub fn create_renderer(
        &mut self,
        target_id: RenderTargetId,
        resolution: (u8, u8),
        multisample: wgpu::MultisampleState,
    ) -> Option<&mut FontCacheRenderer> {
        if self.renderers.contains_key(&target_id) {
            return None;
        }
        let globals = FontCacheGlobals {
            resolution: [resolution.0 as f32, resolution.1 as f32],
            offset: [0f32, 0f32],
        };
        assert!(self
            .renderers
            .insert(target_id, FontCacheRenderer::new(globals, multisample))
            .is_none());
        return self.renderer(&target_id);
    }
}

#[derive(Debug)]
pub enum RenderRunError {
    CharacterMissingInAtlas,
}
#[repr(C)]
#[derive(Clone, Copy, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RenderPoint {
    pub dx: u32,
    pub dy: u32,
    pub sx: u32,
    pub sy: u32,
    pub color_idx: u32,
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
    pub host_indices: Vec<u16>,
}

#[derive(Debug, Default)]
pub struct RenderSpec {
    elements: SmallVec<[RenderSpecElement; SHAPABLE_STRING_ALLOC_LEN]>,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Point2d {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Fixed88(u16); // 8.8 bits fixed point

impl From<f32> for Fixed88 {
    fn from(value: f32) -> Self {
        Self(((value.min(255.0).max(0.0) as u16) << 8) + (value.fract() * 255.0) as u16)
    }
}

impl From<u32> for Fixed88 {
    fn from(value: u32) -> Self {
        From::<f32>::from(f32::from_bits(value))
    }
}
impl Into<f32> for Fixed88 {
    fn into(self) -> f32 {
        (self.0 >> 8) as f32 + ((self.0 & 0xFF) as f32 * 255.0)
    }
}
impl Into<u32> for Fixed88 {
    fn into(self) -> u32 {
        Into::<f32>::into(self).to_bits()
    }
}

/* This struct is optimized for size to make the RenderSpec smallvec as small as possible */
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RenderSpecElement {
    pub color_idx: u8,
    pub key_glyph_id: u16,
    pub key_font_size: Fixed88,
    pub key_bins: PackedSubpixels,
    pub offset: Origin2d, // TODO: Use signed?
    pub extent: (u8, u8),
}

impl RenderSpecElement {
    pub fn key(&self) -> SwashCacheKey {
        SwashCacheKey {
            glyph_id: self.key_glyph_id,
            font_size_bits: Into::into(self.key_font_size),
            bins: self.key_bins,
        }
    }
    pub fn extent_o2d(&self) -> Origin2d {
        Origin2d {
            x: self.extent.0 as u32,
            y: self.extent.1 as u32,
        }
    }
}
impl RenderSpec {
    pub fn unique_keys(&self) -> Vec<SwashCacheKey> {
        let mut keys = Vec::with_capacity(self.elements.len());
        keys.extend(self.elements.iter().map(|rse| rse.key()));
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
        /* NB: The y coordinates in this square match the wgpu coordinate system,
        which means they are bottom to top */
        let tw = texture_width as f32;
        let th = texture_height as f32;
        let palette_len = 128;
        let color_idx =
            (((element.color_idx % palette_len) as f32 / palette_len as f32) as f32).to_bits();
        Self {
            top_left1: RenderPoint {
                dx: (element.offset.x as f32).to_bits(),
                dy: (element.offset.y as f32).to_bits(),
                sx: (atlas.origin.x as f32 / tw).to_bits(),
                sy: (atlas.origin.y as f32 / th).to_bits(),
                color_idx,
            },
            bottom_left1: RenderPoint {
                dx: (element.offset.x as f32).to_bits(),
                dy: ((element.offset.y + atlas.extent.height) as f32).to_bits(),
                sx: (atlas.origin.x as f32 / tw).to_bits(),
                sy: ((atlas.origin.y + atlas.extent.height) as f32 / tw).to_bits(),
                color_idx,
            },
            top_right1: RenderPoint {
                dx: ((element.offset.x + atlas.extent.width) as f32).to_bits(),
                dy: ((element.offset.y) as f32).to_bits(),
                sx: ((atlas.origin.x + atlas.extent.width) as f32 / tw).to_bits(),
                sy: ((atlas.origin.y) as f32 / th).to_bits(),
                color_idx,
            },
            top_right2: RenderPoint {
                dx: ((element.offset.x + atlas.extent.width) as f32).to_bits(),
                dy: ((element.offset.y) as f32).to_bits(),
                sx: ((atlas.origin.x + atlas.extent.width) as f32 / tw).to_bits(),
                sy: ((atlas.origin.y) as f32 / th).to_bits(),
                color_idx,
            },
            bottom_right2: RenderPoint {
                dx: ((element.offset.x + atlas.extent.width) as f32).to_bits(),
                dy: ((element.offset.y + atlas.extent.height) as f32).to_bits(),
                sx: ((atlas.origin.x + atlas.extent.width) as f32 / tw).to_bits(),
                sy: ((atlas.origin.y + atlas.extent.height) as f32 / th).to_bits(),
                color_idx,
            },
            bottom_left2: RenderPoint {
                dx: ((element.offset.x) as f32).to_bits(),
                dy: ((element.offset.y + atlas.extent.height) as f32).to_bits(),
                sx: ((atlas.origin.x) as f32 / tw).to_bits(),
                sy: ((atlas.origin.y + atlas.extent.height) as f32 / th).to_bits(),
                color_idx,
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
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::VERTEX,
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
            match atlas.look_up(&element.key()) {
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
                    //                    println!("SE ({:?}): {:?}", location, spec_element);
                    self.host_vertices.push(spec_element);
                    for x in 0..POINTS_PER_SQUARE {
                        self.host_indices
                            .push(((idx * POINTS_PER_SQUARE) + x) as u16);
                    }
                }
                None => {
                    /* Caller: Populate atlas with all elements and retry */
                    log::warn!("Missing in atlas: {:?}", element.key());
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

    use wgpu::MultisampleState;

    use crate::font::{swash_font::SwashFont, texture_map::TextureCoordinate2D};

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
        log::info!(
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
        let mut font_cache = FontCache::new(font, true, None);
        let mut spec = RenderSpec::default();
        font_cache.cache.add_textureless_atlas(
            TextureCoordinate2D { x: 1024, y: 1024 },
            TextureFormat::Bgra8UnormSrgb,
        );

        let char_width = font_cache
            .owner()
            .swash_font()
            .metrics(&[])
            .scale(font_scale_f)
            .average_width;

        for x in 0..200 {
            spec.elements.push(RenderSpecElement {
                //                char: ' ',
                key_glyph_id: 120 + x as u16,
                key_font_size: Fixed88::from(font_scale_f),
                key_bins: PackedSubpixels::default(),
                offset: Origin2d {
                    x: char_width as u32 * x,
                    y: 0,
                },
                extent: (0, 0),
                color_idx: 0,
            })
        }
        // TODO: Fill render spec with some default (statically shaped) data
        //let run = font_cache.render_run(&spec).unwrap();
        //assert_eq!(run.gpu_vertices.len(), spec.elements.len());
        //println!("Run: {:?}", run);
    }
}
