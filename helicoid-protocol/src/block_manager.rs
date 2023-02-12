use crate::gfx::PointF16;
use crate::gfx::RemoteBoxUpdate;
use crate::gfx::RenderBlockDescription;
use crate::gfx::RenderBlockId;
use crate::gfx::RenderBlockLocation;
use std::collections::HashMap;
use std::hash::Hash;
use std::hash::Hasher;

/* Ideally we want clear ownership of render blocks, like having a set of top level blocks, and then
add/ remove those wilth all their descendents. */

/* This file contains renderer agnostic render block logic for keeping track of a (client side)
tree of render blocks */
pub trait BlockGfx {}
pub trait ManagerGfx<B: BlockGfx> {
    fn create_gfx_block(&mut self, wire_description: &RenderBlockDescription, parent: RenderBlockId, id: RenderBlockId) -> B;
}

pub struct Block<G: BlockGfx> {
    render_info: G,
    wire_description: RenderBlockDescription,
    parent: RenderBlockId,
    id: RenderBlockId,
}

pub struct Manager<BG: BlockGfx> {
    layers: Vec<Option<Vec<RenderBlockId>>>,
    /* The blocks are options so they can be moved out while rendering to enable the manager to be
    passed mutable for sub-blocks */
    blocks: HashMap<RenderBlockId, Option<Block<BG>>>,
    top_level_block: RenderBlockId,
}

//MG: ManagerGfx<BG>
impl<BG: BlockGfx> Manager<BG> {
    pub fn new() -> Self {
        Self {
            layers: Default::default(),
            blocks: Default::default(),
            top_level_block: 1, /* TODO: The server have to specify this more properly? */
        }
    }

    pub fn handle_block_update<MG: ManagerGfx<BG>>(
        &mut self,
        update: &RemoteBoxUpdate,
        gfx_manager: &mut MG,
    ) {
        for block in update.new_render_blocks.iter() {
            log::trace!("Update render block: {}", block.id);
            //let parent = 0; /* Set parent to 0 as this is sent as a top level block? */
            let new_rendered_block = Block::new(
                block.contents.clone(),
                gfx_manager.create_gfx_block(&block.contents, block.parent, block.id),
                block.parent,
                block.id,
            );
            if let Some(render_block) = self.blocks.get_mut(&block.id) {
                *render_block = Some(new_rendered_block);
            } else {
                self.blocks.insert(block.id, Some(new_rendered_block));
            }
        }
    }
}

impl<BG: BlockGfx> Block<BG> {
    pub fn new(desc: RenderBlockDescription, render_info: BG, id: RenderBlockId, parent: RenderBlockId) -> Self {
        Self {
            render_info,
            wire_description: desc,
            parent,
            id
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
}
