use hashbrown::HashMap;
use std::cell::RefCell;
use std::hash::{BuildHasher, Hash, Hasher};

use helicoid_protocol::block_manager::{
    BlockContainer, BlockGfx, BlockRenderParents, InteriorBlockContainer, ManagerGfx, MetaBlock,
    RenderBlockFullId,
};
use helicoid_protocol::gfx::{
    FontPaint, PathVerb, PointF16, PointF32, RemoteBoxUpdate, RenderBlockLocation,
    SimpleDrawElement, SimplePaint, SVG_RESOURCE_NAME_LEN,
};
use helicoid_protocol::{
    gfx::{MetaDrawBlock, RenderBlockDescription, RenderBlockId, SimpleDrawBlock},
    text::ShapedTextBlock,
};
use parking_lot::Mutex;
use skia_safe as skia;
use skia_safe::canvas::PointMode;
use skia_safe::gpu::{DirectContext, SurfaceOrigin};
use skia_safe::{
    BlendMode, Budgeted, Canvas, Color, Data, Handle, ISize, Image, ImageInfo, Paint, Path,
    PathFillType, Point, Size, Surface, SurfaceProps, SurfacePropsFlags, Vector,
};
use smallvec::SmallVec;

use crate::renderer::fonts::blob_builder::ShapedBlobBuilder;

lazy_static! {
    static ref SVG_CACHE: SvgResourcePixmapCache = SvgResourcePixmapCache::new();
}

thread_local! {
    pub static SHAPED_BLOB_BUILDER : RefCell<ShapedBlobBuilder> = RefCell::new(ShapedBlobBuilder::new());
}
/* Seeds for hashes: The hashes should stay consistent so we can compare them */
const S1: u64 = 0x1199AACCDD117766;
const S2: u64 = 0x99AACCDD11776611;
const S3: u64 = 0xAACCDD1177661199;
const S4: u64 = 0xCCDD117766AACE7D;

struct SvgResourcePixmapCache {
    resources: Mutex<HashMap<SmallVec<[u8; SVG_RESOURCE_NAME_LEN]>, HashMap<(u16, u16), Vec<u8>>>>,
}
impl SvgResourcePixmapCache {
    pub fn new() -> Self {
        Self {
            resources: Mutex::new(Default::default()),
        }
    }
    pub fn fetch_resource<F: FnOnce(&Vec<u8>, u32, u32) -> V, V>(
        &self,
        name: &SmallVec<[u8; SVG_RESOURCE_NAME_LEN]>,
        size: &PointF32,
        handle: F,
    ) -> Option<V> {
        let mut res = self.resources.lock();
        if !res.contains_key(name) {
            res.insert(name.clone(), HashMap::default());
        }
        let name_resource = res.get_mut(name).unwrap();
        let resource_name_str = std::str::from_utf8(name).unwrap();
        let sx = size.x().round() as u16;
        let sy = size.y().round() as u16;
        match name_resource.entry((sx, sy)) {
            hashbrown::hash_map::Entry::Occupied(e) => {
                Some(handle(e.into_mut(), sx as u32, sy as u32))
            }
            hashbrown::hash_map::Entry::Vacant(ve) => {
                let exe_path = std::env::current_exe().unwrap();
                let resource_path = exe_path
                    .as_path()
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .join("assets")
                    .join(resource_name_str)
                    .with_extension("svg");
                let Ok(resource_contents) = std::fs::read_to_string(resource_path) else {
                        log::trace!("Could not load data from svg path");
                return None;
            };

                if let Ok(svg_tree) = usvg::Tree::from_str(&resource_contents, &Default::default())
                {
                    let mut pixmap = tiny_skia::Pixmap::new(sx as u32, sy as u32).unwrap();
                    resvg::render(
                        &svg_tree,
                        usvg::FitTo::Size(sx as u32, sy as u32),
                        tiny_skia::Transform::identity(),
                        pixmap.as_mut(),
                    )
                    .unwrap();
                    let rdata = pixmap.data();
                    let mut rvec = Vec::with_capacity(rdata.len());
                    rvec.extend_from_slice(rdata);
                    Some(handle(ve.insert(rvec), sx as u32, sy as u32))
                } else {
                    log::trace!("Coult not parse svg");
                    None
                }
            }
        }
    }
}
pub struct SkiaGfxManager {}

struct RenderedRenderBlock {
    image: Image,
    description_hash: u64,
}
/* Implement debug manually as skia sometime panics when printing debug info for its images */
impl std::fmt::Debug for RenderedRenderBlock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RenderedRenderBlock")
            .field("description_hash", &self.description_hash)
            .finish()
    }
}
#[derive(Debug)]
pub struct SkiaClientRenderBlock {
    rendered: Option<RenderedRenderBlock>,
    //    canvas: Option<Canvas>
    //    id: RenderBlockId,
    //    wire_description: RenderBlockDescription,
}
/*
pub struct BlockManager {
    layers: Vec<Option<Vec<RenderBlockId>>>,
    /* The blocks are options so they can be moved out while rendering to enable the manager to be
    passed mutable for sub-blocks */
    blocks: HashMap<RenderBlockId, Option<RenderBlock>>,
    top_level_block: RenderBlockId,
}

impl BlockManager {
    pub fn new() -> Self {
        Self {
            layers: Default::default(),
            blocks: Default::default(),
            top_level_block: RenderBlockId::normal(1).unwrap(), /* TODO: The server have to specify this more properly? */
        }
    }
    pub fn render(&mut self, target: &mut Surface) {
        //        let block_id = self.top_level_block;i
        //                self.top
        let top_block_id = self.top_level_block;
        let block = self.blocks.get_mut(&top_block_id);
        log::trace!(
            "BM try render block:{:?} ({:?})",
            block
                .as_ref()
                .map(|hb| hb.as_ref().map(|b| b.wire_description.clone())),
            top_block_id
        );

        if block.as_ref().map(|b| b.is_some()).unwrap_or(false) {
            let mut moved_block = block.unwrap().take().unwrap();
            let location = RenderBlockLocation {
                id: top_block_id,
                location: PointF16::default(),
                layer: 0,
            };
            log::trace!("BM render block:{:?}", location);
            /* The bloc is temporary moved out of the storage, so storage can be passed on as mutable */
            moved_block.render(&location, self, target);
            // Put the block back

            let post_block = self.blocks.get_mut(&top_block_id);
            let post_block_inner = post_block.unwrap();
            *post_block_inner = Some(moved_block);
        }
    }
    pub fn handle_block_update(&mut self, update: &RemoteBoxUpdate) {
        for block in update.new_render_blocks.iter() {
            log::trace!("Update render block: {:?}", block.id);
            let new_rendered_block = RenderBlock::new(block.contents.clone());
            if let Some(render_block) = self.blocks.get_mut(&block.id) {
                *render_block = Some(new_rendered_block);
            } else {
                self.blocks.insert(block.id, Some(new_rendered_block));
            }
        }
    }
}
*/
fn simple_paint_to_sk_paint(sm_paint: &SimplePaint, fill: bool) -> Paint {
    let mut sk_paint = Paint::default();
    //sk_paint.set_blend_mode(BlendMode::SrcOver);
    let sk_blend_mode = match sm_paint.blend {
        helicoid_protocol::gfx::SimpleBlendMode::Clear => skia::BlendMode::Clear,
        helicoid_protocol::gfx::SimpleBlendMode::Src => skia::BlendMode::Src,
        helicoid_protocol::gfx::SimpleBlendMode::Dst => skia::BlendMode::Dst,
        helicoid_protocol::gfx::SimpleBlendMode::SrcOver => skia::BlendMode::SrcOver,
        helicoid_protocol::gfx::SimpleBlendMode::DstOver => skia::BlendMode::DstOver,
        helicoid_protocol::gfx::SimpleBlendMode::SrcIn => skia::BlendMode::SrcIn,
        helicoid_protocol::gfx::SimpleBlendMode::DstIn => skia::BlendMode::DstIn,
        helicoid_protocol::gfx::SimpleBlendMode::SrcOut => skia::BlendMode::SrcOut,
        helicoid_protocol::gfx::SimpleBlendMode::DstOut => skia::BlendMode::DstOut,
        helicoid_protocol::gfx::SimpleBlendMode::SrcATop => skia::BlendMode::SrcATop,
        helicoid_protocol::gfx::SimpleBlendMode::DstATop => skia::BlendMode::DstATop,
        helicoid_protocol::gfx::SimpleBlendMode::Xor => skia::BlendMode::Xor,
        helicoid_protocol::gfx::SimpleBlendMode::Plus => skia::BlendMode::Plus,
        helicoid_protocol::gfx::SimpleBlendMode::Modulate => skia::BlendMode::Modulate,
        helicoid_protocol::gfx::SimpleBlendMode::Screen => skia::BlendMode::Screen,
        helicoid_protocol::gfx::SimpleBlendMode::Overlay => skia::BlendMode::Overlay,
        helicoid_protocol::gfx::SimpleBlendMode::Darken => skia::BlendMode::Darken,
        helicoid_protocol::gfx::SimpleBlendMode::Lighten => skia::BlendMode::Lighten,
        helicoid_protocol::gfx::SimpleBlendMode::ColorDodge => skia::BlendMode::ColorDodge,
        helicoid_protocol::gfx::SimpleBlendMode::ColorBurn => skia::BlendMode::ColorBurn,
        helicoid_protocol::gfx::SimpleBlendMode::HardLight => skia::BlendMode::HardLight,
        helicoid_protocol::gfx::SimpleBlendMode::SoftLight => skia::BlendMode::SoftLight,
        helicoid_protocol::gfx::SimpleBlendMode::Difference => skia::BlendMode::Difference,
        helicoid_protocol::gfx::SimpleBlendMode::Exclusion => skia::BlendMode::Exclusion,
        helicoid_protocol::gfx::SimpleBlendMode::Multiply => skia::BlendMode::Multiply,
        helicoid_protocol::gfx::SimpleBlendMode::Hue => skia::BlendMode::Hue,
        helicoid_protocol::gfx::SimpleBlendMode::Saturation => skia::BlendMode::Saturation,
        helicoid_protocol::gfx::SimpleBlendMode::Color => skia::BlendMode::Color,
        helicoid_protocol::gfx::SimpleBlendMode::Luminosity => skia::BlendMode::Luminosity,
    };
    sk_paint.set_blend_mode(sk_blend_mode);
    sk_paint.set_anti_alias(true);

    if fill {
        sk_paint.set_color(sm_paint.fill_color);
        sk_paint.set_style(skia::PaintStyle::Fill);
    } else {
        sk_paint.set_style(skia::PaintStyle::Stroke);
        sk_paint.set_color(sm_paint.line_color);
        sk_paint.set_stroke_width(sm_paint.line_width());
    }
    sk_paint
}

pub fn font_paint_to_sk_paint(sm_paint: &FontPaint) -> Paint {
    let mut sk_paint = Paint::default();
    //sk_paint.set_blend_mode(BlendMode::SrcOver);
    let sk_blend_mode = match sm_paint.blend {
        helicoid_protocol::gfx::SimpleBlendMode::Clear => skia::BlendMode::Clear,
        helicoid_protocol::gfx::SimpleBlendMode::Src => skia::BlendMode::Src,
        helicoid_protocol::gfx::SimpleBlendMode::Dst => skia::BlendMode::Dst,
        helicoid_protocol::gfx::SimpleBlendMode::SrcOver => skia::BlendMode::SrcOver,
        helicoid_protocol::gfx::SimpleBlendMode::DstOver => skia::BlendMode::DstOver,
        helicoid_protocol::gfx::SimpleBlendMode::SrcIn => skia::BlendMode::SrcIn,
        helicoid_protocol::gfx::SimpleBlendMode::DstIn => skia::BlendMode::DstIn,
        helicoid_protocol::gfx::SimpleBlendMode::SrcOut => skia::BlendMode::SrcOut,
        helicoid_protocol::gfx::SimpleBlendMode::DstOut => skia::BlendMode::DstOut,
        helicoid_protocol::gfx::SimpleBlendMode::SrcATop => skia::BlendMode::SrcATop,
        helicoid_protocol::gfx::SimpleBlendMode::DstATop => skia::BlendMode::DstATop,
        helicoid_protocol::gfx::SimpleBlendMode::Xor => skia::BlendMode::Xor,
        helicoid_protocol::gfx::SimpleBlendMode::Plus => skia::BlendMode::Plus,
        helicoid_protocol::gfx::SimpleBlendMode::Modulate => skia::BlendMode::Modulate,
        helicoid_protocol::gfx::SimpleBlendMode::Screen => skia::BlendMode::Screen,
        helicoid_protocol::gfx::SimpleBlendMode::Overlay => skia::BlendMode::Overlay,
        helicoid_protocol::gfx::SimpleBlendMode::Darken => skia::BlendMode::Darken,
        helicoid_protocol::gfx::SimpleBlendMode::Lighten => skia::BlendMode::Lighten,
        helicoid_protocol::gfx::SimpleBlendMode::ColorDodge => skia::BlendMode::ColorDodge,
        helicoid_protocol::gfx::SimpleBlendMode::ColorBurn => skia::BlendMode::ColorBurn,
        helicoid_protocol::gfx::SimpleBlendMode::HardLight => skia::BlendMode::HardLight,
        helicoid_protocol::gfx::SimpleBlendMode::SoftLight => skia::BlendMode::SoftLight,
        helicoid_protocol::gfx::SimpleBlendMode::Difference => skia::BlendMode::Difference,
        helicoid_protocol::gfx::SimpleBlendMode::Exclusion => skia::BlendMode::Exclusion,
        helicoid_protocol::gfx::SimpleBlendMode::Multiply => skia::BlendMode::Multiply,
        helicoid_protocol::gfx::SimpleBlendMode::Hue => skia::BlendMode::Hue,
        helicoid_protocol::gfx::SimpleBlendMode::Saturation => skia::BlendMode::Saturation,
        helicoid_protocol::gfx::SimpleBlendMode::Color => skia::BlendMode::Color,
        helicoid_protocol::gfx::SimpleBlendMode::Luminosity => skia::BlendMode::Luminosity,
    };
    sk_paint.set_blend_mode(sk_blend_mode);
    sk_paint.set_anti_alias(true);

    sk_paint.set_color(sm_paint.color);
    sk_paint
}

impl SkiaClientRenderBlock {
    pub fn new(_desc: &RenderBlockDescription) -> Self {
        Self {
            rendered: None,
            //            wire_description: desc,
        }
    }
    pub fn new_top_block() -> Self {
        Self { rendered: None }
    }
    pub fn render_text_box(
        &mut self,
        location: &RenderBlockLocation,
        target: &mut SkiaClientRenderTarget<'_>,
        meta: &mut MetaBlock<SkiaClientRenderBlock>,
    ) {
        let Some(RenderBlockDescription::ShapedTextBlock(stb)) = &meta.wire_description() else {
            panic!("Render text box should not be called with a description that is not a ShapedTextBlock")
        };
        log::trace!("Render text box: {:?} {:?}", meta.parent_path(), meta.id());
        /* TODO: Use and configuration  of blob builder and storage of fonts should be improved,
        probably delegated to storage */
        let shaped = stb;
        let blobs = SHAPED_BLOB_BUILDER.with(|blob_builder| {
            let mut blob_builder_mut = blob_builder.borrow_mut();
            if !blob_builder_mut.has_font_key(1) {
                blob_builder_mut.set_font_key(1, String::from("FiraCodeNerdFont-Regular"));
            }
            blob_builder_mut.bulid_blobs(&shaped)
        });
        //blob_builder.set_font_key(0, String::from("AnonymiceNerd"));
        //blob_builder.set_font_key(1, String::from("NotoSansMono-Regular"));
        /*        blob_builder.set_font_key(1, String::from("FiraCodeNerdFont-Regular"));
        blob_builder.set_font_key(2, String::from("NotoColorEmoji"));
        blob_builder.set_font_key(3, String::from("MissingGlyphs"));
        blob_builder.set_font_key(4, String::from("LastResort-Regular"));*/

        let mut x = 0f32;
        let y = 0f32;

        let mut paint = Paint::default();
        paint.set_blend_mode(BlendMode::SrcOver);
        paint.set_anti_alias(true);
        let canvas = target.target_surface.canvas();
        canvas.save();
        canvas.translate(Vector::new(location.location.x(), location.location.y()));

        log::trace!(
            "Draw text: {:?} at x:{}, y:{}, paint: {:?}",
            blobs,
            x,
            y,
            shaped.metadata_runs.first().map(|r| &r.paint)
        );
        for (blob, metadata_run) in blobs.iter().zip(shaped.metadata_runs.iter()) {
            let paint = font_paint_to_sk_paint(&metadata_run.paint);
            canvas.draw_text_blob(blob, (x as f32, y as f32), &paint);
        }
        let mut rect_paint = Paint::default();
        rect_paint.set_stroke_width(1.0);
        rect_paint.set_style(skia::PaintStyle::Stroke);
        for metadata_run in shaped.metadata_runs.iter() {
            if metadata_run.font_info.font_parameters.underlined {
                canvas.draw_line(
                    Point {
                        x: x as f32,
                        y: y + metadata_run.baseline_y(),
                    },
                    Point {
                        x: x as f32 + metadata_run.advance_x(),
                        y: y + metadata_run.baseline_y(),
                    },
                    &rect_paint,
                );
            }
            /* This is kind of unneeded / silly */
            /*            target.draw_rect(
                Rect {
                    left: x,
                    top: y,
                    right: x + metadata_run.advance_x(),
                    bottom: y + metadata_run.advance_y(),
                },
                &rect_paint,
            );*/
            x += metadata_run.advance_x();
        }
        canvas.restore();
    }
    pub fn render_simple_draw(
        &mut self,
        location: &RenderBlockLocation,
        target: &mut SkiaClientRenderTarget<'_>,
        meta: &mut MetaBlock<SkiaClientRenderBlock>,
    ) {
        let Some(RenderBlockDescription::SimpleDraw(sd)) = &meta.wire_description() else {
            panic!("Render simple draw should not be called with a description that is not SimpleDraw")
        };
        log::trace!(
            "Render simple draw: {:?} {:?}",
            meta.parent_path(),
            meta.id()
        );
        let mut paint = Paint::default();
        paint.set_blend_mode(BlendMode::Overlay);
        paint.set_anti_alias(true);
        let any_bg_blur = sd.draw_elements.iter().any(|e| match e {
            SimpleDrawElement::Fill(f) => f.paint.background_blur_amount() > 0f32,
            SimpleDrawElement::RoundRect(rr) => rr.paint.background_blur_amount() > 0f32,
            _ => false,
        });
        let canvas = if any_bg_blur {
            let image = target.target_surface.image_snapshot();
            let canvas = target.target_surface.canvas();
            let mut image_filter = None;
            /* Blur the target surface at the areas filled by the squares */
            for element in &sd.draw_elements {
                let filter_rect_info = match element {
                    SimpleDrawElement::Fill(f) if f.paint.background_blur_amount() > 0f32 => {
                        Some((
                            skia::Rect::new(0.0, 0.0, sd.extent.x(), sd.extent.y()),
                            f.paint.background_blur_amount(),
                        ))
                    }
                    SimpleDrawElement::RoundRect(rr)
                        if rr.paint.background_blur_amount() > 0f32 =>
                    {
                        Some((
                            skia::Rect::new(
                                rr.topleft.x(),
                                rr.topleft.y(),
                                rr.bottomright.x(),
                                rr.bottomright.y(),
                            ),
                            rr.paint.background_blur_amount(),
                        ))
                    }
                    _ => None,
                };
                if let Some((filter_rect, blur_amount)) = filter_rect_info {
                    log::debug!("Dev transform: {:?}", canvas.local_to_device_as_3x3());
                    image_filter = skia::image_filters::blur(
                        (blur_amount, blur_amount),
                        skia::TileMode::Clamp,
                        image_filter.take(),
                        skia::matrix::Matrix::concat(
                            &skia::matrix::Matrix::translate(Vector::new(
                                location.location.x(),
                                location.location.y(),
                            )),
                            &canvas.local_to_device_as_3x3(),
                        )
                        .map_rect(filter_rect)
                        .0,
                    );
                }
            }
            //canvas.set_image_filter(image_filter);
            let mut paint = skia::Paint::default();
            paint.set_image_filter(image_filter);
            canvas.draw_image(image, (0, 0), Some(&paint));
            canvas
        } else {
            target.target_surface.canvas()
        };

        canvas.save();
        canvas.translate(Vector::new(location.location.x(), location.location.y()));

        for element in &sd.draw_elements {
            match element {
                SimpleDrawElement::Polygon(sdp) => {
                    let mut points =
                        SmallVec::<[skia::Point; 16]>::with_capacity(sdp.draw_elements.len());
                    points.extend(
                        sdp.draw_elements
                            .iter()
                            .map(|p| skia::Point::new(p.x(), p.y())),
                    );
                    let path = Path::polygon(&points, sdp.closed, PathFillType::Winding, None);
                    log::trace!("Draw polygon: {:?} {:?}", points, sdp.paint);
                    if (sdp.paint.fill_color >> 24 & 0xFF) != 0 {
                        let polygon_fill_paint = simple_paint_to_sk_paint(&sdp.paint, true);
                        canvas.draw_path(&path, &polygon_fill_paint);
                    }
                    if sdp.paint.line_width() != 0.0 {
                        let polygon_stroke_paint = simple_paint_to_sk_paint(&sdp.paint, false);
                        canvas.draw_path(&path, &polygon_stroke_paint);
                    }
                }
                SimpleDrawElement::Path(sdp) => {
                    let mut path = Path::new();
                    for (verb, p1, p2, p3) in sdp.draw_elements.iter() {
                        match verb {
                            PathVerb::Move => {
                                path.move_to(skia::Point::new(p1.x(), p1.y()));
                            }
                            PathVerb::Line => {
                                path.line_to(skia::Point::new(p1.x(), p1.y()));
                            }
                            PathVerb::Quad => {
                                path.quad_to(
                                    skia::Point::new(p1.x(), p1.y()),
                                    skia::Point::new(p2.x(), p2.y()),
                                );
                            }
                            PathVerb::Conic => {
                                path.conic_to(
                                    skia::Point::new(p1.x(), p1.y()),
                                    skia::Point::new(p2.x(), p2.y()),
                                    p3.x(),
                                );
                            }
                            PathVerb::Cubic => {
                                path.cubic_to(
                                    skia::Point::new(p1.x(), p1.y()),
                                    skia::Point::new(p2.x(), p2.y()),
                                    skia::Point::new(p3.x(), p3.y()),
                                );
                            }
                            PathVerb::Close => {
                                path.close();
                            }
                            PathVerb::Done => {
                                break;
                            }
                        }
                    }
                    log::trace!("Draw path: {:?} {:?}", path, sdp.paint);
                    if (sdp.paint.fill_color >> 24 & 0xFF) != 0 {
                        let polygon_fill_paint = simple_paint_to_sk_paint(&sdp.paint, true);
                        canvas.draw_path(&path, &polygon_fill_paint);
                    }
                    if sdp.paint.line_width() != 0.0 {
                        let polygon_stroke_paint = simple_paint_to_sk_paint(&sdp.paint, false);
                        canvas.draw_path(&path, &polygon_stroke_paint);
                    }
                }
                SimpleDrawElement::RoundRect(rr) => {
                    let rect = skia::Rect::new(
                        rr.topleft.x(),
                        rr.topleft.y(),
                        rr.bottomright.x(),
                        rr.bottomright.y(),
                    );
                    if (rr.paint.fill_color >> 24 & 0xFF) != 0 {
                        let rect_fill_paint = simple_paint_to_sk_paint(&rr.paint, true);
                        canvas.draw_round_rect(
                            rect,
                            rr.roundedness.x(),
                            rr.roundedness.y(),
                            &rect_fill_paint,
                        );
                    }
                    if rr.paint.line_width() != 0.0 {
                        let rect_stroke_paint = simple_paint_to_sk_paint(&rr.paint, false);
                        canvas.draw_round_rect(
                            rect,
                            rr.roundedness.x(),
                            rr.roundedness.y(),
                            &rect_stroke_paint,
                        );
                    }
                }
                SimpleDrawElement::Fill(f) => {
                    let rect = skia::Rect::new(0f32, 0f32, sd.extent.x(), sd.extent.y());
                    if (f.paint.fill_color >> 24 & 0xFF) != 0 {
                        let mut rect_fill_paint = simple_paint_to_sk_paint(&f.paint, true);
                        canvas.draw_rect(rect, &rect_fill_paint);
                    }
                    if f.paint.line_width() != 0.0 {
                        let rect_stroke_paint = simple_paint_to_sk_paint(&f.paint, false);
                        canvas.draw_rect(rect, &rect_stroke_paint);
                    }
                }
                SimpleDrawElement::SvgResource(svg) => {
                    /* TODO: We should really cache this svg as a pixmap */
                    let resource_name_str = std::str::from_utf8(&svg.resource_name).unwrap();
                    /* Make sure this is an acceptable / valid resource name */
                    log::trace!("Render svg: {:?}", resource_name_str);
                    assert!(resource_name_str.chars().all(|c| (('A' <= c && c <= 'Z')
                        || ('a' <= c && c <= 'z')
                        || ('0' <= c && c <= '9'))));
                    assert!(resource_name_str.len() < 64);
                    /* This is slow when rendering, ideally it should be made async and communicate when it is done */
                    SVG_CACHE.fetch_resource(&svg.resource_name, &svg.extent, |data, sx, sy| {
                        let sk_paint = simple_paint_to_sk_paint(&svg.paint, true);
                        let pixmap_img = Image::from_raster_data(
                            &ImageInfo::new(
                                ISize::new((sx as i32).max(1), (sy as i32).max(1)),
                                skia::ColorType::RGBA8888,
                                skia::AlphaType::Premul,
                                None,
                            ),
                            unsafe { Data::new_bytes(&data) },
                            4 * sx as usize,
                        )
                        .unwrap();
                        canvas.draw_image(
                            pixmap_img,
                            Point::new(location.location.x(), location.location.y()),
                            Some(&sk_paint),
                        );
                    });
                }
            }
        }
        canvas.restore();
    }

    /* // Remove the hashing from the renderer, that is the domain of the meta
    fn hash_block_recursively<H: Hasher>(&self, hasher: &mut H,
        meta: &mut MetaBlock<SkiaClientRenderBlock>
    ) {
        match self.wire_description() {
            RenderBlockDescription::MetaBox(_) => {
                // TODO: Handle none containers better here
                todo!("Hash from the protocol")
                meta.hash_meta_box_recursively(hasher)
            }
            _ => self.wire_description().hash(hasher),
        }
    }*/

    /* This function renders a box containing other boxes, using a cached texture if it exists
    and the hash of the description matches the previously rendered contents */
    pub fn render_meta_box(
        &mut self,
        location: &RenderBlockLocation,
        target: &mut SkiaClientRenderTarget<'_>,
        meta: &mut MetaBlock<SkiaClientRenderBlock>,
        //        parents: &mut BlockRenderParents<Self>,
    ) {
        let Some(RenderBlockDescription::MetaBox(mb)) = &meta.wire_description() else {
            panic!("Render simple draw should not be called with a description that is not a simple draw")
        };
        let target_surface = &mut target.target_surface;
        let mut hasher =
            ahash::random_state::RandomState::with_seeds(S1, S2, S3, S4).build_hasher();
        location.hash(&mut hasher);
        //mb.hash(&mut hasher);
        meta.hash_block_recursively(&mut hasher);
        /* TODO: All referenced blocks needs to be recursively hashed here for it to work */
        let hashed = hasher.finish();
        let mut paint = Paint::default();
        paint.set_blend_mode(BlendMode::Overlay);
        paint.set_anti_alias(true);
        if let Some(cached) = &self.rendered {
            /* TODO: Do we trust the hash here, or do we want to store the previous contents too so
            we can do a proper equality comparision?*/
            if cached.description_hash == hashed && !meta.contains_blur() {
                log::trace!("Reuse hash: {} for {:?}", hashed, self);
                /* Contents is already rendered, reuse the rendered image */
                /*                target_surface.canvas().draw_image(
                    &cached.image,
                    as_skpoint(&location.location),
                    Some(&paint),
                );*/
                let src_img = &cached.image;
                let img_tmp; /* For lifetime reasons */
                let extent = mb.extent;
                if (extent.x() as i32 > 0) && (extent.y() as i32 > 0) {
                    let clipped_src_img = if (src_img.width() as f32 > extent.x()
                        || src_img.height() as f32 > extent.y())
                    {
                        /* Clip image as it is too big */
                        img_tmp = src_img
                            .new_subset(skia::IRect::new(
                                0,
                                0,
                                extent.x() as i32,
                                extent.y() as i32,
                            ))
                            .unwrap();
                        &img_tmp
                    } else {
                        src_img
                    };
                    paint.set_blend_mode(BlendMode::SrcOver);
                    target_surface.canvas().draw_image(
                        src_img,
                        as_skpoint(&location.location),
                        Some(&paint),
                    );
                }
                return;
            }
        }
        if mb.buffered {
            let mut context: DirectContext = target_surface
                .recording_context()
                .map(|mut c| c.as_direct_context())
                .flatten()
                .unwrap();
            let target_image_info = target_surface.image_info();
            let image_info = ImageInfo::new(
                ISize {
                    width: (mb.extent.x() as i32).max(1),
                    height: (mb.extent.y() as i32).max(1),
                },
                target_image_info.color_type(),
                target_image_info.alpha_type(),
                target_image_info.color_space(),
            );
            //TODO: We should try to reuse the same surface as before if the parameters has not changed
            let mut dest_surface = build_sub_surface(&mut context, image_info.clone());
            dest_surface.canvas().clear(Color::new(0));
            log::trace!(
                "Meta box surface: {:?} (image info: {:?})",
                dest_surface,
                image_info
            );
            let adjusted_location = RenderBlockLocation {
                id: location.id,
                location: PointF32::default(),
                layer: location.layer,
            };
            let extent = mb.extent;
            self.render_meta_box_contents(&adjusted_location, &mut dest_surface, meta);
            self.rendered = Some(RenderedRenderBlock {
                image: dest_surface.image_snapshot(),
                description_hash: hashed,
            });
            let src_img = &self.rendered.as_ref().unwrap().image;
            let img_tmp; /* For lifetime reasons */
            if (extent.x() as i32 > 0) && (extent.y() as i32 > 0) {
                let clipped_src_img = if (src_img.width() as f32 > extent.x()
                    || src_img.height() as f32 > extent.y())
                {
                    /* Clip image as it is too big */
                    img_tmp = src_img
                        .new_subset(skia::IRect::new(0, 0, extent.x() as i32, extent.y() as i32))
                        .unwrap();
                    &img_tmp
                } else {
                    src_img
                };
                paint.set_blend_mode(BlendMode::SrcOver);
                target_surface.canvas().draw_image(
                    src_img,
                    as_skpoint(&location.location),
                    Some(&paint),
                );
            }
        } else {
            let adjusted_location = RenderBlockLocation {
                id: location.id,
                location: location.location,
                layer: location.layer,
            };
            self.rendered = None;
            target_surface.canvas().save();
            target_surface.canvas().clip_rect(
                skia::Rect::new(
                    location.location.x(),
                    location.location.y(),
                    location.location.x() + mb.extent.x(),
                    location.location.y() + mb.extent.y(),
                ),
                Some(skia::ClipOp::Intersect),
                Some(true),
            );
            self.render_meta_box_contents(&adjusted_location, target_surface, meta);
            target_surface.canvas().restore();
        }
    }
    fn render_meta_box_contents<'a, 't, 'p>(
        &'a mut self,
        location: &'t RenderBlockLocation,
        target: &'t mut Surface,
        meta: &mut MetaBlock<SkiaClientRenderBlock>,
    ) {
        let (wire_description, container) = meta.destruct_mut();
        let Some(RenderBlockDescription::MetaBox(mb)) = wire_description else {
            panic!("Render meta box should not be called with a description that is not a meta box")
        };
        let container = container
            .as_mut()
            .expect("Expecting block to have container if wire description has children");
        /*let mut target = SkiaClientRenderTarget {
            location,
            target_surface,
        };*/
        //        let popt = parents.parent.as_ref();
        /*      let mut parents = BlockRenderParents::<Self> {
            parent: popt,
            gfx_block: &mut parents.gfx_block,
        };*/
        let mut skr_target = SkiaClientRenderTarget::<'t> {
            location,
            target_surface: target,
        };
        meta.process_block_recursively(self, &mut skr_target);

        /*
        // How do we sort the blocks?
        let mut blocks =
            SmallVec::<[(RenderBlockLocation); 64]>::with_capacity(mb.sub_blocks.len());
        blocks.extend(mb.sub_blocks.iter().map(|b| b.clone()));
        //blocks.extend(mb.sub_blocks.iter().map(|b| (b.id, b.layer, b.location)));
        blocks.sort_by(|a, b| a.layer.cmp(&b.layer));
        for location in blocks {
            let block = container.block_ref_mut(location.id);
            if block.as_ref().map(|b| b.is_some()).unwrap_or(false) {
                let mut moved_block = block.unwrap().take().unwrap();
                /* The bloc is temporary moved out of the storage, so storage can be passed on as mutable */
                let (block, gfx) = moved_block.destruct_mut();
                gfx.render(&location, target, block);
                // Put the block back
                let post_block = container.block_ref_mut(location.id);
                let post_block_inner = post_block.unwrap();
                *post_block_inner = Some(moved_block);
            }
        }*/
    }
}

pub struct SkiaClientRenderTarget<'a> {
    pub location: &'a RenderBlockLocation,
    pub target_surface: &'a mut Surface,
}
impl BlockGfx for SkiaClientRenderBlock {
    type RenderTarget<'b> = SkiaClientRenderTarget<'b>;

    fn render(
        &mut self,
        location: &RenderBlockLocation,
        //        parents: &mut BlockRenderParents<Self>,
        block: &mut MetaBlock<Self>,
        target: &mut Self::RenderTarget<'_>,
    ) {
        log::trace!(
            "Render block gfx: {:?}/{}",
            block.parent_path(),
            block.id().0
        );
        //        let target = parents.gfx_block.painter;
        //        let desc = block.wire_description();
        if let Some(wire_description) = block.wire_description().as_ref() {
            match wire_description {
                RenderBlockDescription::ShapedTextBlock(_) => {
                    self.render_text_box(location, target, block)
                }
                RenderBlockDescription::SimpleDraw(_) => {
                    self.render_simple_draw(location, target, block)
                }
                RenderBlockDescription::MetaBox(_) => self.render_meta_box(location, target, block),
            }
        }
    }
}

impl ManagerGfx<SkiaClientRenderBlock> for SkiaGfxManager {
    fn create_gfx_block(
        &mut self,
        wire_description: &RenderBlockDescription,
        _parent_path: helicoid_protocol::gfx::RenderBlockPath,
        _id: RenderBlockId,
    ) -> SkiaClientRenderBlock {
        SkiaClientRenderBlock::new(wire_description)
    }

    fn create_top_block(&mut self, id: RenderBlockId) -> SkiaClientRenderBlock {
        SkiaClientRenderBlock::new_top_block()
    }
    fn reset(&mut self) {
        /* Clear any manager specific resources */
    }
}
impl SkiaGfxManager {
    pub fn new() -> Self {
        Self {}
    }
}

fn build_sub_surface(context: &mut DirectContext, image_info: ImageInfo) -> Surface {
    let budgeted = Budgeted::Yes;
    let surface_origin = SurfaceOrigin::TopLeft;
    // Subpixel layout (should be configurable/obtained from fontconfig).
    let props = SurfaceProps::new(SurfacePropsFlags::default(), skia::PixelGeometry::RGBH);
    Surface::new_render_target(
        context,
        budgeted,
        &image_info,
        None,
        surface_origin,
        Some(&props),
        None,
    )
    .expect(format!("Could not create surface: {:?}", image_info).as_str())
}

fn as_skpoint(p: &PointF32) -> Point {
    Point { x: p.x(), y: p.y() }
}
/*
trait BlockContentsRenderer {
    fn render(&self, desc: &RenderBlockDescription, storage: &BlockManager, target: &mut Surface);
}
enum BlockContentsRenderers {
    ShapedTextBlock(ShapedTextBlockRenderer),
    SimpleDraw(SimpleDrawBlockRenderer),
    MetaBox(MetaDrawBlockRenderer),
}

impl BlockContentsRenderers {
    pub fn as_renderer(&self) -> &dyn BlockContentsRenderer {
        match self {
            BlockContentsRenderers::ShapedTextBlock(stb) => stb,
            BlockContentsRenderers::SimpleDraw(sd) => sd,
            BlockContentsRenderers::MetaBox(mb) => mb,
        }
    }
}

struct ShapedTextBlockRenderer {}
impl BlockContentsRenderer for ShapedTextBlockRenderer {
    fn render(&self, desc: &RenderBlockDescription, storage: &BlockManager, target: &mut Surface) {}
}
struct SimpleDrawBlockRenderer {}
impl BlockContentsRenderer for SimpleDrawBlockRenderer {
    fn render(&self, desc: &RenderBlockDescription, storage: &BlockManager, target: &mut Surface) {}
}
struct MetaDrawBlockRenderer {}
impl BlockContentsRenderer for MetaDrawBlockRenderer {
    fn render(&self, desc: &RenderBlockDescription, storage: &BlockManager, target: &mut Surface) {}
}
*/
