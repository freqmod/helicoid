use hashbrown::HashMap;
use helicoid_protocol::text::SHAPABLE_STRING_ALLOC_RUNS;
use std::cell::RefCell;
use std::hash::{BuildHasher, Hash, Hasher};
use wgpu::RenderPass;

use helicoid_protocol::block_manager::{BlockGfx, ManagerGfx, MetaBlock};
use helicoid_protocol::gfx::{
    FontPaint, PathVerb, PointF32, RenderBlockLocation, SimpleDrawElement, SimplePaint,
    SVG_RESOURCE_NAME_LEN,
};
use helicoid_protocol::gfx::{RenderBlockDescription, RenderBlockId};
use parking_lot::Mutex;

use smallvec::SmallVec;

use cosmic_text;

use crate::font::fontcache::{FontCache, FontId, RenderSpec, RenderTargetId, RenderedRun};
use crate::font::swash_font::SwashFont;
use crate::font::texture_atlases::TextureInfo;

/* Seeds for hashes: The hashes should stay consistent so we can compare them */
const S1: u64 = 0x1199AACCDD117766;
const S2: u64 = 0x99AACCDD11776611;
const S3: u64 = 0xAACCDD1177661199;
const S4: u64 = 0xCCDD117766AACE7D;

pub struct WGpuGfxManager {}
/*
struct RenderedRenderBlock {
    image: TextureInfo,
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
*/

#[derive(Debug)]
struct TextRenderBlockInner {
    source: RenderSpec,
    runs: SmallVec<[RenderedRun; SHAPABLE_STRING_ALLOC_RUNS]>,
}
#[derive(Default, Debug)]
enum RenderBlockInner {
    #[default]
    None,
    MetaBlock(),
    TextBlock(TextRenderBlockInner),
}
#[derive(Debug, Default)]
pub struct WGpuClientRenderBlock {
    inner: RenderBlockInner,
    //    rendered: Option<RenderedRenderBlock>,
}

impl WGpuClientRenderBlock {
    pub fn new(_desc: &RenderBlockDescription) -> Self {
        Self {
            inner: Default::default(), //            rendered: None,
                                       //            wire_description: desc,
        }
    }
    pub fn new_top_block() -> Self {
        Default::default()
    }
    pub fn render_text_box(
        &mut self,
        location: &RenderBlockLocation,
        target: &mut WGpuClientRenderTarget<'_>,
        meta: &mut MetaBlock<WGpuClientRenderBlock>,
    ) {
        //self.render_cache(location, target, meta, &Self::render_text_box_contents)
        self.render_text_box_contents(location, target, meta);
    }
    fn render_text_box_contents(
        &mut self,
        location: &RenderBlockLocation,
        target: &mut WGpuClientRenderTarget,
        meta: &mut MetaBlock<WGpuClientRenderBlock>,
    ) {
        /*    fn render_text_box_contents(
            &mut self,
            location: &RenderBlockLocation,
            target: &mut SkiaClientRenderTarget<'_>,
            meta: &mut MetaBlock<WGpuClientRenderBlock>,
        ) {*/
        let Some(RenderBlockDescription::ShapedTextBlock(stb)) = &meta.wire_description() else {
            panic!("Render text box should not be called with a description that is not a ShapedTextBlock")
        };
        log::trace!("Render text box: {:?} {:?}", meta.parent_path(), meta.id());
        // TODO: Render text using wgpu (see helcoid-wgpu main)
        /* Create a vertexlist etc. and hash it. If the source and atlas haven't
        changed reuse the vertex list. */

        /*
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
               let canvas = target.canvas();
               canvas.save();
               canvas.translate(Vector::new(location.location.x(), location.location.y()));

               log::trace!(
                   "Draw text: {:?} at x:{}, y:{}, size: {:?}",
                   blobs,
                   x,
                   y,
                   shaped
                       .metadata_runs
                       .first()
                       .map(|r| &r.font_info.font_parameters.size)
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
        */
    }
    pub fn render_simple_draw(
        &mut self,
        location: &RenderBlockLocation,
        target: &mut WGpuClientRenderTarget<'_>,
        meta: &mut MetaBlock<WGpuClientRenderBlock>,
    ) {
        let Some(RenderBlockDescription::SimpleDraw(sd)) = &meta.wire_description() else {
            panic!("Render simple draw should not be called with a description that is not SimpleDraw")
        };
        log::trace!(
            "Render simple draw: {:?} {:?}",
            meta.parent_path(),
            meta.id()
        );
        /*
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
                        let rect_fill_paint = simple_paint_to_sk_paint(&f.paint, true);
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
        */
    }

    /* // Remove the hashing from the renderer, that is the domain of the meta
    fn hash_block_recursively<H: Hasher>(&self, hasher: &mut H,
        meta: &mut MetaBlock<WGpuClientRenderBlock>
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
    pub fn render_meta_box(
        &mut self,
        location: &RenderBlockLocation,
        target: &mut WGpuClientRenderTarget<'_>,
        meta: &mut MetaBlock<WGpuClientRenderBlock>,
    ) {
        /* This function renders a box containing other boxes, using a cached texture if it exists
        and the hash of the description matches the previously rendered contents */
        //self.render_cached(location, target, meta, &Self::render_meta_box_contents);

        /* For WGPU caching the rendered results are not done, as it is assumed
        that rendering every frame is just as fast as managing all the texture
        memory for cached render. However the data used for rendering (vertex lists)
        etc are cached */

        let (wire_description, container) = meta.destruct_mut();
        let Some(RenderBlockDescription::MetaBox(_mb)) = wire_description else {
            panic!("Render meta box should not be called with a description that is not a meta box")
        };
        let _container = container
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
        meta.process_block_recursively(self, target);

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

pub struct WGpuClientRenderTarget<'a> {
    pub location: &'a RenderBlockLocation,
    pub target_pass: &'a mut RenderPass<'a>,
    pub target_id: RenderTargetId,
    pub font_caches: HashMap<FontId, FontCache<SwashFont>>,
}
impl BlockGfx for WGpuClientRenderBlock {
    type RenderTarget<'b> = WGpuClientRenderTarget<'b>;

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

impl ManagerGfx<WGpuClientRenderBlock> for WGpuGfxManager {
    fn create_gfx_block(
        &mut self,
        wire_description: &RenderBlockDescription,
        _parent_path: helicoid_protocol::gfx::RenderBlockPath,
        _id: RenderBlockId,
    ) -> WGpuClientRenderBlock {
        WGpuClientRenderBlock::new(wire_description)
    }

    fn create_top_block(&mut self, _id: RenderBlockId) -> WGpuClientRenderBlock {
        WGpuClientRenderBlock::new_top_block()
    }
    fn reset(&mut self) {
        /* Clear any manager specific resources */
    }
}

impl WGpuGfxManager {
    pub fn new() -> Self {
        Self {}
    }
}

// TODO: Consider adding a function to create a temporary texture to render to
// (similar to build_sub_surface below)
/*
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
*/
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
