/* Used to talk to a helix / helicoid backend over a relyable TCP connection
managed by Tokio, and connected to the user interface by channels */

use async_trait::async_trait;
use rkyv::ser::serializers::{
    AlignedSerializer, AllocScratch, CompositeSerializer, WriteSerializer,
};
use rkyv::ser::ScratchSpace;
use rkyv::ser::{serializers::AllocSerializer, Serializer};
use rkyv::{Archive, Deserialize, Infallible, Serialize};
use std::collections::HashMap;
use std::io::Write;

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

pub struct TcpBridgeSend<M> {
    tcp_conn: OwnedWriteHalf,
    serializer: Option<TBSSerializer>,
    dummy_serializer:
        Option<CompositeSerializer<WriteSerializer<DummyWriter>, AllocScratch, Infallible>>,
    chan: Receiver<M>,
    close_chan: OReceiver<()>,
}
pub struct TcpBridgeReceive<M> {
    tcp_conn: OwnedReadHalf,
    chan: Sender<M>,
    close_chan: Option<OSender<()>>,
}
pub struct ClientTcpBridge {
    send: TcpBridgeSend<TcpBridgeToServerMessage>,
    receive: TcpBridgeReceive<TcpBridgeToClientMessage>,
}

pub struct TcpBridgeServer<S> {
    close_sender: BSender<()>,
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

#[async_trait]
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

impl ClientTcpBridge {
    pub async fn connect(
        addr: &String,
    ) -> Result<(
        Self,
        Sender<TcpBridgeToServerMessage>,
        Receiver<TcpBridgeToClientMessage>,
    )> {
        //        Ok(Self{})
        //        unimplemented!()
        let stream = TcpStream::connect(addr).await?;
        let (r, w) = stream.into_split();
        let (cs, cr) = oneshot::channel();
        let (send, send_channel) = TcpBridgeSend::new(w, cr)?;
        let (receive, receive_channel) = TcpBridgeReceive::new(r, cs)?;
        Ok((Self { send, receive }, send_channel, receive_channel))
    }
    pub async fn process_rxtx(&mut self) -> Result<()> {
        let ClientTcpBridge { send, receive } = self;
        let send_proc_fut = send.process();
        let recv_proc_fut = receive.process();
        let (send_proc_res, rec_proc_res) = tokio::join!(send_proc_fut, recv_proc_fut);
        send_proc_res?;
        rec_proc_res?;
        log::trace!("TcpCB: Processrxtx complete");
        Ok(())
        // Need to call process on send and on receive
    }
}
impl ServerSingleTcpBridge {
    pub fn handle_connection(
        stream: TcpStream,
    ) -> Result<(
        Self,
        Sender<Arc<TransferBuffer>>,
        Receiver<TcpBridgeToServerMessage>,
    )> {
        let (r, w) = stream.into_split();
        let (cs, cr) = oneshot::channel();
        let (send, send_channel) = TcpBridgeSend::new(w, cr)?;
        let (receive, receive_channel) = TcpBridgeReceive::new(r, cs)?;
        Ok((Self { send, receive }, send_channel, receive_channel))
    }
    pub async fn process_rxtx(&mut self) -> Result<()> {
        let ServerSingleTcpBridge { send, receive } = self;
        let send_proc_fut = send.process();
        let recv_proc_fut = receive.process();
        let (send_proc_res, rec_proc_res) = tokio::join!(send_proc_fut, recv_proc_fut);
        send_proc_res?;
        rec_proc_res?;
        Ok(())
        // Need to call process on send and on receive
    }
}

impl<S: TcpBridgeServerConnectionState> TcpBridgeServer<S> {
    pub async fn new() -> Result<Self> {
        let (close_sender, _) = broadcast::channel(1);

        Ok(Self {
            _listener: None,
            _connections: Default::default(),
            close_sender,
        })
    }

    async fn establish_connection(
        stream: TcpStream,
        peer_addr: SocketAddr,
        close_receiver: BReceiver<()>,
        state_data: S::StateData,
    ) -> Result<()> {
        //let local_address = socket.local_addr()?;
        log::trace!("Handle connection");
        let (mut bridge, channel_tx, channel_rx) =
            ServerSingleTcpBridge::handle_connection(stream).unwrap();
        tokio::spawn(async move { bridge.process_rxtx().await.unwrap() });
        let mut connection_state = S::new_state(
            peer_addr,
            channel_tx,
            channel_rx,
            close_receiver,
            state_data,
        )
        .await;
        log::trace!("Initialize connection");
        connection_state.initialize().await?;
        log::trace!("Connection intialized, run connection event loop");
        connection_state.event_loop().await?;
        log::trace!("Connection event loop completed");
        Ok(())
    }
    pub async fn wait_for_connection(
        this: Arc<TMutex<Self>>,
        addr: &String,
        state_data: S::StateData,
    ) -> Result<()> {
        let addr_spawn = addr.clone();
        let listener = TcpListener::bind(&addr).await?;
        // Asynchronously wait for an inbound socket.
        log::trace!("Waiting for connection, bound {}", addr_spawn);
        let (socket, peer_addr) = listener.accept().await?;
        log::trace!("Waiting for connection, accepted {}", addr_spawn);
        let close_receiver = {
            let this_locked = this.lock().await;
            this_locked.close_sender.subscribe()
        };
        log::trace!("Waiting for connection, got close channel {}", addr_spawn);
        tokio::spawn(async move {
            log::trace!("Waiting for connection on {}", addr_spawn);
            match Self::establish_connection(socket, peer_addr, close_receiver, state_data).await {
                Ok(_) => {
                    log::trace!("Establish connection returned");
                }
                Err(e) => {
                    log::warn!("Got error while processing connection: {:?}", e)
                }
            }
        });
        Ok(())
    }
}
impl<S> Drop for TcpBridgeServer<S> {
    fn drop(&mut self) {
        let _ = self.close_sender.send(());
    }
}

type TBSSerializer = AllocSerializer<0x4000>;
impl<M> TcpBridgeSend<M>
where
    M: SerializeWith,
{
    fn new(writer: OwnedWriteHalf, close_chan: OReceiver<()>) -> Result<(Self, Sender<M>)> {
        let (tx, rx) = mpsc::channel(32);
        let serializer = Some(TBSSerializer::default());
        let dummy_serializer = Some(CompositeSerializer::new(
            WriteSerializer::new(DummyWriter::default()),
            AllocScratch::new(),
            Default::default(),
        ));

        Ok((
            Self {
                tcp_conn: writer,
                serializer,
                dummy_serializer,
                chan: rx,
                close_chan,
            },
            tx,
        ))
    }
    pub async fn process(&mut self) -> Result<()> {
        loop {
            tokio::select! {
                received = self.chan.recv() => {
                    match received {
                        Some(message) => {
                            let mut serializer = self.serializer.take().unwrap();
                            let mut dummy_serializer = self.dummy_serializer.take().unwrap();
                            message.serialize(&mut serializer, &mut dummy_serializer).unwrap();
                            let (inner_serializer, scratch, shared) = serializer.into_components();
                            let mut bytes = inner_serializer.into_inner();
                            log::trace!("Tcp bridge Sending {} bytes ({:?})", bytes.len(), bytes);
                            self.tcp_conn.write(&bytes).await?;
                            bytes.clear();
                            self.serializer = Some(TBSSerializer::new(AlignedSerializer::new(bytes), scratch, shared));
                            self.dummy_serializer = Some(dummy_serializer);
                        }
                        None => {
                            break;
                        }
                    }
                },
                _ = &mut self.close_chan => {
                    break;
                }
            }
        }
        Ok(())
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
impl<M: Archive> TcpBridgeReceive<M>
where
    M::Archived: Deserialize<M, rkyv::Infallible>,
{
    fn new(reader: OwnedReadHalf, close_chan: OSender<()>) -> Result<(Self, Receiver<M>)> {
        let (tx, rx) = mpsc::channel(32);
        Ok((
            Self {
                tcp_conn: reader,
                chan: tx,
                close_chan: Some(close_chan),
            },
            rx,
        ))
    }
    /* Returns true if interations should continue */
    async fn try_read(
        &mut self,
        buffer: &mut [u8],
        pkg_offset: &mut usize,
        pkg_len: &mut usize,
        buffer_filled: &mut usize,
    ) -> Result<ReadResult> {
        let data_read;
        if *pkg_len == u32::MAX as usize {
            /* Read u32 length prefix */
            log::trace!("Try read");
            data_read = match self.tcp_conn.try_read(buffer) {
                Ok(0) => {
                    return Ok(ReadResult::StopReading);
                }
                Ok(n) => n,
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    log::trace!("Read would block");
                    return Ok(ReadResult::NoData);
                }
                Err(e) => return Err(e.into()),
            };

            if data_read == 0 {
                log::warn!("No data received: {}", data_read);
                return Ok(ReadResult::NoData);
            }
            if data_read < PACKET_HEADER_LENGTH {
                log::warn!("Unexpectedly small data received: {}", data_read);
                return Ok(ReadResult::StopReading);
            }
            *pkg_len =
                u32::from_le_bytes(buffer[0..(PACKET_HEADER_LENGTH)].try_into().unwrap()) as usize;
            debug_assert!(*pkg_len + PACKET_HEADER_LENGTH <= buffer.len());
            *pkg_offset = PACKET_HEADER_LENGTH;
            *buffer_filled = 0;
        } else {
            log::trace!(
                "Read: {}..{}+{}-{} (of {})",
                buffer_filled,
                pkg_len,
                buffer_filled,
                PACKET_HEADER_LENGTH,
                buffer.len()
            );
            data_read = self.tcp_conn.try_read(
                &mut buffer[*buffer_filled..*pkg_len + *buffer_filled - PACKET_HEADER_LENGTH],
            )?;
        }
        log::trace!("Received event data: {}", pkg_len);
        *buffer_filled += data_read;

        while *buffer_filled >= PACKET_HEADER_LENGTH + *pkg_len {
            /* All the required data was transferred */
            let data_sliced = &buffer[*pkg_offset..(*pkg_len + *pkg_offset)];

            //println!("Data slized ptr; 0x{:X}", data_sliced.as_ptr() as usize);
            //let msg = rkyv::from_bytes::<HelicoidToClientMessage>(data_sliced)
            //    .map_err(|_| anyhow!("Error while deserializing message from wire"))?;
            let archived = unsafe { rkyv::archived_root::<M>(data_sliced) };
            // TODO: Does the deserialized type copy or reference the archived memory (currently we assume copy)
            let deserialized =
                Deserialize::<M, _>::deserialize(archived, &mut rkyv::Infallible).unwrap();

            //let channel_message = TcpBridgeMessage{ message: deserialized };
            //log::trace!("Sending event message: {}", pkg_len);

            match self.chan.send(deserialized).await {
                Ok(_) => {}
                Err(_e) => {
                    /* There are no receiver anymore, close the socket receiver */
                    log::debug!("Client channel send error");
                    return Ok(ReadResult::StopReading);
                }
            }
            /* If not all data was read, move the extra data to the start of the buffer */
            //let pkt_outer_len = pkg_len + PACKET_HEADER_LENGTH;
            let pkt_end = *pkg_offset + *pkg_len;

            //buffer_filled -= pkt_outer_len;
            //pkg_offset += pkt_outer_len;
            if *buffer_filled == pkt_end {
                *pkg_len = u32::MAX as usize;
                /* All other temp variable related to size are undefined at this point */
            } else {
                /* There are still some data in the buffer, prepare for more data to come */
                assert!(*buffer_filled > *pkg_len);
                assert!(*buffer_filled - *pkg_len >= PACKET_HEADER_LENGTH);
                buffer.copy_within(pkt_end..*buffer_filled, 0);
                *buffer_filled -= pkt_end;
                *pkg_offset = PACKET_HEADER_LENGTH;
                *pkg_len = u32::from_le_bytes(buffer[0..(PACKET_HEADER_LENGTH)].try_into().unwrap())
                    as usize;
            }
        }
        return Ok(ReadResult::GotPacket);
    }

    pub async fn process(&mut self) -> Result<()> {
        let mut backing_buffer = AlignedBuffer {
            contents: [0u8; 0x10000],
        };
        let buffer = &mut backing_buffer.contents[PACKET_HEADER_ADJUST..];
        let mut pkg_offset: usize = 0;
        let mut pkg_len: usize = u32::MAX as usize;
        let mut buffer_filled: usize = 0;
        log::trace!("TCPBR proc");
        'outer_loop: loop {
            tokio::select! {
            _readable = self.tcp_conn.readable() => {
                'inner_read: loop{
                        match self.try_read(
                            buffer,
                            &mut pkg_offset,
                            &mut pkg_len,
                            &mut buffer_filled,
                        ).await?{
                            ReadResult::GotPacket => {},
                            ReadResult::NoData => {break 'inner_read;},
                            ReadResult::StopReading => {break 'outer_loop;},
                        }
                    }
                },
                  _ = self.chan.closed() =>{
                        log::trace!("Client channel closed");
                        break;
                    }
                }
        }
        /* Tell the sender that the connection has closed */
        if let Some(close_chan) = self.close_chan.take() {
            close_chan
                .send(())
                .map_err(|_| anyhow!("Error while notifying sender about disconnect"))?;
        }
        log::trace!("TCPBR proc end");
        Ok(())
    }
}
