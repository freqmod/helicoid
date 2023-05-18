use ahash::HashMap;

use crate::gfx::{NewRenderBlock, RenderBlockId, RenderBlockLocation, RenderBlockPath};

/* Buffer that contains and reorganizes buffers to be transferred to the client */
/* TODO: Use rkyv types where possible */
#[derive(Default)]
pub struct TransferBuffer {
    removals: HashMap<RenderBlockPath, Vec<RenderBlockId>>,
    additions: HashMap<RenderBlockPath, Vec<NewRenderBlock>>,
    moves: HashMap<RenderBlockPath, Vec<RenderBlockLocation>>,
}

impl TransferBuffer {
    pub fn new() -> Self {
        Default::default()
    }
    /* Clears the contents of the transfer buffer while trying to keep the memory */
    pub fn clear(&mut self) {
        for (_path, removals) in self.removals.iter_mut() {
            removals.clear();
        }
        for (_path, additions) in self.additions.iter_mut() {
            additions.clear();
        }
        for (_path, moves) in self.moves.iter_mut() {
            moves.clear();
        }
    }
}
