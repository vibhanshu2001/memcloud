pub mod auth;
pub mod transcript;
pub mod secure_stream;

use serde::{Serialize, Deserialize};
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};
use anyhow::Result;
use log::{info, error};
use std::net::SocketAddr;
use crate::metadata::{BlockId, NodeId};
use tokio::sync::Mutex;

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    Hello {
        version: u16,
        node_id: NodeId,
        name: String,
        total_memory: u64,
        used_memory: u64,
    },
    PutBlock {
        id: BlockId,
        data: Vec<u8>,
    },
    GetBlock {
        id: BlockId,
    },
    BlockData {
        id: BlockId,
        data: Option<Vec<u8>>,
    },
    GetKey {
        key: String,
    },
    KeyFound {
        key: String,
        data: Option<Vec<u8>>,
    },
    PutKey {
        key: String,
        data: Vec<u8>,
    },
    KeyStored {
        key: String,
        id: BlockId,
    },
    UpdateQuota {
        quota: u64,
    },
    Ack,
    Flush,
    Bye,
}

use std::sync::Arc;
use crate::peers::PeerManager;
use crate::blocks::{InMemoryBlockManager, BlockManager}; 
use crate::net::secure_stream::{SecureReader, SecureWriter};

pub struct TransportServer {
    listener: TcpListener,
    block_manager: Arc<InMemoryBlockManager>,
    peer_manager: Arc<PeerManager>,
}

impl TransportServer {
    pub async fn bind(start_port: u16, block_manager: Arc<InMemoryBlockManager>, peer_manager: Arc<PeerManager>) -> Result<(Self, u16)> {
        let mut port = start_port;
        // Try up to 10 ports
        for _ in 0..10 {
            let addr = format!("0.0.0.0:{}", port);
            match TcpListener::bind(&addr).await {
                Ok(listener) => {
                    info!("Transport listening on {}", addr);
                    return Ok((Self { listener, block_manager, peer_manager }, port));
                }
                Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                    info!("Port {} in use, trying next available port...", port);
                    port += 1;
                }
                Err(e) => return Err(anyhow::Error::new(e)),
            }
        }
        anyhow::bail!("Could not bind to any port starting from {} (tried 10 ports)", start_port);
    }

    pub async fn run(&self) {
        loop {
            match self.listener.accept().await {
                Ok((mut stream, addr)) => {
                    info!("Incoming connection from {}", addr);
                    let bm = self.block_manager.clone();
                    let pm = self.peer_manager.clone();
                    
                     // Spawn per-connection handler
                     tokio::spawn(async move {
                         let identity = pm.get_identity();
                         info!("Starting handshake with {}", addr);
                         
                         let sys_mem = pm.get_total_system_memory();
                         let my_quota = bm.get_max_memory();
                         
                         match auth::handshake_responder(&mut stream, &identity, my_quota, sys_mem).await {
                             Ok(session) => {
                                 info!("Handshake accepted from {} ({}). Negotiated secure session.", session.peer_name, session.peer_id);
                                 
                                 let (reader, writer) = stream.into_split();
                                 let secure_reader = SecureReader::new(reader, &session.recv_key);
                                 let secure_writer = SecureWriter::from_raw(writer, &session.send_key);
                                 
                                 let writer_arc = Arc::new(tokio::sync::Mutex::new(secure_writer));
                                 
                                 pm.register_authenticated_peer(session.peer_id, addr, session.peer_name, writer_arc.clone(), my_quota, session.peer_total_memory, session.peer_quota);
                                 
                                 if let Err(e) = handle_connection_split(secure_reader, writer_arc, addr, session.peer_id, bm, pm).await {
                                     error!("Connection error from {}: {}", addr, e);
                                 }
                             }
                             Err(e) => {
                                 error!("Handshake failed handling {}: {}", addr, e);
                             }
                         }
                    });
                }
                Err(e) => {
                    error!("Accept error: {}", e);
                }
            }
        }
    }
}

use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};

pub async fn send_message_locked(writer: &mut tokio::sync::MutexGuard<'_, SecureWriter>, msg: &Message) -> Result<()> {
    let bytes = bincode::serialize(msg)?;
    writer.send_frame(&bytes).await?;
    Ok(())
}

pub async fn handle_connection_split(
    mut reader: SecureReader, 
    writer: Arc<Mutex<SecureWriter>>, 
    addr: SocketAddr, 
    peer_id: crate::metadata::NodeId, // Added peer_id
    block_manager: Arc<InMemoryBlockManager>, 
    peer_manager: Arc<PeerManager>
) -> Result<()> {
    
    loop {
        match reader.recv_frame().await {
            Ok(frame_data) => {
                // Deserialize
                let msg: Message = bincode::deserialize(&frame_data)?;

                match msg {
                    Message::Hello { .. } => {
                        // Ignored securely; legacy
                    }
                    Message::GetBlock { id } => {
                        use crate::blocks::BlockManager;
                        match block_manager.get_block(id) {
                            Ok(Some(block)) => {
                                let resp = Message::BlockData { id, data: Some(block.data) };
                                let mut w = writer.lock().await;
                                send_message_locked(&mut w, &resp).await?;
                            }
                            Ok(None) => {
                                let resp = Message::BlockData { id, data: None };
                                let mut w = writer.lock().await;
                                send_message_locked(&mut w, &resp).await?;
                            }
                            Err(e) => {
                                 error!("Error retrieving block {}: {}", id, e);
                                 let resp = Message::BlockData { id, data: None };
                                 let mut w = writer.lock().await;
                                 send_message_locked(&mut w, &resp).await?;
                            }
                        }
                    }
                    Message::BlockData { id, data } => {
                        if let Some(d) = data {
                            peer_manager.satisfy_request(id, d);
                        }
                    }
                    Message::PutBlock { id, data } => {
                         use crate::blocks::{BlockManager, Block};
                         let size = data.len() as u64;
                         
                         // Check Quota
                         if peer_manager.try_reserve_storage(peer_id, size) {
                             info!("Storing remote block {} from authenticated peer {}", id, peer_id);
                             let block = Block { id, data };
                             if let Err(e) = block_manager.put_block(block) {
                                 error!("Failed to store remote block: {}", e);
                                 // Release quota on failure?
                                 peer_manager.release_storage(peer_id, size);
                             }
                         } else {
                             error!("Rejected PutBlock from {}: Quota Exceeded", peer_id);
                             // TODO: Send NACK?
                         }
                    }
                    Message::GetKey { key } => {
                        let id_opt = block_manager.get_named_block_id(&key);
                        let mut data_opt = None;
                        if let Some(id) = id_opt {
                            if let Ok(Some(block)) = block_manager.get_block(id) {
                                 data_opt = Some(block.data);
                            }
                        }
                        let resp = Message::KeyFound { key, data: data_opt };
                        let mut w = writer.lock().await;
                        send_message_locked(&mut w, &resp).await?;
                    }
                    Message::KeyFound { key, data } => {
                        if let Some(d) = data {
                            peer_manager.satisfy_key_request(&key, d);
                        }
                    }
                    Message::Flush => {
                        info!("Received Flush command from authenticated peer. Clearing local memory.");
                        block_manager.flush();
                    }
                    Message::PutKey { key, data } => {
                        let size = data.len() as u64;
                        if peer_manager.try_reserve_storage(peer_id, size) {
                             // Use block_manager.set (needs to be exposed or match existing API)
                             // Assuming block_manager has `set` (it handles SdkCommand::Set)
                             match block_manager.set(&key, data) { 
                                  Ok(id) => {
                                      let resp = Message::KeyStored { key, id };
                                      let mut w = writer.lock().await;
                                      if let Err(e) = send_message_locked(&mut w, &resp).await {
                                           error!("Failed to send KeyStored ack: {}", e);
                                      }
                                  }
                                  Err(e) => {
                                      peer_manager.release_storage(peer_id, size);
                                      error!("Failed to set key from peer {}: {}", peer_id, e);
                                  }
                             }
                        } else {
                             error!("Quota exceeded for PutKey from {}", peer_id);
                        }
                    }
                    Message::KeyStored { key, id } => {
                        peer_manager.satisfy_key_store(&key, id);
                    }
                    Message::UpdateQuota { quota } => {
                        info!("Received quota update from {}: {} bytes", peer_id, quota);
                        peer_manager.update_peer_ram_quota(peer_id, quota);
                    }
                    Message::Bye => {
                        info!("Peer {} disconnected gracefully.", peer_id);
                        break;
                    }
                    _ => {}
                }
            }
            Err(e) => {
                // Connection closed or error
                 error!("Read error from {}: {} (Disconnecting)", addr, e);
                 break;
            }
        }
    }
    
    // Cleanup on disconnect (graceful or error)
    peer_manager.handle_peer_disconnect(peer_id);
    Ok(())
}

#[allow(dead_code)]
pub async fn send_message(stream: &mut TcpStream, msg: &Message) -> Result<()> {
    let bytes = bincode::serialize(msg)?;
    let len = bytes.len() as u32;
    stream.write_all(&len.to_be_bytes()).await?;
    stream.write_all(&bytes).await?;
    Ok(())
}
