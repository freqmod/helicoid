use ahash::HashMap;
use anyhow::Result;
use rkyv::ser::{ScratchSpace, Serializer};
use smallvec::SmallVec;

use crate::{
    gfx::{
        HelicoidToClientMessage, NewRenderBlock, RemoteSingleChange, RenderBlockId,
        RenderBlockLocation, RenderBlockPath, RenderBlockRemoveInstruction,
    },
    tcp_bridge::{TcpBridgeToClientMessage, TcpBridgeToServerMessage},
};

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
    pub fn async_write<S: Serializer + ScratchSpace>(&mut self, serializer: &mut S) -> Result<()> {
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
            serializer
                .serialize_value(&removal)
                .map_err(|_e| format!("Helicoid serialization error"))
                .unwrap();
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
            serializer
                .serialize_value(&removal)
                .map_err(|_e| format!("Helicoid serialization error"))
                .unwrap();
        }
        /* Additions */
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
            serializer
                .serialize_value(&removal)
                .map_err(|_e| format!("Helicoid serialization error"))
                .unwrap();
        }

        /* Moves */
        Ok(())
    }
}
