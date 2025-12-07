use serde::{Serialize, Deserialize};
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
    Connect { addr: String },
    // New KV commands
    Set { key: String, #[serde(with = "serde_bytes")] data: Vec<u8> },
    Get { key: String },
    /// List keys with pattern (simple glob: *, prefix*, *suffix, *contains*)
    ListKeys { pattern: String },
    Stat,
    // Streaming Commands
    StreamStart { size_hint: Option<u64> },
    StreamChunk { stream_id: u64, chunk_seq: u32, #[serde(with = "serde_bytes")] data: Vec<u8> },
    StreamFinish { stream_id: u64 },
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "res")]
pub enum SdkResponse {
    Stored { #[serde(with = "string_id")] id: BlockId },
    Loaded { #[serde(with = "serde_bytes")] data: Vec<u8> },
    Success,
    List { items: Vec<String> }, // Reuse for Keys? Or explicit?
    // Let's reuse List for keys to be simple, or add Keys?
    // List is Vec<String>, Keys is Vec<String>. Reuse is fine.
    // Clarification: ListPeers uses List. ListKeys can use List.
    Error { msg: String },
    // Stat response
    Status {
        blocks: usize,
        peers: usize,
        memory_usage: usize, // simplified
    },
    StreamStarted { stream_id: u64 },
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
                                // Re-use generic handler or specific?
                                // Let's use generic stream wrapper or just dup code for now to avoid refactor complexity
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
                // Accessing peer_manager from block_manager requires exposing it or holding a ref in RpcServer
                // Since BlockManager trait doesn't expose it, and we hold Arc<InMemoryBlockManager> concrete type, we need to access the field.
                // However, the field `peer_manager` is private in BlockManager. 
                // We should expose a helper on InMemoryBlockManager or update the architecture.
                // For speed, let's assume we add a `get_peer_list` method to InMemoryBlockManager.
                let peers = block_manager.get_peer_list();
                SdkResponse::List { items: peers }
            }
            SdkCommand::Connect { addr } => {
                match block_manager.connect_peer(&addr, block_manager.clone()).await {
                    Ok(_) => SdkResponse::Success,
                    Err(e) => SdkResponse::Error { msg: e.to_string() },
                }
            }
            SdkCommand::Set { key, data } => {
                 let id = rand::random::<u64>();
                 let block = Block { id, data };
                 match block_manager.put_named_block(key, block) {
                     Ok(_) => SdkResponse::Stored { id }, // Or Success? Returning ID is useful.
                     Err(e) => SdkResponse::Error { msg: e.to_string() },
                 }
            }
            SdkCommand::Get { key } => {
                match block_manager.get_distributed_key(&key).await {
                    Ok(Some(data)) => SdkResponse::Loaded { data },
                    Ok(None) => SdkResponse::Error { msg: "Key not found locally or in cluster".to_string() },
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
            SdkCommand::StreamFinish { stream_id } => {
                match block_manager.finish_stream(stream_id) {
                    Ok(id) => SdkResponse::Stored { id },
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

async fn handle_client_unix(stream: UnixStream, bm: Arc<InMemoryBlockManager>) -> Result<()> {
    handle_generic_stream(stream, bm).await
}

async fn handle_client_tcp(stream: tokio::net::TcpStream, bm: Arc<InMemoryBlockManager>) -> Result<()> {
    handle_generic_stream(stream, bm).await
}
