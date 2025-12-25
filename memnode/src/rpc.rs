use serde::{Serialize, Deserialize};
#[cfg(unix)]
use tokio::net::{UnixListener, UnixStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use anyhow::Result;
use log::{info, error};
use std::sync::Arc;
use crate::blocks::{BlockManager, Block, InMemoryBlockManager}; // Need concrete type for async method or cast
use crate::metadata::BlockId;

// Removed local string_id, SdkCommand, SdkResponse, etc. Using memsdk versions.
use memsdk::{SdkCommand, SdkResponse, TrustedDevice, PendingConsent};

pub struct RpcServer {
    socket_path: String,
    // We retain Arc<InMemoryBlockManager> to access specific async methods if trait doesn't have them
    // Or we update trait. For now, let's keep it simple and cast or hold concrete type.
    block_manager: Arc<InMemoryBlockManager>,
}

impl RpcServer {
    pub fn new(socket_path: &str, block_manager: Arc<InMemoryBlockManager>) -> Self {
        let _ = std::fs::remove_file(socket_path);
        
        Self {
            socket_path: socket_path.to_string(),
            block_manager,
        }
    }

    #[cfg(unix)]
    pub async fn run(&self) -> Result<()> {
        let unix_listener = UnixListener::bind(&self.socket_path)?;
        let tcp_listener = tokio::net::TcpListener::bind("127.0.0.1:7070").await?;
        
        info!("RPC Server listenting on {} and 127.0.0.1:7070 (JSON)", self.socket_path);

        loop {
            tokio::select! {
                res = unix_listener.accept() => {
                   match res {
                       Ok((stream, _)) => {
                           let bm = self.block_manager.clone();
                           tokio::spawn(async move {
                               if let Err(e) = handle_client_unix(stream, bm).await {
                                   error!("RPC Client error (Unix): {}", e);
                               }
                           });
                       }
                       Err(e) => error!("Unix Accept Error: {}", e),
                   }
                }
                res = tcp_listener.accept() => {
                    match res {
                        Ok((stream, _)) => {
                            let bm = self.block_manager.clone();
                            tokio::spawn(async move {
                                if let Err(e) = handle_client_tcp(stream, bm).await {
                                     error!("RPC Client error (TCP): {}", e);
                                }
                            });
                        }
                         Err(e) => error!("TCP Accept Error: {}", e),
                    }
                }
            }
        }
    }

    #[cfg(windows)]
    pub async fn run(&self) -> Result<()> {
        let tcp_listener = tokio::net::TcpListener::bind("127.0.0.1:7070").await?;
        info!("RPC Server listening on 127.0.0.1:7070 (JSON)");

        loop {
            match tcp_listener.accept().await {
                Ok((stream, _)) => {
                    let bm = self.block_manager.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_client_tcp(stream, bm).await {
                                error!("RPC Client error (TCP): {}", e);
                        }
                    });
                }
                Err(e) => error!("TCP Accept Error: {}", e),
            }
        }
    }
}

// Generic handler using AsyncRead/Write
async fn handle_generic_stream<S>(mut stream: S, block_manager: Arc<InMemoryBlockManager>) -> Result<()> 
where S: AsyncReadExt + AsyncWriteExt + Unpin 
{
    loop {
        let mut len_buf = [0u8; 4];
        if stream.read_exact(&mut len_buf).await.is_err() {
            break; 
        }
        let len = u32::from_be_bytes(len_buf) as usize;

        let mut buf = vec![0u8; len];
        stream.read_exact(&mut buf).await?;

        // SWITCH TO MessagePack
        let cmd: SdkCommand = rmp_serde::from_slice(&buf)?;
        
        let response = match cmd {
            SdkCommand::Store { data, durability } => {
                     let mode = durability.unwrap_or(memsdk::Durability::Pinned);
                     let id = rand::random::<u64>();
                     
                     let block = crate::blocks::Block {
                         id,
                         data,
                         durability: mode,
                         last_accessed: std::sync::atomic::AtomicU64::new(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()).into(),
                     };
                     
                     match block_manager.put_block(block) {
                         Ok(_) => SdkResponse::Stored { id },
                         Err(e) => SdkResponse::Error { msg: e.to_string() },
                     }
                }
            SdkCommand::StoreRemote { data, target, durability } => {
                     let mode = durability.unwrap_or(memsdk::Durability::Pinned);
                     let id = rand::random::<u64>();
                     let block = crate::blocks::Block {
                         id,
                         data,
                         durability: mode,
                         last_accessed: std::sync::atomic::AtomicU64::new(0).into(),
                     };

                     match block_manager.put_block_remote(block, target).await {
                         Ok(_) => SdkResponse::Stored { id },
                         Err(e) => SdkResponse::Error { msg: e.to_string() },
                     }
                }       
            SdkCommand::Load { id } => {
                match block_manager.get_block_async(id).await {
                    Ok(Some(block)) => SdkResponse::Loaded { data: block.data },
                    Ok(None) => SdkResponse::Error { msg: "Block not found".to_string() },
                    Err(e) => SdkResponse::Error { msg: e.to_string() },
                }
            }
            SdkCommand::Free { id } => {
                if block_manager.vm_free(id).is_ok() {
                    SdkResponse::Success
                } else {
                    match block_manager.evict_block(id) {
                        Ok(_) => SdkResponse::Success,
                        Err(e) => SdkResponse::Error { msg: e.to_string() },
                    }
                }
            }
            SdkCommand::ListPeers => {
                let peers = block_manager.get_peer_ext_list();
                let sdk_peers = peers.into_iter().map(|p| memsdk::PeerMetadata {
                    id: p.id,
                    name: p.name,
                    addr: p.addr,
                    total_memory: p.total_memory,
                    used_memory: p.used_memory,
                    quota: p.quota,
                    allowed_quota: p.allowed_quota,
                }).collect();
                SdkResponse::PeerList { peers: sdk_peers }
            }
            SdkCommand::Connect { addr, quota } => {
                let bm_clone = block_manager.clone();
                let addr_clone = addr.clone();
                let quota_clone = quota;
                
                tokio::spawn(async move {
                    let _ = bm_clone.connect_peer(&addr_clone, bm_clone.clone(), quota_clone.unwrap_or(0)).await;
                });
                
                SdkResponse::ConnectionStatus { state: "pending".to_string(), msg: None }
            }
            SdkCommand::PollConnection { addr } => {
                 use std::net::SocketAddr;
                 use crate::peers::HandshakeState;
                 
                 if let Ok(socket_addr) = addr.parse::<SocketAddr>() {
                     if let Some(state) = block_manager.peer_manager.outgoing_handshakes.get(&socket_addr) {
                         let (status, msg) = match state.value() {
                             HandshakeState::Connecting => ("pending", None),
                             HandshakeState::WaitingForConsent => ("waiting_consent", None),
                             HandshakeState::Authenticated => ("connected", None),
                             HandshakeState::Failed(e) => ("failed", Some(e.clone())),
                         };
                         SdkResponse::ConnectionStatus { state: status.to_string(), msg: msg.map(|s| s) }
                     } else {
                         // Not found - could be not started or potential race if processed very fast?
                         // Assume idle/none
                         SdkResponse::ConnectionStatus { state: "unknown".to_string(), msg: Some("No active handshake found".to_string()) }
                     }
                 } else {
                     SdkResponse::Error { msg: "Invalid address format".to_string() }
                 }
            }
            SdkCommand::UpdatePeerQuota { peer_id, quota } => {
                 if quota > block_manager.get_max_memory() {
                     SdkResponse::Error { msg: format!("Quota exceeds node memory limit ({})", block_manager.get_max_memory()) }
                 } else {
                     match block_manager.update_peer_quota(&peer_id, quota).await {
                         Ok(_) => SdkResponse::Success,
                         Err(e) => SdkResponse::Error { msg: e.to_string() },
                     }
                 }
            }
            SdkCommand::Disconnect { peer_id } => {
                match block_manager.disconnect_peer(&peer_id).await {
                     Ok(true) => SdkResponse::Success,
                     Ok(false) => SdkResponse::Error { msg: "Peer not found".to_string() },
                     Err(e) => SdkResponse::Error { msg: e.to_string() },
                }
            }
            SdkCommand::Set { key, data, target, durability } => {
                    let mode = durability.unwrap_or(memsdk::Durability::Pinned);
                     if let Some(t) = target {
                         match block_manager.set_remote(&key, data, &t, mode).await {
                             Ok(id) => SdkResponse::Stored { id },
                             Err(e) => SdkResponse::Error { msg: e.to_string() },
                         }
                     } else {
                         // Local set
                         match block_manager.set(&key, data, mode) {
                             Ok(id) => SdkResponse::Stored { id },
                             Err(e) => SdkResponse::Error { msg: e.to_string() },
                         }
                     }
                }          
            SdkCommand::Get { key, target } => {
                let res = if let Some(t) = target {
                    block_manager.get_remote(&key, &t).await
                } else {
                    block_manager.get_distributed_key(&key).await
                };

                match res {
                    Ok(Some(data)) => SdkResponse::Loaded { data },
                    Ok(None) => SdkResponse::Error { msg: "Key not found".to_string() },
                    Err(e) => SdkResponse::Error { msg: e.to_string() },
                }
            }
            SdkCommand::ListKeys { pattern } => {
                let keys = block_manager.list_keys(&pattern);
                SdkResponse::List { items: keys }
            }
             SdkCommand::Stat => {
                  let blocks_count = block_manager.blocks.len();
                  let peers_count = block_manager.get_peer_list().len();
                  let memory = block_manager.used_space() as usize;
                  
                  let (vm_regions, vm_pages) = block_manager.vm_manager.get_stats();
 
                  SdkResponse::Status { 
                      blocks: blocks_count, 
                      peers: peers_count, 
                      memory_usage: memory,
                      vm_regions,
                      vm_pages_mapped: vm_pages,
                      vm_memory_in_use: vm_pages * 4096,
                  }
             }
            // Streaming Handlers
            SdkCommand::StreamStart { size_hint } => {
                let stream_id = block_manager.start_stream(size_hint);
                SdkResponse::StreamStarted { stream_id }
            }
            SdkCommand::StreamChunk { stream_id, chunk_seq: _, data } => {
                // chunk_seq can be used for ordering if using UDP, but over TCP/Unix it's sequential.
                // We ignore it for now or could assert it matches expected index.
                match block_manager.append_stream(stream_id, data) {
                    Ok(_) => SdkResponse::Success,
                    Err(e) => SdkResponse::Error { msg: e.to_string() },
                }
            }
            SdkCommand::StreamFinish { stream_id, target, durability } => {
                     let mode = durability.unwrap_or(memsdk::Durability::Pinned);
                     match block_manager.finalize_stream(stream_id) {
                         Ok(data) => {
                             if let Some(t) = target {
                                 let id = rand::random::<u64>();
                                 let block = crate::blocks::Block { id, data, durability: mode, last_accessed: std::sync::atomic::AtomicU64::new(0).into() };
                                 match block_manager.put_block_remote(block, Some(t)).await {
                                     Ok(_) => SdkResponse::Stored { id },
                                     Err(e) => SdkResponse::Error { msg: e.to_string() },
                                 }
                             } else {
                                 let id = rand::random::<u64>();
                                 let block = crate::blocks::Block { 
                                     id, 
                                     data, 
                                     durability: mode,
                                     last_accessed: std::sync::atomic::AtomicU64::new(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()).into()
                                 };
                                 match block_manager.put_block(block) {
                                     Ok(_) => SdkResponse::Stored { id },
                                     Err(e) => SdkResponse::Error { msg: e.to_string() },
                                 }
                             }
                         }
                         Err(e) => SdkResponse::Error { msg: e.to_string() },
                     }
                }       
            SdkCommand::Flush { target } => {
                if let Some(t) = target {
                    match block_manager.flush_remote(t).await {
                         Ok(_) => SdkResponse::FlushSuccess,
                         Err(e) => SdkResponse::Error { msg: e.to_string() },
                    }
                } else {
                    block_manager.flush();
                    SdkResponse::FlushSuccess
                }
            }
            // Trust & Consent
            SdkCommand::TrustList => {
                let items = block_manager.peer_manager.trusted_store.list_trusted();
                // Map local type to RPC type (duplicated def)
                let rpc_items = items.into_iter().map(|d| TrustedDevice {
                    public_key: d.public_key,
                    name: d.name,
                    first_seen: d.first_seen,
                    last_approved: d.last_approved,
                }).collect();
                SdkResponse::TrustedList { items: rpc_items }
            }
            SdkCommand::TrustRemove { key_or_name } => {
                 match block_manager.peer_manager.trusted_store.remove_trusted(&key_or_name) {
                     Ok(removed) => {
                         if removed.is_empty() {
                             SdkResponse::Error { msg: "No matching trusted device found".to_string() }
                         } else {
                             for device in removed {
                                 // Disconnect if connected
                                 if let Some(peer_id) = block_manager.peer_manager.get_peer_id_by_name(&device.name) {
                                     info!("Disconnecting removed peer {} ({})", device.name, peer_id);
                                     block_manager.peer_manager.disconnect_peer(peer_id).await;
                                 }
                             }
                             SdkResponse::Success
                         }
                     }
                     Err(e) => SdkResponse::Error { msg: e.to_string() },
                 }
            }
            SdkCommand::ConsentList => {
                let items = block_manager.peer_manager.consent_manager.get_pending_list();
                let rpc_items = items.into_iter().map(|c| PendingConsent {
                    session_id: c.session_id,
                    peer_pubkey: c.peer_pubkey,
                    peer_name: c.peer_name,
                    quota: c.quota,
                    created_at: c.created_at,
                }).collect();
                SdkResponse::ConsentList { items: rpc_items }
            }
            SdkCommand::ConsentApprove { session_id, trust_always } => {
                 use crate::peers::consent::ConsentDecision;
                 let decision = if trust_always {
                     ConsentDecision::ApprovedAndTrusted
                 } else {
                     ConsentDecision::ApprovedOnce
                 };
                 
                 match block_manager.peer_manager.consent_manager.resolve(&session_id, decision) {
                     Ok(_) => SdkResponse::Success,
                     Err(e) => SdkResponse::Error { msg: e.to_string() },
                 }
            }
            SdkCommand::ConsentDeny { session_id } => {
                 use crate::peers::consent::ConsentDecision;
                 match block_manager.peer_manager.consent_manager.resolve(&session_id, ConsentDecision::Denied) {
                     Ok(_) => SdkResponse::Success,
                     Err(e) => SdkResponse::Error { msg: e.to_string() },
                 }
            }
            SdkCommand::VmAlloc { size } => {
                let region_id = block_manager.vm_alloc(size);
                SdkResponse::VmCreated { region_id }
            }
            SdkCommand::VmFetch { region_id, page_index } => {
                match block_manager.vm_fetch(region_id, page_index).await {
                    Ok(data) => SdkResponse::PageData { data },
                    Err(e) => SdkResponse::Error { msg: e.to_string() },
                }
            }
            SdkCommand::VmStore { region_id, page_index, data } => {
                match block_manager.vm_store(region_id, page_index, data).await {
                    Ok(_) => SdkResponse::Success,
                    Err(e) => SdkResponse::Error { msg: e.to_string() },
                }
            }
        };

        // Serialize MessagePack
        let resp_bytes = rmp_serde::to_vec_named(&response)?;
        let resp_len = resp_bytes.len() as u32;
        stream.write_all(&resp_len.to_be_bytes()).await?;
        stream.write_all(&resp_bytes).await?;
    }
    Ok(())
}

#[cfg(unix)]
async fn handle_client_unix(stream: UnixStream, bm: Arc<InMemoryBlockManager>) -> Result<()> {
    handle_generic_stream(stream, bm).await
}

async fn handle_client_tcp(stream: tokio::net::TcpStream, bm: Arc<InMemoryBlockManager>) -> Result<()> {
    handle_generic_stream(stream, bm).await
}
