use std::{collections::HashMap, sync::Arc};

use helicoid_protocol::gfx::RemoteBoxUpdate;
use parking_lot::Mutex;
use skia_safe::{
    gpu::{DirectContext, SurfaceOrigin},
    Budgeted, Canvas, ISize, ImageInfo, Surface, SurfaceProps, SurfacePropsFlags,
};

#[derive(Default, Debug, Hash, Eq, Clone, Copy, PartialEq)]
pub struct SurfaceCacheKey {
    pub width: u16,
    pub height: u16,
}

pub struct SurfaceCache {
    context: DirectContext,
    parent_image_info: ImageInfo,
    cache: HashMap<SurfaceCacheKey, Vec<Surface>>,
}

pub type SharedSurfaceCache = Arc<Mutex<SurfaceCache>>;
impl SurfaceCache {
    pub fn new(parent_surface: &mut Surface) -> Option<Self> {
        let mut rec_context = parent_surface.recording_context()?;
        let direct_context = rec_context.as_direct_context()?;
        let parent_image_info = parent_surface.image_info();
        Some(Self {
            context: direct_context,
            parent_image_info,
            cache: Default::default(),
        })
    }
    fn allocate_surface(&mut self, params: SurfaceCacheKey) -> Surface {
        let image_info = ImageInfo::new(
            ISize {
                width: params.width as i32,
                height: params.height as i32,
            },
            self.parent_image_info.color_type(),
            self.parent_image_info.alpha_type(),
            self.parent_image_info.color_space(),
        );
        build_sub_surface(&mut self.context, image_info)
    }
    pub fn get_surface(&mut self, params: SurfaceCacheKey) -> Surface {
        if let Some(cached_surfaces) = self.cache.get_mut(&params) {
            //            if !cached_surfaces.is_empty(){
            if let Some(surface_from_cache) = cached_surfaces.pop() {
                return surface_from_cache;
            }
        }
        /* Allocate surface and return that */
        self.allocate_surface(params)
    }
    /// Insert a surface into the cache. If successful return None, if unsuccessful,
    /// because the surface is not using the same context as the cache was initialised with,
    /// the surface is returned.
    pub fn put_surface(&mut self, mut surface: Surface) -> Option<Surface> {
        let key = SurfaceCacheKey {
            width: surface.width() as u16,
            height: surface.height() as u16,
        };
        /* Validate that the provided surface uses the same rendering context as the cache */
        if surface
            .recording_context()
            .map(|mut c| {
                c.as_direct_context()
                    .map(|dc| dc.id().eq(&self.context.id()))
            })
            .flatten()
            .unwrap_or(false)
        {
            if !self.cache.contains_key(&key) {
                /* Insert vector for key to add surface to */
                self.cache.insert(key, Vec::with_capacity(1));
            }
            let cached_surfaces = self.cache.get_mut(&key).unwrap();
            cached_surfaces.push(surface);
            None
        } else {
            Some(surface)
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

pub struct RemoteBoxRenderer {
    surface_cache: SharedSurfaceCache,
    rendered_surfaces: HashMap<u16, Surface>,
    //bridge: TcpBridge,
    dimensions: ISize,
}
impl RemoteBoxRenderer {
    pub fn new(dimensions: ISize, surface_cache: SharedSurfaceCache) -> Self {
        Self {
            surface_cache,
            rendered_surfaces: Default::default(),
            dimensions,
        }
    }
    pub fn update_contents(&mut self, update: &RemoteBoxUpdate) {
        let mut cache = self.surface_cache.lock();
        for block in update.remove_render_blocks.iter() {
            unimplemented!();
            /*            if let Some(entry) = self.rendered_surfaces.remove(block) {
                cache.put_surface(entry);
            }*/
        }
        /* Render new blocks */
        for block in update.new_render_blocks.iter() {}
    }
    pub fn draw_box(&mut self, root_canvas: &mut Canvas) -> bool {
        false
    }
}
