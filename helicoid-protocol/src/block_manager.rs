use crate::gfx::PointF16;
use crate::gfx::RemoteBoxUpdate;
use crate::gfx::RenderBlockDescription;
use crate::gfx::RenderBlockId;
use crate::gfx::RenderBlockLocation;
use crate::gfx::RenderBlockPath;
use std::collections::HashMap;
use std::hash::Hash;
use std::hash::Hasher;

/* Ideally we want clear ownership of render blocks, like having a set of top level blocks, and then
add/ remove those wilth all their descendents. */

/* This file contains renderer agnostic render block logic for keeping track of a (client side)
tree of render blocks */
pub trait BlockGfx {}
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
    fn remove_blocks(&mut self, mask_id: RenderBlockId, base_id: RenderBlockId);
    fn move_blocks(
        &mut self,
        mask_id: RenderBlockId,
        dst_mask_id: RenderBlockId,
        base_id: RenderBlockId,
    );
    // Not sure if the ability to add blocks should be part of this interface
}

struct InteriorBlockContainer<G: BlockGfx> {
    path: RenderBlockPath,
    blocks: HashMap<RenderBlockId, Option<Block<G>>>,
    layers: Vec<Option<Vec<RenderBlockId>>>,
}
pub struct Block<G: BlockGfx> {
    render_info: G,
    id: RenderBlockId,
    parent_path: RenderBlockPath,
    wire_description: RenderBlockDescription,
    container: InteriorBlockContainer<G>,
}

pub struct Manager<BG: BlockGfx> {
    /* The blocks are options so they can be moved out while rendering to enable the manager to be
    passed mutable for sub-blocks */
    //    blocks: HashMap<RenderBlockId, Option<Block<BG>>>,
    top_level_block: RenderBlockId,
    container: InteriorBlockContainer<BG>,
}

impl<G: BlockGfx> InteriorBlockContainer<G> {
    pub fn new(path: RenderBlockPath) -> Self {
        Self {
            layers: Default::default(),
            blocks: Default::default(),
            path,
        }
    }
}
//MG: ManagerGfx<BG>
impl<BG: BlockGfx> Manager<BG> {
    pub fn new() -> Self {
        Self {
            top_level_block: RenderBlockId::normal(1).unwrap(), /* TODO: The server have to specify this more properly? */
            container: InteriorBlockContainer::new(RenderBlockPath::top()),
        }
    }

    pub fn handle_block_update<MG: ManagerGfx<BG>>(
        &mut self,
        update: &RemoteBoxUpdate,
        gfx_manager: &mut MG,
    ) {
        for block in update.new_render_blocks.iter() {
            log::trace!("Update render block: {:?}", block.id);
            //let parent = 0; /* Set parent to 0 as this is sent as a top level block? */
            let new_rendered_block = Block::new(
                block.contents.clone(),
                gfx_manager.create_gfx_block(&block.contents, update.parent.clone(), block.id),
                block.id,
                update.parent.clone(),
            );
            if let Some(render_block) = self.container.block_mut(block.id) {
                *render_block = new_rendered_block;
            } else {
                /* TODO: Replace unwrap with proper error handling */
                self.container
                    .add_block(block.id, new_rendered_block)
                    .unwrap();
            }
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
            wire_description: desc,
            parent_path,
            id,
        }
    }

    pub fn hash_block_recursively<H: Hasher>(
        &self,
        storage: &Manager<BG>,
        //location: &RenderBlockLocation,
        hasher: &mut H,
    ) {
        match self.wire_description {
            RenderBlockDescription::MetaBox(_) => self.hash_meta_box_recursively(storage, hasher),
            _ => self.wire_description.hash(hasher),
        }
    }

    pub fn hash_meta_box_recursively<H: Hasher>(&self, storage: &Manager<BG>, hasher: &mut H) {
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
    pub fn as_container(&self) -> Option<&dyn BlockContainer<BG>> {
        None
    }
    pub fn as_container_mut(&mut self) -> Option<&mut dyn BlockContainer<BG>> {
        None
    }
    pub fn parent_path(&self) -> &RenderBlockPath {
        &self.parent_path
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
        self.blocks.get(&id).map(|b| b.as_ref()).flatten()
    }

    fn block_mut(&mut self, id: RenderBlockId) -> Option<&mut Block<BG>> {
        self.blocks.get_mut(&id).map(|b| b.as_mut()).flatten()
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
