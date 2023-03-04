use std::collections::{hash_map::DefaultHasher, HashMap};
use std::hash::{Hash, Hasher};

use helicoid_protocol::block_manager::{
    BlockContainer, BlockGfx, BlockRenderParents, InteriorBlockContainer, ManagerGfx, MetaBlock,
    RenderBlockFullId,
};
use helicoid_protocol::gfx::{PointF16, RemoteBoxUpdate, RenderBlockLocation};
use helicoid_protocol::{
    gfx::{MetaDrawBlock, RenderBlockDescription, RenderBlockId, SimpleDrawBlock},
    text::ShapedTextBlock,
};
use skia_safe::gpu::{DirectContext, SurfaceOrigin};
use skia_safe::{
    BlendMode, Budgeted, Canvas, Color, ISize, Image, ImageInfo, Paint, Point, Surface,
    SurfaceProps, SurfacePropsFlags, Vector,
};
use smallvec::SmallVec;

use crate::renderer::fonts::blob_builder::ShapedBlobBuilder;
pub struct SkiaGfxManager {}

struct RenderedRenderBlock {
    image: Image,
    description_hash: u64,
}
/* Implement debug manually as skia sometime panics when printing debug info for its images */
impl std::fmt::Debug for RenderedRenderBlock{
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
    pub fn render(
        &mut self,
        location: &RenderBlockLocation,
        target_surface: &mut Surface,
        meta: &mut MetaBlock<SkiaClientRenderBlock>,
    ) {
        //        s
        let mut target = SkiaClientRenderTarget {
            location,
            target_surface,
        };
        if let Some(wire_description) = meta.wire_description().as_ref() {
            match wire_description {
                RenderBlockDescription::ShapedTextBlock(_) => {
                    self.render_text_box(location, &mut target, meta)
                }
                RenderBlockDescription::SimpleDraw(_) => {
                    self.render_simple_draw(location, &mut target, meta)
                }
                RenderBlockDescription::MetaBox(_) => {
                    self.render_meta_box(location, &mut target, meta)
                }
            }
        }
    }
    //    pub fn set_desc()
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
        let mut blob_builder = ShapedBlobBuilder::new();
        blob_builder.set_font_key(0, String::from("Anonymous Pro"));
        //blob_builder.set_font_key(1, String::from("NotoSansMono-Regular"));
        blob_builder.set_font_key(1, String::from("FiraCodeNerdFont-Regular"));
        blob_builder.set_font_key(2, String::from("NotoColorEmoji"));
        blob_builder.set_font_key(3, String::from("MissingGlyphs"));
        blob_builder.set_font_key(4, String::from("LastResort-Regular"));

        let shaped = stb;
        let blobs = blob_builder.bulid_blobs(&shaped);
        let mut x = 0f32;
        let y = 0f32;

        let mut paint = Paint::default();
        paint.set_blend_mode(BlendMode::SrcOver);
        paint.set_anti_alias(true);
        let canvas = target.target_surface.canvas();
        canvas.translate(Vector::new(location.location.x(), location.location.y()));

        log::trace!("Draw text: {:?}", blobs);
        for (blob, metadata_run) in blobs.iter().zip(shaped.metadata_runs.iter()) {
            paint.set_color(Color::new(metadata_run.font_color));
            canvas.draw_text_blob(blob, (x as f32, y as f32), &paint);
        }
        let mut rect_paint = Paint::default();
        rect_paint.set_stroke_width(1.0);
        rect_paint.set_style(skia_safe::PaintStyle::Stroke);
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
    }
    pub fn render_simple_draw(
        &mut self,
        location: &RenderBlockLocation,
        target: &mut SkiaClientRenderTarget<'_>,
        meta: &mut MetaBlock<SkiaClientRenderBlock>,
    ) {
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
        let mut hasher = DefaultHasher::new();
        location.hash(&mut hasher);
        //mb.hash(&mut hasher);
        meta.hash_block_recursively(&mut hasher);
        /* TODO: All referenced blocks needs to be recursively hashed here for it to work */
        let hashed = hasher.finish();
        let mut paint = Paint::default();
        paint.set_blend_mode(BlendMode::SrcOver);
        paint.set_anti_alias(true);
        if let Some(cached) = &self.rendered {
            /* TODO: Do we trust the hash here, or do we want to store the previous contents too so
            we can do a proper equality comparision?*/
            if cached.description_hash == hashed {
                /* Contents is already rendered, reuse the rendered image */
                target_surface.canvas().draw_image(
                    &cached.image,
                    as_skpoint(&location.location),
                    Some(&paint),
                );
                return;
            }
        }
        let mut context: DirectContext = target_surface
            .recording_context()
            .map(|mut c| c.as_direct_context())
            .flatten()
            .unwrap();
        let target_image_info = target_surface.image_info();
        let image_info = ImageInfo::new(
            ISize {
                width: mb.extent.x() as i32,
                height: mb.extent.y() as i32,
            },
            target_image_info.color_type(),
            target_image_info.alpha_type(),
            target_image_info.color_space(),
        );
        let mut dest_surface = build_sub_surface(&mut context, image_info.clone());
        log::trace!(
            "Meta box surface: {:?} (image info: {:?})",
            dest_surface,
            image_info
        );
        let adjusted_location = RenderBlockLocation {
            id: location.id,
            location: PointF16::default(),
            layer: location.layer,
        };
        self.render_meta_box_contents(&adjusted_location, &mut dest_surface, meta);
        self.rendered = Some(RenderedRenderBlock {
            image: dest_surface.image_snapshot(),
            description_hash: hashed,
        });
        target_surface.canvas().draw_image(
            &self.rendered.as_ref().unwrap().image,
            as_skpoint(&location.location),
            Some(&paint),
        );
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
        log::trace!("Render block gfx");
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
    let props = SurfaceProps::new(SurfacePropsFlags::default(), skia_safe::PixelGeometry::RGBH);
    Surface::new_render_target(
        context,
        budgeted,
        &image_info,
        None,
        surface_origin,
        Some(&props),
        None,
    )
    .expect("Could not create surface")
}

fn as_skpoint(p: &PointF16) -> Point {
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
