use lyon::extra::rust_logo::build_logo_path;
use lyon::math::*;
use lyon::path::{Path, Polygon, NO_ATTRIBUTES};
use lyon::tessellation;
use lyon::tessellation::geometry_builder::*;
use lyon::tessellation::{FillOptions, FillTessellator};
use lyon::tessellation::{StrokeOptions, StrokeTessellator};

use lyon::algorithms::{rounded_polygon, walk};

pub mod fontcache;
pub mod swash_font;
pub mod text_renderer;
pub mod texture_atlases;
pub mod texture_map;

struct WgpuState {
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: winit::dpi::PhysicalSize<u32>,
    // NEW!
    render_pipeline: wgpu::RenderPipeline,
}
