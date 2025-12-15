use serde::{Serialize, Deserialize};
#[cfg(unix)]
use tokio::net::{UnixListener, UnixStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use anyhow::Result;
use log::{info, error};
use std::sync::Arc;
use crate::blocks::{BlockManager, Block, InMemoryBlockManager}; // Need concrete type for async method or cast
use crate::metadata::BlockId;

// Helper for string serialization of BlockId to safe-guard against JS/JSON precision loss
mod string_id {
    use serde::{Deserialize, Deserializer, Serializer};
    use crate::metadata::BlockId;

    pub fn serialize<S>(id: &BlockId, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&id.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<BlockId, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "cmd")]
pub enum SdkCommand {
    Store { #[serde(with = "serde_bytes")] data: Vec<u8> },
    StoreRemote { #[serde(with = "serde_bytes")] data: Vec<u8>, target: Option<String> }, 
    Load { #[serde(with = "string_id")] id: BlockId },
    Free { #[serde(with = "string_id")] id: BlockId },
    ListPeers,
    Connect { addr: String, quota: Option<u64> },
    UpdatePeerQuota { peer_id: String, quota: u64 },
    Disconnect { peer_id: String },
    // New KV commands
    Set { key: String, #[serde(with = "serde_bytes")] data: Vec<u8>, target: Option<String> },
    Get { key: String, target: Option<String> },
    ListKeys { pattern: String },
    Stat,
    // Polling
    PollConnection { addr: String },
    // Streaming Commands
    StreamStart { size_hint: Option<u64> },
    StreamChunk { stream_id: u64, chunk_seq: u32, #[serde(with = "serde_bytes")] data: Vec<u8> },
    StreamFinish { stream_id: u64, target: Option<String> },
    Flush { target: Option<String> },
    // Trust & Consent
    TrustList,
    TrustRemove { key_or_name: String },
    ConsentList,
    ConsentApprove { session_id: String, trust_always: bool },
    ConsentDeny { session_id: String },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TrustedDevice {
    pub public_key: String,
    pub name: String,
    pub first_seen: u64,
    pub last_approved: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PendingConsent {
    pub session_id: String,
    pub peer_pubkey: String,
    pub peer_name: String,
    pub created_at: u64,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "res")]
pub enum SdkResponse {
    Stored { #[serde(with = "string_id")] id: BlockId },
    Loaded { #[serde(with = "serde_bytes")] data: Vec<u8> },
    Success,
    List { items: Vec<String> }, // Keep for keys
    PeerList { peers: Vec<crate::peers::PeerMetadata> },
    PeerConnected { metadata: crate::peers::PeerMetadata },
    Error { msg: String },
    // Stat response
    Status {
        blocks: usize,
        peers: usize,
        memory_usage: usize, // simplified
    },
    StreamStarted { stream_id: u64 },
    FlushSuccess,
    TrustedList { items: Vec<TrustedDevice> },
    ConsentList { items: Vec<PendingConsent> },
    ConnectionStatus { state: String, msg: Option<String> },
}

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
            SdkCommand::Store { data } => {
                let id = rand::random::<u64>();
                let block = Block { id, data };
                match block_manager.put_block(block) {
                    Ok(_) => SdkResponse::Stored { id },
                    Err(e) => SdkResponse::Error { msg: e.to_string() },
                }
            }
            SdkCommand::StoreRemote { data, target } => {
                let id = rand::random::<u64>();
                let block = Block { id, data };
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
                match block_manager.evict_block(id) {
                    Ok(_) => SdkResponse::Success,
                    Err(e) => SdkResponse::Error { msg: e.to_string() },
                }
            }
            SdkCommand::ListPeers => {
                let peers = block_manager.get_peer_ext_list();
                SdkResponse::PeerList { peers }
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
            SdkCommand::Set { key, data, target } => {
                 let id = rand::random::<u64>();
                 
                 if let Some(t) = target {
                     match block_manager.set_remote(&key, data, &t).await {
                         Ok(remote_id) => SdkResponse::Stored { id: remote_id },
                         Err(e) => SdkResponse::Error { msg: e.to_string() },
                     }
                 } else {
                     let block = Block { id, data };
                     match block_manager.put_named_block(key, block) {
                         Ok(_) => SdkResponse::Stored { id },
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
                 // Get real stats
                 let blocks_count = block_manager.blocks.len();
                 let peers_count = block_manager.get_peer_list().len();
                 let memory = block_manager.used_space() as usize;

                 SdkResponse::Status { 
                     blocks: blocks_count, 
                     peers: peers_count, 
                     memory_usage: memory,
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
            SdkCommand::StreamFinish { stream_id, target } => {
                match block_manager.finalize_stream(stream_id) {
                    Ok(data) => {
                         let id = rand::random::<u64>();
                         let block = Block { id, data };
                         
                         if let Some(t) = target {
                             // Remote
                             match block_manager.put_block_remote(block, Some(t)).await {
                                 Ok(_) => SdkResponse::Stored { id },
                                 Err(e) => SdkResponse::Error { msg: e.to_string() },
                             }
                         } else {
                             // Local
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
