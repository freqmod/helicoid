/* Used to talk to a helix / helicoid backend over a relyable TCP connection
managed by Tokio, and connected to the user interface by channels */

use rkyv::{Archive, Deserialize, Serialize};
use std::net::SocketAddr;

use crate::gfx::HelicoidToClientMessage;
use anyhow::{anyhow, Result};
use bytecheck::CheckBytes;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::mpsc::{self, Receiver, Sender};
/*pub struct OwnedRkyvArchive<T: Archive, L: usize> {
    bytes: [u8; L],
    archive: T,
}*/
#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct TcpBridgeToClientMessage {
    message: HelicoidToClientMessage,
}
#[derive(Debug, Hash, Eq, Clone, PartialEq, Archive, Serialize, Deserialize, CheckBytes)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct TcpBridgeToServerMessage {
    message: HelicoidToClientMessage,
}
pub struct TcpBridgeSend<M> {
    tcp_conn: OwnedWriteHalf,
    chan: Receiver<M>,
}
pub struct TcpBridgeReceive<M> {
    tcp_conn: OwnedReadHalf,
    chan: Sender<M>,
}
pub struct ClientTcpBridge {
    send: TcpBridgeSend<TcpBridgeToServerMessage>,
    receive: TcpBridgeReceive<TcpBridgeToClientMessage>,
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
        let mut stream = TcpStream::connect(addr).await?;
        let (r, w) = stream.into_split();
        let (send, send_channel) = TcpBridgeSend::new(w).await?;
        let (receive, receive_channel) = TcpBridgeReceive::new(r).await?;
        Ok((Self { send, receive }, send_channel, receive_channel))
    }
}
impl<M> TcpBridgeSend<M> {
    async fn new(writer: OwnedWriteHalf) -> Result<(Self, Sender<M>)> {
        let (tx, rx) = mpsc::channel(32);
        Ok((
            Self {
                tcp_conn: writer,
                chan: rx,
            },
            tx,
        ))
    }
    pub async fn process(&mut self) -> Result<()> {
        loop {
            let received = self.chan.recv().await;
            match received {
                Some(message) => {}
                None => {
                    break;
                }
            }
        }
        Ok(())
    }
}
const PACKET_HEADER_LENGTH: usize = 2;
impl<M: Archive> TcpBridgeReceive<M>
where
    M::Archived: Deserialize<M, rkyv::Infallible>,
{
    async fn new(reader: OwnedReadHalf) -> Result<(Self, Receiver<M>)> {
        let (tx, rx) = mpsc::channel(32);
        Ok((
            Self {
                tcp_conn: reader,
                chan: tx,
            },
            rx,
        ))
    }
    pub async fn process(&mut self) -> Result<()> {
        let mut buffer = [0u8; 0x8000];
        let mut pkg_offset: usize = 0;
        let mut pkg_len: usize = u16::MAX as usize;
        let mut buffer_filled: usize = 0;
        loop {
            tokio::select! {
                            readable = self.tcp_conn.readable() =>{
                                let data_read;
                                if pkg_len == u16::MAX as usize{
                                    /* Read u16 length prefix */
                                    data_read = self.tcp_conn.try_read(&mut buffer)?;
                                    if data_read < 2{
                                        log::warn!("Unexpectedly small data received: {}", data_read);
                                        break;
                                    }
                                    pkg_len = u16::from_le_bytes(buffer[0..PACKET_HEADER_LENGTH].try_into().unwrap()) as usize;
                                    debug_assert!(pkg_len+PACKET_HEADER_LENGTH <= u16::MAX as usize);
                                    pkg_offset = PACKET_HEADER_LENGTH;
                                    buffer_filled = data_read;
                                }
                                else{
                                    data_read = self.tcp_conn.try_read(&mut buffer[buffer_filled..pkg_len-buffer_filled-PACKET_HEADER_LENGTH])?;

                                }
                                buffer_filled += data_read;
                                if buffer_filled >= PACKET_HEADER_LENGTH + pkg_len{
                                        /* All the required data was transferred */
                                        let data_sliced = &buffer[pkg_offset..(pkg_len+pkg_offset)];
                                        /*let msg = rkyv::from_bytes::<HelicoidToClientMessage>(data_sliced)
                                            .map_err(|_| anyhow!("Error while deserializing message from wire"))?;*/
                                        let archived = unsafe { rkyv::archived_root::<M>(data_sliced) };
                                        // TODO: Does the deserialized type copy or reference the archived memory (currently we assume copy)
                                        let deserialized = Deserialize::<M, _>::deserialize(archived, &mut rkyv::Infallible).unwrap();

            //                        let channel_message = TcpBridgeMessage{ message: deserialized };
                                    match self.chan.send(deserialized).await {
                                        Ok(_) =>{},
                                        Err(e) => {
                                            /* There are no receiver anymore, close the socket receiver */
                                            break;
                                        },
                                    }
                                    /* If not all data was read, move the extra data to the start of the buffer */
                                    //let pkt_outer_len = pkg_len + PACKET_HEADER_LENGTH;
                                    let pkt_end = pkg_offset + pkg_len;

                                    //buffer_filled -= pkt_outer_len;
                                    //pkg_offset += pkt_outer_len;
                                    if buffer_filled == pkt_end{
                                        pkg_len = u16::MAX as usize;
                                        /* All other temp variable related to size are undefined at this point */
                                    } else{
                                        /* There are still some data in the buffer, prepare for more data to come */
                                        assert!(buffer_filled > pkg_len);
                                        assert!(pkt_end - buffer_filled >=2);
                                        buffer.copy_within(pkt_end..buffer_filled, 0);
                                        buffer_filled -= pkt_end;
                                        pkg_offset = PACKET_HEADER_LENGTH;
                                        pkg_len = u16::from_le_bytes(buffer[0..PACKET_HEADER_LENGTH].try_into().unwrap()) as usize;
                                    }
                                }
                            },
                          _ = self.chan.closed() =>{
                                break;
                            }
                        }
        }
        Ok(())
    }
}
/*
pub async fn connect(
    addr: &SocketAddr,
    mut stdin: impl Stream<Item = Result<Bytes, io::Error>> + Unpin,
    mut stdout: impl Sink<Bytes, Error = io::Error> + Unpin,
) -> Result<(), Box<dyn Error>> {
    let mut stream = TcpStream::connect(addr).await?;
    let (r, w) = stream.split();
    let mut sink = FramedWrite::new(w, BytesCodec::new());
    // filter map Result<BytesMut, Error> stream into just a Bytes stream to match stdout Sink
    // on the event of an Error, log the error and end the stream
    let mut stream = FramedRead::new(r, BytesCodec::new())
        .filter_map(|i| match i {
            //BytesMut into Bytes
            Ok(i) => future::ready(Some(i.freeze())),
            Err(e) => {
                println!("failed to read from socket; error={}", e);
                future::ready(None)
            }
        })
        .map(Ok);

    match future::join(sink.send_all(&mut stdin), stdout.send_all(&mut stream)).await {
        (Err(e), _) | (_, Err(e)) => Err(e.into()),
        _ => Ok(()),
    }
}
*/
