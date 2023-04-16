use smallvec::SmallVec;

use crate::gfx::BlockLayer;
use crate::gfx::PointF16;
use crate::gfx::RemoteBoxUpdate;
use crate::gfx::RenderBlockDescription;
use crate::gfx::RenderBlockId;
use crate::gfx::RenderBlockLocation;
use crate::gfx::RenderBlockPath;
use crate::gfx::SimpleDrawElement;
use hashbrown::HashMap;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

pub struct BlockRenderParents<'a, 'b, 'p, G: BlockGfx> {
    pub parent: Option<&'a mut BlockRenderParents<'p, 'p, 'p, G>>,
    pub gfx_block: &'b mut G,
}

/* Ideally we want clear ownership of render blocks, like having a set of top level blocks, and then
add/ remove those wilth all their descendents. */

pub type ChangeGeneration = u8;

/* This file contains renderer agnostic render block logic for keeping track of a (client side)
tree of render blocks */
pub trait BlockGfx: std::fmt::Debug + Sized {
    type RenderTarget<'a>
    where
        Self: 'a;
    //    fn hash(); // consider if we can use ownership to mark stuff as dirty
    //    fn render();
    fn render<'b>(
        &mut self,
        location: &RenderBlockLocation,
        block: &mut MetaBlock<Self>,
        target: &mut Self::RenderTarget<'b>,
    );
}

pub trait ManagerGfx<B: BlockGfx> {
    fn create_gfx_block(
        &mut self,
        wire_description: &RenderBlockDescription,
        parent_path: RenderBlockPath,
        id: RenderBlockId,
    ) -> B;
    fn create_top_block(&mut self, id: RenderBlockId) -> B;
}

pub trait BlockContainer<G: BlockGfx>: std::fmt::Debug {
    fn path(&self) -> &RenderBlockPath;
    fn add_block(&mut self, id: RenderBlockId, block: Block<G>) -> anyhow::Result<()>;
    fn block(&self, id: RenderBlockId) -> Option<&Block<G>>;
    fn block_mut(&mut self, id: RenderBlockId) -> Option<&mut Block<G>>;
    fn block_ref_mut(&mut self, id: RenderBlockId) -> Option<&mut Option<Block<G>>>;
    fn remove_blocks(&mut self, mask_id: RenderBlockId, base_id: RenderBlockId);
    fn move_blocks(
        &mut self,
        mask_id: RenderBlockId,
        dst_mask_id: RenderBlockId,
        base_id: RenderBlockId,
    );
    fn log_block_tree(&self, depth: usize);
    // Not sure if the ability to add blocks should be part of this interface
}

#[derive(Debug)]
struct ContainerBlock<G: BlockGfx> {
    layer: BlockLayer,
    //location: PointF16,
    block: Option<Block<G>>,
    hash: u64,
    last_changed: ChangeGeneration,
}

#[derive(Debug)]
pub struct InteriorBlockContainer<G: BlockGfx> {
    path: RenderBlockPath,
    blocks: HashMap<RenderBlockId, ContainerBlock<G>>,
    layers: HashMap<BlockLayer, Vec<(RenderBlockId, PointF16)>>,
    /* Used when iterating trough sublayers. Declare it here to avoid repeated heap allications */
    sorted_layers_tmp: SmallVec<[BlockLayer; 16]>,
}

#[derive(Debug, Hash, Eq, Clone, PartialEq)]
pub struct RenderBlockFullId {
    pub id: RenderBlockId,
    pub parent_path: RenderBlockPath,
}

#[derive(Debug)]
pub struct MetaBlock<G: BlockGfx> {
    id: RenderBlockFullId,
    wire_description: Option<RenderBlockDescription>,
    container: Option<InteriorBlockContainer<G>>,
    gfx_type: PhantomData<G>,
}
#[derive(Debug)]
pub struct Block<G: BlockGfx> {
    render_info: G,
    meta: MetaBlock<G>,
}

#[derive(Debug)]
pub struct Manager<BG: BlockGfx> {
    /* The blocks are options so they can be moved out while rendering to enable the manager to be
    passed mutable for sub-blocks */
    //    blocks: HashMap<RenderBlockId, Option<Block<BG>>>,
    containers: HashMap<RenderBlockId, Block<BG>>,
    path: RenderBlockPath,
}

impl<G: BlockGfx> InteriorBlockContainer<G> {
    pub fn new(path: RenderBlockPath) -> Self {
        Self {
            layers: Default::default(),
            blocks: Default::default(),
            sorted_layers_tmp: Default::default(),
            path,
        }
    }
    pub fn update_location(&mut self, new_location: &RenderBlockLocation) {
        log::debug!(
            "Updated location on a block container: {:?} <- update: {:?}",
            self.path(),
            new_location.id
        );
        let Some(cblock) = self.blocks.get_mut(&new_location.id) else {
            log::warn!("Tried to update location on a non existing block container: {:?} <- update: {:?}", self.path(), new_location.id);
            return;
        };

        if cblock.layer != new_location.layer {
            /* Remove from old layer */
            if let Some(old_layer) = self.layers.get_mut(&cblock.layer) {
                if let Some(old_block_idx) =
                    old_layer.iter().enumerate().find_map(|(idx, (id, _))| {
                        if *id == new_location.id {
                            Some(idx)
                        } else {
                            None
                        }
                    })
                {
                    old_layer.swap_remove(old_block_idx);
                }
            }
            /* Add to new layer */
            self.layers
                .entry(new_location.layer)
                .or_insert(Default::default())
                .push((new_location.id, new_location.location));
        } else {
            /* Change the location in the layer in place, if it exists */
            if let Some(current_layer) = self.layers.get_mut(&cblock.layer) {
                if let Some(current_block_idx) =
                    current_layer.iter().enumerate().find_map(|(idx, (id, _))| {
                        if *id == new_location.id {
                            Some(idx)
                        } else {
                            None
                        }
                    })
                {
                    current_layer[current_block_idx].1 = new_location.location;
                }
            }
        }
        cblock.layer = new_location.layer;
        //        cblock.location = new_location.location;
    }
    fn container_block_mut(&mut self, id: RenderBlockId) -> Option<&mut ContainerBlock<G>> {
        self.blocks.get_mut(&id).map(|b| {
            b.last_changed = b.last_changed.wrapping_add(1);
            b
        })
    }
}
//MG: ManagerGfx<BG>
impl<BG: BlockGfx> Manager<BG> {
    pub fn new() -> Self {
        Self {
            //top_level_block: RenderBlockId::normal(1).unwrap(), /* TODO: The server have to specify this more properly? */
            containers: Default::default(), //InteriorBlockContainer::new(RenderBlockPath::top()),
            path: RenderBlockPath::top(),
        }
    }

    pub fn handle_block_update<MG: ManagerGfx<BG>>(
        &mut self,
        client_id: RenderBlockId,
        update: &RemoteBoxUpdate,
        gfx_manager: &mut MG,
    ) {
        if update.parent.path().is_empty() {
            let mgr_entry = self.containers.entry(client_id).or_insert_with(|| {
                log::trace!("Make container entry for: {:?}", client_id);
                Block::new(
                    None,
                    gfx_manager.create_top_block(client_id),
                    client_id,
                    RenderBlockPath::top(),
                    Some(InteriorBlockContainer::new(RenderBlockPath::top())),
                )
            });

            for update in update.remove_render_blocks.iter() {
                //                if let Some(render_block) = self.containers.get_mut(&block.id) {
                mgr_entry
                    .meta
                    .container
                    .as_mut()
                    .unwrap()
                    .remove_blocks(update.mask, update.offset);
            }
            /* If the block update is for a top level block */
            for block in update.new_render_blocks.iter() {
                log::trace!("Update new render block: {:?}", block.id);
                //let parent = 0; /* Set parent to 0 as this is sent as a top level block? */
                let container = match block.contents {
                    RenderBlockDescription::ShapedTextBlock(_) => None,
                    RenderBlockDescription::SimpleDraw(_) => None,
                    RenderBlockDescription::MetaBox(_) => Some(InteriorBlockContainer::new(
                        RenderBlockPath::child(&update.parent, block.id),
                    )),
                };
                let new_rendered_block = Block::new(
                    Some(block.contents.clone()),
                    gfx_manager.create_gfx_block(&block.contents, update.parent.clone(), block.id),
                    block.id,
                    update.parent.clone(),
                    container,
                );
                log::trace!("Render block to add: {:?}", new_rendered_block);
                mgr_entry
                    .meta
                    .container
                    .as_mut()
                    .unwrap()
                    .add_block(block.id, new_rendered_block)
                    .unwrap();
                /*                if let Some(render_block) =
                    mgr_entry.meta.container.unwrap().blocks.get_mut(&block.id)
                {
                    //                *render_block = new_rendered_block;
                    *render_block = new_rendered_block; //.handle_block_update(update, gfx_manager);
                } else {
                    /* TODO: Replace unwrap with proper error handling */
                    /*                    assert!(self
                    .containers
                    .insert(block.id, new_rendered_block)
                    .is_none());*/
                }*/
            }

            for new_location in update.move_block_locations.iter() {
                mgr_entry
                    .meta
                    .container
                    .as_mut()
                    .unwrap()
                    .update_location(new_location);
            }
        } else {
            /* If the block update has a parent, find the parent and forward the update */
            if let Some(child_block) = self.block_for_path_mut(client_id, &update.parent) {
                child_block.handle_block_update(update, gfx_manager);
            } else {
                log::debug!(
                    "Could not get block for path: {:?} {:?}",
                    client_id,
                    update.parent
                )
            }
        }
    }
    pub fn log_block_tree(&self, client_id: RenderBlockId) {
        let mgr_entry = self.containers.get(&client_id).unwrap();
        mgr_entry.meta.container.as_ref().unwrap().log_block_tree(0);
    }
    pub fn block_for_path_mut(
        &mut self,
        id: RenderBlockId,
        path: &RenderBlockPath,
    ) -> Option<&mut Block<BG>> {
        if let Some(render_block) = self.containers.get_mut(&id) {
            log::trace!("Resolve {:?} in {:?}", path, render_block);
            path.resolve_block_mut(render_block)
        } else {
            log::debug!("No container for client id: {:?}", id);
            None
        }
    }

    pub fn process_blocks_for_client<'t>(
        &mut self,
        client_id: RenderBlockId,
        target: &mut BG::RenderTarget<'t>,
    ) {
        if let Some(render_block) = self.containers.get_mut(&client_id) {
            log::trace!("Process blocks recursively for client id: {:?}", client_id);
            let (mb, bg) = render_block.destruct_mut();
            mb.process_block_recursively(bg, target);
        } else {
            log::trace!("Could not find any block to process for: {:?}", client_id);
        }
    }
}

impl<BG: BlockGfx> ContainerBlock<BG> {
    pub fn new(block: Block<BG>, layer: BlockLayer) -> Self {
        Self {
            layer,
            //location,
            block: Some(block),
            hash: 0,
            last_changed: 0,
        }
    }
}

impl<BG: BlockGfx> Block<BG> {
    pub fn new(
        desc: Option<RenderBlockDescription>,
        render_info: BG,
        id: RenderBlockId,
        parent_path: RenderBlockPath,
        container: Option<InteriorBlockContainer<BG>>,
    ) -> Self {
        Self {
            render_info,
            meta: MetaBlock {
                wire_description: desc,
                id: RenderBlockFullId { id, parent_path },
                container,
                gfx_type: PhantomData::<BG>,
            },
        }
    }
    pub fn meta(&self) -> &MetaBlock<BG> {
        &self.meta
    }
    pub fn meta_mut(&mut self) -> &mut MetaBlock<BG> {
        &mut self.meta
    }
    pub fn render_info(&self) -> &BG {
        &self.render_info
    }
    pub fn render_info_mut(&mut self) -> &mut BG {
        &mut self.render_info
    }
    pub fn destruct(&self) -> (&MetaBlock<BG>, &BG) {
        let Self { render_info, meta } = self;
        (meta, render_info)
    }
    pub fn destruct_mut(&mut self) -> (&mut MetaBlock<BG>, &mut BG) {
        let Self { render_info, meta } = self;
        (meta, render_info)
    }
    pub fn handle_block_update<MG: ManagerGfx<BG>>(
        &mut self,
        update: &RemoteBoxUpdate,
        gfx_manager: &mut MG,
    ) {
        log::trace!(
            "Block: Handle update: {:?}, {:?}",
            self.meta.parent_path(),
            self.meta.id
        );
        let Some(container) = self.meta.container.as_mut() else{
            log::debug!("Trying to send RemoteBoxUpdate to a block that isn't a container");
            return;
        };
        for instruction in update.remove_render_blocks.iter() {
            container.remove_blocks(instruction.mask, instruction.offset);
        }
        for block in update.new_render_blocks.iter() {
            let new_block_container = match block.contents {
                RenderBlockDescription::ShapedTextBlock(_) => None,
                RenderBlockDescription::SimpleDraw(_) => None,
                RenderBlockDescription::MetaBox(_) => Some(InteriorBlockContainer::new(
                    RenderBlockPath::child(&update.parent, block.id),
                )),
            };
            let new_rendered_block = Block::new(
                Some(block.contents.clone()),
                gfx_manager.create_gfx_block(&block.contents, update.parent.clone(), block.id),
                block.id,
                update.parent.clone(),
                new_block_container,
            );
            /* TODO: Replace unwrap with proper error handling */
            container.add_block(block.id, new_rendered_block).unwrap();
        }
        for new_location in update.move_block_locations.iter() {
            container.update_location(new_location);
        }
    }
}

impl<BG: BlockGfx> MetaBlock<BG> {
    pub fn hash_block_recursively<H: Hasher>(&self, hasher: &mut H) {
        if let Some(wire_description) = self.wire_description.as_ref() {
            match wire_description {
                RenderBlockDescription::MetaBox(_) => self.hash_meta_box_recursively(hasher),
                _ => self.wire_description.hash(hasher),
            }
        }
    }

    pub fn hash_meta_box_recursively<H: Hasher>(&self, hasher: &mut H) {
        let Some(RenderBlockDescription::MetaBox(mb)) = &self.wire_description else {
            panic!("Hash meta box should not be called with a description that is not a meta box")
        };
        mb.hash(hasher);
        for block in mb.sub_blocks.iter() {
            let render_block = self.container.as_ref().map(|c| c.block(block.id)).flatten();
            if render_block.is_some() {
                let extracted_block = render_block.unwrap();
                extracted_block.meta().hash_block_recursively(hasher);
            } else {
                false.hash(hasher);
            }
        }
    }

    pub fn contains_blur(&self) -> bool {
        match self.wire_description.as_ref() {
            Some(RenderBlockDescription::MetaBox(mb)) => {
                for block in mb.sub_blocks.iter() {
                    let render_block = self.container.as_ref().map(|c| c.block(block.id)).flatten();
                    if render_block.is_some() {
                        let extracted_block = render_block.unwrap();
                        if extracted_block.meta().contains_blur() {
                            return true;
                        }
                    }
                }
            }
            Some(RenderBlockDescription::SimpleDraw(sd)) => {
                if sd.draw_elements.iter().any(|e| match e {
                    SimpleDrawElement::Fill(f) => f.paint.background_blur_amount() > 0f32,
                    SimpleDrawElement::RoundRect(rr) => rr.paint.background_blur_amount() > 0f32,
                    _ => false,
                }) {
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    pub fn process_block_recursively<'a, 't, 'p>(
        &mut self,
        _parent_gfx: &'a mut BG,
        target: &mut BG::RenderTarget<'t>,
    ) {
        log::trace!("Rendering block: {:?} ", self.id);
        let container = self
            .container
            .as_mut()
            .expect("Expecting block to have container if wire description has children");
        container.sorted_layers_tmp.clear();
        container.sorted_layers_tmp.reserve(container.layers.len());
        container
            .sorted_layers_tmp
            .extend(container.layers.keys().map(|k| *k));
        container.sorted_layers_tmp.sort();
        //        let layer_ids = container.layers.iter().;
        //        for (_layer_id, layer_blocks) in container.layers.iter() {
        for layer_id in container.sorted_layers_tmp.iter() {
            let layer_blocks = container.layers.get_mut(layer_id).unwrap();
            log::trace!(
                "Render layer: {}: {:?}",
                layer_id,
                layer_blocks.iter().map(|(b, _)| b.clone())
            );
            for (block_id, location) in layer_blocks.iter() {
                let block_id = block_id.clone();
                let container_block = container.blocks.get_mut(&block_id);
                if let Some(container_block) = container_block {
                    let mut moved_block = container_block.block.take().unwrap();
                    /* The block is temporary moved out of the storage, so storage can be passed on as mutable */
                    let (block, gfx) = moved_block.destruct_mut();
                    let location = RenderBlockLocation {
                        id: block_id,
                        location: location.clone(),
                        layer: container_block.layer,
                    };
                    gfx.render(&location, block, target);
                    // Put the block back
                    container_block.block = Some(moved_block);
                }
            }
        }
    }

    pub fn as_container(&self) -> Option<&dyn BlockContainer<BG>> {
        self.container
            .as_ref()
            .map(|c| c as &dyn BlockContainer<BG>)
    }
    pub fn as_container_mut(&mut self) -> Option<&mut dyn BlockContainer<BG>> {
        self.container
            .as_mut()
            .map(|c| c as &mut dyn BlockContainer<BG>)
    }
    pub fn parent_path(&self) -> &RenderBlockPath {
        &self.id.parent_path
    }
    pub fn id(&self) -> &RenderBlockId {
        &self.id.id
    }
    pub fn wire_description(&self) -> &Option<RenderBlockDescription> {
        &self.wire_description
    }
    pub fn destruct_mut(
        &mut self,
    ) -> (
        &mut Option<RenderBlockDescription>,
        &mut Option<InteriorBlockContainer<BG>>,
    ) {
        (&mut self.wire_description, &mut self.container)
    }
}

impl<BG: BlockGfx> BlockContainer<BG> for InteriorBlockContainer<BG> {
    fn path(&self) -> &RenderBlockPath {
        &self.path
    }

    fn add_block(&mut self, id: RenderBlockId, block: Block<BG>) -> anyhow::Result<()> {
        log::trace!("Adding block {:?} <- {:?}", self.path, id);
        let container_block = ContainerBlock::new(block, 0);
        if self.blocks.insert(id, container_block).is_none() {
            log::trace!("Adding to layer");
            /* TODO: Consider remove: Should we require a new location before displaying or display
            items at a default location by this (which is slightly inefficent in the most likely case)*/
            self.layers
                .entry(0)
                .or_insert(Default::default())
                .push((id, PointF16::new(0f32, 0f32)));
            Ok(())
        } else {
            /* TODO: Make a better, more structured error return */
            log::trace!(
                "Trying to add block when it already is present: {:?} {:?}",
                self.path,
                id
            );
            Ok(())
        }
    }

    fn block(&self, id: RenderBlockId) -> Option<&Block<BG>> {
        self.blocks.get(&id).map(|b| b.block.as_ref()).flatten()
    }

    fn block_mut(&mut self, id: RenderBlockId) -> Option<&mut Block<BG>> {
        self.blocks
            .get_mut(&id)
            .map(|b| {
                b.last_changed = b.last_changed.wrapping_add(1);
                b.block.as_mut()
            })
            .flatten()
    }

    fn block_ref_mut(&mut self, id: RenderBlockId) -> Option<&mut Option<Block<BG>>> {
        self.blocks.get_mut(&id).map(|b| {
            b.last_changed = b.last_changed.wrapping_add(1);

            &mut b.block
        })
    }

    fn remove_blocks(&mut self, mask_id: RenderBlockId, base_id: RenderBlockId) {
        todo!()
    }

    fn move_blocks(
        &mut self,
        mask_id: RenderBlockId,
        dst_mask_id: RenderBlockId,
        base_id: RenderBlockId,
    ) {
        todo!()
    }
    fn log_block_tree(&self, depth: usize) {
        for (id, block) in self.blocks.iter() {
            log::debug!("{}{:4x}", "  ".repeat(depth), id.0);
            if let Some(ref block) = block.block {
                if let Some(ref container) = block.meta().container {
                    container.log_block_tree(depth + 1);
                }
            }
        }
    }
}

impl<BG: BlockGfx> BlockContainer<BG> for Manager<BG> {
    fn path(&self) -> &RenderBlockPath {
        &self.path
    }

    fn add_block(&mut self, id: RenderBlockId, block: Block<BG>) -> anyhow::Result<()> {
        todo!()
    }

    fn block(&self, id: RenderBlockId) -> Option<&Block<BG>> {
        self.containers.get(&id)
    }

    fn block_mut(&mut self, id: RenderBlockId) -> Option<&mut Block<BG>> {
        self.containers.get_mut(&id)
    }

    fn block_ref_mut(&mut self, id: RenderBlockId) -> Option<&mut Option<Block<BG>>> {
        None /* Not supported for (top level) block manager */
    }

    fn remove_blocks(&mut self, mask_id: RenderBlockId, base_id: RenderBlockId) {
        todo!()
    }

    fn move_blocks(
        &mut self,
        mask_id: RenderBlockId,
        dst_mask_id: RenderBlockId,
        base_id: RenderBlockId,
    ) {
        todo!()
    }

    fn log_block_tree(&self, depth: usize) {
        todo!()
    }
}
