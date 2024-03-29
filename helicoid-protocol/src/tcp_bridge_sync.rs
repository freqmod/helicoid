use anyhow::Result;

use rkyv::Deserialize;
use rkyv::{Archive, Infallible};
use std::io::{Read, Write};
use std::{marker::PhantomData, net::TcpStream};

use rkyv::ser::serializers::{
    AlignedSerializer, AllocScratch, CompositeSerializer, WriteSerializer,
};

use crate::bridge_logic::SerializeWith;
use crate::bridge_logic::{DummyWriter, TBSSerializer, TcpBridgeReceiveProcessor};

pub struct TcpBridgeSend<M> {
    tcp_conn: TcpStream,
    serializer: Option<TBSSerializer>,
    dummy_serializer:
        Option<CompositeSerializer<WriteSerializer<DummyWriter>, AllocScratch, Infallible>>,
    send_type: PhantomData<M>,
}
pub struct TcpBridgeReceive<M> {
    tcp_conn: TcpStream,
    processor: TcpBridgeReceiveProcessor<M>,
}

pub fn connect_to_server<R, S>(
    addr: &String,
) -> Result<(TcpBridgeReceive<R>, TcpBridgeSend<S>), std::io::Error>
where
    R: Archive,
    R::Archived: Deserialize<R, rkyv::Infallible>,
    S: SerializeWith,
{
    let stream = TcpStream::connect(addr)?;
    let r = stream.try_clone()?;
    let w = stream;
    let send = TcpBridgeSend::new(w)?;
    let receive = TcpBridgeReceive::new(r)?;
    Ok((receive, send))
}

pub enum TcpBridgeReceiveError {
    IoError(std::io::Error),
    NoMoreData,
}
pub enum TcpBridgeSendError {
    IoError(std::io::Error),
}
impl<M: Archive> TcpBridgeReceive<M>
where
    M::Archived: Deserialize<M, rkyv::Infallible>,
{
    pub fn new(reader: TcpStream) -> Result<Self, std::io::Error> {
        reader.set_nonblocking(true)?;
        Ok(Self {
            tcp_conn: reader,
            processor: TcpBridgeReceiveProcessor::new(),
        })
    }
    pub fn try_receive(&mut self) -> Result<M, TcpBridgeReceiveError> {
        loop {
            if let Some(buffer) = self.processor.next_read_buffer() {
                let num_read = match self.tcp_conn.read(buffer) {
                    Ok(num_read) => num_read,
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        self.processor.mark_data_read(0);
                        log::trace!("Read would block");
                        return Err(TcpBridgeReceiveError::NoMoreData);
                    }
                    Err(e) => return Err(TcpBridgeReceiveError::IoError(e)),
                };
                self.processor.mark_data_read(num_read);
                if num_read == 0 {
                    break;
                }
                if let Some(archive) = self.processor.extract_archive() {
                    return Ok(archive);
                }
            }
        }
        Err(TcpBridgeReceiveError::NoMoreData)
    }
}

impl<M> TcpBridgeSend<M>
where
    M: SerializeWith,
{
    pub fn new(writer: TcpStream) -> Result<Self, std::io::Error> {
        //writer.set_nonblocking(true)?;
        Ok(Self {
            tcp_conn: writer,
            serializer: Default::default(),
            dummy_serializer: Default::default(),
            send_type: PhantomData,
        })
    }
    pub fn send(&mut self, message: &mut M) -> Result<(), TcpBridgeSendError> {
        let mut serializer = self.serializer.take().unwrap();
        let mut dummy_serializer = self.dummy_serializer.take().unwrap();
        message
            .serialize(&mut serializer, &mut dummy_serializer)
            .unwrap();
        let (inner_serializer, scratch, shared) = serializer.into_components();
        let mut bytes = inner_serializer.into_inner();
        log::trace!("Tcp bridge Sending {} bytes ({:?})", bytes.len(), bytes);
        match self.tcp_conn.write(&bytes) {
            Ok(written) => {
                assert!(written == bytes.len());
            }
            Err(e) => {
                return Err(TcpBridgeSendError::IoError(e));
            }
        }
        bytes.clear();
        self.serializer = Some(TBSSerializer::new(
            AlignedSerializer::new(bytes),
            scratch,
            shared,
        ));
        self.dummy_serializer = Some(dummy_serializer);

        Ok(())
    }
}
