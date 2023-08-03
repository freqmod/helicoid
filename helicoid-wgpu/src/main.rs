use bytemuck::offset_of;
use cosmic_text::fontdb::{Database, FaceInfo, Language, ID};
use cosmic_text::{Attrs, Buffer as TextBuffer, Font, FontSystem, Metrics, Shaping, Weight};
/*
(c) Frederik Vestre - Licensed under MPL (like the rest of helicoid).

Based on lyon wgpu example, by nical. Licensed Apachev2, MIT or MPL.
The example code code will hopefully be rewritten until most of it is gone.

*/
use lyon::extra::rust_logo::build_logo_path;
use lyon::math::*;
use lyon::path::{Path, Polygon, NO_ATTRIBUTES};
use lyon::tessellation;
use lyon::tessellation::geometry_builder::*;
use lyon::tessellation::{FillOptions, FillTessellator};
use lyon::tessellation::{StrokeOptions, StrokeTessellator};

use lyon::algorithms::{rounded_polygon, walk};

use wgpu::{CompositeAlphaMode, Extent3d, Origin2d, TextureDescriptor, TextureViewDescriptor};
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, Event, KeyEvent, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::KeyCode;
use winit::window::{Window, WindowBuilder};

// For create_buffer_init()
use wgpu::util::DeviceExt;

use core::slice;
use futures::executor::block_on;
use std::mem;
use std::ops::Rem;
use std::sync::Arc;
use std::time::{Duration, Instant};

use helicoid_gpurender::{
    fontcache::{
        FontCache, FontOwner, RenderPoint, RenderSpec, RenderSpecElement, RenderSquare,
        SubpixelBin, SwashCacheKey,
    },
    swash_font::SwashFont,
};
use std::{env, path::PathBuf};
use swash::{CacheKey, FontRef};

//use log;

const PRIM_BUFFER_LEN: usize = 256;

#[repr(C)]
#[derive(Copy, Clone)]
struct Globals {
    resolution: [f32; 2],
    scroll_offset: [f32; 2],
    zoom: f32,
    _pad: f32,
}

unsafe impl bytemuck::Pod for Globals {}
unsafe impl bytemuck::Zeroable for Globals {}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Color32 {
    /// Red component of the color
    pub r: u32,
    /// Green component of the color
    pub g: u32,
    /// Blue component of the color
    pub b: u32,
    /// Alpha component of the color
    pub a: u32,
}
unsafe impl bytemuck::Pod for Color32 {}
unsafe impl bytemuck::Zeroable for Color32 {}

#[repr(C)]
#[derive(Copy, Clone)]
struct GpuVertex {
    position: [f32; 2],
    normal: [f32; 2],
    prim_id: u32,
}
unsafe impl bytemuck::Pod for GpuVertex {}
unsafe impl bytemuck::Zeroable for GpuVertex {}

#[repr(C)]
#[derive(Copy, Clone)]
struct Primitive {
    color: [f32; 4],
    translate: [f32; 2],
    z_index: i32,
    width: f32,
    angle: f32,
    scale: f32,
    _pad1: i32,
    _pad2: i32,
}

impl Primitive {
    const DEFAULT: Self = Primitive {
        color: [0.0; 4],
        translate: [0.0; 2],
        z_index: 0,
        width: 0.0,
        angle: 0.0,
        scale: 1.0,
        _pad1: 0,
        _pad2: 0,
    };
}

unsafe impl bytemuck::Pod for Primitive {}
unsafe impl bytemuck::Zeroable for Primitive {}

#[repr(C)]
#[derive(Copy, Clone)]
struct BgPoint {
    point: [f32; 2],
}
unsafe impl bytemuck::Pod for BgPoint {}
unsafe impl bytemuck::Zeroable for BgPoint {}

const DEFAULT_WINDOW_WIDTH: f32 = 800.0;
const DEFAULT_WINDOW_HEIGHT: f32 = 800.0;

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

fn create_font_cache(dev: &wgpu::Device) -> FontCache<SwashFont> {
    let font = SwashFont::from_path(
        &base_asset_path().join("fonts").join("AnonymiceNerd.ttf"),
        /* &base_asset_path()
        .join("fonts")
        .join("FiraCodeNerdFont-Regular.ttf"),*/
        //        &base_asset_path().join("fonts").join("NotoSans-Regular.ttf"),
        0,
    )
    .unwrap();
    let mut font_cache = FontCache::new(font);
    let mut spec = RenderSpec::default();
    font_cache.add_atlas(
        dev,
        Extent3d {
            width: 1024,
            height: 1024,
            depth_or_array_layers: 0,
        },
    );
    font_cache
}

fn font_system_from_swash(font: &FontRef) -> FontSystem {
    let mut data = Vec::with_capacity(font.data.len());
    data.extend_from_slice(font.data);
    let mut font_db = Database::new();
    font_db.load_font_data(data);
    FontSystem::new_with_locale_and_db(String::from("C"), font_db)
    //    FontSystem::new_with_fonts([cosmic_text::fontdb::Source::Binary(Arc::new(data))].into_iter())
}

fn face_info_from_swash(font: &FontRef) -> FaceInfo {
    let mut data = Vec::with_capacity(font.data.len());
    data.extend_from_slice(font.data);
    FaceInfo {
        id: ID::dummy(),
        source: cosmic_text::fontdb::Source::Binary(Arc::new(data)),
        index: 0,
        families: vec![(String::from(""), Language::Unknown)],
        post_script_name: String::from(""),
        style: cosmic_text::Style::Normal,
        weight: Weight::NORMAL,
        stretch: cosmic_text::Stretch::Normal,
        monospaced: false,
    }
}

fn cosmic_shape_str(
    init_offset: Origin2d,
    text: &str,
    font_system: &mut FontSystem,
    scale: f32,
) -> RenderSpec {
    // Text metrics indicate the font size and line height of a buffer
    let metrics = Metrics::new(scale, scale * 1.1f32);

    // A Buffer provides shaping and layout for a UTF-8 string, create one per text widget
    let mut buffer = TextBuffer::new(font_system, metrics);

    // Borrow buffer together with the font system for more convenient method calls
    let mut buffer = buffer.borrow_with(font_system);

    // Set a size for the text buffer, in pixels
    buffer.set_size(1000.0, 1000.0);

    // Attributes indicate what font to choose
    let attrs = Attrs::new();

    // Add some text!
    buffer.set_text(text, attrs, Shaping::Basic);

    // Perform shaping as desired
    buffer.shape_until(10);
    let mut spec = RenderSpec::default();

    for run in buffer.layout_runs() {
        for (idx, glyph) in run.glyphs.iter().enumerate() {
            let physical = glyph.physical((glyph.x_offset, glyph.y_offset), 1.0);
            spec.add_element(RenderSpecElement {
                char: 'a', //run.text[glyph.start..glyph.end].chars().next().unwrap(),
                key: SwashCacheKey {
                    glyph_id: physical.cache_key.glyph_id,
                    font_size_bits: physical.cache_key.font_size_bits,
                    x_bin: physical.cache_key.x_bin.into(),
                    y_bin: physical.cache_key.y_bin.into(),
                },
                offset: Origin2d {
                    x: init_offset.x + physical.x as u32,
                    y: init_offset.y + physical.y as u32 + (run.line_y as u32),
                },
                extent: Origin2d::ZERO,
                color_idx: (idx % 128) as u16,
            })
        }
    }

    spec
}
fn simple_shape_str(text: &str, font: &FontRef, scale: f32) -> RenderSpec {
    let scaled_metrics = font.metrics(&[]).scale(scale);
    let char_width =
        (scaled_metrics.average_width + scaled_metrics.vertical_leading).ceil() as usize;
    let mut spec = RenderSpec::default();
    let charmap = font.charmap();

    for (i, char) in text.chars().into_iter().enumerate() {
        spec.add_element(RenderSpecElement {
            char,
            key: SwashCacheKey {
                glyph_id: charmap.map(char) as u16,
                font_size_bits: scale.to_bits(),
                x_bin: SubpixelBin::Zero,
                y_bin: SubpixelBin::Zero,
            },
            offset: Origin2d {
                x: (char_width * i) as u32,
                y: 0,
            },
            extent: Origin2d::ZERO,
            color_idx: 0,
        })
    }
    spec
}
/// Creates a texture that uses MSAA and fits a given swap chain
fn create_multisampled_framebuffer(
    device: &wgpu::Device,
    desc: &wgpu::SurfaceConfiguration,
    sample_count: u32,
) -> wgpu::TextureView {
    let multisampled_frame_descriptor = &wgpu::TextureDescriptor {
        label: Some("Multisampled frame descriptor"),
        size: wgpu::Extent3d {
            width: desc.width,
            height: desc.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count,
        dimension: wgpu::TextureDimension::D2,
        format: desc.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    };

    device
        .create_texture(multisampled_frame_descriptor)
        .create_view(&wgpu::TextureViewDescriptor::default())
}

/// Creates a texture that can be used for an atlas
fn create_simple_texture(
    device: &wgpu::Device,
    desc: &wgpu::SurfaceConfiguration,
) -> wgpu::TextureView {
    let frame_descriptor = &wgpu::TextureDescriptor {
        label: Some("Frame descriptor"),
        size: wgpu::Extent3d {
            width: desc.width,
            height: desc.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: desc.format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    };

    device
        .create_texture(frame_descriptor)
        .create_view(&wgpu::TextureViewDescriptor::default())
}

fn main() {
    env_logger::init();
    println!("== wgpu example ==");
    println!("Controls:");
    println!("  Arrow keys: scrolling");
    println!("  PgUp/PgDown: zoom in/out");
    println!("  b: toggle drawing the background");
    println!("  a/z: increase/decrease the stroke width");

    // Number of samples for anti-aliasing
    // Set to 1 to disable
    let sample_count = 4;

    let num_instances: u32 = 32;
    let tolerance = 0.02;

    let stroke_prim_id = 0;
    let fill_prim_id = 1;
    let arrows_prim_id = num_instances + 1;

    let mut geometry: VertexBuffers<GpuVertex, u16> = VertexBuffers::new();

    let mut fill_tess = FillTessellator::new();
    let mut stroke_tess = StrokeTessellator::new();

    // Build a Path for the rust logo.
    let mut builder = Path::builder().with_svg();
    build_logo_path(&mut builder);
    let path = builder.build();

    let arrow_points = [
        point(-1.0, -0.3),
        point(0.0, -0.3),
        point(0.0, -1.0),
        point(1.5, 0.0),
        point(0.0, 1.0),
        point(0.0, 0.3),
        point(-1.0, 0.3),
    ];

    let arrow_polygon = Polygon {
        points: &arrow_points,
        closed: true,
    };
    // Build a Path for the arrow.
    let mut builder = Path::builder();
    rounded_polygon::add_rounded_polygon(&mut builder, arrow_polygon, 0.2, NO_ATTRIBUTES);
    //builder.add_polygon(arrow_polygon);
    let arrow_path = builder.build();

    fill_tess
        .tessellate_path(
            &path,
            &FillOptions::tolerance(tolerance).with_fill_rule(tessellation::FillRule::NonZero),
            &mut BuffersBuilder::new(&mut geometry, WithId(fill_prim_id as u32)),
        )
        .unwrap();

    let fill_range = 0..(geometry.indices.len() as u32);

    stroke_tess
        .tessellate_path(
            &path,
            &StrokeOptions::tolerance(tolerance),
            &mut BuffersBuilder::new(&mut geometry, WithId(stroke_prim_id as u32)),
        )
        .unwrap();

    let stroke_range = fill_range.end..(geometry.indices.len() as u32);

    fill_tess
        .tessellate_path(
            &arrow_path,
            &FillOptions::tolerance(tolerance),
            &mut BuffersBuilder::new(&mut geometry, WithId(arrows_prim_id)),
        )
        .unwrap();

    let arrow_range = stroke_range.end..(geometry.indices.len() as u32);

    let mut bg_geometry: VertexBuffers<BgPoint, u16> = VertexBuffers::new();

    fill_tess
        .tessellate_rectangle(
            &Box2D {
                min: point(-1.0, -1.0),
                max: point(1.0, 1.0),
            },
            &FillOptions::DEFAULT,
            &mut BuffersBuilder::new(&mut bg_geometry, Custom),
        )
        .unwrap();

    let mut cpu_primitives = Vec::with_capacity(PRIM_BUFFER_LEN);
    for _ in 0..PRIM_BUFFER_LEN {
        cpu_primitives.push(Primitive {
            color: [1.0, 0.0, 0.0, 1.0],
            z_index: 0,
            width: 0.0,
            translate: [0.0, 0.0],
            angle: 0.0,
            ..Primitive::DEFAULT
        });
    }

    // Stroke primitive
    cpu_primitives[stroke_prim_id] = Primitive {
        color: [0.0, 0.0, 0.0, 1.0],
        z_index: num_instances as i32 + 2,
        width: 1.0,
        ..Primitive::DEFAULT
    };
    // Main fill primitive
    cpu_primitives[fill_prim_id] = Primitive {
        color: [1.0, 1.0, 1.0, 1.0],
        z_index: num_instances as i32 + 1,
        ..Primitive::DEFAULT
    };
    // Instance primitives
    for (idx, cpu_prim) in cpu_primitives
        .iter_mut()
        .enumerate()
        .skip(fill_prim_id + 1)
        .take(num_instances as usize - 1)
    {
        cpu_prim.z_index = (idx as u32 + 1) as i32;
        cpu_prim.color = [
            (0.1 * idx as f32).rem(1.0),
            (0.5 * idx as f32).rem(1.0),
            (0.9 * idx as f32).rem(1.0),
            1.0,
        ];
    }

    let mut scene = SceneParams {
        target_zoom: 5.0,
        zoom: 5.0,
        target_scroll: vector(70.0, 70.0),
        scroll: vector(70.0, 70.0),
        show_points: false,
        stroke_width: 1.0,
        target_stroke_width: 1.0,
        draw_background: true,
        draw_text: String::from(
            "Testing rust rendering -> a <=> !@#$ || && .target_scroll: vector(70.0, 70.0),
println!(\"Insert-err: {:?} {:?}\", &key, e);            ",
        ),
        window_size: PhysicalSize::new(DEFAULT_WINDOW_WIDTH as u32, DEFAULT_WINDOW_HEIGHT as u32),
        size_changed: true,
        render: false,
        changed: true,
    };

    let event_loop = EventLoop::new();
    let window_builder = WindowBuilder::new().with_inner_size(scene.window_size);
    let window = window_builder.build(&event_loop).unwrap();

    // create an instance
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        dx12_shader_compiler: wgpu::Dx12Compiler::Fxc,
    });

    // create an surface
    let surface = unsafe { instance.create_surface(&window) }.unwrap();

    // create an adapter
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: Some(&surface),
        force_fallback_adapter: false,
    }))
    .unwrap();
    // create a device and a queue
    let (device, queue) = block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: None,
            features: wgpu::Features::default(),
            limits: wgpu::Limits::default(),
        },
        None,
    ))
    .unwrap();

    // Create a text font cache and prepare a rendered string
    let mut font_cache = create_font_cache(&device);
    let text_spec = if scene.draw_text.is_empty() {
        None
    } else {
        let mut font_system = font_system_from_swash(&font_cache.owner().swash_font());
        let mut text_spec = Some(cosmic_shape_str(
            Origin2d { x: 10, y: 10 },
            &scene.draw_text,
            &mut font_system,
            //50.0,
            17.0,
        ));
        font_cache.offset_glyphs(text_spec.as_mut().unwrap());
        text_spec
        /*
        Some(simple_shape_str(
            &scene.draw_text,
            &font_cache.owner().swash_font(),
            12f32,
        ))*/
    };

    let vbo = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&geometry.vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });

    let ibo = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&geometry.indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    let bg_vbo = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&bg_geometry.vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });

    let bg_ibo = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&bg_geometry.indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    let prim_buffer_byte_size = (PRIM_BUFFER_LEN * std::mem::size_of::<Primitive>()) as u64;
    let globals_buffer_byte_size = std::mem::size_of::<Globals>() as u64;

    let prims_ubo = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Prims ubo"),
        size: prim_buffer_byte_size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let globals_ubo = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Globals ubo"),
        size: globals_buffer_byte_size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let vs_module = &device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Geometry vs"),
        source: wgpu::ShaderSource::Wgsl(include_str!("./../shaders/geometry.vs.wgsl").into()),
    });
    let fs_module = &device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Geometry fs"),
        source: wgpu::ShaderSource::Wgsl(include_str!("./../shaders/geometry.fs.wgsl").into()),
    });
    let bg_vs_module = &device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Background vs"),
        source: wgpu::ShaderSource::Wgsl(include_str!("./../shaders/background.vs.wgsl").into()),
    });
    let bg_fs_module = &device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Background fs"),
        source: wgpu::ShaderSource::Wgsl(include_str!("./../shaders/background.fs.wgsl").into()),
    });
    let text_vs_module = &device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Text vs"),
        source: wgpu::ShaderSource::Wgsl(include_str!("./../shaders/text.vs.wgsl").into()),
    });
    let text_fs_module = &device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Text fs"),
        source: wgpu::ShaderSource::Wgsl(include_str!("./../shaders/text.fs.wgsl").into()),
    });

    let mut palette_host = Vec::<u32>::with_capacity(128);
    palette_host.resize(128, 0xFF000000);

    palette_host[0] = 0xFFFF0000;
    palette_host[1] = 0xFF00FF00;
    palette_host[2] = 0xFF0000FF;
    palette_host[3] = 0xFF00FFFF;
    palette_host[4] = 0xFFFF00FF;
    //palette_host[5] = 0xFFFFFF00;
    palette_host[10] = 0xF0000000;
    palette_host[11] = 0xFF000000;
    palette_host[12] = 0xFF000000;
    palette_host[13] = 0xFF000088;
    palette_host[14] = 0xFF000088;
    palette_host[15] = 0xFFFFFFFF;
    palette_host[16] = 0xFFFFFFFF;
    palette_host[17] = 0xFFFFFFFF;
    palette_host[18] = 0xFFFFFFFF;
    palette_host[19] = 0xFFFFFFFF;

    let palette_descriptor = &wgpu::TextureDescriptor {
        label: Some("Frame descriptor"),
        size: wgpu::Extent3d {
            width: 128,
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

    let palette_texture = device.create_texture(palette_descriptor);

    let palette_view = palette_texture.create_view(&TextureViewDescriptor::default());
    let palette_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        address_mode_w: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Nearest,
        mipmap_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Bind group layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(globals_buffer_byte_size),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(prim_buffer_byte_size),
                },
                count: None,
            },
        ], //
    });

    let text_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Text Bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(globals_buffer_byte_size),
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

    let mut text_render_run = if let Some(text_spec) = text_spec {
        Some(font_cache.render_run(&device, &text_spec).unwrap())
    } else {
        None
    };

    //        Some(font_cache.render_run(&text_spec).unwrap())
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Bind group"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(globals_ubo.as_entire_buffer_binding()),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Buffer(prims_ubo.as_entire_buffer_binding()),
            },
        ],
    });

    let text_bind_group = if let Some(render_run) = text_render_run.as_ref() {
        let buffer_vertices = render_run.gpu_vertices.as_ref().unwrap();
        Some(
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Text Bind group"),
                layout: &text_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(
                            globals_ubo.as_entire_buffer_binding(),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(
                            font_cache
                                .atlas_ref(
                                    &helicoid_gpurender::texture_atlases::AtlasLocation::atlas_only(
                                        0,
                                    ),
                                )
                                .unwrap()
                                .sampler()
                                .unwrap(),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(
                            &font_cache
                                .atlas_ref(
                                    &helicoid_gpurender::texture_atlases::AtlasLocation::atlas_only(
                                        0,
                                    ),
                                )
                                .unwrap()
                                .texture()
                                .unwrap()
                                .create_view(&TextureViewDescriptor::default()),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&palette_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(&palette_view),
                    },
                ],
            }),
        )
    } else {
        None
    };

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
        label: None,
    });
    let text_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        bind_group_layouts: &[&text_bind_group_layout],
        push_constant_ranges: &[],
        label: None,
    });

    let depth_stencil_state = Some(wgpu::DepthStencilState {
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
    });

    let mut render_pipeline_descriptor = wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: vs_module,
            entry_point: "main",
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<GpuVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute {
                        offset: 0,
                        format: wgpu::VertexFormat::Float32x4,
                        shader_location: 0,
                    },
                    wgpu::VertexAttribute {
                        offset: 8,
                        format: wgpu::VertexFormat::Float32x2,
                        shader_location: 1,
                    },
                    wgpu::VertexAttribute {
                        offset: 16,
                        format: wgpu::VertexFormat::Uint32,
                        shader_location: 2,
                    },
                ],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: fs_module,
            entry_point: "main",
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Bgra8UnormSrgb,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            polygon_mode: wgpu::PolygonMode::Fill,
            front_face: wgpu::FrontFace::Ccw,
            strip_index_format: None,
            cull_mode: Some(wgpu::Face::Back),
            conservative: false,
            unclipped_depth: false,
        },
        depth_stencil: depth_stencil_state.clone(),
        multisample: wgpu::MultisampleState {
            count: sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
    };

    let render_pipeline = device.create_render_pipeline(&render_pipeline_descriptor);

    // TODO: this isn't what we want: we'd need the equivalent of VK_POLYGON_MODE_LINE,
    // but it doesn't seem to be exposed by wgpu?
    render_pipeline_descriptor.primitive.topology = wgpu::PrimitiveTopology::LineList;

    let bg_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: bg_vs_module,
            entry_point: "main",
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Point>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[wgpu::VertexAttribute {
                    offset: 0,
                    format: wgpu::VertexFormat::Float32x2,
                    shader_location: 0,
                }],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: bg_fs_module,
            entry_point: "main",
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Bgra8UnormSrgb,
                blend: None,
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
        depth_stencil: depth_stencil_state.clone(),
        multisample: wgpu::MultisampleState {
            count: sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
    });

    let text_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&text_pipeline_layout),
        vertex: wgpu::VertexState {
            module: text_vs_module,
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
            module: text_fs_module,
            entry_point: "main",
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Bgra8UnormSrgb,
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::Src,
                        dst_factor: wgpu::BlendFactor::OneMinusSrc1,
                        //dst_factor: wgpu::BlendFactor::OneMinusSrc,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::Src,
                        dst_factor: wgpu::BlendFactor::OneMinusSrc1Alpha,
                        operation: wgpu::BlendOperation::Add,
                    },
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
        multisample: wgpu::MultisampleState {
            count: sample_count,
            mask: !0,
            alpha_to_coverage_enabled: true,
        },
        multiview: None,
    });

    let size = window.inner_size();

    let mut surface_desc = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Bgra8UnormSrgb,
        width: size.width,
        height: size.height,
        present_mode: wgpu::PresentMode::AutoVsync,
        alpha_mode: CompositeAlphaMode::Auto,
        view_formats: vec![],
    };

    let mut multisampled_render_target = None;

    surface.configure(&device, &surface_desc);

    queue.write_texture(
        wgpu::ImageCopyTexture {
            aspect: wgpu::TextureAspect::All,
            texture: &palette_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
        },
        bytemuck::cast_slice(palette_host.as_slice()),
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(4 * 128),
            rows_per_image: Some(1),
        },
        Extent3d {
            width: 128,
            height: 1,
            depth_or_array_layers: 1,
        },
    );

    let mut depth_texture_view = None;

    let start = Instant::now();
    let mut next_report = start + Duration::from_secs(1);
    let mut frame_count: u32 = 0;
    let mut time_secs: f32 = 0.0;

    window.request_redraw();

    event_loop.run(move |event, _, control_flow| {
        if !update_inputs(event, &window, control_flow, &mut scene) {
            // keep polling inputs.
            return;
        }

        if scene.size_changed {
            println!("Scene changed: Size: {:?}", scene.window_size);
            scene.size_changed = false;
            let physical = scene.window_size;
            surface_desc.width = physical.width;
            surface_desc.height = physical.height;
            surface.configure(&device, &surface_desc);

            let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Depth texture"),
                size: wgpu::Extent3d {
                    width: surface_desc.width,
                    height: surface_desc.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });

            depth_texture_view =
                Some(depth_texture.create_view(&wgpu::TextureViewDescriptor::default()));

            multisampled_render_target = if sample_count > 1 {
                Some(create_multisampled_framebuffer(
                    &device,
                    &surface_desc,
                    sample_count,
                ))
            } else {
                None
            };
        }

        if !scene.render {
            return;
        }

        scene.render = false;
        scene.changed = false;

        let frame = match surface.get_current_texture() {
            Ok(texture) => texture,
            Err(e) => {
                println!("Swap-chain error: {e:?}");
                return;
            }
        };

        let frame_view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Encoder"),
        });

        cpu_primitives[stroke_prim_id].width = scene.stroke_width;
        cpu_primitives[stroke_prim_id].color = [
            (time_secs * 0.8 - 1.6).sin() * 0.1 + 0.1,
            (time_secs * 0.5 - 1.6).sin() * 0.1 + 0.1,
            (time_secs - 1.6).sin() * 0.1 + 0.1,
            1.0,
        ];

        for idx in 2..(num_instances + 1) {
            cpu_primitives[idx as usize].translate = [
                (time_secs * 0.05 * idx as f32).sin() * (100.0 + idx as f32 * 10.0),
                (time_secs * 0.1 * idx as f32).sin() * (100.0 + idx as f32 * 10.0),
            ];
        }

        let mut arrow_count = 0;
        let offset = (time_secs * 10.0).rem(5.0);
        walk::walk_along_path(
            path.iter(),
            offset,
            0.1,
            &mut walk::RepeatedPattern {
                callback: |event: walk::WalkerEvent| {
                    if arrow_count + num_instances as usize + 1 >= PRIM_BUFFER_LEN {
                        // Don't want to overflow the primitive buffer,
                        // just skip the remaining arrows.
                        return false;
                    }
                    cpu_primitives[arrows_prim_id as usize + arrow_count] = Primitive {
                        color: [0.7, 0.9, 0.8, 1.0],
                        translate: (event.position * 2.3 - vector(80.0, 80.0)).to_array(),
                        angle: event.tangent.angle_from_x_axis().get(),
                        scale: 2.0,
                        z_index: arrows_prim_id as i32,
                        ..Primitive::DEFAULT
                    };
                    arrow_count += 1;
                    true
                },
                intervals: &[5.0, 5.0, 5.0],
                index: 0,
            },
        );

        queue.write_buffer(
            &globals_ubo,
            0,
            bytemuck::cast_slice(&[Globals {
                resolution: [
                    scene.window_size.width as f32,
                    scene.window_size.height as f32,
                ],
                zoom: scene.zoom,
                scroll_offset: scene.scroll.to_array(),
                _pad: 0.0,
            }]),
        );

        queue.write_buffer(&prims_ubo, 0, bytemuck::cast_slice(&cpu_primitives));

        if let Some(text_render_run) = text_render_run.as_mut() {
            text_render_run.queue_write_buffer(&queue);
        }
        {
            // A resolve target is only supported if the attachment actually uses anti-aliasing
            // So if sample_count == 1 then we must render directly to the surface's buffer
            let color_attachment = if let Some(msaa_target) = &multisampled_render_target {
                wgpu::RenderPassColorAttachment {
                    view: msaa_target,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.25,
                            g: 0.25,
                            b: 0.25,
                            a: 1.0,
                        }),
                        store: true,
                    },
                    resolve_target: Some(&frame_view),
                }
            } else {
                wgpu::RenderPassColorAttachment {
                    view: &frame_view,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.25,
                            g: 0.25,
                            b: 0.25,
                            a: 1.0,
                        }),
                        //                        load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                        store: true,
                    },
                    resolve_target: None,
                }
            };

            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(color_attachment)],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_texture_view.as_ref().unwrap(),
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: true,
                    }),
                    stencil_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0),
                        store: true,
                    }),
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(&render_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.set_index_buffer(ibo.slice(..), wgpu::IndexFormat::Uint16);
            pass.set_vertex_buffer(0, vbo.slice(..));

            /*pass.draw_indexed(fill_range.clone(), 0, 0..num_instances);
            pass.draw_indexed(stroke_range.clone(), 0, 0..1);
            pass.draw_indexed(arrow_range.clone(), 0, 0..(arrow_count as u32));*/

            if scene.draw_background {
                pass.set_pipeline(&bg_pipeline);
                pass.set_bind_group(0, &bind_group, &[]);
                pass.set_index_buffer(bg_ibo.slice(..), wgpu::IndexFormat::Uint16);
                pass.set_vertex_buffer(0, bg_vbo.slice(..));

                pass.draw_indexed(0..6, 0, 0..1);
            }

            //if !scene.draw_text.is_empty() {
            //if true {
            if let Some(text_render_run) = text_render_run.as_ref() {
                /*println!(
                    "Host data: {:?}",
                    &font_cache
                        .atlas(&helicoid_gpurender::texture_atlases::AtlasLocation::atlas_only(0))
                        .unwrap()
                        .backed_up_texture()
                        .host_data()[0..1024]
                );*/
                font_cache
                    .atlas(&helicoid_gpurender::texture_atlases::AtlasLocation::atlas_only(0))
                    .unwrap()
                    .update_texture(&device, &queue);

                pass.set_pipeline(&text_pipeline);
                let tmp_text_bind_group = text_bind_group.as_ref().unwrap();
                let text_vbo = text_render_run.gpu_vertices.as_ref().unwrap();
                let buffer_indices = text_render_run.gpu_indices.as_ref().unwrap();
                pass.set_bind_group(0, tmp_text_bind_group, &[]);
                pass.set_index_buffer(buffer_indices.slice(..), wgpu::IndexFormat::Uint16);
                pass.set_vertex_buffer(0, text_vbo.slice(..));
                pass.draw_indexed(
                    0..(buffer_indices.size() as u32 / std::mem::size_of::<u16>() as u32),
                    0,
                    0..1,
                );
            }
        }

        queue.submit(Some(encoder.finish()));
        frame.present();

        frame_count += 1;
        let now = Instant::now();
        time_secs = (now - start).as_secs_f32();
        if now >= next_report {
            println!("{frame_count} FPS");
            frame_count = 0;
            next_report = now + Duration::from_secs(1);
        }
    });
}

/// This vertex constructor forwards the positions and normals provided by the
/// tessellators and add a shape id.
pub struct WithId(pub u32);

impl FillVertexConstructor<GpuVertex> for WithId {
    fn new_vertex(&mut self, vertex: tessellation::FillVertex) -> GpuVertex {
        GpuVertex {
            position: vertex.position().to_array(),
            normal: [0.0, 0.0],
            prim_id: self.0,
        }
    }
}

impl StrokeVertexConstructor<GpuVertex> for WithId {
    fn new_vertex(&mut self, vertex: tessellation::StrokeVertex) -> GpuVertex {
        GpuVertex {
            position: vertex.position_on_path().to_array(),
            normal: vertex.normal().to_array(),
            prim_id: self.0,
        }
    }
}

pub struct Custom;

impl FillVertexConstructor<BgPoint> for Custom {
    fn new_vertex(&mut self, vertex: tessellation::FillVertex) -> BgPoint {
        BgPoint {
            point: vertex.position().to_array(),
        }
    }
}

struct SceneParams {
    target_zoom: f32,
    zoom: f32,
    target_scroll: Vector,
    scroll: Vector,
    show_points: bool,
    stroke_width: f32,
    target_stroke_width: f32,
    draw_background: bool,
    draw_text: String,
    window_size: PhysicalSize<u32>,
    size_changed: bool,
    render: bool,
    changed: bool,
}

fn update_inputs(
    event: Event<()>,
    window: &Window,
    control_flow: &mut ControlFlow,
    scene: &mut SceneParams,
) -> bool {
    match event {
        Event::RedrawRequested(_) => {
            scene.render = true;
        }
        Event::RedrawEventsCleared => {
            if scene.changed || scene.size_changed {
                window.request_redraw();
            }
        }
        Event::WindowEvent {
            event: WindowEvent::Destroyed,
            ..
        }
        | Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } => {
            *control_flow = ControlFlow::Exit;
            return false;
        }
        Event::WindowEvent {
            event: WindowEvent::Resized(size),
            ..
        } => {
            println!("Window evt: {:?}", event);
            scene.window_size = size;
            scene.size_changed = true;
            scene.changed = true;
        }
        Event::WindowEvent {
            event:
                WindowEvent::KeyboardInput {
                    event:
                        KeyEvent {
                            state: ElementState::Pressed,
                            physical_key: key,
                            ..
                        },
                    ..
                },
            ..
        } => {
            scene.changed = true;
            match key {
                KeyCode::Escape => {
                    *control_flow = ControlFlow::Exit;
                    return false;
                }
                KeyCode::PageDown => {
                    scene.target_zoom *= 0.8;
                }
                KeyCode::PageUp => {
                    scene.target_zoom *= 1.25;
                }
                KeyCode::ArrowLeft => {
                    scene.target_scroll.x -= 50.0 / scene.target_zoom;
                }
                KeyCode::ArrowRight => {
                    scene.target_scroll.x += 50.0 / scene.target_zoom;
                }
                KeyCode::ArrowUp => {
                    scene.target_scroll.y -= 50.0 / scene.target_zoom;
                }
                KeyCode::ArrowDown => {
                    scene.target_scroll.y += 50.0 / scene.target_zoom;
                }
                KeyCode::KeyP => {
                    scene.show_points = !scene.show_points;
                }
                KeyCode::KeyB => {
                    scene.draw_background = !scene.draw_background;
                }
                KeyCode::KeyA => {
                    scene.target_stroke_width /= 0.8;
                }
                KeyCode::KeyZ => {
                    scene.target_stroke_width *= 0.8;
                }
                _key => {}
            }
        }
        _evt => {
            //println!("{:?}", _evt);
        }
    }
    //println!(" -- zoom: {}, scroll: {:?}", scene.target_zoom, scene.target_scroll);

    scene.zoom += (scene.target_zoom - scene.zoom) / 3.0;
    scene.scroll = scene.scroll + (scene.target_scroll - scene.scroll) / 3.0;
    scene.stroke_width =
        scene.stroke_width + (scene.target_stroke_width - scene.stroke_width) / 5.0;

    *control_flow = ControlFlow::Poll;

    true
}
