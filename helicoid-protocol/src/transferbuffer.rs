use std::{collections::BTreeMap, mem::size_of, sync::Arc};

use ahash::HashMap;
use anyhow::Result;
use itertools::assert_equal;
use rkyv::{
    ser::{
        serializers::{AllocScratch, CompositeSerializer, WriteSerializer},
        ScratchSpace, Serializer,
    },
    Archive,
};
use smallvec::SmallVec;

use crate::{
    gfx::{
        HelicoidToClientMessage, NewRenderBlock, RemoteSingleChange, RenderBlockId,
        RenderBlockLocation, RenderBlockPath, RenderBlockRemoveInstruction,
    },
    tcp_bridge::{DummyWriter, SerializeWith, TcpBridgeToClientMessage, TcpBridgeToServerMessage},
};

/* Buffer that contains and reorganizes buffers to be transferred to the client */
/* TODO: Use rkyv types where possible */
#[derive(Default, Debug)]
pub struct TransferBuffer {
    removals: BTreeMap<RenderBlockPath, Vec<RenderBlockId>>,
    additions: BTreeMap<RenderBlockPath, Vec<NewRenderBlock>>,
    moves: BTreeMap<RenderBlockPath, Vec<RenderBlockLocation>>,
}

impl SerializeWith for TransferBuffer {
    fn serialize<R: Serializer + ScratchSpace, D: Serializer + ScratchSpace>(
        &self,
        serializer: &mut R,
        dummy_serializer: &mut D,
    ) -> Result<usize, ()> {
        TransferBuffer::serialize(self, serializer, dummy_serializer)
    }
}
impl SerializeWith for Arc<TransferBuffer> {
    fn serialize<R: Serializer + ScratchSpace, D: Serializer + ScratchSpace>(
        &self,
        serializer: &mut R,
        dummy_serializer: &mut D,
    ) -> Result<usize, ()> {
        TransferBuffer::serialize(self, serializer, dummy_serializer)
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

    fn serialize<R: Serializer + ScratchSpace, D: Serializer + ScratchSpace>(
        &self,
        serializer: &mut R,
        dummy_serializer: &mut D,
    ) -> Result<usize, ()> {
        let mut size = 0usize;
        log::trace!("Serialize transfer buffer start");
        /* Removals */
        for (path, removals) in self.removals.iter().rev() {
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
            let before_pos = dummy_serializer.pos();
            let _dummy_root_pos = dummy_serializer
                .serialize_value(&removal)
                .map_err(|_e| ())?;
            serializer
                .write(&u32::to_le_bytes(
                    (dummy_serializer.pos() - before_pos) as u32,
                ))
                .map_err(|_e| ())?;
            let _root_pos = serializer.serialize_value(&removal).map_err(|_e| ())?;
            size += dummy_serializer.pos() - before_pos;
        }
        /* Additions */
        for (path, additions) in self.additions.iter() {
            let addition = TcpBridgeToClientMessage {
                message: HelicoidToClientMessage {
                    updates: vec![RemoteSingleChange {
                        parent: path.clone(),
                        change: crate::gfx::RemoteSingleChangeElement::NewRenderBlocks(
                            additions.iter().map(|b| b.clone()).collect(),
                        ),
                    }],
                },
            };
            let before_pos = dummy_serializer.pos();
            let _dummy_root_pos = dummy_serializer
                .serialize_value(&addition)
                .map_err(|_e| ())?;
            serializer
                .write(&u32::to_le_bytes(
                    (dummy_serializer.pos() - before_pos) as u32,
                ))
                .map_err(|_e| ())?;
            let _root_pos = serializer.serialize_value(&addition).map_err(|_e| ())?;
            size += dummy_serializer.pos() - before_pos;
        }
        /* Moves */
        for (path, moves) in self.moves.iter() {
            let movement = TcpBridgeToClientMessage {
                message: HelicoidToClientMessage {
                    updates: vec![RemoteSingleChange {
                        parent: path.clone(),
                        change: crate::gfx::RemoteSingleChangeElement::MoveBlockLocations(
                            moves.iter().map(|b| b.clone()).collect(),
                        ),
                    }],
                },
            };
            let before_pos = dummy_serializer.pos();
            let _dummy_root_pos = dummy_serializer
                .serialize_value(&movement)
                .map_err(|_e| ())?;
            serializer
                .write(&u32::to_le_bytes(
                    (dummy_serializer.pos() - before_pos) as u32,
                ))
                .map_err(|_e| ())?;
            let _root_pos = serializer.serialize_value(&movement).map_err(|_e| ())?;
            size += dummy_serializer.pos() - before_pos;
        }
        log::trace!("Serialize transfer buffer end, size:{}", size);
        Ok(size)
    }
}
