use rkyv::ser::ScratchSpace;
use rkyv::ser::{serializers::AllocSerializer, Serializer};
use rkyv::{AlignedVec, Archive, Deserialize, Serialize};

use std::io::Write;

use std::marker::PhantomData;

use crate::gfx::HelicoidToClientMessage;
use crate::input::HelicoidToServerMessage;

use anyhow::Result;
use bytecheck::CheckBytes;

pub type TBSSerializer = AllocSerializer<0x4000>;

#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct TcpBridgeToClientMessage {
    pub message: HelicoidToClientMessage,
}
#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize)]
#[archive_attr(derive(Debug))]
pub struct TcpBridgeToServerMessage {
    pub message: HelicoidToServerMessage,
}

pub trait SerializeWith {
    fn serialize<R: Serializer + ScratchSpace, D: Serializer + ScratchSpace>(
        &self,
        serializer: &mut R,
        dummy_serializer: &mut D,
    ) -> Result<usize, ()>;
}
#[derive(Debug, Default)]
pub struct DummyWriter {}

impl Write for DummyWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl SerializeWith for TcpBridgeToClientMessage {
    fn serialize<R: Serializer + ScratchSpace, D: Serializer + ScratchSpace>(
        &self,
        serializer: &mut R,
        dummy_serializer: &mut D,
    ) -> Result<usize, ()> {
        let dummy_start_pos = dummy_serializer.serialize_value(self).map_err(|_e| ())?;
        serializer
            .write(&u32::to_le_bytes(
                (dummy_serializer.pos() - dummy_start_pos) as u32,
            ))
            .map_err(|_e| ())?;

        serializer.serialize_value(self).map_err(|_e| ())
    }
}

impl SerializeWith for TcpBridgeToServerMessage {
    fn serialize<R: Serializer + ScratchSpace, D: Serializer + ScratchSpace>(
        &self,
        serializer: &mut R,
        dummy_serializer: &mut D,
    ) -> Result<usize, ()> {
        let dummy_start_pos = dummy_serializer.serialize_value(self).map_err(|_e| ())?;
        serializer
            .write(&u32::to_le_bytes(
                (dummy_serializer.pos() - dummy_start_pos) as u32,
            ))
            .map_err(|_e| ())?;
        serializer.serialize_value(&self.message).map_err(|_e| ())
    }
}

const PACKET_HEADER_LENGTH: usize = 4;
const PACKET_HEADER_ADJUST: usize = 16 - PACKET_HEADER_LENGTH;

pub enum TcpBridgeReceiveState {
    WaitingForHeader,
    WaitingForHeaderRead,
    WaitingForContents,
    WaitingForContentsRead,
    WaitingForExtract,
}

impl Default for TcpBridgeReceiveState {
    fn default() -> Self {
        TcpBridgeReceiveState::WaitingForHeader
    }
}

/* TODO: Currently this function will result in quite a lot of (small) reads and possibly memcpys,
which may be worthwile to optimize */
#[derive(Default)]
pub struct TcpBridgeReceiveProcessor<M> {
    recv_buffer: AlignedVec,
    message_type: PhantomData<M>,
    state: TcpBridgeReceiveState,
    current_offset: usize, // NB: There are some invariants between state and current_offset
}

impl<M: Archive> TcpBridgeReceiveProcessor<M>
where
    M::Archived: Deserialize<M, rkyv::Infallible>,
{
    pub fn new() -> Self {
        Self {
            recv_buffer: Default::default(),
            message_type: PhantomData,
            state: Default::default(),
            current_offset: Default::default(),
        }
    }
    pub fn next_read_buffer(&mut self) -> Option<&mut [u8]> {
        match self.state {
            TcpBridgeReceiveState::WaitingForHeader => {
                debug_assert!(self.current_offset <= PACKET_HEADER_LENGTH);
                self.state = TcpBridgeReceiveState::WaitingForHeaderRead;
                Some(self.offsetted_message_buffer(
                    self.current_offset,
                    PACKET_HEADER_LENGTH - self.current_offset,
                ))
            }
            TcpBridgeReceiveState::WaitingForContents => {
                debug_assert!(self.current_offset >= PACKET_HEADER_LENGTH);
                let packet_contents_length = self.packet_length();
                debug_assert!(self.current_offset < packet_contents_length + PACKET_HEADER_LENGTH);
                self.state = TcpBridgeReceiveState::WaitingForContentsRead;
                Some(self.offsetted_message_buffer(
                    self.current_offset,
                    packet_contents_length + PACKET_HEADER_LENGTH - self.current_offset,
                ))
            }
            TcpBridgeReceiveState::WaitingForExtract
            | TcpBridgeReceiveState::WaitingForHeaderRead
            | TcpBridgeReceiveState::WaitingForContentsRead => None,
        }
    }
    pub fn mark_data_read(&mut self, read_length: usize) {
        debug_assert!(
            PACKET_HEADER_ADJUST + self.current_offset + read_length <= self.recv_buffer.len()
        );
        match self.state {
            TcpBridgeReceiveState::WaitingForHeaderRead => {
                self.current_offset += read_length;
                self.state = if self.current_offset >= PACKET_HEADER_LENGTH {
                    TcpBridgeReceiveState::WaitingForContents
                } else {
                    TcpBridgeReceiveState::WaitingForHeader
                };
            }
            TcpBridgeReceiveState::WaitingForContentsRead => {
                self.current_offset += read_length;
                let packet_length = self.packet_length();
                self.state = if self.current_offset >= PACKET_HEADER_LENGTH + packet_length {
                    TcpBridgeReceiveState::WaitingForExtract
                } else {
                    TcpBridgeReceiveState::WaitingForContents
                };
            }
            TcpBridgeReceiveState::WaitingForHeader
            | TcpBridgeReceiveState::WaitingForExtract
            | TcpBridgeReceiveState::WaitingForContents => {}
        }
    }
    fn offsetted_message_buffer(&mut self, local_offset: usize, length: usize) -> &mut [u8] {
        let slice_start = PACKET_HEADER_ADJUST + local_offset;
        let min_length = slice_start + length;
        self.recv_buffer.resize(min_length, 0);
        &mut self.recv_buffer[slice_start..(slice_start + length)]
    }
    fn packet_length(&self) -> usize {
        assert!(match self.state {
            TcpBridgeReceiveState::WaitingForContents
            | TcpBridgeReceiveState::WaitingForContentsRead
            | TcpBridgeReceiveState::WaitingForExtract => true,
            TcpBridgeReceiveState::WaitingForHeader
            | TcpBridgeReceiveState::WaitingForHeaderRead => false,
        });
        u32::from_le_bytes(
            self.recv_buffer[PACKET_HEADER_ADJUST..(PACKET_HEADER_ADJUST + PACKET_HEADER_LENGTH)]
                .try_into()
                .unwrap(),
        ) as usize
    }

    /* Returns true if a partial read in in progress
       (e.g. that the header of a packet or a partial packet is read),
    false if waiting for a new element / packet */
    pub fn partial_read(&self) -> bool {
        self.current_offset > 0
    }
    pub fn transform_element(buffer: &[u8]) -> Option<M> {
        let archived = unsafe { rkyv::archived_root::<M>(buffer) };
        Deserialize::<M, _>::deserialize(archived, &mut rkyv::Infallible).ok()
    }
    pub fn extract_archive(&mut self) -> Option<M> {
        match self.state {
            TcpBridgeReceiveState::WaitingForExtract => {
                let packet_length = self.packet_length();
                debug_assert!(
                    PACKET_HEADER_ADJUST + PACKET_HEADER_LENGTH + packet_length
                        <= self.recv_buffer.len()
                );
                debug_assert!(PACKET_HEADER_LENGTH + packet_length <= self.current_offset);
                let element_data =
                    self.offsetted_message_buffer(PACKET_HEADER_LENGTH, packet_length);
                let result = Self::transform_element(element_data);
                let current_element_end =
                    PACKET_HEADER_ADJUST + PACKET_HEADER_LENGTH + packet_length;
                let current_read_data_end = PACKET_HEADER_ADJUST + self.current_offset;
                if current_read_data_end - current_element_end > 0 {
                    self.recv_buffer.copy_within(
                        current_element_end..current_read_data_end,
                        PACKET_HEADER_ADJUST,
                    );
                }
                self.current_offset -= PACKET_HEADER_LENGTH + packet_length;
                self.state = if self.current_offset > PACKET_HEADER_LENGTH {
                    TcpBridgeReceiveState::WaitingForContents
                } else {
                    TcpBridgeReceiveState::WaitingForHeader
                };
                result
            }
            _ => None,
        }
    }
}
