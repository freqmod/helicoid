use crate::gfx::BlockLayer;
use crate::gfx::PointF16;
use crate::gfx::RemoteBoxUpdate;
use crate::gfx::RenderBlockDescription;
use crate::gfx::RenderBlockId;
use crate::gfx::RenderBlockLocation;
use crate::gfx::RenderBlockPath;
use std::collections::HashMap;
use std::hash::Hash;
use std::hash::Hasher;
use std::marker::PhantomData;

/* Ideally we want clear ownership of render blocks, like having a set of top level blocks, and then
add/ remove those wilth all their descendents. */

pub type ChangeGeneration = u8;

/* This file contains renderer agnostic render block logic for keeping track of a (client side)
tree of render blocks */
pub trait BlockGfx: std::fmt::Debug {
    //    fn hash(); // consider if we can use ownership to mark stuff as dirty
    //    fn render();
}

pub trait ManagerGfx<B: BlockGfx> {
    fn create_gfx_block(
        &mut self,
        wire_description: &RenderBlockDescription,
        parent_path: RenderBlockPath,
        id: RenderBlockId,
    ) -> B;
}

pub trait BlockContainer<G: BlockGfx> {
    fn path(&self) -> &RenderBlockPath;
    fn add_block(&self, id: RenderBlockId, block: Block<G>) -> anyhow::Result<()>;
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
    // Not sure if the ability to add blocks should be part of this interface
}

#[derive(Debug)]
struct ContainerBlock<G: BlockGfx> {
    layer: BlockLayer,
    block: Option<Block<G>>,
    hash: u64,
    last_changed: ChangeGeneration,
}

#[derive(Debug)]
pub struct InteriorBlockContainer<G: BlockGfx> {
    path: RenderBlockPath,
    blocks: HashMap<RenderBlockId, ContainerBlock<G>>,
    layers: HashMap<BlockLayer, Vec<(RenderBlockId, PointF16)>>,
}

#[derive(Debug, Hash, Eq, Clone, PartialEq)]
pub struct RenderBlockFullId {
    pub id: RenderBlockId,
    pub parent_path: RenderBlockPath,
}

#[derive(Debug)]
pub struct MetaBlock<G: BlockGfx> {
    id: RenderBlockFullId,
    wire_description: RenderBlockDescription,
    container: Option<InteriorBlockContainer<G>>,
    gfx_type: PhantomData<G>,
}
#[derive(Debug)]
pub struct Block<G: BlockGfx> {
    render_info: G,
    meta: MetaBlock<G>,
}

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
            path,
        }
    }
    pub fn update_location(&mut self, new_location: &RenderBlockLocation) {
        let Some(cblock) = self.blocks.get_mut(&new_location.id) else {
            log::warn!("Tried to update location on a non existing block container: {:?} update: {:?}", self, new_location);
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
            /* If the block update is for a top level block */
            for block in update.new_render_blocks.iter() {
                log::trace!("Update render block: {:?}", block.id);
                //let parent = 0; /* Set parent to 0 as this is sent as a top level block? */
                let new_rendered_block = Block::new(
                    block.contents.clone(),
                    gfx_manager.create_gfx_block(&block.contents, update.parent.clone(), block.id),
                    block.id,
                    update.parent.clone(),
                );
                if let Some(render_block) = self.containers.get_mut(&block.id) {
                    //                *render_block = new_rendered_block;
                    *render_block = new_rendered_block; //.handle_block_update(update, gfx_manager);
                } else {
                    /* TODO: Replace unwrap with proper error handling */
                    assert!(self
                        .containers
                        .insert(block.id, new_rendered_block)
                        .is_none());
                }
            }
            for update in update.remove_render_blocks.iter() {
                //                if let Some(render_block) = self.containers.get_mut(&block.id) {
                assert!(update.mask.0 == 0xFF);
                self.containers.remove(&update.offset);
            }
        } else {
            /* If the block update has a parent, find the parent and forward the update */
            if let Some(parent_block) = self.block_for_path_mut(client_id, &update.parent) {
                parent_block.handle_block_update(update, gfx_manager);
            }
        }
    }
    pub fn block_for_path_mut(
        &mut self,
        id: RenderBlockId,
        path: &RenderBlockPath,
    ) -> Option<&mut Block<BG>> {
        if let Some(render_block) = self.containers.get_mut(&id) {
            path.resolve_block_mut(render_block)
        } else {
            None
        }
    }
}

impl<BG: BlockGfx> ContainerBlock<BG> {
    pub fn new(block: Block<BG>, layer: BlockLayer) -> Self {
        Self {
            layer: 0,
            block: Some(block),
            hash: 0,
            last_changed: 0,
        }
    }
}

impl<BG: BlockGfx> Block<BG> {
    pub fn new(
        desc: RenderBlockDescription,
        render_info: BG,
        id: RenderBlockId,
        parent_path: RenderBlockPath,
    ) -> Self {
        Self {
            render_info,
            meta: MetaBlock {
                wire_description: desc,
                id: RenderBlockFullId { id, parent_path },
                container: None,
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
        let Some(container) = self.meta.container.as_mut() else{
            log::debug!("Trying to send RemoteBoxUpdate to a block that isn't a container");
            return;
        };
        for instruction in update.remove_render_blocks.iter() {
            container.remove_blocks(instruction.mask, instruction.offset);
        }
        for block in update.new_render_blocks.iter() {
            let new_rendered_block = Block::new(
                block.contents.clone(),
                gfx_manager.create_gfx_block(&block.contents, update.parent.clone(), block.id),
                block.id,
                update.parent.clone(),
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
        match self.wire_description {
            RenderBlockDescription::MetaBox(_) => self.hash_meta_box_recursively(hasher),
            _ => self.wire_description.hash(hasher),
        }
    }

    pub fn hash_meta_box_recursively<H: Hasher>(&self, hasher: &mut H) {
        let RenderBlockDescription::MetaBox(mb) = &self.wire_description else {
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
    pub fn as_container(&self) -> Option<&dyn BlockContainer<BG>> {
        None
    }
    pub fn as_container_mut(&mut self) -> Option<&mut dyn BlockContainer<BG>> {
        None
    }
    pub fn parent_path(&self) -> &RenderBlockPath {
        &self.id.parent_path
    }
    pub fn wire_description(&self) -> &RenderBlockDescription {
        &self.wire_description
    }
    pub fn destruct_mut(
        &mut self,
    ) -> (
        &mut RenderBlockDescription,
        &mut Option<InteriorBlockContainer<BG>>,
    ) {
        (&mut self.wire_description, &mut self.container)
    }
}

impl<BG: BlockGfx> BlockContainer<BG> for InteriorBlockContainer<BG> {
    fn path(&self) -> &RenderBlockPath {
        &self.path
    }

    fn add_block(&self, id: RenderBlockId, block: Block<BG>) -> anyhow::Result<()> {
        todo!()
    }

    fn block(&self, id: RenderBlockId) -> Option<&Block<BG>> {
        self.blocks.get(&id).map(|b| b.block.as_ref()).flatten()
    }

    fn block_mut(&mut self, id: RenderBlockId) -> Option<&mut Block<BG>> {
        self.blocks.get_mut(&id).map(|b| b.block.as_mut()).flatten()
    }

    fn block_ref_mut(&mut self, id: RenderBlockId) -> Option<&mut Option<Block<BG>>> {
        self.blocks.get_mut(&id).map(|b| &mut b.block)
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
}

impl<BG: BlockGfx> BlockContainer<BG> for Manager<BG> {
    fn path(&self) -> &RenderBlockPath {
        &self.path
    }

    fn add_block(&self, id: RenderBlockId, block: Block<BG>) -> anyhow::Result<()> {
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
}
