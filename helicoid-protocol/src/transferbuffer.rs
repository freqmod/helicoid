use ahash::HashMap;
use anyhow::Result;
use rkyv::ser::{ScratchSpace, Serializer};
use smallvec::SmallVec;

use crate::{
    gfx::{
        HelicoidToClientMessage, NewRenderBlock, RemoteSingleChange, RenderBlockId,
        RenderBlockLocation, RenderBlockPath, RenderBlockRemoveInstruction,
    },
    tcp_bridge::{SerializeWith, TcpBridgeToClientMessage, TcpBridgeToServerMessage},
};

/* Buffer that contains and reorganizes buffers to be transferred to the client */
/* TODO: Use rkyv types where possible */
#[derive(Default, Debug)]
pub struct TransferBuffer {
    removals: HashMap<RenderBlockPath, Vec<RenderBlockId>>,
    additions: HashMap<RenderBlockPath, Vec<NewRenderBlock>>,
    moves: HashMap<RenderBlockPath, Vec<RenderBlockLocation>>,
}

impl SerializeWith for TransferBuffer {
    fn serialize<R: Serializer + ScratchSpace>(&mut self, serializer: &mut R) -> Result<usize, ()> {
        TransferBuffer::serialize(self, serializer)
    }
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

    pub fn add_moves(&mut self, path: &RenderBlockPath, mv: &[RenderBlockLocation]) {
        let path_entry = self.moves.entry(path.clone()).or_insert(Vec::new());
        path_entry.extend_from_slice(mv);
    }

    pub fn add_moves_from_iter<I: Iterator<Item = RenderBlockLocation>>(
        &mut self,
        path: &RenderBlockPath,
        mv: I,
    ) {
        let path_entry = self.moves.entry(path.clone()).or_insert(Vec::new());
        path_entry.extend(mv);
    }
    pub fn add_removes(&mut self, path: &RenderBlockPath, rmv: &[RenderBlockId]) {
        let path_entry = self.removals.entry(path.clone()).or_insert(Vec::new());
        path_entry.extend_from_slice(rmv);
    }
    pub fn add_removes_from_iter<I: Iterator<Item = RenderBlockId>>(
        &mut self,
        path: &RenderBlockPath,
        rmv: I,
    ) {
        let path_entry = self.removals.entry(path.clone()).or_insert(Vec::new());
        path_entry.extend(rmv);
    }

    pub fn add_news(&mut self, path: &RenderBlockPath, new: &[NewRenderBlock]) {
        let path_entry = self.additions.entry(path.clone()).or_insert(Vec::new());
        path_entry.extend_from_slice(new);
    }

    pub fn add_news_from_iter<I: Iterator<Item = NewRenderBlock>>(
        &mut self,
        path: &RenderBlockPath,
        new: I,
    ) {
        let path_entry = self.additions.entry(path.clone()).or_insert(Vec::new());
        path_entry.extend(new);
    }

    pub fn serialize<S: Serializer + ScratchSpace>(
        &mut self,
        serializer: &mut S,
    ) -> Result<usize, ()> {
        let mut size = 0usize;
        /* Removals */
        for (path, removals) in self.removals.iter_mut() {
            let removal = TcpBridgeToClientMessage {
                message: HelicoidToClientMessage {
                    updates: vec![RemoteSingleChange {
                        parent: path.clone(),
                        change: crate::gfx::RemoteSingleChangeElement::RemoveRenderBlocks(
                            SmallVec::from_iter(removals.iter().map(|id| {
                                RenderBlockRemoveInstruction {
                                    offset: id.clone(),
                                    mask: RenderBlockId(0),
                                }
                            })),
                        ),
                    }],
                },
            };
            size += serializer.serialize_value(&removal).map_err(|_e| ())?;
        }
        /* Additions */
        for (path, additions) in self.additions.iter_mut() {
            let removal = TcpBridgeToClientMessage {
                message: HelicoidToClientMessage {
                    updates: vec![RemoteSingleChange {
                        parent: path.clone(),
                        change: crate::gfx::RemoteSingleChangeElement::NewRenderBlocks(
                            additions.iter().map(|b| b.clone()).collect(),
                        ),
                    }],
                },
            };
            size += serializer.serialize_value(&removal).map_err(|_e| ())?;
        }
        /* Moves */
        for (path, moves) in self.moves.iter_mut() {
            let removal = TcpBridgeToClientMessage {
                message: HelicoidToClientMessage {
                    updates: vec![RemoteSingleChange {
                        parent: path.clone(),
                        change: crate::gfx::RemoteSingleChangeElement::MoveBlockLocations(
                            moves.iter().map(|b| b.clone()).collect(),
                        ),
                    }],
                },
            };
            size += serializer.serialize_value(&removal).map_err(|_e| ())?;
        }

        Ok(size)
    }
}
