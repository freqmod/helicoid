use async_trait::async_trait;
use rkyv::ser::serializers::{
    AlignedSerializer, AllocScratch, CompositeSerializer, WriteSerializer,
};
use rkyv::ser::ScratchSpace;
use rkyv::ser::{serializers::AllocSerializer, Serializer};
use rkyv::{AlignedVec, Archive, Deserialize, Infallible, Serialize};
use std::collections::HashMap;
use std::io::Write;

use std::marker::PhantomData;
use std::net::SocketAddr;
use std::sync::Arc;

use crate::gfx::HelicoidToClientMessage;
use crate::input::HelicoidToServerMessage;
use crate::transferbuffer::TransferBuffer;
use anyhow::{anyhow, Result};
use bytecheck::CheckBytes;
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{
    broadcast::{self, Receiver as BReceiver, Sender as BSender},
    mpsc::{self, Receiver, Sender},
    oneshot::{self, Receiver as OReceiver, Sender as OSender},
    Mutex as TMutex,
};

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
#[repr(C, align(16))]
struct AlignedBuffer {
    contents: [u8; 0x10000],
}
enum ReadResult {
    GotPacket,
    NoData,
    StopReading,
}
const PACKET_HEADER_LENGTH: usize = 4;
const PACKET_HEADER_ADJUST: usize = 16 - PACKET_HEADER_LENGTH;

#[derive(Default)]
struct TcpBridgeReceiveProcessor<M> {
    recv_buffer: AlignedVec,
    message_type: PhantomData<M>,
}

impl<M: Archive> TcpBridgeReceiveProcessor<M>
where
    M::Archived: Deserialize<M, rkyv::Infallible>,
{
    pub fn new() -> Self {
        Default::default()
    }
    pub fn next_buffer_read(&mut self) -> &mut [u8] {
        &[]
    }
    /* Returns true if a partial read in in progress
       (e.g. that the header of a packet or a partial packet is read),
    false if waiting for a new element / packet */
    pub fn partial_read(&self) -> bool {}
    pub fn pop_element(&mut self) -> Option<M> {
        None
    }
}
