use rkyv::ser::ScratchSpace;
use rkyv::ser::{serializers::AllocSerializer, Serializer};
use rkyv::{Archive, Deserialize, Infallible, Serialize};
use std::{
    marker::PhantomData,
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        mpsc::{Receiver, Sender},
        Arc,
    },
};

use hashbrown::HashMap;
use rkyv::ser::serializers::{AllocScratch, CompositeSerializer, WriteSerializer};

use crate::bridge_logic::SerializeWith;
use crate::{
    bridge_logic::{
        DummyWriter, TBSSerializer, TcpBridgeReceiveProcessor, TcpBridgeToClientMessage,
        TcpBridgeToServerMessage,
    },
    transferbuffer::TransferBuffer,
};

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

pub struct ClientTcpBridge {
    send: TcpBridgeSend<TcpBridgeToServerMessage>,
    receive: TcpBridgeReceive<TcpBridgeToClientMessage>,
}

pub struct TcpBridgeServer<S> {
    _listener: Option<TcpListener>,
    _connections: HashMap<SocketAddr, TcpBridgeServerConnection<S>>,
}

#[allow(dead_code)]
pub struct TcpBridgeServerConnection<S> {
    bridge: ServerSingleTcpBridge,
    connection_state: S,
}

pub struct ServerSingleTcpBridge {
    send: TcpBridgeSend<Arc<TransferBuffer>>,
    receive: TcpBridgeReceive<TcpBridgeToServerMessage>,
}
/*
pub trait TcpBridgeServerConnectionState: Send {
    type StateData: Send + 'static;
    async fn new_state(
        peer_address: SocketAddr,
        channel_tx: Sender<Arc<TransferBuffer>>,
        channel_rx: Receiver<TcpBridgeToServerMessage>,
        close_rx: BReceiver<()>,
        state_data: Self::StateData,
    ) -> Self;
    async fn initialize(&mut self) -> Result<()>;
    async fn event_loop(&mut self) -> Result<()>;
}
*/
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
impl<M: Archive> TcpBridgeReceive<M>
where
    M::Archived: Deserialize<M, rkyv::Infallible>,
{
    fn new(r: TcpStream) -> Result<Self, std::io::Error> {
        todo!()
    }
}
impl<M> TcpBridgeSend<M>
where
    M: SerializeWith,
{
    fn new(writer: TcpStream) -> Result<Self, std::io::Error> {
        todo!()
    }
}
