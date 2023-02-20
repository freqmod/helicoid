use std::collections::{hash_map::DefaultHasher, HashMap};
use std::hash::{Hash, Hasher};

use helicoid_protocol::gfx::{PointF16, RemoteBoxUpdate, RenderBlockLocation};
use helicoid_protocol::{
    gfx::{MetaDrawBlock, RenderBlockDescription, RenderBlockId, SimpleDrawBlock},
    text::ShapedTextBlock,
};
use skia_safe::gpu::{DirectContext, SurfaceOrigin};
use skia_safe::{
    BlendMode, Budgeted, Color, ISize, Image, ImageInfo, Paint, Point, Surface, SurfaceProps,
    SurfacePropsFlags, Vector,
};
use smallvec::SmallVec;

use crate::renderer::fonts::blob_builder::ShapedBlobBuilder;
struct RenderedRenderBlock {
    image: Image,
    description_hash: u64,
}
pub struct RenderBlock {
    rendered: Option<RenderedRenderBlock>,
    //    id: RenderBlockId,
    wire_description: RenderBlockDescription,
}

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

impl RenderBlock {
    pub fn new(desc: RenderBlockDescription) -> Self {
        Self {
            rendered: None,
            wire_description: desc,
        }
    }
    pub fn render(
        &mut self,
        //desc: &RenderBlockDescription,
        location: &RenderBlockLocation,
        storage: &mut BlockManager,
        target: &mut Surface,
    ) {
        //        s
        match self.wire_description {
            RenderBlockDescription::ShapedTextBlock(_) => {
                self.render_text_box(location, storage, target)
            }
            RenderBlockDescription::SimpleDraw(_) => {
                self.render_simple_draw(location, storage, target)
            }
            RenderBlockDescription::MetaBox(_) => self.render_meta_box(location, storage, target),
        }
    }
    //    pub fn set_desc()
    pub fn render_text_box(
        &mut self,
        location: &RenderBlockLocation,
        storage: &mut BlockManager,
        target: &mut Surface,
    ) {
        let RenderBlockDescription::ShapedTextBlock(stb) = &self.wire_description else {
            panic!("Render text box should not be called with a description that is not a ShapedTextBlock")
        };
        /* TODO: Use and configuration  of blob builder and storage of fonts should be improved,
        probably delegated to storage */
        let mut blob_builder = ShapedBlobBuilder::new();
        blob_builder.set_font_key(0, String::from("FiraCodeNerdFont-Regular"));
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
        let canvas = target.canvas();
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
        storage: &mut BlockManager,
        target: &mut Surface,
    ) {
    }
    fn hash_block_recursively<H: Hasher>(
        &self,
        storage: &BlockManager,
        //        location: &RenderBlockLocation,
        hasher: &mut H,
    ) {
        match self.wire_description {
            RenderBlockDescription::MetaBox(_) => self.hash_meta_box_recursively(storage, hasher),
            _ => self.wire_description.hash(hasher),
        }
    }
    fn hash_meta_box_recursively<H: Hasher>(&self, storage: &BlockManager, hasher: &mut H) {
        let RenderBlockDescription::MetaBox(mb) = &self.wire_description else {
            panic!("Hash meta box should not be called with a description that is not a meta box")
        };
        mb.hash(hasher);
        for block in mb.sub_blocks.iter() {
            let render_block = storage.blocks.get(&block.id);
            if render_block.as_ref().map(|b| b.is_some()).unwrap_or(false) {
                let extracted_block = render_block.unwrap().as_ref().unwrap();
                extracted_block.hash_block_recursively(storage, hasher);
            } else {
                false.hash(hasher);
            }
        }
    }
    /* This function renders a box containing other boxes, using a cached texture if it exists
    and the hash of the description matches the previously rendered contents */
    pub fn render_meta_box(
        &mut self,
        location: &RenderBlockLocation,
        storage: &mut BlockManager,
        target: &mut Surface,
    ) {
        let RenderBlockDescription::MetaBox(mb) = &self.wire_description else {
            panic!("Render simple draw should not be called with a description that is not a simple draw")
        };
        let mut hasher = DefaultHasher::new();
        location.hash(&mut hasher);
        //mb.hash(&mut hasher);
        self.hash_block_recursively(storage, &mut hasher);
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
                target.canvas().draw_image(
                    &cached.image,
                    as_skpoint(&location.location),
                    Some(&paint),
                );
                return;
            }
        }
        //        let dest_image
        let mut context: DirectContext = target
            .recording_context()
            .map(|mut c| c.as_direct_context())
            .flatten()
            .unwrap();
        let target_image_info = target.image_info();
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
        self.render_meta_box_contents(&adjusted_location, storage, &mut dest_surface);
        self.rendered = Some(RenderedRenderBlock {
            image: dest_surface.image_snapshot(),
            description_hash: hashed,
        });
        target.canvas().draw_image(
            &self.rendered.as_ref().unwrap().image,
            as_skpoint(&location.location),
            Some(&paint),
        );
    }
    fn render_meta_box_contents(
        &mut self,
        _location: &RenderBlockLocation,
        storage: &mut BlockManager,
        target: &mut Surface,
    ) {
        let RenderBlockDescription::MetaBox(mb) = &self.wire_description else {
            panic!("Render meta box should not be called with a description that is not a meta box")
        };
        // How do we sort the blocks?
        let mut blocks =
            SmallVec::<[(RenderBlockLocation); 64]>::with_capacity(mb.sub_blocks.len());
        blocks.extend(mb.sub_blocks.iter().map(|b| b.clone()));
        //blocks.extend(mb.sub_blocks.iter().map(|b| (b.id, b.layer, b.location)));
        blocks.sort_by(|a, b| a.layer.cmp(&b.layer));
        for location in blocks {
            let block = storage.blocks.get_mut(&location.id);
            if block.as_ref().map(|b| b.is_some()).unwrap_or(false) {
                let mut moved_block = block.unwrap().take().unwrap();
                /* The bloc is temporary moved out of the storage, so storage can be passed on as mutable */
                moved_block.render(&location, storage, target);
                // Put the block back
                let post_block = storage.blocks.get_mut(&location.id);
                let post_block_inner = post_block.unwrap();
                *post_block_inner = Some(moved_block);
            }
        }
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
